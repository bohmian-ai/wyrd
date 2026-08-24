//! `file_list` writer — atomic INSERT + audit fan-out per CONTRACTS §11.

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_sql::OperatorPool;
use vala_sql::{SqlError, TenantConn};

use crate::scribe::stream_identity::StreamIdentity;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::ParquetEncoded;

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

/// Result of a file-list write or an already-validated replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListInsertOutcome {
    /// Durable `vala.file_list` row identity.
    pub id: Uuid,
    /// Full identity validated against the durable row.
    pub commit_key: FileListCommitKey,
    /// Whether the unique replay key already existed.
    pub replayed: bool,
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
    let first_ordinal = encoded
        .artifacts
        .first()
        .map_or(0_usize, |artifact| usize::from(artifact.ordinal));
    encoded
        .artifacts
        .iter()
        .enumerate()
        .map(|(offset, artifact)| {
            if usize::from(artifact.ordinal) != first_ordinal.saturating_add(offset)
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
            Ok(FileListArtifactInsert {
                id: artifact_row_id(&artifact.object_identity),
                data_tenant_id: binding.tenant,
                namespace: binding.logical_namespace.clone(),
                table_name: binding.table_name.clone(),
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
                partition: encoded.partition,
                node_id,
                writer_epoch,
                wal_lsn_min,
                wal_lsn_max,
                file_ordinal: i16::try_from(artifact.ordinal).map_err(|_| {
                    ScribeError::Internal {
                        detail: "writer-v2 ordinal exceeds smallint".to_owned(),
                    }
                })?,
                file_checksum: artifact.checksum.clone(),
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

/// File-list INSERT row matching the `vala.file_list` columns.
pub struct FileListInsert<'a> {
    pub id: Uuid,
    pub data_tenant_id: DataTenantId,
    pub namespace: &'a str,
    pub table_name: &'a str,
    pub file_path: &'a str,
    pub file_size: i64,
    pub row_count: i64,
    pub min_event_time: chrono::DateTime<chrono::Utc>,
    pub max_event_time: chrono::DateTime<chrono::Utc>,
    /// Exact time partition inherited from the generation seal key.
    pub partition: TimePartition,
    pub node_id: Uuid,
    pub writer_epoch: i64,
    pub wal_lsn_min: i64,
    pub wal_lsn_max: i64,
}

impl FileListInsert<'_> {
    /// Return the full commit identity for this row.
    #[must_use]
    pub fn commit_key(&self) -> FileListCommitKey {
        FileListCommitKey {
            data_tenant_id: self.data_tenant_id,
            namespace: self.namespace.to_owned(),
            table_name: self.table_name.to_owned(),
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            wal_lsn_min: self.wal_lsn_min,
            wal_lsn_max: self.wal_lsn_max,
        }
    }

    /// Return the unique replay key for this row.
    #[must_use]
    pub const fn conflict_key(&self) -> FileListConflictKey {
        FileListConflictKey {
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            wal_lsn_min: self.wal_lsn_min,
            wal_lsn_max: self.wal_lsn_max,
            file_ordinal: 0,
        }
    }
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

/// Extract min/max event time from row group stats.
fn extract_event_time_range(
    encoded: &ParquetEncoded,
) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let epoch = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
    match encoded.row_group_stats.first() {
        Some(rg) => (
            rg.min_event_time
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch),
            rg.max_event_time
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch),
        ),
        None => (epoch, epoch),
    }
}

/// Build the `FileListInsert` row from freeze metadata and its canonical binding.
///
/// Persistence workers may move the encoded bytes into object storage before
/// constructing the SQL row; `file_size_override` preserves the original byte
/// count in that path. The direct seal path passes `None` and derives the size
/// from the retained payload.
///
/// # Errors
/// Returns [`ScribeError`] when the binding does not match the frozen seal key,
/// metadata cannot be represented in the SQL types, or the writer identity is
/// not a UUID.
pub fn build_insert<'a>(
    frozen: &'a FrozenMemtable,
    encoded: &'a ParquetEncoded,
    binding: &'a TenantTableBinding,
    node_id: &str,
    writer_epoch: i64,
    file_path: &'a str,
    file_size_override: Option<usize>,
) -> Result<FileListInsert<'a>, ScribeError> {
    if binding.tenant != frozen.seal_key.tenant || binding.table_ref != frozen.seal_key.table {
        return Err(ScribeError::Internal {
            detail: format!(
                "tenant-table binding does not match seal key: binding tenant/table=({},{}) seal tenant/table=({},{})",
                binding.tenant, binding.table_ref, frozen.seal_key.tenant, frozen.seal_key.table
            ),
        });
    }

    let (wal_lsn_min, wal_lsn_max) = extract_lsn_range(encoded)?;
    let (min_event_time, max_event_time) = extract_event_time_range(encoded);

    // Row count and file size are usize; the Postgres columns are BIGINT (i64).
    // A single Parquet file cannot approach i64::MAX rows or bytes on any real
    // machine, so a failed conversion is an invariant violation.
    let row_count = i64::try_from(frozen.row_count()).map_err(|_| ScribeError::Internal {
        detail: "row_count exceeds i64::MAX (invariant violation)".to_string(),
    })?;
    let encoded_size = encoded
        .artifacts
        .iter()
        .try_fold(0_usize, |sum, artifact| {
            sum.checked_add(usize::try_from(artifact.file_size).ok()?)
        })
        .ok_or_else(|| ScribeError::Internal {
            detail: "writer-v2 artifact-set size overflows".to_owned(),
        })?;
    let file_size = i64::try_from(file_size_override.unwrap_or(encoded_size)).map_err(|_| {
        ScribeError::Internal {
            detail: "file_size exceeds i64::MAX (invariant violation)".to_string(),
        }
    })?;

    let node_uuid = Uuid::parse_str(node_id).map_err(|e| ScribeError::Internal {
        detail: format!("invalid node_id UUID: {e}"),
    })?;

    Ok(FileListInsert {
        id: Uuid::now_v7(),
        data_tenant_id: binding.tenant,
        namespace: &binding.logical_namespace,
        table_name: &binding.table_name,
        file_path,
        file_size,
        row_count,
        min_event_time,
        max_event_time,
        partition: encoded.partition,
        node_id: node_uuid,
        writer_epoch,
        wal_lsn_min,
        wal_lsn_max,
    })
}

/// Validate and return the row already stored for a replay conflict.
async fn validate_replay(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    commit_key: FileListCommitKey,
) -> Result<FileListInsertOutcome, SqlError> {
    let conflict = row.conflict_key();
    let existing: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        r"
 SELECT id, data_tenant_id, namespace, table_name
   FROM vala.file_list
  WHERE node_id = $1
    AND writer_epoch = $2
    AND wal_lsn_min = $3
    AND wal_lsn_max = $4
    AND file_ordinal = $5
  FOR UPDATE
 ",
    )
    .bind(conflict.node_id)
    .bind(conflict.writer_epoch)
    .bind(conflict.wal_lsn_min)
    .bind(conflict.wal_lsn_max)
    .bind(conflict.file_ordinal)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    let Some((id, data_tenant_id, namespace, table_name)) = existing else {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list replay conflict has no RLS-visible row for stream {}:{}:{}-{}",
                conflict.node_id, conflict.writer_epoch, conflict.wal_lsn_min, conflict.wal_lsn_max
            ),
        });
    };

    if data_tenant_id != row.data_tenant_id.as_uuid()
        || namespace != row.namespace
        || table_name != row.table_name
    {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list replay conflict identity mismatch: expected ({},{},{}) found ({},{},{})",
                row.data_tenant_id,
                row.namespace,
                row.table_name,
                data_tenant_id,
                namespace,
                table_name
            ),
        });
    }

    tracing::debug!(
        node_id = %row.node_id,
        writer_epoch = row.writer_epoch,
        wal_lsn_min = row.wal_lsn_min,
        wal_lsn_max = row.wal_lsn_max,
        "replay-driven re-seal: validated existing file_list row; skipping audit fan-out",
    );
    Ok(FileListInsertOutcome {
        id,
        commit_key,
        replayed: true,
    })
}

/// Publishes one recovered or live generation while holding the replacement
/// Scribe actor's exact membership fence in the same operator transaction.
///
/// The actor fence authorizes the mutation only. The supplied row continues to
/// carry the source WAL stream and LSN range used for replay idempotency.
///
/// # Errors
/// Returns [`SqlError`] when the actor fence is stale, publication conflicts
/// with a different identity, audit append fails, or the transaction cannot
/// commit.
pub async fn insert_and_audit_fenced(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<FileListInsertOutcome, SqlError> {
    insert_and_audit_fenced_inner(
        operator_pool,
        actor,
        row,
        events,
        false,
        #[cfg(any(test, feature = "test-support"))]
        None,
    )
    .await
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
             (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,file_ordinal,file_checksum) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
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

/// Publishes through the legacy single-row transaction with a pre-mutation barrier.
///
/// # Errors
/// Returns the same fence, replay, audit, SQL, or commit errors as
/// [`insert_and_audit_fenced`].
#[cfg(any(test, feature = "test-support"))]
pub async fn insert_and_audit_fenced_with_barrier(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
    barrier: &PublicationFenceBarrier,
) -> Result<FileListInsertOutcome, SqlError> {
    insert_and_audit_fenced_inner(operator_pool, actor, row, events, false, Some(barrier)).await
}

/// Own the one atomic fenced publication transaction and optional test rollback.
///
/// # Errors
///
/// Returns fence, replay, audit, SQL, or injected pre-commit failures; every
/// error drops the transaction without publishing partial durable state.
async fn insert_and_audit_fenced_inner(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
    fail_before_commit: bool,
    #[cfg(any(test, feature = "test-support"))] barrier: Option<&PublicationFenceBarrier>,
) -> Result<FileListInsertOutcome, SqlError> {
    let mut transaction = operator_pool.begin().await.map_err(SqlError::from)?;
    let actor_live: bool =
        sqlx::query_scalar("SELECT vala.assert_scribe_publication_fence($1, $2, $3)")
            .bind(wyrd_spec::ids::DataTenantId::SYSTEM_OWNER.as_uuid())
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
    #[cfg(any(test, feature = "test-support"))]
    if let Some(barrier) = barrier {
        barrier.pause_before_publication().await;
    }

    let result = insert_file(&mut transaction, row).await?;
    let commit_key = row.commit_key();
    let outcome = if result.rows_affected() == 0 {
        validate_replay_transaction(&mut transaction, row, commit_key).await?
    } else {
        let mut audit = vala_sql::queries::audit_outbox::OperatorAudit::new(
            row.data_tenant_id,
            &mut transaction,
        );
        for event in events {
            audit.append(event).await?;
        }
        FileListInsertOutcome {
            id: row.id,
            commit_key,
            replayed: false,
        }
    };
    if fail_before_commit {
        return Err(SqlError::InvariantViolation {
            detail: "test SQL commit failure".to_owned(),
        });
    }
    transaction.commit().await.map_err(SqlError::from)?;
    Ok(outcome)
}

/// Inserts the file-list row through an already-owned SQL transaction.
async fn insert_file(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    row: &FileListInsert<'_>,
) -> Result<sqlx::postgres::PgQueryResult, SqlError> {
    sqlx::query(
        r"INSERT INTO vala.file_list (
 id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,
 min_event_time,max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,wal_lsn_max)
 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
 ON CONFLICT (data_tenant_id,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,file_ordinal) DO NOTHING",
    )
    .bind(row.id)
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.namespace)
    .bind(row.table_name)
    .bind(row.file_path)
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
    .execute(&mut **transaction)
    .await
    .map_err(SqlError::from)
}

/// Validates an idempotent replay through an operator-owned transaction.
async fn validate_replay_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    row: &FileListInsert<'_>,
    commit_key: FileListCommitKey,
) -> Result<FileListInsertOutcome, SqlError> {
    let existing: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT id,data_tenant_id,namespace,table_name FROM vala.file_list \
         WHERE data_tenant_id=$1 AND node_id=$2 AND writer_epoch=$3 \
         AND wal_lsn_min=$4 AND wal_lsn_max=$5 AND file_ordinal=0 FOR UPDATE",
    )
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.node_id)
    .bind(row.writer_epoch)
    .bind(row.wal_lsn_min)
    .bind(row.wal_lsn_max)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(SqlError::from)?;
    let Some((id, tenant, namespace, table_name)) = existing else {
        return Err(SqlError::InvariantViolation {
            detail: "file_list replay conflict has no operator-visible row".to_owned(),
        });
    };
    if tenant != row.data_tenant_id.as_uuid()
        || namespace != row.namespace
        || table_name != row.table_name
    {
        return Err(SqlError::InvariantViolation {
            detail: "file_list replay conflict identity mismatch".to_owned(),
        });
    }
    Ok(FileListInsertOutcome {
        id,
        commit_key,
        replayed: true,
    })
}

/// Insert one `vala.file_list` row + N `vala.audit_outbox` rows in one transaction.
///
/// The insert uses `FileListConflictKey` for replay detection. A conflict is
/// successful only after the existing row's complete `FileListCommitKey` is
/// visible under the same RLS-bound transaction and matches the attempted row.
///
/// # Errors
/// Returns [`vala_sql::SqlError`] on transaction failure or an unvalidated replay.
pub async fn insert_and_audit(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<FileListInsertOutcome, SqlError> {
    if row.data_tenant_id != conn.data_tenant_id() {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list tenant mismatch: row tenant `{}` does not match TenantConn tenant `{}`",
                row.data_tenant_id,
                conn.data_tenant_id()
            ),
        });
    }

    let result = sqlx::query(
        r"
 INSERT INTO vala.file_list (
 id,
 data_tenant_id,
 namespace,
 table_name,
 file_path,
 file_size,
 row_count,
 min_event_time,
 max_event_time,
 partition_granularity,
 partition_start,
 node_id,
 writer_epoch,
 wal_lsn_min,
 wal_lsn_max
 )
 VALUES (
 $1,
 $2,
 $3,
 $4,
 $5,
 $6,
 $7,
 $8,
 $9,
 $10,
 $11,
 $12,
 $13,
 $14,
 $15
 )
 ON CONFLICT (data_tenant_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, file_ordinal) DO NOTHING
 ",
    )
    .bind(row.id)
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.namespace)
    .bind(row.table_name)
    .bind(row.file_path)
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
    .execute(&mut **conn.transaction())
    .await?;

    let commit_key = row.commit_key();
    if result.rows_affected() == 0 {
        return validate_replay(conn, row, commit_key).await;
    }

    for event in events {
        vala_sql::queries::audit_outbox::append_audit(conn, event).await?;
    }

    Ok(FileListInsertOutcome {
        id: row.id,
        commit_key,
        replayed: false,
    })
}

/// Stages a complete writer-v2 artifact set in one caller-owned tenant transaction.
///
/// The caller owns the eventual COMMIT result. This method performs no commit
/// and therefore cannot classify the outcome as known or unknown.
///
/// # Errors
/// Returns a tenant, contiguity, replay-identity, audit, or SQL error without
/// leaving any mutation outside the caller's transaction.
pub async fn insert_artifact_set_and_audit(
    conn: &mut TenantConn<'_>,
    rows: &[FileListArtifactInsert],
    events: &[AuditEvent],
) -> Result<FileListArtifactSetOutcome, SqlError> {
    let first = validate_artifact_set(
        rows,
        Some(conn.data_tenant_id()),
        "writer-v2 tenant publication set is empty",
        "writer-v2 tenant publication set crosses identity boundaries",
    )?;
    let mut inserted = 0_u64;
    for row in rows {
        inserted += sqlx::query(
            "INSERT INTO vala.file_list \
             (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,file_ordinal,file_checksum) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
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
        .execute(&mut **conn.transaction())
        .await?
        .rows_affected();
    }
    if inserted != 0 && inserted != u64::try_from(rows.len()).unwrap_or(u64::MAX) {
        return Err(SqlError::InvariantViolation {
            detail: "writer-v2 tenant publication observed a partial replay set".to_owned(),
        });
    }
    let existing: Vec<ExistingArtifactRow> = sqlx::query_as(
        "SELECT file_ordinal,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,file_checksum FROM vala.file_list \
         WHERE data_tenant_id=wyrd.current_tenant() AND node_id=$1 AND writer_epoch=$2 AND wal_lsn_min=$3 AND wal_lsn_max=$4 \
         ORDER BY file_ordinal FOR UPDATE",
    )
    .bind(first.node_id)
    .bind(first.writer_epoch)
    .bind(first.wal_lsn_min)
    .bind(first.wal_lsn_max)
    .fetch_all(&mut **conn.transaction())
    .await?;
    if !artifact_replay_matches(&existing, rows) {
        return Err(SqlError::InvariantViolation {
            detail: "writer-v2 tenant replay identity does not match the complete set".to_owned(),
        });
    }
    if inserted > 0 {
        for event in events {
            vala_sql::queries::audit_outbox::append_audit(conn, event).await?;
        }
    }
    artifact_set_outcome(first, rows, inserted)
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
