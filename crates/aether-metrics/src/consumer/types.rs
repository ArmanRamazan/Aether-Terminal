//! Prometheus API response types for JSON deserialization.

use std::collections::HashMap;

use serde::Deserialize;

/// Top-level Prometheus API response.
#[derive(Debug, Deserialize)]
pub struct PromResponse {
    pub status: String,
    pub data: PromData,
}

/// Data payload containing result type and results.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PromData {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub result: Vec<PromResult>,
}

/// A single result entry with metric labels and sample value(s).
#[derive(Debug, Deserialize)]
pub struct PromResult {
    pub metric: HashMap<String, String>,
    /// Instant query value: `[timestamp, "value"]`.
    pub value: Option<(f64, String)>,
    /// Range query values: `[[timestamp, "value"], ...]`.
    pub values: Option<Vec<(f64, String)>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prom_response_deserialize() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    {
                        "metric": {"__name__": "up", "job": "node"},
                        "value": [1710000000.0, "1"]
                    },
                    {
                        "metric": {"__name__": "up", "job": "prom"},
                        "value": [1710000000.0, "0.5"],
                        "values": [[1710000000.0, "0.5"], [1710000001.0, "0.6"]]
                    }
                ]
            }
        }"#;

        let resp: PromResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(resp.status, "success");
        assert_eq!(resp.data.result_type, "vector");
        assert_eq!(resp.data.result.len(), 2);

        let first = &resp.data.result[0];
        assert_eq!(first.metric.get("job").unwrap(), "node");
        let (ts, val) = first.value.as_ref().expect("should have value");
        assert!((ts - 1_710_000_000.0).abs() < 0.1);
        assert_eq!(val, "1");

        let second = &resp.data.result[1];
        let values = second.values.as_ref().expect("should have values");
        assert_eq!(values.len(), 2);
        assert_eq!(values[1].1, "0.6");
    }
}
