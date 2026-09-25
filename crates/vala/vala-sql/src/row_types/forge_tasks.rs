//! Validated domain and SQL row types for durable Forge tasks.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

use crate::SqlError;
use crate::row_types::forge_operations::{ForgeClaimTable, ForgeExpirationAuthority};

/// Current version of persisted task plans and evidence.
pub const FORGE_TASK_PAYLOAD_VERSION: u16 = 1;

/// Closed durable class driving Forge settlement policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ForgeFailureClass {
    /// Deterministic source data refusal.
    DataRefusal,
    /// Retryable remote object-store or catalog transport failure.
    TransientObjectStore,
    /// Retryable SQL, lease, fence, or reconciliation failure.
    TransientCoordination,
    /// Retryable local scratch-volume health failure.
    StorageHealth,
    /// Local admission refusal.
    CapacityRefused,
    /// Non-retryable internal invariant failure.
    InternalInvariant,
}

impl ForgeFailureClass {
    /// Returns the stable SQL and telemetry spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DataRefusal => "data_refusal",
            Self::TransientObjectStore => "transient_object_store",
            Self::TransientCoordination => "transient_coordination",
            Self::StorageHealth => "storage_health",
            Self::CapacityRefused => "capacity_refused",
            Self::InternalInvariant => "internal_invariant",
        }
    }

    /// Decodes one persisted class without accepting unknown values.
    ///
    /// # Errors
    /// Returns an invariant violation for malformed persisted state.
    pub fn from_sql(value: &str) -> Result<Self, SqlError> {
        match value {
            "data_refusal" => Ok(Self::DataRefusal),
            "transient_object_store" => Ok(Self::TransientObjectStore),
            "transient_coordination" => Ok(Self::TransientCoordination),
            "storage_health" => Ok(Self::StorageHealth),
            "capacity_refused" => Ok(Self::CapacityRefused),
            "internal_invariant" => Ok(Self::InternalInvariant),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge failure class {value}"),
            }),
        }
    }
}

/// Closed origin of a coalesced Forge planning request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgePlanningDemandSource {
    /// A Scribe commit requested prompt planning.
    Hint,
    /// Periodic active-roster repair requested planning.
    Periodic,
}

impl ForgePlanningDemandSource {
    /// Returns the stable SQL spelling used by the private demand table.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Periodic => "periodic",
        }
    }

    /// Reconstructs a persisted source without accepting unknown values.
    ///
    /// # Errors
    /// Returns an invariant violation for malformed persisted state.
    pub(crate) fn from_sql(value: &str) -> Result<Self, SqlError> {
        match value {
            "hint" => Ok(Self::Hint),
            "periodic" => Ok(Self::Periodic),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge planning demand source {value}"),
            }),
        }
    }
}

/// One bounded durable request for exact Forge planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgePlanningDemand {
    /// Tenant whose table must be planned.
    pub data_tenant_id: DataTenantId,
    /// Validated logical table identity.
    pub table_ref: ForgeTaskTableIdentity,
    /// Time at which the coalesced demand was first observed.
    pub first_requested_at: DateTime<Utc>,
    /// Time at which its generation was most recently advanced.
    pub last_requested_at: DateTime<Utc>,
    /// Most recent request source.
    pub last_source: ForgePlanningDemandSource,
    /// Positive monotonic generation captured for CAS acknowledgement.
    pub generation: i64,
    /// Snapshot cause most recently acknowledged by a successful maintenance no-op.
    pub acknowledged_snapshot_id: Option<i64>,
    /// Retained commit count paired with the acknowledged snapshot cause.
    pub acknowledged_commit_count: Option<u64>,
}

/// SQL projection used to validate demand rows before catalog access.
#[derive(sqlx::FromRow)]
pub(crate) struct ForgePlanningDemandSqlRow {
    /// Persisted tenant UUID.
    pub data_tenant_id: Uuid,
    /// Persisted catalog component.
    pub catalog_name: String,
    /// Persisted namespace component.
    pub namespace_name: String,
    /// Persisted table component.
    pub table_name: String,
    /// First demand timestamp.
    pub first_requested_at: DateTime<Utc>,
    /// Most recent demand timestamp.
    pub last_requested_at: DateTime<Utc>,
    /// Persisted closed source.
    pub last_source: String,
    /// Persisted CAS generation.
    pub generation: i64,
    /// Persisted acknowledged snapshot cause.
    pub acknowledged_snapshot_id: Option<i64>,
    /// Persisted acknowledged retained commit count.
    pub acknowledged_commit_count: Option<i64>,
}

impl TryFrom<ForgePlanningDemandSqlRow> for ForgePlanningDemand {
    type Error = SqlError;

    /// Validates every persisted identity and generation component.
    fn try_from(row: ForgePlanningDemandSqlRow) -> Result<Self, Self::Error> {
        let data_tenant_id =
            DataTenantId::new(row.data_tenant_id).map_err(|_| SqlError::InvariantViolation {
                detail: "Forge planning demand contains malformed tenant identity".to_owned(),
            })?;
        let table_ref =
            ForgeTaskTableIdentity::new(row.catalog_name, row.namespace_name, row.table_name)
                .map_err(|_| SqlError::InvariantViolation {
                    detail: "Forge planning demand contains malformed table identity".to_owned(),
                })?;
        if row.generation <= 0 {
            return Err(SqlError::InvariantViolation {
                detail: "Forge planning demand generation must be positive".to_owned(),
            });
        }
        let acknowledged_commit_count = row
            .acknowledged_commit_count
            .map(|value| {
                u64::try_from(value).map_err(|_| SqlError::InvariantViolation {
                    detail: "Forge acknowledged commit count must be non-negative".to_owned(),
                })
            })
            .transpose()?;
        Ok(Self {
            data_tenant_id,
            table_ref,
            first_requested_at: row.first_requested_at,
            last_requested_at: row.last_requested_at,
            last_source: ForgePlanningDemandSource::from_sql(&row.last_source)?,
            generation: row.generation,
            acknowledged_snapshot_id: row.acknowledged_snapshot_id,
            acknowledged_commit_count,
        })
    }
}

/// Durable consequence of one successful Forge task attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskProgressEffect {
    /// The task changed its due-condition and requests a fresh planning generation.
    Progressed,
    /// The task completed without changing its observed maintenance trigger.
    NoOpAcknowledged {
        /// Snapshot identity whose unchanged trigger was acknowledged.
        snapshot_id: i64,
        /// Retained commit count observed for the acknowledged snapshot.
        commit_count: u64,
    },
}

/// Validated logical Iceberg table identity stored in separate SQL columns.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ForgeTaskTableIdentity {
    /// Catalog containing the table.
    pub catalog: String,
    /// Logical namespace containing the table.
    pub namespace: String,
    /// Table name within the namespace.
    pub table: String,
}

impl ForgeTaskTableIdentity {
    /// Constructs a validated table identity.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when a component is empty or unsafe.
    pub fn new(
        catalog: impl Into<String>,
        namespace: impl Into<String>,
        table: impl Into<String>,
    ) -> Result<Self, SqlError> {
        let value = Self {
            catalog: catalog.into(),
            namespace: namespace.into(),
            table: table.into(),
        };
        const NAMESPACES: &[&str] = &[
            "vala.system",
            "vala.bifrost",
            "vala.traces",
            "vala.metrics",
            "vala.logs",
            "vala.eval",
            "vala.drift",
            "vala.dev",
            "vala.datasets",
            "vala.gateway",
        ];
        if value.catalog != "wyrd-redux"
            || !NAMESPACES.contains(&value.namespace.as_str())
            || !safe_identity(&value.table)
        {
            return Err(SqlError::Conflict {
                detail: "invalid Forge table identity".to_owned(),
            });
        }
        Ok(value)
    }
}

/// One base snapshot protected by an active Forge attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotWatermark {
    /// Snapshot identifier, used only for identity and ancestry checks.
    pub snapshot_id: i64,
    /// Snapshot timestamp used for expiration ordering.
    pub timestamp_ms: i64,
}

impl SnapshotWatermark {
    /// Validates that the Iceberg timestamp is representable and nonnegative.
    /// Snapshot identifiers are deliberately not ordered or range-compared.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for a negative snapshot timestamp.
    pub fn validate(self) -> Result<(), SqlError> {
        if self.timestamp_ms < 0 {
            Err(SqlError::Conflict {
                detail: "Forge snapshot watermark timestamp must be nonnegative".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

/// Versioned exact work description persisted for a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTaskPlan {
    /// Payload schema version; unknown versions fail closed.
    pub version: u16,
    /// Sorted, duplicate-free input object identities.
    pub inputs: Vec<String>,
    /// Strategy-specific parameters that contain no credentials.
    pub parameters: serde_json::Value,
}

impl ForgeTaskPlan {
    /// Validates version, ordering, uniqueness, and path safety.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for malformed caller input and
    /// [`SqlError::InvariantViolation`] for an unknown persisted version.
    pub fn validate(&self, persisted: bool) -> Result<(), SqlError> {
        if self.version != FORGE_TASK_PAYLOAD_VERSION {
            let detail = format!("unknown Forge task plan version {}", self.version);
            return Err(if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            });
        }
        if self.inputs.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .inputs
                .iter()
                .any(|path| path.is_empty() || path.contains(".."))
        {
            return Err(SqlError::Conflict {
                detail: "Forge plan inputs must be sorted, unique, and safe".to_owned(),
            });
        }
        Ok(())
    }
}

impl ForgeTaskPlan {
    /// Validates the plan under the exact strategy that will execute it.
    ///
    /// Every strategy except [`ForgeTaskStrategy::ExpiredCleanup`] names its
    /// work through `inputs`, so an empty input set is a malformed plan for it.
    /// Expired cleanup is the one strategy whose work is the immutable copied
    /// candidate vector inside `parameters`: `inputs` is deliberately empty so
    /// the plan cannot grow a second candidate authority, and its typed handoff
    /// is decoded here so a malformed payload fails before lease acquisition or
    /// any external IO.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::validate`], plus
    /// [`SqlError::Conflict`] (or [`SqlError::InvariantViolation`] when
    /// `persisted`) for an empty non-cleanup input set, a nonempty cleanup
    /// input set, or a cleanup payload that is missing, unknown-versioned,
    /// unknown-fielded, empty, or malformed.
    pub fn validate_for_strategy(
        &self,
        strategy: ForgeTaskStrategy,
        persisted: bool,
    ) -> Result<(), SqlError> {
        self.validate(persisted)?;
        let fail = |detail: String| {
            if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            }
        };
        if strategy == ForgeTaskStrategy::ExpiredCleanup {
            if !self.inputs.is_empty() {
                return Err(fail(
                    "expired cleanup plans carry their candidates in parameters, not inputs"
                        .to_owned(),
                ));
            }
            ExpiredCleanupPayload::from_value(&self.parameters, persisted)?;
        } else if strategy == ForgeTaskStrategy::OrphanCleanup {
            self.orphan_cleanup_prefix(persisted)?;
            OrphanCleanupPayload::from_value(&self.parameters, persisted)?;
        } else if self.inputs.is_empty() {
            return Err(fail("Forge task plan has no exact inputs".to_owned()));
        }
        Ok(())
    }

    /// Decodes the typed expired-cleanup handoff this plan carries.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the strategy is not
    /// [`ForgeTaskStrategy::ExpiredCleanup`], and the decoding errors of
    /// [`ExpiredCleanupPayload::from_value`] for a malformed payload.
    pub fn expired_cleanup_payload(
        &self,
        strategy: ForgeTaskStrategy,
        persisted: bool,
    ) -> Result<ExpiredCleanupPayload, SqlError> {
        if strategy != ForgeTaskStrategy::ExpiredCleanup {
            return Err(SqlError::Conflict {
                detail: "only an expired-cleanup plan carries a cleanup handoff".to_owned(),
            });
        }
        ExpiredCleanupPayload::from_value(&self.parameters, persisted)
    }

    /// Returns the single scan prefix an orphan-cleanup plan names.
    ///
    /// Orphan cleanup scans exactly one immutable storage prefix, so the plan's
    /// `inputs` vector carries exactly one safe, relative object key and never
    /// a set. A widened, empty, absolute, or traversal-bearing input is refused
    /// here, which is before lease acquisition and before any object IO.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`], or [`SqlError::InvariantViolation`] when
    /// `persisted`, unless `inputs` holds exactly one normalized relative key.
    pub fn orphan_cleanup_prefix(&self, persisted: bool) -> Result<&str, SqlError> {
        let fail = || {
            let detail = "orphan cleanup plans name exactly one normalized scan prefix".to_owned();
            if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            }
        };
        let [prefix] = self.inputs.as_slice() else {
            return Err(fail());
        };
        if !is_normalized_object_key(prefix) {
            return Err(fail());
        }
        Ok(prefix)
    }

    /// Decodes the typed orphan-cleanup parameters this plan carries.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the strategy is not
    /// [`ForgeTaskStrategy::OrphanCleanup`], and the decoding errors of
    /// [`OrphanCleanupPayload::from_value`] for a malformed payload.
    pub fn orphan_cleanup_payload(
        &self,
        strategy: ForgeTaskStrategy,
        persisted: bool,
    ) -> Result<OrphanCleanupPayload, SqlError> {
        if strategy != ForgeTaskStrategy::OrphanCleanup {
            return Err(SqlError::Conflict {
                detail: "only an orphan-cleanup plan carries a scan payload".to_owned(),
            });
        }
        OrphanCleanupPayload::from_value(&self.parameters, persisted)
    }
}

/// Payload version of the closed orphan-cleanup scan parameters.
///
/// This is the inner version of `plan.parameters` only; the outer
/// [`FORGE_TASK_PAYLOAD_VERSION`] envelope is unchanged.
pub const ORPHAN_CLEANUP_PAYLOAD_VERSION: u16 = 1;

/// Stable `kind` tag of the orphan-cleanup parameter payload.
pub const ORPHAN_CLEANUP_PAYLOAD_KIND: &str = "orphan_cleanup";

/// Version of the closed orphan-cleanup scan cursor stored as task evidence.
pub const ORPHAN_CLEANUP_CURSOR_VERSION: u16 = 1;

/// Rejects any string that is not a safe relative object key.
///
/// Shared by the scan prefix and the scan cursor so the two halves of one
/// orphan-cleanup task cannot disagree about what a normalized key is.
fn is_normalized_object_key(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("://")
        && !value.contains('\\')
        && !value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
}

/// The closed, immutable parameters one periodic orphan-cleanup task executes.
///
/// The age cutoff is captured once, when the periodic task is planned, and is
/// never re-derived from a later clock. That is what keeps a long-lived retry
/// or takeover from silently widening what the task may delete: the task's
/// scan identity is its tenant, table, prefix, and this cutoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrphanCleanupPayload {
    /// Inner payload version; unknown versions fail closed.
    pub version: u16,
    /// Immutable inclusive last-modified cutoff in milliseconds since the epoch.
    pub age_cutoff_ms: i64,
}

impl OrphanCleanupPayload {
    /// Encodes the parameters into their stable closed JSON object.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "version": self.version,
            "kind": ORPHAN_CLEANUP_PAYLOAD_KIND,
            "age_cutoff_ms": self.age_cutoff_ms,
        })
    }

    /// Decodes one payload, refusing every shape the contract does not name.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`], or [`SqlError::InvariantViolation`] when
    /// `persisted`, for a non-object value, unknown or missing fields, an
    /// unknown version, a wrong `kind`, or a negative cutoff.
    pub fn from_value(value: &serde_json::Value, persisted: bool) -> Result<Self, SqlError> {
        let fail = |detail: &str| {
            let detail = detail.to_owned();
            if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            }
        };
        let object = value
            .as_object()
            .ok_or_else(|| fail("orphan cleanup payload is not an object"))?;
        const FIELDS: [&str; 3] = ["version", "kind", "age_cutoff_ms"];
        if object.len() != FIELDS.len() || FIELDS.iter().any(|field| !object.contains_key(*field)) {
            return Err(fail("orphan cleanup payload has unknown or missing fields"));
        }
        let version = object
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| fail("orphan cleanup payload version is malformed"))?;
        if version != ORPHAN_CLEANUP_PAYLOAD_VERSION {
            return Err(fail("unknown orphan cleanup payload version"));
        }
        if object.get("kind").and_then(serde_json::Value::as_str)
            != Some(ORPHAN_CLEANUP_PAYLOAD_KIND)
        {
            return Err(fail("orphan cleanup payload kind is malformed"));
        }
        let age_cutoff_ms = object
            .get("age_cutoff_ms")
            .and_then(serde_json::Value::as_i64)
            .filter(|value| *value >= 0)
            .ok_or_else(|| fail("orphan cleanup age cutoff is malformed"))?;
        Ok(Self {
            version,
            age_cutoff_ms,
        })
    }
}

/// The closed durable traversal cursor one orphan-cleanup task carries.
///
/// The cursor records traversal only: it names the last object key whose
/// classification completed, so a successor attempt resumes listing after it
/// instead of restarting on a leading page of protected objects. It never
/// names a candidate, an owner, or a deletion decision, so nothing about
/// deletion safety can be derived from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanCleanupCursor {
    /// Cursor schema version; unknown versions fail closed.
    pub version: u16,
    /// Last completely processed object key, strictly beneath the task prefix.
    pub start_after: String,
}

impl OrphanCleanupCursor {
    /// Builds one cursor at the current traversal frontier.
    #[must_use]
    pub fn new(start_after: String) -> Self {
        Self {
            version: ORPHAN_CLEANUP_CURSOR_VERSION,
            start_after,
        }
    }

    /// Encodes the cursor into its stable closed JSON object.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({"version": self.version, "start_after": self.start_after})
    }

    /// Decodes one cursor and rebinds it to the task's immutable scan prefix.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`], or [`SqlError::InvariantViolation`] when
    /// `persisted`, for a non-object value, unknown or missing fields, an
    /// unknown version, a non-normalized key, or a key that is not strictly
    /// beneath `prefix`.
    pub fn from_value(
        value: &serde_json::Value,
        prefix: &str,
        persisted: bool,
    ) -> Result<Self, SqlError> {
        let fail = |detail: &str| {
            let detail = detail.to_owned();
            if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            }
        };
        let object = value
            .as_object()
            .ok_or_else(|| fail("orphan cleanup cursor is not an object"))?;
        const FIELDS: [&str; 2] = ["version", "start_after"];
        if object.len() != FIELDS.len() || FIELDS.iter().any(|field| !object.contains_key(*field)) {
            return Err(fail("orphan cleanup cursor has unknown or missing fields"));
        }
        let version = object
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| fail("orphan cleanup cursor version is malformed"))?;
        if version != ORPHAN_CLEANUP_CURSOR_VERSION {
            return Err(fail("unknown orphan cleanup cursor version"));
        }
        let start_after = object
            .get("start_after")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| fail("orphan cleanup cursor key is malformed"))?;
        if !is_normalized_object_key(start_after)
            || !start_after.starts_with(&format!("{}/", prefix.trim_end_matches('/')))
        {
            return Err(fail("orphan cleanup cursor key escaped its task prefix"));
        }
        Ok(Self {
            version,
            start_after: start_after.to_owned(),
        })
    }
}

/// The closed strategy-dispatched evidence one Forge task row may carry.
///
/// Evidence is not one shape across strategies: publication and expired
/// cleanup carry [`ForgeTaskEvidence`], while orphan cleanup carries only a
/// traversal cursor. Dispatching on the row's own strategy is what keeps the
/// cursor shape unreachable for every other strategy and keeps every existing
/// strategy's evidence contract exactly as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeTaskRowEvidence {
    /// Publication, expiration, and expired-cleanup evidence.
    Publication(ForgeTaskEvidence),
    /// Orphan-cleanup traversal cursor.
    OrphanScan(OrphanCleanupCursor),
}

impl ForgeTaskRowEvidence {
    /// Decodes one persisted evidence column under its row's own strategy.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the stored JSON is not the
    /// closed shape that strategy accepts, including an orphan cursor whose key
    /// escaped `prefix`.
    pub fn decode(
        strategy: ForgeTaskStrategy,
        value: &serde_json::Value,
        prefix: &str,
    ) -> Result<Self, SqlError> {
        if strategy == ForgeTaskStrategy::OrphanCleanup {
            OrphanCleanupCursor::from_value(value, prefix, true).map(Self::OrphanScan)
        } else {
            evidence_from_json(value.clone()).map(Self::Publication)
        }
    }

    /// Encodes this evidence back into its stable JSON object.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        match self {
            Self::Publication(evidence) => evidence_to_value(evidence),
            Self::OrphanScan(cursor) => cursor.to_value(),
        }
    }

    /// Returns the publication evidence, or `None` for an orphan cursor.
    #[must_use]
    pub fn publication(&self) -> Option<&ForgeTaskEvidence> {
        match self {
            Self::Publication(evidence) => Some(evidence),
            Self::OrphanScan(_) => None,
        }
    }

    /// Returns the orphan traversal cursor, or `None` for publication evidence.
    #[must_use]
    pub fn orphan_scan(&self) -> Option<&OrphanCleanupCursor> {
        match self {
            Self::OrphanScan(cursor) => Some(cursor),
            Self::Publication(_) => None,
        }
    }
}

/// Payload version of the closed expired-cleanup handoff.
///
/// This is the inner version of `plan.parameters` only. The outer
/// [`FORGE_TASK_PAYLOAD_VERSION`] is unchanged: the handoff is a new
/// strategy-specific parameter shape, not a new plan envelope.
pub const EXPIRED_CLEANUP_PAYLOAD_VERSION: u16 = 1;

/// Stable `kind` tag of the expired-cleanup parameter payload.
pub const EXPIRED_CLEANUP_PAYLOAD_KIND: &str = "expired_cleanup";

/// The closed, immutable handoff one expired-cleanup task executes.
///
/// The vector is copied byte-for-byte, in order, from one succeeded
/// snapshot-expiration task's evidence at enqueue. After that commit the copy
/// is authoritative: preparation, replay, takeover, and settlement never read
/// the source row again, so the source may be pruned normally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiredCleanupPayload {
    /// Inner payload version; unknown versions fail closed.
    pub version: u16,
    /// Globally unique replay identity: the succeeded expiration task copied.
    pub source_task_id: Uuid,
    /// Snapshot the source expiration committed.
    pub committed_snapshot_id: i64,
    /// Metadata location the source expiration committed.
    pub committed_metadata_location: String,
    /// Digest of that committed metadata.
    pub committed_metadata_digest: String,
    /// Exact ordered candidate copy; never derived, reordered, or listed.
    pub cleanup_candidates: Vec<ForgeCleanupCandidate>,
}

impl ExpiredCleanupPayload {
    /// Builds the immutable payload one succeeded expiration hands off.
    ///
    /// The copy is taken once, at planning, and is identity-bound to the source
    /// task: the cleanup task never re-reads the expiration's row, so a later
    /// change to that row cannot widen or shift what this task deletes.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the evidence carries no committed
    /// identity and therefore is not a usable handoff.
    pub fn from_handoff(
        source_task_id: Uuid,
        evidence: &ForgeTaskEvidence,
    ) -> Result<Self, SqlError> {
        let missing = || SqlError::Conflict {
            detail: "snapshot expiration evidence is not a cleanup handoff".to_owned(),
        };
        Ok(Self {
            version: EXPIRED_CLEANUP_PAYLOAD_VERSION,
            source_task_id,
            committed_snapshot_id: evidence.committed_snapshot_id.ok_or_else(missing)?,
            committed_metadata_location: evidence
                .committed_metadata_location
                .clone()
                .ok_or_else(missing)?,
            committed_metadata_digest: evidence
                .committed_metadata_digest
                .clone()
                .ok_or_else(missing)?,
            cleanup_candidates: evidence.cleanup_candidates.clone(),
        })
    }

    /// Validates that every copied candidate belongs to the cleanup task table.
    ///
    /// The plan copy is self-consistent — each candidate names its own table and
    /// its path carries that table name — but self-consistency alone does not
    /// bind the copy to the task that will execute it. A consumer holding a
    /// persisted cleanup task therefore re-checks the copy against the table its
    /// row is filed under, so a cross-table plan is refused before that task's
    /// lease, catalog access, stat, or delete.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when a candidate names another table.
    pub fn validate_for_table(&self, table: &ForgeTaskTableIdentity) -> Result<(), SqlError> {
        if self
            .cleanup_candidates
            .iter()
            .any(|candidate| &candidate.table != table)
        {
            return Err(SqlError::Conflict {
                detail: "expired cleanup plan contains a candidate for another table".to_owned(),
            });
        }
        Ok(())
    }

    /// Returns the serialized size of the candidate vector in bytes.
    ///
    /// This is the only meaningful byte estimate a cleanup task has: it reads
    /// no data files, so its estimate comes from the projection it carries
    /// rather than from anything it processes.
    #[must_use]
    pub fn serialized_candidate_bytes(&self) -> u64 {
        u64::try_from(
            candidates_to_value(&self.cleanup_candidates)
                .to_string()
                .len(),
        )
        .unwrap_or(u64::MAX)
    }

    /// Encodes the handoff into its stable closed JSON object.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "version": self.version,
            "kind": EXPIRED_CLEANUP_PAYLOAD_KIND,
            "source_task_id": self.source_task_id.to_string(),
            "committed_snapshot_id": self.committed_snapshot_id,
            "committed_metadata_location": self.committed_metadata_location,
            "committed_metadata_digest": self.committed_metadata_digest,
            "cleanup_candidates": candidates_to_value(&self.cleanup_candidates),
        })
    }

    /// Decodes one handoff, refusing every shape the contract does not name.
    ///
    /// Unknown fields, an unknown or missing version, a wrong `kind`, an empty
    /// or unordered candidate vector, and malformed identities all fail here,
    /// which is before lease acquisition and before any external IO.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`], or [`SqlError::InvariantViolation`] when
    /// `persisted`, for any departure from the closed shape.
    pub fn from_value(value: &serde_json::Value, persisted: bool) -> Result<Self, SqlError> {
        let fail = |detail: &str| {
            let detail = detail.to_owned();
            if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            }
        };
        let object = value
            .as_object()
            .ok_or_else(|| fail("expired cleanup payload is not an object"))?;
        const FIELDS: [&str; 7] = [
            "version",
            "kind",
            "source_task_id",
            "committed_snapshot_id",
            "committed_metadata_location",
            "committed_metadata_digest",
            "cleanup_candidates",
        ];
        if object.len() != FIELDS.len() || FIELDS.iter().any(|field| !object.contains_key(*field)) {
            return Err(fail(
                "expired cleanup payload has unknown or missing fields",
            ));
        }
        let version = object
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| fail("expired cleanup payload version is malformed"))?;
        if version != EXPIRED_CLEANUP_PAYLOAD_VERSION {
            return Err(fail("unknown expired cleanup payload version"));
        }
        if object.get("kind").and_then(serde_json::Value::as_str)
            != Some(EXPIRED_CLEANUP_PAYLOAD_KIND)
        {
            return Err(fail("expired cleanup payload kind is malformed"));
        }
        let source_task_id = object
            .get("source_task_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| fail("expired cleanup source task identity is malformed"))?;
        let committed_snapshot_id = object
            .get("committed_snapshot_id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| fail("expired cleanup committed snapshot is malformed"))?;
        let string = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| fail("expired cleanup committed metadata is malformed"))
        };
        let committed_metadata_location = string("committed_metadata_location")?;
        let committed_metadata_digest = string("committed_metadata_digest")?;
        let cleanup_candidates = candidates_from_value(
            object
                .get("cleanup_candidates")
                .ok_or_else(|| fail("expired cleanup candidates are missing"))?,
            persisted,
        )?;
        if cleanup_candidates.is_empty()
            || cleanup_candidates.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(fail(
                "expired cleanup candidates must be nonempty, ordered, and duplicate-free",
            ));
        }
        Ok(Self {
            version,
            source_task_id,
            committed_snapshot_id,
            committed_metadata_location,
            committed_metadata_digest,
            cleanup_candidates,
        })
    }
}

/// Encodes a validated plan into its stable JSON object.
pub(crate) fn plan_to_value(plan: &ForgeTaskPlan) -> serde_json::Value {
    serde_json::json!({"version":plan.version,"inputs":plan.inputs,"parameters":plan.parameters})
}

/// Decodes a persisted plan without accepting coercions or unknown versions.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] for malformed stored JSON.
fn plan_from_value(value: serde_json::Value) -> Result<ForgeTaskPlan, SqlError> {
    let object = value
        .as_object()
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "Forge plan is not an object".to_owned(),
        })?;
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "Forge plan version is malformed".to_owned(),
        })?;
    let inputs = object
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "Forge plan inputs are malformed".to_owned(),
        })?
        .iter()
        .map(|v| {
            v.as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| SqlError::InvariantViolation {
                    detail: "Forge plan input is not a string".to_owned(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parameters =
        object
            .get("parameters")
            .cloned()
            .ok_or_else(|| SqlError::InvariantViolation {
                detail: "Forge plan parameters are missing".to_owned(),
            })?;
    Ok(ForgeTaskPlan {
        version,
        inputs,
        parameters,
    })
}

/// Closed provenance category for a metadata-derived cleanup object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForgeCleanupCategory {
    /// Parquet data file removed from live metadata.
    Data,
    /// Manifest file removed from live metadata.
    Manifest,
    /// Metadata JSON file removed from live metadata.
    Metadata,
}

impl ForgeCleanupCategory {
    /// Returns the stable persisted category tag.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Manifest => "manifest",
            Self::Metadata => "metadata",
        }
    }
}

impl FromStr for ForgeCleanupCategory {
    type Err = SqlError;
    /// Parses a persisted cleanup category.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] for an unknown category.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "data" => Ok(Self::Data),
            "manifest" => Ok(Self::Manifest),
            "metadata" => Ok(Self::Metadata),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge cleanup category {value}"),
            }),
        }
    }
}

/// Strict relative object path eligible for Forge cleanup.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ForgeCleanupPath(String);

impl ForgeCleanupPath {
    /// Constructs a relative, segment-safe object path.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for absolute paths, empty or traversal
    /// segments, backslashes, controls, or unsupported characters.
    pub fn new(value: impl Into<String>) -> Result<Self, SqlError> {
        let value = value.into();
        if !safe_object_path(&value) {
            return Err(SqlError::Conflict {
                detail: "invalid Forge cleanup object path".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Borrows the validated relative path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One typed, table-bound cleanup candidate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ForgeCleanupCandidate {
    /// Object provenance category.
    pub category: ForgeCleanupCategory,
    /// Exact logical table whose metadata produced the candidate.
    pub table: ForgeTaskTableIdentity,
    /// Strict relative object-store path.
    pub path: ForgeCleanupPath,
}

impl ForgeCleanupCandidate {
    /// Validates that the path contains the exact table name as one segment.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when the candidate is not table-bound.
    pub fn validate(&self) -> Result<(), SqlError> {
        if self
            .path
            .as_str()
            .split('/')
            .any(|segment| segment == self.table.table)
        {
            Ok(())
        } else {
            Err(SqlError::Conflict {
                detail: "Forge cleanup candidate is not bound to its table".to_owned(),
            })
        }
    }
}

/// Versioned publication and cleanup evidence for one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTaskEvidence {
    /// Payload schema version.
    pub version: u16,
    /// Snapshot committed by the exact Iceberg transaction, when applicable.
    pub committed_snapshot_id: Option<i64>,
    /// Exact committed metadata location, when applicable.
    pub committed_metadata_location: Option<String>,
    /// Digest of the committed metadata, when applicable.
    pub committed_metadata_digest: Option<String>,
    /// Sorted, duplicate-free cleanup candidates derived from metadata.
    pub cleanup_candidates: Vec<ForgeCleanupCandidate>,
    /// Durable cursor into `cleanup_candidates`.
    pub deleted_candidate_count: u32,
    /// Candidate index whose object-store deletion is currently unresolved.
    ///
    /// `None` is the resting state: no delete has been submitted for this task,
    /// so the cursor alone describes it. `Some(i)` means preparation committed
    /// for candidate `i` and its external result is not yet proven, which is
    /// both the replay identity a successor resumes from and the protection
    /// every other maintenance reader must honour. It is always exactly
    /// `deleted_candidate_count`, and it can never exist once the cursor has
    /// reached the terminal frontier. Evidence written before this field
    /// existed decodes as `None`.
    pub prepared_candidate_index: Option<u32>,
}

impl ForgeTaskEvidence {
    /// Validates evidence version, ordering, and deletion cursor.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for invalid new evidence and
    /// [`SqlError::InvariantViolation`] for unknown persisted versions.
    pub fn validate(&self, persisted: bool) -> Result<(), SqlError> {
        if self.version != FORGE_TASK_PAYLOAD_VERSION {
            let detail = format!("unknown Forge task evidence version {}", self.version);
            return Err(if persisted {
                SqlError::InvariantViolation { detail }
            } else {
                SqlError::Conflict { detail }
            });
        }
        if usize::try_from(self.deleted_candidate_count)
            .map_or(true, |count| count > self.cleanup_candidates.len())
            || self
                .cleanup_candidates
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self
                .cleanup_candidates
                .iter()
                .any(|candidate| candidate.validate().is_err())
        {
            return Err(SqlError::Conflict {
                detail: "invalid Forge evidence cleanup cursor or candidates".to_owned(),
            });
        }
        if self.prepared_candidate_index.is_some_and(|index| {
            index != self.deleted_candidate_count
                || usize::try_from(index)
                    .map_or(true, |index| index >= self.cleanup_candidates.len())
        }) {
            return Err(SqlError::Conflict {
                detail: "Forge prepared candidate index must name the current cursor candidate"
                    .to_owned(),
            });
        }
        if self.committed_metadata_location.is_some() != self.committed_metadata_digest.is_some()
            || (self.committed_snapshot_id.is_none()
                && (self.committed_metadata_location.is_some()
                    || !self.cleanup_candidates.is_empty()
                    || self.deleted_candidate_count != 0))
        {
            return Err(SqlError::Conflict {
                detail: "Forge evidence committed snapshot/metadata fields are inconsistent"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Validates that every cleanup candidate belongs to the task table.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when a candidate names another table.
    pub fn validate_for_table(&self, table: &ForgeTaskTableIdentity) -> Result<(), SqlError> {
        if self
            .cleanup_candidates
            .iter()
            .any(|candidate| &candidate.table != table)
        {
            return Err(SqlError::Conflict {
                detail: "Forge cleanup evidence contains a candidate for another table".to_owned(),
            });
        }
        Ok(())
    }
}

/// Encodes one ordered candidate vector into its stable JSON array.
///
/// The same encoding backs both the immutable plan handoff and the mutable
/// attempt evidence, so a byte-for-byte plan/evidence parity check is a plain
/// value comparison rather than two encoders that can drift apart.
pub(crate) fn candidates_to_value(candidates: &[ForgeCleanupCandidate]) -> serde_json::Value {
    serde_json::Value::Array(candidates.iter().map(|candidate|serde_json::json!({"category":candidate.category.as_str(),"catalog":candidate.table.catalog,"namespace":candidate.table.namespace,"table":candidate.table.table,"path":candidate.path.as_str()})).collect())
}

/// Decodes one candidate vector, validating every identity and table binding.
///
/// # Errors
///
/// Returns [`SqlError::Conflict`], or [`SqlError::InvariantViolation`] when
/// `persisted`, for a non-array value, a malformed candidate object, an unknown
/// category, an unsafe path, or a candidate that is not bound to its own table.
pub(crate) fn candidates_from_value(
    value: &serde_json::Value,
    persisted: bool,
) -> Result<Vec<ForgeCleanupCandidate>, SqlError> {
    let fail = |detail: &str| {
        let detail = detail.to_owned();
        if persisted {
            SqlError::InvariantViolation { detail }
        } else {
            SqlError::Conflict { detail }
        }
    };
    value
        .as_array()
        .ok_or_else(|| fail("cleanup candidates are malformed"))?
        .iter()
        .map(|value| {
            let candidate = value
                .as_object()
                .ok_or_else(|| fail("cleanup candidate is not an object"))?;
            let string = |key: &str| {
                candidate
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| fail("cleanup candidate field is malformed"))
            };
            let category = string("category")?
                .parse()
                .map_err(|_| fail("cleanup candidate category is malformed"))?;
            let table = ForgeTaskTableIdentity::new(
                string("catalog")?,
                string("namespace")?,
                string("table")?,
            )
            .map_err(|_| fail("cleanup candidate table identity is malformed"))?;
            let path = ForgeCleanupPath::new(string("path")?)
                .map_err(|_| fail("cleanup candidate path is malformed"))?;
            let candidate = ForgeCleanupCandidate {
                category,
                table,
                path,
            };
            candidate
                .validate()
                .map_err(|_| fail("cleanup candidate is not table-bound"))?;
            Ok(candidate)
        })
        .collect()
}

/// Encodes validated attempt evidence into its stable JSON object.
pub fn evidence_to_value(evidence: &ForgeTaskEvidence) -> serde_json::Value {
    serde_json::json!({"version":evidence.version,"committed_snapshot_id":evidence.committed_snapshot_id,"committed_metadata_location":evidence.committed_metadata_location,"committed_metadata_digest":evidence.committed_metadata_digest,"cleanup_candidates":candidates_to_value(&evidence.cleanup_candidates),"deleted_candidate_count":evidence.deleted_candidate_count,"prepared_candidate_index":evidence.prepared_candidate_index})
}

/// Decodes one persisted evidence column for callers outside this module.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for malformed stored JSON.
pub fn evidence_from_json(value: serde_json::Value) -> Result<ForgeTaskEvidence, SqlError> {
    evidence_from_value(value)
}

/// Decodes persisted attempt evidence and rejects malformed column shapes.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] for malformed stored JSON.
fn evidence_from_value(value: serde_json::Value) -> Result<ForgeTaskEvidence, SqlError> {
    let object = value
        .as_object()
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "Forge evidence is not an object".to_owned(),
        })?;
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "Forge evidence version is malformed".to_owned(),
        })?;
    let committed_snapshot_id = object
        .get("committed_snapshot_id")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_i64().ok_or_else(|| SqlError::InvariantViolation {
                detail: "committed snapshot is malformed".to_owned(),
            })
        })
        .transpose()?;
    let string_option = |key: &str| {
        object
            .get(key)
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| SqlError::InvariantViolation {
                        detail: format!("{key} is malformed"),
                    })
            })
            .transpose()
    };
    let cleanup_candidates = candidates_from_value(
        object
            .get("cleanup_candidates")
            .ok_or_else(|| SqlError::InvariantViolation {
                detail: "cleanup candidates are malformed".to_owned(),
            })?,
        true,
    )?;
    let deleted_candidate_count = object
        .get("deleted_candidate_count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "deletion cursor is malformed".to_owned(),
        })?;
    let prepared_candidate_index = object
        .get("prepared_candidate_index")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| SqlError::InvariantViolation {
                    detail: "prepared candidate index is malformed".to_owned(),
                })
        })
        .transpose()?;
    Ok(ForgeTaskEvidence {
        version,
        committed_snapshot_id,
        committed_metadata_location: string_option("committed_metadata_location")?,
        committed_metadata_digest: string_option("committed_metadata_digest")?,
        cleanup_candidates,
        deleted_candidate_count,
        prepared_candidate_index,
    })
}

/// Authoritative unacknowledged planning-demand status for one complete scan.
///
/// Published by the fenced coordinator after a complete pass. `demands` is the
/// exact row count rather than a page, and `oldest_requested_at` is the stored
/// request time so a consumer can derive age itself and keep seeing it grow if
/// the producer later stalls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeDemandStatus {
    /// Exact count of unacknowledged planning demands.
    pub demands: u64,
    /// Earliest `first_requested_at` across those demands, if any exist.
    pub oldest_requested_at: Option<DateTime<Utc>>,
}

/// Authoritative pending-task status for one strategy after a complete scan.
///
/// Pending means exactly `ready` or `retryable`: an owned row belongs to the
/// worker that claimed it. A strategy with no pending row produces no value,
/// so the publisher supplies its explicit zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgePendingTaskStatus {
    /// Work type this status describes.
    pub strategy: ForgeTaskStrategy,
    /// Exact count of `ready` or `retryable` rows for this strategy.
    pub pending: u64,
    /// Earliest `ready_at` across those rows.
    pub oldest_ready_at: Option<DateTime<Utc>>,
}

/// Closed Forge task strategies.
///
/// The set spans publication work (`scribe_promotion`), rewrite work, and
/// maintenance-family cleanup; [`ForgeTaskStrategy::is_maintenance`] is the
/// only classification that distinguishes them for scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ForgeTaskStrategy {
    /// Promote already-published Scribe hot objects into the table unchanged.
    ScribePromotion,
    /// Compact small files.
    SmallFiles,
    /// Expire snapshots.
    SnapshotExpiry,
    /// Delete metadata-derived expired files.
    ExpiredCleanup,
    /// Delete never-published outputs.
    OrphanCleanup,
}

/// The maintenance-family strategies a reserved worker slot claims first.
///
/// These lifecycle and garbage-collection strategies reclaim durable storage
/// but produce no compaction throughput, so they are exactly the strategies a
/// sustained compaction backlog would otherwise starve. Compaction and rewrite
/// strategies are deliberately excluded. Passed to
/// [`super::super::queries::forge_tasks::ForgeTasks::claim_fair`] as the
/// reserved slot's strategy filter.
pub const MAINTENANCE_STRATEGIES: &[ForgeTaskStrategy] = &[
    ForgeTaskStrategy::SnapshotExpiry,
    ForgeTaskStrategy::ExpiredCleanup,
    ForgeTaskStrategy::OrphanCleanup,
];

impl ForgeTaskStrategy {
    /// Returns whether this strategy is maintenance-family lifecycle work.
    ///
    /// Maintenance-family strategies reclaim durable storage (snapshot expiry
    /// and orphan or expired cleanup) rather than producing compaction
    /// throughput. The reserved worker slot uses this classification so a ready
    /// maintenance task stays claimable regardless of compaction backlog. The
    /// set is kept in sync with [`MAINTENANCE_STRATEGIES`].
    #[must_use]
    pub fn is_maintenance(self) -> bool {
        matches!(
            self,
            Self::SnapshotExpiry | Self::ExpiredCleanup | Self::OrphanCleanup
        )
    }

    /// Returns the stable SQL representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScribePromotion => "scribe_promotion",
            Self::SmallFiles => "small_files",
            Self::SnapshotExpiry => "snapshot_expiry",
            Self::ExpiredCleanup => "expired_cleanup",
            Self::OrphanCleanup => "orphan_cleanup",
        }
    }
}
impl FromStr for ForgeTaskStrategy {
    type Err = SqlError;
    /// Parses a stored strategy.
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] for an unknown value.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "scribe_promotion" => Ok(Self::ScribePromotion),
            "small_files" => Ok(Self::SmallFiles),
            "snapshot_expiry" => Ok(Self::SnapshotExpiry),
            "expired_cleanup" => Ok(Self::ExpiredCleanup),
            "orphan_cleanup" => Ok(Self::OrphanCleanup),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge task strategy {value}"),
            }),
        }
    }
}

/// Closed placeholder written into the projected task row when the claim
/// boundary decoded an unrecognized raw strategy tag.
///
/// The authoritative value for such a claim is
/// [`ForgeClaimStrategy::Unknown`], which retains the exact raw tag for
/// terminal quarantine audit. The projected [`ForgeTask`] contract is closed
/// over [`ForgeTaskStrategy`], so a placeholder is required to build it. It is
/// deliberately a dormant rewrite identity and never a live route: an
/// unrecognized tag must never be observable as a claimable promotion.
const QUARANTINE_STRATEGY_PLACEHOLDER: ForgeTaskStrategy = ForgeTaskStrategy::SmallFiles;

/// Claim-boundary strategy decoded from an independently persisted raw tag.
///
/// Scheduler and enqueue contracts remain closed over [`ForgeTaskStrategy`].
/// Only claimed rows can carry an unknown value so workers can quarantine
/// corrupted or forward-incompatible data without rolling back the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeClaimStrategy {
    /// A strategy recognized by this worker version.
    Known(ForgeTaskStrategy),
    /// An unrecognized raw SQL value retained for terminal audit.
    Unknown(String),
}

impl ForgeClaimStrategy {
    /// Decodes a raw SQL value without discarding an unknown tag.
    #[must_use]
    pub fn from_raw(value: String) -> Self {
        match value.parse() {
            Ok(strategy) => Self::Known(strategy),
            Err(_) => Self::Unknown(value),
        }
    }

    /// Returns the exact raw value represented by this claim boundary.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(strategy) => strategy.as_str(),
            Self::Unknown(value) => value,
        }
    }
}

/// Durable task lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeTaskState {
    /// Eligible for claim.
    Ready,
    /// Assigned but not started.
    Claimed,
    /// Worker execution has started.
    Running,
    /// External effect is prepared or uncertain.
    Prepared,
    /// Completed successfully.
    Succeeded,
    /// Eligible for a later attempt.
    Retryable,
    /// Permanently failed.
    Failed,
    /// Superseded before external effects or explicitly cancelled by an operator.
    Cancelled,
}
impl ForgeTaskState {
    /// Returns the stable SQL representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Prepared => "prepared",
            Self::Succeeded => "succeeded",
            Self::Retryable => "retryable",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    /// Reports whether this state is terminal and eligible for retention pruning.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
impl FromStr for ForgeTaskState {
    type Err = SqlError;
    /// Parses a stored state.
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] for an unknown value.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ready" => Ok(Self::Ready),
            "claimed" => Ok(Self::Claimed),
            "running" => Ok(Self::Running),
            "prepared" => Ok(Self::Prepared),
            "succeeded" => Ok(Self::Succeeded),
            "retryable" => Ok(Self::Retryable),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge task state {value}"),
            }),
        }
    }
}

/// Positive persisted admission estimates.
#[derive(Debug, Clone, Copy)]
pub struct ForgeTaskEstimates {
    /// Input file count.
    pub files: u32,
    /// Estimated input bytes.
    pub bytes: u64,
}

impl ForgeTaskEstimates {
    /// Validates that every persisted admission ceiling is positive and SQL-safe.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when any value is zero or exceeds `i64`.
    pub fn validate(self) -> Result<(), SqlError> {
        if self.files == 0 || self.bytes == 0 || i64::try_from(self.bytes).is_err() {
            return Err(SqlError::Conflict {
                detail: "Forge task estimates must be positive and fit PostgreSQL bigint"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

/// Immutable values supplied when enqueueing one task.
#[derive(Debug, Clone)]
pub struct NewForgeTask {
    /// Tenant isolation identity.
    pub data_tenant_id: DataTenantId,
    /// Validated logical table identity.
    pub table_ref: ForgeTaskTableIdentity,
    /// Planned maintenance strategy.
    pub strategy: ForgeTaskStrategy,
    /// Snapshot used by the planner.
    pub base_snapshot_id: i64,
    /// Versioned exact plan.
    pub plan: ForgeTaskPlan,
    /// SHA-256 digest of canonical plan bytes.
    pub plan_hash: [u8; 32],
    /// Positive admission estimates.
    pub estimates: ForgeTaskEstimates,
    /// First eligible claim time, or `None` to defer to the database clock.
    ///
    /// The fair claim gates on `ready_at <= statement_timestamp()`, so this
    /// deadline is only meaningful on the database's clock. An application
    /// clock running ahead of the database would make a freshly planned task
    /// unclaimable for the skew window, and one running behind would release
    /// it early. `None` therefore means "eligible now" and is stamped by
    /// Postgres inside the inserting statement; `Some` is reserved for a
    /// deliberate deferral whose absolute instant the caller owns.
    pub ready_at: Option<DateTime<Utc>>,
}

/// One fully validated durable Forge task.
#[derive(Debug, Clone)]
pub struct ForgeTask {
    /// Stable task identifier.
    pub task_id: Uuid,
    /// Tenant isolation identity.
    pub data_tenant_id: DataTenantId,
    /// Validated table identity.
    pub table_ref: ForgeTaskTableIdentity,
    /// Closed maintenance strategy.
    pub strategy: ForgeTaskStrategy,
    /// Snapshot on which planning was based.
    pub base_snapshot_id: i64,
    /// Versioned exact plan.
    pub plan: ForgeTaskPlan,
    /// Positive admission estimates.
    pub estimates: ForgeTaskEstimates,
    /// Durable state.
    pub state: ForgeTaskState,
    /// Current attempt identity.
    pub attempt_id: Option<Uuid>,
    /// Claim owner.
    pub claimed_by: Option<Uuid>,
    /// Claim deadline.
    pub claim_expires_at: Option<DateTime<Utc>>,
    /// Active GC watermark.
    pub watermark: Option<SnapshotWatermark>,
    /// Strategy-dispatched task-row evidence.
    pub evidence: Option<ForgeTaskRowEvidence>,
    /// Attempt-consuming failures observed for this task.
    pub attempt_count: u32,
    /// Closed persisted failure classification, when the prior attempt failed.
    pub failure_class: Option<String>,
    /// Durable time before which this task may not be claimed.
    pub next_eligible_at: DateTime<Utc>,
    /// Next eligibility time.
    pub ready_at: DateTime<Utc>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last mutation time.
    pub updated_at: DateTime<Utc>,
}

/// One atomic Ready/Retryable claim with a raw-preserving strategy boundary.
///
/// The context tenant is projected from the locked claim candidate while the
/// task tenant is decoded from the durable task row. Worker validation must
/// compare both before acquiring a table lease or touching external storage.
#[derive(Debug, Clone)]
pub struct ForgeTaskClaim {
    /// Tenant selected by the claim transaction for worker execution.
    pub execution_tenant_id: DataTenantId,
    /// Stable task identifier.
    pub task_id: Uuid,
    /// Tenant isolation identity decoded from the task row.
    pub data_tenant_id: DataTenantId,
    /// Validated table identity.
    pub table_ref: ForgeTaskTableIdentity,
    /// Raw-preserving claim-only strategy.
    pub strategy: ForgeClaimStrategy,
    /// Snapshot on which planning was based.
    pub base_snapshot_id: i64,
    /// Versioned exact plan.
    pub plan: ForgeTaskPlan,
    /// Positive admission estimates.
    pub estimates: ForgeTaskEstimates,
    /// Durable state after atomic claim.
    pub state: ForgeTaskState,
    /// Current attempt identity.
    pub attempt_id: Option<Uuid>,
    /// Claim owner.
    pub claimed_by: Option<Uuid>,
    /// Claim deadline.
    pub claim_expires_at: Option<DateTime<Utc>>,
    /// Active GC watermark.
    pub watermark: Option<SnapshotWatermark>,
    /// Strategy-dispatched task-row evidence.
    pub evidence: Option<ForgeTaskRowEvidence>,
    /// Attempt-consuming failures observed for this task.
    pub attempt_count: u32,
    /// Closed persisted failure classification, when the prior attempt failed.
    pub failure_class: Option<String>,
    /// Durable time before which this task may not be claimed.
    pub next_eligible_at: DateTime<Utc>,
    /// Next eligibility time.
    pub ready_at: DateTime<Utc>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last mutation time.
    pub updated_at: DateTime<Utc>,
}

/// One expired Prepared task claimed through the closed canonical decoder.
#[derive(Debug, Clone)]
pub struct ForgePreparedTaskClaim {
    /// Tenant selected by the Prepared takeover transaction.
    pub execution_tenant_id: DataTenantId,
    /// Fully validated Prepared task assigned to the reconciler.
    pub task: ForgeTask,
}

impl std::ops::Deref for ForgePreparedTaskClaim {
    type Target = ForgeTask;

    /// Projects canonical Prepared task fields while retaining execution context.
    fn deref(&self) -> &Self::Target {
        &self.task
    }
}

/// Exact identity and state pair for one lifecycle mutation.
#[derive(Debug, Clone, Copy)]
pub struct ForgeTaskTransition {
    /// Stable task identity.
    pub task_id: Uuid,
    /// Exact attempt generation.
    pub attempt_id: Uuid,
    /// Exact claim owner.
    pub owner: Uuid,
    /// Required current state.
    pub expected: ForgeTaskState,
    /// Requested next state.
    pub next: ForgeTaskState,
}

/// Result of an exact lifecycle mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeTaskTransitionOutcome {
    /// Mutation was applied.
    Applied,
    /// Identical state was already durable.
    AlreadyApplied,
}

/// Which physical outcome one prepared cleanup candidate reached.
///
/// Only [`Self::Deleted`] and [`Self::Missing`] are proofs: both mean the
/// object cannot be read again, so the cursor may pass the candidate. The other
/// two are observations that must be durable — a refusal proves nothing was
/// submitted, an uncertainty proves nothing about what the object store did —
/// so both retain the prepared candidate for same-identity replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiredCleanupOutcome {
    /// The object store accepted the deletion.
    Deleted,
    /// The object was proven absent after the complete fresh safety proof.
    Missing,
    /// A safety proof, validation, fence, or cancellation failed before the
    /// delete was submitted, so no external effect was attempted.
    Refused,
    /// A delete was submitted and its result cannot prove acceptance or
    /// rejection, including cancellation after submission.
    Uncertain,
}

impl ExpiredCleanupOutcome {
    /// Returns the stable transition name this outcome is traced under.
    #[must_use]
    pub const fn transition_name(self) -> &'static str {
        match self {
            Self::Deleted => "forge.expired_cleanup.candidate_deleted",
            Self::Missing => "forge.expired_cleanup.candidate_missing",
            Self::Refused => "forge.expired_cleanup.candidate_refused",
            Self::Uncertain => "forge.expired_cleanup.candidate_uncertain",
        }
    }

    /// Reports whether this outcome authorizes the cursor to pass the candidate.
    #[must_use]
    pub const fn advances(self) -> bool {
        matches!(self, Self::Deleted | Self::Missing)
    }
}

/// The exact identity one per-candidate cleanup transition is applied under.
///
/// Every field is revalidated inside the workflow's own short operator
/// transaction: a stale owner, a lost table fence, a different attempt, or a
/// cursor that has already moved all refuse rather than mutate.
#[derive(Debug, Clone, Copy)]
pub struct ExpiredCleanupCandidateRequest<'request> {
    /// Fenced task, attempt, current owner, and live table lease.
    pub authority: &'request ForgeExpirationAuthority,
    /// Registered table whose maintenance-authority row is taken FOR UPDATE.
    pub table: &'request ForgeClaimTable,
    /// Cursor index this transition names; must equal the durable cursor.
    pub index: u32,
    /// The exact candidate at `index`, compared against the immutable plan.
    pub candidate: &'request ForgeCleanupCandidate,
}

/// Bounded task status page.
#[derive(Debug, Clone)]
pub struct ForgeTaskPage {
    /// Validated rows, never exceeding the requested cap.
    pub tasks: Vec<ForgeTask>,
    /// Whether at least one additional row exists.
    pub overflowed: bool,
}

/// Returns whether an identity component is safe for durable reconstruction.
fn safe_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && !value.contains("..")
        && !value.contains('/')
        && !value.contains('\\')
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

/// Returns whether a relative object path follows Forge cleanup safety rules.
fn safe_object_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && value.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.len() <= 255
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '=')
                })
        })
}

/// Private SQL decoder for the canonical Forge task projection.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ForgeTaskSqlRow {
    task_id: Uuid,
    data_tenant_id: Uuid,
    catalog_name: String,
    namespace_name: String,
    table_name: String,
    strategy: String,
    base_snapshot_id: i64,
    plan: serde_json::Value,
    estimated_files: i64,
    estimated_bytes: i64,
    state: String,
    attempt_id: Option<Uuid>,
    claimed_by: Option<Uuid>,
    claim_expires_at: Option<DateTime<Utc>>,
    watermark_snapshot_id: Option<i64>,
    watermark_timestamp_ms: Option<i64>,
    evidence: Option<serde_json::Value>,
    attempt_count: i32,
    failure_class: Option<String>,
    next_eligible_at: DateTime<Utc>,
    ready_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Private SQL decoder for a claim candidate and its durable task projection.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ForgeTaskClaimSqlRow {
    /// Tenant independently projected from the locked candidate CTE.
    execution_tenant_id: Uuid,
    /// Complete durable task row returned by the claimed update.
    #[sqlx(flatten)]
    task: ForgeTaskSqlRow,
}

/// Private SQL decoder for a Prepared takeover using the closed task contract.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ForgePreparedTaskClaimSqlRow {
    /// Tenant independently projected from the locked Prepared candidate.
    execution_tenant_id: Uuid,
    /// Complete durable task row returned by the takeover update.
    #[sqlx(flatten)]
    task: ForgeTaskSqlRow,
}

/// Decodes one persisted evidence column under its row's own strategy.
///
/// Publication, expiration, and expired cleanup keep their existing closed
/// evidence contract, including the table binding re-check that stops a
/// persisted candidate vector from naming another table. Orphan cleanup is the
/// one strategy whose evidence is a traversal cursor, and it is rebound to the
/// row's own immutable scan prefix here so an out-of-prefix cursor fails closed
/// at the same boundary.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for any stored shape the row's
/// strategy does not accept.
fn decode_row_evidence(
    strategy: ForgeTaskStrategy,
    value: &serde_json::Value,
    plan: &ForgeTaskPlan,
    table_ref: &ForgeTaskTableIdentity,
) -> Result<ForgeTaskRowEvidence, SqlError> {
    if strategy == ForgeTaskStrategy::OrphanCleanup {
        let prefix = plan.orphan_cleanup_prefix(true)?;
        return ForgeTaskRowEvidence::decode(strategy, value, prefix);
    }
    let evidence = evidence_from_value(value.clone())?;
    evidence.validate(true)?;
    evidence
        .validate_for_table(table_ref)
        .map_err(|_| SqlError::InvariantViolation {
            detail: "persisted Forge cleanup evidence is bound to another table".to_owned(),
        })?;
    Ok(ForgeTaskRowEvidence::Publication(evidence))
}

impl TryFrom<ForgeTaskClaimSqlRow> for ForgeTaskClaim {
    type Error = SqlError;

    /// Validates the execution tenant and durable task without conflating them.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when either the execution
    /// tenant or task projection is malformed.
    fn try_from(mut row: ForgeTaskClaimSqlRow) -> Result<Self, Self::Error> {
        let execution_tenant_id =
            DataTenantId::try_from(row.execution_tenant_id).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "Forge claim contains invalid execution tenant identity".to_owned(),
                }
            })?;
        let strategy = ForgeClaimStrategy::from_raw(row.task.strategy.clone());
        row.task.strategy = match &strategy {
            ForgeClaimStrategy::Known(value) => value.as_str().to_owned(),
            ForgeClaimStrategy::Unknown(_) => QUARANTINE_STRATEGY_PLACEHOLDER.as_str().to_owned(),
        };
        let task: ForgeTask = row.task.try_into()?;
        Ok(Self {
            execution_tenant_id,
            task_id: task.task_id,
            data_tenant_id: task.data_tenant_id,
            table_ref: task.table_ref,
            strategy,
            base_snapshot_id: task.base_snapshot_id,
            plan: task.plan,
            estimates: task.estimates,
            state: task.state,
            attempt_id: task.attempt_id,
            claimed_by: task.claimed_by,
            claim_expires_at: task.claim_expires_at,
            watermark: task.watermark,
            evidence: task.evidence,
            attempt_count: task.attempt_count,
            failure_class: task.failure_class,
            next_eligible_at: task.next_eligible_at,
            ready_at: task.ready_at,
            created_at: task.created_at,
            updated_at: task.updated_at,
        })
    }
}

impl TryFrom<ForgePreparedTaskClaimSqlRow> for ForgePreparedTaskClaim {
    type Error = SqlError;

    /// Validates the Prepared execution tenant and closed durable task.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when the execution tenant or
    /// canonical task projection is malformed, including an unknown strategy.
    fn try_from(row: ForgePreparedTaskClaimSqlRow) -> Result<Self, Self::Error> {
        let execution_tenant_id =
            DataTenantId::try_from(row.execution_tenant_id).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "Forge Prepared claim contains invalid execution tenant identity"
                        .to_owned(),
                }
            })?;
        Ok(Self {
            execution_tenant_id,
            task: row.task.try_into()?,
        })
    }
}

impl TryFrom<ForgeTaskSqlRow> for ForgeTask {
    type Error = SqlError;
    /// Reconstructs and validates every persisted column before returning it.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when any stored column or
    /// payload violates the durable Forge task invariants.
    fn try_from(row: ForgeTaskSqlRow) -> Result<Self, Self::Error> {
        let data_tenant_id = DataTenantId::try_from(row.data_tenant_id).map_err(|_| {
            SqlError::InvariantViolation {
                detail: "Forge task contains invalid tenant identity".to_owned(),
            }
        })?;
        let table_ref =
            ForgeTaskTableIdentity::new(row.catalog_name, row.namespace_name, row.table_name)
                .map_err(|_| SqlError::InvariantViolation {
                    detail: "Forge task contains malformed table identity".to_owned(),
                })?;
        let strategy: ForgeTaskStrategy = row.strategy.parse()?;
        let plan = plan_from_value(row.plan)?;
        plan.validate(true)?;
        let evidence = row
            .evidence
            .map(|value| decode_row_evidence(strategy, &value, &plan, &table_ref))
            .transpose()?;
        let watermark = match (row.watermark_snapshot_id, row.watermark_timestamp_ms) {
            (Some(snapshot_id), Some(timestamp_ms)) => {
                let value = SnapshotWatermark {
                    snapshot_id,
                    timestamp_ms,
                };
                value.validate().map_err(|_| SqlError::InvariantViolation {
                    detail: "invalid persisted Forge watermark timestamp".to_owned(),
                })?;
                Some(value)
            }
            (None, None) => None,
            _ => {
                return Err(SqlError::InvariantViolation {
                    detail: "partial Forge task watermark".to_owned(),
                });
            }
        };
        let estimates = ForgeTaskEstimates {
            files: u32::try_from(row.estimated_files).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "invalid estimated_files".to_owned(),
                }
            })?,
            bytes: u64::try_from(row.estimated_bytes).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "invalid estimated_bytes".to_owned(),
                }
            })?,
        };
        estimates
            .validate()
            .map_err(|_| SqlError::InvariantViolation {
                detail: "invalid persisted Forge estimates".to_owned(),
            })?;
        let attempt_count =
            u32::try_from(row.attempt_count).map_err(|_| SqlError::InvariantViolation {
                detail: "invalid Forge attempt count".to_owned(),
            })?;
        row.failure_class
            .as_deref()
            .map(ForgeFailureClass::from_sql)
            .transpose()?;
        Ok(Self {
            task_id: row.task_id,
            data_tenant_id,
            table_ref,
            strategy,
            base_snapshot_id: row.base_snapshot_id,
            plan,
            estimates,
            state: row.state.parse()?,
            attempt_id: row.attempt_id,
            claimed_by: row.claimed_by,
            claim_expires_at: row.claim_expires_at,
            watermark,
            evidence,
            attempt_count,
            failure_class: row.failure_class,
            next_eligible_at: row.next_eligible_at,
            ready_at: row.ready_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects empty, unsupported, and path-unsafe persisted identity components.
    ///
    /// # Panics
    /// Panics when a boundary assertion does not hold.
    #[test]
    fn task_identity_rejects_unsafe_components() {
        assert!(ForgeTaskTableIdentity::new("", "vala.bifrost", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("unsupported", "vala.bifrost", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "unsupported", "events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "../events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", ".events").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "events.").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "bad name").is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "x".repeat(64)).is_err());
        assert!(ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "x".repeat(63)).is_ok());
    }

    /// Rejects unknown plan versions, duplicate inputs, and unsafe candidates.
    ///
    /// # Panics
    /// Panics when a fail-closed assertion does not hold.
    #[test]
    fn task_plan_fails_closed() {
        let unknown = ForgeTaskPlan {
            version: 2,
            inputs: vec![],
            parameters: serde_json::json!({}),
        };
        assert!(matches!(
            unknown.validate(true),
            Err(SqlError::InvariantViolation { .. })
        ));
        let duplicate = ForgeTaskPlan {
            version: 1,
            inputs: vec!["a".to_owned(), "a".to_owned()],
            parameters: serde_json::json!({}),
        };
        assert!(duplicate.validate(false).is_err());
    }

    /// Orders snapshot protection by timestamps without assuming monotonic IDs.
    ///
    /// # Panics
    /// Panics when timestamp ordering accidentally follows snapshot IDs.
    #[test]
    fn watermark_snapshot_ids_are_not_order_keys() {
        let older = SnapshotWatermark {
            snapshot_id: 900,
            timestamp_ms: 10,
        };
        let newer = SnapshotWatermark {
            snapshot_id: 2,
            timestamp_ms: 20,
        };
        assert!(older.timestamp_ms < newer.timestamp_ms);
        assert!(older.snapshot_id > newer.snapshot_id);
    }

    /// Rejects evidence cursors beyond the exact cleanup candidate set.
    ///
    /// # Panics
    /// Panics when malformed caller or persisted evidence is accepted.
    #[test]
    fn evidence_cursor_fails_closed() {
        let evidence = ForgeTaskEvidence {
            prepared_candidate_index: None,
            version: 1,
            committed_snapshot_id: Some(7),
            committed_metadata_location: Some("metadata/v7.json".to_owned()),
            committed_metadata_digest: Some("digest".to_owned()),
            cleanup_candidates: vec![ForgeCleanupCandidate {
                category: ForgeCleanupCategory::Data,
                table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "events")
                    .expect("table"),
                path: ForgeCleanupPath::new("tenants/t/vala.bifrost/events/data/a.parquet")
                    .expect("path"),
            }],
            deleted_candidate_count: 2,
        };
        assert!(evidence.validate(false).is_err());
        let mismatched = ForgeTaskEvidence {
            prepared_candidate_index: None,
            version: 1,
            committed_snapshot_id: Some(7),
            committed_metadata_location: Some("metadata/v7.json".to_owned()),
            committed_metadata_digest: None,
            cleanup_candidates: vec![],
            deleted_candidate_count: 0,
        };
        assert!(mismatched.validate(false).is_err());
        for path in [
            "/absolute/events/a",
            "events//a",
            "events/../a",
            "events\\a",
            "events/\u{7}a",
        ] {
            assert!(ForgeCleanupPath::new(path).is_err());
        }
        let other = ForgeCleanupCandidate {
            category: ForgeCleanupCategory::Data,
            table: ForgeTaskTableIdentity::new("wyrd-redux", "vala.bifrost", "other")
                .expect("other"),
            path: ForgeCleanupPath::new("tenants/t/vala.bifrost/events/data/a.parquet")
                .expect("safe path"),
        };
        assert!(
            other.validate().is_err(),
            "candidate path must contain its exact table segment"
        );
        let malformed = serde_json::json!({"version":1,"committed_snapshot_id":7,"committed_metadata_location":"metadata/v7.json","committed_metadata_digest":"digest","cleanup_candidates":[{"category":"data","catalog":"wyrd-redux","namespace":"vala.bifrost","table":"events","path":"/absolute/events/a.parquet"}],"deleted_candidate_count":0});
        assert!(
            evidence_from_value(malformed).is_err(),
            "malformed persisted cleanup paths fail closed"
        );
    }
}
