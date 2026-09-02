//! SQL row mirrors and domain types for `vala.forge_operation_state`.
//!
//! The private [`ForgeOperationStateSqlRow`] mirrors the exact column layout of
//! that table and nothing else — the projection is the sole Forge recovery
//! authority, so no read joins the `vala.audit_outbox` delivery table. The
//! public [`ForgeOperationStateRow`] is produced by explicit, fallible
//! conversion that validates every stored value: closed enum strings,
//! JSONB-to-detail decoding, phase/nullability invariants, and tenant/resource/
//! detail identity.

use chrono::{DateTime, Utc};
use std::str::FromStr;

use sqlx::types::Uuid;
use wyrd_spec::vala::api::{
    AuditDetail, ForgeIcebergRewritePhase, ForgeOrphanGcPhase, ForgeScribePromotionPhase,
    ForgeSnapshotExpirePhase,
};

use crate::SqlError;

/// Closed Forge operation families tracked by the state projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeOperationFamily {
    /// Scribe hot-object promotion operations (`forge.scribe_promotion.*`).
    ScribePromotion,
    /// Iceberg partition-rewrite operations (`forge.iceberg_rewrite.*`).
    IcebergRewrite,
    /// Snapshot-expiry operations (`forge.snapshot_expire.*`).
    SnapshotExpire,
    /// Orphan-GC operations (`forge.orphan_gc.*`).
    OrphanGc,
    /// Metadata-only manifest-rewrite operations (`forge.manifest_rewrite.*`).
    ManifestRewrite,
}

impl ForgeOperationFamily {
    /// Returns the SQL `family` string for this variant.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ScribePromotion => "scribe_promotion",
            Self::IcebergRewrite => "iceberg_rewrite",
            Self::SnapshotExpire => "snapshot_expire",
            Self::OrphanGc => "orphan_gc",
            Self::ManifestRewrite => "manifest_rewrite",
        }
    }

    /// Returns the expected `AuditDetail` kind string for this family.
    #[must_use]
    pub fn expected_detail_kind(&self) -> &'static str {
        match self {
            Self::ScribePromotion => "forge_scribe_promotion",
            Self::IcebergRewrite => "forge_iceberg_rewrite",
            Self::SnapshotExpire => "forge_snapshot_expire",
            Self::OrphanGc => "forge_orphan_gc",
            Self::ManifestRewrite => "forge_manifest_rewrite",
        }
    }

    /// Returns the expected audit operation prefix for this family.
    #[must_use]
    pub fn operation_prefix(&self) -> &'static str {
        match self {
            Self::ScribePromotion => "forge.scribe_promotion",
            Self::IcebergRewrite => "forge.iceberg_rewrite",
            Self::SnapshotExpire => "forge.snapshot_expire",
            Self::OrphanGc => "forge.orphan_gc",
            Self::ManifestRewrite => "forge.manifest_rewrite",
        }
    }
}

impl std::str::FromStr for ForgeOperationFamily {
    type Err = SqlError;

    /// Parses the SQL `family` string into a typed variant.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when `s` is not a known family.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "scribe_promotion" => Ok(Self::ScribePromotion),
            "iceberg_rewrite" => Ok(Self::IcebergRewrite),
            "snapshot_expire" => Ok(Self::SnapshotExpire),
            "orphan_gc" => Ok(Self::OrphanGc),
            "manifest_rewrite" => Ok(Self::ManifestRewrite),
            other => Err(SqlError::InvariantViolation {
                detail: format!("unknown forge operation family: {other}"),
            }),
        }
    }
}

/// Closed phases tracked in the operation state projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeOperationPhase {
    /// Operation is prepared and its evidence is in durable storage.
    Prepared,
    /// Operation was committed externally.
    Committed,
    /// Reconciliation recovered the external commit.
    Recovered,
    /// Operation was reset after a failed external commit (scribe_promotion
    /// and iceberg_rewrite only).
    Reset,
}

impl ForgeOperationPhase {
    /// Returns the SQL `phase` string for this variant.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Committed => "committed",
            Self::Recovered => "recovered",
            Self::Reset => "reset",
        }
    }
}

impl std::str::FromStr for ForgeOperationPhase {
    type Err = SqlError;

    /// Parses the SQL `phase` string into a typed variant.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when `s` is not a known phase.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prepared" => Ok(Self::Prepared),
            "committed" => Ok(Self::Committed),
            "recovered" => Ok(Self::Recovered),
            "reset" => Ok(Self::Reset),
            other => Err(SqlError::InvariantViolation {
                detail: format!("unknown forge operation phase: {other}"),
            }),
        }
    }
}

/// One validated row from the `vala.forge_operation_state` projection.
#[derive(Debug, Clone)]
pub struct ForgeOperationStateRow {
    /// Canonical tenant/table/resource identity.
    pub resource: String,
    /// Closed operation family.
    pub family: ForgeOperationFamily,
    /// Deterministic operation identifier shared with the audit detail.
    pub operation_id: Uuid,
    /// Current phase of the operation.
    pub phase: ForgeOperationPhase,
    /// Original prepared detail from the first Prepared audit row.
    pub prepared_detail: AuditDetail,
    /// Current detail — matches `prepared_detail` for Prepared rows, or
    /// carries the terminal detail for terminal rows.
    pub current_detail: AuditDetail,
    /// Audit sequence of the prepared event that opened this operation.
    pub prepared_audit_seq: i64,
    /// Audit sequence of the terminal event that closed this operation, if any.
    pub terminal_audit_seq: Option<i64>,
    /// Wall-clock time of the prepared event.
    pub prepared_at: DateTime<Utc>,
    /// Wall-clock time of the most recent update.
    pub updated_at: DateTime<Utc>,
}

/// A bounded page of open (Prepared) operations.
#[derive(Debug, Clone)]
pub struct OpenForgeOperationPage {
    /// At most `cap` validated Prepared operations.
    pub operations: Vec<ForgeOperationStateRow>,
    /// `true` when `cap + 1` state rows existed, meaning the caller should
    /// paginate or widen the resource/family scope.
    pub overflowed: bool,
}

/// Outcome of attempting to apply a transition to the operation state.
#[derive(Debug, Clone)]
pub enum ForgeOperationTransition {
    /// The transition was applied and the caller should consider it the
    /// authoritative result.
    Applied {
        /// The audit sequence that was (or will be) committed in the same
        /// transaction.
        audit_seq: i64,
    },
    /// The transition was already applied by a previous attempt; no audit or
    /// state change occurred.
    AlreadyApplied {
        /// The existing audit sequence that recorded this transition.
        audit_seq: i64,
    },
}

// ---------------------------------------------------------------------------
// Private SQL row mirror — maps the exact `forge_operation_state` columns.
// ---------------------------------------------------------------------------

/// Raw SQL row mirror for the `forge_operation_state` projection.
///
/// The projection is the sole Forge operation and recovery authority: it stores
/// the complete typed prepared and current details alongside the audit
/// sequences the transitions that wrote them produced. `vala.audit_outbox` is a
/// delivery table with its own retention lifecycle, so no read here joins it —
/// a delivered audit row that later ages out must not remove a worker's ability
/// to recover its own prepared operation.
///
/// This is the sole `sqlx::FromRow` decoder. Every decoded row goes through
/// an explicit [`TryInto<ForgeOperationStateRow>`] that validates stored
/// invariants before the public domain row is produced.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ForgeOperationStateSqlRow {
    /// Tenant isolation key (not exposed — RLS enforces the boundary).
    pub(crate) data_tenant_id: Uuid,
    /// Canonical tenant/table/resource identity.
    pub(crate) resource: String,
    /// SQL family string.
    pub(crate) family: String,
    /// Deterministic operation identifier.
    pub(crate) operation_id: Uuid,
    /// SQL phase string.
    pub(crate) phase: String,
    /// JSONB value of the prepared detail.
    pub(crate) prepared_detail: serde_json::Value,
    /// JSONB value of the current detail.
    pub(crate) current_detail: serde_json::Value,
    /// Audit sequence of the prepared event.
    pub(crate) prepared_audit_seq: i64,
    /// Audit sequence of the terminal event, if any.
    pub(crate) terminal_audit_seq: Option<i64>,
    /// Wall-clock time of the prepared event.
    pub(crate) prepared_at: DateTime<Utc>,
    /// Wall-clock time of the most recent update.
    pub(crate) updated_at: DateTime<Utc>,
}

impl TryFrom<ForgeOperationStateSqlRow> for ForgeOperationStateRow {
    /// Error returned when persisted state violates an invariant.
    type Error = SqlError;

    /// Converts a raw SQL row into a validated public domain row.
    ///
    /// Validation steps in order:
    /// 1. Parse closed `family` and `phase` strings.
    /// 2. Decode both JSONB columns to typed `AuditDetail`.
    /// 3. Validate tenant/resource/family/operation/phase identity in both
    ///    state details.
    /// 4. Enforce the terminal-sequence nullability check.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when any stored value fails
    /// validation.
    fn try_from(row: ForgeOperationStateSqlRow) -> Result<Self, Self::Error> {
        if row.data_tenant_id.is_nil() {
            return Err(SqlError::InvariantViolation {
                detail: "forge operation state has a nil data tenant ID".to_owned(),
            });
        }
        let family = ForgeOperationFamily::from_str(&row.family)?;
        let phase = ForgeOperationPhase::from_str(&row.phase)?;

        let prepared_detail = decode_detail_value(row.prepared_detail, "prepared_detail")?;
        let current_detail = decode_detail_value(row.current_detail, "current_detail")?;

        // Validate identity in both state details.
        validate_detail_identity(
            &prepared_detail,
            &row.resource,
            family,
            row.operation_id,
            ForgeOperationPhase::Prepared,
        )?;
        validate_detail_identity(
            &current_detail,
            &row.resource,
            family,
            row.operation_id,
            phase,
        )?;

        // Enforce nullability invariant.
        match phase {
            ForgeOperationPhase::Prepared => {
                if row.terminal_audit_seq.is_some() {
                    return Err(SqlError::InvariantViolation {
                        detail: "Prepared row has non-null terminal_audit_seq".to_owned(),
                    });
                }
            }
            _ => {
                if row.terminal_audit_seq.is_none() {
                    return Err(SqlError::InvariantViolation {
                        detail: format!("{:?} row has null terminal_audit_seq", phase),
                    });
                }
            }
        }

        Ok(Self {
            resource: row.resource,
            family,
            operation_id: row.operation_id,
            phase,
            prepared_detail,
            current_detail,
            prepared_audit_seq: row.prepared_audit_seq,
            terminal_audit_seq: row.terminal_audit_seq,
            prepared_at: row.prepared_at,
            updated_at: row.updated_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Private decoding helpers
// ---------------------------------------------------------------------------

/// Decodes a `serde_json::Value` stored in a JSONB column into a typed
/// [`AuditDetail`].
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] when the value cannot be parsed as
/// an `AuditDetail`.
fn decode_detail_value(
    value: serde_json::Value,
    stored_field: &'static str,
) -> Result<AuditDetail, SqlError> {
    serde_json::from_value(value).map_err(|e| SqlError::InvariantViolation {
        detail: format!("failed to decode {stored_field} audit detail: {e}"),
    })
}

/// Validates that a decoded [`AuditDetail`] carries the expected identity
/// values for the given resource, family, operation ID, and phase.
///
/// This is called for both `prepared_detail` and `current_detail` during row
/// conversion.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] when any identity field does not
/// match.
fn validate_detail_identity(
    detail: &AuditDetail,
    expected_resource: &str,
    expected_family: ForgeOperationFamily,
    expected_operation_id: Uuid,
    expected_phase: ForgeOperationPhase,
) -> Result<(), SqlError> {
    let (actual_operation_id, actual_group, actual_kind) = match detail {
        AuditDetail::ForgeScribePromotion {
            operation_id,
            group,
            ..
        } => (*operation_id, group.as_str(), "forge_scribe_promotion"),
        AuditDetail::ForgeIcebergRewrite {
            operation_id,
            group,
            ..
        } => (*operation_id, group.as_str(), "forge_iceberg_rewrite"),
        AuditDetail::ForgeSnapshotExpire {
            operation_id,
            group,
            ..
        } => (*operation_id, group.as_str(), "forge_snapshot_expire"),
        AuditDetail::ForgeOrphanGc {
            operation_id,
            group,
            ..
        } => (*operation_id, group.as_str(), "forge_orphan_gc"),
        AuditDetail::ForgeManifestRewrite {
            operation_id,
            group,
            ..
        } => (*operation_id, group.as_str(), "forge_manifest_rewrite"),
        _ => {
            return Err(SqlError::InvariantViolation {
                detail: "unexpected audit detail kind for forge operation".to_owned(),
            });
        }
    };

    let expected_kind = expected_family.expected_detail_kind();
    if actual_kind != expected_kind {
        return Err(SqlError::InvariantViolation {
            detail: format!("detail kind mismatch: expected {expected_kind}, got {actual_kind}"),
        });
    }

    if actual_group != expected_resource {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "detail group mismatch: expected {expected_resource}, got {actual_group}"
            ),
        });
    }

    if actual_operation_id != expected_operation_id {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "detail operation_id mismatch: expected {expected_operation_id}, got {actual_operation_id}"
            ),
        });
    }

    let expected_phase_name = expected_phase.as_str();
    let detail_phase_str = match detail {
        AuditDetail::ForgeScribePromotion { phase, .. } => match phase {
            ForgeScribePromotionPhase::Prepared => "prepared",
            ForgeScribePromotionPhase::Committed => "committed",
            ForgeScribePromotionPhase::Recovered => "recovered",
            ForgeScribePromotionPhase::Reset => "reset",
        },
        AuditDetail::ForgeIcebergRewrite { phase, .. } => match phase {
            ForgeIcebergRewritePhase::Prepared => "prepared",
            ForgeIcebergRewritePhase::Committed => "committed",
            ForgeIcebergRewritePhase::Recovered => "recovered",
            ForgeIcebergRewritePhase::Reset => "reset",
        },
        AuditDetail::ForgeSnapshotExpire { phase, .. } => match phase {
            ForgeSnapshotExpirePhase::Prepared => "prepared",
            ForgeSnapshotExpirePhase::Committed => "committed",
            ForgeSnapshotExpirePhase::Recovered => "recovered",
        },
        AuditDetail::ForgeOrphanGc { phase, .. } => match phase {
            ForgeOrphanGcPhase::Prepared => "prepared",
            ForgeOrphanGcPhase::Committed => "committed",
            ForgeOrphanGcPhase::Recovered => "recovered",
        },
        _ => {
            return Err(SqlError::InvariantViolation {
                detail: "unexpected audit detail kind for forge operation".to_owned(),
            });
        }
    };

    if expected_phase_name != detail_phase_str {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "detail phase mismatch: expected {expected_phase_name}, got {detail_phase_str}"
            ),
        });
    }

    Ok(())
}
