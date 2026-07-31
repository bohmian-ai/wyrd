//! Validated domain and SQL row types for durable Forge tasks.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

use crate::SqlError;

/// Current version of persisted task plans and evidence.
pub const FORGE_TASK_PAYLOAD_VERSION: u16 = 1;

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
        Ok(Self {
            data_tenant_id,
            table_ref,
            first_requested_at: row.first_requested_at,
            last_requested_at: row.last_requested_at,
            last_source: ForgePlanningDemandSource::from_sql(&row.last_source)?,
            generation: row.generation,
        })
    }
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
            "vala.genai",
            "vala.eval",
            "vala.drift",
            "vala.dev",
            "vala.datasets",
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

/// Encodes validated attempt evidence into its stable JSON object.
pub(crate) fn evidence_to_value(evidence: &ForgeTaskEvidence) -> serde_json::Value {
    let candidates=evidence.cleanup_candidates.iter().map(|candidate|serde_json::json!({"category":candidate.category.as_str(),"catalog":candidate.table.catalog,"namespace":candidate.table.namespace,"table":candidate.table.table,"path":candidate.path.as_str()})).collect::<Vec<_>>();
    serde_json::json!({"version":evidence.version,"committed_snapshot_id":evidence.committed_snapshot_id,"committed_metadata_location":evidence.committed_metadata_location,"committed_metadata_digest":evidence.committed_metadata_digest,"cleanup_candidates":candidates,"deleted_candidate_count":evidence.deleted_candidate_count})
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
    let cleanup_candidates = object
        .get("cleanup_candidates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "cleanup candidates are malformed".to_owned(),
        })?
        .iter()
        .map(|value| {
            let candidate = value
                .as_object()
                .ok_or_else(|| SqlError::InvariantViolation {
                    detail: "cleanup candidate is not an object".to_owned(),
                })?;
            let string = |key: &str| {
                candidate
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| SqlError::InvariantViolation {
                        detail: format!("cleanup candidate {key} is malformed"),
                    })
            };
            let category = string("category")?.parse()?;
            let table = ForgeTaskTableIdentity::new(
                string("catalog")?,
                string("namespace")?,
                string("table")?,
            )
            .map_err(|_| SqlError::InvariantViolation {
                detail: "cleanup candidate table identity is malformed".to_owned(),
            })?;
            let path = ForgeCleanupPath::new(string("path")?).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "cleanup candidate path is malformed".to_owned(),
                }
            })?;
            let candidate = ForgeCleanupCandidate {
                category,
                table,
                path,
            };
            candidate
                .validate()
                .map_err(|_| SqlError::InvariantViolation {
                    detail: "cleanup candidate is not table-bound".to_owned(),
                })?;
            Ok::<ForgeCleanupCandidate, SqlError>(candidate)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let deleted_candidate_count = object
        .get("deleted_candidate_count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "deletion cursor is malformed".to_owned(),
        })?;
    Ok(ForgeTaskEvidence {
        version,
        committed_snapshot_id,
        committed_metadata_location: string_option("committed_metadata_location")?,
        committed_metadata_digest: string_option("committed_metadata_digest")?,
        cleanup_candidates,
        deleted_candidate_count,
    })
}

/// Closed Forge maintenance strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeTaskStrategy {
    /// Fold staging files.
    StagingFold,
    /// Compact small files.
    SmallFiles,
    /// Repair full table identity.
    FullIdentity,
    /// Rewrite manifests.
    ManifestRewrite,
    /// Expire snapshots.
    SnapshotExpiry,
    /// Delete metadata-derived expired files.
    ExpiredCleanup,
    /// Delete never-published outputs.
    OrphanCleanup,
}

impl ForgeTaskStrategy {
    /// Returns the stable SQL representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StagingFold => "staging_fold",
            Self::SmallFiles => "small_files",
            Self::FullIdentity => "full_identity",
            Self::ManifestRewrite => "manifest_rewrite",
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
            "staging_fold" => Ok(Self::StagingFold),
            "small_files" => Ok(Self::SmallFiles),
            "full_identity" => Ok(Self::FullIdentity),
            "manifest_rewrite" => Ok(Self::ManifestRewrite),
            "snapshot_expiry" => Ok(Self::SnapshotExpiry),
            "expired_cleanup" => Ok(Self::ExpiredCleanup),
            "orphan_cleanup" => Ok(Self::OrphanCleanup),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge task strategy {value}"),
            }),
        }
    }
}

/// Scheduler capacity lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeTaskLane {
    /// Normal bounded capacity.
    Ordinary,
    /// Cluster-wide singleton oversized capacity.
    LargeSingleton,
}
impl ForgeTaskLane {
    /// Returns the stable SQL representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::LargeSingleton => "large_singleton",
        }
    }
}
impl FromStr for ForgeTaskLane {
    type Err = SqlError;
    /// Parses a stored lane.
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] for an unknown value.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ordinary" => Ok(Self::Ordinary),
            "large_singleton" => Ok(Self::LargeSingleton),
            _ => Err(SqlError::InvariantViolation {
                detail: format!("unknown Forge task lane {value}"),
            }),
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
    /// Permanently exceeds capacity.
    Unschedulable,
    /// Permanently failed.
    Failed,
    /// Cancelled by an operator.
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
            Self::Unschedulable => "unschedulable",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    /// Reports whether this state is terminal and eligible for retention pruning.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Unschedulable | Self::Failed | Self::Cancelled
        )
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
            "unschedulable" => Ok(Self::Unschedulable),
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
    /// Planned parallelism.
    pub parallelism: u16,
    /// Peak memory bytes.
    pub memory_bytes: u64,
    /// Spill budget bytes.
    pub spill_bytes: u64,
    /// Large-lane ceiling bytes.
    pub large_ceiling_bytes: u64,
}

impl ForgeTaskEstimates {
    /// Validates that every persisted admission ceiling is positive and SQL-safe.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when any value is zero or exceeds `i64`.
    pub fn validate(self) -> Result<(), SqlError> {
        if self.files == 0
            || self.bytes == 0
            || self.parallelism == 0
            || self.memory_bytes == 0
            || self.spill_bytes == 0
            || self.large_ceiling_bytes == 0
            || i64::try_from(self.bytes).is_err()
            || i64::try_from(self.memory_bytes).is_err()
            || i64::try_from(self.spill_bytes).is_err()
            || i64::try_from(self.large_ceiling_bytes).is_err()
        {
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
    /// Capacity lane.
    pub lane: ForgeTaskLane,
    /// Snapshot used by the planner.
    pub base_snapshot_id: i64,
    /// Versioned exact plan.
    pub plan: ForgeTaskPlan,
    /// SHA-256 digest of canonical plan bytes.
    pub plan_hash: [u8; 32],
    /// Positive admission estimates.
    pub estimates: ForgeTaskEstimates,
    /// First eligible claim time.
    pub ready_at: DateTime<Utc>,
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
    /// Maintenance strategy.
    pub strategy: ForgeTaskStrategy,
    /// Capacity lane.
    pub lane: ForgeTaskLane,
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
    /// Publication or cleanup evidence.
    pub evidence: Option<ForgeTaskEvidence>,
    /// Next eligibility time.
    pub ready_at: DateTime<Utc>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last mutation time.
    pub updated_at: DateTime<Utc>,
}

/// Result returned to a worker after a successful claim.
pub type ForgeTaskClaim = ForgeTask;

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
    lane: String,
    base_snapshot_id: i64,
    plan: serde_json::Value,
    estimated_files: i64,
    estimated_bytes: i64,
    estimated_parallelism: i32,
    estimated_memory_bytes: i64,
    estimated_spill_bytes: i64,
    large_task_ceiling_bytes: i64,
    state: String,
    attempt_id: Option<Uuid>,
    claimed_by: Option<Uuid>,
    claim_expires_at: Option<DateTime<Utc>>,
    watermark_snapshot_id: Option<i64>,
    watermark_timestamp_ms: Option<i64>,
    evidence: Option<serde_json::Value>,
    ready_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
        let plan = plan_from_value(row.plan)?;
        plan.validate(true)?;
        let evidence = row.evidence.map(evidence_from_value).transpose()?;
        if let Some(value) = &evidence {
            value.validate(true)?;
            value
                .validate_for_table(&table_ref)
                .map_err(|_| SqlError::InvariantViolation {
                    detail: "persisted Forge cleanup evidence is bound to another table".to_owned(),
                })?;
        }
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
            parallelism: u16::try_from(row.estimated_parallelism).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "invalid estimated_parallelism".to_owned(),
                }
            })?,
            memory_bytes: u64::try_from(row.estimated_memory_bytes).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "invalid estimated_memory_bytes".to_owned(),
                }
            })?,
            spill_bytes: u64::try_from(row.estimated_spill_bytes).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "invalid estimated_spill_bytes".to_owned(),
                }
            })?,
            large_ceiling_bytes: u64::try_from(row.large_task_ceiling_bytes).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "invalid large ceiling".to_owned(),
                }
            })?,
        };
        estimates
            .validate()
            .map_err(|_| SqlError::InvariantViolation {
                detail: "invalid persisted Forge estimates".to_owned(),
            })?;
        Ok(Self {
            task_id: row.task_id,
            data_tenant_id,
            table_ref,
            strategy: row.strategy.parse()?,
            lane: row.lane.parse()?,
            base_snapshot_id: row.base_snapshot_id,
            plan,
            estimates,
            state: row.state.parse()?,
            attempt_id: row.attempt_id,
            claimed_by: row.claimed_by,
            claim_expires_at: row.claim_expires_at,
            watermark,
            evidence,
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
