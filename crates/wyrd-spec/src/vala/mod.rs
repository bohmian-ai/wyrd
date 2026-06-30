//! Vala primitive types — drift, eval, records, transport, trace, bifrost,
//! and OLAP shapes.
//!
//! `vala` is Wyrd's drift-monitoring engine, agent-evaluation runtime, OTel
//! trace store (trace tables on Bifrost), and OLAP layer. `wyrd-spec::vala`
//! owns the typed contracts these surfaces exchange across HTTP, the Python
//! SDK, MCP, and Postgres control-plane / Iceberg object-store storage.
//! Runtime behavior lives in the `vala-*` crates.
//!
//! Per PR4.0 §0.1 framing 1, `wyrd-spec` ships only declarative artifacts —
//! no IO, no async, no PyO3. See
//! `architecture/v1/06-crates/wyrd-spec.md` and AGENTS.md §9.

/// Public Bifrost wire contracts — table management, query, and ingest types.
pub mod api;
/// Observation correlation: the code axis carried on a run and the reserved
/// correlation column names.
pub mod correlation;
/// Public Bifrost error catalog for HTTP, MCP, and Python SDK boundaries.
pub mod error;
pub mod eval;
pub mod ids;
/// Observation forward contract — envelope, closed kind taxonomy, and record
/// descriptor for the Vala ingest surface.
pub mod observation;
/// Reserved system column names and the [`SystemColumnSet`] descriptor.
pub mod system_columns;
pub mod trace;

pub use correlation::{CorrelationColumns, CorrelationContext};
pub use error::BifrostError;
pub use system_columns::{
    DATA_TENANT_ID, RESERVED_SYSTEM_COLUMNS, SystemColumnSet, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, is_reserved_system_column,
};
