//! Active Prometheus `/metrics` scraping.
//!
//! - [`parser`] — Prometheus text exposition format parser
//! - [`scraper`] — HTTP scraper that fetches and parses targets

pub mod parser;
pub mod scraper;
