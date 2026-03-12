//! PromQL query builder for constructing metric queries.

/// Builds PromQL query strings incrementally.
#[derive(Debug, Clone, Default)]
pub struct QueryBuilder {
    metric_name: String,
    labels: Vec<(String, String)>,
    wrappers: Vec<Wrapper>,
}

#[derive(Debug, Clone)]
enum Wrapper {
    Rate(String),
    AvgOverTime(String),
}

impl QueryBuilder {
    /// Sets the metric name.
    pub fn metric(mut self, name: &str) -> Self {
        self.metric_name = name.to_string();
        self
    }

    /// Adds a label matcher (`key="value"`).
    pub fn label(mut self, key: &str, value: &str) -> Self {
        self.labels.push((key.to_string(), value.to_string()));
        self
    }

    /// Wraps the query in `rate(...[interval])`.
    pub fn rate(mut self, interval: &str) -> Self {
        self.wrappers.push(Wrapper::Rate(interval.to_string()));
        self
    }

    /// Wraps the query in `avg_over_time(...[interval])`.
    pub fn avg_over_time(mut self, interval: &str) -> Self {
        self.wrappers
            .push(Wrapper::AvgOverTime(interval.to_string()));
        self
    }

    /// Produces the final PromQL string.
    pub fn build(&self) -> String {
        let mut query = self.metric_name.clone();

        if !self.labels.is_empty() {
            let pairs: Vec<String> = self
                .labels
                .iter()
                .map(|(k, v)| format!("{k}=\"{v}\""))
                .collect();
            query = format!("{query}{{{}}}", pairs.join(","));
        }

        for wrapper in &self.wrappers {
            query = match wrapper {
                Wrapper::Rate(interval) => format!("rate({query}[{interval}])"),
                Wrapper::AvgOverTime(interval) => {
                    format!("avg_over_time({query}[{interval}])")
                }
            };
        }

        query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder_simple() {
        let q = QueryBuilder::default().metric("cpu_usage").build();
        assert_eq!(q, "cpu_usage");
    }

    #[test]
    fn test_query_builder_labels() {
        let q = QueryBuilder::default()
            .metric("up")
            .label("job", "node")
            .build();
        assert_eq!(q, r#"up{job="node"}"#);
    }

    #[test]
    fn test_query_builder_rate() {
        let q = QueryBuilder::default().metric("cpu").rate("5m").build();
        assert_eq!(q, "rate(cpu[5m])");
    }

    #[test]
    fn test_query_builder_combined() {
        let q = QueryBuilder::default()
            .metric("http_requests")
            .label("code", "200")
            .rate("1m")
            .build();
        assert_eq!(q, r#"rate(http_requests{code="200"}[1m])"#);
    }

    #[test]
    fn test_query_builder_avg_over_time() {
        let q = QueryBuilder::default()
            .metric("temperature")
            .avg_over_time("10m")
            .build();
        assert_eq!(q, "avg_over_time(temperature[10m])");
    }
}
