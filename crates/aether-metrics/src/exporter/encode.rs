use std::fmt::Write;

use super::registry::{MetricFamily, MetricType};

/// Encode metric families in Prometheus text exposition format.
pub fn encode_openmetrics(families: &[MetricFamily]) -> String {
    let mut out = String::new();

    for family in families {
        let name = &family.desc.name;
        let type_str = match family.desc.metric_type {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
        };

        let _ = writeln!(out, "# HELP {name} {}", family.desc.help);
        let _ = writeln!(out, "# TYPE {name} {type_str}");

        for (labels, value) in &family.samples {
            if labels.is_empty() {
                let _ = writeln!(out, "{name} {value}");
            } else {
                let label_str: String = labels
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = writeln!(out, "{name}{{{label_str}}} {value}");
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exporter::registry::{LabelSet, MetricDesc, MetricRegistry, MetricType};

    #[test]
    fn test_encode_valid_format() {
        let mut reg = MetricRegistry::default();
        reg.register(MetricDesc {
            name: "up".into(),
            help: "Service up".into(),
            metric_type: MetricType::Gauge,
        });
        reg.set_gauge("up", LabelSet::new(), 1.0);

        let snap = reg.snapshot();
        let encoded = encode_openmetrics(&snap);

        assert!(
            encoded.contains("# TYPE up gauge"),
            "should contain TYPE line"
        );
        assert!(encoded.contains("up 1"), "should contain metric value line");
    }

    #[test]
    fn test_labels_sorted() {
        let mut reg = MetricRegistry::default();
        reg.register(MetricDesc {
            name: "temp".into(),
            help: "Temperature".into(),
            metric_type: MetricType::Gauge,
        });

        let mut labels = LabelSet::new();
        labels.insert("zone".into(), "us-east".into());
        labels.insert("app".into(), "web".into());
        reg.set_gauge("temp", labels, 72.0);

        let snap = reg.snapshot();
        let encoded = encode_openmetrics(&snap);

        assert!(
            encoded.contains("app=\"web\",zone=\"us-east\""),
            "labels should be sorted alphabetically, got: {encoded}"
        );
    }
}
