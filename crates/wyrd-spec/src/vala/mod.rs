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
/// Typed and redacted audit detail contracts.
pub mod audit_detail;
/// Observation correlation: the code axis carried on a run and the reserved
/// correlation column names.
pub mod correlation;
/// Dev-capture records (`agent_traces`).
pub mod dev;
/// Drift observation records (traditional-ML drift measurements).
pub mod drift;
/// Public Bifrost error catalog for HTTP, MCP, and Python SDK boundaries.
pub mod error;
pub mod eval;
pub mod ids;
/// Log observation records (faithful OTel LogRecord).
pub mod logs;
/// Metric observation records (full OTLP fidelity).
pub mod metrics;
/// Observation forward contract — envelope, closed kind taxonomy, and record
/// descriptor for the Vala ingest surface.
pub mod observation;
/// Reserved system column names and the [`SystemColumnSet`] descriptor.
pub mod system_columns;
pub mod trace;

pub use audit_detail::{
    AuditDetail, AuditDetailValueError, BatchId, ForgeCompactionPhase, ScopeHash, StoragePath,
    audit_detail_canonical_json,
};
pub use correlation::{CorrelationColumns, CorrelationContext};
pub use error::BifrostError;
pub use system_columns::{
    CARD_REF, CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RESERVED_CORRELATION_COLUMNS,
    RESERVED_SYSTEM_COLUMNS, RUN_ID, SystemColumnSet, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, is_reserved_correlation_column, is_reserved_system_column,
};
