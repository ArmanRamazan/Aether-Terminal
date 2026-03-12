//! Prometheus PromQL consumer: queries a Prometheus HTTP API and converts results to TimeSeries.

mod types;

mod client;
mod query;

pub use client::PrometheusConsumer;
pub use query::QueryBuilder;
