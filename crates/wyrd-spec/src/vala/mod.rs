//! Vala primitive types — drift, eval, records, transport, trace, bifrost,
//! and OLAP shapes.
//!
//! `vala` is Wyrd's drift-monitoring engine, agent-evaluation runtime, OTel
//! trace warehouse, and OLAP layer. `wyrd-spec::vala` owns the typed
//! contracts these surfaces exchange across HTTP, the Python SDK, MCP, and
//! Postgres / Delta storage. Runtime behavior lives in the `vala-*` crates.
//!
//! Per PR4.0 §0.1 framing 1, `wyrd-spec` ships only declarative artifacts —
//! no IO, no async, no PyO3. See
//! `architecture/v1/06-crates/wyrd-spec.md` and AGENTS.md §9.

pub mod eval;
