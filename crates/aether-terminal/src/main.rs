use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::events::SystemEvent;
use aether_ingestion::pipeline::IngestionPipeline;
use aether_ingestion::sysinfo_probe::SysinfoProbe;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let probe = Arc::new(SysinfoProbe::new());
    let (event_tx, mut event_rx) = mpsc::channel::<SystemEvent>(256);
    let cancel = CancellationToken::new();

    let pipeline = IngestionPipeline::new(probe, event_tx);
    let pipeline_cancel = cancel.child_token();
    tokio::spawn(async move {
        if let Err(e) = pipeline.run(pipeline_cancel).await {
            tracing::error!("ingestion pipeline error: {e}");
        }
    });

    tracing::info!("Aether Terminal started, collecting metrics…");

    tokio::select! {
        _ = async {
            while let Some(event) = event_rx.recv().await {
                if let SystemEvent::MetricsUpdate { snapshot } = event {
                    print!("\x1B[2J\x1B[H");

                    let mut procs = snapshot.processes;
                    procs.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));

                    println!("Aether Terminal v0.1.0 — Live Process Data");
                    println!("Processes: {}", procs.len());
                    println!();

                    for p in procs.iter().take(10) {
                        println!(
                            "[PID {:>6}] {:<20} CPU: {:>5.1}%  MEM: {}  HP: {}",
                            p.pid,
                            p.name,
                            p.cpu_percent,
                            format_mem(p.mem_bytes),
                            p.hp,
                        );
                    }
                }
            }
        } => {}
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            tracing::info!("timeout reached, shutting down");
            cancel.cancel();
        }
    }

    Ok(())
}

/// Format bytes into a human-readable string.
fn format_mem(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{} KB", bytes / KB)
    } else if bytes < GB {
        format!("{} MB", bytes / MB)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}
