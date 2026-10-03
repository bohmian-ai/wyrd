//! Post-acknowledgement Eval run requests, staged on the generic outbox.
//!
//! Gate hands every Eval observation frame Scribe has durably acknowledged to
//! [`ObservationEnqueue`], which derives each row's exact `record_id`,
//! subject, and committed `wyrd_event_time` and stages one run request per
//! record on the [`ObservationRunOutbox`] without waiting. Gate hands over
//! only the acknowledgement that first committed a batch, so a suppressed
//! replay never queues and its later receipt instant can never be frozen as a
//! run's event time.
//!
//! The generic [`Outbox`] owns the unbounded queue, per-tenant batching,
//! retry with backoff, and shutdown; [`ObservationRunSink`] is its one durable
//! call, [`VerifierRunQueue::enqueue_observation_batch`], a multi-row insert
//! keyed by tenant, binding, and record, so a repeated write inserts nothing.
//! Losses are counted in `outbox_events_lost_total{outbox="eval_run_requests"}`.

use std::sync::Arc;

use bytes::Bytes;
use vala_bifrost_redux::gate::{AuthContext, ObservationAck};
use vala_bifrost_redux::tables::EvalObservationsTable;
use wyrd_runtime::outbox::{Outbox, OutboxSink};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_runs::{ObservationRecord, VerifierRunQueue};
use wyrd_sql::{SqlError, WyrdPostgres};

/// Tenants the run-request writer inserts for at once, which bounds the Wyrd
/// pool connections it may hold.
const OBSERVATION_RUN_WRITER_CONNECTIONS: usize = 4;

/// The process outbox of Eval run requests: the generic outbox over
/// [`ObservationRunSink`].
pub type ObservationRunOutbox = Outbox<ObservationRunSink>;

/// Run-request destination of the outbox: one tenant's records, one
/// transaction, one multi-row insert into `verifier_runs`.
pub struct ObservationRunSink {
    /// Wyrd Postgres owner each tenant write opens its transaction through.
    postgres: WyrdPostgres,
    /// The shared durable run queue.
    runs: VerifierRunQueue,
}

impl ObservationRunSink {
    /// Starts the process run-request outbox over `postgres`.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn outbox(postgres: WyrdPostgres) -> Arc<ObservationRunOutbox> {
        Outbox::new(
            Self {
                postgres,
                runs: VerifierRunQueue::default(),
            },
            OBSERVATION_RUN_WRITER_CONNECTIONS,
        )
    }
}

impl OutboxSink for ObservationRunSink {
    type Item = ObservationRecord;
    type Error = SqlError;
    const NAME: &'static str = "eval_run_requests";

    /// Inserts one run per active `observations_ready` binding of each
    /// record's subject in the tenant's transaction and commits it.
    ///
    /// Repeating the write is harmless: the insert is keyed by tenant,
    /// binding, and record with `ON CONFLICT DO NOTHING`.
    ///
    /// # Errors
    /// Returns the connection, insert, or commit failure; the outbox retries
    /// the batch.
    async fn write(
        &self,
        tenant: DataTenantId,
        records: &[ObservationRecord],
    ) -> Result<(), SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        self.runs
            .enqueue_observation_batch(&mut conn, records)
            .await
            .map_err(SqlError::Query)?;
        conn.commit().await
    }
}

/// Gate's observation acknowledgement hook: decodes each acknowledged Eval
/// frame and stages its run requests on the shared [`ObservationRunOutbox`].
pub struct ObservationEnqueue {
    /// Outbox the run requests are staged on.
    runs: Arc<ObservationRunOutbox>,
}

impl ObservationEnqueue {
    /// Builds the hook over the process run-request outbox.
    #[must_use]
    pub const fn new(runs: Arc<ObservationRunOutbox>) -> Self {
        Self { runs }
    }
}

impl ObservationAck for ObservationEnqueue {
    /// Decodes the acknowledged frame's records and stages one run request
    /// per record without waiting.
    ///
    /// A frame that does not decode is logged and counted as lost; the
    /// acknowledged observation is never affected.
    fn acknowledged(&self, auth: &AuthContext, frame: Bytes, receipt_micros: i64) {
        match EvalObservationsTable::acknowledged(&frame, &auth.principal, receipt_micros) {
            Ok(keys) => {
                for key in keys {
                    self.runs.stage(
                        auth.tenant,
                        ObservationRecord {
                            subject: key.card_uid,
                            record_id: key.record_id,
                            event_time: key.event_time,
                        },
                    );
                }
            }
            Err(error) => {
                tracing::error!(
                    tenant = %auth.tenant,
                    request_id = auth.request_id.as_str(),
                    %error,
                    "acknowledged Eval observation frame did not decode into run requests"
                );
                metrics::counter!("outbox_events_lost_total", "outbox" => ObservationRunSink::NAME)
                    .increment(1);
            }
        }
    }
}
