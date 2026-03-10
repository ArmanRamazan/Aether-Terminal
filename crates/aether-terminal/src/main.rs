use std::sync::{Arc, Mutex, RwLock};

use clap::Parser;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::events::SystemEvent;
use aether_core::{AgentAction, WorldGraph};
use aether_ingestion::ebpf_bridge::EbpfBridge;
use aether_ingestion::pipeline::IngestionPipeline;
use aether_ingestion::sysinfo_probe::SysinfoProbe;
use aether_mcp::arbiter::ArbiterQueue;
use aether_mcp::McpServer;
use aether_render::tui::app::App;

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

    /// Enable eBPF telemetry (Linux only, requires CAP_BPF)
    #[arg(long)]
    ebpf: bool,
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

    // Spawn graph updater: SystemEvent → WorldGraph
    let updater_world = Arc::clone(&world);
    let updater_cancel = cancel.child_token();
    tokio::spawn(async move {
        let mut event_rx = event_rx;
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Some(SystemEvent::MetricsUpdate { snapshot }) => {
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

    // Mode handling
    if cli.mcp_stdio {
        // Stdio MCP mode: no TUI, no crossterm
        tracing::info!("running in MCP stdio mode");
        let mcp = McpServer::new(
            Arc::clone(&world),
            Arc::clone(&arbiter),
            action_tx,
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
        );
        let sse_cancel = cancel.child_token();
        tokio::spawn(async move {
            if let Err(e) = mcp.run_sse(port, sse_cancel).await {
                tracing::error!("MCP SSE server error: {e}");
            }
        });
    }

    // Initialize terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Run TUI app
    let mut app = App::new(Arc::clone(&world));
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
