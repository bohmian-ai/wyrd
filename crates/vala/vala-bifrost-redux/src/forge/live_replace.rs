//! Atomic replacement of one exact current-snapshot Iceberg file group.

use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::NaiveDate;
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use wyrd_spec::vala::api::{AuditDetail, ForgeIcebergRewritePhase, StoragePath};

use super::Forge;
use super::binpack::ForgeGroupKey;
use super::compact::forge_transition_event;
use super::error::ForgeError;
use super::lease::ForgeLease;
use super::rewrite::{RewriteOutput, RewriteRequest, RewriteSourceFile};
use super::right_size::IcebergRewriteGroup;
use crate::catalog::TenantTableBinding;
use crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION;

#[cfg(feature = "test-support")]
/// Injects one pre-`Prepared` audit failure for the real catalog integration seam.
static FAIL_NEXT_PREPARED_LIVE_AUDIT: AtomicBool = AtomicBool::new(false);

/// Result of attempting one plan-fenced live Iceberg replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcebergRewriteDisposition {
    /// The selected group cannot produce a useful replacement.
    NoWork,
    /// The table no longer matches the snapshot observed by its plan.
    SnapshotChanged,
    /// The exact delete/add replacement was committed and audited.
    Committed {
        /// Stable operation identity shared by Iceberg and audit records.
        operation_id: Uuid,
        /// Snapshot returned by the replacement commit.
        snapshot_id: i64,
        /// Exact input file count.
        input_files: usize,
        /// Exact output file count.
        output_files: usize,
        /// Rows read from the exact input set.
        input_rows: u64,
        /// Rows written into the exact output set.
        output_rows: u64,
        /// Peak bounded sort spill observed during the rewrite.
        spill_bytes: u64,
    },
}

/// Prepared in-memory state for one live replacement after output writes finish.
struct LiveRewrite {
    /// Table-scoped audit identity.
    key: ForgeGroupKey,
    /// Snapshot that fenced the discovery plan.
    base_snapshot_id: i64,
    /// Partition specification shared by the exact source set.
    partition_spec_id: i32,
    /// Target size captured from the table metadata.
    target_file_size_bytes: u64,
    /// Exact ordered source identities.
    source_files: Vec<RewriteSourceFile>,
    /// Stable operation and output identity.
    operation_id: Uuid,
    /// Outputs and row accounting from the bounded rewrite owner.
    rewrite: RewriteOutput,
}

impl LiveRewrite {
    /// Build the persisted detail for one lifecycle phase.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Group`] when an exact catalog or output path is
    /// invalid for the persisted non-secret storage-path contract.
    fn detail(
        &self,
        phase: ForgeIcebergRewritePhase,
        committed_snapshot_id: Option<i64>,
    ) -> Result<AuditDetail, ForgeError> {
        live_detail(self, phase, committed_snapshot_id)
    }
}

impl Forge {
    /// Rewrite and atomically replace one exact live group under an explicit plan snapshot.
    ///
    /// The method refuses stale plans before output IO, persists `Prepared` only
    /// after every output PUT succeeds, fences cancellation and lease ownership
    /// immediately before the Iceberg action, and leaves prepared evidence intact
    /// whenever the catalog response is uncertain.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError`] when validation, rewrite IO, audit persistence,
    /// fencing, or a definite catalog failure prevents replacement.
    ///
    /// # Cancellation
    ///
    /// Cancellation before `Prepared` prevents publication. After `Prepared`,
    /// cancellation never deletes outputs or claims a terminal audit phase.
    pub(crate) async fn replace_live_group(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        table: &Table,
        base_snapshot_id: i64,
        group: &IcebergRewriteGroup,
        stop: &CancellationToken,
    ) -> Result<IcebergRewriteDisposition, ForgeError> {
        let Some(operation) = self
            .prepare_live_rewrite(lease, binding, table, base_snapshot_id, group, stop)
            .await?
        else {
            return Ok(if group.files.len() < 2 {
                IcebergRewriteDisposition::NoWork
            } else {
                IcebergRewriteDisposition::SnapshotChanged
            });
        };
        let prepared = match operation.detail(ForgeIcebergRewritePhase::Prepared, None) {
            Ok(detail) => {
                self.append_live_audit(
                    lease,
                    &operation.key,
                    "forge.iceberg_rewrite.prepared",
                    detail,
                )
                .await
            }
            Err(error) => Err(error),
        };
        if let Err(error) = prepared {
            self.core
                .rewrite
                .cleanup_unprepared_outputs(
                    &operation.rewrite.object_paths,
                    binding,
                    stop,
                    lease,
                    &self.core.operator_pool,
                )
                .await;
            return Err(error);
        }
        self.commit_live_rewrite(lease, binding, base_snapshot_id, operation, stop)
            .await
    }

    /// Validate and rewrite exact live inputs without publishing an Iceberg transition.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError`] when the group is malformed or bounded rewrite IO fails.
    async fn prepare_live_rewrite(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        table: &Table,
        base_snapshot_id: i64,
        group: &IcebergRewriteGroup,
        stop: &CancellationToken,
    ) -> Result<Option<LiveRewrite>, ForgeError> {
        if group.files.len() < 2 || table.metadata().current_snapshot_id() != Some(base_snapshot_id)
        {
            return Ok(None);
        }
        let first = group.files.first().ok_or_else(|| ForgeError::Invariant {
            detail: "non-empty live rewrite group lost its first input".to_owned(),
        })?;
        if group.files.iter().any(|file| {
            file.partition_day != first.partition_day
                || file.partition_spec_id != first.partition_spec_id
        }) {
            return Err(ForgeError::Invariant {
                detail: "live rewrite group crosses a partition boundary".to_owned(),
            });
        }
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition_day: first.partition_day,
        };
        let target_file_size_bytes = u64::try_from(
            table
                .metadata()
                .table_properties()
                .map_err(ForgeError::Catalog)?
                .write_target_file_size_bytes,
        )
        .map_err(|_| ForgeError::Invariant {
            detail: "Iceberg target file size exceeds u64".to_owned(),
        })?;
        let source_files = group
            .files
            .iter()
            .map(|file| RewriteSourceFile {
                catalog_path: file.catalog_path.clone(),
                object_path: file.object_path.clone(),
                file_size_bytes: file.file_size_bytes,
                record_count: file.record_count,
            })
            .collect::<Vec<_>>();
        let operation_id = iceberg_rewrite_operation_id(
            &key.audit_resource(),
            base_snapshot_id,
            first.partition_spec_id,
            first.partition_day,
            &source_files,
        );
        let schema = Arc::new(
            iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())
                .map_err(ForgeError::Catalog)?,
        );
        let rewrite = self
            .core
            .rewrite
            .rewrite(
                RewriteRequest {
                    operation_id,
                    binding,
                    schema,
                    iceberg_schema: table.metadata().current_schema().clone(),
                    source_files: &source_files,
                    partition_day: first.partition_day,
                    table_location: table.metadata().location(),
                    partition_spec_id: table.metadata().default_partition_spec_id(),
                    sort_order_id: i32::try_from(table.metadata().default_sort_order_id())
                        .map_err(|_| ForgeError::InvalidConfig {
                            detail: "Iceberg default sort-order ID exceeds i32".to_owned(),
                        })?,
                    target_file_size_bytes,
                },
                stop,
                lease,
                &self.core.operator_pool,
            )
            .await?;
        Ok(Some(LiveRewrite {
            key,
            base_snapshot_id,
            partition_spec_id: first.partition_spec_id,
            target_file_size_bytes,
            source_files,
            operation_id,
            rewrite,
        }))
    }

    /// Fence, commit, and terminally audit an already prepared live replacement.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError`] when fencing, commit, or terminal audit persistence fails.
    async fn commit_live_rewrite(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        base_snapshot_id: i64,
        operation: LiveRewrite,
        stop: &CancellationToken,
    ) -> Result<IcebergRewriteDisposition, ForgeError> {
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let current = self.load_table(&binding.table_ident()).await?;
        if current.metadata().current_snapshot_id() != Some(base_snapshot_id) {
            return Ok(IcebergRewriteDisposition::SnapshotChanged);
        }
        let mut properties = HashMap::new();
        properties.insert("forge.workflow".to_owned(), "iceberg-rewrite".to_owned());
        properties.insert(
            "forge.operation_id".to_owned(),
            operation.operation_id.to_string(),
        );
        properties.insert("forge.group".to_owned(), operation.key.audit_resource());
        let tx = Transaction::new(&current);
        let action = tx
            .rewrite_files()
            .delete_files(
                operation
                    .source_files
                    .iter()
                    .map(|file| file.catalog_path.clone())
                    .collect::<Vec<_>>(),
            )
            .add_data_files(operation.rewrite.files.iter().cloned())
            .set_commit_uuid(operation.operation_id)
            .set_snapshot_properties(properties);
        let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
        lease.require_fence(&self.core.operator_pool).await?;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        let commit = transaction.commit_once(self.core.catalog.as_ref());
        tokio::pin!(commit);
        let response = tokio::select! {
            response = tokio::time::timeout(self.core.config.iceberg_total_retry_timeout, &mut commit) => response,
            () = stop.cancelled() => return Err(ForgeError::Shutdown),
        };
        match response {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => return Err(ForgeError::Catalog(error)),
            Err(_) => {
                return Err(ForgeError::Timeout {
                    operation: "Iceberg live replacement commit",
                });
            }
        }
        let committed = self.load_table(&binding.table_ident()).await?;
        let snapshot_id = committed.metadata().current_snapshot_id().ok_or_else(|| {
            ForgeError::Reconciliation {
                detail: "Iceberg live replacement committed without a current snapshot".to_owned(),
            }
        })?;
        self.append_live_audit(
            lease,
            &operation.key,
            "forge.iceberg_rewrite.committed",
            operation.detail(ForgeIcebergRewritePhase::Committed, Some(snapshot_id))?,
        )
        .await?;
        Ok(IcebergRewriteDisposition::Committed {
            operation_id: operation.operation_id,
            snapshot_id,
            input_files: operation.source_files.len(),
            output_files: operation.rewrite.files.len(),
            input_rows: operation.rewrite.input_rows,
            output_rows: operation.rewrite.output_rows,
            spill_bytes: operation.rewrite.spill_bytes,
        })
    }

    /// Execute one explicit-base live replacement through the test-support seam.
    ///
    /// This wrapper exposes no production scheduling behavior; catalog fixtures
    /// use it to assert exact Iceberg replacement membership and audit ordering.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::replace_live_group`].
    #[cfg(feature = "test-support")]
    pub async fn replace_live_group_for_test(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        table: &Table,
        base_snapshot_id: i64,
        group: &IcebergRewriteGroup,
        stop: &CancellationToken,
    ) -> Result<IcebergRewriteDisposition, ForgeError> {
        self.replace_live_group(lease, binding, table, base_snapshot_id, group, stop)
            .await
    }

    /// Fail the next test-support `Prepared` live-replacement audit append.
    ///
    /// This is a single-use integration seam. It fails before any audit row is
    /// durable so callers can prove that rewrite-owned outputs are reclaimed
    /// while the original audit error remains authoritative.
    #[cfg(feature = "test-support")]
    pub fn fail_next_prepared_live_audit_for_test(&self) {
        FAIL_NEXT_PREPARED_LIVE_AUDIT.store(true, Ordering::Release);
    }

    /// Append one live-replacement audit and projection transition atomically.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError`] when lease renewal, tenant transaction creation,
    /// operation-state transition, audit append, fence assertion, or transaction
    /// commit fails. The caller-owned transaction rolls back both durable rows.
    pub(super) async fn append_live_audit(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        operation: &str,
        detail: AuditDetail,
    ) -> Result<(), ForgeError> {
        if !lease.renew(&self.core.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        lease.require_fence(&self.core.operator_pool).await?;
        #[cfg(feature = "test-support")]
        if operation == "forge.iceberg_rewrite.prepared"
            && FAIL_NEXT_PREPARED_LIVE_AUDIT.swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Invariant {
                detail: "injected Prepared audit append failure".to_owned(),
            });
        }
        let resource = key.audit_resource();
        let event = forge_transition_event(operation, resource.clone(), detail);
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::IcebergRewrite)
            .map_err(ForgeError::Sql)?;
        let transition = if operation == "forge.iceberg_rewrite.prepared" {
            operations.append_prepared(&mut conn, &event).await
        } else {
            operations.append_terminal(&mut conn, &event).await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            ForgeOperationTransition::Applied { .. }
            | ForgeOperationTransition::AlreadyApplied { .. } => {}
        }
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}

/// Derive the stable length-framed UUID for one explicit-base live replacement.
#[must_use]
pub(crate) fn iceberg_rewrite_operation_id(
    resource: &str,
    base_snapshot_id: i64,
    partition_spec_id: i32,
    partition_day: NaiveDate,
    source_files: &[RewriteSourceFile],
) -> Uuid {
    let mut hasher = Sha256::new();
    update_frame(&mut hasher, resource.as_bytes());
    hasher.update(base_snapshot_id.to_be_bytes());
    hasher.update(partition_spec_id.to_be_bytes());
    update_frame(&mut hasher, partition_day.to_string().as_bytes());
    hasher.update(
        u32::try_from(source_files.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for file in source_files {
        update_frame(&mut hasher, file.catalog_path.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

/// Append one `u32` big-endian length frame and its exact byte payload.
fn update_frame(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u32::try_from(value.len()).unwrap_or(u32::MAX).to_be_bytes());
    hasher.update(value);
}

/// Construct the stable persisted detail for one live replacement transition.
///
/// # Errors
///
/// Returns [`ForgeError::Group`] when a catalog or output path cannot enter the
/// non-secret persisted `StoragePath` contract.
fn live_detail(
    operation: &LiveRewrite,
    phase: ForgeIcebergRewritePhase,
    committed_snapshot_id: Option<i64>,
) -> Result<AuditDetail, ForgeError> {
    let input_paths = operation
        .source_files
        .iter()
        .map(|file| {
            StoragePath::new(file.catalog_path.clone()).map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output_paths = operation
        .rewrite
        .files
        .iter()
        .map(|file| {
            StoragePath::new(file.file_path().to_owned()).map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AuditDetail::ForgeIcebergRewrite {
        operation_id: operation.operation_id,
        phase,
        group: operation.key.audit_resource(),
        base_snapshot_id: operation.base_snapshot_id,
        committed_snapshot_id,
        partition_spec_id: operation.partition_spec_id,
        partition_day: operation.key.partition_day.to_string(),
        target_file_size_bytes: operation.target_file_size_bytes,
        input_paths,
        output_paths,
        writer_recipe_version: BIFROST_WRITER_RECIPE_VERSION.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{RewriteSourceFile, iceberg_rewrite_operation_id};

    /// Proves framing distinguishes strings that would collide under delimiters.
    #[test]
    fn length_framed_operation_identity_distinguishes_ambiguous_inputs() {
        let day = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid test day");
        let left = [RewriteSourceFile {
            catalog_path: "a|bc".to_owned(),
            object_path: "a|bc".to_owned(),
            file_size_bytes: 1,
            record_count: 1,
        }];
        let right = [
            RewriteSourceFile {
                catalog_path: "a".to_owned(),
                object_path: "a".to_owned(),
                file_size_bytes: 1,
                record_count: 1,
            },
            RewriteSourceFile {
                catalog_path: "bc".to_owned(),
                object_path: "bc".to_owned(),
                file_size_bytes: 1,
                record_count: 1,
            },
        ];
        assert_ne!(
            iceberg_rewrite_operation_id("bifrost://t/a", 7, 3, day, &left),
            iceberg_rewrite_operation_id("bifrost://t/a", 7, 3, day, &right)
        );
    }

    /// Proves the explicit plan base, including a non-ASCII resource, affects identity.
    #[test]
    fn operation_identity_uses_explicit_plan_base() {
        let day = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid test day");
        let files = [RewriteSourceFile {
            catalog_path: "live/é.parquet".to_owned(),
            object_path: "live/é.parquet".to_owned(),
            file_size_bytes: 1,
            record_count: 1,
        }];
        let operation_id = iceberg_rewrite_operation_id("bifrost://t/é", 41, 3, day, &files);
        assert_eq!(
            operation_id,
            uuid::Uuid::parse_str("7360b1c2-10cf-b354-4d58-56d0db85a2c7")
                .expect("fixed operation UUID")
        );
        assert_ne!(
            operation_id,
            iceberg_rewrite_operation_id("bifrost://t/é", 42, 3, day, &files)
        );
    }
}
