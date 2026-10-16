//! The batched queue writer of acknowledged Verifier submissions.
//!
//! A submitted execution's inputs row is staged on the Scribe outbox with its
//! [`Submission`]; once Scribe acknowledges the row, the Scribe sink stages
//! the submission here without waiting. The generic [`Outbox`] owns the
//! unbounded queue, per-tenant batching, retry with backoff, and shutdown;
//! [`SubmissionSink`] is its one durable write: one tenant transaction that
//! accepts the batch through [`VerifierExecutionQueue::accept_submissions`].
//! Replay, idempotency conflicts, and refusals are decided there, so a
//! repeated batch accepts nothing twice. The writer grants no authority: each
//! execution runs under the authority accepted with its submission. Losses
//! are counted in `outbox_events_lost_total{outbox="verifier_submissions"}`.

use std::sync::Arc;

use wyrd_runtime::outbox::{Outbox, OutboxSink};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_executions::{
    Submission, SubmissionOutcome, VerifierExecutionQueue,
};
use wyrd_sql::{SqlError, WyrdPostgres};

/// Tenants the queue writer accepts for at once, which bounds the Wyrd pool
/// connections it may hold.
const SUBMISSION_WRITER_CONNECTIONS: usize = 4;

/// The process outbox of acknowledged submissions: the generic outbox over
/// [`SubmissionSink`].
pub type SubmissionOutbox = Outbox<SubmissionSink>;

/// Queue destination of the outbox: one tenant's submissions, one
/// transaction.
pub struct SubmissionSink {
    /// Wyrd Postgres owner each tenant write opens its transaction through.
    postgres: WyrdPostgres,
    /// The durable execution queue the submissions are accepted into.
    queue: VerifierExecutionQueue,
}

impl SubmissionSink {
    /// Start the process submission outbox over `postgres`.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn outbox(postgres: WyrdPostgres) -> Arc<SubmissionOutbox> {
        Outbox::new(
            Self {
                postgres,
                queue: VerifierExecutionQueue::default(),
            },
            SUBMISSION_WRITER_CONNECTIONS,
        )
    }
}

impl OutboxSink for SubmissionSink {
    /// One accepted caller submission, staged once Scribe acknowledged its
    /// pinned inputs and written into its tenant's execution queue.
    type Item = Submission;
    /// The Postgres failure of one tenant batch; the outbox retries the
    /// batch, and replay makes the retry idempotent.
    type Error = SqlError;
    /// The outbox name in logs, metrics, and `Debug` output.
    const NAME: &'static str = "verifier_submissions";

    /// Accept `submissions` into the tenant's queue in one transaction and
    /// commit it.
    ///
    /// A refused or conflicting submission is logged; it settled `errored`
    /// in the same transaction, so its status says why.
    ///
    /// # Errors
    /// Returns the connection, write, or commit failure; the outbox retries
    /// the batch, which replays every submission that did commit.
    ///
    /// # Cancellation
    /// The outbox's shutdown deadline may drop this future mid-write.
    /// Cancelled before `commit` is sent, the transaction rolls back and no
    /// execution exists; cancelled while `commit` resolves, the executions
    /// may already be durable. Either way the outbox counts the batch lost.
    #[tracing::instrument(name = "verification.submissions", skip_all, fields(%tenant))]
    async fn write(
        &self,
        tenant: DataTenantId,
        submissions: &[Submission],
    ) -> Result<(), SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let outcomes = self
            .queue
            .accept_submissions(&mut conn, submissions)
            .await
            .map_err(SqlError::Query)?;
        conn.commit().await?;
        for (submission, outcome) in submissions.iter().zip(outcomes) {
            match outcome {
                SubmissionOutcome::Accepted | SubmissionOutcome::Replayed(_) => {}
                SubmissionOutcome::Conflict { original } => tracing::info!(
                    %tenant,
                    execution_id = %submission.execution_id,
                    %original,
                    "submission conflicts with an earlier request under its idempotency key"
                ),
                SubmissionOutcome::Refused(refusal) => tracing::info!(
                    %tenant,
                    execution_id = %submission.execution_id,
                    code = %refusal.error().code,
                    "submission refused"
                ),
            }
        }
        Ok(())
    }
}
