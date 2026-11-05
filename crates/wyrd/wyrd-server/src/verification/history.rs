//! Projection of committed execution events into Bifrost history.
//!
//! Every execution, invocation, and publication transition commits its event
//! in `wyrd.verifier_execution_events` with the transition itself.
//! [`EventProjector`] runs beside the Verifier runner: it takes each tenant's
//! oldest batch of unacknowledged events, whose identity the queue commits
//! before it is submitted, and stages it on the Scribe outbox as one frame
//! carrying that identity. Scribe's acknowledgement marks the batch
//! acknowledged; anything else leaves it to be resubmitted under the same
//! identity, which Scribe's batch-id dedup absorbs. Projection lag therefore
//! delays history only: live progress is read from the execution records.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::tables::staged::StagedRow;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_sql::queries::verifier_executions::{EventBatch, VerifierExecutionQueue};
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};

use crate::scribe_outbox::{ScribeOutbox, ScribeWrite};
use crate::verification::results::{ResultPayloadBuilder, ResultPayloadError, ResultRun};

/// Most events one projected batch holds.
const BATCH_EVENTS: u32 = 256;

/// Projects committed execution events through the Scribe outbox.
pub(crate) struct EventProjector {
    /// Wyrd Postgres owner opening each tenant's batch transactions.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant pending scan.
    operator: OperatorPool,
    /// Queue owner of the event records.
    queue: VerifierExecutionQueue,
    /// The outbox batches are staged on.
    outbox: Arc<ScribeOutbox>,
    /// Idle wait between passes.
    poll: Duration,
}

impl EventProjector {
    /// A projector over `postgres` staging on `outbox`, idling `poll`
    /// between passes.
    pub(crate) const fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        queue: VerifierExecutionQueue,
        outbox: Arc<ScribeOutbox>,
        poll: Duration,
    ) -> Self {
        Self {
            postgres,
            operator,
            queue,
            outbox,
            poll,
        }
    }

    /// Project every tenant's pending events, then idle, until `stop`.
    ///
    /// A batch waiting for its acknowledgement when `stop` fires stays
    /// assigned and unacknowledged, so the next projector resubmits it.
    pub(crate) async fn run(&self, stop: CancellationToken) {
        loop {
            match self.queue.tenants_with_pending_events(&self.operator).await {
                Ok(tenants) => {
                    for tenant in tenants {
                        while self.project(tenant, &stop).await {}
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "the pending execution events could not be listed");
                }
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.poll) => {}
            }
        }
    }

    /// Project `tenant`'s next batch; whether it was acknowledged, so the
    /// caller continues with the tenant's next one.
    async fn project(&self, tenant: DataTenantId, stop: &CancellationToken) -> bool {
        let batch = match self.batch(tenant).await {
            Ok(Some(batch)) => batch,
            Ok(None) => return false,
            Err(error) => {
                tracing::warn!(%tenant, %error, "an execution event batch could not be read");
                return false;
            }
        };
        let acked = match Self::rows(&batch) {
            Ok(rows) => {
                let (sender, receiver) = oneshot::channel();
                self.outbox.stage(
                    tenant,
                    ScribeWrite::ExecutionEvents {
                        batch: batch.batch_id,
                        rows,
                        acked: Mutex::new(Some(sender)),
                    },
                );
                tokio::select! {
                    acked = receiver => acked.is_ok(),
                    () = stop.cancelled() => false,
                }
            }
            Err(error) => {
                tracing::error!(%tenant, batch_id = %batch.batch_id, %error, "an execution event batch could not be projected");
                false
            }
        };
        if let Err(error) = self.settle(tenant, batch.batch_id, acked).await {
            tracing::warn!(%tenant, batch_id = %batch.batch_id, %error, "an execution event batch outcome could not be recorded");
            return false;
        }
        acked
    }

    /// Read `tenant`'s next batch, committing a fresh batch's assignment.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the connection, statement, or commit fails.
    async fn batch(&self, tenant: DataTenantId) -> Result<Option<EventBatch>, SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let batch = self
            .queue
            .event_batch(&mut conn, BATCH_EVENTS, Uuid::now_v7())
            .await?;
        conn.commit().await?;
        Ok(batch)
    }

    /// Record whether Scribe acknowledged `batch`.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the connection, statement, or commit fails.
    async fn settle(&self, tenant: DataTenantId, batch: Uuid, acked: bool) -> Result<(), SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        if acked {
            self.queue.acknowledge_events(&mut conn, batch).await?;
        } else {
            self.queue.defer_events(&mut conn, batch).await?;
        }
        conn.commit().await?;
        Ok(())
    }

    /// Each event's row with its execution's subject and actor.
    ///
    /// # Errors
    /// Returns the row error of the first event that cannot be built.
    fn rows(batch: &EventBatch) -> Result<Vec<(StagedRow, Attribution)>, ResultPayloadError> {
        batch
            .events
            .iter()
            .map(|event| {
                let run = ResultRun {
                    execution_id: event.execution_id,
                    run_id: event.run_id.as_ref(),
                    verifier_version: "",
                    card_uid: &event.card_uid,
                    owner_card_uid: event.owner_card_uid.as_ref(),
                    group_id: None,
                    deployment_environment: event.deployment_environment.as_deref(),
                    binding_id: None,
                    trigger: None,
                    window: None,
                };
                let row = ResultPayloadBuilder::new(
                    run,
                    &event.verifier_card_uid,
                    event.occurred_at,
                    event.occurred_at,
                )
                .event(event)?;
                Ok((row.row, event.attribution))
            })
            .collect()
    }
}
