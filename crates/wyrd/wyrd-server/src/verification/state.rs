//! The recoverable state of one running Verifier execution.
//!
//! [`RunState`] is the one owner of an execution's [`ExecutionState`] while
//! it runs, for direct and queued executions alike. Its claim installs the
//! runner as the state's only writer and answers the accepted revision, and
//! recovery reads exactly that revision from
//! `bifrost.verification.execution_events`: revision 0 is authoritative
//! absence, and a revision not visible yet is [`StateError::Unavailable`],
//! never older or empty state. Every change is one complete revision staged
//! on the process Scribe outbox and applied in memory only once Scribe
//! accepted it from this writer at exactly the next revision, so a writer
//! shut out by a takeover advances nothing and a refused revision never
//! becomes accepted. Changes are serialized per execution; a change whose
//! acceptance is unknown breaks the state, and the attempt recovers again
//! under a new claim.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio::sync::oneshot;
use vala_bifrost_redux::contracts::ScribeStateAdvance;
use vala_bifrost_redux::contracts::scribe_state_heads::{self, ScribeStateClaim};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, ExecutionId, VerificationResultId};
use wyrd_spec::storage::StoredObjectRef;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    InvocationSettlement, Producer, Publication, PublicationKind, VerificationOutcome,
};
use wyrd_spec::verification::state::{
    AcceptedPublication, ExecutionState, ExecutionStateError, InvocationRecord, TerminalSelection,
};

use super::inputs::InputResolver;
use super::leases::HeldLease;
use super::results::{ResultPayloadBuilder, ResultRun};
use crate::scribe_outbox::{ScribeOutbox, ScribeWrite};
use crate::state::AppState;

/// The state-fenced table every revision is a row of, and the state head's
/// logical table.
const STATE_TABLE: &str = "bifrost.verification.execution_events";

/// Why an execution's state cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The accepted revision cannot be read or written now: the head or the
    /// revision's row is not reachable or not visible yet, or Scribe has no
    /// capacity. Nothing was assumed absent; recovery is retried.
    #[error("the execution state is unavailable: {0}")]
    Unavailable(String),
    /// A newer claim owns the state; this writer must stop.
    #[error("a newer claim owns the execution state")]
    Superseded,
    /// The accepted revision is in a format this worker does not run.
    #[error(transparent)]
    Incompatible(#[from] ExecutionStateError),
    /// A change was refused or its acceptance is unknown; this writer
    /// changes nothing more.
    #[error("the execution state no longer accepts this writer's changes")]
    Stale,
}

/// The execution identities every revision's row records.
#[derive(Debug, Clone)]
pub struct StateRow {
    /// The execution.
    pub execution_id: ExecutionId,
    /// The Verifier running it.
    pub verifier_uid: CardUid,
    /// The Card verified.
    pub card_uid: CardUid,
    /// The owner Service, when known.
    pub owner_card_uid: Option<CardUid>,
    /// The application Run judged, when there is one.
    pub run_id: Option<RunId>,
    /// The accepted deployment environment tag.
    pub deployment_environment: Option<String>,
    /// The execution's subject and actor, which Scribe stamps on each row.
    pub attribution: Attribution,
}

/// How one claim writes the state.
#[derive(Debug, Clone, Copy)]
pub struct StateWriter {
    /// This claim's writer identity.
    pub writer: uuid::Uuid,
    /// The claim's generation; a lower one never displaces a higher one.
    pub generation: i64,
    /// The instant after which Scribe accepts no revision of this writer,
    /// on the PostgreSQL clock; `None` sets no bound.
    pub not_after: Option<DateTime<Utc>>,
    /// How long this process waits for any one revision's acceptance.
    pub deadline: tokio::time::Instant,
}

/// The call that asks to accept one publication.
#[derive(Debug, Clone, Copy)]
pub struct PublicationCall<'a> {
    /// The publication to accept.
    pub publication: &'a Publication,
    /// The accepting call's identity, stable across its replays.
    pub call_id: &'a str,
    /// Digest of the call's canonical arguments.
    pub arguments: &'a Sha256Hex,
    /// The completed File descriptor of each manifest file, in manifest
    /// order; empty for a JSON publication.
    pub objects: &'a [StoredObjectRef],
    /// The bytes the publication retains: a JSON value's serialized length,
    /// or the sum of its manifest's file sizes.
    pub retained_bytes: u64,
}

/// The effective cumulative ceilings of one execution's publications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationBudget {
    /// Most publications the execution may accept across all attempts.
    pub max_count: u32,
    /// Most bytes its accepted publications may retain together.
    pub max_bytes: u64,
}

/// The cumulative ceiling a refused publication would cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationLimit {
    /// [`PublicationBudget::max_count`].
    Count,
    /// [`PublicationBudget::max_bytes`].
    Bytes,
}

/// Outcome of accepting one publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationAcceptance {
    /// The publication is part of the accepted state.
    Accepted,
    /// The same call with the same arguments already published it; the
    /// accepted kind is its receipt.
    Replayed(PublicationKind),
    /// Another call, or the same call with other arguments, holds the name
    /// in this attempt.
    Conflict,
    /// Accepting it would cross a cumulative ceiling; nothing changed.
    OverBudget(PublicationLimit),
    /// The writer no longer holds the state, its lease, or its deadline;
    /// nothing changed.
    StaleLease,
}

/// The state as this writer last saw Scribe accept it.
struct Held {
    /// The accepted state.
    state: ExecutionState,
    /// Its accepted revision; 0 before any.
    revision: i64,
    /// Set once a change was refused or its acceptance is unknown.
    broken: bool,
}

/// The recoverable state of one running execution and its only writer.
pub struct RunState {
    /// The outbox every revision is staged on.
    outbox: Arc<ScribeOutbox>,
    /// The execution's tenant.
    tenant: DataTenantId,
    /// The identities every revision's row records.
    row: StateRow,
    /// This claim's writer.
    writer: StateWriter,
    /// The queued execution's lease, confirmed before each revision so a
    /// cancelled, expired, or reclaimed run stops changing its state; `None`
    /// for a direct execution.
    lease: Option<Arc<HeldLease>>,
    /// The accepted state, locked across each change's acceptance.
    held: tokio::sync::Mutex<Held>,
}

impl RunState {
    /// Claim `tenant`'s execution state for `writer` and recover its accepted
    /// revision through `records`, whose read authority must cover
    /// `bifrost.verification.execution_events`. Without `records` the
    /// identity must be new: any accepted revision is refused rather than
    /// read.
    ///
    /// # Errors
    /// Returns [`StateError::Superseded`] when a newer claim owns the state,
    /// [`StateError::Unavailable`] when the head or the accepted revision
    /// cannot be read, and [`StateError::Incompatible`] for a revision in
    /// another format.
    pub async fn claim(
        app: &AppState,
        outbox: Arc<ScribeOutbox>,
        tenant: DataTenantId,
        row: StateRow,
        writer: StateWriter,
        lease: Option<Arc<HeldLease>>,
        records: Option<&InputResolver<'_>>,
    ) -> Result<Self, StateError> {
        let unavailable =
            |error: &dyn std::fmt::Display| StateError::Unavailable(error.to_string());
        let mut conn = app
            .postgres
            .vala()
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let claimed = scribe_state_heads::claim(
            &mut conn,
            STATE_TABLE,
            row.execution_id.as_uuid(),
            writer.writer,
            writer.generation,
        )
        .await
        .map_err(|error| unavailable(&error))?;
        let ScribeStateClaim::Claimed(head) = claimed else {
            return Err(StateError::Superseded);
        };
        conn.commit().await.map_err(|error| unavailable(&error))?;
        let state = match records {
            Some(records) => Self::read(records, row.execution_id, head.revision).await?,
            None if head.revision == 0 => ExecutionState::fresh(),
            None => {
                return Err(StateError::Unavailable(
                    "a new execution identity already has accepted state".to_owned(),
                ));
            }
        };
        Ok(Self {
            outbox,
            tenant,
            row,
            writer,
            lease,
            held: tokio::sync::Mutex::new(Held {
                state,
                revision: head.revision,
                broken: false,
            }),
        })
    }

    /// Claim the state of an execution identity no writer has advanced, such
    /// as a direct execution's, for a new writer with no lease and no
    /// database-clock bound, waiting for each revision's acceptance until
    /// `deadline`.
    ///
    /// A direct execution's identity is new to its request and never
    /// resumed, so its state starts fresh and nothing is read; any accepted
    /// revision is refused.
    ///
    /// # Errors
    /// As [`Self::claim`] without `records`.
    pub async fn fresh(
        app: &AppState,
        outbox: Arc<ScribeOutbox>,
        tenant: DataTenantId,
        row: StateRow,
        deadline: tokio::time::Instant,
    ) -> Result<Self, StateError> {
        let writer = StateWriter {
            writer: uuid::Uuid::now_v7(),
            generation: 0,
            not_after: None,
            deadline,
        };
        Self::claim(app, outbox, tenant, row, writer, None, None).await
    }

    /// The instant this writer's attempt ends; no revision is awaited past it.
    #[must_use]
    pub const fn deadline(&self) -> tokio::time::Instant {
        self.writer.deadline
    }

    /// The accepted revision and state of `execution` without claiming it,
    /// so inspecting a running execution never shuts out its writer; `None`
    /// when no writer ever claimed it.
    ///
    /// # Errors
    /// As [`Self::claim`], without [`StateError::Superseded`].
    pub async fn accepted(
        app: &AppState,
        tenant: DataTenantId,
        execution: ExecutionId,
        records: &InputResolver<'_>,
    ) -> Result<Option<(i64, ExecutionState)>, StateError> {
        let unavailable =
            |error: &dyn std::fmt::Display| StateError::Unavailable(error.to_string());
        let mut conn = app
            .postgres
            .vala()
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let head = scribe_state_heads::head(&mut conn, STATE_TABLE, execution.as_uuid())
            .await
            .map_err(|error| unavailable(&error))?;
        drop(conn);
        let Some(head) = head else {
            return Ok(None);
        };
        Ok(Some((
            head.revision,
            Self::read(records, execution, head.revision).await?,
        )))
    }

    /// The state at exactly `revision`: fresh at 0, otherwise its one row.
    ///
    /// # Errors
    /// As [`Self::claim`].
    async fn read(
        records: &InputResolver<'_>,
        execution: ExecutionId,
        revision: i64,
    ) -> Result<ExecutionState, StateError> {
        if revision == 0 {
            return Ok(ExecutionState::fresh());
        }
        let value = records
            .state(execution, revision)
            .await
            .map_err(|error| StateError::Unavailable(error.to_string()))?
            .ok_or_else(|| {
                StateError::Unavailable(format!("revision {revision} is not visible yet"))
            })?;
        Ok(ExecutionState::decode(value)?)
    }

    /// A copy of the accepted state.
    pub async fn snapshot(&self) -> ExecutionState {
        self.held.lock().await.state.clone()
    }

    /// The terminal outcome the state selected, selecting `outcome` with a
    /// new result identity first when it has none; once selected, every
    /// later entry delivers the same result.
    ///
    /// # Errors
    /// Returns the [`Self::change`] errors.
    pub async fn select(
        &self,
        outcome: &VerificationOutcome,
        started_at: DateTime<Utc>,
    ) -> Result<TerminalSelection, StateError> {
        let mut held = self.held.lock().await;
        if let Some(selected) = &held.state.terminal {
            return Ok(selected.clone());
        }
        let mut next = held.state.clone();
        let selected = TerminalSelection {
            result_id: VerificationResultId::new_v7(),
            started_at,
            ended_at: Utc::now(),
            outcome: outcome.clone(),
        };
        next.terminal = Some(selected.clone());
        self.change(&mut held, next).await?;
        Ok(selected)
    }

    /// The receipt `call` already holds, when it holds one, without
    /// changing the state.
    pub async fn publication_receipt(
        &self,
        call: PublicationCall<'_>,
    ) -> Option<PublicationAcceptance> {
        let held = self.held.lock().await;
        if held.broken {
            return Some(PublicationAcceptance::StaleLease);
        }
        receipt(&held.state, call)
    }

    /// Accept `call`'s publication, once, within `budget`: the same call
    /// with the same arguments replays, another holder of the name in the
    /// attempt conflicts, and a publication over a cumulative ceiling is
    /// refused. A new publication is accepted only once its revision is.
    pub async fn accept_publication(
        &self,
        call: PublicationCall<'_>,
        budget: PublicationBudget,
    ) -> PublicationAcceptance {
        let mut held = self.held.lock().await;
        if held.broken {
            return PublicationAcceptance::StaleLease;
        }
        if let Some(held) = receipt(&held.state, call) {
            return held;
        }
        let publications = &held.state.publications;
        if publications.len() >= budget.max_count as usize {
            return PublicationAcceptance::OverBudget(PublicationLimit::Count);
        }
        let retained = publications
            .iter()
            .map(|accepted| accepted.retained_bytes)
            .sum::<u64>();
        if retained.saturating_add(call.retained_bytes) > budget.max_bytes {
            return PublicationAcceptance::OverBudget(PublicationLimit::Bytes);
        }
        let mut next = held.state.clone();
        next.sequence += 1;
        next.publications.push(AcceptedPublication {
            sequence: next.sequence,
            publication: call.publication.clone(),
            call_id: call.call_id.to_owned(),
            arguments: *call.arguments,
            objects: call.objects.to_vec(),
            retained_bytes: call.retained_bytes,
        });
        match self.change(&mut held, next).await {
            Ok(()) => PublicationAcceptance::Accepted,
            Err(error) => {
                tracing::warn!(execution_id = %self.row.execution_id, %error, "a publication was not accepted");
                PublicationAcceptance::StaleLease
            }
        }
    }

    /// Every publication `step` accepted, grouped by item, iteration, and
    /// attempt, in acceptance order within an attempt.
    pub async fn publications(&self, step: &TaskId) -> Vec<AcceptedPublication> {
        let mut found: Vec<_> = self
            .held
            .lock()
            .await
            .state
            .publications
            .iter()
            .filter(|accepted| accepted.publication.producer.step_id == *step)
            .cloned()
            .collect();
        found.sort_by_key(|accepted| {
            let producer = &accepted.publication.producer;
            (
                producer.item_index,
                producer.iteration_index,
                producer.attempt,
                accepted.sequence,
            )
        });
        found
    }

    /// Change the state to `next` through one revision Scribe accepts from
    /// this writer at exactly the next revision, then hold it.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] when the state is broken, the lease no
    /// longer holds, or the revision is refused or unacknowledged by the
    /// writer's deadline, which breaks the state; and
    /// [`StateError::Unavailable`] when the revision cannot be encoded or no
    /// outbox capacity frees by the deadline, which changes nothing.
    async fn change(&self, held: &mut Held, next: ExecutionState) -> Result<(), StateError> {
        if held.broken {
            return Err(StateError::Stale);
        }
        if let Some(lease) = &self.lease
            && !matches!(lease.confirm().await, Ok(true))
        {
            held.broken = true;
            return Err(StateError::Stale);
        }
        let revision = held.revision + 1;
        let value = serde_json::to_value(&next)
            .map_err(|error| StateError::Unavailable(error.to_string()))?;
        let row = &self.row;
        let row = ResultPayloadBuilder::new(
            ResultRun {
                execution_id: row.execution_id,
                run_id: row.run_id.as_ref(),
                verifier_version: "",
                card_uid: &row.card_uid,
                owner_card_uid: row.owner_card_uid.as_ref(),
                group_id: None,
                deployment_environment: row.deployment_environment.as_deref(),
                binding_id: None,
                trigger: None,
                window: None,
            },
            &row.verifier_uid,
            Utc::now(),
            Utc::now(),
        )
        .checkpoint(revision, next.terminal.is_some(), &value)
        .map_err(|error| StateError::Unavailable(error.to_string()))?;
        let slot = self
            .outbox
            .reserve_until(1, self.writer.deadline.into_std())
            .await
            .map_err(|full| StateError::Unavailable(full.to_string()))?;
        let (sender, acked) = oneshot::channel();
        slot.stage(
            self.tenant,
            ScribeWrite::ExecutionState {
                batch: uuid::Uuid::now_v7(),
                row: row.row,
                attribution: self.row.attribution,
                state: ScribeStateAdvance {
                    key: self.row.execution_id.as_uuid(),
                    writer: self.writer.writer,
                    revision,
                    not_after: self.writer.not_after,
                },
                acked: Mutex::new(Some(sender)),
            },
        );
        if !matches!(
            tokio::time::timeout_at(self.writer.deadline, acked).await,
            Ok(Ok(()))
        ) {
            held.broken = true;
            return Err(StateError::Stale);
        }
        held.state = next;
        held.revision = revision;
        Ok(())
    }
}

/// The receipt `call` holds in `state`: replayed with the accepted kind for
/// the same call and arguments, a conflict for any other holder of the name
/// in the producer attempt, or `None` while the name is free.
fn receipt(state: &ExecutionState, call: PublicationCall<'_>) -> Option<PublicationAcceptance> {
    let wanted = call.publication;
    state
        .publications
        .iter()
        .find(|accepted| {
            accepted.publication.producer == wanted.producer
                && accepted.publication.name == wanted.name
        })
        .map(|accepted| {
            if accepted.call_id == call.call_id && accepted.arguments == *call.arguments {
                PublicationAcceptance::Replayed(accepted.publication.content.kind())
            } else {
                PublicationAcceptance::Conflict
            }
        })
}

/// The producer of `task`'s `attempt`, which runs without fanout or repeat.
fn producer(task: &TaskId, attempt: u32) -> Producer {
    Producer {
        step_id: task.clone(),
        item_index: None,
        iteration_index: None,
        attempt,
    }
}

#[async_trait::async_trait]
impl vala_verify::blocks::Checkpoints for RunState {
    /// Record that `attempt` of `task` started.
    ///
    /// # Errors
    /// Returns a bounded reason when the revision is not accepted.
    async fn begin(&self, task: &TaskId, attempt: u32) -> Result<(), String> {
        let mut held = self.held.lock().await;
        let mut next = held.state.clone();
        next.sequence += 1;
        next.invocations.push(InvocationRecord {
            sequence: next.sequence,
            producer: producer(task, attempt),
            settlement: None,
        });
        self.change(&mut held, next)
            .await
            .map_err(|error| error.to_string())
    }

    /// Remove the unsettled `attempt` of `task`, refused capacity before it
    /// ran, so a later attempt replays it.
    ///
    /// # Errors
    /// Returns a bounded reason when the revision is not accepted.
    async fn forget(&self, task: &TaskId, attempt: u32) -> Result<(), String> {
        let mut held = self.held.lock().await;
        let mut next = held.state.clone();
        let wanted = producer(task, attempt);
        next.invocations
            .retain(|record| record.producer != wanted || record.settlement.is_some());
        if next.invocations.len() == held.state.invocations.len() {
            return Ok(());
        }
        self.change(&mut held, next)
            .await
            .map_err(|error| error.to_string())
    }

    /// Commit `settled` as the settlement of `attempt` of `task`, once.
    ///
    /// # Errors
    /// Returns a bounded reason when the attempt is not recorded unsettled or
    /// the revision is not accepted.
    async fn settle(
        &self,
        task: &TaskId,
        attempt: u32,
        settled: &InvocationSettlement,
    ) -> Result<(), String> {
        let mut held = self.held.lock().await;
        let mut next = held.state.clone();
        let wanted = producer(task, attempt);
        let record = next
            .invocations
            .iter_mut()
            .find(|record| record.producer == wanted && record.settlement.is_none())
            .ok_or_else(|| "the invocation attempt is not recorded unsettled".to_owned())?;
        record.settlement = Some(settled.clone());
        self.change(&mut held, next)
            .await
            .map_err(|error| error.to_string())
    }
}

/// A [`WyrdError`] for state an inspection read cannot use now.
impl From<StateError> for WyrdError {
    fn from(error: StateError) -> Self {
        tracing::warn!(%error, "the execution state cannot be read");
        Self::from(wyrd_spec::vala::BifrostError::QueryVisibilityUnavailable)
    }
}
