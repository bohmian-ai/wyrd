//! `TraceSource` async trait plus in-memory impl.
//!
//! `fetch(trace_id, deadline) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable>`
//! shape lands in Commit 10 (`11-trace-and-agent-executors.md`); the server impl
//! is a follow-up.
