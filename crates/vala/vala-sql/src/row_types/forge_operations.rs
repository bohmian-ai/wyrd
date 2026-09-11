//! SQL row mirrors and domain types for `vala.forge_operation_state`.
//!
//! The private [`ForgeOperationStateSqlRow`] mirrors the exact column layout of
//! that table and nothing else — the projection is the sole Forge recovery
//! authority, and a Forge transition evaluates no principal permission, so no
//! read here joins audit state. The public [`ForgeOperationStateRow`] is
//! produced by explicit, fallible conversion that validates every stored value:
//! closed enum strings, JSONB-to-detail decoding, and tenant/resource/detail
//! identity.

use chrono::{DateTime, Utc};
use std::str::FromStr;

use sqlx::types::Uuid;
use wyrd_spec::vala::api::{
    AuditDetail, ForgeIcebergRewritePhase, ForgeOrphanGcPhase, ForgeScribePromotionPhase,
    ForgeSnapshotExpirePhase,
};

use crate::SqlError;
use crate::row_types::forge_tasks::ForgeTaskEvidence;

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
    /// Deterministic operation identifier shared with the typed detail.
    pub operation_id: Uuid,
    /// Current phase of the operation.
    pub phase: ForgeOperationPhase,
    /// Original prepared detail recorded by the first Prepared transition.
    pub prepared_detail: AuditDetail,
    /// Current detail — matches `prepared_detail` for Prepared rows, or
    /// carries the terminal detail for terminal rows.
    pub current_detail: AuditDetail,
    /// Wall-clock time of the prepared transition.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeOperationTransition {
    /// The transition was applied and the caller should consider it the
    /// authoritative result. It becomes durable when the caller commits.
    Applied,
    /// The transition was already applied by a previous attempt; no state
    /// change occurred.
    AlreadyApplied,
}

// ---------------------------------------------------------------------------
// Private SQL row mirror — maps the exact `forge_operation_state` columns.
// ---------------------------------------------------------------------------

/// Raw SQL row mirror for the `forge_operation_state` projection.
///
/// The projection is the sole Forge operation and recovery authority: it stores
/// the complete typed prepared and current details that every transition wrote.
/// A Forge maintenance transition evaluates no principal permission, so it is
/// lineage rather than audit and this projection joins no audit state at all.
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
    /// Wall-clock time of the prepared transition.
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
        // A snapshot-expiry Reset is internal: the public
        // `ForgeSnapshotExpirePhase` has no Reset variant, so the reset row
        // keeps its immutable Prepared detail as `current_detail` and records
        // the release only in the column phase and terminal sequence.
        let current_phase = if family == ForgeOperationFamily::SnapshotExpire
            && phase == ForgeOperationPhase::Reset
        {
            ForgeOperationPhase::Prepared
        } else {
            phase
        };
        validate_detail_identity(
            &current_detail,
            &row.resource,
            family,
            row.operation_id,
            current_phase,
        )?;

        Ok(Self {
            resource: row.resource,
            family,
            operation_id: row.operation_id,
            phase,
            prepared_detail,
            current_detail,
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

// ---------------------------------------------------------------------------
// Snapshot-expiration claims
// ---------------------------------------------------------------------------

/// The exact registered Bifrost table one expiration claim is bound to.
///
/// `table_uid` is the durable identity; the catalog/namespace/table payload and
/// the Iceberg `table_uuid` are carried so drift between the registry, the
/// maintenance authority, and the catalog is visible in the claim itself rather
/// than silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeClaimTable {
    /// Durable 16-byte Bifrost table UID.
    pub table_uid: [u8; 16],
    /// Catalog the table is registered in; always the Bifrost catalog.
    pub catalog_name: String,
    /// Logical namespace segment of the table identity.
    pub namespace_name: String,
    /// Final segment of the table identity.
    pub table_name: String,
    /// Exact Iceberg table UUID observed when the selection was made.
    pub table_uuid: Uuid,
}

/// One worker's fenced authority over a snapshot-expiration operation.
///
/// At preparation this is written into every claim row as immutable historical
/// evidence of who prepared the selection and under which table fence. At reset
/// and settlement the same shape describes the *current* mutation authority:
/// the task's unchanged `task_id`/`attempt_id`, its current `claimed_by` owner
/// with an unexpired claim, and that owner's own newly acquired live fence. A
/// successor is therefore never required to be the preparing worker or to reuse
/// the preparing fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeExpirationAuthority {
    /// Stable Forge task identity carrying the expiration.
    pub task_id: Uuid,
    /// Exact attempt generation, preserved across reconciliation takeover.
    pub attempt_id: Uuid,
    /// Worker acting on the task right now.
    pub worker_id: Uuid,
    /// Forge table lease namespace that worker holds.
    pub lease_key: String,
    /// Live fencing token of that lease.
    pub lease_fencing_token: i64,
}

/// One immutable claim row: exactly one unresolved snapshot of one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeSnapshotExpirationClaim {
    /// Canonical tenant/table resource identity shared with the operation.
    pub resource: String,
    /// Deterministic operation identifier the claim belongs to.
    pub operation_id: Uuid,
    /// Exact snapshot this claim reserves against reader widening.
    pub snapshot_id: i64,
    /// Identity the selection was prepared under.
    pub prepared_by: ForgeExpirationAuthority,
    /// Table the claim is bound to.
    pub table: ForgeClaimTable,
}

/// How one prepared snapshot-expiration operation settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeExpirationSettlement {
    /// The preparing worker proved the Iceberg commit itself.
    Committed,
    /// Reconciliation proved a predecessor's commit had been accepted.
    Recovered,
}

impl ForgeExpirationSettlement {
    /// Returns the operation phase this settlement writes.
    #[must_use]
    pub fn phase(self) -> ForgeOperationPhase {
        match self {
            Self::Committed => ForgeOperationPhase::Committed,
            Self::Recovered => ForgeOperationPhase::Recovered,
        }
    }
}

/// Outcome of applying the internal `Prepared -> Reset` expiration transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeExpirationResetOutcome {
    /// The reset applied: claims were deleted and planning demand advanced.
    Applied {
        /// Planning-demand generation the same transaction advanced to.
        demand_generation: i64,
    },
    /// A matching reset was already durable; nothing was written again.
    AlreadyApplied,
}

/// Everything one atomic snapshot-expiration preparation needs.
///
/// The request bundles the mutation authority, the table the claims bind to,
/// the Prepared task evidence, and the typed operation transition so the caller
/// cannot present a partial preparation to the single transaction that applies
/// it.
pub struct ForgeExpirationPreparation<'request> {
    /// Live task/attempt/worker/lease identity preparing the selection.
    pub authority: &'request ForgeExpirationAuthority,
    /// Registered table the claims and the maintenance authority row name.
    pub table: &'request ForgeClaimTable,
    /// Prepared attempt evidence recorded on the task in the same transaction.
    pub evidence: &'request ForgeTaskEvidence,
    /// Prepared `forge.snapshot_expire.prepared` operation name.
    pub operation: &'request str,
    /// Typed Prepared selection detail recorded as the operation's lineage.
    pub detail: &'request AuditDetail,
}

/// Everything one atomic snapshot-expiration settlement needs.
pub struct ForgeExpirationSettlementRequest<'request> {
    /// Current mutation authority, which need not be the preparing worker.
    pub authority: &'request ForgeExpirationAuthority,
    /// Registered table the resolved claims are bound to.
    pub table: &'request ForgeClaimTable,
    /// Whether this attempt proved the commit or recovered a predecessor's.
    pub settlement: ForgeExpirationSettlement,
    /// Final attempt evidence, including the exact cleanup candidate names.
    pub evidence: &'request ForgeTaskEvidence,
    /// Terminal `forge.snapshot_expire.{committed,recovered}` operation name.
    pub operation: &'request str,
    /// Typed terminal detail recorded as the operation's lineage.
    pub detail: &'request AuditDetail,
}

/// Everything one atomic snapshot-expiration reset needs.
pub struct ForgeExpirationResetRequest<'request> {
    /// Current mutation authority releasing the unproven selection.
    pub authority: &'request ForgeExpirationAuthority,
    /// Registered table whose claims are released.
    pub table: &'request ForgeClaimTable,
    /// Immutable Prepared detail the released operation was opened with. The
    /// public snapshot-expiry phase enum has no Reset variant, so the reset
    /// carries the Prepared detail unchanged and records the release only in
    /// the state row's `reset` phase.
    pub detail: &'request AuditDetail,
}

/// Exact column mirror of one `vala.forge_snapshot_expiration_claims` row.
///
/// The mirror is private to conversion: [`ForgeSnapshotExpirationClaim`] is the
/// public shape, and the conversion below is where the stored 16-byte table UID
/// is proven to be exactly that.
#[derive(sqlx::FromRow)]
pub(crate) struct ForgeSnapshotExpirationClaimSqlRow {
    /// Canonical tenant/table resource identity shared with the operation.
    resource: String,
    /// Deterministic operation identifier the claim belongs to.
    operation_id: Uuid,
    /// Exact reserved snapshot.
    snapshot_id: i64,
    /// Task the selection was prepared under.
    task_id: Uuid,
    /// Attempt the selection was prepared under.
    attempt_id: Uuid,
    /// Worker that prepared the selection.
    worker_id: Uuid,
    /// Table lease that worker held when preparing.
    lease_key: String,
    /// Fencing token of that lease.
    lease_fencing_token: i64,
    /// Durable table UID the claim is bound to.
    table_uid: Vec<u8>,
    /// Exact catalog wire name.
    catalog_name: String,
    /// Logical Bifrost namespace.
    namespace_name: String,
    /// Physical table name.
    table_name: String,
    /// Iceberg table UUID observed at preparation.
    table_uuid: Uuid,
}

impl TryFrom<ForgeSnapshotExpirationClaimSqlRow> for ForgeSnapshotExpirationClaim {
    /// Error returned when a stored claim violates its column invariants.
    type Error = SqlError;

    /// Converts one stored claim row, proving the table UID is 16 bytes.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when `table_uid` is not exactly
    /// 16 bytes, which the table's own CHECK also forbids.
    fn try_from(row: ForgeSnapshotExpirationClaimSqlRow) -> Result<Self, Self::Error> {
        let table_uid: [u8; 16] =
            row.table_uid
                .try_into()
                .map_err(|_| SqlError::InvariantViolation {
                    detail: "snapshot expiration claim table UID is not 16 bytes".to_owned(),
                })?;
        Ok(Self {
            resource: row.resource,
            operation_id: row.operation_id,
            snapshot_id: row.snapshot_id,
            prepared_by: ForgeExpirationAuthority {
                task_id: row.task_id,
                attempt_id: row.attempt_id,
                worker_id: row.worker_id,
                lease_key: row.lease_key,
                lease_fencing_token: row.lease_fencing_token,
            },
            table: ForgeClaimTable {
                table_uid,
                catalog_name: row.catalog_name,
                namespace_name: row.namespace_name,
                table_name: row.table_name,
                table_uuid: row.table_uuid,
            },
        })
    }
}
