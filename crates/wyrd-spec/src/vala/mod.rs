//! Vala primitive types — drift, verification, records, transport, trace, bifrost,
//! and OLAP shapes.
//!
//! `vala` is Wyrd's drift-monitoring engine, agent-evaluation runtime, OTel
//! trace store (trace tables on Bifrost), and OLAP layer. `wyrd-spec::vala`
//! owns the typed contracts these surfaces exchange across HTTP, the Python
//! SDK, MCP, and Postgres control-plane / Iceberg object-store storage.
//! Runtime behavior lives in the `vala-*` crates.
//!
//! `wyrd-spec` ships only declarative artifacts — no IO, no async, no PyO3.
//! See AGENTS.md.

/// Public Bifrost wire contracts — table management, query, and ingest types.
pub mod api;
/// Closed scan-predicate/literal vocabulary and the v2 signed
/// assignment-authority digest.
pub mod assignment_authority;
/// Typed and redacted audit detail contracts.
pub mod audit_detail;
/// Drift observation records (traditional-ML drift measurements).
pub mod drift;
/// Public Bifrost error catalog for HTTP, MCP, and Python SDK boundaries.
pub mod error;
pub mod ids;
/// Log observation records (faithful OTel LogRecord).
pub mod logs;
/// Managed physical column names and reservation predicates.
pub mod managed_columns;
/// Metric observation records (full OTLP fidelity).
pub mod metrics;
/// Observation forward contract — envelope, closed kind taxonomy, and record
/// descriptor for the Vala ingest surface.
pub mod observation;
pub mod trace;

pub use audit_detail::{
    AuditDelegationStep, AuditDetail, AuditDetailValueError, BatchId, ForgeIcebergRewritePhase,
    ForgeManifestRewritePhase, ForgeOrphanGcPhase, ForgePromotedFile, ForgePromotedFileSetDigest,
    ForgeScribePromotionPhase, ForgeSnapshotExpirePhase, OracleTableProtectionPhase,
    RevocationReason, ScopeHash, StoragePath, audit_detail_canonical_json,
};
pub use error::{BifrostError, PhysicalLayoutField, PhysicalLayoutViolation, TableEditViolation};
pub use managed_columns::{
    ACT_PRINCIPAL_ID, CARD_UID, RUN_ID, SUB_PRINCIPAL_ID, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};
