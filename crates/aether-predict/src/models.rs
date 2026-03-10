//! Prediction types and confidence scoring.
//!
//! Defines [`PredictedAnomaly`] (the output of the prediction pipeline),
//! [`AnomalyType`] (classification of detected anomalies), and
//! [`ConfidenceScorer`] (maps raw model scores to classified anomaly types).

use serde::{Deserialize, Serialize};

/// Classification of predicted anomaly types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AnomalyType {
    /// Process likely to be killed by OOM killer.
    OomRisk,
    /// CPU usage spike predicted.
    CpuSpike,
    /// Gradual memory growth without release.
    MemoryLeak,
    /// Thread contention or lock cycle detected.
    Deadlock,
    /// Disk partition approaching capacity.
    DiskExhaustion,
}

impl AnomalyType {
    /// Default anomaly score threshold for this type.
    ///
    /// Scores above this threshold trigger an anomaly prediction.
    pub fn default_threshold(self) -> f32 {
        match self {
            Self::OomRisk => 0.75,
            Self::CpuSpike => 0.70,
            Self::MemoryLeak => 0.80,
            Self::Deadlock => 0.85,
            Self::DiskExhaustion => 0.78,
        }
    }

    /// Suggested action when this anomaly is predicted.
    pub fn recommended_action(self) -> &'static str {
        match self {
            Self::OomRisk => "reduce memory usage or increase limits",
            Self::CpuSpike => "throttle workload or scale horizontally",
            Self::MemoryLeak => "investigate allocations and restart if needed",
            Self::Deadlock => "check lock ordering and thread dependencies",
            Self::DiskExhaustion => "free disk space or expand volume",
        }
    }
}

/// A predicted anomaly emitted by the prediction pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedAnomaly {
    /// Process ID of the affected process.
    pub pid: u32,
    /// Name of the affected process.
    pub process_name: String,
    /// Type of anomaly predicted.
    pub anomaly_type: AnomalyType,
    /// Confidence score in [0.0, 1.0].
    pub confidence: f32,
    /// Estimated seconds until the anomaly manifests.
    pub eta_seconds: f32,
    /// Suggested remediation action.
    pub recommended_action: String,
}

/// Maps raw model output scores to classified anomaly types.
pub struct ConfidenceScorer {
    thresholds: [(AnomalyType, f32); 5],
}

impl ConfidenceScorer {
    /// Creates a scorer with default thresholds for each anomaly type.
    pub fn new() -> Self {
        Self {
            thresholds: [
                (AnomalyType::OomRisk, AnomalyType::OomRisk.default_threshold()),
                (AnomalyType::CpuSpike, AnomalyType::CpuSpike.default_threshold()),
                (AnomalyType::MemoryLeak, AnomalyType::MemoryLeak.default_threshold()),
                (AnomalyType::Deadlock, AnomalyType::Deadlock.default_threshold()),
                (AnomalyType::DiskExhaustion, AnomalyType::DiskExhaustion.default_threshold()),
            ],
        }
    }

    /// Clamp a raw score to [0.0, 1.0].
    pub fn score(raw: f32) -> f32 {
        raw.clamp(0.0, 1.0)
    }

    /// Classify a feature vector's scores into the highest-confidence anomaly type.
    ///
    /// `scores` maps anomaly type index to raw score:
    /// [oom_risk, cpu_spike, memory_leak, deadlock, disk_exhaustion].
    ///
    /// Returns `Some((anomaly_type, confidence))` if any score exceeds its threshold,
    /// choosing the highest-confidence match. Returns `None` if all scores are below threshold.
    pub fn classify(&self, scores: &[f32; 5]) -> Option<(AnomalyType, f32)> {
        let mut best: Option<(AnomalyType, f32)> = None;

        for (i, &(anomaly_type, threshold)) in self.thresholds.iter().enumerate() {
            let confidence = Self::score(scores[i]);
            if confidence >= threshold {
                match best {
                    Some((_, best_conf)) if best_conf >= confidence => {}
                    _ => best = Some((anomaly_type, confidence)),
                }
            }
        }

        best
    }
}

impl Default for ConfidenceScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predicted_anomaly_serde_roundtrip() {
        let anomaly = PredictedAnomaly {
            pid: 1234,
            process_name: "test-proc".into(),
            anomaly_type: AnomalyType::CpuSpike,
            confidence: 0.85,
            eta_seconds: 30.0,
            recommended_action: "throttle workload".into(),
        };

        let json = serde_json::to_string(&anomaly).expect("serialize");
        let deserialized: PredictedAnomaly = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.pid, anomaly.pid);
        assert_eq!(deserialized.process_name, anomaly.process_name);
        assert_eq!(deserialized.anomaly_type, anomaly.anomaly_type);
        assert!((deserialized.confidence - anomaly.confidence).abs() < f32::EPSILON);
        assert!((deserialized.eta_seconds - anomaly.eta_seconds).abs() < f32::EPSILON);
        assert_eq!(deserialized.recommended_action, anomaly.recommended_action);
    }

    #[test]
    fn test_confidence_in_zero_one_range() {
        assert!((ConfidenceScorer::score(0.5) - 0.5).abs() < f32::EPSILON);
        assert!((ConfidenceScorer::score(-0.3) - 0.0).abs() < f32::EPSILON);
        assert!((ConfidenceScorer::score(1.5) - 1.0).abs() < f32::EPSILON);
        assert!((ConfidenceScorer::score(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((ConfidenceScorer::score(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_classify_high_cpu_returns_cpu_spike() {
        let scorer = ConfidenceScorer::new();
        // Only cpu_spike (index 1) above threshold.
        let scores = [0.1, 0.90, 0.1, 0.1, 0.1];
        let result = scorer.classify(&scores);
        assert!(result.is_some(), "should classify high CPU score");
        let (anomaly_type, confidence) = result.unwrap();
        assert_eq!(anomaly_type, AnomalyType::CpuSpike);
        assert!((confidence - 0.90).abs() < f32::EPSILON);
    }

    #[test]
    fn test_default_thresholds_sensible() {
        let types = [
            AnomalyType::OomRisk,
            AnomalyType::CpuSpike,
            AnomalyType::MemoryLeak,
            AnomalyType::Deadlock,
            AnomalyType::DiskExhaustion,
        ];

        for anomaly_type in types {
            let threshold = anomaly_type.default_threshold();
            assert!(
                (0.5..=0.95).contains(&threshold),
                "{anomaly_type:?} threshold {threshold} outside sensible range [0.5, 0.95]"
            );
        }
    }

    #[test]
    fn test_classify_returns_none_below_thresholds() {
        let scorer = ConfidenceScorer::new();
        let scores = [0.1, 0.1, 0.1, 0.1, 0.1];
        assert!(scorer.classify(&scores).is_none(), "all below threshold");
    }

    #[test]
    fn test_classify_picks_highest_confidence() {
        let scorer = ConfidenceScorer::new();
        // Both OomRisk and CpuSpike above threshold, OomRisk higher.
        let scores = [0.95, 0.80, 0.1, 0.1, 0.1];
        let (anomaly_type, _) = scorer.classify(&scores).unwrap();
        assert_eq!(anomaly_type, AnomalyType::OomRisk);
    }
}
