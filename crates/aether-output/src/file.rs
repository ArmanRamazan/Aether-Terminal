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

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::metrics::HostId;
    use aether_core::models::{
        DiagCategory, DiagTarget, Evidence, Recommendation, RecommendedAction, Urgency,
    };
    use std::time::Instant;

    fn make_diag(id: u64) -> Diagnostic {
        Diagnostic {
            id,
            host: HostId::new("node-1"),
            target: DiagTarget::Process {
                pid: 100,
                name: "api-server".into(),
            },
            severity: Severity::Critical,
            category: DiagCategory::ThroughputDrop,
            summary: "throughput dropped 45%".into(),
            evidence: vec![Evidence {
                metric: "req/s".into(),
                current: 55.0,
                threshold: 100.0,
                trend: Some(-0.45),
                context: "last 5 minutes".into(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::NoAction {
                    reason: "investigate".into(),
                },
                reason: "investigate root cause".into(),
                urgency: Urgency::Immediate,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[tokio::test]
    async fn test_file_append() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("output.jsonl");

        let sink = FileSink::new(path.clone(), 1, Severity::Info);

        sink.send(&make_diag(1)).await.expect("first write");
        sink.send(&make_diag(2)).await.expect("second write");

        let content = tokio::fs::read_to_string(&path).await.expect("read file");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "should have two JSON lines");

        // Verify each line is valid JSON
        for (i, line) in lines.iter().enumerate() {
            let parsed: serde_json::Value =
                serde_json::from_str(line).expect("each line should be valid JSON");
            assert_eq!(parsed["id"], (i + 1) as u64, "id should match");
        }
    }

    #[tokio::test]
    async fn test_file_rotation() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("output.jsonl");
        let rotated = dir.path().join("output.1");

        // Use raw bytes constructor to set a very small max size
        let sink = FileSink {
            path: path.clone(),
            max_size_bytes: 50, // very small to trigger rotation
            min_severity: Severity::Info,
        };

        // First write — creates file
        sink.send(&make_diag(1)).await.expect("first write");
        assert!(path.exists(), "output file should exist");

        // File should be larger than 50 bytes (a JSON line is ~300+ bytes)
        let meta = tokio::fs::metadata(&path).await.expect("metadata");
        assert!(meta.len() > 50, "file should exceed max_size_bytes");

        // Second write — should trigger rotation then append
        sink.send(&make_diag(2)).await.expect("second write");
        assert!(rotated.exists(), "rotated file (.1) should exist");
        assert!(path.exists(), "new output file should exist after rotation");

        // Rotated file has the first entry
        let old = tokio::fs::read_to_string(&rotated).await.expect("read rotated");
        let parsed: serde_json::Value =
            serde_json::from_str(old.trim()).expect("rotated content should be valid JSON");
        assert_eq!(parsed["id"], 1);

        // New file has the second entry
        let new = tokio::fs::read_to_string(&path).await.expect("read new");
        let parsed: serde_json::Value =
            serde_json::from_str(new.trim()).expect("new file content should be valid JSON");
        assert_eq!(parsed["id"], 2);
    }
}
