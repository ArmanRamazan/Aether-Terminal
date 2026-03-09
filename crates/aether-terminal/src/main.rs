use std::sync::{Arc, RwLock};

use clap::Parser;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::events::SystemEvent;
use aether_core::WorldGraph;
use aether_ingestion::pipeline::IngestionPipeline;
use aether_ingestion::sysinfo_probe::SysinfoProbe;
use aether_render::tui::app::App;

#[derive(Parser)]
#[command(name = "aether", version, about = "Cinematic 3D TUI system monitor")]
struct Cli {
    /// Logging level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&cli.log_level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let world = Arc::new(RwLock::new(WorldGraph::new()));

    let probe = Arc::new(SysinfoProbe::new());
    let (event_tx, event_rx) = mpsc::channel::<SystemEvent>(256);
    let cancel = CancellationToken::new();

    // Spawn ingestion pipeline
    let pipeline = IngestionPipeline::new(probe, event_tx);
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
