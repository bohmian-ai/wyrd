//! The batched queue writer of acknowledged Verifier submissions.
//!
//! A submitted execution's inputs row is staged on the Scribe outbox with its
//! [`Submission`]; once Scribe acknowledges the row, the Scribe sink stages
//! the submission here without waiting. The generic [`Outbox`] owns the
//! bounded queue, per-lane batching, retry with backoff, and shutdown;
//! [`SubmissionSink`] is its one durable write. Each tenant's submissions are
//! spread over [`SubmissionSink::LANES`] lanes that commit concurrently, and
//! every submission under one idempotency key shares a lane, so its retries
//! and conflicts are decided in arrival order. Each write is one tenant
//! transaction that resolves each distinct target through
//! [`VerifierCardCache::resolve`], the one read route every Verifier
//! execution shares, and accepts the batch through
//! [`VerifierExecutionQueue::accept_submissions`].
//! Replay, idempotency conflicts, and refusals are decided there, so a
//! repeated batch accepts nothing twice. The writer grants no authority: each
//! execution runs under the authority accepted with its submission. Losses
//! are counted in `outbox_events_lost_total{outbox="verifier_submissions"}`.
//! A commit that queued a new execution notifies [`SubmissionSink::accepted`],
//! which wakes this process's idle Verifier runner instead of leaving the
//! execution to its poll interval.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use tokio::sync::Notify;

use wyrd_runtime::outbox::{Outbox, OutboxLimits, OutboxSink};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_executions::{
    Submission, SubmissionOutcome, VerifierExecutionQueue,
};
use wyrd_sql::{SqlError, WyrdPostgres};

use super::cache::{Db, VerifierCardCache};

/// Wyrd pool connections the queue writer holds at once across every
/// tenant, and the lanes of each tenant; one busy tenant may use all of them.
const SUBMISSION_WRITER_CONNECTIONS: usize = 4;

/// Most acknowledged submissions the process holds unqueued at once. A
/// submission reserves its slot here before its inputs are staged, so the
/// Scribe acknowledgement never hands off into a full outbox.
const SUBMISSION_CAPACITY: usize = 16_384;

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
    /// The process's verification target resolution, shared with direct
    /// verify and the runner through [`Self::verifier_cards`].
    verifier_cards: Arc<VerifierCardCache>,
    /// Notified once per commit that queued at least one new execution; it
    /// stores a single notification while no claim loop waits.
    accepted: Arc<Notify>,
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
                verifier_cards: Arc::default(),
                accepted: Arc::new(Notify::new()),
            },
            SUBMISSION_WRITER_CONNECTIONS,
            OutboxLimits {
                items: SUBMISSION_CAPACITY,
                // Each submission is bounded by the verification input
                // limits, so the item limit bounds this outbox's memory.
                bytes: usize::MAX,
            },
        )
    }

    /// The signal notified after each commit that queued a new execution,
    /// which the Verifier runner's claim loop waits on while idle.
    #[must_use]
    pub fn accepted(&self) -> Arc<Notify> {
        Arc::clone(&self.accepted)
    }

    /// The target resolution this writer refuses submissions through, which
    /// the process shares as `AppState::verifier_cards`.
    #[must_use]
    pub fn verifier_cards(&self) -> Arc<VerifierCardCache> {
        Arc::clone(&self.verifier_cards)
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

    /// Wyrd pool connections the writer holds per tenant at once.
    const LANES: usize = SUBMISSION_WRITER_CONNECTIONS;

    /// A keyed submission's lane is the hash of its (subject, actor, key), so
    /// every request under one key is decided in arrival order; an unkeyed
    /// one has no order to keep and hashes its execution ID.
    fn lane(submission: &Submission) -> u64 {
        let mut hasher = DefaultHasher::new();
        match &submission.key {
            Some(key) => {
                let attribution = submission.authority.attribution();
                (
                    attribution.sub_principal_id,
                    attribution.act_principal_id,
                    &key.key,
                )
                    .hash(&mut hasher);
            }
            None => submission.execution_id.hash(&mut hasher),
        }
        hasher.finish()
    }

    /// Accept `submissions` into the tenant's queue in one transaction and
    /// commit it.
    ///
    /// A refused or conflicting submission is logged; it settled `errored`
    /// in the same transaction, so its status says why. When any submission
    /// was newly accepted, [`SubmissionSink::accepted`] is notified after the
    /// commit.
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
        let mut refusals = HashMap::new();
        for submission in submissions {
            let request = &submission.request;
            let pair = (
                request.verifier_card_uid.as_uuid(),
                request.card_uid.as_uuid(),
            );
            if refusals.contains_key(&pair) {
                continue;
            }
            if let Err(refusal) = self
                .verifier_cards
                .resolve(
                    Db::Conn(conn.transaction()),
                    tenant,
                    &request.verifier_card_uid,
                    &request.card_uid,
                )
                .await?
            {
                refusals.insert(pair, refusal);
            }
        }
        let outcomes = self
            .queue
            .accept_submissions(&mut conn, submissions, &refusals)
            .await
            .map_err(SqlError::Query)?;
        conn.commit().await?;
        if outcomes
            .iter()
            .any(|outcome| matches!(outcome, SubmissionOutcome::Accepted))
        {
            self.accepted.notify_one();
        }
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

#[cfg(test)]
mod tests {
    //! How submissions are spread over the writer's lanes.

    use std::collections::{BTreeMap, HashSet};

    use uuid::Uuid;
    use wyrd_runtime::AcceptedAuthority;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::verification::execution::VerificationRequest;

    use super::*;

    /// `count` submissions of one caller, keyed `key-{n % keys}` when `keys`
    /// is set and unkeyed otherwise.
    ///
    /// # Panics
    /// Panics if a fresh `UUIDv7` is rejected as a Card UID or a request
    /// fails to canonicalize.
    fn submissions(count: usize, keys: Option<usize>) -> Vec<Submission> {
        let tenant = DataTenantId::new_v7();
        let caller = Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::Service,
            tenant,
            Vec::new(),
            PermissionSet::default(),
        );
        (0..count)
            .map(|position| {
                let request = VerificationRequest {
                    verifier_card_uid: CardUid::from_uuid(Uuid::now_v7()).expect("v7 Card UID"),
                    card_uid: CardUid::from_uuid(Uuid::now_v7()).expect("v7 Card UID"),
                    run_id: None,
                    group_id: None,
                    inputs: BTreeMap::new(),
                    execution_route_overrides: BTreeMap::new(),
                    deployment_environment: None,
                };
                let key = keys.map(|keys| format!("key-{}", position % keys));
                Submission::new(
                    tenant,
                    AcceptedAuthority::new(&caller, &[]),
                    request,
                    None,
                    key,
                )
                .expect("request canonicalizes")
            })
            .collect()
    }

    /// Every submission under one (subject, actor, key) shares a lane, so
    /// its retries and conflicts are decided in arrival order, while distinct
    /// keys and unkeyed submissions spread over every lane.
    ///
    /// # Panics
    /// Panics when one key's submissions are split across lanes or either
    /// population misses a lane.
    #[test]
    fn one_keys_submissions_share_a_lane() {
        let lanes = SubmissionSink::LANES as u64;
        let mut homes: HashMap<String, u64> = HashMap::new();
        let mut keyed_lanes = HashSet::new();
        for submission in submissions(1_024, Some(63)) {
            let lane = SubmissionSink::lane(&submission) % lanes;
            let key = submission.key.as_ref().expect("keyed").key.clone();
            assert_eq!(*homes.entry(key).or_insert(lane), lane, "a key was split");
            keyed_lanes.insert(lane);
        }
        let unkeyed_lanes: HashSet<u64> = submissions(1_024, None)
            .iter()
            .map(|submission| SubmissionSink::lane(submission) % lanes)
            .collect();
        assert_eq!(keyed_lanes.len(), SubmissionSink::LANES);
        assert_eq!(unkeyed_lanes.len(), SubmissionSink::LANES);
    }
}
