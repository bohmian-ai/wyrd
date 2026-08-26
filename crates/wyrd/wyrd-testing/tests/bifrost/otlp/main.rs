//! OTLP export journey binary.
//!
//! Surface-scoped test target carved out of the former monolithic `integration`
//! binary. It aggregates the OTLP trace/metrics/logs export journeys, the
//! mixed-batch and negative journeys, and the Bifrost ingest-runtime journey that
//! shares the OTLP trace fixtures. `support` stays in this binary because
//! the sibling OTLP modules reference it via `crate::support`; keeping them
//! together preserves that path. Modules point at unchanged files via `#[path]`,
//! so every test name and `#[ignore]` gate is retained.

mod logs_export;
mod metrics_export;
mod mixed_batch;
mod negative;
mod support;
mod trace_export;
mod trace_export_http;
