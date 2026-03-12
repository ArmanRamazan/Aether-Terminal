use std::collections::BTreeMap;

/// Type of a metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

/// Descriptor for a registered metric.
#[derive(Debug, Clone)]
pub struct MetricDesc {
    pub name: String,
    pub help: String,
    pub metric_type: MetricType,
}

/// Sorted label key-value pairs.
pub type LabelSet = BTreeMap<String, String>;

/// A metric family: descriptor plus collected samples.
#[derive(Debug, Clone)]
pub struct MetricFamily {
    pub desc: MetricDesc,
    pub samples: Vec<(LabelSet, f64)>,
}

/// Collects and stores metric samples for export.
#[derive(Debug, Default)]
pub struct MetricRegistry {
    descs: Vec<MetricDesc>,
    /// name -> (label_set -> value)
    values: BTreeMap<String, BTreeMap<LabelSet, f64>>,
}

impl MetricRegistry {
    /// Register a metric descriptor.
    pub fn register(&mut self, desc: MetricDesc) {
        self.values.entry(desc.name.clone()).or_default();
        self.descs.push(desc);
    }

    /// Set a gauge to an absolute value.
    pub fn set_gauge(&mut self, name: &str, labels: LabelSet, value: f64) {
        if let Some(samples) = self.values.get_mut(name) {
            samples.insert(labels, value);
        }
    }

    /// Increment a counter by delta.
    pub fn inc_counter(&mut self, name: &str, labels: LabelSet, delta: f64) {
        if let Some(samples) = self.values.get_mut(name) {
            let entry = samples.entry(labels).or_insert(0.0);
            *entry += delta;
        }
    }

    /// Take a consistent snapshot of all metric families.
    pub fn snapshot(&self) -> Vec<MetricFamily> {
        self.descs
            .iter()
            .map(|desc| {
                let samples = self
                    .values
                    .get(&desc.name)
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
                    .unwrap_or_default();
                MetricFamily {
                    desc: desc.clone(),
                    samples,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gauge_set_and_snapshot() {
        let mut reg = MetricRegistry::default();
        reg.register(MetricDesc {
            name: "cpu_usage".into(),
            help: "CPU usage percentage".into(),
            metric_type: MetricType::Gauge,
        });

        let mut labels = LabelSet::new();
        labels.insert("host".into(), "node1".into());
        reg.set_gauge("cpu_usage", labels, 42.5);

        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1, "should have one metric family");
        assert_eq!(snap[0].samples.len(), 1, "should have one sample");
        assert_eq!(snap[0].samples[0].1, 42.5, "gauge value should be 42.5");
    }

    #[test]
    fn test_counter_increments() {
        let mut reg = MetricRegistry::default();
        reg.register(MetricDesc {
            name: "requests_total".into(),
            help: "Total requests".into(),
            metric_type: MetricType::Counter,
        });

        let labels = LabelSet::new();
        reg.inc_counter("requests_total", labels.clone(), 3.0);
        reg.inc_counter("requests_total", labels, 7.0);

        let snap = reg.snapshot();
        assert_eq!(snap[0].samples[0].1, 10.0, "counter should sum to 10");
    }
}
