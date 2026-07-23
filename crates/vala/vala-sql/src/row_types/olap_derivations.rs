//! Row types for the cross-table derivation registry (`vala.olap_derivations`).

use sqlx::types::{Uuid, chrono};

/// One derivation registry row from `vala.olap_derivations`.
///
/// A derivation reads from a source table and writes into a target table. The
/// `watermark` is the last source commit position (`batch_id`) fully consumed;
/// it is an opaque 16-byte commit-position identity that matches
/// `vala.olap_commits.batch_id` and is never compared numerically.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DerivationRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Opaque 16-byte derivation identifier.
    pub derivation_uid: Vec<u8>,
    /// Opaque 16-byte source table identifier.
    pub source_table_uid: Vec<u8>,
    /// Opaque 16-byte target table identifier.
    pub target_table_uid: Vec<u8>,
    /// Last source commit position (`batch_id`) fully derived; `NULL` when
    /// nothing has been consumed yet.
    pub watermark: Option<Vec<u8>>,
    /// Source position captured at registration (the earliest pin); the pin
    /// resolves here when `watermark` is `NULL`.
    pub registered_watermark: Option<Vec<u8>>,
    /// Health / lifecycle state: `idle`, `deriving`, `failed`, or `degraded`.
    pub derivation_state: String,
    /// Human-readable identity (`namespace.name`).
    pub fqn: String,
    /// Wall-clock registration time.
    pub registered_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock time of the last successful watermark advance; `NULL` until
    /// the first advance.
    pub derived_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Wall-clock last-update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Freshness / lag view of a derivation, returned by `derivation_freshness`.
///
/// `lag_seconds` is `now() - derived_at` (seconds); it is `NULL` when the
/// derivation has never advanced its watermark (`derived_at IS NULL`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DerivationFreshnessRow {
    /// Opaque 16-byte derivation identifier.
    pub derivation_uid: Vec<u8>,
    /// Current watermark (last consumed source `batch_id`); `NULL` when nothing
    /// consumed yet.
    pub watermark: Option<Vec<u8>>,
    /// Health / lifecycle state.
    pub derivation_state: String,
    /// Wall-clock time of the last successful watermark advance.
    pub derived_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Seconds since the last advance (`now() - derived_at`); `NULL` when the
    /// derivation has never advanced.
    pub lag_seconds: Option<f64>,
}
