//! Seal state machine — freeze → Parquet → PUT → PG tx → manifest → retire WAL.

use std::sync::Arc;

use opendal::{ErrorKind, Operator};
use tracing::{info, warn};
use vala_sql::TenantConn;
use wyrd_spec::ids::PodId;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::scribe::file_list_writer;
use crate::scribe::filename::seal_filename;
use crate::scribe::memtable::Memtable;
use crate::scribe::parquet_writer::{ParquetEncoded, encode_batch};
use crate::scribe::seal_key::SealKey;

/// Handle returned by `pre_commit` to be passed to `post_commit` after the
/// caller commits the seal transaction.
#[derive(Debug, Clone)]
pub struct SealCommit {
    pub seal_key: SealKey,
    pub binding: TenantTableBinding,
    pub wal_lsn_max: u64,
    pub parquet_path: String,
}

/// Seal state machine for one seal-key.
///
/// States: `Freeze` → `WriteParquet` → `PutObject` → `AtomicPgTx` → `ManifestUpdate` → `RetireWal`
#[derive(Debug)]
pub struct SealDriver {
    operator: Arc<Operator>,
}

impl SealDriver {
    /// Construct a new `SealDriver` with the given opendal operator.
    #[must_use]
    pub fn new(operator: Arc<Operator>) -> Self {
        Self { operator }
    }

    /// Execute seal stages 1-4 (Freeze → Parquet → PUT → PG tx) and return a
    /// commit handle. The caller must commit the transaction, then call
    /// `post_commit` with the handle to complete stages 5-6 (manifest + WAL
    /// retirement).
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
        let frozen = memtable.freeze(seal_key)?;

        // 2. WriteParquet (spawn_blocking to avoid blocking reactor)
        info!("seal stage: WriteParquet");
        let frozen_clone = frozen.clone();
        let binding_clone = binding.clone();
        let seal_tenant = seal_key.tenant;
        let encoded = tokio::task::spawn_blocking(move || {
            encode_batch(&frozen_clone, &binding_clone, seal_tenant)
        })
        .await
        .map_err(|e| ScribeError::Internal {
            detail: format!("Parquet encode task panic: {e}"),
        })??;

        // 3. PutObject
        info!("seal stage: PutObject");
        let parquet_path = self.put_object(binding, &encoded, node_id).await?;

        // 4. AtomicPgTx
        info!("seal stage: AtomicPgTx");
        let row = file_list_writer::build_insert(
            &frozen,
            &encoded,
            binding,
            node_id,
            writer_epoch,
            &parquet_path,
        )?;
        file_list_writer::insert_and_audit(conn, &row, &encoded.audit_events)
            .await
            .map_err(ScribeError::from)?;

        // Extract max WAL LSN from append metas
        let wal_lsn_max = encoded
            .append_metas
            .iter()
            .map(|m| m.wal_lsn_max.as_u64())
            .max()
            .unwrap_or(0);

        // Return handle for post-commit stages
        Ok(SealCommit {
            seal_key: seal_key.clone(),
            binding: binding.clone(),
            wal_lsn_max,
            parquet_path,
        })
    }

    /// Complete seal stages 5-6 (manifest + WAL retirement) after the caller
    /// commits the seal transaction.
    ///
    /// # Errors
    /// Returns [`ScribeError`] if any post-commit stage fails.
    #[tracing::instrument(skip(self), fields(seal_key = %handle.seal_key))]
    pub async fn post_commit(&self, handle: SealCommit) -> Result<(), ScribeError> {
        // 5. ManifestUpdate
        info!("seal stage: ManifestUpdate (stub)");
        // TODO: Update manifest sealed_lsn[K] and retire WAL bytes

        // 6. RetireWal
        info!("seal stage: RetireWal (stub)");
        // TODO: Retire WAL bytes at or below the new watermark

        Ok(())
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
        let pod_id = PodId::new(node_id).map_err(|e| ScribeError::Internal {
            detail: format!("node_id is not a valid PodId: {e}"),
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
