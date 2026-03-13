use std::path::PathBuf;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use aether_core::models::{Diagnostic, Severity};
use aether_core::traits::OutputSink;

use crate::serialize::diagnostic_to_json;

/// Appends diagnostics as JSON lines to a file with size-based rotation.
pub struct FileSink {
    path: PathBuf,
    max_size_bytes: u64,
    min_severity: Severity,
}

impl FileSink {
    /// Create a new file sink.
    pub fn new(path: PathBuf, max_size_mb: u64, min_severity: Severity) -> Self {
        Self {
            path,
            max_size_bytes: max_size_mb * 1024 * 1024,
            min_severity,
        }
    }

    /// Rotate the file if it exceeds max size.
    async fn maybe_rotate(&self) -> Result<(), std::io::Error> {
        match tokio::fs::metadata(&self.path).await {
            Ok(meta) if meta.len() >= self.max_size_bytes => {
                let rotated = self.path.with_extension("1");
                tokio::fs::rename(&self.path, &rotated).await?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[async_trait]
impl OutputSink for FileSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.maybe_rotate().await?;

        let mut line = diagnostic_to_json(diagnostic);
        line.push('\n');

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(line.as_bytes()).await?;

        Ok(())
    }

    fn name(&self) -> &str {
        "file"
    }

    fn min_severity(&self) -> Severity {
        self.min_severity
    }
}
