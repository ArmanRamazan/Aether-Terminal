use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use clap::Parser;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_analyze::engine::{AnalyzeConfig, AnalyzeEngine};
use aether_config::AetherConfig;
use aether_config::types::TargetConfig;
use aether_core::event_bus::EventBus;
use aether_core::events::{IntegrationEvent, SystemEvent};
use aether_core::models::{
    DiagCategory, DiagTarget, Diagnostic, Endpoint, EndpointType, Evidence, Recommendation,
    RecommendedAction, Severity, Target, TargetKind, Urgency,
};
use aether_core::metrics::HostId;
use aether_core::traits::ServiceDiscovery;
use aether_core::{AgentAction, WorldGraph};
use aether_discovery::DiscoveryEngine;
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
use aether_metrics::scraper::PrometheusScraper;
use aether_output::{
    DiscordSink, FileSink, OutputFormat, OutputPipeline, SlackSink, StdoutSink, TelegramSink,
};
use aether_api::AetherGrpcServer;
use aether_api::proto::aether_service_server::AetherServiceServer;
use aether_core::event_bus::InProcessEventBus;
use aether_prober::ProberEngine;
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
    /// Path to configuration file (TOML or YAML)
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

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
        .unwrap_or_else(|e| {
            eprintln!(
                "Warning: invalid log level '{}': {}. Using 'info'.",
                cli.log_level, e
            );
            tracing_subscriber::EnvFilter::new("info")
        });

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

    // Load configuration file if --config provided
    let config = if let Some(ref config_path) = cli.config {
        tracing::info!("loading config from {}", config_path.display());
        Arc::new(aether_config::load(config_path)?)
    } else {
        Arc::new(AetherConfig::default())
    };

    // --- Service Discovery ---
    // Convert static config targets to core Target type
    let mut targets: Vec<Target> = config
        .targets
        .iter()
        .map(target_from_config)
        .collect();

    // Run auto-discovery if enabled
    let discovery_engine = if config.discovery.enabled {
        let engine = DiscoveryEngine::new(
            "127.0.0.1".to_owned(),
            config.discovery.scan_ports.clone(),
            Duration::from_secs(config.discovery.interval_seconds),
        );
        match engine.discover().await {
            Ok(discovered) => {
                let new_count = merge_targets(&mut targets, discovered);
                if new_count > 0 {
                    tracing::info!(new_count, "auto-discovery found new targets");
                }
            }
            Err(e) => {
                tracing::warn!("initial discovery failed: {e}, continuing with config targets");
            }
        }
        Some(engine)
    } else {
        None
    };

    let targets = Arc::new(RwLock::new(targets));
    tracing::info!(count = targets.read().map(|t| t.len()).unwrap_or(0), "monitoring targets loaded");

    let world = Arc::new(RwLock::new(WorldGraph::new()));
    let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
    let event_bus = Arc::new(InProcessEventBus::new(256));
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

    // Spawn periodic re-discovery task
    if let Some(engine) = discovery_engine {
        let rediscovery_targets = Arc::clone(&targets);
        let rediscovery_cancel = cancel.child_token();
        let interval = engine.interval();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip immediate tick (already ran initial discovery)
            loop {
                tokio::select! {
                    _ = rediscovery_cancel.cancelled() => break,
                    _ = ticker.tick() => {
                        match engine.discover().await {
                            Ok(discovered) => {
                                if let Ok(mut tgt) = rediscovery_targets.write() {
                                    let new_count = merge_targets(&mut tgt, discovered);
                                    if new_count > 0 {
                                        tracing::info!(new_count, "re-discovery found new targets");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("periodic discovery failed: {e}");
                            }
                        }
                    }
                }
            }
        });
        tracing::info!(interval_secs = interval.as_secs(), "periodic re-discovery enabled");
    }

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

    // Shared metric channel: all remote metric sources fan-in here → AnalyzeEngine.
    // Multiple senders (consumer, scraper, prober), one receiver.
    let (metrics_tx, metrics_rx) = mpsc::channel::<Vec<aether_core::TimeSeries>>(128);

    // Spawn Prometheus consumer if --prometheus is set (polls Prometheus server API)
    if let Some(ref prom_url) = cli.prometheus {
        let interval_secs = cli.prometheus_interval
            .unwrap_or(config.scrape.interval_seconds);
        let consumer = PrometheusConsumer::new(
            prom_url,
            std::time::Duration::from_secs(interval_secs),
        )?;
        let consumer_tx = metrics_tx.clone();
        let prom_cancel = cancel.child_token();
        tokio::spawn(async move {
            consumer.run(consumer_tx, prom_cancel).await;
        });
        tracing::info!("Prometheus consumer polling {prom_url} every {interval_secs}s");
    }

    // Spawn Prometheus scraper: scrapes /metrics endpoints on discovered targets
    {
        let scraper = PrometheusScraper::new(
            Arc::clone(&targets),
            Duration::from_secs(config.scrape.timeout_seconds),
        );
        let scraper_tx = metrics_tx.clone();
        let scraper_cancel = cancel.child_token();
        let scrape_interval = Duration::from_secs(config.scrape.interval_seconds);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(scrape_interval);
            ticker.tick().await; // skip immediate tick — let discovery populate targets first
            loop {
                tokio::select! {
                    _ = scraper_cancel.cancelled() => break,
                    _ = ticker.tick() => {
                        match scraper.scrape().await {
                            Ok(series) if !series.is_empty() => {
                                if scraper_tx.send(series).await.is_err() {
                                    break;
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!("prometheus scraper error: {e}");
                            }
                        }
                    }
                }
            }
        });
        tracing::info!(
            interval_secs = config.scrape.interval_seconds,
            "prometheus scraper enabled"
        );
    }

    // Spawn network prober: HTTP health + TCP connectivity checks on targets
    {
        let prober = ProberEngine::new(
            Arc::clone(&targets),
            Duration::from_secs(config.probe.timeout_seconds),
        );
        let prober_tx = metrics_tx.clone();
        let prober_cancel = cancel.child_token();
        let probe_interval = Duration::from_secs(config.probe.interval_seconds);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(probe_interval);
            ticker.tick().await; // skip immediate tick
            loop {
                tokio::select! {
                    _ = prober_cancel.cancelled() => break,
                    _ = ticker.tick() => {
                        match prober.probe().await {
                            Ok(metrics) if !metrics.is_empty() => {
                                // Convert CollectedMetric → TimeSeries for MetricStore
                                let series = collected_to_timeseries(metrics);
                                if !series.is_empty()
                                    && prober_tx.send(series).await.is_err()
                                {
                                    break;
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!("network prober error: {e}");
                            }
                        }
                    }
                }
            }
        });
        tracing::info!(
            interval_secs = config.probe.interval_seconds,
            "network prober enabled"
        );
    }

    // Drop the original sender so channel closes when all spawned senders finish
    drop(metrics_tx);

    // Build output pipeline from config
    let output_pipeline = {
        let mut sinks: Vec<Box<dyn aether_core::traits::OutputSink>> = Vec::new();

        if let Some(ref slack) = config.output.slack {
            let sev = parse_severity(&slack.severity);
            sinks.push(Box::new(SlackSink::new(slack.webhook_url.clone(), sev)));
            tracing::info!("output: Slack sink enabled (min severity: {sev})");
        }
        if let Some(ref discord) = config.output.discord {
            let sev = parse_severity(&discord.severity);
            sinks.push(Box::new(DiscordSink::new(discord.webhook_url.clone(), sev)));
            tracing::info!("output: Discord sink enabled (min severity: {sev})");
        }
        if let Some(ref telegram) = config.output.telegram {
            let sev = parse_severity(&telegram.severity);
            sinks.push(Box::new(TelegramSink::new(
                telegram.bot_token.clone(),
                telegram.chat_id.clone(),
                sev,
            )));
            tracing::info!("output: Telegram sink enabled (min severity: {sev})");
        }
        if let Some(ref stdout_cfg) = config.output.stdout {
            if stdout_cfg.enabled {
                let sev = parse_severity(&stdout_cfg.severity);
                let fmt = OutputFormat::from_str_config(&stdout_cfg.format);
                sinks.push(Box::new(StdoutSink::new(fmt, sev)));
                tracing::info!("output: stdout sink enabled (format: {}, min severity: {sev})", stdout_cfg.format);
            }
        }
        if let Some(ref file_cfg) = config.output.file {
            let sev = parse_severity(&file_cfg.severity);
            sinks.push(Box::new(FileSink::new(
                std::path::PathBuf::from(&file_cfg.path),
                file_cfg.max_size_mb,
                sev,
            )));
            tracing::info!("output: file sink enabled (path: {}, min severity: {sev})", file_cfg.path);
        }

        if !sinks.is_empty() {
            tracing::info!(sink_count = sinks.len(), "output pipeline initialized");
        }
        Arc::new(OutputPipeline::new(sinks, Duration::from_secs(60)))
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
        engine = engine.with_prometheus_rx(metrics_rx);
        let (diag_tx, mut diag_rx) = mpsc::channel::<Vec<Diagnostic>>(32);
        let analyze_cancel = cancel.child_token();
        let analyze_world = Arc::clone(&world);
        tokio::spawn(async move {
            engine.run(analyze_world, diag_tx, analyze_cancel).await;
        });

        let collector_diags = Arc::clone(&diagnostics);
        let collector_cancel = cancel.child_token();
        let collector_pipeline = Arc::clone(&output_pipeline);
        let collector_bus = Arc::clone(&event_bus);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = collector_cancel.cancelled() => break,
                    batch = diag_rx.recv() => {
                        match batch {
                            Some(new_diags) => {
                                // Dispatch each new diagnostic through the output pipeline
                                if collector_pipeline.sink_count() > 0 {
                                    for diag in &new_diags {
                                        let pipeline = Arc::clone(&collector_pipeline);
                                        let diag = diag.clone();
                                        tokio::spawn(async move {
                                            pipeline.dispatch(&diag).await;
                                        });
                                    }
                                }
                                // Publish integration events for each diagnostic
                                for diag in &new_diags {
                                    collector_bus.publish(IntegrationEvent::DiagnosticCreated {
                                        diagnostic_id: diag.id,
                                        severity: format!("{}", diag.severity),
                                        summary: diag.summary.clone(),
                                    }).await;
                                }
                                if let Ok(mut diags) = collector_diags.lock() {
                                    upsert_diagnostics(&mut diags, new_diags);
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

    // Spawn gRPC API server if configured
    if let Some(ref grpc_cfg) = config.api.grpc {
        if grpc_cfg.enabled {
            let grpc_targets = {
                let snapshot = targets.read().map(|t| t.clone()).unwrap_or_default();
                Arc::new(Mutex::new(snapshot))
            };
            let grpc_server = AetherGrpcServer::new(
                Arc::clone(&diagnostics),
                grpc_targets,
                Arc::clone(&event_bus),
                Arc::clone(&arbiter),
            );
            let addr = grpc_cfg.bind.clone();
            let grpc_cancel = cancel.child_token();
            tokio::spawn(async move {
                let addr = match addr.parse() {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::error!("invalid gRPC bind address: {e}");
                        return;
                    }
                };
                tracing::info!("gRPC API listening on {addr}");
                if let Err(e) = tonic::transport::Server::builder()
                    .add_service(AetherServiceServer::new(grpc_server))
                    .serve_with_shutdown(addr, grpc_cancel.cancelled())
                    .await
                {
                    tracing::error!("gRPC server error: {e}");
                }
            });
        }
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
                            upsert_diagnostics(&mut d, std::iter::once(diag));
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
        Severity::Info | _ => Urgency::Informational,
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
        _ => {
            tracing::warn!("unknown agent action: {action:?}");
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

/// Parse a severity string from config into a Severity enum.
fn parse_severity(s: &str) -> Severity {
    match s {
        "critical" => Severity::Critical,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    }
}

/// Upsert diagnostics by (target, category) key. Evicts lowest-severity when over capacity.
fn upsert_diagnostics(diags: &mut Vec<Diagnostic>, new_diags: impl IntoIterator<Item = Diagnostic>) {
    const MAX_DIAGNOSTICS: usize = 200;

    for new_diag in new_diags {
        if let Some(existing) = diags
            .iter_mut()
            .find(|d| d.target == new_diag.target && d.category == new_diag.category)
        {
            *existing = new_diag;
        } else {
            diags.push(new_diag);
        }
    }
    if diags.len() > MAX_DIAGNOSTICS {
        diags.sort_by_key(|d| d.severity);
        let excess = diags.len() - MAX_DIAGNOSTICS;
        diags.drain(..excess);
    }
}

/// Convert a config target definition into a core Target.
fn target_from_config(tc: &TargetConfig) -> Target {
    let kind = match tc.kind.as_deref() {
        Some("container") => TargetKind::Container,
        Some("pod") => TargetKind::Pod,
        Some("process") => TargetKind::Process,
        _ => TargetKind::Service,
    };

    let mut endpoints = Vec::new();
    if let Some(ref url) = tc.prometheus {
        endpoints.push(Endpoint {
            url: url.clone(),
            endpoint_type: EndpointType::Prometheus,
        });
    }
    if let Some(ref url) = tc.health {
        endpoints.push(Endpoint {
            url: url.clone(),
            endpoint_type: EndpointType::Health,
        });
    }
    if let Some(ref addr) = tc.probe_tcp {
        endpoints.push(Endpoint {
            url: format!("tcp://{addr}"),
            endpoint_type: EndpointType::TcpProbe,
        });
    }
    if let Some(ref path) = tc.logs {
        endpoints.push(Endpoint {
            url: path.clone(),
            endpoint_type: EndpointType::Logs,
        });
    }

    Target {
        id: format!("cfg-{}", tc.name),
        name: tc.name.clone(),
        kind,
        endpoints,
        labels: tc.labels.clone(),
        discovered_at: SystemTime::now(),
    }
}

/// Convert collected metrics (from prober/scraper) into TimeSeries for MetricStore.
fn collected_to_timeseries(metrics: Vec<aether_core::models::CollectedMetric>) -> Vec<aether_core::TimeSeries> {
    use std::collections::BTreeMap;
    use std::time::Instant;

    let now = Instant::now();
    metrics
        .into_iter()
        .map(|m| {
            let mut ts = aether_core::TimeSeries::new(&m.name, 3600);
            ts.labels = m.labels.into_iter().collect::<BTreeMap<_, _>>();
            ts.push_sample(aether_core::MetricSample {
                timestamp: now,
                value: m.value,
            });
            ts
        })
        .collect()
}

/// Merge discovered targets into the list, skipping duplicates by id.
/// Returns the number of newly added targets.
fn merge_targets(existing: &mut Vec<Target>, discovered: Vec<Target>) -> usize {
    let mut added = 0;
    for target in discovered {
        if !existing.iter().any(|t| t.id == target.id) {
            tracing::info!(id = %target.id, name = %target.name, "discovered new target");
            existing.push(target);
            added += 1;
        }
    }
    added
}

#[cfg(not(all(target_os = "linux", feature = "ebpf")))]
async fn try_init_ebpf(
    _event_tx: &mpsc::Sender<SystemEvent>,
    _cancel: &CancellationToken,
) -> anyhow::Result<EbpfBridge> {
    anyhow::bail!("eBPF requires Linux with the 'ebpf' feature enabled")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn make_diag(pid: u32, category: DiagCategory, severity: Severity) -> Diagnostic {
        Diagnostic {
            id: pid as u64,
            host: HostId::new("test"),
            target: DiagTarget::Process {
                pid,
                name: format!("proc-{pid}"),
            },
            severity,
            category,
            summary: String::new(),
            evidence: vec![],
            recommendation: Recommendation {
                action: RecommendedAction::NoAction {
                    reason: String::new(),
                },
                urgency: Urgency::Informational,
                reason: String::new(),
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[test]
    fn test_upsert_replaces_existing_by_target_and_category() {
        let mut diags = vec![make_diag(1, DiagCategory::CpuSpike, Severity::Warning)];
        let updated = make_diag(1, DiagCategory::CpuSpike, Severity::Critical);
        upsert_diagnostics(&mut diags, std::iter::once(updated));

        assert_eq!(diags.len(), 1, "should replace, not append");
        assert_eq!(diags[0].severity, Severity::Critical);
    }

    #[test]
    fn test_upsert_pushes_new_when_key_differs() {
        let mut diags = vec![make_diag(1, DiagCategory::CpuSpike, Severity::Info)];
        upsert_diagnostics(
            &mut diags,
            std::iter::once(make_diag(2, DiagCategory::MemoryLeak, Severity::Warning)),
        );

        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn test_upsert_evicts_lowest_severity_over_capacity() {
        let mut diags: Vec<Diagnostic> = (0..200)
            .map(|i| make_diag(i, DiagCategory::CpuSpike, Severity::Critical))
            .collect();
        // Add one more with Info severity — should trigger eviction of lowest
        upsert_diagnostics(
            &mut diags,
            std::iter::once(make_diag(999, DiagCategory::MemoryLeak, Severity::Info)),
        );

        assert_eq!(diags.len(), 200, "should evict back to 200");
        // The Info one should be evicted (lowest severity)
        assert!(
            diags.iter().all(|d| d.severity == Severity::Critical),
            "Info diagnostic should have been evicted"
        );
    }
}
