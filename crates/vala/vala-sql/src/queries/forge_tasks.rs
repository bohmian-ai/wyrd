//! Durable PostgreSQL coordination for Forge maintenance work.
//!
//! [`ForgeTasks`] owns bounded enqueue, fair claim, attempt, watermark,
//! lifecycle, status, and retention workflows. Claims assign compute only;
//! they never replace the separate Forge publication lease.

// raw-query grep allowlist: Forge task tables post-date the sqlx offline cache and remain confined to OperatorPool/TenantConn.

use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, types::Uuid};
use wyrd_spec::vala::api::AuditEvent;

use crate::queries::audit_outbox::append_audit;
use crate::row_types::forge_tasks::{
    ForgeTask, ForgeTaskEvidence, ForgeTaskPage, ForgeTaskSqlRow, ForgeTaskState,
    ForgeTaskTableIdentity, ForgeTaskTransition, ForgeTaskTransitionOutcome, NewForgeTask,
    SnapshotWatermark,
};
use crate::{OperatorPool, SqlError, TenantConn};

const TASK_PROJECTION: &str = "task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,evidence,ready_at,created_at,updated_at";

/// Exact PostgreSQL-16 fair-claim statement used by production and scale-plan proof.
pub const FAIR_CLAIM_SQL: &str = r#"WITH scheduler AS MATERIALIZED (SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton AND fencing_token=$1 AND owner=$2 AND expires_at>statement_timestamp() FOR UPDATE), eligible_tenants AS MATERIALIZED (SELECT t.data_tenant_id FROM vala.forge_tasks t CROSS JOIN scheduler s WHERE t.state IN ('ready','retryable') AND t.ready_at<=statement_timestamp() AND (SELECT count(*) FROM vala.forge_tasks active WHERE active.data_tenant_id=t.data_tenant_id AND active.state IN ('claimed','running','prepared')) < $3 AND (((t.lane='ordinary') AND t.estimated_files<=$6 AND t.estimated_bytes<=$7) OR ((t.lane='large_singleton') AND t.estimated_files=1 AND t.estimated_bytes<=LEAST(t.large_task_ceiling_bytes,$11))) AND t.estimated_parallelism<=$8 AND t.estimated_memory_bytes<=$9 AND t.estimated_spill_bytes<=$10 GROUP BY t.data_tenant_id,s.last_tenant_id ORDER BY (t.data_tenant_id>s.last_tenant_id) DESC,t.data_tenant_id LIMIT 1), candidate AS MATERIALIZED (SELECT t.task_id,t.data_tenant_id,t.lane FROM vala.forge_tasks t JOIN eligible_tenants e USING(data_tenant_id) WHERE t.state IN ('ready','retryable') AND t.ready_at<=statement_timestamp() AND (((t.lane='ordinary') AND t.estimated_files<=$6 AND t.estimated_bytes<=$7) OR ((t.lane='large_singleton') AND t.estimated_files=1 AND t.estimated_bytes<=LEAST(t.large_task_ceiling_bytes,$11))) AND t.estimated_parallelism<=$8 AND t.estimated_memory_bytes<=$9 AND t.estimated_spill_bytes<=$10 ORDER BY t.ready_at,t.task_id FOR UPDATE OF t SKIP LOCKED LIMIT 1), large_lock AS MATERIALIZED (UPDATE vala.forge_large_lane_lease l SET task_id=c.task_id,owner=$2,attempt_id=$4,fencing_token=l.fencing_token+1,expires_at=statement_timestamp()+($5*interval '1 second'),updated_at=statement_timestamp() FROM candidate c WHERE l.singleton AND c.lane='large_singleton' AND (l.task_id IS NULL OR l.expires_at<statement_timestamp()) RETURNING l.task_id), claimed AS (UPDATE vala.forge_tasks t SET state='claimed',attempt_id=$4,claimed_by=$2,claim_expires_at=statement_timestamp()+($5*interval '1 second'),updated_at=statement_timestamp() FROM candidate c WHERE t.task_id=c.task_id AND (c.lane='ordinary' OR EXISTS(SELECT 1 FROM large_lock)) RETURNING t.*), cursor_update AS (UPDATE vala.forge_scheduler_state s SET last_tenant_id=c.data_tenant_id,updated_at=statement_timestamp() FROM candidate c WHERE EXISTS(SELECT 1 FROM claimed) RETURNING s.singleton) SELECT task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,evidence,ready_at,created_at,updated_at FROM claimed"#;

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
    /// Maximum singleton large-lane bytes.
    pub max_large_task_bytes: u64,
}

/// Concrete owner of durable Forge task SQL workflows.
#[derive(Debug, Clone, Copy, Default)]
pub struct ForgeTasks;

impl ForgeTasks {
    /// Constructs the durable task owner.
    #[must_use]
    pub const fn new() -> Self {
        Self
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
        op: &OperatorPool,
        owner: Uuid,
        lease_seconds: u32,
    ) -> Result<Option<i64>, SqlError> {
        if lease_seconds == 0 {
            return Err(SqlError::Conflict {
                detail: "scheduler lease must be positive".to_owned(),
            });
        }
        sqlx::query_scalar("UPDATE vala.forge_scheduler_state SET owner=$1,fencing_token=fencing_token+1,expires_at=statement_timestamp()+($2*interval '1 second'),updated_at=statement_timestamp() WHERE singleton AND (expires_at IS NULL OR expires_at<statement_timestamp() OR owner=$1) RETURNING fencing_token").bind(owner).bind(i64::from(lease_seconds)).fetch_optional(op.pool()).await.map_err(SqlError::from)
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
    pub async fn enqueue(&self, op: &OperatorPool, task: &NewForgeTask) -> Result<Uuid, SqlError> {
        task.plan.validate(false)?;
        task.estimates.validate()?;
        let plan = crate::row_types::forge_tasks::plan_to_value(&task.plan);
        let task_id = Uuid::now_v7();
        sqlx::query_scalar(r#"INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,ready_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,'ready',$17) ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan_hash) DO UPDATE SET updated_at=vala.forge_tasks.updated_at RETURNING task_id"#)
            .bind(task_id).bind(task.data_tenant_id.as_uuid()).bind(&task.table_ref.catalog).bind(&task.table_ref.namespace).bind(&task.table_ref.table).bind(task.strategy.as_str()).bind(task.lane.as_str()).bind(task.base_snapshot_id).bind(plan).bind(task.plan_hash.as_slice()).bind(i64::from(task.estimates.files)).bind(i64::try_from(task.estimates.bytes).map_err(|_|SqlError::Conflict{detail:"estimated bytes overflow".to_owned()})?).bind(i32::from(task.estimates.parallelism)).bind(i64::try_from(task.estimates.memory_bytes).map_err(|_|SqlError::Conflict{detail:"memory estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.spill_bytes).map_err(|_|SqlError::Conflict{detail:"spill estimate overflow".to_owned()})?).bind(i64::try_from(task.estimates.large_ceiling_bytes).map_err(|_|SqlError::Conflict{detail:"large ceiling overflow".to_owned()})?).bind(task.ready_at).fetch_one(op.pool()).await.map_err(SqlError::from)
    }

    /// Claims one FIFO task for the next eligible tenant in the durable ring.
    ///
    /// The singleton scheduler row is locked first. PostgreSQL 16-compatible
    /// `FOR UPDATE SKIP LOCKED` then selects one row and advances the cursor in
    /// the same transaction. Large tasks additionally acquire the singleton
    /// large-lane row. The returned attempt UUID is a generation for output
    /// identity, not publication authority.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] for zero limits, invariant errors for
    /// malformed persisted rows, and query errors for transaction failures.
    ///
    /// # Cancellation
    /// Cancellation rolls back the claim, cursor, and large-lane acquisition.
    pub async fn claim_fair(
        &self,
        op: &OperatorPool,
        owner: Uuid,
        scheduler_fence: i64,
        limits: ForgeClaimLimits,
    ) -> Result<Option<ForgeTask>, SqlError> {
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
        let mut tx = op.pool().begin().await.map_err(SqlError::from)?;
        let row = sqlx::query_as::<_, ForgeTaskSqlRow>(FAIR_CLAIM_SQL)
            .bind(scheduler_fence)
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
        op: &OperatorPool,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        watermark: SnapshotWatermark,
    ) -> Result<(), SqlError> {
        watermark.validate()?;
        let changed=sqlx::query("UPDATE vala.forge_tasks SET state='running',watermark_snapshot_id=$4,watermark_timestamp_ms=$5,updated_at=statement_timestamp() WHERE task_id=$1 AND state='claimed' AND attempt_id=$2 AND claimed_by=$3 AND claim_expires_at>statement_timestamp()").bind(task_id).bind(attempt).bind(owner).bind(watermark.snapshot_id).bind(watermark.timestamp_ms).execute(op.pool()).await.map_err(SqlError::from)?.rows_affected();
        exact_one(changed, "start")
    }

    /// Extends a live exact claim without producing an audit row.
    ///
    /// # Errors
    /// Returns conflict for stale identity/state/expiry, or SQL errors.
    ///
    /// # Cancellation
    /// Task and matching large-lane renewal share one statement and cannot
    /// commit partial heartbeat progress.
    pub async fn heartbeat(
        &self,
        op: &OperatorPool,
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
        let changed:i64=sqlx::query_scalar("WITH candidate AS MATERIALIZED (SELECT task_id,lane FROM vala.forge_tasks WHERE task_id=$1 AND state IN ('claimed','running','prepared') AND attempt_id=$2 AND claimed_by=$3 AND claim_expires_at>statement_timestamp() FOR UPDATE), renewed AS (UPDATE vala.forge_large_lane_lease l SET expires_at=statement_timestamp()+($4*interval '1 second'),updated_at=statement_timestamp() FROM candidate c WHERE l.singleton AND c.lane='large_singleton' AND l.task_id=$1 AND l.attempt_id=$2 AND l.owner=$3 AND l.expires_at>statement_timestamp() RETURNING l.task_id), changed AS (UPDATE vala.forge_tasks t SET claim_expires_at=statement_timestamp()+($4*interval '1 second'),updated_at=statement_timestamp() FROM candidate c WHERE t.task_id=c.task_id AND (c.lane='ordinary' OR EXISTS(SELECT 1 FROM renewed)) RETURNING t.task_id) SELECT count(*) FROM changed").bind(task_id).bind(attempt).bind(owner).bind(i64::from(lease_seconds)).fetch_one(op.pool()).await.map_err(SqlError::from)?;
        exact_one(
            u64::try_from(changed).map_err(|_| SqlError::InvariantViolation {
                detail: "negative Forge heartbeat row count".to_owned(),
            })?,
            "heartbeat",
        )
    }

    /// Returns an exact Claimed or Running attempt to Retryable with a new eligibility time.
    ///
    /// # Errors
    /// Returns conflict for stale task/attempt/owner/state or SQL errors.
    ///
    /// # Cancellation
    /// Task retry and matching large-lane release share one statement.
    pub async fn retry(
        &self,
        op: &OperatorPool,
        task_id: Uuid,
        attempt: Uuid,
        owner: Uuid,
        ready_at: DateTime<Utc>,
    ) -> Result<(), SqlError> {
        let changed:i64=sqlx::query_scalar("WITH changed AS (UPDATE vala.forge_tasks SET state='retryable',attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,ready_at=$4,updated_at=statement_timestamp() WHERE task_id=$1 AND state IN ('claimed','running') AND attempt_id=$2 AND claimed_by=$3 RETURNING task_id), released AS (UPDATE vala.forge_large_lane_lease SET task_id=NULL,owner=NULL,attempt_id=NULL,expires_at=NULL,updated_at=statement_timestamp() WHERE singleton AND task_id=$1 AND attempt_id=$2 AND owner=$3 AND EXISTS(SELECT 1 FROM changed) RETURNING singleton) SELECT count(*) FROM changed").bind(task_id).bind(attempt).bind(owner).bind(ready_at).fetch_one(op.pool()).await.map_err(SqlError::from)?;
        exact_one(
            u64::try_from(changed).map_err(|_| SqlError::InvariantViolation {
                detail: "negative Forge retry row count".to_owned(),
            })?,
            "retry",
        )
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
    /// # Errors
    /// Returns SQL errors from the bounded operator update.
    ///
    /// # Cancellation
    /// Reclaim and matching large-lane releases share one bounded statement.
    pub async fn reclaim_expired(&self, op: &OperatorPool, cap: u32) -> Result<u64, SqlError> {
        let changed:i64=sqlx::query_scalar("WITH victims AS (SELECT task_id,attempt_id,claimed_by FROM vala.forge_tasks WHERE state IN ('claimed','running') AND claim_expires_at<statement_timestamp() ORDER BY claim_expires_at,task_id FOR UPDATE SKIP LOCKED LIMIT $1), changed AS (UPDATE vala.forge_tasks t SET state='retryable',attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,ready_at=statement_timestamp(),updated_at=statement_timestamp() FROM victims v WHERE t.task_id=v.task_id RETURNING t.task_id), released AS (UPDATE vala.forge_large_lane_lease l SET task_id=NULL,owner=NULL,attempt_id=NULL,expires_at=NULL,updated_at=statement_timestamp() FROM victims v WHERE l.singleton AND l.task_id=v.task_id AND l.attempt_id=v.attempt_id AND l.owner=v.claimed_by AND EXISTS(SELECT 1 FROM changed c WHERE c.task_id=v.task_id) RETURNING l.singleton) SELECT count(*) FROM changed").bind(i64::from(cap)).fetch_one(op.pool()).await.map_err(SqlError::from)?;
        u64::try_from(changed).map_err(|_| SqlError::InvariantViolation {
            detail: "negative Forge reclaim row count".to_owned(),
        })
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
        let changed=sqlx::query("WITH victims AS (SELECT task_id FROM vala.forge_tasks WHERE state IN ('succeeded','unschedulable','failed','cancelled') AND updated_at<$1 ORDER BY updated_at,task_id FOR UPDATE SKIP LOCKED LIMIT $2) DELETE FROM vala.forge_tasks t USING victims v WHERE t.task_id=v.task_id").bind(before).bind(i64::from(cap)).execute(&mut **conn.transaction()).await.map_err(SqlError::from)?.rows_affected();
        Ok(changed)
    }

    /// Applies an exact tenant mutation and audit append inside the caller transaction.
    ///
    /// # Errors
    /// Returns conflicts for stale transitions and SQL errors for state, lease,
    /// or audit writes.
    ///
    /// # Cancellation
    /// The caller transaction rolls back state, large-lane release, and audit.
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
        if transition.next.is_terminal() {
            let _: bool = sqlx::query_scalar("SELECT vala.release_forge_large_lane($1,$2,$3)")
                .bind(transition.task_id)
                .bind(transition.attempt_id)
                .bind(transition.owner)
                .fetch_one(&mut **conn.transaction())
                .await
                .map_err(SqlError::from)?;
        }
        append_audit(conn, event).await?;
        Ok(ForgeTaskTransitionOutcome::Applied)
    }
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
