//! Replayable progress history of queued Verifier executions.
//!
//! Triggers commit one row of `wyrd.verifier_execution_events` with every
//! execution, invocation, and publication transition. These methods hand
//! those rows to the projector in stable batches, record Scribe's
//! acknowledgement, and report how far one execution's history has been
//! projected. The authoritative progress stays in the execution records;
//! projection lag never hides it.
// raw-query grep allowlist: verifier execution tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::ids::{CardUid, ExecutionId};
use wyrd_spec::vala::ids::RunId;

use super::{VerifierExecutionQueue, stored, timed};
use crate::{OperatorPool, TenantConn};

/// The oldest unacknowledged event's batch, when one was already assigned.
///
/// A batch whose submission had no acknowledgement is resubmitted before any
/// new batch is formed, so an ambiguous outcome never gets a fresh identity.
const ASSIGNED_BATCH_SQL: &str = r"
    SELECT batch_id
      FROM wyrd.verifier_execution_events
     WHERE acknowledged_at IS NULL AND batch_id IS NOT NULL
     ORDER BY sequence
     LIMIT 1
";

/// Assign batch `$2` to up to `$1` of the oldest unbatched events.
///
/// Rows another projector is assigning are skipped, so two projectors never
/// put one event in two batches.
const ASSIGN_BATCH_SQL: &str = r"
    WITH chosen AS (
        SELECT sequence
          FROM wyrd.verifier_execution_events
         WHERE acknowledged_at IS NULL AND batch_id IS NULL
         ORDER BY sequence
         LIMIT $1
           FOR UPDATE SKIP LOCKED
    )
    UPDATE wyrd.verifier_execution_events v
       SET batch_id = $2
      FROM chosen
     WHERE v.sequence = chosen.sequence
";

/// Every event of batch `$1`, in sequence order, with the execution context
/// its Bifrost row is attributed by.
// ponytail: filters the tenant's unacknowledged rows by batch through the
// partial index; index batch_id if projection lag grows large.
const BATCH_EVENTS_SQL: &str = r"
    SELECT v.event_id, v.sequence, v.execution_id, v.event, v.status, v.task_id,
           v.item_index, v.iteration_index, v.attempt, v.publication_name,
           v.publication_kind, v.size_bytes, v.file_count, v.sha256, v.reason, v.value,
           v.occurred_at, e.verifier_card_uid, e.card_uid, e.owner_card_uid, e.run_id,
           e.deployment_environment, e.sub_principal_id, e.act_principal_id
      FROM wyrd.verifier_execution_events v
      JOIN wyrd.verifier_executions e
        ON e.data_tenant_id = v.data_tenant_id AND e.execution_id = v.execution_id
     WHERE v.acknowledged_at IS NULL AND v.batch_id = $1
     ORDER BY v.sequence
";

/// Mark batch `$1` acknowledged.
const ACKNOWLEDGE_SQL: &str = r"
    UPDATE wyrd.verifier_execution_events
       SET acknowledged_at = statement_timestamp()
     WHERE acknowledged_at IS NULL AND batch_id = $1
";

/// Count one unacknowledged submission of batch `$1`.
const DEFER_SQL: &str = r"
    UPDATE wyrd.verifier_execution_events
       SET projection_attempts = projection_attempts + 1
     WHERE acknowledged_at IS NULL AND batch_id = $1
";

/// Every tenant with an unacknowledged event, walked one tenant at a time
/// down the partial index so the scan reads one entry per tenant rather than
/// the whole lag.
const PENDING_TENANTS_SQL: &str = r"
    WITH RECURSIVE pending(tenant) AS (
        (SELECT data_tenant_id FROM wyrd.verifier_execution_events
          WHERE acknowledged_at IS NULL
          ORDER BY data_tenant_id, sequence LIMIT 1)
        UNION ALL
        SELECT (SELECT v.data_tenant_id FROM wyrd.verifier_execution_events v
                 WHERE v.acknowledged_at IS NULL AND v.data_tenant_id > p.tenant
                 ORDER BY v.data_tenant_id, v.sequence LIMIT 1)
          FROM pending p
         WHERE p.tenant IS NOT NULL
    )
    SELECT tenant FROM pending WHERE tenant IS NOT NULL
";

/// How far one execution's history is projected.
const POSITION_SQL: &str = r"
    SELECT count(*) FILTER (WHERE acknowledged_at IS NOT NULL),
           count(*) FILTER (WHERE acknowledged_at IS NULL AND projection_attempts = 0),
           count(*) FILTER (WHERE acknowledged_at IS NULL AND projection_attempts > 0),
           max(sequence) FILTER (WHERE acknowledged_at IS NOT NULL)
      FROM wyrd.verifier_execution_events
     WHERE execution_id = $1
";

/// One committed execution event and the execution context its Bifrost row
/// is attributed by.
///
/// `event` is `execution`, `invocation`, `invocation_abandoned`, or
/// `publication`; the remaining fields are those the event kind records, as
/// `bifrost.verification.execution_events` documents.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionEvent {
    /// Server-assigned event identity.
    pub event_id: Uuid,
    /// Position in the tenant's committed history.
    pub sequence: i64,
    /// The execution the event belongs to.
    pub execution_id: ExecutionId,
    /// Event kind.
    pub event: String,
    /// New execution status, invocation status, or step outcome.
    pub status: Option<String>,
    /// Producing step.
    pub task_id: Option<String>,
    /// Producing fanout item.
    pub item_index: Option<i32>,
    /// Producing iteration.
    pub iteration_index: Option<i32>,
    /// Invocation attempt, or the execution's attempt count.
    pub attempt: Option<i32>,
    /// Publication name.
    pub publication_name: Option<String>,
    /// Publication kind.
    pub publication_kind: Option<String>,
    /// Publication size in bytes.
    pub size_bytes: Option<i64>,
    /// Files a file or directory publication retains.
    pub file_count: Option<i32>,
    /// A file publication's digest.
    pub sha256: Option<String>,
    /// Error code or recorded reason.
    pub reason: Option<String>,
    /// A JSON publication's value.
    pub value: Option<Value>,
    /// Database time the event committed under.
    pub occurred_at: DateTime<Utc>,
    /// The execution's exact Verifier.
    pub verifier_card_uid: CardUid,
    /// The Card the execution verifies.
    pub card_uid: CardUid,
    /// The binding owner, for a bound execution.
    pub owner_card_uid: Option<CardUid>,
    /// The application Run, when the execution names one.
    pub run_id: Option<RunId>,
    /// The deployment environment the execution was accepted with.
    pub deployment_environment: Option<String>,
    /// The execution's subject and accepting actor.
    pub attribution: Attribution,
}

/// A batch of one tenant's events and the identity every submission of it
/// carries.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBatch {
    /// Persisted batch identity.
    pub batch_id: Uuid,
    /// The batch's events, in sequence order.
    pub events: Vec<ExecutionEvent>,
}

/// How far one execution's history has been projected into Bifrost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectionPosition {
    /// Events Scribe acknowledged.
    pub acknowledged: i64,
    /// Events not yet submitted.
    pub pending: i64,
    /// Events submitted without an acknowledgement, to be resubmitted.
    pub failed: i64,
    /// The latest acknowledged event's sequence.
    pub acknowledged_through: Option<i64>,
}

/// A [`BATCH_EVENTS_SQL`] row.
#[derive(sqlx::FromRow)]
struct EventRow {
    /// Event identity.
    event_id: Uuid,
    /// Tenant history position.
    sequence: i64,
    /// Execution identity.
    execution_id: Uuid,
    /// Event kind.
    event: String,
    /// Recorded status.
    status: Option<String>,
    /// Producing step.
    task_id: Option<String>,
    /// Producing item.
    item_index: Option<i32>,
    /// Producing iteration.
    iteration_index: Option<i32>,
    /// Attempt.
    attempt: Option<i32>,
    /// Publication name.
    publication_name: Option<String>,
    /// Publication kind.
    publication_kind: Option<String>,
    /// Publication size.
    size_bytes: Option<i64>,
    /// Publication file count.
    file_count: Option<i32>,
    /// File digest.
    sha256: Option<String>,
    /// Reason.
    reason: Option<String>,
    /// JSON value.
    value: Option<Json<Value>>,
    /// Commit time.
    occurred_at: DateTime<Utc>,
    /// Verifier UID.
    verifier_card_uid: Uuid,
    /// Subject UID.
    card_uid: Uuid,
    /// Binding owner UID.
    owner_card_uid: Option<Uuid>,
    /// Run identity.
    run_id: Option<String>,
    /// Deployment environment.
    deployment_environment: Option<String>,
    /// Subject principal.
    sub_principal_id: Uuid,
    /// Accepting actor.
    act_principal_id: Uuid,
}

impl EventRow {
    /// Decode the row's identities.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when a stored identity is malformed.
    fn into_event(self) -> Result<ExecutionEvent, SqlxError> {
        Ok(ExecutionEvent {
            event_id: self.event_id,
            sequence: self.sequence,
            execution_id: stored(ExecutionId::new(self.execution_id))?,
            event: self.event,
            status: self.status,
            task_id: self.task_id,
            item_index: self.item_index,
            iteration_index: self.iteration_index,
            attempt: self.attempt,
            publication_name: self.publication_name,
            publication_kind: self.publication_kind,
            size_bytes: self.size_bytes,
            file_count: self.file_count,
            sha256: self.sha256,
            reason: self.reason,
            value: self.value.map(|Json(value)| value),
            occurred_at: self.occurred_at,
            verifier_card_uid: stored(CardUid::from_uuid(self.verifier_card_uid))?,
            card_uid: stored(CardUid::from_uuid(self.card_uid))?,
            owner_card_uid: self
                .owner_card_uid
                .map(|uid| stored(CardUid::from_uuid(uid)))
                .transpose()?,
            run_id: self.run_id.map(RunId::from_string),
            deployment_environment: self.deployment_environment,
            attribution: Attribution::new(
                PrincipalId::new(self.sub_principal_id),
                PrincipalId::new(self.act_principal_id),
            ),
        })
    }
}

impl VerifierExecutionQueue {
    /// The next batch of the tenant's events to project, if any.
    ///
    /// A batch already assigned and not acknowledged is returned again,
    /// exactly as first formed, before a new one is assigned; otherwise up to
    /// `limit` of the oldest unbatched events are assigned `fresh`. The
    /// caller commits before submitting, so the assignment survives any
    /// submission outcome and a retry resubmits the same identity.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when a stored identity is malformed.
    pub async fn event_batch(
        &self,
        conn: &mut TenantConn<'_>,
        limit: u32,
        fresh: Uuid,
    ) -> Result<Option<EventBatch>, SqlxError> {
        timed("event_batch", async {
            let assigned: Option<Uuid> = sqlx::query_scalar(ASSIGNED_BATCH_SQL)
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let batch_id = if let Some(batch_id) = assigned {
                batch_id
            } else {
                let assigned = sqlx::query(ASSIGN_BATCH_SQL)
                    .bind(i64::from(limit))
                    .bind(fresh)
                    .execute(&mut **conn.transaction())
                    .await?;
                if assigned.rows_affected() == 0 {
                    return Ok(None);
                }
                fresh
            };
            let rows: Vec<EventRow> = sqlx::query_as(BATCH_EVENTS_SQL)
                .bind(batch_id)
                .fetch_all(&mut **conn.transaction())
                .await?;
            let events = rows
                .into_iter()
                .map(EventRow::into_event)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(EventBatch { batch_id, events }))
        })
        .await
    }

    /// Record that Scribe acknowledged `batch`.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn acknowledge_events(
        &self,
        conn: &mut TenantConn<'_>,
        batch: Uuid,
    ) -> Result<(), SqlxError> {
        timed("acknowledge_events", async {
            sqlx::query(ACKNOWLEDGE_SQL)
                .bind(batch)
                .execute(&mut **conn.transaction())
                .await?;
            Ok(())
        })
        .await
    }

    /// Record that a submission of `batch` was not acknowledged; the batch
    /// keeps its identity and is resubmitted.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn defer_events(
        &self,
        conn: &mut TenantConn<'_>,
        batch: Uuid,
    ) -> Result<(), SqlxError> {
        timed("defer_events", async {
            sqlx::query(DEFER_SQL)
                .bind(batch)
                .execute(&mut **conn.transaction())
                .await?;
            Ok(())
        })
        .await
    }

    /// Every tenant with an event not yet acknowledged.
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored tenant key is malformed.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn tenants_with_pending_events(
        &self,
        operator: &OperatorPool,
    ) -> Result<Vec<DataTenantId>, SqlxError> {
        timed("pending_events_scan", async {
            let rows: Vec<Uuid> = sqlx::query_scalar(PENDING_TENANTS_SQL)
                .fetch_all(operator.pool())
                .await?;
            rows.into_iter()
                .map(|tenant| stored(DataTenantId::new(tenant)))
                .collect()
        })
        .await
    }

    /// How far `execution`'s history has been projected.
    ///
    /// An unknown or other-tenant execution reads as no events.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    pub async fn projection_position(
        &self,
        conn: &mut TenantConn<'_>,
        execution: ExecutionId,
    ) -> Result<ProjectionPosition, SqlxError> {
        timed("projection_position", async {
            let (acknowledged, pending, failed, acknowledged_through): (
                i64,
                i64,
                i64,
                Option<i64>,
            ) = sqlx::query_as(POSITION_SQL)
                .bind(execution.as_uuid())
                .fetch_one(&mut **conn.transaction())
                .await?;
            Ok(ProjectionPosition {
                acknowledged,
                pending,
                failed,
                acknowledged_through,
            })
        })
        .await
    }
}
