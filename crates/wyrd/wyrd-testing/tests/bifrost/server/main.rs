//! Server-surface Bifrost journeys driven against a real `WyrdTestServer`.
// A journey future that awaits `BifrostCatalog::create_table` nests deeper
// than rustc's default 128-query layout depth; boxing the inner future does
// not help because drop-check still recurses through `Box<T>`.
#![recursion_limit = "256"]

mod audit_retention;
mod eval_verification;
mod observe_ingest;
mod owner_inspection;
mod query;
mod query_lifecycle;
mod verification_runtime;
