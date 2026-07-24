//! Seal state machine — freeze → Parquet → PUT → PG tx → manifest → retire WAL.

use std::sync::Arc;

use opendal::{ErrorKind, Operator};
use tracing::{info, warn};
use uuid::Uuid;
use vala_sql::TenantConn;
use wyrd_spec::ids::PodId;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribePersistenceCpuResult,
};
use crate::scribe::file_list_writer;
use crate::scribe::file_list_writer::FileListCommitKey;
use crate::scribe::filename::seal_filename;
use crate::scribe::memtable::{FrozenMemtable, Memtable};
use crate::scribe::parquet_writer::ParquetEncoded;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::WalLsn;

/// Capability returned by `pre_commit` and consumed after the SQL transaction commits.
#[derive(Debug, Clone)]
pub struct PostCommitToken {
    /// Local immutable generation identity.
    pub seal_id: u64,
    /// Seal scope represented by the generation.
    pub seal_key: SealKey,
    /// Exact durable file-list identity.
    pub file_list_key: FileListCommitKey,
    /// Durable file-list row identity.
    pub file_list_row_id: Uuid,
    /// Inclusive minimum WAL LSN in the generation.
    pub wal_lsn_min: WalLsn,
    /// Inclusive maximum WAL LSN in the generation.
    pub wal_lsn_max: WalLsn,
    /// Arrow bytes transferred from the writable to immutable tier.
    pub memtable_bytes: usize,
}

/// A set of post-commit capabilities produced by one force-seal call.
#[derive(Debug, Clone)]
pub struct PostCommitBatch(pub Vec<PostCommitToken>);

impl From<PostCommitToken> for PostCommitBatch {
    fn from(token: PostCommitToken) -> Self {
        Self(vec![token])
    }
}

/// Handle returned by `pre_commit` to be completed after the caller commits.
#[derive(Debug, Clone)]
pub struct SealCommit {
    /// Post-commit lifecycle capability.
    pub token: PostCommitToken,
    /// Canonical binding used by the seal.
    pub binding: TenantTableBinding,
    /// Object-store path written during pre-commit.
    pub parquet_path: String,
}

/// Seal state machine for one seal-key.
///
/// States: `Freeze` → `WriteParquet` → `PutObject` → `AtomicPgTx` → `ManifestUpdate` → `RetireWal`
#[derive(Debug)]
pub struct SealDriver {
    operator: Arc<Operator>,
    persistence_cpu: ScribePersistenceCpuPool,
}

impl SealDriver {
    /// Construct a new `SealDriver` with the given opendal operator.
    #[must_use]
    pub fn new(operator: Arc<Operator>) -> Self {
        Self::new_with_lane(operator, ScribePersistenceCpuPool::new(1))
    }

    /// Construct a seal driver using the boot-owned persistence CPU lane.
    #[must_use]
    pub(crate) fn new_with_lane(
        operator: Arc<Operator>,
        persistence_cpu: ScribePersistenceCpuPool,
    ) -> Self {
        Self {
            operator,
            persistence_cpu,
        }
    }

    /// Execute seal stages 1-4 (Freeze → Parquet → PUT → PG tx) and return a
    /// post-commit capability. The caller owns commit/rollback and must
    /// explicitly complete or abort the capability afterward.
    ///
    /// # Errors
    /// Returns [`ScribeError`] if any pre-commit stage fails.
    #[tracing::instrument(skip(self, memtable, conn), fields(seal_key = %seal_key))]
    pub async fn pre_commit(
        &self,
        memtable: &Memtable,
        seal_key: &SealKey,
        binding: &TenantTableBinding,
        conn: &mut TenantConn<'_>,
        node_id: &str,
        writer_epoch: i64,
    ) -> Result<SealCommit, ScribeError> {
        binding
            .validate_authenticated_tenant(conn.data_tenant_id())
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        if binding.table_ref != seal_key.table || binding.tenant != seal_key.tenant {
            return Err(ScribeError::Internal {
                detail: format!(
                    "tenant-table binding does not match seal key: binding tenant/table=({},{}) seal tenant/table=({},{})",
                    binding.tenant, binding.table_ref, seal_key.tenant, seal_key.table
                ),
            });
        }

        // 1. Freeze
        info!("seal stage: Freeze");
        let freeze_started = std::time::Instant::now();
        let frozen = memtable.freeze(seal_key)?;
        Self::record(
            "freeze",
            freeze_started.elapsed(),
            frozen.batch.num_rows(),
            frozen.batch.get_array_memory_size(),
        );

        // 2. WriteParquet on the boot-owned persistence CPU lane.
        info!("seal stage: WriteParquet");
        let parquet_started = std::time::Instant::now();
        let encoded = self
            .encode_parquet(&frozen, binding, seal_key.tenant)
            .await?;
        Self::record(
            "parquet_encode",
            parquet_started.elapsed(),
            frozen.batch.num_rows(),
            encoded.bytes.len(),
        );

        // 3. PutObject
        info!("seal stage: PutObject");
        let put_started = std::time::Instant::now();
        let parquet_path = self.put_object(binding, &encoded, node_id).await?;
        Self::record(
            "object_store_put",
            put_started.elapsed(),
            frozen.batch.num_rows(),
            encoded.bytes.len(),
        );

        // 4. AtomicPgTx
        info!("seal stage: AtomicPgTx");
        let pg_started = std::time::Instant::now();
        let row = file_list_writer::build_insert(
            &frozen,
            &encoded,
            binding,
            node_id,
            writer_epoch,
            &parquet_path,
        )?;
        let insert_outcome = file_list_writer::insert_and_audit(conn, &row, &encoded.audit_events)
            .await
            .map_err(ScribeError::from)?;
        Self::record(
            "file_list_transaction",
            pg_started.elapsed(),
            frozen.batch.num_rows(),
            encoded.bytes.len(),
        );

        let wal_lsn_min = encoded
            .append_metas
            .iter()
            .map(|meta| meta.wal_lsn_min)
            .min()
            .unwrap_or_else(|| WalLsn::new(0));
        let wal_lsn_max = encoded
            .append_metas
            .iter()
            .map(|meta| meta.wal_lsn_max)
            .max()
            .unwrap_or_else(|| WalLsn::new(0));

        Ok(SealCommit {
            token: PostCommitToken {
                seal_id: frozen.seal_id,
                seal_key: seal_key.clone(),
                file_list_key: insert_outcome.commit_key,
                file_list_row_id: insert_outcome.id,
                wal_lsn_min,
                wal_lsn_max,
                memtable_bytes: frozen.batch.get_array_memory_size(),
            },
            binding: binding.clone(),
            parquet_path,
        })
    }

    async fn encode_parquet(
        &self,
        frozen: &FrozenMemtable,
        binding: &TenantTableBinding,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<ParquetEncoded, ScribeError> {
        match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::EncodeParquet {
                frozen: Box::new(frozen.clone()),
                binding: binding.clone(),
                tenant,
            })
            .await?
        {
            ScribePersistenceCpuResult::ParquetEncoded(encoded) => Ok(encoded),
            ScribePersistenceCpuResult::Prepared(_) => Err(ScribeError::Internal {
                detail: "persistence lane returned the wrong seal result".to_owned(),
            }),
            ScribePersistenceCpuResult::ReplayRestored => Err(ScribeError::Internal {
                detail: "persistence lane returned replay output during seal".to_owned(),
            }),
        }
    }

    fn record(stage: &str, elapsed: std::time::Duration, rows: usize, bytes: usize) {
        metrics::histogram!("bifrost_scribe_seal_stage_seconds", "stage" => stage.to_owned())
            .record(elapsed.as_secs_f64());
        metrics::counter!("bifrost_scribe_seal_rows_total", "stage" => stage.to_owned())
            .increment(u64::try_from(rows).unwrap_or(u64::MAX));
        metrics::counter!("bifrost_scribe_seal_bytes_total", "stage" => stage.to_owned())
            .increment(u64::try_from(bytes).unwrap_or(u64::MAX));
    }

    /// PUT the Parquet object to storage with retry on transient failures.
    ///
    /// # Errors
    /// Returns [`ScribeError::ObjectStorePutFailed`] on non-transient failures.
    async fn put_object(
        &self,
        binding: &TenantTableBinding,
        encoded: &ParquetEncoded,
        node_id: &str,
    ) -> Result<String, ScribeError> {
        let node_uuid = Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
            detail: format!("node_id is not a valid UUID: {error}"),
        })?;
        let pod_id = PodId::new(format!("pod-{}", node_uuid.simple())).map_err(|error| {
            ScribeError::Internal {
                detail: format!("derived node PodId is invalid: {error}"),
            }
        })?;
        let filename = seal_filename(&pod_id);
        let path = format!("{}/{}", binding.object_prefix, filename);

        // Retry policy: base 100 ms, cap 5 s, 5 attempts max, jitter enabled
        let mut attempt = 0;
        let max_attempts = 5;
        let base_delay_ms = 100;
        let max_delay_ms = 5000;

        loop {
            attempt += 1;

            // Wrap the write in a 30-second timeout
            let write_result = tokio::time::timeout(
                tokio::time::Duration::from_secs(30),
                self.operator.write(&path, encoded.bytes.clone()),
            )
            .await;

            match write_result {
                Ok(Ok(_)) => {
                    info!(path = %path, bytes = encoded.bytes.len(), "Parquet PUT succeeded");
                    return Ok(path);
                }
                Ok(Err(e)) => {
                    // Classify error for retry
                    let is_fail_fast = matches!(
                        e.kind(),
                        ErrorKind::NotFound
                            | ErrorKind::PermissionDenied
                            | ErrorKind::ConditionNotMatch
                            | ErrorKind::ConfigInvalid
                    );

                    let is_retriable = matches!(e.kind(), ErrorKind::RateLimited)
                        || (matches!(e.kind(), ErrorKind::Unexpected) && e.is_temporary());

                    if is_fail_fast || (!is_retriable) || attempt >= max_attempts {
                        return Err(ScribeError::ObjectStorePutFailed(e));
                    }

                    // Exponential backoff with jitter
                    let delay_ms = (base_delay_ms * 2_u64.pow(attempt - 1)).min(max_delay_ms);
                    let jitter = rand::random::<u64>() % (delay_ms / 10 + 1);
                    let actual_delay_ms = delay_ms + jitter;

                    warn!(
                    attempt = attempt,
                    delay_ms = actual_delay_ms,
                    error = %e,
                    "transient object store error, retrying"
                    );

                    tokio::time::sleep(tokio::time::Duration::from_millis(actual_delay_ms)).await;
                }
                Err(_) => {
                    // Timeout - treat as transient
                    if attempt >= max_attempts {
                        return Err(ScribeError::ObjectStorePutFailed(opendal::Error::new(
                            ErrorKind::Unexpected,
                            "write timeout after 30s",
                        )));
                    }
                    warn!(attempt = attempt, "object store write timeout, retrying");
                    let delay_ms = (base_delay_ms * 2_u64.pow(attempt - 1)).min(max_delay_ms);
                    let jitter = rand::random::<u64>() % (delay_ms / 10 + 1);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms + jitter)).await;
                }
            }
        }
    }
}
