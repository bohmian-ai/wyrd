//! `file_list` writer — atomic INSERT + audit fan-out per CONTRACTS §11.

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_sql::OperatorPool;
use vala_sql::SqlError;

use crate::scribe::stream_identity::StreamIdentity;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::{BIFROST_PARTITION_SPEC_ID, BIFROST_SORT_ORDER_ID, TimePartition};
use crate::contracts::ScribeError;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::ParquetEncoded;
use crate::scribe::promotion::{PublishedHotFileIdentity, ScribePublishedHotFileV1};

/// The unique replay key on `vala.file_list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileListConflictKey {
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing Scribe writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
    /// Zero-based artifact position within the generation.
    pub file_ordinal: i16,
}

/// The full identity that must match when a conflict key is replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListCommitKey {
    /// Organization stamped on the physical Parquet file.
    pub data_tenant_id: DataTenantId,
    /// Logical namespace stored in `vala.file_list`.
    pub namespace: String,
    /// Local logical table name.
    pub table_name: String,
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing Scribe writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
}

/// Production-only writer-v2 file-list row with explicit artifact identity.
#[derive(Debug, Clone)]
pub struct FileListArtifactInsert {
    /// Durable row identity derived once for this publication attempt.
    pub id: Uuid,
    /// Tenant owning the complete generation.
    pub data_tenant_id: DataTenantId,
    /// Logical namespace stored in `vala.file_list`.
    pub namespace: String,
    /// Logical table name stored in `vala.file_list`.
    pub table_name: String,
    /// Deterministic object identity stamped in the Parquet footer.
    pub file_path: String,
    /// Exact sealed artifact size.
    pub file_size: i64,
    /// Rows represented by this artifact.
    pub row_count: i64,
    /// Minimum event timestamp for this artifact.
    pub min_event_time: chrono::DateTime<chrono::Utc>,
    /// Maximum event timestamp for this artifact.
    pub max_event_time: chrono::DateTime<chrono::Utc>,
    /// Exact time partition inherited from the generation seal key.
    pub partition: TimePartition,
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
    /// Contiguous zero-based artifact ordinal.
    pub file_ordinal: i16,
    /// Exact lowercase SHA-256 checksum.
    pub file_checksum: String,
    /// Iceberg-ready promotion evidence derived from this object's footer.
    pub promotion_record: ScribePublishedHotFileV1,
}

/// Complete identity returned by an atomic writer-v2 set publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListArtifactSetOutcome {
    /// Existing generation-scoped commit identity.
    pub commit_key: FileListCommitKey,
    /// Positive artifact count validated in the transaction.
    pub artifact_count: u16,
    /// SHA-256 digest of ordered ordinal/path/size/checksum identities.
    pub artifact_set_digest: String,
    /// Whether every row already existed and matched exactly.
    pub replayed: bool,
}

/// Database projection used to validate one ordered writer-v2 artifact row.
type ExistingArtifactRow = (
    i16,
    String,
    String,
    String,
    i64,
    i64,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    String,
    chrono::DateTime<chrono::Utc>,
    Option<String>,
);

/// Validates that rows form one contiguous generation-scoped artifact set.
///
/// # Errors
///
/// Returns an invariant error when the set is empty, crosses a generation
/// boundary, contains a malformed artifact, or belongs to another tenant.
fn validate_artifact_set<'a>(
    rows: &'a [FileListArtifactInsert],
    tenant: Option<DataTenantId>,
    empty_detail: &'static str,
    invalid_detail: &'static str,
) -> Result<&'a FileListArtifactInsert, SqlError> {
    let first = rows.first().ok_or_else(|| SqlError::InvariantViolation {
        detail: empty_detail.to_owned(),
    })?;
    if tenant.is_some_and(|tenant| tenant != first.data_tenant_id)
        || rows.iter().enumerate().any(|(ordinal, row)| {
            usize::try_from(row.file_ordinal).ok() != Some(ordinal)
                || row.data_tenant_id != first.data_tenant_id
                || row.node_id != first.node_id
                || row.writer_epoch != first.writer_epoch
                || row.wal_lsn_min != first.wal_lsn_min
                || row.wal_lsn_max != first.wal_lsn_max
                || !valid_artifact_identity(row)
        })
    {
        return Err(SqlError::InvariantViolation {
            detail: invalid_detail.to_owned(),
        });
    }
    Ok(first)
}

/// Confirms that selected database rows match the complete requested set.
fn artifact_replay_matches(
    existing: &[ExistingArtifactRow],
    rows: &[FileListArtifactInsert],
) -> bool {
    existing.len() == rows.len()
        && existing.iter().zip(rows).all(|(actual, expected)| {
            actual.0 == expected.file_ordinal
                && actual.1 == expected.namespace
                && actual.2 == expected.table_name
                && actual.3 == expected.file_path
                && actual.4 == expected.file_size
                && actual.5 == expected.row_count
                && actual.6 == expected.min_event_time
                && actual.7 == expected.max_event_time
                && actual.8 == expected.partition.granularity_str()
                && actual.9 == expected.partition.start_utc()
                && actual.10.as_deref() == Some(expected.file_checksum.as_str())
        })
}

/// Builds the deterministic publication outcome after SQL identity validation.
///
/// # Errors
///
/// Returns an invariant error when the artifact count exceeds its durable
/// `u16` representation.
fn artifact_set_outcome(
    first: &FileListArtifactInsert,
    rows: &[FileListArtifactInsert],
    inserted: u64,
) -> Result<FileListArtifactSetOutcome, SqlError> {
    let mut digest = Sha256::new();
    for row in rows {
        digest.update(row.file_ordinal.to_be_bytes());
        digest.update((row.file_path.len() as u64).to_be_bytes());
        digest.update(row.file_path.as_bytes());
        digest.update(row.file_size.to_be_bytes());
        digest.update(row.file_checksum.as_bytes());
    }
    Ok(FileListArtifactSetOutcome {
        commit_key: FileListCommitKey {
            data_tenant_id: first.data_tenant_id,
            namespace: first.namespace.clone(),
            table_name: first.table_name.clone(),
            node_id: first.node_id,
            writer_epoch: first.writer_epoch,
            wal_lsn_min: first.wal_lsn_min,
            wal_lsn_max: first.wal_lsn_max,
        },
        artifact_count: u16::try_from(rows.len()).map_err(|_| SqlError::InvariantViolation {
            detail: "writer-v2 artifact count exceeds u16".to_owned(),
        })?,
        artifact_set_digest: hex::encode(digest.finalize()),
        replayed: inserted == 0,
    })
}

/// Builds the ordered writer-v2 SQL rows for one sealed artifact set.
///
/// # Errors
/// Returns a Scribe invariant error for an empty/noncontiguous set, malformed
/// checksum, identity mismatch, or values outside `PostgreSQL` integer domains.
pub fn build_artifact_inserts(
    frozen: &FrozenMemtable,
    encoded: &ParquetEncoded,
    binding: &TenantTableBinding,
    node_id: &str,
    writer_epoch: i64,
) -> Result<Vec<FileListArtifactInsert>, ScribeError> {
    if encoded.artifacts.is_empty()
        || binding.tenant != frozen.seal_key.tenant
        || binding.table_ref != frozen.seal_key.table
    {
        return Err(ScribeError::Internal {
            detail: "writer-v2 artifact set does not match its frozen generation".to_owned(),
        });
    }
    let node_id = Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
        detail: format!("invalid writer-v2 node identity: {error}"),
    })?;
    let (wal_lsn_min, wal_lsn_max) = extract_lsn_range(encoded)?;
    build_inserts(
        encoded.artifacts.iter(),
        &PublishedArtifactFacts {
            binding,
            partition: encoded.partition,
            node_id,
            writer_epoch,
            wal_lsn_min,
            wal_lsn_max,
        },
    )
}

/// Builds the ordered writer-v2 SQL rows for one assembled claim's objects.
///
/// A claim has no frozen generation behind it: its identity comes from the
/// assembly key every contributing member shares, and its WAL range is the
/// union of the members' ranges, which is exactly the span the published object
/// makes retirable.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] for an empty or noncontiguous object set,
/// a malformed checksum, an assembly key whose tenant or table is not the
/// binding's, a reversed WAL range, or values outside `PostgreSQL` integer
/// domains.
pub fn build_claim_inserts(
    key: &crate::scribe::assembly::ScribeAssemblyKey,
    artifacts: &crate::scribe::parquet_writer::BoundedParquetArtifactSet,
    binding: &TenantTableBinding,
    wal: crate::scribe::hot_stage::StagedLsnRange,
) -> Result<Vec<FileListArtifactInsert>, ScribeError> {
    if artifacts.is_empty() || binding.tenant != key.tenant() || binding.table_ref != *key.table() {
        return Err(ScribeError::Internal {
            detail: "assembled claim objects do not match their assembly key".to_owned(),
        });
    }
    if wal.min > wal.max {
        return Err(ScribeError::Internal {
            detail: "assembled claim carries a reversed WAL range".to_owned(),
        });
    }
    let wal_lsn_min = i64::try_from(wal.min).map_err(|_| ScribeError::Internal {
        detail: "assembled claim wal_lsn_min exceeds bigint".to_owned(),
    })?;
    let wal_lsn_max = i64::try_from(wal.max).map_err(|_| ScribeError::Internal {
        detail: "assembled claim wal_lsn_max exceeds bigint".to_owned(),
    })?;
    build_inserts(
        artifacts.iter(),
        &PublishedArtifactFacts {
            binding,
            partition: key.partition(),
            node_id: key.node_id().as_uuid(),
            writer_epoch: key.writer_epoch().as_i64(),
            wal_lsn_min,
            wal_lsn_max,
        },
    )
}

/// Publication facts every object of one set shares.
///
/// These are the columns that describe *where* the rows came from rather than
/// what one object contains, so they are resolved once and applied to every row
/// of the set; a per-artifact copy would let two objects of one publication
/// disagree about their own origin.
struct PublishedArtifactFacts<'a> {
    /// Tenant-qualified physical binding the rows are published under.
    binding: &'a TenantTableBinding,
    /// Physical partition the objects belong to.
    partition: crate::catalog::layout::TimePartition,
    /// Node whose writer produced the rows.
    node_id: Uuid,
    /// Fenced writer epoch that produced the rows.
    writer_epoch: i64,
    /// Inclusive lower WAL bound the objects make retirable.
    wal_lsn_min: i64,
    /// Inclusive upper WAL bound the objects make retirable.
    wal_lsn_max: i64,
}

/// Builds one contiguous set's rows from its objects and shared facts.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when ordinals are noncontiguous, a
/// checksum is not lowercase hex of the expected length, or a value is outside
/// a `PostgreSQL` integer domain.
fn build_inserts<'a>(
    artifacts: impl Iterator<Item = &'a crate::scribe::parquet_writer::BoundedParquetArtifact>,
    facts: &PublishedArtifactFacts<'_>,
) -> Result<Vec<FileListArtifactInsert>, ScribeError> {
    let mut first_ordinal = None;
    artifacts
        .enumerate()
        .map(|(offset, artifact)| {
            let first = *first_ordinal.get_or_insert(usize::from(artifact.ordinal));
            if usize::from(artifact.ordinal) != first.saturating_add(offset)
                || artifact.checksum.len() != 64
                || !artifact
                    .checksum
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(ScribeError::Internal {
                    detail: "writer-v2 artifact identity is malformed or noncontiguous".to_owned(),
                });
            }
            let epoch = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
            let min_event_time = artifact
                .row_group_stats
                .iter()
                .filter_map(|stats| stats.min_event_time)
                .min()
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch);
            let max_event_time = artifact
                .row_group_stats
                .iter()
                .filter_map(|stats| stats.max_event_time)
                .max()
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch);
            let row_id = artifact_row_id(&artifact.object_identity);
            Ok(FileListArtifactInsert {
                id: row_id,
                data_tenant_id: facts.binding.tenant,
                namespace: facts.binding.logical_namespace.clone(),
                table_name: facts.binding.table_name.clone(),
                file_path: artifact.object_identity.clone(),
                file_size: i64::try_from(artifact.file_size).map_err(|_| {
                    ScribeError::Internal {
                        detail: "writer-v2 file size exceeds bigint".to_owned(),
                    }
                })?,
                row_count: i64::try_from(artifact.row_count).map_err(|_| {
                    ScribeError::Internal {
                        detail: "writer-v2 row count exceeds bigint".to_owned(),
                    }
                })?,
                min_event_time,
                max_event_time,
                partition: facts.partition,
                node_id: facts.node_id,
                writer_epoch: facts.writer_epoch,
                wal_lsn_min: facts.wal_lsn_min,
                wal_lsn_max: facts.wal_lsn_max,
                file_ordinal: i16::try_from(artifact.ordinal).map_err(|_| {
                    ScribeError::Internal {
                        detail: "writer-v2 ordinal exceeds smallint".to_owned(),
                    }
                })?,
                file_checksum: artifact.checksum.clone(),
                promotion_record: ScribePublishedHotFileV1::from_metrics(
                    &PublishedHotFileIdentity {
                        data_tenant_id: facts.binding.tenant.as_uuid(),
                        namespace: &facts.binding.logical_namespace,
                        table_name: &facts.binding.table_name,
                        file_list_id: row_id,
                        object_key: &artifact.object_identity,
                        file_checksum: &artifact.checksum,
                        partition: facts.partition,
                        partition_spec_id: BIFROST_PARTITION_SPEC_ID,
                        sort_order_id: BIFROST_SORT_ORDER_ID,
                    },
                    artifact.data_file_metrics.clone(),
                ),
            })
        })
        .collect()
}

/// Derives the stable catalog row identity owned by one deterministic artifact.
///
/// Publication retries rebuild compact SQL rows from the retained immutable
/// generation. The row identifier must therefore remain identical to the
/// elected local publication manifest instead of introducing attempt identity.
fn artifact_row_id(object_identity: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"wyrd.scribe.file-list-row.v1\0");
    hasher.update(object_identity.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Extract LSN range from append metadata.
///
/// LSN values are u64 but real-world Postgres LSNs fit in i64 (PG column is BIGINT).
fn extract_lsn_range(encoded: &ParquetEncoded) -> Result<(i64, i64), ScribeError> {
    let min = encoded
        .append_metas
        .iter()
        .map(|m| m.wal_lsn_min.as_u64())
        .min()
        .unwrap_or(0);
    let max = encoded
        .append_metas
        .iter()
        .map(|m| m.wal_lsn_max.as_u64())
        .max()
        .unwrap_or(0);

    let min_i = i64::try_from(min).map_err(|_| ScribeError::Internal {
        detail: format!("wal_lsn_min {min} exceeds i64::MAX (invariant violation)"),
    })?;
    let max_i = i64::try_from(max).map_err(|_| ScribeError::Internal {
        detail: format!("wal_lsn_max {max} exceeds i64::MAX (invariant violation)"),
    })?;

    Ok((min_i, max_i))
}

/// Atomically publishes a complete writer-v2 artifact set under one actor fence.
///
/// Either every contiguous row plus the generation's audit fan-out commits, or
/// none do. Replays succeed only when every ordered identity matches exactly.
///
/// # Errors
/// Returns a SQL invariant error for an empty/noncontiguous/mixed replay set,
/// a stale actor fence, identity mismatch, audit failure, or commit failure.
pub async fn insert_artifact_set_and_audit_fenced(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    rows: &[FileListArtifactInsert],
    events: &[AuditEvent],
) -> Result<FileListArtifactSetOutcome, SqlError> {
    let first = validate_artifact_set(
        rows,
        None,
        "writer-v2 publication set is empty",
        "writer-v2 publication set is noncontiguous or crosses generations",
    )?;
    let mut transaction = operator_pool.begin().await.map_err(SqlError::from)?;
    let actor_live: bool =
        sqlx::query_scalar("SELECT vala.assert_scribe_publication_fence($1, $2, $3)")
            .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
            .bind(actor.node_id.as_uuid())
            .bind(actor.writer_epoch.as_i64())
            .fetch_one(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
    if !actor_live {
        return Err(SqlError::InvariantViolation {
            detail: format!("Scribe publication fence lost for actor {actor}"),
        });
    }
    let mut inserted = 0_u64;
    for row in rows {
        inserted += sqlx::query(
            "INSERT INTO vala.file_list \
             (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,file_ordinal,file_checksum,promotion_record) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) \
             ON CONFLICT (data_tenant_id,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,file_ordinal) DO NOTHING",
        )
        .bind(row.id)
        .bind(row.data_tenant_id.as_uuid())
        .bind(&row.namespace)
        .bind(&row.table_name)
        .bind(&row.file_path)
        .bind(row.file_size)
        .bind(row.row_count)
        .bind(row.min_event_time)
        .bind(row.max_event_time)
        .bind(row.partition.granularity_str())
        .bind(row.partition.start_utc())
        .bind(row.node_id)
        .bind(row.writer_epoch)
        .bind(row.wal_lsn_min)
        .bind(row.wal_lsn_max)
        .bind(row.file_ordinal)
        .bind(&row.file_checksum)
        .bind(row.promotion_record.to_json().map_err(|error| {
            SqlError::InvariantViolation {
                detail: format!("writer-v2 promotion record does not encode: {error}"),
            }
        })?)
        .execute(&mut *transaction)
        .await
        .map_err(SqlError::from)?
        .rows_affected();
    }
    if inserted != 0 && inserted != u64::try_from(rows.len()).unwrap_or(u64::MAX) {
        return Err(SqlError::InvariantViolation {
            detail: "writer-v2 publication observed a partial replay set".to_owned(),
        });
    }
    let existing: Vec<ExistingArtifactRow> = sqlx::query_as(
        "SELECT file_ordinal,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,file_checksum FROM vala.file_list \
         WHERE data_tenant_id=$1 AND node_id=$2 AND writer_epoch=$3 AND wal_lsn_min=$4 AND wal_lsn_max=$5 \
         ORDER BY file_ordinal FOR UPDATE",
    )
    .bind(first.data_tenant_id.as_uuid())
    .bind(first.node_id)
    .bind(first.writer_epoch)
    .bind(first.wal_lsn_min)
    .bind(first.wal_lsn_max)
    .fetch_all(&mut *transaction)
    .await
    .map_err(SqlError::from)?;
    if !artifact_replay_matches(&existing, rows) {
        return Err(SqlError::InvariantViolation {
            detail: "writer-v2 replay identity does not match the complete artifact set".to_owned(),
        });
    }
    if inserted > 0 {
        let mut audit = vala_sql::queries::audit_outbox::OperatorAudit::new(
            first.data_tenant_id,
            &mut transaction,
        );
        for event in events {
            audit.append(event).await?;
        }
    }
    transaction.commit().await.map_err(SqlError::from)?;
    artifact_set_outcome(first, rows, inserted)
}

/// Deterministic test barrier spanning the two sides of durable publication.
///
/// The first phase pauses after Scribe has persisted its local publication
/// manifest but before the fenced file-list transaction starts. The second
/// phase pauses after that transaction is visible but before the persistence
/// worker may advance its WAL manifest, clean local stages, or notify the shard
/// owner to retire immutable state.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
pub struct PublicationFenceBarrier {
    /// Optional logical table selecting which concurrent cohort member pauses.
    target_table: Option<String>,
    /// Signals that the durable local publication manifest is query-pinnable.
    before_publication: std::sync::Arc<tokio::sync::Notify>,
    /// Releases the reconciler to enter the fenced SQL transaction.
    release_before_publication: std::sync::Arc<tokio::sync::Notify>,
    /// Signals that file-list and audit publication are durably visible.
    after_publication: std::sync::Arc<tokio::sync::Notify>,
    /// Releases post-publication manifest advancement and local cleanup.
    release_after_publication: std::sync::Arc<tokio::sync::Notify>,
}

#[cfg(any(test, feature = "test-support"))]
impl PublicationFenceBarrier {
    /// Creates one single-use two-phase publication fence barrier.
    #[must_use]
    pub fn new() -> Self {
        Self {
            target_table: None,
            before_publication: std::sync::Arc::new(tokio::sync::Notify::new()),
            release_before_publication: std::sync::Arc::new(tokio::sync::Notify::new()),
            after_publication: std::sync::Arc::new(tokio::sync::Notify::new()),
            release_after_publication: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Creates a barrier selecting the next publication for one logical table.
    #[must_use]
    pub fn for_table(table_name: impl Into<String>) -> Self {
        Self {
            target_table: Some(table_name.into()),
            ..Self::new()
        }
    }

    /// Reports whether one writer-v2 artifact set is the selected publication.
    pub(crate) fn matches(&self, rows: &[FileListArtifactInsert]) -> bool {
        self.target_table.as_ref().is_none_or(|table_name| {
            rows.first()
                .is_some_and(|row| row.table_name == *table_name)
        })
    }

    /// Waits until the local publication manifest is durable but SQL is absent.
    pub async fn wait_before_publication(&self) {
        self.before_publication.notified().await;
    }

    /// Releases the reconciler to perform fenced file-list publication.
    pub fn release_before_publication(&self) {
        self.release_before_publication.notify_one();
    }

    /// Waits until SQL publication is visible but local immutable state remains.
    pub async fn wait_after_publication(&self) {
        self.after_publication.notified().await;
    }

    /// Releases manifest advancement, local cleanup, and shard completion.
    pub fn release_after_publication(&self) {
        self.release_after_publication.notify_one();
    }

    /// Pauses the production reconciler before it starts fenced SQL publication.
    pub(crate) async fn pause_before_publication(&self) {
        self.before_publication.notify_one();
        self.release_before_publication.notified().await;
    }

    /// Pauses the production reconciler after SQL commit and before local cleanup.
    pub(crate) async fn pause_after_publication(&self) {
        self.after_publication.notify_one();
        self.release_after_publication.notified().await;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Default for PublicationFenceBarrier {
    /// Create the default single-use publication fence barrier.
    fn default() -> Self {
        Self::new()
    }
}

/// Validates one writer-v2 object's nonempty physical and checksum identity.
fn valid_artifact_identity(row: &FileListArtifactInsert) -> bool {
    row.file_size > 0
        && row.row_count > 0
        && !row.namespace.is_empty()
        && !row.table_name.is_empty()
        && !row.file_path.is_empty()
        && row.file_checksum.len() == 64
        && row
            .file_checksum
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::artifact_row_id;

    /// Proves retries reuse one row identity while distinct artifacts remain distinct.
    #[test]
    fn deterministic_artifact_rows_are_retry_stable_and_artifact_local() {
        let artifact = "tenant/table/day/member-0.parquet";
        assert_eq!(artifact_row_id(artifact), artifact_row_id(artifact));
        assert_ne!(
            artifact_row_id(artifact),
            artifact_row_id("tenant/table/day/member-1.parquet")
        );
    }
}
