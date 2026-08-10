//! OTLP export journey binary.
//!
//! Surface-scoped test target carved out of the former monolithic `integration`
//! binary (T54). It aggregates the OTLP trace/metrics/logs export journeys, the
//! mixed-batch and negative journeys, and the Bifrost ingest-runtime journey that
//! shares the OTLP trace fixtures. `otlp_support` stays in this binary because
//! the sibling OTLP modules reference it via `crate::otlp_support`; keeping them
//! together preserves that path. Modules point at unchanged files via `#[path]`,
//! so every test name and `#[ignore]` gate is retained.

#[path = "otlp_logs_export.rs"]
mod otlp_logs_export;
#[path = "otlp_metrics_export.rs"]
mod otlp_metrics_export;
#[path = "otlp_mixed_batch_journey.rs"]
mod otlp_mixed_batch_journey;
#[path = "otlp_negative_journeys.rs"]
mod otlp_negative_journeys;
#[path = "otlp_support.rs"]
mod otlp_support;
#[path = "otlp_trace_export.rs"]
mod otlp_trace_export;
#[path = "otlp_trace_export_http.rs"]
mod otlp_trace_export_http;
