//! Bounded claim-driven execution for durable Forge tasks.
//!
//! The same owner runs in the embedded `all` topology and in dedicated worker
//! processes. `PostgreSQL` claims assign compute, while the table-scoped
//! [`ForgeLease`] remains the only publication fence.

use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use iceberg::spec::TableMetadata;
use iceberg::table::Table;
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeTasks};
use vala_sql::row_types::forge_operations::ForgeOperationFamily;
use vala_sql::row_types::forge_tasks::{
    ForgeClaimStrategy, ForgeCleanupCandidate, ForgeCleanupCategory, ForgeCleanupPath,
    ForgePreparedTaskClaim, ForgeTask, ForgeTaskClaim, ForgeTaskEvidence, ForgeTaskState,
    ForgeTaskStrategy, ForgeTaskTableIdentity, ForgeTaskTransition, SnapshotWatermark,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use super::error::ForgeError;
use super::expire::{PendingExpiryTerminal, derive_recovered_files, table_resource_for_key};
use super::identity::task_table_binding;
use super::lease::{ForgeLease, forge_lease_key};
use super::live_replace::{
    IcebergRewriteDisposition, TaskLiveRewriteRequest, TaskLiveRewriteResult,
};
use super::maintenance::{ForgeMaintenance, ForgeMaintenanceResult};
use super::metrics::{ForgeLeaseResult, ForgeMetricStage};
use super::path::catalog_path_to_object_key;
use super::{Forge, ForgeCapacity};
use crate::catalog::TenantTableBinding;

/// Exact external effect returned by strategy dispatch.
enum ForgeDispatchResult {
    /// Ordinary publication whose evidence is not yet Prepared.
    Committed(Table),
    /// Ordered maintenance with exact post-expiry candidates.
    Maintenance(Box<ForgeMaintenanceResult>),
}

/// Exact durable ownership required to advance a Prepared cleanup cursor.
struct CleanupAttempt<'a> {
    /// Durable task identity.
    task_id: Uuid,
    /// Tenant transaction boundary.
    tenant: DataTenantId,
    /// Attempt generation retained across takeover.
    attempt: Uuid,
    /// Physical table binding used for path validation.
    binding: &'a TenantTableBinding,
}

/// Converts fork categories into sorted, table-bound durable cleanup evidence.
///
/// # Errors
///
/// Returns path-binding or SQL validation failures for any unsafe candidate.
fn cleanup_candidates(
    result: &ForgeMaintenanceResult,
    binding: &TenantTableBinding,
    staging: &opendal::Operator,
    identity: &ForgeTaskTableIdentity,
) -> Result<Vec<ForgeCleanupCandidate>, ForgeError> {
    let categories = [
        (ForgeCleanupCategory::Data, &result.expired_files.data_files),
        (
            ForgeCleanupCategory::Data,
            &result.expired_files.delete_files,
        ),
        (ForgeCleanupCategory::Data, &result.expired_files.statistics),
        (
            ForgeCleanupCategory::Manifest,
            &result.expired_files.manifests,
        ),
        (
            ForgeCleanupCategory::Manifest,
            &result.expired_files.manifest_lists,
        ),
        (
            ForgeCleanupCategory::Metadata,
            &result.expired_files.metadata_logs,
        ),
    ];
    let mut candidates = categories
        .into_iter()
        .flat_map(|(category, paths)| paths.iter().map(move |path| (category, path)))
        .map(|(category, path)| {
            let key = catalog_path_to_object_key(
                result.table.metadata().location(),
                binding,
                staging,
                path,
            )?;
            Ok(ForgeCleanupCandidate {
                category,
                table: identity.clone(),
                path: ForgeCleanupPath::new(key).map_err(ForgeError::Sql)?,
            })
        })
        .collect::<Result<Vec<_>, ForgeError>>()?;
    candidates.sort();
    candidates.dedup();
    Ok(candidates)
}

/// Fixed process-local bounds for one Forge worker pool.
#[derive(Debug, Clone, Copy)]
pub struct ForgeWorkerConfig {
    /// Number of task executors spawned by this process.
    pub worker_concurrency: usize,
}

impl Default for ForgeWorkerConfig {
    /// Uses one executor so embedded deployments share dedicated semantics.
    fn default() -> Self {
        Self {
            worker_concurrency: 1,
        }
    }
}

impl ForgeWorkerConfig {
    /// Validates the fixed worker set before any executor is spawned.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when concurrency is zero.
    pub fn validate(self) -> Result<Self, ForgeError> {
        if self.worker_concurrency == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge worker concurrency must be positive".to_owned(),
            });
        }
        Ok(self)
    }
}

/// Claim-driven Forge executor shared by embedded and dedicated topologies.
#[derive(Clone)]
pub struct ForgeWorker {
    /// Complete Forge dependency owner used by every execution slot.
    forge: Arc<Forge>,
    /// Durable lifecycle query owner.
    tasks: ForgeTasks,
    /// Stable worker process identity used by every claim in this pool.
    owner: Uuid,
    /// Fixed process-local concurrency.
    config: ForgeWorkerConfig,
    /// Complete capacity declaration passed to atomic `PostgreSQL` admission.
    capacity: ForgeCapacity,
    /// One-shot crash boundary after task evidence commits and before expiry closes.
    #[cfg(feature = "test-support")]
    fail_after_maintenance_prepared: Arc<AtomicBool>,
}

impl ForgeWorker {
    /// Constructs one bounded worker over an existing Forge dependency graph.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when worker or derived claim limits are
    /// zero or cannot fit their durable integer domains.
    pub fn new(forge: Arc<Forge>, config: ForgeWorkerConfig) -> Result<Self, ForgeError> {
        let config = config.validate()?;
        let limits = &forge.core.config;
        let capacity = ForgeCapacity {
            max_files: u32::try_from(limits.max_files_per_tick).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge worker file capacity exceeds u32".to_owned(),
                }
            })?,
            max_bytes: limits.max_bytes_per_tick,
            max_parallelism: u16::try_from(limits.max_concurrent_reads).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge worker parallelism exceeds u16".to_owned(),
                }
            })?,
            max_memory_bytes: limits.max_memory_bytes,
            max_spill_bytes: limits.spill_limit_bytes,
            max_large_task_bytes: limits.max_large_task_bytes,
        }
        .validate()?;
        Ok(Self {
            forge,
            tasks: ForgeTasks::new(),
            owner: Uuid::now_v7(),
            config,
            capacity,
            #[cfg(feature = "test-support")]
            fail_after_maintenance_prepared: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Arms one failure after maintenance task Prepared commits but before expiry terminal audit.
    #[cfg(feature = "test-support")]
    pub fn fail_after_maintenance_prepared_for_test(&self) {
        self.fail_after_maintenance_prepared
            .store(true, Ordering::Release);
    }

    /// Runs the fixed worker set until shutdown stops claims and drains slots.
    ///
    /// Each slot claims at most one task at a time. Cancellation stops new
    /// claims immediately; active operations observe the same token at their
    /// established safe boundaries before the pool joins every slot.
    ///
    /// # Errors
    ///
    /// Returns a slot panic or an unexpected slot-level configuration failure.
    pub async fn run(self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let mut slots = JoinSet::new();
        for _ in 0..self.config.worker_concurrency {
            let worker = self.clone();
            let stop = shutdown.clone();
            slots.spawn(async move { worker.run_slot(stop).await });
        }
        while let Some(result) = slots.join_next().await {
            result.map_err(|error| ForgeError::Invariant {
                detail: format!("Forge worker slot panicked: {error}"),
            })??;
        }
        Ok(())
    }

    /// Claims and executes work serially for one bounded pool slot.
    ///
    /// # Errors
    ///
    /// Returns only configuration errors that make further claims unsafe.
    /// Individual task failures remain durable and are logged for takeover.
    async fn run_slot(&self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let claim_limits = self.claim_limits()?;
        loop {
            if shutdown.is_cancelled() {
                return Ok(());
            }
            self.tasks
                .reclaim_expired(
                    &self.forge.core.operator_pool,
                    claim_limits.max_active_per_tenant,
                )
                .await
                .map_err(ForgeError::Sql)?;
            if let Some(prepared) = self
                .tasks
                .claim_prepared_for_reconciliation(
                    &self.forge.core.operator_pool,
                    self.owner,
                    claim_limits.lease_seconds,
                )
                .await
                .map_err(ForgeError::Sql)?
            {
                if let Err(error) = self.reconcile_prepared(prepared, &shutdown).await {
                    tracing::warn!(worker = %self.owner, error = %error, "Prepared Forge task reconciliation stopped; exact evidence retained");
                }
                continue;
            }
            let claim = self
                .tasks
                .claim_fair(&self.forge.core.operator_pool, self.owner, claim_limits)
                .await
                .map_err(ForgeError::Sql)?;
            let Some(claim) = claim else {
                tokio::select! {
                    () = shutdown.cancelled() => return Ok(()),
                    () = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
                continue;
            };
            if let Err(error) = self.execute_claim(claim, &shutdown).await {
                tracing::warn!(worker = %self.owner, error = %error, "Forge task execution stopped; durable state retained for recovery");
            }
        }
    }

    /// Claims and executes at most one task for deterministic integration fixtures.
    ///
    /// Prepared recovery retains priority over ordinary claims exactly as it
    /// does in the supervised worker loop.
    ///
    /// # Errors
    ///
    /// Returns claim, validation, execution, reconciliation, or lifecycle
    /// failures instead of logging them for a later supervised retry.
    #[cfg(feature = "test-support")]
    pub async fn execute_one_for_test(
        &self,
        shutdown: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let limits = self.claim_limits()?;
        self.tasks
            .reclaim_expired(&self.forge.core.operator_pool, limits.max_active_per_tenant)
            .await
            .map_err(ForgeError::Sql)?;
        if let Some(prepared) = self
            .tasks
            .claim_prepared_for_reconciliation(
                &self.forge.core.operator_pool,
                self.owner,
                limits.lease_seconds,
            )
            .await
            .map_err(ForgeError::Sql)?
        {
            self.reconcile_prepared(prepared, shutdown).await?;
            return Ok(true);
        }
        let claim = self
            .tasks
            .claim_fair(&self.forge.core.operator_pool, self.owner, limits)
            .await
            .map_err(ForgeError::Sql)?;
        let Some(claim) = claim else {
            return Ok(false);
        };
        self.execute_claim(claim, shutdown).await?;
        Ok(true)
    }

    /// Claims one ordinary task with this worker's production owner and limits.
    ///
    /// This narrow seam lets integration tests drive [`Self::execute_claim`]
    /// directly while preserving the same SQL admission transaction used by
    /// the supervised loop.
    ///
    /// # Errors
    ///
    /// Returns configuration, SQL, or persisted-row decoding failures.
    #[cfg(feature = "test-support")]
    pub async fn claim_for_test(&self) -> Result<Option<ForgeTaskClaim>, ForgeError> {
        let limits = self.claim_limits()?;
        self.tasks
            .claim_fair(&self.forge.core.operator_pool, self.owner, limits)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Executes one exact claimed task through validation, table fencing,
    /// bounded rewrite, exact evidence persistence, and terminal audit.
    ///
    /// Reserved, maintenance, and malformed payloads terminalize before catalog
    /// load, lease acquisition, or object IO. Supported tasks acquire the table
    /// lease before any catalog or object-store effect.
    ///
    /// # Errors
    ///
    /// Returns validation, SQL, catalog, object-store, rewrite, fencing,
    /// cancellation, evidence, or audit errors. Uncertain external outcomes
    /// remain nonterminal for lease-based recovery.
    pub async fn execute_claim(
        &self,
        claim: ForgeTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let task = &claim;
        if claim.execution_tenant_id != task.data_tenant_id {
            return Err(ForgeError::Invariant {
                detail: "Forge task tenant differs from claim execution context".to_owned(),
            });
        }
        let attempt = task.attempt_id.ok_or_else(|| ForgeError::Invariant {
            detail: "claimed Forge task has no attempt generation".to_owned(),
        })?;
        if task.claimed_by != Some(self.owner) || task.state != ForgeTaskState::Claimed {
            return Err(ForgeError::Invariant {
                detail: "Forge worker received a claim owned by another attempt".to_owned(),
            });
        }
        let binding = task_table_binding(
            task.data_tenant_id,
            claim.execution_tenant_id,
            &task.table_ref,
        )?;
        let stage = match Self::validate_payload(task) {
            Ok(validated) => validated,
            Err(error) => {
                self.fail_before_effect(task, attempt, error.to_string())
                    .await?;
                return Ok(());
            }
        };
        let lease_key = forge_lease_key(
            task.data_tenant_id,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let Some(mut lease) = ForgeLease::acquire(
            &self.forge.core.operator_pool,
            lease_key,
            self.owner,
            self.forge.core.config.lease_ttl,
        )
        .await?
        else {
            self.forge
                .core
                .metrics
                .record_lease(ForgeLeaseResult::Contention);
            return Err(ForgeError::FenceLost {
                lease_key: format!("forge:table:{}:{}", task.data_tenant_id, binding.table_ref),
            });
        };
        if lease.takeover() {
            self.forge
                .core
                .metrics
                .record_lease(ForgeLeaseResult::Takeover);
        }
        let started = Instant::now();
        let result = self
            .execute_fenced(task, attempt, &binding, &mut lease, shutdown)
            .await;
        self.forge
            .core
            .metrics
            .record_stage(stage, started.elapsed(), result.is_err());
        if matches!(result, Err(ForgeError::FenceLost { .. })) {
            self.forge
                .core
                .metrics
                .record_lease(ForgeLeaseResult::FenceLost);
        }
        if let Err(error) = lease.release(&self.forge.core.operator_pool).await {
            tracing::warn!(task_id = %task.task_id, error = %error, "Forge table lease release failed");
        }
        result
    }

    /// Reconciles one taken-over Prepared attempt from its exact stored evidence.
    ///
    /// Recovery never repeats the rewrite or reconstructs a serialized table.
    /// It verifies the committed snapshot remains retained and hashes the exact
    /// metadata object named by durable evidence before terminalizing.
    ///
    /// # Errors
    ///
    /// Returns malformed identity/evidence, lease, catalog, object-read,
    /// heartbeat, digest mismatch, audit, or fencing failures. Any failure
    /// retains the Prepared row for another bounded takeover.
    async fn reconcile_prepared(
        &self,
        claim: ForgePreparedTaskClaim,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let task = &claim.task;
        if claim.execution_tenant_id != task.data_tenant_id {
            return Err(ForgeError::Invariant {
                detail: "Prepared task tenant differs from claim execution context".to_owned(),
            });
        }
        let attempt = task.attempt_id.ok_or_else(|| ForgeError::Invariant {
            detail: "Prepared Forge task has no attempt generation".to_owned(),
        })?;
        if task.state != ForgeTaskState::Prepared || task.claimed_by != Some(self.owner) {
            return Err(ForgeError::Invariant {
                detail: "Prepared Forge reconciliation ownership changed".to_owned(),
            });
        }
        let binding = task_table_binding(
            task.data_tenant_id,
            claim.execution_tenant_id,
            &task.table_ref,
        )?;
        let evidence = task
            .evidence
            .as_ref()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared Forge task has no committed evidence".to_owned(),
            })?;
        let lease_key = forge_lease_key(
            task.data_tenant_id,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let Some(mut lease) = ForgeLease::acquire(
            &self.forge.core.operator_pool,
            lease_key,
            self.owner,
            self.forge.core.config.lease_ttl,
        )
        .await?
        else {
            return Err(ForgeError::FenceLost {
                lease_key: format!("forge:table:{}:{}", task.data_tenant_id, binding.table_ref),
            });
        };
        let operation_stop = shutdown.child_token();
        let heartbeat = self.spawn_authority_heartbeat(
            task.task_id,
            attempt,
            lease.clone(),
            operation_stop.clone(),
        )?;
        let reconciliation = self
            .resume_prepared_effect(
                task,
                attempt,
                &binding,
                &mut lease,
                evidence,
                &operation_stop,
            )
            .await;
        operation_stop.cancel();
        let heartbeat_result = heartbeat.await.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge reconciliation heartbeat panicked: {error}"),
        })?;
        let result = async {
            heartbeat_result?;
            reconciliation?;
            self.tasks
                .heartbeat(
                    &self.forge.core.operator_pool,
                    task.task_id,
                    attempt,
                    self.owner,
                    self.claim_limits()?.lease_seconds,
                )
                .await
                .map_err(ForgeError::Sql)?;
            lease.require_fence(&self.forge.core.operator_pool).await?;
            self.persist_terminal_success(
                task.task_id,
                task.data_tenant_id,
                &task.table_ref,
                attempt,
                &lease,
            )
            .await
        }
        .await;
        if let Err(error) = lease.release(&self.forge.core.operator_pool).await {
            tracing::warn!(task_id = %task.task_id, error = %error, "Forge reconciliation lease release failed");
        }
        result
    }

    /// Verifies and resumes every external effect owned by one Prepared task.
    ///
    /// # Errors
    ///
    /// Returns evidence, cleanup, orphan, fencing, catalog, or cancellation failures.
    async fn resume_prepared_effect(
        &self,
        task: &ForgeTask,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        self.verify_committed_evidence(binding, &table, evidence)
            .await?;
        if task.strategy == ForgeTaskStrategy::StagingFold {
            let snapshot_id =
                evidence
                    .committed_snapshot_id
                    .ok_or_else(|| ForgeError::Reconciliation {
                        detail: "recovered staging task has no committed snapshot".to_owned(),
                    })?;
            self.forge
                .stamp_recovered_staging_task(lease, binding, &task.plan.inputs, snapshot_id)
                .await?;
        }
        let expiry_terminals = if task.strategy == ForgeTaskStrategy::SnapshotExpiry {
            let key = super::compact::ForgeTableKey {
                tenant: task.data_tenant_id,
                table_ref: binding.table_ref.clone(),
            };
            self.matching_prepared_expiry(&key, binding, &task.table_ref, &table, evidence)
                .await?
        } else {
            Vec::new()
        };
        self.resume_expired_cleanup(
            CleanupAttempt {
                task_id: task.task_id,
                tenant: task.data_tenant_id,
                attempt,
                binding,
            },
            lease,
            evidence,
            stop,
        )
        .await?;
        if task.strategy == ForgeTaskStrategy::SnapshotExpiry {
            let key = super::compact::ForgeTableKey {
                tenant: task.data_tenant_id,
                table_ref: binding.table_ref.clone(),
            };
            self.forge
                .finalize_expiry_terminals(lease, task.data_tenant_id, expiry_terminals)
                .await?;
            ForgeMaintenance::new(Arc::clone(&self.forge))
                .collect_never_published(lease, &key, binding, stop)
                .await?;
        }
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        Ok(())
    }

    /// Finds the exact open expiry operation whose candidates match task evidence.
    ///
    /// # Errors
    ///
    /// Returns SQL, catalog, traversal, evidence, or audit failures, and fails
    /// closed when more than one open operation matches the durable task.
    async fn matching_prepared_expiry(
        &self,
        key: &super::compact::ForgeTableKey,
        binding: &TenantTableBinding,
        identity: &ForgeTaskTableIdentity,
        table: &Table,
        evidence: &ForgeTaskEvidence,
    ) -> Result<Vec<PendingExpiryTerminal>, ForgeError> {
        let resource = table_resource_for_key(key);
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = ForgeOperations::new(&resource, ForgeOperationFamily::SnapshotExpire)
            .map_err(ForgeError::Sql)?
            .list_open(
                &mut conn,
                self.forge.core.config.max_open_operations_per_table,
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if page.overflowed {
            return Err(ForgeError::Reconciliation {
                detail: "open expiry operations exceeded the recovery bound".to_owned(),
            });
        }
        let open_count = page.operations.len();
        if open_count == 0 {
            return Ok(Vec::new());
        }
        let mut matching = Vec::new();
        for row in page.operations {
            let wyrd_spec::vala::api::AuditDetail::ForgeSnapshotExpire {
                base_metadata_location,
                selected_snapshot_ids,
                ..
            } = &row.prepared_detail
            else {
                continue;
            };
            if selected_snapshot_ids
                .iter()
                .any(|id| table.metadata().snapshot_by_id(*id).is_some())
            {
                continue;
            }
            let expired = derive_recovered_files(
                table,
                base_metadata_location.as_str(),
                &self.forge.core.config,
            )
            .await?;
            let candidate_result = ForgeMaintenanceResult {
                table: table.clone(),
                expired_files: expired,
                expiry_terminals: Vec::new(),
            };
            if cleanup_candidates(
                &candidate_result,
                binding,
                &self.forge.core.staging,
                identity,
            )? == evidence.cleanup_candidates
            {
                matching.push(PendingExpiryTerminal {
                    detail: row.prepared_detail,
                    recovered: true,
                });
            }
        }
        if matching.len() != 1 {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "expected exactly one of {open_count} open expiry operations to match task evidence; matched {}",
                    matching.len()
                ),
            });
        }
        Ok(matching)
    }

    /// Validates the closed T13 strategy and payload contract without IO.
    ///
    /// # Errors
    ///
    /// Returns an invariant error for an unknown or reserved strategy,
    /// maintenance strategy, malformed parameters, or an empty exact input set.
    fn validate_payload(task: &ForgeTaskClaim) -> Result<ForgeMetricStage, ForgeError> {
        if task.plan.inputs.is_empty() {
            return Err(ForgeError::Invariant {
                detail: "Forge task payload has no exact inputs".to_owned(),
            });
        }
        let (expected_kind, stage) = match &task.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold) => {
                ("staging_fold", ForgeMetricStage::StagingFold)
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles) => {
                ("live_rewrite", ForgeMetricStage::IcebergRewrite)
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                ("maintenance", ForgeMetricStage::SnapshotExpiry)
            }
            ForgeClaimStrategy::Known(
                ForgeTaskStrategy::FullIdentity
                | ForgeTaskStrategy::ManifestRewrite
                | ForgeTaskStrategy::ExpiredCleanup
                | ForgeTaskStrategy::OrphanCleanup,
            )
            | ForgeClaimStrategy::Unknown(_) => {
                return Err(ForgeError::Invariant {
                    detail: format!("unsupported Forge task strategy {}", task.strategy.as_str()),
                });
            }
        };
        let parameters = task
            .plan
            .parameters
            .as_object()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge task parameters must be an object".to_owned(),
            })?;
        if parameters.len() != 1
            || parameters.get("kind").and_then(serde_json::Value::as_str) != Some(expected_kind)
        {
            return Err(ForgeError::Invariant {
                detail: "Forge task parameters do not match the strategy contract".to_owned(),
            });
        }
        Ok(stage)
    }

    /// Runs one supported task after acquiring the publication lease.
    ///
    /// # Errors
    ///
    /// Returns catalog, stale-snapshot, lifecycle, heartbeat, rewrite, evidence,
    /// object-store, fence, cancellation, or audit failures.
    async fn execute_fenced(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        shutdown: &CancellationToken,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        let base_matches = Self::base_snapshot_matches(&table, claim.base_snapshot_id);
        let committed_recovery = if base_matches {
            None
        } else {
            self.find_retained_task_evidence(binding, &table, claim.task_id)
                .await?
        };
        let maintenance_recovery = matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        );
        if !base_matches && committed_recovery.is_none() && !maintenance_recovery {
            self.cancel_superseded(claim).await?;
            return Ok(());
        }
        let watermark = Self::execution_watermark(&table, claim, committed_recovery.as_ref())?;
        self.tasks
            .start(
                &self.forge.core.operator_pool,
                claim.task_id,
                attempt,
                self.owner,
                watermark,
            )
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .heartbeat(
                &self.forge.core.operator_pool,
                claim.task_id,
                attempt,
                self.owner,
                self.claim_limits()?.lease_seconds,
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let operation_stop = shutdown.child_token();
        let heartbeat = self.spawn_authority_heartbeat(
            claim.task_id,
            attempt,
            lease.clone(),
            operation_stop.clone(),
        )?;
        let execution = match committed_recovery {
            Some(evidence) => Ok((evidence, true)),
            None => {
                match self
                    .dispatch_claim(claim, attempt, binding, lease, table, &operation_stop)
                    .await
                {
                    Ok(ForgeDispatchResult::Committed(committed)) => self
                        .committed_evidence(binding, &committed)
                        .await
                        .map(|evidence| (evidence, false)),
                    Ok(ForgeDispatchResult::Maintenance(result)) => self
                        .complete_maintenance(
                            claim,
                            attempt,
                            binding,
                            lease,
                            *result,
                            &operation_stop,
                        )
                        .await
                        .map(|evidence| (evidence, true)),
                    Err(error) => Err(error),
                }
            }
        };
        let completion = async {
            let evidence = execution?;
            if operation_stop.is_cancelled() {
                return Err(ForgeError::Shutdown);
            }
            Ok(evidence)
        }
        .await;
        operation_stop.cancel();
        let heartbeat_result = heartbeat.await.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge claim heartbeat panicked: {error}"),
        })?;
        heartbeat_result?;
        let (evidence, already_prepared) = completion?;
        self.finish_claim_execution(claim, attempt, lease, &evidence, already_prepared)
            .await
    }

    /// Refreshes final authority and commits the appropriate success transition.
    ///
    /// # Errors
    ///
    /// Returns heartbeat, fencing, evidence, audit, SQL, or replan failures.
    async fn finish_claim_execution(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        already_prepared: bool,
    ) -> Result<(), ForgeError> {
        self.tasks
            .heartbeat(
                &self.forge.core.operator_pool,
                claim.task_id,
                attempt,
                self.owner,
                self.claim_limits()?.lease_seconds,
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        if already_prepared {
            if matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold)
            ) {
                let snapshot_id =
                    evidence
                        .committed_snapshot_id
                        .ok_or_else(|| ForgeError::Reconciliation {
                            detail: "recovered staging task has no committed snapshot".to_owned(),
                        })?;
                let binding = task_table_binding(
                    claim.data_tenant_id,
                    claim.execution_tenant_id,
                    &claim.table_ref,
                )?;
                self.forge
                    .stamp_recovered_staging_task(lease, &binding, &claim.plan.inputs, snapshot_id)
                    .await?;
            }
            self.persist_terminal_success(
                claim.task_id,
                claim.data_tenant_id,
                &claim.table_ref,
                attempt,
                lease,
            )
            .await
        } else {
            self.persist_success(claim, attempt, lease, evidence).await
        }
    }

    /// Selects the retained snapshot protected while one claimed task runs.
    ///
    /// Normal execution protects the exact planning base. Recovery protects
    /// the task-tagged commit only when the original base has already expired.
    ///
    /// # Errors
    ///
    /// Returns reconciliation failure when neither the planning base nor the
    /// exact committed recovery snapshot remains retained.
    fn execution_watermark(
        table: &Table,
        claim: &ForgeTaskClaim,
        committed_recovery: Option<&ForgeTaskEvidence>,
    ) -> Result<SnapshotWatermark, ForgeError> {
        if let Some(base) = table.metadata().snapshot_by_id(claim.base_snapshot_id) {
            return Ok(SnapshotWatermark {
                snapshot_id: claim.base_snapshot_id,
                timestamp_ms: base.timestamp_ms(),
            });
        }
        if claim.base_snapshot_id == 0 && table.metadata().current_snapshot_id().is_none() {
            return Ok(SnapshotWatermark {
                snapshot_id: 0,
                timestamp_ms: 0,
            });
        }
        if matches!(
            claim.strategy,
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
        ) && let Some(current) = table.metadata().current_snapshot()
        {
            return Ok(SnapshotWatermark {
                snapshot_id: current.snapshot_id(),
                timestamp_ms: current.timestamp_ms(),
            });
        }
        let evidence = committed_recovery.ok_or_else(|| ForgeError::Reconciliation {
            detail: "Forge task base snapshot is no longer retained".to_owned(),
        })?;
        let snapshot_id =
            evidence
                .committed_snapshot_id
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "committed Forge recovery has no snapshot evidence".to_owned(),
                })?;
        let committed = table
            .metadata()
            .snapshot_by_id(snapshot_id)
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "committed Forge recovery snapshot is no longer retained".to_owned(),
            })?;
        Ok(SnapshotWatermark {
            snapshot_id: committed.snapshot_id(),
            timestamp_ms: committed.timestamp_ms(),
        })
    }

    /// Dispatches one validated task to its exact existing rewrite owner.
    ///
    /// # Errors
    ///
    /// Returns stale-snapshot, exact-input, rewrite, catalog, fencing, or
    /// unsupported-strategy failures.
    async fn dispatch_claim(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        table: Table,
        stop: &CancellationToken,
    ) -> Result<ForgeDispatchResult, ForgeError> {
        if Self::current_snapshot_matches_task(&table, claim.task_id) {
            return Ok(ForgeDispatchResult::Committed(table));
        }
        if !Self::base_snapshot_matches(&table, claim.base_snapshot_id)
            && !matches!(
                claim.strategy,
                ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry)
            )
        {
            return Err(ForgeError::Reconciliation {
                detail: "Forge task base snapshot changed before execution".to_owned(),
            });
        }
        match &claim.strategy {
            ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold) => self
                .forge
                .execute_staging_task(
                    lease,
                    binding,
                    &claim.plan.inputs,
                    claim.task_id,
                    attempt,
                    stop,
                )
                .await
                .map(ForgeDispatchResult::Committed),
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles) => {
                let group = self
                    .forge
                    .discover_exact_live_group(binding, &table, &claim.plan.inputs)
                    .await?;
                let TaskLiveRewriteResult {
                    disposition,
                    committed_table,
                } = self
                    .forge
                    .replace_live_group_for_task(TaskLiveRewriteRequest {
                        lease,
                        binding,
                        table: &table,
                        base_snapshot_id: claim.base_snapshot_id,
                        group: &group,
                        task_id: claim.task_id,
                        attempt_id: attempt,
                        stop,
                    })
                    .await?;
                match (disposition, committed_table) {
                    (IcebergRewriteDisposition::Committed { .. }, Some(table)) => {
                        Ok(ForgeDispatchResult::Committed(table))
                    }
                    (
                        IcebergRewriteDisposition::NoWork
                        | IcebergRewriteDisposition::SnapshotChanged,
                        _,
                    ) => Err(ForgeError::Reconciliation {
                        detail: "exact small-file task did not commit".to_owned(),
                    }),
                    (IcebergRewriteDisposition::Committed { .. }, None) => {
                        Err(ForgeError::Reconciliation {
                            detail: "committed rewrite lost its returned table".to_owned(),
                        })
                    }
                }
            }
            ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry) => {
                let key = super::compact::ForgeTableKey {
                    tenant: claim.data_tenant_id,
                    table_ref: binding.table_ref.clone(),
                };
                ForgeMaintenance::new(Arc::clone(&self.forge))
                    .execute(lease, &key, binding, table, &claim.plan.inputs, stop)
                    .await
                    .map(Box::new)
                    .map(ForgeDispatchResult::Maintenance)
            }
            _ => Err(ForgeError::Invariant {
                detail: "unsupported task passed pre-effect validation".to_owned(),
            }),
        }
    }

    /// Persists exact cleanup evidence, deletes it with a durable cursor, then runs orphan GC.
    ///
    /// Candidate derivation has already completed from before/after Iceberg
    /// metadata. This method records the complete ordered set before the first
    /// delete, advances the cursor after every idempotent object result, and
    /// only then enters the disjoint never-published generation collector.
    ///
    /// # Errors
    ///
    /// Returns path binding, evidence, SQL, audit, fencing, object-store, or
    /// cancellation failures. A failure retains Prepared evidence and its last
    /// committed cursor for deterministic takeover.
    async fn complete_maintenance(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        binding: &TenantTableBinding,
        lease: &mut ForgeLease,
        result: ForgeMaintenanceResult,
        stop: &CancellationToken,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        let candidates =
            cleanup_candidates(&result, binding, &self.forge.core.staging, &claim.table_ref)?;
        let mut evidence = self.committed_evidence(binding, &result.table).await?;
        evidence.cleanup_candidates = candidates;
        evidence.validate(false).map_err(ForgeError::Sql)?;

        let mut prepared = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .prepared(
                &mut prepared,
                claim.task_id,
                attempt,
                self.owner,
                &evidence,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Prepared,
                    "exact expired-file cleanup evidence persisted",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut prepared).await?;
        prepared.commit().await.map_err(ForgeError::Sql)?;
        #[cfg(feature = "test-support")]
        if self
            .fail_after_maintenance_prepared
            .swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Reconciliation {
                detail: "injected crash after maintenance task evidence commit".to_owned(),
            });
        }
        self.forge
            .finalize_expiry_terminals(lease, claim.data_tenant_id, result.expiry_terminals)
            .await?;

        for (index, candidate) in evidence.cleanup_candidates.iter().enumerate() {
            if stop.is_cancelled() {
                return Err(ForgeError::Shutdown);
            }
            lease.require_fence(&self.forge.core.operator_pool).await?;
            let path = binding
                .validate_object_path(candidate.path.as_str())
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "expired cleanup candidate escaped table binding".to_owned(),
                })?;
            match self.forge.core.object_store.delete(&path).await {
                Ok(()) => {}
                Err(error) if error.kind() == opendal::ErrorKind::NotFound => {}
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            }
            let expected = u32::try_from(index).map_err(|_| ForgeError::Invariant {
                detail: "expired cleanup cursor exceeds u32".to_owned(),
            })?;
            let next = expected
                .checked_add(1)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "expired cleanup cursor overflowed".to_owned(),
                })?;
            let mut cursor = self
                .forge
                .core
                .vala
                .tenant_conn(claim.data_tenant_id)
                .await
                .map_err(ForgeError::Sql)?;
            self.tasks
                .advance_cleanup_cursor(
                    &mut cursor,
                    claim.task_id,
                    attempt,
                    self.owner,
                    expected,
                    next,
                )
                .await
                .map_err(ForgeError::Sql)?;
            lease.assert_transaction_fence(&mut cursor).await?;
            cursor.commit().await.map_err(ForgeError::Sql)?;
            evidence.deleted_candidate_count = next;
        }
        let key = super::compact::ForgeTableKey {
            tenant: claim.data_tenant_id,
            table_ref: binding.table_ref.clone(),
        };
        ForgeMaintenance::new(Arc::clone(&self.forge))
            .collect_never_published(lease, &key, binding, stop)
            .await?;
        Ok(evidence)
    }

    /// Resumes the undeleted suffix of exact Prepared cleanup evidence.
    ///
    /// # Errors
    ///
    /// Returns malformed cursor, binding, fencing, object-store, SQL, or
    /// cancellation failures. Each successful object result and cursor update
    /// is idempotent, so another owner resumes at the first uncommitted index.
    async fn resume_expired_cleanup(
        &self,
        cleanup: CleanupAttempt<'_>,
        lease: &mut ForgeLease,
        evidence: &ForgeTaskEvidence,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let start = usize::try_from(evidence.deleted_candidate_count).map_err(|_| {
            ForgeError::Invariant {
                detail: "Prepared cleanup cursor exceeds usize".to_owned(),
            }
        })?;
        for (index, candidate) in evidence.cleanup_candidates.iter().enumerate().skip(start) {
            if stop.is_cancelled() {
                return Err(ForgeError::Shutdown);
            }
            lease.require_fence(&self.forge.core.operator_pool).await?;
            let path = cleanup
                .binding
                .validate_object_path(candidate.path.as_str())
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "Prepared cleanup candidate escaped table binding".to_owned(),
                })?;
            match self.forge.core.object_store.delete(&path).await {
                Ok(()) => {}
                Err(error) if error.kind() == opendal::ErrorKind::NotFound => {}
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            }
            let expected = u32::try_from(index).map_err(|_| ForgeError::Invariant {
                detail: "Prepared cleanup cursor exceeds u32".to_owned(),
            })?;
            let next = expected
                .checked_add(1)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Prepared cleanup cursor overflowed".to_owned(),
                })?;
            let mut conn = self
                .forge
                .core
                .vala
                .tenant_conn(cleanup.tenant)
                .await
                .map_err(ForgeError::Sql)?;
            self.tasks
                .advance_cleanup_cursor(
                    &mut conn,
                    cleanup.task_id,
                    cleanup.attempt,
                    self.owner,
                    expected,
                    next,
                )
                .await
                .map_err(ForgeError::Sql)?;
            lease.assert_transaction_fence(&mut conn).await?;
            conn.commit().await.map_err(ForgeError::Sql)?;
        }
        Ok(())
    }

    /// Starts coordinated task-claim and table-lease renewal for one operation.
    ///
    /// Either authority loss cancels the shared operation token. The task
    /// heartbeat also renews a large-lane reservation when the claim owns one,
    /// while the cloned table lease retains the same publication fence token.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the claim TTL cannot fit seconds.
    fn spawn_authority_heartbeat(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        mut lease: ForgeLease,
        stop: CancellationToken,
    ) -> Result<tokio::task::JoinHandle<Result<(), ForgeError>>, ForgeError> {
        let lease_seconds =
            u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge claim TTL exceeds u32 seconds".to_owned(),
                }
            })?;
        let interval =
            (self.forge.core.config.lease_ttl / 3).min(self.forge.core.maintenance_interval);
        let tasks = self.tasks;
        let operator_pool = self.forge.core.operator_pool.clone();
        let owner = self.owner;
        Ok(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                tokio::select! {
                    biased;
                    () = stop.cancelled() => return Ok(()),
                    _ = ticker.tick() => {
                        if let Err(error) = tasks.heartbeat(&operator_pool, task_id, attempt, owner, lease_seconds).await {
                            stop.cancel();
                            return Err(ForgeError::Sql(error));
                        }
                        match lease.renew(&operator_pool).await {
                            Ok(true) => {}
                            Ok(false) => {
                                stop.cancel();
                                return Err(ForgeError::FenceLost {
                                    lease_key: lease.lease_key.clone(),
                                });
                            }
                            Err(error) => {
                                stop.cancel();
                                return Err(error);
                            }
                        }
                    }
                }
            }
        }))
    }

    /// Derives exact raw-byte metadata evidence from the returned table.
    ///
    /// # Errors
    ///
    /// Returns missing-location, path-binding, object read, or snapshot errors.
    /// Read/hash failure leaves the task Running for recovery and never stores a
    /// fabricated digest.
    async fn committed_evidence(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
    ) -> Result<ForgeTaskEvidence, ForgeError> {
        let snapshot_id =
            table
                .metadata()
                .current_snapshot_id()
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "committed Forge table has no current snapshot".to_owned(),
                })?;
        let location = table
            .metadata_location_result()
            .map_err(ForgeError::Catalog)?
            .to_owned();
        let object_path = catalog_path_to_object_key(
            table.metadata().location(),
            binding,
            &self.forge.core.staging,
            &location,
        )?;
        let raw = self
            .forge
            .core
            .object_store
            .read(&object_path)
            .await
            .map_err(ForgeError::ObjectStore)?;
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(raw.to_bytes())));
        let evidence = ForgeTaskEvidence {
            version: 1,
            committed_snapshot_id: Some(snapshot_id),
            committed_metadata_location: Some(location),
            committed_metadata_digest: Some(digest),
            cleanup_candidates: Vec::new(),
            deleted_candidate_count: 0,
        };
        evidence.validate(false).map_err(ForgeError::Sql)?;
        Ok(evidence)
    }

    /// Locates the earliest retained metadata object whose current snapshot was
    /// committed by one exact Forge task.
    ///
    /// The metadata log is searched oldest-to-newest before the current
    /// location so a later property-only catalog commit cannot replace the
    /// task's original raw-byte evidence. Search is bounded by the configured
    /// retained-snapshot ceiling and every candidate is table-path validated.
    ///
    /// # Errors
    ///
    /// Returns catalog, object-read, path-binding, malformed metadata, or
    /// retention errors. A failure leaves the claim nonterminal and never
    /// fabricates evidence.
    async fn find_retained_task_evidence(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        task_id: Uuid,
    ) -> Result<Option<ForgeTaskEvidence>, ForgeError> {
        let mut locations = table
            .metadata()
            .metadata_log()
            .iter()
            .map(|entry| entry.metadata_file.clone())
            .collect::<Vec<_>>();
        locations.push(
            table
                .metadata_location_result()
                .map_err(ForgeError::Catalog)?
                .to_owned(),
        );
        let max_locations = self
            .forge
            .core
            .config
            .max_retained_snapshots_per_table
            .checked_add(1)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "Forge retained metadata search limit overflowed".to_owned(),
            })?;
        if locations.len() > max_locations {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "retained Forge metadata search has {} locations above limit {max_locations}",
                    locations.len()
                ),
            });
        }
        for location in locations {
            let object_path = catalog_path_to_object_key(
                table.metadata().location(),
                binding,
                &self.forge.core.staging,
                &location,
            )?;
            let raw = self
                .forge
                .core
                .object_store
                .read(&object_path)
                .await
                .map_err(ForgeError::ObjectStore)?;
            let metadata = TableMetadata::read_from(table.file_io(), &location)
                .await
                .map_err(ForgeError::Catalog)?;
            if metadata.location() != table.metadata().location() {
                return Err(ForgeError::Reconciliation {
                    detail: "retained Forge metadata belongs to another table".to_owned(),
                });
            }
            let Some(snapshot) = metadata.current_snapshot() else {
                continue;
            };
            if snapshot
                .summary()
                .additional_properties
                .get("forge.task_id")
                != Some(&task_id.to_string())
            {
                continue;
            }
            if table
                .metadata()
                .snapshot_by_id(snapshot.snapshot_id())
                .is_none()
            {
                return Err(ForgeError::Reconciliation {
                    detail: "committed Forge recovery snapshot is no longer retained".to_owned(),
                });
            }
            let evidence = ForgeTaskEvidence {
                version: 1,
                committed_snapshot_id: Some(snapshot.snapshot_id()),
                committed_metadata_location: Some(location),
                committed_metadata_digest: Some(format!(
                    "sha256:{}",
                    hex::encode(Sha256::digest(raw.to_bytes()))
                )),
                cleanup_candidates: Vec::new(),
                deleted_candidate_count: 0,
            };
            evidence.validate(false).map_err(ForgeError::Sql)?;
            return Ok(Some(evidence));
        }
        Ok(None)
    }

    /// Verifies that Prepared evidence still names exact durable commit bytes.
    ///
    /// # Errors
    ///
    /// Returns reconciliation, catalog-path, object-read, or digest failures.
    async fn verify_committed_evidence(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        evidence: &ForgeTaskEvidence,
    ) -> Result<(), ForgeError> {
        evidence.validate(true).map_err(ForgeError::Sql)?;
        let snapshot_id =
            evidence
                .committed_snapshot_id
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "Prepared evidence has no committed snapshot".to_owned(),
                })?;
        if table.metadata().snapshot_by_id(snapshot_id).is_none() {
            return Err(ForgeError::Reconciliation {
                detail: "Prepared evidence snapshot is no longer retained".to_owned(),
            });
        }
        let location = evidence
            .committed_metadata_location
            .as_deref()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared evidence has no committed metadata location".to_owned(),
            })?;
        let expected = evidence
            .committed_metadata_digest
            .as_deref()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Prepared evidence has no committed metadata digest".to_owned(),
            })?;
        let object_path = catalog_path_to_object_key(
            table.metadata().location(),
            binding,
            &self.forge.core.staging,
            location,
        )?;
        let raw = self
            .forge
            .core
            .object_store
            .read(&object_path)
            .await
            .map_err(ForgeError::ObjectStore)?;
        let actual = format!("sha256:{}", hex::encode(Sha256::digest(raw.to_bytes())));
        if actual != expected {
            return Err(ForgeError::Reconciliation {
                detail: "Prepared metadata digest does not match exact durable bytes".to_owned(),
            });
        }
        Ok(())
    }

    /// Persists Prepared evidence and Succeeded as two audited tenant transitions.
    ///
    /// # Errors
    ///
    /// Returns tenant connection, task transition, audit, fence, or commit errors.
    async fn persist_success(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        lease: &ForgeLease,
        evidence: &ForgeTaskEvidence,
    ) -> Result<(), ForgeError> {
        let mut prepared = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .prepared(
                &mut prepared,
                claim.task_id,
                attempt,
                self.owner,
                evidence,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Prepared,
                    "exact committed Iceberg metadata evidence persisted",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut prepared).await?;
        prepared.commit().await.map_err(ForgeError::Sql)?;

        let mut terminal = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .terminal(
                &mut terminal,
                ForgeTaskTransition {
                    task_id: claim.task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Succeeded,
                    "exact Forge task completed",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut terminal).await?;
        terminal.commit().await.map_err(ForgeError::Sql)?;
        self.request_replan(claim.data_tenant_id, &claim.table_ref)
            .await
    }

    /// Persists only the terminal transition for already-verified Prepared work.
    ///
    /// # Errors
    ///
    /// Returns tenant connection, exact lifecycle, audit, fence, or commit errors.
    async fn persist_terminal_success(
        &self,
        task_id: Uuid,
        tenant: DataTenantId,
        table_ref: &ForgeTaskTableIdentity,
        attempt: Uuid,
        lease: &ForgeLease,
    ) -> Result<(), ForgeError> {
        let mut terminal = self
            .forge
            .core
            .vala
            .tenant_conn(tenant)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .terminal(
                &mut terminal,
                ForgeTaskTransition {
                    task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Prepared,
                    next: ForgeTaskState::Succeeded,
                },
                &task_event(
                    task_id,
                    ForgeTaskState::Succeeded,
                    "exact Prepared Forge evidence reconciled",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        lease.assert_transaction_fence(&mut terminal).await?;
        terminal.commit().await.map_err(ForgeError::Sql)?;
        self.request_replan(tenant, table_ref).await
    }

    /// Terminally audits a malformed or unsupported claim before external effects.
    ///
    /// # Errors
    ///
    /// Returns tenant transaction, lifecycle, or audit errors.
    async fn fail_before_effect(
        &self,
        claim: &ForgeTaskClaim,
        attempt: Uuid,
        detail: String,
    ) -> Result<(), ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .terminal(
                &mut conn,
                ForgeTaskTransition {
                    task_id: claim.task_id,
                    attempt_id: attempt,
                    owner: self.owner,
                    expected: ForgeTaskState::Claimed,
                    next: ForgeTaskState::Failed,
                },
                &task_event(claim.task_id, ForgeTaskState::Failed, &detail),
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        self.request_replan(claim.data_tenant_id, &claim.table_ref)
            .await
    }

    /// Atomically cancels a superseded claim and creates its successor demand.
    ///
    /// # Errors
    ///
    /// Returns tenant transaction, exact lifecycle, audit, lane-release, demand,
    /// or commit errors. Rollback preserves the original claim for repair.
    async fn cancel_superseded(&self, claim: &ForgeTaskClaim) -> Result<(), ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(claim.data_tenant_id)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .cancel_superseded(
                &mut conn,
                claim,
                &task_event(
                    claim.task_id,
                    ForgeTaskState::Cancelled,
                    "base_snapshot_superseded",
                ),
            )
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Re-enqueues authoritative planning after one terminal table mutation.
    ///
    /// # Errors
    ///
    /// Returns SQL errors when the durable periodic demand cannot be advanced.
    async fn request_replan(
        &self,
        data_tenant_id: wyrd_spec::DataTenantId,
        table_ref: &ForgeTaskTableIdentity,
    ) -> Result<(), ForgeError> {
        let table = ForgeTaskTableIdentity::new(
            table_ref.catalog.clone(),
            table_ref.namespace.clone(),
            table_ref.table.clone(),
        )
        .map_err(ForgeError::Sql)?;
        self.tasks
            .upsert_periodic(&self.forge.core.operator_pool, data_tenant_id, &table)
            .await
            .map(|_| ())
            .map_err(ForgeError::Sql)
    }

    /// Reports whether the loaded current snapshot is the exact task commit.
    fn current_snapshot_matches_task(table: &Table, task_id: Uuid) -> bool {
        table.metadata().current_snapshot().is_some_and(|snapshot| {
            snapshot
                .summary()
                .additional_properties
                .get("forge.task_id")
                == Some(&task_id.to_string())
        })
    }

    /// Reports whether one table still exposes the task's exact planning base.
    fn base_snapshot_matches(table: &Table, base_snapshot_id: i64) -> bool {
        table.metadata().current_snapshot_id()
            == (base_snapshot_id != 0).then_some(base_snapshot_id)
    }

    /// Builds the positive atomic claim limits from Forge capacity.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the claim TTL exceeds `u32`.
    fn claim_limits(&self) -> Result<ForgeClaimLimits, ForgeError> {
        Ok(ForgeClaimLimits {
            max_active_per_tenant: u32::try_from(self.config.worker_concurrency)
                .unwrap_or(u32::MAX),
            lease_seconds: u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(
                |_| ForgeError::InvalidConfig {
                    detail: "Forge claim TTL exceeds u32 seconds".to_owned(),
                },
            )?,
            max_files: self.capacity.max_files,
            max_bytes: self.capacity.max_bytes,
            max_parallelism: self.capacity.max_parallelism,
            max_memory_bytes: self.capacity.max_memory_bytes,
            max_spill_bytes: self.capacity.max_spill_bytes,
            max_large_task_bytes: self.capacity.max_large_task_bytes,
        })
    }
}

/// Builds one internal task lifecycle audit envelope.
fn task_event(task_id: Uuid, state: ForgeTaskState, reason: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        format!("forge.task.{}", state.as_str()),
        format!("forge-task:{task_id}"),
        None,
        PrincipalId::new(Uuid::nil()),
        PrincipalKindTag::Service,
        AuthMethod::Internal,
        "bifrost:forge".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        reason.to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw metadata evidence hashes exact bytes and uses lowercase encoding.
    #[test]
    fn raw_metadata_digest_contract_is_exact() {
        let raw = br#"{"format-version":2,"current-snapshot-id":7}"#;
        assert_eq!(
            format!("sha256:{}", hex::encode(Sha256::digest(raw))),
            "sha256:6467490277052fc1bcd80842434ed2ea67de45224d5adb4a6c50b7fc5a0ce727"
        );
    }

    /// Worker concurrency is a hard positive construction invariant.
    #[test]
    fn worker_concurrency_must_be_positive() {
        assert!(
            ForgeWorkerConfig {
                worker_concurrency: 0
            }
            .validate()
            .is_err()
        );
        assert_eq!(ForgeWorkerConfig::default().worker_concurrency, 1);
    }
}
