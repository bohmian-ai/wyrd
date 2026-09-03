//! Durable PostgreSQL coordination for Forge maintenance work.
//!
//! [`ForgeTasks`] owns bounded enqueue, fair claim, attempt, watermark,
//! lifecycle, status, and retention workflows. Claims assign compute only;
//! they never replace the separate Forge publication lease.

// raw-query grep allowlist: Forge task tables post-date the sqlx offline cache and remain confined to OperatorPool/TenantConn.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, types::Uuid};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::queries::audit_outbox::{OperatorAudit, append_audit};
use crate::queries::forge_operations::{
    assert_lease_fence, bind_tenant, list_table_protection_in_operator_tx, lock_table_authority,
};
use crate::row_types::forge_operations::{ForgeClaimTable, ForgeExpirationAuthority};
use crate::row_types::forge_tasks::{
    ExpiredCleanupCandidateRequest, ExpiredCleanupOutcome, ExpiredCleanupPayload,
    FORGE_TASK_PAYLOAD_VERSION,
    ForgeCleanupCandidate, ForgePlanningDemand, ForgePlanningDemandSqlRow, ForgePreparedTaskClaim,
    ForgePreparedTaskClaimSqlRow, ForgeTask, ForgeTaskClaim, ForgeTaskClaimSqlRow,
    ForgeTaskEvidence, ForgeTaskPage, ForgeTaskSqlRow, ForgeTaskState, ForgeTaskStrategy,
    ForgeTaskTableIdentity, ForgeTaskTransition, ForgeTaskTransitionOutcome, NewForgeTask,
    SnapshotWatermark, TaskProgressEffect,
};
use crate::{OperatorPool, SqlError, TenantConn};

const TASK_PROJECTION: &str = "task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,envelope_version,decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,sort_spill_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,evidence,attempt_count,failure_class,next_eligible_at,failed_volume_identity,ready_at,created_at,updated_at";
const CLAIM_TASK_PROJECTION: &str = "t.task_id,t.data_tenant_id,t.catalog_name,t.namespace_name,t.table_name,t.strategy,t.lane,t.base_snapshot_id,t.plan,t.estimated_files,t.estimated_bytes,t.estimated_parallelism,t.estimated_memory_bytes,t.estimated_spill_bytes,t.large_task_ceiling_bytes,t.envelope_version,t.decoded_batch_bytes,t.decoded_input_bytes,t.sort_working_bytes,t.sort_merge_reservation_bytes,t.encoder_buffer_bytes,t.upload_chunk_bytes,t.footer_encoded_bytes,t.footer_decode_workspace_bytes,t.sort_spill_bytes,t.state,t.attempt_id,t.claimed_by,t.claim_expires_at,t.watermark_snapshot_id,t.watermark_timestamp_ms,t.evidence,t.attempt_count,t.failure_class,t.next_eligible_at,t.failed_volume_identity,t.ready_at,t.created_at,t.updated_at";

/// Exact PostgreSQL-16 fair-claim statement used by production and scale-plan proof.
pub const FAIR_CLAIM_SQL: &str = include_str!("forge_fair_claim.sql");

/// Claim limits enforced atomically by PostgreSQL.
#[derive(Debug, Clone, Copy)]
pub struct ForgeClaimLimits {
    /// Maximum active tasks for the selected tenant.
    pub max_active_per_tenant: u32,
    /// Claim lifetime in seconds.
    pub lease_seconds: u32,
    /// Maximum ordinary-lane input files.
    pub max_files: u32,
    /// Maximum ordinary-lane input bytes.
    pub max_bytes: u64,
    /// Maximum task parallelism.
    pub max_parallelism: u16,
    /// Maximum task memory bytes.
    pub max_memory_bytes: u64,
    /// Maximum task spill bytes.
    pub max_spill_bytes: u64,
    /// Per-task size ceiling for a `large_singleton` claim, applied together
    /// with the task's own `large_task_ceiling_bytes`. This bounds one large
    /// task's input size; it is not a cluster-wide large-lane cap (D78).
    pub max_large_task_bytes: u64,
}

/// Exact executable and terminal planning outputs persisted in one transaction.
#[derive(Debug, Clone, Copy)]
pub struct ForgeEnqueueBatch<'tasks> {
    /// Capacity-admitted tasks that become Ready.
    pub executable: &'tasks [NewForgeTask],
    /// Tasks that exceed every capacity lane and become Unschedulable.
    pub unschedulable: &'tasks [NewForgeTask],
}

/// Concrete owner of durable Forge task SQL workflows.
#[derive(Clone)]
pub struct ForgeTasks {
    /// Audited cross-tenant pool used by durable Forge coordination workflows.
    operator_pool: OperatorPool,
}

impl ForgeTasks {
    /// Returns ready task identities whose persisted envelope cannot fit one pod capacity.
    ///
    /// # Errors
    /// Returns SQL errors while reading durable task envelopes.
    pub async fn unclaimable_ready_task_ids(
        &self,
        limits: ForgeClaimLimits,
    ) -> Result<Vec<Uuid>, SqlError> {
        sqlx::query_scalar("SELECT task_id FROM vala.forge_tasks WHERE state IN ('ready','retryable') AND (estimated_parallelism>$1 OR estimated_memory_bytes>$2 OR estimated_spill_bytes>$3) ORDER BY task_id")
            .bind(i32::from(limits.max_parallelism))
            .bind(i64::try_from(limits.max_memory_bytes).unwrap_or(i64::MAX))
            .bind(i64::try_from(limits.max_spill_bytes).unwrap_or(i64::MAX))
            .fetch_all(self.operator_pool.pool())
            .await
            .map_err(SqlError::from)
    }

    /// Registers a healthy worker or clears quarantine after a successful probe.
    ///
    /// # Errors
    /// Returns SQL errors when the durable worker registry cannot be updated.
    pub async fn register_healthy_worker(
        &self,
        worker: Uuid,
        volume: &str,
    ) -> Result<(), SqlError> {
        sqlx::query("INSERT INTO vala.forge_worker_registry(worker_id,scratch_volume_identity,quarantined,quarantine_reason,quarantined_at,heartbeat_at) VALUES($1,$2,false,NULL,NULL,statement_timestamp()) ON CONFLICT(worker_id) DO UPDATE SET scratch_volume_identity=EXCLUDED.scratch_volume_identity,quarantined=false,quarantine_reason=NULL,quarantined_at=NULL,heartbeat_at=statement_timestamp()")
            .bind(worker).bind(volume).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        Ok(())
    }

    /// Quarantines one worker after a typed local-storage failure.
    ///
    /// # Errors
    /// Returns SQL errors when quarantine cannot be persisted.
    pub async fn quarantine_worker(&self, worker: Uuid, volume: &str) -> Result<(), SqlError> {
        sqlx::query("INSERT INTO vala.forge_worker_registry(worker_id,scratch_volume_identity,quarantined,quarantine_reason,quarantined_at,heartbeat_at) VALUES($1,$2,true,'storage_health',statement_timestamp(),statement_timestamp()) ON CONFLICT(worker_id) DO UPDATE SET scratch_volume_identity=EXCLUDED.scratch_volume_identity,quarantined=true,quarantine_reason='storage_health',quarantined_at=statement_timestamp(),heartbeat_at=statement_timestamp()")
            .bind(worker).bind(volume).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        Ok(())
    }

    /// Reads the durable attempt count for worker-side bounded settlement.
    ///
    /// # Errors
    /// Returns SQL or invariant errors when the task is absent or malformed.
    pub async fn attempt_count(&self, task_id: Uuid) -> Result<u32, SqlError> {
        let count: Option<i32> =
            sqlx::query_scalar("SELECT attempt_count FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_optional(self.operator_pool.pool())
                .await
                .map_err(SqlError::from)?;
        let count = count.ok_or_else(|| SqlError::Conflict {
            detail: "Forge task is absent".to_owned(),
        })?;
        u32::try_from(count).map_err(|_| SqlError::InvariantViolation {
            detail: "negative Forge attempt count".to_owned(),
        })
    }

    /// Records the closed failure class inside a caller-owned audited transaction.
    ///
    /// # Errors
    /// Returns conflict when the task is not owned by the transaction tenant.
    pub async fn qualify_terminal_failure(
        &self,
        conn: &mut TenantConn<'_>,
        task_id: Uuid,
        failure_class: &str,
        failed_volume_identity: Option<&str>,
    ) -> Result<(), SqlError> {
        let changed = sqlx::query("UPDATE vala.forge_tasks SET attempt_count=attempt_count+1,failure_class=$3,failed_volume_identity=$4,next_eligible_at=statement_timestamp(),updated_at=statement_timestamp() WHERE task_id=$1 AND data_tenant_id=$2")
            .bind(task_id).bind(conn.data_tenant_id().as_uuid()).bind(failure_class).bind(failed_volume_identity)
            .execute(&mut **conn.transaction()).await.map_err(SqlError::from)?.rows_affected();
        exact_one(changed, "qualify terminal failure")
    }

    /// Constructs the durable task owner.
    #[must_use]
    pub fn new(operator_pool: OperatorPool) -> Self {
        Self { operator_pool }
    }

    /// Resolve the durable identity assigned to one exactly persisted plan.
    ///
    /// This lookup uses the same immutable uniqueness fields as enqueue so a
    /// passive lifecycle observer can correlate planning with the worker that
    /// later claims the task without influencing task ownership.
    ///
    /// # Errors
    ///
    /// Returns SQL errors or [`SqlError::InvariantViolation`] when the exact
    /// acknowledged plan cannot be resolved after its enqueue transaction.
    pub async fn task_id_for_plan(&self, task: &NewForgeTask) -> Result<Uuid, SqlError> {
        sqlx::query_scalar(
            "SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4 AND strategy=$5 AND base_snapshot_id=$6 AND plan_hash=$7",
        )
        .bind(task.data_tenant_id.as_uuid())
        .bind(&task.table_ref.catalog)
        .bind(&task.table_ref.namespace)
        .bind(&task.table_ref.table)
        .bind(task.strategy.as_str())
        .bind(task.base_snapshot_id)
        .bind(task.plan_hash.as_slice())
        .fetch_optional(self.operator_pool.pool())
        .await
        .map_err(SqlError::from)?
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: "acknowledged Forge plan has no durable task identity".to_owned(),
        })
    }

    /// Coalesces one tenant-authenticated Scribe hint and advances its generation.
    ///
    /// # Errors
    /// Returns conflict when the supplied tenant differs from the connection
    /// binding, or SQL errors when the upsert cannot complete.
    ///
    /// # Cancellation
    /// The single statement either advances the durable generation or has no effect.
    pub async fn upsert_hint(
        &self,
        conn: &mut TenantConn<'_>,
        data_tenant_id: DataTenantId,
        table: &ForgeTaskTableIdentity,
    ) -> Result<i64, SqlError> {
        if data_tenant_id != conn.data_tenant_id() {
            return Err(SqlError::Conflict {
                detail: "Forge hint tenant does not match TenantConn binding".to_owned(),
            });
        }
        sqlx::query_scalar("INSERT INTO vala.forge_planning_demands (data_tenant_id,catalog_name,namespace_name,table_name,last_source) VALUES ($1,$2,$3,$4,'hint') ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name) DO UPDATE SET last_requested_at=statement_timestamp(),last_source='hint',generation=vala.forge_planning_demands.generation+1 RETURNING generation")
            .bind(data_tenant_id.as_uuid()).bind(&table.catalog).bind(&table.namespace).bind(&table.table)
            .fetch_one(&mut **conn.transaction()).await.map_err(SqlError::from)
    }

    /// Coalesces periodic roster repair through the same durable demand ingress.
    ///
    /// # Errors
    /// Returns SQL errors or overflow errors from the positive generation check.
    ///
    /// # Cancellation
    /// The single statement is atomic.
    pub async fn upsert_periodic(
        &self,
        data_tenant_id: DataTenantId,
        table: &ForgeTaskTableIdentity,
    ) -> Result<i64, SqlError> {
        sqlx::query_scalar("INSERT INTO vala.forge_planning_demands (data_tenant_id,catalog_name,namespace_name,table_name,last_source) VALUES ($1,$2,$3,$4,'periodic') ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name) DO UPDATE SET last_requested_at=statement_timestamp(),last_source='periodic',generation=vala.forge_planning_demands.generation+1 RETURNING generation")
            .bind(data_tenant_id.as_uuid()).bind(&table.catalog).bind(&table.namespace).bind(&table.table)
            .fetch_one(self.operator_pool.pool()).await.map_err(SqlError::from)
    }

    /// Lists a bounded tenant-ring page and returns each observed CAS generation.
    ///
    /// The page is a round-robin interleave: demands are ranked per tenant by
    /// how long they have waited, and the page takes every tenant's oldest
    /// demand before any tenant's second, so one tenant with many tables still
    /// cannot fill a bounded page ahead of its neighbours. Taking only rank one
    /// would bound a tenant to one table per pass, and because every re-request
    /// resets `last_requested_at`, a table that is re-demanded on each pass
    /// would then hold rank one forever and starve its siblings indefinitely.
    ///
    /// # Errors
    /// Returns conflict for a zero bound or stale exact scheduler fence and
    /// fails closed on malformed rows.
    ///
    /// # Cancellation
    /// This read has no durable partial progress.
    pub async fn planning_demands(
        &self,
        owner: Uuid,
        scheduler_fence: i64,
        cap: u32,
    ) -> Result<(Vec<ForgePlanningDemand>, bool), SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "planning demand cap must be positive".to_owned(),
            });
        }
        let rows = sqlx::query_as::<_, ForgePlanningDemandSqlRow>("WITH scheduler AS MATERIALIZED (SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp()), ranked AS MATERIALIZED (SELECT d.*,row_number() OVER (PARTITION BY d.data_tenant_id ORDER BY d.last_requested_at,d.catalog_name,d.namespace_name,d.table_name) AS tenant_rank FROM vala.forge_planning_demands d) SELECT d.data_tenant_id,d.catalog_name,d.namespace_name,d.table_name,d.first_requested_at,d.last_requested_at,d.last_source,d.generation,d.acknowledged_snapshot_id,d.acknowledged_commit_count FROM ranked d CROSS JOIN scheduler s ORDER BY d.tenant_rank,(s.last_tenant_id IS NULL OR d.data_tenant_id>s.last_tenant_id) DESC,d.data_tenant_id LIMIT $3")
            .bind(owner).bind(scheduler_fence).bind(i64::from(cap) + 1).fetch_all(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vala.forge_scheduler_state WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp())")
            .bind(owner).bind(scheduler_fence).fetch_one(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        if !live {
            return Err(SqlError::Conflict {
                detail: "Forge scheduler fence is stale".to_owned(),
            });
        }
        let overflowed = rows.len() > cap as usize;
        let demands = rows
            .into_iter()
            .take(cap as usize)
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((demands, overflowed))
    }

    /// Reloads one exact demand while validating the scheduler's current fence.
    ///
    /// A writer or terminal worker can advance this demand after a scheduler
    /// has planned it but before its acknowledgement transaction commits.
    /// Returning the newest row lets the scheduler coalesce that optimistic
    /// race without relaxing its owner or generation checks.
    ///
    /// # Errors
    /// Returns a stale-fence conflict or SQL and persisted-row decoding errors.
    ///
    /// # Cancellation
    /// This read has no durable partial progress.
    pub async fn refresh_demand(
        &self,
        owner: Uuid,
        scheduler_fence: i64,
        demand: &ForgePlanningDemand,
    ) -> Result<Option<ForgePlanningDemand>, SqlError> {
        let row = sqlx::query_as::<_, ForgePlanningDemandSqlRow>("WITH scheduler AS MATERIALIZED (SELECT singleton FROM vala.forge_scheduler_state WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp()) SELECT d.data_tenant_id,d.catalog_name,d.namespace_name,d.table_name,d.first_requested_at,d.last_requested_at,d.last_source,d.generation,d.acknowledged_snapshot_id,d.acknowledged_commit_count FROM vala.forge_planning_demands d CROSS JOIN scheduler WHERE d.data_tenant_id=$3 AND d.catalog_name=$4 AND d.namespace_name=$5 AND d.table_name=$6")
            .bind(owner).bind(scheduler_fence).bind(demand.data_tenant_id.as_uuid()).bind(&demand.table_ref.catalog).bind(&demand.table_ref.namespace).bind(&demand.table_ref.table)
            .fetch_optional(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        if let Some(row) = row {
            return row.try_into().map(Some);
        }
        let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vala.forge_scheduler_state WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp())")
            .bind(owner).bind(scheduler_fence).fetch_one(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        if live {
            Ok(None)
        } else {
            Err(SqlError::Conflict {
                detail: "Forge scheduler fence is stale".to_owned(),
            })
        }
    }

    /// Atomically enqueues all exact plans and CAS-acknowledges their source demand.
    ///
    /// The scheduler fence is revalidated before any insert. A concurrent newer
    /// demand generation aborts the transaction so task, audit, acknowledgement,
    /// and cursor state cannot commit against different generations.
    ///
    /// # Errors
    /// Returns validation, fencing, or SQL errors. Any error rolls back all inserts.
    ///
    /// # Cancellation
    /// Cancellation rolls back the transaction, retaining the demand.
    pub async fn enqueue_and_acknowledge<F>(
        &self,
        owner: Uuid,
        scheduler_fence: i64,
        demand: &ForgePlanningDemand,
        batch: ForgeEnqueueBatch<'_>,
        unschedulable_event: F,
    ) -> Result<u64, SqlError>
    where
        F: Fn(Uuid) -> AuditEvent,
    {
        if batch
            .executable
            .iter()
            .chain(batch.unschedulable)
            .any(|task| {
                task.data_tenant_id != demand.data_tenant_id || task.table_ref != demand.table_ref
            })
        {
            return Err(SqlError::Conflict {
                detail: "planned task does not match Forge demand binding".to_owned(),
            });
        }
        let mut tx = self
            .operator_pool
            .pool()
            .begin()
            .await
            .map_err(SqlError::from)?;
        let fenced: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vala.forge_scheduler_state WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp() FOR UPDATE)")
            .bind(owner).bind(scheduler_fence).fetch_one(&mut *tx).await.map_err(SqlError::from)?;
        if !fenced {
            return Err(SqlError::Conflict {
                detail: "Forge scheduler fence is stale".to_owned(),
            });
        }
        let mut inserted_count = 0_u64;
        for task in batch.executable {
            task.plan.validate_for_strategy(task.strategy, false)?;
            if task.strategy == ForgeTaskStrategy::ExpiredCleanup
                && !Self::admit_cleanup_handoff(&mut tx, task).await?
            {
                continue;
            }
            task.estimates.validate()?;
            let envelope = task.estimates.envelope.ok_or_else(|| SqlError::Conflict {
                detail: "new Forge tasks require an executable envelope".to_owned(),
            })?;
            let plan = crate::row_types::forge_tasks::plan_to_value(&task.plan);
            let envelope_json = serde_json::json!({"version":envelope.version,"decoded_batch":envelope.decoded_batch_bytes,"decoded_input":envelope.decoded_input_bytes,"sort_working":envelope.sort_working_bytes,"sort_merge":envelope.sort_merge_reservation_bytes,"encoder":envelope.encoder_buffer_bytes,"upload":envelope.upload_chunk_bytes,"footer_encoded":envelope.footer_encoded_bytes,"footer_decode":envelope.footer_decode_workspace_bytes,"sort_spill":envelope.sort_spill_bytes});
            let inserted = sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,envelope_version,decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,sort_spill_bytes,state,ready_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,(($17::jsonb)->>'version')::smallint,(($17::jsonb)->>'decoded_batch')::bigint,(($17::jsonb)->>'decoded_input')::bigint,(($17::jsonb)->>'sort_working')::bigint,(($17::jsonb)->>'sort_merge')::bigint,(($17::jsonb)->>'encoder')::bigint,(($17::jsonb)->>'upload')::bigint,(($17::jsonb)->>'footer_encoded')::bigint,(($17::jsonb)->>'footer_decode')::bigint,(($17::jsonb)->>'sort_spill')::bigint,'ready',$18) ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan_hash) DO NOTHING")
                .bind(Uuid::now_v7()).bind(task.data_tenant_id.as_uuid()).bind(&task.table_ref.catalog).bind(&task.table_ref.namespace).bind(&task.table_ref.table).bind(task.strategy.as_str()).bind(task.lane.as_str()).bind(task.base_snapshot_id).bind(plan).bind(task.plan_hash.as_slice()).bind(i64::from(task.estimates.files)).bind(i64::try_from(task.estimates.bytes).map_err(|_|SqlError::Conflict{detail:"estimated bytes overflow".to_owned()})?).bind(i32::from(task.estimates.parallelism)).bind(i64::try_from(task.estimates.memory_bytes).map_err(|_|SqlError::Conflict{detail:"memory estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.spill_bytes).map_err(|_|SqlError::Conflict{detail:"spill estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.large_ceiling_bytes).map_err(|_|SqlError::Conflict{detail:"large ceiling overflow".to_owned()})?).bind(envelope_json).bind(task.ready_at).execute(&mut *tx).await.map_err(SqlError::from)?;
            inserted_count = inserted_count.saturating_add(inserted.rows_affected());
        }
        for task in batch.unschedulable {
            if task.strategy == ForgeTaskStrategy::ExpiredCleanup {
                return Err(SqlError::Conflict {
                    detail: "expired cleanup carries a fixed bounded projection and is never unschedulable"
                        .to_owned(),
                });
            }
            task.plan.validate_for_strategy(task.strategy, false)?;
            task.estimates.validate()?;
            let envelope = task.estimates.envelope.ok_or_else(|| SqlError::Conflict {
                detail: "new Forge tasks require an executable envelope".to_owned(),
            })?;
            let plan = crate::row_types::forge_tasks::plan_to_value(&task.plan);
            let envelope_json = serde_json::json!({"version":envelope.version,"decoded_batch":envelope.decoded_batch_bytes,"decoded_input":envelope.decoded_input_bytes,"sort_working":envelope.sort_working_bytes,"sort_merge":envelope.sort_merge_reservation_bytes,"encoder":envelope.encoder_buffer_bytes,"upload":envelope.upload_chunk_bytes,"footer_encoded":envelope.footer_encoded_bytes,"footer_decode":envelope.footer_decode_workspace_bytes,"sort_spill":envelope.sort_spill_bytes});
            let task_id = Uuid::now_v7();
            let inserted: Option<Uuid> = sqlx::query_scalar("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,envelope_version,decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,sort_spill_bytes,state,ready_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,(($17::jsonb)->>'version')::smallint,(($17::jsonb)->>'decoded_batch')::bigint,(($17::jsonb)->>'decoded_input')::bigint,(($17::jsonb)->>'sort_working')::bigint,(($17::jsonb)->>'sort_merge')::bigint,(($17::jsonb)->>'encoder')::bigint,(($17::jsonb)->>'upload')::bigint,(($17::jsonb)->>'footer_encoded')::bigint,(($17::jsonb)->>'footer_decode')::bigint,(($17::jsonb)->>'sort_spill')::bigint,'unschedulable',$18) ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan_hash) DO NOTHING RETURNING task_id")
                .bind(task_id).bind(task.data_tenant_id.as_uuid()).bind(&task.table_ref.catalog).bind(&task.table_ref.namespace).bind(&task.table_ref.table).bind(task.strategy.as_str()).bind(task.lane.as_str()).bind(task.base_snapshot_id).bind(plan).bind(task.plan_hash.as_slice()).bind(i64::from(task.estimates.files)).bind(i64::try_from(task.estimates.bytes).map_err(|_|SqlError::Conflict{detail:"estimated bytes overflow".to_owned()})?).bind(i32::from(task.estimates.parallelism)).bind(i64::try_from(task.estimates.memory_bytes).map_err(|_|SqlError::Conflict{detail:"memory estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.spill_bytes).map_err(|_|SqlError::Conflict{detail:"spill estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.large_ceiling_bytes).map_err(|_|SqlError::Conflict{detail:"large ceiling overflow".to_owned()})?).bind(envelope_json).bind(task.ready_at)
                .fetch_optional(&mut *tx).await.map_err(SqlError::from)?;
            if let Some(inserted_id) = inserted {
                inserted_count = inserted_count.saturating_add(1);
                let event = unschedulable_event(inserted_id);
                validate_audit_event(&event, inserted_id, ForgeTaskState::Unschedulable)?;
                OperatorAudit::new(demand.data_tenant_id, &mut tx)
                    .append(&event)
                    .await?;
            }
        }
        let acknowledged = if batch.executable.is_empty()
            && batch.unschedulable.is_empty()
            && demand.acknowledged_snapshot_id.is_some()
        {
            sqlx::query("UPDATE vala.forge_planning_demands SET last_requested_at=statement_timestamp() WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4 AND generation=$5")
                .bind(demand.data_tenant_id.as_uuid()).bind(&demand.table_ref.catalog).bind(&demand.table_ref.namespace).bind(&demand.table_ref.table).bind(demand.generation).execute(&mut *tx).await.map_err(SqlError::from)?.rows_affected()
        } else {
            sqlx::query("DELETE FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4 AND generation=$5")
                .bind(demand.data_tenant_id.as_uuid()).bind(&demand.table_ref.catalog).bind(&demand.table_ref.namespace).bind(&demand.table_ref.table).bind(demand.generation).execute(&mut *tx).await.map_err(SqlError::from)?.rows_affected()
        };
        if acknowledged != 1 {
            return Err(SqlError::ForgeDemandGenerationChanged);
        }
        {
            let advanced = sqlx::query("UPDATE vala.forge_scheduler_state SET last_tenant_id=$3,updated_at=statement_timestamp() WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp()")
                .bind(owner).bind(scheduler_fence).bind(demand.data_tenant_id.as_uuid()).execute(&mut *tx).await.map_err(SqlError::from)?.rows_affected();
            if advanced != 1 {
                return Err(SqlError::Conflict {
                    detail: "Forge scheduler fence was lost before cursor advance".to_owned(),
                });
            }
        }
        tx.commit().await.map_err(SqlError::from)?;
        Ok(inserted_count)
    }

    /// Reads bounded authoritative pending demand and nonterminal task status.
    ///
    /// # Errors
    /// Returns conflict for zero capacity and SQL errors from the bounded union.
    pub async fn planning_status(
        &self,
        cap: u32,
    ) -> Result<(u64, Option<DateTime<Utc>>, bool), SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "planning status cap must be positive".to_owned(),
            });
        }
        let rows: Vec<(DateTime<Utc>,)> = sqlx::query_as("SELECT requested_at FROM (SELECT first_requested_at AS requested_at FROM vala.forge_planning_demands UNION ALL SELECT ready_at AS requested_at FROM vala.forge_tasks WHERE state IN ('ready','retryable','claimed','running','prepared')) pending ORDER BY requested_at LIMIT $1")
            .bind(i64::from(cap) + 1).fetch_all(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        let overflowed = rows.len() > cap as usize;
        let visible = rows
            .into_iter()
            .take(cap as usize)
            .map(|row| row.0)
            .collect::<Vec<_>>();
        Ok((
            u64::try_from(visible.len()).map_err(|_| SqlError::InvariantViolation {
                detail: "Forge backlog count overflow".to_owned(),
            })?,
            visible.first().copied(),
            overflowed,
        ))
    }

    /// Acquires or renews the singleton scheduler fence and returns its generation.
    ///
    /// # Errors
    /// Returns conflict for a zero lease and SQL errors when fencing cannot be persisted.
    ///
    /// # Cancellation
    /// The single update either advances the fence completely or has no effect.
    pub async fn acquire_scheduler(
        &self,
        owner: Uuid,
        lease_seconds: u32,
    ) -> Result<Option<i64>, SqlError> {
        if lease_seconds == 0 {
            return Err(SqlError::Conflict {
                detail: "scheduler lease must be positive".to_owned(),
            });
        }
        sqlx::query_scalar("UPDATE vala.forge_scheduler_state SET owner=$1,fencing_token=fencing_token+1,expires_at=statement_timestamp()+($2*interval '1 second'),updated_at=statement_timestamp() WHERE singleton AND (expires_at IS NULL OR expires_at<statement_timestamp() OR owner=$1) RETURNING fencing_token").bind(owner).bind(i64::from(lease_seconds)).fetch_optional(self.operator_pool.pool()).await.map_err(SqlError::from)
    }

    /// Renews one live exact scheduler owner and token without changing its generation.
    ///
    /// # Errors
    ///
    /// Returns conflict for a zero or unrepresentable lease and when the exact
    /// owner/token is expired or replaced, plus SQL errors from the update.
    ///
    /// # Cancellation
    ///
    /// The single update either extends the exact generation or has no effect.
    pub async fn renew_scheduler(
        &self,
        owner: Uuid,
        scheduler_fence: i64,
        lease: Duration,
    ) -> Result<(), SqlError> {
        let lease_millis = i64::try_from(lease.as_millis()).map_err(|_| SqlError::Conflict {
            detail: "scheduler lease exceeds PostgreSQL millisecond range".to_owned(),
        })?;
        if lease_millis == 0 {
            return Err(SqlError::Conflict {
                detail: "scheduler lease must be positive".to_owned(),
            });
        }
        let changed = sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()+($3*interval '1 millisecond'),updated_at=statement_timestamp() WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp()")
            .bind(owner).bind(scheduler_fence).bind(lease_millis).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?.rows_affected();
        if changed != 1 {
            return Err(SqlError::Conflict {
                detail: "Forge scheduler fence is stale".to_owned(),
            });
        }
        Ok(())
    }

    /// Idempotently enqueues a validated plan and returns its stable task ID.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for invalid plan or estimates and a SQL
    /// error when the operator transaction cannot persist the task.
    ///
    /// # Cancellation
    /// Cancellation before statement completion leaves no partial row; the
    /// single statement either inserts or returns the existing idempotency row.
    pub async fn enqueue(&self, task: &NewForgeTask) -> Result<Uuid, SqlError> {
        if task.strategy == ForgeTaskStrategy::ExpiredCleanup {
            return Err(SqlError::Conflict {
                detail: "expired cleanup requires a validated snapshot-expiration handoff"
                    .to_owned(),
            });
        }
        task.plan.validate_for_strategy(task.strategy, false)?;
        task.estimates.validate()?;
        let envelope = task.estimates.envelope.ok_or_else(|| SqlError::Conflict {
            detail: "new Forge tasks require an executable envelope".to_owned(),
        })?;
        let plan = crate::row_types::forge_tasks::plan_to_value(&task.plan);
        let task_id = Uuid::now_v7();
        sqlx::query_scalar(r#"INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,envelope_version,decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,sort_spill_bytes,state,ready_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,'ready',$27) ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan_hash) DO UPDATE SET updated_at=vala.forge_tasks.updated_at RETURNING task_id"#)
            .bind(task_id).bind(task.data_tenant_id.as_uuid()).bind(&task.table_ref.catalog).bind(&task.table_ref.namespace).bind(&task.table_ref.table).bind(task.strategy.as_str()).bind(task.lane.as_str()).bind(task.base_snapshot_id).bind(plan).bind(task.plan_hash.as_slice()).bind(i64::from(task.estimates.files)).bind(i64::try_from(task.estimates.bytes).map_err(|_|SqlError::Conflict{detail:"estimated bytes overflow".to_owned()})?).bind(i32::from(task.estimates.parallelism)).bind(i64::try_from(task.estimates.memory_bytes).map_err(|_|SqlError::Conflict{detail:"memory estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.spill_bytes).map_err(|_|SqlError::Conflict{detail:"spill estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.large_ceiling_bytes).map_err(|_|SqlError::Conflict{detail:"large ceiling overflow".to_owned()})?).bind(i16::try_from(envelope.version).map_err(|_|SqlError::Conflict{detail:"envelope version overflow".to_owned()})?).bind(i64::try_from(envelope.decoded_batch_bytes).map_err(|_|SqlError::Conflict{detail:"decoded batch overflow".to_owned()})?).bind(i64::try_from(envelope.decoded_input_bytes).map_err(|_|SqlError::Conflict{detail:"decoded input overflow".to_owned()})?).bind(i64::try_from(envelope.sort_working_bytes).map_err(|_|SqlError::Conflict{detail:"sort working overflow".to_owned()})?).bind(i64::try_from(envelope.sort_merge_reservation_bytes).map_err(|_|SqlError::Conflict{detail:"sort merge overflow".to_owned()})?).bind(i64::try_from(envelope.encoder_buffer_bytes).map_err(|_|SqlError::Conflict{detail:"encoder buffer overflow".to_owned()})?).bind(i64::try_from(envelope.upload_chunk_bytes).map_err(|_|SqlError::Conflict{detail:"upload chunk overflow".to_owned()})?).bind(i64::try_from(envelope.footer_encoded_bytes).map_err(|_|SqlError::Conflict{detail:"footer encoded overflow".to_owned()})?).bind(i64::try_from(envelope.footer_decode_workspace_bytes).map_err(|_|SqlError::Conflict{detail:"footer decode workspace overflow".to_owned()})?).bind(i64::try_from(envelope.sort_spill_bytes).map_err(|_|SqlError::Conflict{detail:"sort spill overflow".to_owned()})?).bind(task.ready_at).fetch_one(self.operator_pool.pool()).await.map_err(SqlError::from)
    }

    /// Claims one FIFO task for the next eligible tenant in the durable ring.
    ///
    /// The worker-cursor row is locked first. PostgreSQL 16-compatible
    /// `FOR UPDATE SKIP LOCKED` then selects one row and advances the cursor in
    /// the same transaction. Three durable bounds govern concurrency (D78):
    /// the retained per-table publication index
    /// (`forge_tasks_publication_active`) admits one active task per
    /// (tenant, catalog, namespace, table); the per-tenant active cap
    /// (`max_active_per_tenant`) bounds a single tenant's fan-out; and a
    /// per-owner one-active-large rule lets `large_singleton` candidates claim
    /// concurrently on distinct tables across workers while refusing a second
    /// large claim by an owner that already holds one. There is no cluster-wide
    /// large-lane singleton. The returned attempt UUID is a generation for
    /// output identity, not publication authority.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for zero limits, invariant errors for
    /// malformed persisted rows, and query errors for transaction failures.
    ///
    /// A non-empty `strategy_filter` restricts this claim to the listed
    /// strategies, leaving fairness, tenancy, lane, and size selection
    /// otherwise unchanged. A reserved maintenance worker slot uses it to try
    /// maintenance strategies first and then falls back to an unfiltered claim,
    /// so a ready maintenance task is always claimable regardless of compaction
    /// backlog. `None` claims across every strategy, the ordinary worker path.
    ///
    /// # Cancellation
    /// Cancellation rolls back both the claim and the cursor advance.
    pub async fn claim_fair(
        &self,
        owner: Uuid,
        limits: ForgeClaimLimits,
        strategy_filter: Option<&[ForgeTaskStrategy]>,
    ) -> Result<Option<ForgeTaskClaim>, SqlError> {
        self.claim_fair_for_volume(owner, limits, strategy_filter, None)
            .await
    }

    /// Claims fairly while softly deferring a prior failure on the same volume.
    ///
    /// # Errors
    /// Returns the same validation, decoding, and SQL errors as [`Self::claim_fair`].
    pub async fn claim_fair_for_volume(
        &self,
        owner: Uuid,
        limits: ForgeClaimLimits,
        strategy_filter: Option<&[ForgeTaskStrategy]>,
        scratch_volume_identity: Option<&str>,
    ) -> Result<Option<ForgeTaskClaim>, SqlError> {
        if limits.max_active_per_tenant == 0
            || limits.lease_seconds == 0
            || limits.max_files == 0
            || limits.max_bytes == 0
            || limits.max_parallelism == 0
            || limits.max_memory_bytes == 0
            || limits.max_spill_bytes == 0
            || limits.max_large_task_bytes == 0
        {
            return Err(SqlError::Conflict {
                detail: "Forge claim limits must be positive".to_owned(),
            });
        }
        let attempt = Uuid::now_v7();
        // A present-but-empty filter would make nothing claimable; treat it as
        // an unfiltered claim so a misconfigured empty slice never wedges a
        // worker slot.
        let strategy_filter: Option<Vec<String>> = strategy_filter
            .filter(|strategies| !strategies.is_empty())
            .map(|strategies| {
                strategies
                    .iter()
                    .map(|strategy| strategy.as_str().to_owned())
                    .collect()
            });
        let mut tx = self
            .operator_pool
            .pool()
            .begin()
            .await
            .map_err(SqlError::from)?;
        let row = sqlx::query_as::<_, ForgeTaskClaimSqlRow>(FAIR_CLAIM_SQL)
            .bind(owner)
            .bind(i64::from(limits.max_active_per_tenant))
            .bind(attempt)
            .bind(i64::from(limits.lease_seconds))
            .bind(i64::from(limits.max_files))
            .bind(
                i64::try_from(limits.max_bytes).map_err(|_| SqlError::Conflict {
                    detail: "claim max_bytes overflow".to_owned(),
                })?,
            )
            .bind(i32::from(limits.max_parallelism))
            .bind(
                i64::try_from(limits.max_memory_bytes).map_err(|_| SqlError::Conflict {
                    detail: "claim memory limit overflow".to_owned(),
                })?,
            )
            .bind(
                i64::try_from(limits.max_spill_bytes).map_err(|_| SqlError::Conflict {
                    detail: "claim spill limit overflow".to_owned(),
                })?,
            )
            .bind(
                i64::try_from(limits.max_large_task_bytes).map_err(|_| SqlError::Conflict {
                    detail: "claim large limit overflow".to_owned(),
                })?,
            )
            .bind(strategy_filter)
            .bind(scratch_volume_identity)
            .fetch_optional(&mut *tx)
            .await
            .map_err(SqlError::from)?;
        let task = row.map(TryInto::try_into).transpose()?;
        tx.commit().await.map_err(SqlError::from)?;
        Ok(task)
    }

    /// Takes over one expired Prepared attempt without changing its generation.
    ///
    /// Prepared evidence belongs to the committing attempt, so recovery keeps
    /// that attempt UUID while assigning a new compute owner. Recovery is
    /// governed by the same per-owner one-active-large rule as the fair claim
    /// (D78): a `large_singleton` Prepared task is taken over only when the
    /// recovering owner holds no other active large task. There is no separate
    /// lease row to reacquire; the retained per-table publication index and the
    /// task's own `claim_expires_at` expiry are the recovery authority. Because
    /// a candidate's own Prepared row is excluded from the per-owner scan, an
    /// owner that already holds it can renew ownership without self-blocking.
    ///
    /// # Errors
    /// Returns conflict for a zero lease, invariant errors for malformed rows,
    /// and SQL errors when the atomic takeover cannot complete.
    ///
    /// # Cancellation
    /// Cancellation rolls back the task ownership takeover.
    pub async fn claim_prepared_for_reconciliation(
        &self,
        owner: Uuid,
        lease_seconds: u32,
    ) -> Result<Option<ForgePreparedTaskClaim>, SqlError> {
        if lease_seconds == 0 {
            return Err(SqlError::Conflict {
                detail: "Forge reconciliation lease must be positive".to_owned(),
            });
        }
        let sql = format!(
            "WITH candidate AS MATERIALIZED (SELECT task_id,data_tenant_id,lane,attempt_id FROM vala.forge_tasks c WHERE c.state='prepared' AND c.claim_expires_at<statement_timestamp() AND c.attempt_id IS NOT NULL AND (c.lane='ordinary' OR NOT EXISTS (SELECT 1 FROM vala.forge_tasks held WHERE held.claimed_by=$1 AND held.lane='large_singleton' AND held.state IN ('claimed','running','prepared') AND held.task_id<>c.task_id)) ORDER BY c.claim_expires_at,c.task_id FOR UPDATE SKIP LOCKED LIMIT 1), claimed AS (UPDATE vala.forge_tasks t SET claimed_by=$1,claim_expires_at=statement_timestamp()+($2*interval '1 second'),updated_at=statement_timestamp() FROM candidate c WHERE t.task_id=c.task_id RETURNING t.*) SELECT c.data_tenant_id AS execution_tenant_id,{CLAIM_TASK_PROJECTION} FROM claimed t JOIN candidate c USING(task_id)"
        );
        let mut tx = self
            .operator_pool
            .pool()
            .begin()
            .await
            .map_err(SqlError::from)?;
        let row = sqlx::query_as::<_, ForgePreparedTaskClaimSqlRow>(AssertSqlSafe(sql))
            .bind(owner)
            .bind(i64::from(lease_seconds))
            .fetch_optional(&mut *tx)
            .await
            .map_err(SqlError::from)?;
        let task = row.map(TryInto::try_into).transpose()?;
        tx.commit().await.map_err(SqlError::from)?;
        Ok(task)
    }

    /// Moves an exact owned claim to Running and installs its paired watermark.
    ///
    /// # Errors
    /// Returns conflict when task/state/attempt/owner do not match, or SQL errors.
    ///
    /// # Cancellation
    /// The single update is atomic; cancellation cannot expose partial state.
    pub async fn start(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        watermark: SnapshotWatermark,
    ) -> Result<(), SqlError> {
        watermark.validate()?;
        let changed=sqlx::query("UPDATE vala.forge_tasks SET state='running',watermark_snapshot_id=$4,watermark_timestamp_ms=$5,updated_at=statement_timestamp() WHERE task_id=$1 AND state='claimed' AND attempt_id=$2 AND claimed_by=$3 AND claim_expires_at>statement_timestamp()").bind(task_id).bind(attempt).bind(owner).bind(watermark.snapshot_id).bind(watermark.timestamp_ms).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?.rows_affected();
        exact_one(changed, "start")
    }

    /// Extends a live exact claim without producing an audit row.
    ///
    /// Since the cluster-wide large-lane lease was removed (D78), a large task
    /// carries no separate reservation; this single-table update renews only
    /// the task's own `claim_expires_at`, identically for both lanes.
    ///
    /// # Errors
    /// Returns conflict for stale identity/state/expiry, or SQL errors.
    ///
    /// # Cancellation
    /// The single statement cannot commit partial heartbeat progress.
    pub async fn heartbeat(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        lease_seconds: u32,
    ) -> Result<(), SqlError> {
        if lease_seconds == 0 {
            return Err(SqlError::Conflict {
                detail: "heartbeat lease must be positive".to_owned(),
            });
        }
        let changed=sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()+($4*interval '1 second'),updated_at=statement_timestamp() WHERE task_id=$1 AND state IN ('claimed','running','prepared') AND attempt_id=$2 AND claimed_by=$3 AND claim_expires_at>statement_timestamp()").bind(task_id).bind(attempt).bind(owner).bind(i64::from(lease_seconds)).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?.rows_affected();
        exact_one(changed, "heartbeat")
    }

    /// Returns an exact Claimed or Running attempt to Retryable with a new eligibility time.
    ///
    /// Retrying a large task frees it for reclaim purely by clearing its own
    /// ownership columns; with the cluster-wide large-lane lease removed (D78)
    /// there is no separate reservation row to release, so this is one
    /// single-table update for both lanes.
    ///
    /// # Errors
    /// Returns conflict for stale task/attempt/owner/state or SQL errors.
    ///
    /// # Cancellation
    /// The single statement cannot commit partial retry progress.
    pub async fn retry(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        ready_at: DateTime<Utc>,
    ) -> Result<(), SqlError> {
        let changed=sqlx::query("UPDATE vala.forge_tasks SET state='retryable',attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,ready_at=$4,updated_at=statement_timestamp() WHERE task_id=$1 AND state IN ('claimed','running') AND attempt_id=$2 AND claimed_by=$3").bind(task_id).bind(attempt).bind(owner).bind(ready_at).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?.rows_affected();
        exact_one(changed, "retry")
    }

    /// Persists one attempt-consuming failure with bounded exponential backoff.
    ///
    /// The owned attempt is released only by this statement. The returned count
    /// lets the audited worker settlement decide whether the next failure must
    /// terminalize at the fixed attempt bound.
    ///
    /// # Errors
    /// Returns conflict for stale ownership or an unknown closed failure class.
    pub async fn retry_failure(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        failure_class: &str,
        failed_volume_identity: Option<&str>,
    ) -> Result<u32, SqlError> {
        if !matches!(
            failure_class,
            "transient_object_store" | "transient_coordination" | "storage_health"
        ) {
            return Err(SqlError::Conflict {
                detail: "retry failure class is not retryable".to_owned(),
            });
        }
        let count: Option<i32> = sqlx::query_scalar("UPDATE vala.forge_tasks SET state='retryable',attempt_count=attempt_count+1,failure_class=$4,failed_volume_identity=$5,next_eligible_at=statement_timestamp()+LEAST(power(2,attempt_count+1)*interval '30 seconds',interval '15 minutes'),ready_at=statement_timestamp()+LEAST(power(2,attempt_count+1)*interval '30 seconds',interval '15 minutes'),attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,updated_at=statement_timestamp() WHERE task_id=$1 AND state IN ('claimed','running') AND attempt_id=$2 AND claimed_by=$3 RETURNING attempt_count")
            .bind(task_id).bind(attempt).bind(owner).bind(failure_class).bind(failed_volume_identity)
            .fetch_optional(self.operator_pool.pool()).await.map_err(SqlError::from)?;
        let count = count.ok_or_else(|| SqlError::Conflict {
            detail: "retry failure lost attempt ownership".to_owned(),
        })?;
        u32::try_from(count).map_err(|_| SqlError::InvariantViolation {
            detail: "negative Forge attempt count".to_owned(),
        })
    }

    /// Releases one capacity-refused attempt without consuming retry budget.
    ///
    /// # Errors
    /// Returns conflict when the task is no longer owned by the exact attempt.
    pub async fn release_capacity_refused(
        &self,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
    ) -> Result<(), SqlError> {
        let changed = sqlx::query("UPDATE vala.forge_tasks SET state='retryable',failure_class='capacity_refused',next_eligible_at=statement_timestamp(),ready_at=statement_timestamp(),attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,updated_at=statement_timestamp() WHERE task_id=$1 AND state IN ('claimed','running') AND attempt_id=$2 AND claimed_by=$3")
            .bind(task_id)
            .bind(attempt)
            .bind(owner)
            .execute(self.operator_pool.pool())
            .await
            .map_err(SqlError::from)?
            .rows_affected();
        exact_one(changed, "capacity refusal release")
    }

    /// Atomically records Prepared evidence and the supplied tenant audit event.
    ///
    /// # Errors
    /// Returns conflict for stale identity/state, malformed evidence, or SQL/audit errors.
    ///
    /// # Cancellation
    /// Caller-owned transaction rollback removes both state and audit writes.
    pub async fn prepared(
        &self,
        conn: &mut TenantConn<'_>,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        evidence: &ForgeTaskEvidence,
        event: &AuditEvent,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        validate_audit_event(event, task_id, ForgeTaskState::Prepared)?;
        evidence.validate(false)?;
        let identity=sqlx::query_as::<_,(String,String,String)>("SELECT catalog_name,namespace_name,table_name FROM vala.forge_tasks WHERE task_id=$1 AND state IN ('running','prepared') AND attempt_id=$2 AND claimed_by=$3 FOR UPDATE").bind(task_id).bind(attempt).bind(owner).fetch_optional(&mut **conn.transaction()).await.map_err(SqlError::from)?.ok_or_else(||SqlError::Conflict{detail:"Forge Prepared transition did not match exact state, attempt, and owner".to_owned()})?;
        let identity =
            ForgeTaskTableIdentity::new(identity.0, identity.1, identity.2).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "Forge task contains malformed table identity".to_owned(),
                }
            })?;
        evidence.validate_for_table(&identity)?;
        let encoded = crate::row_types::forge_tasks::evidence_to_value(evidence);
        self.audited_transition(
            conn,
            ForgeTaskTransition {
                task_id,
                attempt_id: attempt,
                owner,
                expected: ForgeTaskState::Running,
                next: ForgeTaskState::Prepared,
            },
            Some(encoded),
            event,
        )
        .await
    }

    /// Advances one Prepared cleanup cursor after a completed bounded delete batch.
    ///
    /// # Errors
    /// Returns conflict unless the exact Prepared attempt owns evidence whose
    /// current cursor equals `expected`, or SQL errors while updating it.
    ///
    /// # Cancellation
    /// The caller-owned transaction rolls back the cursor update.
    pub async fn advance_cleanup_cursor(
        &self,
        conn: &mut TenantConn<'_>,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        expected: u32,
        next: u32,
    ) -> Result<(), SqlError> {
        if next <= expected {
            return Err(SqlError::Conflict {
                detail: "Forge cleanup cursor must advance".to_owned(),
            });
        }
        let changed = sqlx::query("UPDATE vala.forge_tasks SET evidence=jsonb_set(evidence,'{deleted_candidate_count}',to_jsonb($5::bigint),false),updated_at=statement_timestamp() WHERE task_id=$1 AND state='prepared' AND attempt_id=$2 AND claimed_by=$3 AND (evidence->>'deleted_candidate_count')::bigint=$4 AND jsonb_array_length(evidence->'cleanup_candidates') >= $5")
            .bind(task_id)
            .bind(attempt)
            .bind(owner)
            .bind(i64::from(expected))
            .bind(i64::from(next))
            .execute(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?
            .rows_affected();
        exact_one(changed, "advance cleanup cursor")?;
        Ok(())
    }

    /// Atomically applies a lifecycle-significant terminal state and audit row.
    ///
    /// # Errors
    /// Returns conflict unless terminal is Succeeded, Unschedulable, Failed, or Cancelled and exact expected identity/state matches.
    ///
    /// # Cancellation
    /// Caller-owned transaction rollback removes both state and audit writes.
    pub async fn terminal(
        &self,
        conn: &mut TenantConn<'_>,
        transition: ForgeTaskTransition,
        event: &AuditEvent,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        if !transition.next.is_terminal() {
            return Err(SqlError::Conflict {
                detail: "requested Forge terminal state is nonterminal".to_owned(),
            });
        }
        validate_audit_event(event, transition.task_id, transition.next)?;
        self.audited_transition(conn, transition, None, event).await
    }

    /// Atomically marks exact Prepared work successful and requests fresh planning.
    ///
    /// The task identity, tenant connection, and table identity are validated
    /// before the audited transition. The successor demand is advanced in the
    /// caller-owned transaction, so a crash cannot expose Succeeded without a
    /// durable request to inspect the newly committed Iceberg snapshot.
    ///
    /// # Errors
    ///
    /// Returns conflict unless the transition is an exact Prepared-to-Succeeded
    /// transition for the supplied tenant/table, or returns audit/SQL errors.
    /// Caller rollback removes both the terminal state and successor demand.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the caller commits rolls back both durable effects.
    pub async fn terminal_and_request_replan(
        &self,
        conn: &mut TenantConn<'_>,
        transition: ForgeTaskTransition,
        table: &ForgeTaskTableIdentity,
        progress_effect: TaskProgressEffect,
        event: &AuditEvent,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        if transition.expected != ForgeTaskState::Prepared
            || transition.next != ForgeTaskState::Succeeded
        {
            return Err(SqlError::Conflict {
                detail: "Forge successful continuation requires Prepared-to-Succeeded".to_owned(),
            });
        }
        let bound: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM vala.forge_tasks WHERE task_id=$1 AND data_tenant_id=$2 AND catalog_name=$3 AND namespace_name=$4 AND table_name=$5)",
        )
        .bind(transition.task_id)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(&table.catalog)
        .bind(&table.namespace)
        .bind(&table.table)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        if !bound {
            return Err(SqlError::Conflict {
                detail: "Forge successful continuation does not match tenant/table binding"
                    .to_owned(),
            });
        }
        let outcome = self.terminal(conn, transition, event).await?;
        sqlx::query("UPDATE vala.forge_tasks SET failure_class=NULL,failed_volume_identity=NULL,next_eligible_at=statement_timestamp() WHERE task_id=$1 AND state='succeeded'")
            .bind(transition.task_id)
            .execute(&mut **conn.transaction()).await.map_err(SqlError::from)?;
        match progress_effect {
            TaskProgressEffect::Progressed => {
                sqlx::query_scalar::<_, i64>("INSERT INTO vala.forge_planning_demands (data_tenant_id,catalog_name,namespace_name,table_name,last_source) VALUES ($1,$2,$3,$4,'periodic') ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name) DO UPDATE SET last_requested_at=statement_timestamp(),last_source='periodic',generation=vala.forge_planning_demands.generation+1,acknowledged_snapshot_id=NULL,acknowledged_commit_count=NULL RETURNING generation")
                    .bind(conn.data_tenant_id().as_uuid()).bind(&table.catalog).bind(&table.namespace).bind(&table.table)
                    .fetch_one(&mut **conn.transaction()).await.map_err(SqlError::from)?;
            }
            TaskProgressEffect::NoOpAcknowledged {
                snapshot_id,
                commit_count,
            } => {
                let commit_count = i64::try_from(commit_count).map_err(|_| SqlError::Conflict {
                    detail: "Forge acknowledged commit count exceeds i64".to_owned(),
                })?;
                sqlx::query_scalar::<_, i64>("INSERT INTO vala.forge_planning_demands (data_tenant_id,catalog_name,namespace_name,table_name,last_source,acknowledged_snapshot_id,acknowledged_commit_count) VALUES ($1,$2,$3,$4,'periodic',$5,$6) ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name) DO UPDATE SET last_requested_at=statement_timestamp(),last_source='periodic',generation=vala.forge_planning_demands.generation+1,acknowledged_snapshot_id=EXCLUDED.acknowledged_snapshot_id,acknowledged_commit_count=EXCLUDED.acknowledged_commit_count RETURNING generation")
                    .bind(conn.data_tenant_id().as_uuid()).bind(&table.catalog).bind(&table.namespace).bind(&table.table).bind(snapshot_id).bind(commit_count)
                    .fetch_one(&mut **conn.transaction()).await.map_err(SqlError::from)?;
            }
        }
        Ok(outcome)
    }

    /// Atomically cancels a superseded claimed attempt and requests a fresh plan.
    ///
    /// The audited terminal transition clears the exact attempt ownership
    /// before the same tenant transaction advances the periodic demand
    /// generation. With the cluster-wide large-lane lease removed (D78), a
    /// cancelled large task frees its per-owner slot by that ownership clear
    /// alone.
    ///
    /// # Errors
    /// Returns conflict unless the transition is an exact Claimed-to-Cancelled
    /// transition for the supplied tenant/table, or returns SQL/audit errors.
    ///
    /// # Cancellation
    /// Caller-owned rollback removes the cancellation, audit, and successor
    /// demand together.
    pub async fn cancel_superseded(
        &self,
        conn: &mut TenantConn<'_>,
        task: &ForgeTaskClaim,
        event: &AuditEvent,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        if task.data_tenant_id != conn.data_tenant_id()
            || task.state != ForgeTaskState::Claimed
            || task.attempt_id.is_none()
            || task.claimed_by.is_none()
        {
            return Err(SqlError::Conflict {
                detail: "superseded Forge task does not match claimed tenant attempt".to_owned(),
            });
        }
        let transition = ForgeTaskTransition {
            task_id: task.task_id,
            attempt_id: task.attempt_id.expect("claimed attempt checked above"),
            owner: task.claimed_by.expect("claimed owner checked above"),
            expected: ForgeTaskState::Claimed,
            next: ForgeTaskState::Cancelled,
        };
        validate_audit_event(event, task.task_id, ForgeTaskState::Cancelled)?;
        let outcome = self
            .audited_transition(conn, transition, None, event)
            .await?;
        sqlx::query_scalar::<_, i64>("INSERT INTO vala.forge_planning_demands (data_tenant_id,catalog_name,namespace_name,table_name,last_source) VALUES ($1,$2,$3,$4,'periodic') ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name) DO UPDATE SET last_requested_at=statement_timestamp(),last_source='periodic',generation=vala.forge_planning_demands.generation+1 RETURNING generation")
            .bind(task.data_tenant_id.as_uuid())
            .bind(&task.table_ref.catalog)
            .bind(&task.table_ref.namespace)
            .bind(&task.table_ref.table)
            .fetch_one(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        Ok(outcome)
    }

    /// Atomically terminalizes unclaimed work that exceeds every capacity lane.
    ///
    /// # Errors
    /// Returns conflict unless the exact task is Ready or Retryable with no
    /// attempt, owner, or watermark, and returns SQL/audit errors.
    ///
    /// # Cancellation
    /// Caller-owned transaction rollback removes both the state and audit row.
    pub async fn unschedulable(
        &self,
        conn: &mut TenantConn<'_>,
        task_id: Uuid,
        event: &AuditEvent,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        validate_audit_event(event, task_id, ForgeTaskState::Unschedulable)?;
        let changed=sqlx::query("UPDATE vala.forge_tasks SET state='unschedulable',updated_at=statement_timestamp() WHERE task_id=$1 AND state IN ('ready','retryable') AND attempt_id IS NULL AND claimed_by IS NULL AND claim_expires_at IS NULL AND watermark_snapshot_id IS NULL AND watermark_timestamp_ms IS NULL").bind(task_id).execute(&mut **conn.transaction()).await.map_err(SqlError::from)?.rows_affected();
        exact_one(changed, "unschedulable")?;
        append_audit(conn, event).await?;
        Ok(ForgeTaskTransitionOutcome::Applied)
    }

    /// Reclaims expired Claimed or Running attempts into Retryable with no audit.
    /// Prepared is intentionally excluded because it may represent an uncertain external effect.
    ///
    /// A reclaimed large task frees its per-owner slot by clearing its own
    /// ownership columns; with the cluster-wide large-lane lease removed (D78)
    /// there is no separate reservation row to release.
    ///
    /// # Errors
    /// Returns SQL errors from the bounded operator update.
    ///
    /// # Cancellation
    /// Reclaim is one bounded statement and cannot commit partial progress.
    pub async fn reclaim_expired(&self, cap: u32) -> Result<u64, SqlError> {
        let changed=sqlx::query("WITH victims AS (SELECT task_id FROM vala.forge_tasks WHERE state IN ('claimed','running') AND claim_expires_at<statement_timestamp() ORDER BY claim_expires_at,task_id FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE vala.forge_tasks t SET state='retryable',attempt_count=attempt_count+1,next_eligible_at=statement_timestamp()+LEAST(power(2,attempt_count+1)*interval '30 seconds',interval '15 minutes'),attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,ready_at=statement_timestamp()+LEAST(power(2,attempt_count+1)*interval '30 seconds',interval '15 minutes'),updated_at=statement_timestamp() FROM victims v WHERE t.task_id=v.task_id").bind(i64::from(cap)).execute(self.operator_pool.pool()).await.map_err(SqlError::from)?.rows_affected();
        Ok(changed)
    }

    /// Reclaims expired attempts and returns their exact scratch ownership identities.
    ///
    /// Prepared attempts remain excluded. The returned identities were captured
    /// under the same row locks that cleared durable ownership.
    ///
    /// # Errors
    /// Returns SQL errors from the bounded reclaim transaction.
    pub async fn reclaim_expired_attempts(&self, cap: u32) -> Result<Vec<(Uuid, Uuid)>, SqlError> {
        sqlx::query_as("WITH victims AS (SELECT task_id,attempt_id FROM vala.forge_tasks WHERE state IN ('claimed','running') AND claim_expires_at<statement_timestamp() AND attempt_id IS NOT NULL ORDER BY claim_expires_at,task_id FOR UPDATE SKIP LOCKED LIMIT $1), updated AS (UPDATE vala.forge_tasks t SET state='retryable',attempt_count=attempt_count+1,next_eligible_at=statement_timestamp()+LEAST(power(2,attempt_count+1)*interval '30 seconds',interval '15 minutes'),attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,ready_at=statement_timestamp()+LEAST(power(2,attempt_count+1)*interval '30 seconds',interval '15 minutes'),updated_at=statement_timestamp() FROM victims v WHERE t.task_id=v.task_id RETURNING v.task_id,v.attempt_id) SELECT task_id,attempt_id FROM updated ORDER BY task_id")
            .bind(i64::from(cap))
            .fetch_all(self.operator_pool.pool())
            .await
            .map_err(SqlError::from)
    }

    /// Reads active watermarks for one table with explicit overflow detection.
    ///
    /// # Errors
    /// Returns conflict for zero cap, invariant errors for malformed pairs, or SQL errors.
    ///
    /// # Cancellation
    /// This read-only operation has no partial durable progress.
    pub async fn watermarks(
        &self,
        conn: &mut TenantConn<'_>,
        identity: &crate::row_types::forge_tasks::ForgeTaskTableIdentity,
        cap: u32,
    ) -> Result<(Vec<SnapshotWatermark>, bool), SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "watermark cap must be positive".to_owned(),
            });
        }
        let rows=sqlx::query_as::<_,(Option<i64>,Option<i64>)>("SELECT watermark_snapshot_id,watermark_timestamp_ms FROM vala.forge_tasks WHERE catalog_name=$1 AND namespace_name=$2 AND table_name=$3 AND state IN ('running','prepared') ORDER BY watermark_timestamp_ms,task_id LIMIT $4").bind(&identity.catalog).bind(&identity.namespace).bind(&identity.table).bind(i64::from(cap)+1).fetch_all(&mut **conn.transaction()).await.map_err(SqlError::from)?;
        let overflowed = rows.len()
            > usize::try_from(cap).map_err(|_| SqlError::Conflict {
                detail: "watermark cap overflow".to_owned(),
            })?;
        let mut values = Vec::with_capacity(rows.len().min(cap as usize));
        for (id, ts) in rows.into_iter().take(cap as usize) {
            match (id, ts) {
                (Some(snapshot_id), Some(timestamp_ms)) => {
                    let value = SnapshotWatermark {
                        snapshot_id,
                        timestamp_ms,
                    };
                    value.validate().map_err(|_| SqlError::InvariantViolation {
                        detail: "invalid persisted Forge watermark timestamp".to_owned(),
                    })?;
                    values.push(value)
                }
                _ => {
                    return Err(SqlError::InvariantViolation {
                        detail: "partial active watermark".to_owned(),
                    });
                }
            }
        }
        Ok((values, overflowed))
    }

    /// Returns a bounded tenant-scoped status page.
    ///
    /// # Errors
    /// Returns conflict for zero cap, invariant errors for malformed rows, or SQL errors.
    ///
    /// # Cancellation
    /// This read-only operation has no partial durable progress.
    pub async fn status(
        &self,
        conn: &mut TenantConn<'_>,
        cap: u32,
    ) -> Result<ForgeTaskPage, SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "status cap must be positive".to_owned(),
            });
        }
        let sql = format!(
            "SELECT {TASK_PROJECTION} FROM vala.forge_tasks ORDER BY updated_at DESC,task_id LIMIT $1"
        );
        let rows = sqlx::query_as::<_, ForgeTaskSqlRow>(AssertSqlSafe(sql))
            .bind(i64::from(cap) + 1)
            .fetch_all(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        let overflowed = rows.len() > cap as usize;
        let tasks: Vec<ForgeTask> = rows
            .into_iter()
            .take(cap as usize)
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        if tasks
            .iter()
            .any(|task| task.data_tenant_id != conn.data_tenant_id())
        {
            return Err(SqlError::InvariantViolation {
                detail: "Forge task tenant does not match TenantConn binding".to_owned(),
            });
        }
        Ok(ForgeTaskPage { tasks, overflowed })
    }

    /// Deletes a bounded tenant-scoped batch of old terminal rows.
    ///
    /// A succeeded snapshot expiration whose evidence still carries candidates
    /// is retained past its retention window until an `expired_cleanup` plan
    /// references its `task_id`. That is the only thing keeping the handoff
    /// reachable between expiration settlement and cleanup enqueue; once the
    /// cleanup row exists the copied plan is authoritative and the source
    /// prunes normally.
    ///
    /// # Errors
    /// Returns SQL errors; nonterminal and Prepared rows are never selected.
    ///
    /// # Cancellation
    /// Deletion is one statement inside the caller transaction; caller rollback
    /// removes the whole bounded batch.
    pub async fn prune_terminal(
        &self,
        conn: &mut TenantConn<'_>,
        before: DateTime<Utc>,
        cap: u32,
    ) -> Result<u64, SqlError> {
        let changed=sqlx::query("WITH victims AS (SELECT task_id FROM vala.forge_tasks source WHERE state IN ('succeeded','unschedulable','failed','cancelled') AND updated_at<$1 AND NOT (source.state = 'succeeded' AND source.strategy = 'snapshot_expiry' AND jsonb_array_length(COALESCE(source.evidence->'cleanup_candidates', '[]'::jsonb)) > 0 AND NOT EXISTS (SELECT 1 FROM vala.forge_tasks cleanup WHERE cleanup.strategy = 'expired_cleanup' AND cleanup.plan #>> '{parameters,source_task_id}' = source.task_id::text)) ORDER BY updated_at,task_id FOR UPDATE SKIP LOCKED LIMIT $2) DELETE FROM vala.forge_tasks t USING victims v WHERE t.task_id=v.task_id").bind(before).bind(i64::from(cap)).execute(&mut **conn.transaction()).await.map_err(SqlError::from)?.rows_affected();
        Ok(changed)
    }

    /// Reads the oldest bounded succeeded expiration handoff this table still owes.
    ///
    /// "Owes" is exactly one condition: a succeeded `snapshot_expiry` task for
    /// this tenant-qualified table whose evidence still carries candidates and
    /// whose `task_id` no `expired_cleanup` plan references yet. The lookup is
    /// bounded to one row and ordered by `(updated_at, task_id)` so repeated
    /// passes drain the backlog oldest-first and deterministically.
    ///
    /// The returned evidence is only a proposal: the authoritative validation
    /// happens inside [`Self::enqueue_and_acknowledge`], which relocks the same
    /// source row and compares it against the plan actually being inserted.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the selected source's
    /// stored evidence is malformed or does not satisfy the handoff contract,
    /// and [`SqlError`] when the bounded read fails.
    ///
    /// # Cancellation
    ///
    /// This read has no durable effect.
    pub async fn unconsumed_expiration_handoff(
        &self,
        tenant: DataTenantId,
        table_ref: &ForgeTaskTableIdentity,
    ) -> Result<Option<(Uuid, ForgeTaskEvidence)>, SqlError> {
        let row: Option<(Uuid, serde_json::Value)> = sqlx::query_as("SELECT source.task_id,source.evidence FROM vala.forge_tasks source WHERE source.data_tenant_id=$1 AND source.catalog_name=$2 AND source.namespace_name=$3 AND source.table_name=$4 AND source.strategy='snapshot_expiry' AND source.state='succeeded' AND jsonb_array_length(COALESCE(source.evidence->'cleanup_candidates', '[]'::jsonb)) > 0 AND NOT EXISTS (SELECT 1 FROM vala.forge_tasks cleanup WHERE cleanup.strategy = 'expired_cleanup' AND cleanup.plan #>> '{parameters,source_task_id}' = source.task_id::text) ORDER BY source.updated_at,source.task_id LIMIT 1")
            .bind(tenant.as_uuid())
            .bind(&table_ref.catalog)
            .bind(&table_ref.namespace)
            .bind(&table_ref.table)
            .fetch_optional(self.operator_pool.pool())
            .await
            .map_err(SqlError::from)?;
        let Some((task_id, evidence)) = row else {
            return Ok(None);
        };
        let evidence = crate::row_types::forge_tasks::evidence_from_json(evidence)?;
        require_handoff_source(&evidence)?;
        Ok(Some((task_id, evidence)))
    }

    /// Validates one proposed cleanup task against its locked source row.
    ///
    /// Returns whether the caller should insert. `false` is the idempotent
    /// replay case: a cleanup row for this exact source already exists carrying
    /// the identical canonical plan, so the handoff is already consumed.
    ///
    /// The source is locked `FOR UPDATE` inside the caller's single scheduler
    /// transaction, so a concurrent prune or state change cannot slip between
    /// validation and insert. After that commit the copied plan is
    /// authoritative and this row is never read again.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the source is missing, pruned, not a
    /// succeeded `snapshot_expiry` task for the same tenant and table, carries
    /// consumed or absent candidates, disagrees with the proposed committed
    /// metadata identity or candidate vector, or when a cleanup row for this
    /// source already exists with a different plan.
    /// Returns [`SqlError::InvariantViolation`] for malformed stored evidence
    /// and [`SqlError`] for statement failures.
    async fn admit_cleanup_handoff(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task: &NewForgeTask,
    ) -> Result<bool, SqlError> {
        let payload = task
            .plan
            .expired_cleanup_payload(ForgeTaskStrategy::ExpiredCleanup, false)?;
        let proposed = crate::row_types::forge_tasks::plan_to_value(&task.plan);
        let existing: Option<serde_json::Value> = sqlx::query_scalar("SELECT plan FROM vala.forge_tasks WHERE strategy='expired_cleanup' AND plan #>> '{parameters,source_task_id}' = $1 FOR UPDATE")
            .bind(payload.source_task_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(SqlError::from)?;
        if let Some(existing) = existing {
            if existing == proposed {
                return Ok(false);
            }
            return Err(SqlError::Conflict {
                detail: "an expired cleanup task already exists for this source with a different plan"
                    .to_owned(),
            });
        }
        let source: Option<(Uuid, String, String, String, String, String, Option<serde_json::Value>)> = sqlx::query_as("SELECT data_tenant_id,catalog_name,namespace_name,table_name,strategy,state,evidence FROM vala.forge_tasks WHERE task_id=$1 FOR UPDATE")
            .bind(payload.source_task_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(SqlError::from)?;
        let Some((tenant, catalog, namespace, table, strategy, state, evidence)) = source else {
            return Err(SqlError::Conflict {
                detail: "expired cleanup names no surviving snapshot-expiration source".to_owned(),
            });
        };
        let evidence = evidence.ok_or_else(|| SqlError::Conflict {
            detail: "expired cleanup source carries no evidence".to_owned(),
        })?;
        let evidence = crate::row_types::forge_tasks::evidence_from_json(evidence)?;
        require_handoff_source(&evidence)?;
        if tenant != task.data_tenant_id.as_uuid()
            || catalog != task.table_ref.catalog
            || namespace != task.table_ref.namespace
            || table != task.table_ref.table
            || strategy != ForgeTaskStrategy::SnapshotExpiry.as_str()
            || state != ForgeTaskState::Succeeded.as_str()
        {
            return Err(SqlError::Conflict {
                detail: "expired cleanup source is not a succeeded expiration for this table"
                    .to_owned(),
            });
        }
        if evidence.committed_snapshot_id != Some(payload.committed_snapshot_id)
            || evidence.committed_metadata_location.as_deref()
                != Some(payload.committed_metadata_location.as_str())
            || evidence.committed_metadata_digest.as_deref()
                != Some(payload.committed_metadata_digest.as_str())
            || evidence.cleanup_candidates != payload.cleanup_candidates
        {
            return Err(SqlError::Conflict {
                detail: "expired cleanup plan is not an exact copy of its source handoff"
                    .to_owned(),
            });
        }
        Ok(true)
    }

    /// Atomically prepares one expired-cleanup candidate for physical deletion.
    ///
    /// This is the durable half of the per-candidate protocol: it opens and
    /// commits its own short operator transaction so no Postgres transaction or
    /// lock is alive while the worker stats or deletes the object. Locks are
    /// taken in the canonical order — live table lease, exact cleanup
    /// task/attempt/current owner, `bifrost_table_maintenance_authority`,
    /// cleanup evidence, then the tenant audit chain.
    ///
    /// The first preparation of a task copies the immutable plan handoff into
    /// evidence and moves `Running -> Prepared`; every later preparation mutates
    /// only the nullable prepared index. Replaying the exact already-prepared
    /// tuple is read-only and emits no audit; every mismatch refuses.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the lease fence is lost, the task,
    /// attempt, owner, table, or claim does not match exactly, the plan and
    /// evidence disagree, the cursor is not `request.index`, a candidate is
    /// already prepared, the named candidate is not the plan's candidate at
    /// that index, or a surviving reader protection frontier is not already on
    /// the committed metadata. Returns [`SqlError::InvariantViolation`] for
    /// malformed stored state and [`SqlError`] for statement failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation drops the uncommitted transaction, so no candidate becomes
    /// prepared and the identical retry is safe.
    pub async fn prepare_expired_cleanup_candidate(
        &self,
        tenant: DataTenantId,
        request: ExpiredCleanupCandidateRequest<'_>,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        validate_cleanup_event(
            request.event,
            request.authority.task_id,
            "forge.expired_cleanup.candidate_prepared",
        )?;
        let mut tx = self
            .operator_pool
            .pool()
            .begin()
            .await
            .map_err(SqlError::from)?;
        bind_tenant(&mut tx, tenant).await?;
        assert_lease_fence(&mut tx, request.authority).await?;
        let locked = lock_cleanup_task(&mut tx, request.authority, request.table).await?;
        let identity = lock_table_authority(&mut tx, tenant, request.table).await?;
        let payload = locked.payload()?;
        require_named_candidate(&payload, request.index, request.candidate)?;

        if let Some(evidence) = locked.evidence.as_ref() {
            require_plan_parity(evidence, &payload)?;
            if evidence.prepared_candidate_index == Some(request.index)
                && evidence.deleted_candidate_count == request.index
            {
                tx.commit().await.map_err(SqlError::from)?;
                return Ok(ForgeTaskTransitionOutcome::AlreadyApplied);
            }
            if evidence.prepared_candidate_index.is_some()
                || evidence.deleted_candidate_count != request.index
            {
                return Err(SqlError::Conflict {
                    detail: "expired cleanup preparation does not match the durable cursor"
                        .to_owned(),
                });
            }
        } else if request.index != 0 || locked.state != ForgeTaskState::Running {
            return Err(SqlError::Conflict {
                detail: "the first expired cleanup preparation must name candidate zero"
                    .to_owned(),
            });
        }

        for record in list_table_protection_in_operator_tx(&mut tx, &identity).await? {
            if !record.frontier.covers(payload.committed_snapshot_id) {
                return Err(SqlError::Conflict {
                    detail: "a reader protection frontier predates the committed expiration"
                        .to_owned(),
                });
            }
        }

        let evidence = ForgeTaskEvidence {
            version: FORGE_TASK_PAYLOAD_VERSION,
            committed_snapshot_id: Some(payload.committed_snapshot_id),
            committed_metadata_location: Some(payload.committed_metadata_location.clone()),
            committed_metadata_digest: Some(payload.committed_metadata_digest.clone()),
            cleanup_candidates: payload.cleanup_candidates.clone(),
            deleted_candidate_count: request.index,
            prepared_candidate_index: Some(request.index),
        };
        evidence.validate(false)?;
        let changed = sqlx::query("UPDATE vala.forge_tasks SET state='prepared',evidence=$5::jsonb,updated_at=statement_timestamp() WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND strategy='expired_cleanup' AND state=$4 AND attempt_id=$2 AND claimed_by=$3")
            .bind(request.authority.task_id)
            .bind(request.authority.attempt_id)
            .bind(request.authority.worker_id)
            .bind(locked.state.as_str())
            .bind(crate::row_types::forge_tasks::evidence_to_value(&evidence).to_string())
            .execute(&mut *tx)
            .await
            .map_err(SqlError::from)?
            .rows_affected();
        exact_one(changed, "expired cleanup candidate preparation")?;
        OperatorAudit::new(tenant, &mut tx)
            .append(request.event)
            .await?;
        tx.commit().await.map_err(SqlError::from)?;
        Ok(ForgeTaskTransitionOutcome::Applied)
    }

    /// Atomically records one prepared candidate's external outcome.
    ///
    /// A proven outcome ([`ExpiredCleanupOutcome::Deleted`] or
    /// [`ExpiredCleanupOutcome::Missing`]) clears the prepared index and
    /// advances the cursor by exactly one in the same commit as its terminal
    /// candidate audit. A refusal or an uncertain acceptance appends only its
    /// audit and leaves the prepared candidate intact, so the same identity
    /// replays it and no blind second delete can precede a fresh stat and
    /// safety proof. A stale owner cannot settle at all.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the lease fence is lost, the task,
    /// attempt, owner, table, or claim does not match exactly, the plan and
    /// evidence disagree, the named candidate is not the prepared candidate, or
    /// no candidate is prepared at `request.index`. Returns
    /// [`SqlError::InvariantViolation`] for malformed stored state and
    /// [`SqlError`] for statement failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation drops the uncommitted transaction, leaving the candidate
    /// prepared and replayable under the same identity.
    pub async fn settle_expired_cleanup_candidate(
        &self,
        tenant: DataTenantId,
        request: ExpiredCleanupCandidateRequest<'_>,
        outcome: ExpiredCleanupOutcome,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        validate_cleanup_event(
            request.event,
            request.authority.task_id,
            outcome.audit_operation(),
        )?;
        let mut tx = self
            .operator_pool
            .pool()
            .begin()
            .await
            .map_err(SqlError::from)?;
        bind_tenant(&mut tx, tenant).await?;
        assert_lease_fence(&mut tx, request.authority).await?;
        let locked = lock_cleanup_task(&mut tx, request.authority, request.table).await?;
        lock_table_authority(&mut tx, tenant, request.table).await?;
        let payload = locked.payload()?;
        require_named_candidate(&payload, request.index, request.candidate)?;
        let mut evidence = locked.evidence.clone().ok_or_else(|| SqlError::Conflict {
            detail: "expired cleanup settlement found no prepared evidence".to_owned(),
        })?;
        require_plan_parity(&evidence, &payload)?;
        if evidence.prepared_candidate_index != Some(request.index) {
            if outcome.advances() && evidence.deleted_candidate_count == request.index + 1 {
                tx.commit().await.map_err(SqlError::from)?;
                return Ok(ForgeTaskTransitionOutcome::AlreadyApplied);
            }
            return Err(SqlError::Conflict {
                detail: "expired cleanup settlement does not name the prepared candidate"
                    .to_owned(),
            });
        }
        if outcome.advances() {
            evidence.prepared_candidate_index = None;
            evidence.deleted_candidate_count = request.index.saturating_add(1);
            evidence.validate(false)?;
            let changed = sqlx::query("UPDATE vala.forge_tasks SET evidence=$6::jsonb,updated_at=statement_timestamp() WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND strategy='expired_cleanup' AND state='prepared' AND attempt_id=$2 AND claimed_by=$3 AND (evidence->>'deleted_candidate_count')::bigint=$4 AND (evidence->>'prepared_candidate_index')::bigint=$5")
                .bind(request.authority.task_id)
                .bind(request.authority.attempt_id)
                .bind(request.authority.worker_id)
                .bind(i64::from(request.index))
                .bind(i64::from(request.index))
                .bind(crate::row_types::forge_tasks::evidence_to_value(&evidence).to_string())
                .execute(&mut *tx)
                .await
                .map_err(SqlError::from)?
                .rows_affected();
            exact_one(changed, "expired cleanup candidate settlement")?;
        }
        OperatorAudit::new(tenant, &mut tx)
            .append(request.event)
            .await?;
        tx.commit().await.map_err(SqlError::from)?;
        Ok(ForgeTaskTransitionOutcome::Applied)
    }

    /// Applies an exact tenant mutation and audit append inside the caller transaction.
    ///
    /// Reaching a terminal state frees the task's per-owner large slot simply by
    /// clearing its ownership columns; with the cluster-wide large-lane lease
    /// removed (D78) there is no separate reservation row to release here.
    ///
    /// # Errors
    /// Returns conflicts for stale transitions and SQL errors for state or audit
    /// writes.
    ///
    /// # Cancellation
    /// The caller transaction rolls back both state and audit.
    async fn audited_transition(
        &self,
        conn: &mut TenantConn<'_>,
        transition: ForgeTaskTransition,
        evidence: Option<serde_json::Value>,
        event: &AuditEvent,
    ) -> Result<ForgeTaskTransitionOutcome, SqlError> {
        let changed=sqlx::query("UPDATE vala.forge_tasks SET state=$5,evidence=COALESCE($6,evidence),attempt_id=CASE WHEN $5='prepared' THEN attempt_id ELSE NULL END,claimed_by=CASE WHEN $5='prepared' THEN claimed_by ELSE NULL END,claim_expires_at=CASE WHEN $5='prepared' THEN claim_expires_at ELSE NULL END,watermark_snapshot_id=CASE WHEN $5='prepared' THEN watermark_snapshot_id ELSE NULL END,watermark_timestamp_ms=CASE WHEN $5='prepared' THEN watermark_timestamp_ms ELSE NULL END,updated_at=statement_timestamp() WHERE task_id=$1 AND state=$4 AND attempt_id=$2 AND claimed_by=$3").bind(transition.task_id).bind(transition.attempt_id).bind(transition.owner).bind(transition.expected.as_str()).bind(transition.next.as_str()).bind(&evidence).execute(&mut **conn.transaction()).await.map_err(SqlError::from)?.rows_affected();
        if changed == 0 && transition.next == ForgeTaskState::Prepared {
            let replay=sqlx::query_as::<_,(String,Option<Uuid>,Option<Uuid>,Option<serde_json::Value>)>("SELECT state,attempt_id,claimed_by,evidence FROM vala.forge_tasks WHERE task_id=$1 FOR UPDATE").bind(transition.task_id).fetch_optional(&mut **conn.transaction()).await.map_err(SqlError::from)?;
            if let Some((state, attempt_id, claimed_by, stored_evidence)) = replay
                && state == ForgeTaskState::Prepared.as_str()
                && attempt_id == Some(transition.attempt_id)
                && claimed_by == Some(transition.owner)
                && stored_evidence == evidence
            {
                return Ok(ForgeTaskTransitionOutcome::AlreadyApplied);
            }
        }
        exact_one(changed, "audited transition")?;
        append_audit(conn, event).await?;
        Ok(ForgeTaskTransitionOutcome::Applied)
    }
}

/// One locked cleanup task row: its immutable plan and current evidence.
struct LockedCleanupTask {
    /// Durable state observed under the row lock.
    state: ForgeTaskState,
    /// Immutable copied plan; the post-enqueue authority for this task.
    plan: serde_json::Value,
    /// Current attempt evidence, absent only before first preparation.
    evidence: Option<ForgeTaskEvidence>,
}

impl LockedCleanupTask {
    /// Decodes the immutable handoff this task was enqueued with.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when the persisted plan is not
    /// the closed expired-cleanup payload.
    fn payload(&self) -> Result<ExpiredCleanupPayload, SqlError> {
        let parameters = self
            .plan
            .get("parameters")
            .ok_or_else(|| SqlError::InvariantViolation {
                detail: "expired cleanup plan parameters are missing".to_owned(),
            })?;
        ExpiredCleanupPayload::from_value(parameters, true)
    }
}

/// Pins the exact live cleanup task, attempt, current owner, unexpired claim,
/// and table under a row lock.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when no row matches that exact identity, and
/// [`SqlError::InvariantViolation`] for malformed persisted state.
async fn lock_cleanup_task(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    authority: &ForgeExpirationAuthority,
    table: &ForgeClaimTable,
) -> Result<LockedCleanupTask, SqlError> {
    let row: Option<(String, serde_json::Value, Option<serde_json::Value>)> = sqlx::query_as("SELECT state,plan,evidence FROM vala.forge_tasks WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND strategy='expired_cleanup' AND state IN ('running','prepared') AND attempt_id=$2 AND claimed_by=$3 AND claim_expires_at>statement_timestamp() AND catalog_name=$4 AND namespace_name=$5 AND table_name=$6 FOR UPDATE")
        .bind(authority.task_id)
        .bind(authority.attempt_id)
        .bind(authority.worker_id)
        .bind(&table.catalog_name)
        .bind(&table.namespace_name)
        .bind(&table.table_name)
        .fetch_optional(&mut **tx)
        .await
        .map_err(SqlError::from)?;
    let Some((state, plan, evidence)) = row else {
        return Err(SqlError::Conflict {
            detail: "expired cleanup did not match an exact live task, attempt, owner, and table"
                .to_owned(),
        });
    };
    Ok(LockedCleanupTask {
        state: state.parse()?,
        plan,
        evidence: evidence
            .map(crate::row_types::forge_tasks::evidence_from_json)
            .transpose()?,
    })
}

/// Requires that `index` names exactly `candidate` in the immutable plan.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the index is past the plan's candidate
/// vector or names a different candidate, which is how a caller that derived,
/// reordered, or listed candidates of its own is refused.
fn require_named_candidate(
    payload: &ExpiredCleanupPayload,
    index: u32,
    candidate: &ForgeCleanupCandidate,
) -> Result<(), SqlError> {
    let named = usize::try_from(index)
        .ok()
        .and_then(|index| payload.cleanup_candidates.get(index));
    if named == Some(candidate) {
        Ok(())
    } else {
        Err(SqlError::Conflict {
            detail: "expired cleanup candidate does not match its immutable plan".to_owned(),
        })
    }
}

/// Requires byte-for-byte parity between cleanup evidence and its plan.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when committed metadata identity or the
/// candidate vector has drifted from the immutable plan.
fn require_plan_parity(
    evidence: &ForgeTaskEvidence,
    payload: &ExpiredCleanupPayload,
) -> Result<(), SqlError> {
    if evidence.committed_snapshot_id == Some(payload.committed_snapshot_id)
        && evidence.committed_metadata_location.as_deref()
            == Some(payload.committed_metadata_location.as_str())
        && evidence.committed_metadata_digest.as_deref()
            == Some(payload.committed_metadata_digest.as_str())
        && evidence.cleanup_candidates == payload.cleanup_candidates
    {
        Ok(())
    } else {
        Err(SqlError::Conflict {
            detail: "expired cleanup evidence has drifted from its immutable plan".to_owned(),
        })
    }
}

/// Requires that a source's evidence still describes an unconsumed handoff.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the source deleted candidates itself,
/// still holds a prepared candidate, carries no candidates, or lacks committed
/// metadata identity.
fn require_handoff_source(evidence: &ForgeTaskEvidence) -> Result<(), SqlError> {
    if evidence.deleted_candidate_count != 0
        || evidence.prepared_candidate_index.is_some()
        || evidence.cleanup_candidates.is_empty()
        || evidence.committed_snapshot_id.is_none()
        || evidence.committed_metadata_location.is_none()
        || evidence.committed_metadata_digest.is_none()
    {
        return Err(SqlError::Conflict {
            detail: "snapshot-expiration handoff is consumed or incomplete".to_owned(),
        });
    }
    Ok(())
}

/// Validates that a per-candidate audit event names this task and boundary.
///
/// # Errors
/// Returns [`SqlError::Conflict`] for mismatched audit identity.
fn validate_cleanup_event(
    event: &AuditEvent,
    task_id: Uuid,
    operation: &str,
) -> Result<(), SqlError> {
    if event.resource != format!("forge-task:{task_id}") || event.operation != operation {
        return Err(SqlError::Conflict {
            detail: "Forge expired-cleanup audit event does not match its task and boundary"
                .to_owned(),
        });
    }
    Ok(())
}

/// Requires an exact single-row transition.
///
/// # Errors
/// Returns [`SqlError::Conflict`] unless exactly one row changed.
fn exact_one(changed: u64, operation: &str) -> Result<(), SqlError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(SqlError::Conflict {
            detail: format!("Forge task {operation} did not match exact state, attempt, and owner"),
        })
    }
}

/// Validates that lifecycle audit identity names the exact task and next state.
///
/// # Errors
/// Returns [`SqlError::Conflict`] for mismatched audit identity.
fn validate_audit_event(
    event: &AuditEvent,
    task_id: Uuid,
    next: ForgeTaskState,
) -> Result<(), SqlError> {
    let expected_resource = format!("forge-task:{task_id}");
    let expected_operation = format!("forge.task.{}", next.as_str());
    if event.resource != expected_resource || event.operation != expected_operation {
        return Err(SqlError::Conflict {
            detail: "Forge task audit event does not match task identity and transition".to_owned(),
        });
    }
    Ok(())
}
