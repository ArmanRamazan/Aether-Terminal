use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use clap::Parser;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_analyze::engine::{AnalyzeConfig, AnalyzeEngine};
use aether_core::events::SystemEvent;
use aether_core::models::{
    DiagCategory, DiagTarget, Diagnostic, Evidence, Recommendation, RecommendedAction, Severity,
    Urgency,
};
use aether_core::metrics::HostId;
use aether_core::{AgentAction, WorldGraph};
use aether_ingestion::ebpf_bridge::EbpfBridge;
use aether_ingestion::pipeline::IngestionPipeline;
use aether_ingestion::sysinfo_probe::SysinfoProbe;
use aether_core::ArbiterQueue;
use aether_mcp::McpServer;
use aether_predict::engine::{PredictConfig, PredictEngine};
use aether_predict::models::PredictedAnomaly;
use aether_render::tui::app::App;
use aether_render::tui::rules::RulesDisplayState;
use aether_render::PredictionDisplay;
use aether_metrics::consumer::PrometheusConsumer;
use aether_metrics::exporter::server::MetricsExporter;
use aether_script::engine::ScriptEngine;
use aether_script::hot_reload::HotReloader;
use aether_script::runtime::{CompiledRuleSet, RuleAction};

#[derive(Parser)]
#[command(
    name = "aether",
    version = env!("CARGO_PKG_VERSION"),
    about = "Cinematic 3D TUI system monitor"
)]
struct Cli {
    /// Logging level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Run in MCP stdio mode (no TUI, reads JSON-RPC from stdin)
    #[arg(long)]
    mcp_stdio: bool,

    /// Run MCP SSE server alongside TUI on the given port
    #[arg(long, value_name = "PORT", default_missing_value = "3000", num_args = 0..=1)]
    mcp_sse: Option<u16>,

    /// Disable 3D rendering, use 2D tables
    #[arg(long)]
    no_3d: bool,

    /// Disable gamification layer
    #[arg(long)]
    no_game: bool,

    /// Color theme name or path to TOML file
    #[arg(long, default_value = "cyberpunk")]
    theme: String,

    /// Load .aether rule files (JIT-compiled DSL)
    #[arg(long, value_name = "PATH")]
    rules: Option<std::path::PathBuf>,

    /// Enable predictive anomaly detection
    #[arg(long)]
    predict: bool,

    /// Path to directory containing ONNX model files
    #[arg(long, value_name = "PATH")]
    model_path: Option<std::path::PathBuf>,

    /// Enable eBPF telemetry (Linux only, requires CAP_BPF)
    #[arg(long)]
    ebpf: bool,

    /// Start web UI server on optional port (default: 8080)
    #[arg(long, num_args = 0..=1, default_missing_value = "8080")]
    web: Option<u16>,

    /// Disable diagnostic engine
    #[arg(long)]
    no_analyze: bool,

    /// Diagnostic analysis interval in seconds
    #[arg(long, value_name = "SECS")]
    analyze_interval: Option<u64>,

    /// Expose Prometheus /metrics endpoint on optional port (default: 9090)
    #[arg(long, num_args = 0..=1, default_missing_value = "9090")]
    metrics: Option<u16>,

    /// Poll a remote Prometheus server at this URL
    #[arg(long, value_name = "URL")]
    prometheus: Option<String>,

    /// Prometheus poll interval in seconds (default: 15)
    #[arg(long, value_name = "SECS")]
    prometheus_interval: Option<u64>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let env_filter = tracing_subscriber::EnvFilter::try_new(&cli.log_level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if cli.mcp_stdio {
        // Stdio MCP mode: logs go to stderr so stdout stays clean for JSON-RPC.
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }

    let world = Arc::new(RwLock::new(WorldGraph::new()));
    let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
    let (action_tx, mut action_rx) = mpsc::channel::<AgentAction>(64);

    let probe = Arc::new(SysinfoProbe::new());
    let (event_tx, event_rx) = mpsc::channel::<SystemEvent>(256);
    let cancel = CancellationToken::new();

    // Spawn ingestion pipeline (optionally with eBPF hybrid mode)
    let mut pipeline = IngestionPipeline::new(probe, event_tx.clone());

    if cli.ebpf {
        match try_init_ebpf(&event_tx, &cancel).await {
            Ok(bridge) => {
                pipeline = pipeline.with_ebpf(bridge);
                tracing::info!("eBPF telemetry enabled (hybrid mode)");
            }
            Err(e) => {
                tracing::warn!("eBPF init failed: {e}, continuing with sysinfo-only");
            }
        }
    }

    let pipeline_cancel = cancel.child_token();
    tokio::spawn(async move {
        if let Err(e) = pipeline.run(pipeline_cancel).await {
            tracing::error!("ingestion pipeline error: {e}");
        }
    });

    // Shared diagnostics state (populated by AnalyzeEngine + script bridge, read by TUI)
    let diagnostics: Arc<Mutex<Vec<Diagnostic>>> = Arc::new(Mutex::new(Vec::new()));

    // Rules engine setup (before graph updater so we can share the event stream)
    let rules_display = Arc::new(Mutex::new(RulesDisplayState::default()));
    let diag_bridge = if cli.rules.is_some() && !cli.no_analyze {
        Some(Arc::clone(&diagnostics))
    } else {
        None
    };
    let engine_event_tx = if let Some(ref rules_path) = cli.rules {
        Some(init_rules_engine(
            rules_path,
            &cancel,
            Arc::clone(&arbiter),
            Arc::clone(&rules_display),
            diag_bridge,
        )?)
    } else {
        None
    };

    // Spawn graph updater: SystemEvent → WorldGraph (+ forward to engine if active)
    let updater_world = Arc::clone(&world);
    let updater_cancel = cancel.child_token();
    tokio::spawn(async move {
        let mut event_rx = event_rx;
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Some(SystemEvent::MetricsUpdate { snapshot }) => {
                            // Forward to engine before consuming snapshot
                            if let Some(ref tx) = engine_event_tx {
                                let _ = tx.try_send(SystemEvent::MetricsUpdate {
                                    snapshot: snapshot.clone(),
                                });
                            }
                            if let Ok(mut graph) = updater_world.write() {
                                graph.apply_snapshot(&snapshot);
                            }
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
                _ = updater_cancel.cancelled() => break,
            }
        }
    });

    // Spawn arbiter action executor: drains approved actions and executes them
    let executor_arbiter = Arc::clone(&arbiter);
    let executor_cancel = cancel.child_token();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let actions = {
                        if let Ok(mut q) = executor_arbiter.lock() {
                            q.drain_approved()
                        } else {
                            continue;
                        }
                    };
                    for action in actions {
                        execute_action(&action);
                    }
                }
                _ = executor_cancel.cancelled() => break,
            }
        }
    });

    // Spawn background task forwarding channel actions to arbiter queue
    let forwarder_arbiter = Arc::clone(&arbiter);
    let forwarder_cancel = cancel.child_token();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                action = action_rx.recv() => {
                    match action {
                        Some(a) => {
                            if let Ok(mut q) = forwarder_arbiter.lock() {
                                q.submit("MCP Agent".into(), a);
                            }
                        }
                        None => break,
                    }
                }
                _ = forwarder_cancel.cancelled() => break,
            }
        }
    });

    // Shared predictions state (populated by PredictEngine, read by MCP + render)
    let predictions: Arc<Mutex<Vec<PredictedAnomaly>>> = Arc::new(Mutex::new(Vec::new()));

    // Spawn prediction engine if --predict flag is set
    if cli.predict {
        let mut config = PredictConfig::default();
        if let Some(ref path) = cli.model_path {
            config.model_path = path.clone();
        }

        let (pred_tx, mut pred_rx) = tokio::sync::mpsc::channel::<PredictedAnomaly>(64);
        let mut engine = PredictEngine::new(config, pred_tx);
        let predict_world = Arc::clone(&world);
        let predict_cancel = cancel.child_token();
        tokio::spawn(async move {
            engine.run(predict_world, predict_cancel).await;
        });

        // Spawn prediction collector: drains predictions into shared state
        let collector_predictions = Arc::clone(&predictions);
        let collector_cancel = cancel.child_token();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = collector_cancel.cancelled() => break,
                    prediction = pred_rx.recv() => {
                        match prediction {
                            Some(p) => {
                                if let Ok(mut preds) = collector_predictions.lock() {
                                    // Keep only latest predictions, cap at 50
                                    preds.push(p);
                                    if preds.len() > 50 {
                                        let excess = preds.len() - 50;
                                        preds.drain(..excess);
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        tracing::info!("prediction engine enabled");
    }

    // Spawn Prometheus consumer if --prometheus is set (before AnalyzeEngine so we can pass rx)
    let prometheus_rx = if let Some(ref prom_url) = cli.prometheus {
        let interval_secs = cli.prometheus_interval.unwrap_or(15);
        let consumer = PrometheusConsumer::new(
            prom_url,
            std::time::Duration::from_secs(interval_secs),
        )?;
        let (prom_tx, prom_rx) = mpsc::channel(64);
        let prom_cancel = cancel.child_token();
        tokio::spawn(async move {
            consumer.run(prom_tx, prom_cancel).await;
        });
        tracing::info!("Prometheus consumer polling {prom_url} every {interval_secs}s");
        Some(prom_rx)
    } else {
        None
    };

    // Spawn diagnostic engine unless --no-analyze
    if !cli.no_analyze {
        let interval_secs = cli.analyze_interval.unwrap_or(5);
        let config = AnalyzeConfig {
            interval: std::time::Duration::from_secs(interval_secs),
            host: HostId::new("local"),
            ..Default::default()
        };
        let mut engine = AnalyzeEngine::new(config);
        if let Some(rx) = prometheus_rx {
            engine = engine.with_prometheus_rx(rx);
        }
        let (diag_tx, mut diag_rx) = mpsc::channel::<Vec<Diagnostic>>(32);
        let analyze_cancel = cancel.child_token();
        let analyze_world = Arc::clone(&world);
        tokio::spawn(async move {
            engine.run(analyze_world, diag_tx, analyze_cancel).await;
        });

        let collector_diags = Arc::clone(&diagnostics);
        let collector_cancel = cancel.child_token();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = collector_cancel.cancelled() => break,
                    batch = diag_rx.recv() => {
                        match batch {
                            Some(new_diags) => {
                                if let Ok(mut diags) = collector_diags.lock() {
                                    diags.extend(new_diags);
                                    if diags.len() > 200 {
                                        let excess = diags.len() - 200;
                                        diags.drain(..excess);
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        tracing::info!("diagnostic engine enabled (interval: {interval_secs}s)");
    }

    // Spawn Prometheus metrics exporter if --metrics is set
    if let Some(port) = cli.metrics {
        let exporter = MetricsExporter::new();
        let exporter_world = Arc::clone(&world);
        let exporter_diags = Arc::clone(&diagnostics);

        // Periodic update task: feed WorldGraph + diagnostics into the exporter
        let update_cancel = cancel.child_token();
        let exporter_ref = exporter.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                tokio::select! {
                    _ = update_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(graph) = exporter_world.read() {
                            let diags = exporter_diags.lock()
                                .map(|d| d.clone())
                                .unwrap_or_default();
                            exporter_ref.update_from_world(&graph, &diags);
                        }
                    }
                }
            }
        });

        let serve_cancel = cancel.child_token();
        tokio::spawn(async move {
            if let Err(e) = exporter.serve(port, serve_cancel).await {
                tracing::error!("metrics server error: {e}");
            }
        });

        tracing::info!("Prometheus /metrics endpoint on port {port}");
    }

    // Mode handling
    if cli.mcp_stdio {
        // Stdio MCP mode: no TUI, no crossterm
        tracing::info!("running in MCP stdio mode");
        let mcp = McpServer::new(
            Arc::clone(&world),
            Arc::clone(&arbiter),
            action_tx,
            Arc::clone(&predictions),
            Arc::clone(&diagnostics),
        );
        let result = mcp.run_stdio(cancel.child_token()).await;
        cancel.cancel();
        return result.map_err(Into::into);
    }

    // SSE mode: spawn MCP server as background task alongside TUI
    if let Some(port) = cli.mcp_sse {
        tracing::info!("spawning MCP SSE server on port {port}");
        let mcp = McpServer::new(
            Arc::clone(&world),
            Arc::clone(&arbiter),
            action_tx,
            Arc::clone(&predictions),
            Arc::clone(&diagnostics),
        );
        let sse_cancel = cancel.child_token();
        tokio::spawn(async move {
            if let Err(e) = mcp.run_sse(port, sse_cancel).await {
                tracing::error!("MCP SSE server error: {e}");
            }
        });
    }

    // Web UI server
    if let Some(port) = cli.web {
        let web_state = aether_web::SharedState::new(
            Arc::clone(&world),
            Arc::clone(&arbiter),
            Arc::clone(&diagnostics),
        );

        // Spawn periodic system metrics updater (memory_total, load_avg)
        let metrics_state = web_state.clone();
        let metrics_cancel = cancel.child_token();
        tokio::spawn(async move {
            use sysinfo::System;
            let mut sys = System::new();
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                tokio::select! {
                    _ = metrics_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        sys.refresh_memory();
                        let memory_total = sys.total_memory();
                        let load_avg = System::load_average();
                        metrics_state.update_system_metrics(
                            memory_total,
                            [load_avg.one, load_avg.five, load_avg.fifteen],
                        );
                    }
                }
            }
        });

        let web_cancel = cancel.child_token();
        tokio::spawn(async move {
            aether_web::serve(web_state, port, web_cancel).await;
        });

        let url = format!("http://localhost:{port}");
        tracing::info!("Opening browser at {url}");
        if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            if let Err(e) = open::that(&url) {
                tracing::warn!("failed to open browser: {e}");
            }
        }
    }

    // Initialize terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Run TUI app
    let mut app = App::new(Arc::clone(&world));
    if cli.rules.is_some() {
        app.set_rules_display_state(Arc::clone(&rules_display));
    }
    app.set_diagnostics_source(Arc::clone(&diagnostics));

    // Feed predictions into the TUI app on a timer
    if cli.predict {
        let render_predictions = Arc::clone(&predictions);
        let render_cancel = cancel.child_token();
        let app_predictions = Arc::new(Mutex::new(Vec::<PredictionDisplay>::new()));
        let app_predictions_writer = Arc::clone(&app_predictions);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                tokio::select! {
                    _ = render_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(preds) = render_predictions.lock() {
                            let displays: Vec<PredictionDisplay> = preds.iter().map(|p| {
                                PredictionDisplay {
                                    pid: p.pid,
                                    process_name: p.process_name.clone(),
                                    anomaly_label: format!("{:?}", p.anomaly_type),
                                    confidence: p.confidence,
                                    eta_seconds: p.eta_seconds,
                                }
                            }).collect();
                            if let Ok(mut ap) = app_predictions_writer.lock() {
                                *ap = displays;
                            }
                        }
                    }
                }
            }
        });
        app.set_predictions_source(app_predictions);
    }

    let result = app.run(&mut terminal).await;

    // Cleanup: always restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    cancel.cancel();

    result.map_err(Into::into)
}

/// Initialize the JIT rule engine. Returns the event sender that the graph
/// updater should use to forward `MetricsUpdate` events to the engine.
fn init_rules_engine(
    rules_path: &std::path::Path,
    cancel: &CancellationToken,
    arbiter: Arc<Mutex<ArbiterQueue>>,
    display: Arc<Mutex<RulesDisplayState>>,
    diag_bridge: Option<Arc<Mutex<Vec<Diagnostic>>>>,
) -> anyhow::Result<mpsc::Sender<SystemEvent>> {
    // Load and compile rule file: lexer → parser → type-check → compile
    let source = std::fs::read_to_string(rules_path)?;
    let tokens = aether_script::lexer::tokenize(&source)?;
    let rule_file = aether_script::parser::parse(tokens).map_err(|errs| {
        anyhow::anyhow!(
            "parse errors in {}: {}",
            rules_path.display(),
            errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
        )
    })?;
    aether_script::types::TypeChecker::new()
        .check(&rule_file)
        .map_err(|errs| {
            anyhow::anyhow!(
                "type errors in {}: {}",
                rules_path.display(),
                errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
            )
        })?;
    let compiled = CompiledRuleSet::compile(&rule_file.rules)?;

    // Populate initial display state
    let names = compiled.rule_names();
    if let Ok(mut ds) = display.lock() {
        ds.rule_names.clone_from(&names);
        ds.match_counts = vec![0; names.len()];
    }

    // Create hot-reloader (owns Arc<ArcSwap<CompiledRuleSet>>)
    let reloader = HotReloader::new(compiled, vec![rules_path.to_path_buf()]);
    let shared_rules = reloader.rules();

    // Spawn hot-reload watcher
    let reload_cancel = cancel.child_token();
    tokio::spawn(async move {
        reloader.watch(reload_cancel).await;
    });

    // Create ScriptEngine with action channel
    let (rule_action_tx, mut rule_action_rx) = mpsc::channel::<RuleAction>(128);
    let mut engine = ScriptEngine::new(shared_rules, rule_action_tx);

    // Engine event channel (graph updater forwards MetricsUpdate here)
    let (engine_event_tx, engine_event_rx) = mpsc::channel::<SystemEvent>(256);

    // Spawn engine task
    let engine_cancel = cancel.child_token();
    tokio::spawn(async move {
        engine.run(engine_event_rx, engine_cancel).await;
    });

    // Spawn action forwarder: RuleAction → display state + arbiter + diagnostic bridge
    let forwarder_cancel = cancel.child_token();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = forwarder_cancel.cancelled() => break,
                Some(action) = rule_action_rx.recv() => {
                    // Update display state
                    if let Ok(mut ds) = display.lock() {
                        ds.total_actions += 1;
                        if let Some(idx) = ds.rule_names.iter().position(|n| n == &action.rule_name) {
                            if let Some(count) = ds.match_counts.get_mut(idx) {
                                *count += 1;
                            }
                        }
                    }

                    // Submit to arbiter as CustomScript action
                    let agent_action = AgentAction::CustomScript {
                        command: format!(
                            "rule:{} action:{} pid:{} sev:{}",
                            action.rule_name, action.action, action.target_pid, action.severity
                        ),
                    };
                    if let Ok(mut q) = arbiter.lock() {
                        q.submit("RuleEngine".into(), agent_action);
                    }

                    // Bridge to diagnostic engine if active
                    if let Some(ref diags) = diag_bridge {
                        let diag = rule_action_to_diagnostic(&action);
                        if let Ok(mut d) = diags.lock() {
                            d.push(diag);
                            if d.len() > 200 {
                                let excess = d.len() - 200;
                                d.drain(..excess);
                            }
                        }
                    }
                }
            }
        }
    });

    tracing::info!(
        "rules engine initialized: {} rules from {}",
        names.len(),
        rules_path.display()
    );
    Ok(engine_event_tx)
}

/// Convert a JIT script RuleAction into a core Diagnostic.
fn rule_action_to_diagnostic(action: &RuleAction) -> Diagnostic {
    static SCRIPT_DIAG_ID: AtomicU64 = AtomicU64::new(1_000_000);

    let severity = match action.severity {
        2 => Severity::Critical,
        1 => Severity::Warning,
        _ => Severity::Info,
    };

    let recommended = match action.action {
        2 => RecommendedAction::KillProcess {
            pid: action.target_pid,
            reason: format!("script rule '{}' triggered kill", action.rule_name),
        },
        1 => RecommendedAction::Investigate {
            what: format!("script rule '{}' raised alert", action.rule_name),
        },
        _ => RecommendedAction::NoAction {
            reason: format!("script rule '{}' logged event", action.rule_name),
        },
    };

    let urgency = match severity {
        Severity::Critical => Urgency::Immediate,
        Severity::Warning => Urgency::Soon,
        Severity::Info => Urgency::Informational,
    };

    Diagnostic {
        id: SCRIPT_DIAG_ID.fetch_add(1, Ordering::Relaxed),
        host: HostId::new("local"),
        target: DiagTarget::Process {
            pid: action.target_pid,
            name: String::new(),
        },
        severity,
        category: DiagCategory::ScriptRule,
        summary: format!(
            "[script] rule '{}' fired on pid {}",
            action.rule_name, action.target_pid
        ),
        evidence: vec![Evidence {
            metric: "script_rule".to_string(),
            current: action.severity as f64,
            threshold: 0.0,
            trend: None,
            context: format!(
                "action={}, severity={}",
                action.action, action.severity
            ),
        }],
        recommendation: Recommendation {
            action: recommended,
            reason: format!("triggered by .aether rule '{}'", action.rule_name),
            urgency,
            auto_executable: false,
        },
        detected_at: std::time::Instant::now(),
        resolved_at: None,
    }
}

/// Execute an approved agent action.
fn execute_action(action: &AgentAction) {
    match action {
        AgentAction::KillProcess { pid } => {
            tracing::info!("executing kill for pid {pid}");
            // Use sysinfo-based kill via system signal
            #[cfg(unix)]
            {
                use std::process::Command;
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
            #[cfg(not(unix))]
            {
                tracing::warn!("process kill not supported on this platform");
                let _ = pid;
            }
        }
        AgentAction::RestartService { name } => {
            tracing::info!("restart service '{name}' requested (not yet implemented)");
        }
        AgentAction::Inspect { pid } => {
            tracing::info!("inspect pid {pid} requested (handled by MCP tools)");
        }
        AgentAction::CustomScript { command } => {
            tracing::info!("custom script '{command}' requested (not yet implemented)");
        }
    }
}

/// Attempt to initialize eBPF telemetry: load BPF programs, create ring buffer
/// reader, and return an EbpfBridge for the ingestion pipeline.
#[cfg(all(target_os = "linux", feature = "ebpf"))]
async fn try_init_ebpf(
    event_tx: &mpsc::Sender<SystemEvent>,
    cancel: &CancellationToken,
) -> anyhow::Result<EbpfBridge> {
    use aether_ebpf::events::RawKernelEvent;
    use aether_ebpf::ring_buffer::RingBufferReader;
    use aether_ebpf::ProbeManager;

    // BPF bytecodes compiled from bpf/*.bpf.c — expected alongside the binary or
    // at a well-known path. The build step produces these ELF objects.
    let exe_dir = std::env::current_exe()?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let bpf_dir = exe_dir.join("bpf");

    let process_bytes = std::fs::read(bpf_dir.join("process_monitor.o"))?;
    let net_bytes = std::fs::read(bpf_dir.join("net_monitor.o"))?;
    let syscall_bytes = std::fs::read(bpf_dir.join("syscall_monitor.o"))?;

    let mut manager =
        ProbeManager::attach_all(&process_bytes, &net_bytes, &syscall_bytes).await?;

    let process_prog = manager
        .process_mut()
        .ok_or_else(|| anyhow::anyhow!("process monitor program not loaded"))?;
    let mut reader = RingBufferReader::new(process_prog.bpf_mut())?;

    let (raw_tx, raw_rx) = mpsc::channel::<RawKernelEvent>(256);
    let bridge = EbpfBridge::new(raw_rx, event_tx.clone());

    let reader_cancel = cancel.child_token();
    tokio::spawn(async move {
        let _manager = manager; // keep BPF programs attached
        loop {
            tokio::select! {
                _ = reader_cancel.cancelled() => break,
                result = reader.poll() => {
                    match result {
                        Ok(events) => {
                            for event in events {
                                if raw_tx.send(event).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("ring buffer poll error: {e}");
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(bridge)
}

#[cfg(not(all(target_os = "linux", feature = "ebpf")))]
async fn try_init_ebpf(
    _event_tx: &mpsc::Sender<SystemEvent>,
    _cancel: &CancellationToken,
) -> anyhow::Result<EbpfBridge> {
    anyhow::bail!("eBPF requires Linux with the 'ebpf' feature enabled")
}
