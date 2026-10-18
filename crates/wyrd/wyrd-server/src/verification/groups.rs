//! Turns ended groups of a `group_end` binding into group executions.
//!
//! [`GroupPoller`] serves one binding claimed by
//! [`VerifierExecutionQueue::claim_due_group`]. It discovers, in Bifrost, the
//! groups whose first source row is still within the binding's finality,
//! newest activity first. Per candidate it counts each source binding's
//! distinct executions, reads the row identities of the bindings within the
//! deployment ceiling, and decides in Rust whether the group ended (idle or
//! max_age) within its finality bound. A due group whose rows no queued group
//! execution covers gets one pinned snapshot: per source slot one ordered,
//! deduplicated list read from exactly those executions' inputs or
//! authoritative results, or, past the ceiling, no read at all. Every read
//! runs under the binding owner's accepted authority, executed by SYSTEM.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use arrow::array::{Array, RecordBatch, TimestampMicrosecondArray};
use arrow::util::display::array_value_to_string;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::tables::DomainTable;
use vala_bifrost_redux::tables::verification::{ExecutionInputsTable, ResultsTable};
use wyrd_spec::card::trigger::TriggerDuration;
use wyrd_spec::card::verifier::{SourceField, SourceText};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ExecutionId, GroupId, IdError, SlotName, VerificationResultId};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::verification::execution::InputDeclaration;
use wyrd_spec::verification::{GroupCoverage, GroupEndSettings, GroupSourceSnapshot};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::{
    DueGroupBinding, EnqueueOutcome, ExecutionAuthority, GroupSnapshot, VerifierExecutionQueue,
};

use super::drift::ObservationWindow;
use crate::components::auth::Caller;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::state::AppState;

/// Error code of a read of a Bifrost table the tenant never created.
const TABLE_NOT_FOUND: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

/// Candidate groups one poll of one binding examines, newest activity first.
// ponytail: a binding with more open in-finality groups than this examines
// only the newest-active page each poll; add a continuing discovery cursor if
// older open groups must be judged while newer ones stay active.
const GROUPS_PER_POLL: usize = 64;

/// Why one poll did not finish.
#[derive(Debug, thiserror::Error)]
pub enum GroupPollError {
    /// A Postgres statement failed; the caller's transaction is unusable.
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    /// A Bifrost read failed or returned malformed rows; the transaction is
    /// intact and the claimed cursor may still commit, so the next poll
    /// retries discovery.
    #[error("group discovery failed: {0}")]
    Read(String),
}

impl From<WyrdError> for GroupPollError {
    /// A failed Bifrost read.
    fn from(error: WyrdError) -> Self {
        Self::Read(error.to_string())
    }
}

/// Owner of the Bifrost reads that turn ended groups into group executions.
pub struct GroupPoller {
    /// Server state owning the Bifrost catalog and query service.
    state: AppState,
    /// The deployment source-execution ceiling of one source reference.
    ceiling: u32,
    /// Deadline of one Bifrost statement.
    query_timeout: Duration,
}

impl GroupPoller {
    /// Build a poller reading through `state`, bounding every source
    /// reference by `ceiling` distinct executions and every statement by
    /// `query_timeout`.
    #[must_use]
    pub const fn new(state: AppState, ceiling: u32, query_timeout: Duration) -> Self {
        Self {
            state,
            ceiling,
            query_timeout,
        }
    }

    /// Enqueue one group execution for every ended, uncovered group of the
    /// claimed binding `due`, under the owner's accepted `authority`.
    ///
    /// Every statement runs as `authority` executed by its SYSTEM actor, so
    /// ordinary query admission, table authorization and denial audit decide
    /// what the owner may read. Discovery reads at most [`GROUPS_PER_POLL`]
    /// candidates; each candidate is skipped when it is not yet due, has an
    /// outstanding group execution, is already covered by the latest one, or
    /// its selected payloads are not all visible yet (the next poll retries
    /// it). Rows of a submission the queue refused, such as an idempotency
    /// conflict, are not source rows and are dropped before the end is
    /// judged. The caller commits the returned enqueues together with the
    /// binding's advanced cursor.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Sql`] when a Postgres statement fails and
    /// [`GroupPollError::Read`] when the owner may not query, a Bifrost read
    /// fails or is refused, or a stored source declaration or row is
    /// malformed; enqueues of whole snapshots made before the failure stay in
    /// the caller's transaction.
    pub async fn poll(
        &self,
        conn: &mut TenantConn<'_>,
        queue: &VerifierExecutionQueue,
        due: &DueGroupBinding,
        authority: &ExecutionAuthority,
    ) -> Result<Vec<EnqueueOutcome>, GroupPollError> {
        let caller = Caller::accepted(
            authority.authority(),
            authority.attribution().act_principal_id,
        );
        let mut poll = BindingPoll {
            query: ScheduledQueryCaller::authenticated(
                self.state.clone(),
                caller,
                CancellationToken::new(),
            )?,
            deadline_ms: i64::try_from(self.query_timeout.as_millis()).unwrap_or(i64::MAX),
            sources: Sources::new(&due.settings, &due.owner_card_uid.to_string())?,
            ceiling: u64::from(self.ceiling),
            due,
            results: true,
        };
        let mut enqueued = Vec::new();
        for group in poll.candidates().await? {
            let counts = poll.counts(&group).await?;
            let over: BTreeMap<usize, TupleCount> = counts
                .into_iter()
                .filter(|(_, count)| count.executions > poll.ceiling)
                .collect();
            let mut rows = poll.identities(&group, &over).await?;
            let ids: Vec<ExecutionId> = rows.iter().map(|row| row.execution_id).collect();
            let refused = queue.refused_submissions(conn, &ids).await?;
            rows.retain(|row| !refused.contains(&row.execution_id));
            let Some(ended) = Ended::judge(&rows, over.values(), &due.settings, due.clock) else {
                continue;
            };
            let states = queue
                .group_states(conn, due.binding_id, std::slice::from_ref(&group))
                .await?;
            if states.get(&group).is_some_and(|state| {
                state.outstanding || state.covers(ended.through, ended.members)
            }) {
                continue;
            }
            let Some(snapshot) = poll
                .snapshot(conn, queue, group, &rows, &over, &ended)
                .await?
            else {
                continue;
            };
            enqueued.push(
                queue
                    .enqueue_group(conn, due.binding_id, &snapshot, authority)
                    .await?,
            );
        }
        Ok(enqueued)
    }
}

/// One claimed binding's poll: its owner-authorized reader, sources and
/// filters.
struct BindingPoll<'a> {
    /// The owner's accepted caller every statement runs through.
    query: ScheduledQueryCaller,
    /// Deadline of one statement, in milliseconds.
    deadline_ms: i64,
    /// The binding's parsed source slots.
    sources: Sources,
    /// The deployment source-execution ceiling of one source reference.
    ceiling: u64,
    /// The claimed binding.
    due: &'a DueGroupBinding,
    /// Whether the tenant's results table exists; until it does, sources
    /// have inputs only.
    results: bool,
}

impl BindingPoll<'_> {
    /// Groups whose first visible source row is within the binding's
    /// finality of the tick clock, newest activity first, at most
    /// [`GROUPS_PER_POLL`].
    ///
    /// Nothing past finality can open a group execution, so finality is the
    /// only discovery bound; the queue's coverage decides what is new. A
    /// finality reaching before the earliest representable instant bounds
    /// nothing. A tenant without a results table is discovered from inputs
    /// alone, and the poll remembers that for its later reads.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a read fails or a group ID is
    /// malformed.
    async fn candidates(&mut self) -> Result<Vec<GroupId>, GroupPollError> {
        let open = self
            .due
            .clock
            .checked_sub_signed(span(&self.due.settings.finality))
            .map(|since| format!(" HAVING MIN(at) >= {}", ObservationWindow::instant(since)))
            .unwrap_or_default();
        // ponytail: scans the binding's whole retained source history each
        // poll; prune by a time-partitioned semi-join if that outgrows a poll.
        let sql = |rows: String| {
            format!(
                "SELECT group_id, MAX(at) AS newest FROM ({rows}) s GROUP BY group_id{open} \
                 ORDER BY newest DESC, group_id LIMIT {GROUPS_PER_POLL}"
            )
        };
        let batches = match self.run(&sql(self.surfaces("true"))).await {
            Err(error) if error.code() == TABLE_NOT_FOUND => {
                self.results = false;
                self.query(&sql(self.surfaces("true"))).await?
            }
            batches => batches?,
        };
        let mut groups = Vec::new();
        for batch in batches {
            for row in 0..batch.num_rows() {
                let group = text(&batch, "group_id", row)?
                    .ok_or_else(|| GroupPollError::Read("null group_id".to_owned()))?;
                groups.push(GroupId::new(group).map_err(malformed)?);
            }
        }
        Ok(groups)
    }

    /// Per source tuple of `group`, its distinct executions and rows, read
    /// before any identity so a tuple over the ceiling is never listed.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when the read fails or a row is
    /// malformed.
    async fn counts(&self, group: &GroupId) -> Result<BTreeMap<usize, TupleCount>, GroupPollError> {
        let sql = format!(
            "SELECT card_uid, verifier_card_uid, COUNT(DISTINCT execution_id) AS executions, \
             COUNT(*) AS members, MIN(at) AS first, MAX(at) AS newest FROM \
             (SELECT DISTINCT surface, card_uid, verifier_card_uid, execution_id, at FROM ({}) s) d \
             GROUP BY card_uid, verifier_card_uid",
            self.surfaces(&self.group(group))
        );
        let mut counts = BTreeMap::new();
        for batch in self.query(&sql).await? {
            let (first, newest) = (times(&batch, "first")?, times(&batch, "newest")?);
            for row in 0..batch.num_rows() {
                let Some(tuple) = self.sources.tuple(&batch, row)? else {
                    continue;
                };
                counts.insert(
                    tuple,
                    TupleCount {
                        executions: number(&batch, "executions", row)?,
                        members: number(&batch, "members", row)?,
                        first: instant(first, row)?,
                        newest: instant(newest, row)?,
                    },
                );
            }
        }
        Ok(counts)
    }

    /// Every distinct visible source row identity of `group` outside the
    /// `over`-ceiling tuples, so at most the ceiling's executions per tuple.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when the read fails or a row is
    /// malformed.
    async fn identities(
        &self,
        group: &GroupId,
        over: &BTreeMap<usize, TupleCount>,
    ) -> Result<Vec<SourceRow>, GroupPollError> {
        let within: Vec<usize> = (0..self.sources.tuples.len())
            .filter(|tuple| !over.contains_key(tuple))
            .collect();
        if within.is_empty() {
            return Ok(Vec::new());
        }
        let filter = format!("{} AND ({})", self.group(group), self.sources.any(&within));
        let batches = self
            .query(&format!(
                "SELECT DISTINCT surface, card_uid, verifier_card_uid, execution_id, at FROM ({}) s",
                self.surfaces(&filter)
            ))
            .await?;
        self.sources.rows(&batches)
    }

    /// Pin the snapshot of one ended `group`: per source binding its
    /// eligible executions in list order, then per slot one aligned list.
    ///
    /// A source binding `over` the ceiling reads nothing: each of its slots
    /// pins an empty list marked with the ceiling. Otherwise the inputs and
    /// authoritative results of exactly its listed executions are read once
    /// and every slot takes its value per execution, null when absent.
    /// Returns `None`, deferring the group to the next poll, when a listed
    /// row's payload is not yet visible.
    ///
    /// # Errors
    /// Returns [`GroupPollError`] when a read fails or a row is malformed.
    async fn snapshot(
        &self,
        conn: &mut TenantConn<'_>,
        queue: &VerifierExecutionQueue,
        group: GroupId,
        rows: &[SourceRow],
        over: &BTreeMap<usize, TupleCount>,
        ended: &Ended,
    ) -> Result<Option<GroupSnapshot>, GroupPollError> {
        let lists: Vec<Option<Vec<ExecutionId>>> = (0..self.sources.tuples.len())
            .map(|tuple| (!over.contains_key(&tuple)).then(|| ended.executions(rows, tuple)))
            .collect();
        let readable: BTreeSet<ExecutionId> = lists.iter().flatten().flatten().copied().collect();
        let payloads = if readable.is_empty() {
            Payloads::default()
        } else {
            let ids: Vec<ExecutionId> = readable.iter().copied().collect();
            let selected = queue.selected_results(conn, &ids).await?;
            let payloads = self
                .payloads(&group, ended.cutoff, &readable, &selected)
                .await?;
            if !self.complete(&payloads, rows, ended, &selected) {
                tracing::warn!(binding_id = %self.due.binding_id, group_id = %group, "group payloads not yet visible; deferred");
                return Ok(None);
            }
            payloads
        };
        let ceiling = u32::try_from(self.ceiling).unwrap_or(u32::MAX);
        let mut inputs = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for slot in &self.sources.slots {
            let list = lists[slot.tuple].clone().unwrap_or_default();
            let values = list
                .iter()
                .map(|execution| payloads.value(*execution, &slot.field))
                .collect();
            inputs.insert(
                slot.name.clone(),
                InputDeclaration::Json {
                    value: Value::Array(values),
                },
            );
            sources.insert(
                slot.name.clone(),
                GroupSourceSnapshot {
                    reference: slot.reference.clone(),
                    execution_ids: list,
                    exceeded_ceiling: lists[slot.tuple].is_none().then_some(ceiling),
                },
            );
        }
        Ok(Some(GroupSnapshot {
            group_id: group,
            inputs,
            coverage: GroupCoverage {
                through: ended.through,
                members: ended.members,
                sources,
            },
        }))
    }

    /// Whether `payloads` holds every listed eligible row's value a slot
    /// reads: an input for each input row, and an outcome for each result
    /// row of a direct execution. A queued execution's outcome is its
    /// `selected` result's, which may genuinely be not yet published.
    fn complete(
        &self,
        payloads: &Payloads,
        rows: &[SourceRow],
        ended: &Ended,
        selected: &BTreeMap<ExecutionId, Option<VerificationResultId>>,
    ) -> bool {
        let (inputs, outcomes) = self.sources.reads();
        rows.iter().filter(|row| ended.eligible(row)).all(|row| {
            if row.result {
                !outcomes
                    || !self.results
                    || selected.contains_key(&row.execution_id)
                    || payloads.outcomes.contains_key(&row.execution_id)
            } else {
                !inputs || payloads.inputs.contains_key(&row.execution_id)
            }
        })
    }

    /// Read the inputs and results of exactly the `readable` executions of
    /// `group` timed before `cutoff`.
    ///
    /// Each execution takes its earliest visible input row. A queued
    /// execution takes only the result its settlement `selected`; a direct
    /// one its earliest visible result. Each table is read only when some
    /// slot needs it.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a read fails or a row is
    /// malformed.
    async fn payloads(
        &self,
        group: &GroupId,
        cutoff: Option<DateTime<Utc>>,
        readable: &BTreeSet<ExecutionId>,
        selected: &BTreeMap<ExecutionId, Option<VerificationResultId>>,
    ) -> Result<Payloads, GroupPollError> {
        let mut payloads = Payloads::default();
        let (inputs, outcomes) = self.sources.reads();
        let members = readable
            .iter()
            .map(|execution| ObservationWindow::text(&execution.to_string()))
            .collect::<Vec<_>>()
            .join(", ");
        let select = |columns: &str, table: &str, at: &str| {
            let before = cutoff
                .map(|cutoff| format!(" AND {at} < {}", ObservationWindow::instant(cutoff)))
                .unwrap_or_default();
            format!(
                "SELECT {columns}, {at} FROM vala.verification.{table} WHERE {} \
                 AND {} AND execution_id IN ({members}){before}",
                self.sources.eligible,
                self.group(group),
            )
        };
        if inputs {
            let sql = select(
                "execution_id, inputs",
                ExecutionInputsTable::NAME,
                "recorded_at",
            );
            let mut earliest = BTreeMap::new();
            for batch in self.query(&sql).await? {
                let times = times(&batch, "recorded_at")?;
                for row in 0..batch.num_rows() {
                    let execution = id(&batch, row)?;
                    let at = instant(times, row)?;
                    if earliest.get(&execution).is_none_or(|known| at < *known) {
                        earliest.insert(execution, at);
                        payloads
                            .inputs
                            .insert(execution, variant(&batch, "inputs", row)?);
                    }
                }
            }
        }
        if outcomes && self.results {
            let sql = select(
                "execution_id, result_id, outcome",
                ResultsTable::NAME,
                "ended_at",
            );
            let mut earliest = BTreeMap::new();
            for batch in self.query(&sql).await? {
                let times = times(&batch, "ended_at")?;
                for row in 0..batch.num_rows() {
                    let execution = id(&batch, row)?;
                    let at = instant(times, row)?;
                    let authoritative = match selected.get(&execution) {
                        Some(chosen) => {
                            chosen.map(|chosen| chosen.to_string())
                                == text(&batch, "result_id", row)?
                        }
                        None => earliest.get(&execution).is_none_or(|known| at < *known),
                    };
                    if authoritative {
                        earliest.insert(execution, at);
                        payloads
                            .outcomes
                            .insert(execution, variant(&batch, "outcome", row)?);
                    }
                }
            }
        }
        Ok(payloads)
    }

    /// The `group_id = <group>` predicate.
    fn group(&self, group: &GroupId) -> String {
        format!("group_id = {}", ObservationWindow::text(group.as_str()))
    }

    /// Both source surfaces as one `(surface, group_id, card_uid,
    /// verifier_card_uid, execution_id, at)` row set restricted by `filter`;
    /// inputs alone without a results table.
    fn surfaces(&self, filter: &str) -> String {
        let inputs = format!(
            "SELECT 'i' AS surface, group_id, card_uid, verifier_card_uid, execution_id, \
             recorded_at AS at FROM vala.verification.execution_inputs WHERE {} AND {filter}",
            self.sources.eligible,
        );
        if !self.results {
            return inputs;
        }
        format!(
            "{inputs} UNION ALL SELECT 'r' AS surface, group_id, card_uid, verifier_card_uid, \
             execution_id, ended_at AS at FROM vala.verification.results WHERE {} AND {filter}",
            self.sources.eligible,
        )
    }

    /// Run `sql` and collect its batches; a missing table reads as empty.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when the read fails.
    async fn query(&self, sql: &str) -> Result<Vec<RecordBatch>, GroupPollError> {
        match self.run(sql).await {
            Err(error) if error.code() == TABLE_NOT_FOUND => Ok(Vec::new()),
            batches => Ok(batches?),
        }
    }

    /// Run `sql` and collect its batches.
    ///
    /// # Errors
    /// Returns the query's [`WyrdError`].
    async fn run(&self, sql: &str) -> Result<Vec<RecordBatch>, WyrdError> {
        let mut batches = Vec::new();
        self.query
            .run_with(
                BifrostQueryRequest {
                    params: Vec::new(),
                    sql: sql.to_owned(),
                    deadline_ms: Some(self.deadline_ms),
                },
                |batch| {
                    batches.push(batch);
                    Ok(())
                },
            )
            .await?;
        Ok(batches)
    }
}

/// One source tuple's distinct visible executions and rows in one group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TupleCount {
    /// Distinct source executions.
    executions: u64,
    /// Distinct `(surface, execution, time)` rows.
    members: u64,
    /// Earliest row time.
    first: DateTime<Utc>,
    /// Newest row time.
    newest: DateTime<Utc>,
}

/// One binding's source slots and the distinct source tuples they read.
struct Sources {
    /// Every source slot of the group Verifier.
    slots: Vec<SourceSlot>,
    /// Distinct `(card_uid, verifier_card_uid)` source bindings, in slot
    /// order, as their columns store them.
    tuples: Vec<(String, String)>,
    /// The owner and source-tuple filter of every source read.
    eligible: String,
}

/// One source slot: what it reads, from which source tuple.
struct SourceSlot {
    /// The group Verifier slot it fills.
    name: SlotName,
    /// The authored declaration, pinned into the coverage.
    reference: SourceText,
    /// The value each source execution contributes.
    field: SourceField,
    /// Index into [`Sources::tuples`].
    tuple: usize,
}

impl Sources {
    /// Parse the pinned source declarations of `settings`.
    ///
    /// Slots of one source binding share one tuple, so their lists hold the
    /// same executions in the same order.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a pinned declaration no longer
    /// parses.
    fn new(settings: &GroupEndSettings, owner: &str) -> Result<Self, GroupPollError> {
        let mut tuples: Vec<(String, String)> = Vec::new();
        let mut slots = Vec::new();
        for (name, source) in &settings.sources {
            let field = source.reference.parse().map_err(malformed)?.field;
            let key = (
                source.card_uid.to_string(),
                source.verifier_card_uid.to_string(),
            );
            let tuple = tuples
                .iter()
                .position(|known| *known == key)
                .unwrap_or_else(|| {
                    tuples.push(key);
                    tuples.len() - 1
                });
            slots.push(SourceSlot {
                name: name.clone(),
                reference: source.reference.clone(),
                field,
                tuple,
            });
        }
        let mut sources = Self {
            slots,
            tuples,
            eligible: String::new(),
        };
        let all: Vec<usize> = (0..sources.tuples.len()).collect();
        sources.eligible = format!(
            "owner_card_uid = {} AND group_id IS NOT NULL AND ({})",
            ObservationWindow::text(owner),
            sources.any(&all)
        );
        Ok(sources)
    }

    /// The predicate matching a row of any of `tuples`.
    fn any(&self, tuples: &[usize]) -> String {
        tuples
            .iter()
            .map(|tuple| {
                let (card, verifier) = &self.tuples[*tuple];
                format!(
                    "(card_uid = {} AND verifier_card_uid = {})",
                    ObservationWindow::text(card),
                    ObservationWindow::text(verifier)
                )
            })
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    /// Whether some slot reads an input, and whether some reads an outcome.
    fn reads(&self) -> (bool, bool) {
        let input = |slot: &SourceSlot| matches!(slot.field, SourceField::Input(_));
        (
            self.slots.iter().any(input),
            self.slots.iter().any(|slot| !input(slot)),
        )
    }

    /// The source tuple of `row` in `batch`, `None` for a row of no tuple.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a tuple column is missing or
    /// malformed.
    fn tuple(&self, batch: &RecordBatch, row: usize) -> Result<Option<usize>, GroupPollError> {
        let card = text(batch, "card_uid", row)?;
        let verifier = text(batch, "verifier_card_uid", row)?;
        Ok(self.tuples.iter().position(|(c, v)| {
            card.as_deref() == Some(c.as_str()) && verifier.as_deref() == Some(v.as_str())
        }))
    }

    /// Decode identity rows, dropping rows of no source tuple.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a column is missing or a value
    /// is malformed.
    fn rows(&self, batches: &[RecordBatch]) -> Result<Vec<SourceRow>, GroupPollError> {
        let mut rows = Vec::new();
        for batch in batches {
            let times = times(batch, "at")?;
            for row in 0..batch.num_rows() {
                let Some(tuple) = self.tuple(batch, row)? else {
                    continue;
                };
                rows.push(SourceRow {
                    result: text(batch, "surface", row)?.as_deref() == Some("r"),
                    tuple,
                    execution_id: id(batch, row)?,
                    at: instant(times, row)?,
                });
            }
        }
        Ok(rows)
    }
}

/// One visible source input or result row's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceRow {
    /// Whether the row is a result rather than an input.
    result: bool,
    /// Index of its source tuple.
    tuple: usize,
    /// The source execution.
    execution_id: ExecutionId,
    /// Input `recorded_at` or result `ended_at`.
    at: DateTime<Utc>,
}

/// A group that ended: its finality cutoff and the coverage it is judged at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ended {
    /// Rows at or after this instant are past finality and never count;
    /// `None` when first + finality is past the latest representable
    /// instant, so every row counts.
    cutoff: Option<DateTime<Utc>>,
    /// Newest eligible row time.
    through: DateTime<Utc>,
    /// Distinct eligible `(surface, execution, time)` rows.
    members: u64,
}

impl Ended {
    /// Whether the group of `rows` and of the `over`-ceiling tuple counts
    /// ended at `clock` under `settings`.
    ///
    /// The first row anchors max_age and finality; rows at or after first +
    /// finality are dropped; the group ended when its newest eligible row is
    /// at least `idle` old or its first is at least `max_age` old. Ages are
    /// compared as elapsed spans, so no accepted duration overflows.
    // ponytail: an over-ceiling tuple contributes its whole count, refused
    // submissions and rows past finality included; list it too if that
    // precision ever matters for a group already judged inconclusive.
    fn judge<'a>(
        rows: &[SourceRow],
        over: impl Iterator<Item = &'a TupleCount> + Clone,
        settings: &GroupEndSettings,
        clock: DateTime<Utc>,
    ) -> Option<Self> {
        let first = rows
            .iter()
            .map(|row| row.at)
            .chain(over.clone().map(|count| count.first))
            .min()?;
        let cutoff = first.checked_add_signed(span(&settings.finality));
        let ended = Self {
            cutoff,
            through: first,
            members: 0,
        };
        let eligible = rows.iter().filter(|row| ended.eligible(row));
        let through = eligible
            .clone()
            .map(|row| row.at)
            .chain(over.clone().map(|count| count.newest))
            .max()?;
        let members = eligible
            .map(|row| (row.result, row.execution_id, row.at))
            .collect::<BTreeSet<_>>()
            .len();
        let members = u64::try_from(members)
            .unwrap_or(u64::MAX)
            .saturating_add(over.map(|count| count.members).sum());
        (clock.signed_duration_since(through) >= span(&settings.idle)
            || clock.signed_duration_since(first) >= span(&settings.max_age))
        .then_some(Self {
            cutoff,
            through,
            members,
        })
    }

    /// Whether `row` is timed before the finality cutoff.
    fn eligible(&self, row: &SourceRow) -> bool {
        self.cutoff.is_none_or(|cutoff| row.at < cutoff)
    }

    /// The eligible executions of `tuple` in list order: by earliest input
    /// time, or earliest result time for a result-only execution, then
    /// execution ID.
    fn executions(&self, rows: &[SourceRow], tuple: usize) -> Vec<ExecutionId> {
        let mut order: BTreeMap<ExecutionId, (bool, DateTime<Utc>)> = BTreeMap::new();
        for row in rows
            .iter()
            .filter(|row| row.tuple == tuple && self.eligible(row))
        {
            let key = (row.result, row.at);
            order
                .entry(row.execution_id)
                .and_modify(|known| *known = (*known).min(key))
                .or_insert(key);
        }
        let mut list: Vec<_> = order
            .into_iter()
            .map(|(execution, (_, at))| (at, execution))
            .collect();
        list.sort_unstable();
        list.into_iter().map(|(_, execution)| execution).collect()
    }
}

/// The selected source values of one snapshot, by execution.
#[derive(Default)]
struct Payloads {
    /// Each execution's earliest visible input declarations, as JSON.
    inputs: BTreeMap<ExecutionId, Value>,
    /// Each execution's authoritative result outcome, as JSON.
    outcomes: BTreeMap<ExecutionId, Value>,
}

impl Payloads {
    /// The value `execution` contributes for `field`, null when absent.
    ///
    /// A JSON input unwraps to its value and a trace input to its trace ID;
    /// a file or Artifact input is its declaration. An outcome is the named
    /// requirement's outcome in the authoritative result.
    fn value(&self, execution: ExecutionId, field: &SourceField) -> Value {
        let found = match field {
            SourceField::Input(slot) => self
                .inputs
                .get(&execution)
                .and_then(|inputs| inputs.get(slot.as_str()))
                .and_then(
                    |declaration| match declaration.get("kind").and_then(Value::as_str) {
                        Some("json") => declaration.get("value"),
                        Some("trace") => declaration.get("trace_id"),
                        _ => Some(declaration),
                    },
                ),
            SourceField::Outcome(requirement) => self
                .outcomes
                .get(&execution)
                .and_then(|outcome| outcome.get("requirements"))
                .and_then(Value::as_array)
                .and_then(|requirements| {
                    requirements.iter().find(|item| {
                        item.get("id").and_then(Value::as_str) == Some(requirement.as_str())
                    })
                })
                .and_then(|item| item.get("outcome")),
        };
        found.cloned().unwrap_or(Value::Null)
    }
}

/// `duration` as a chrono span, saturating at [`TimeDelta::MAX`], which no
/// difference of two instants reaches.
fn span(duration: &TriggerDuration) -> TimeDelta {
    i64::try_from(duration.seconds())
        .ok()
        .and_then(TimeDelta::try_seconds)
        .unwrap_or(TimeDelta::MAX)
}

/// Cell `row` of integer column `name`.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the cell is absent, null, or not a
/// non-negative integer.
fn number(batch: &RecordBatch, name: &str, row: usize) -> Result<u64, GroupPollError> {
    text(batch, name, row)?
        .ok_or_else(|| GroupPollError::Read(format!("null {name}")))?
        .parse()
        .map_err(malformed)
}

/// A malformed stored value as a read failure.
fn malformed(error: impl std::fmt::Display) -> GroupPollError {
    GroupPollError::Read(error.to_string())
}

/// The `name` column of `batch`.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the column is absent.
fn column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a dyn Array, GroupPollError> {
    batch
        .column_by_name(name)
        .map(AsRef::as_ref)
        .ok_or_else(|| GroupPollError::Read(format!("no {name} column")))
}

/// Cell `row` of text column `name`, `None` when null.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the column is absent or the cell
/// cannot be displayed.
fn text(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<String>, GroupPollError> {
    let column = column(batch, name)?;
    if column.is_null(row) {
        return Ok(None);
    }
    array_value_to_string(column, row)
        .map(Some)
        .map_err(malformed)
}

/// Cell `row` of the `execution_id` column.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the cell is absent, null, or not an
/// execution ID.
fn id(batch: &RecordBatch, row: usize) -> Result<ExecutionId, GroupPollError> {
    text(batch, "execution_id", row)?
        .ok_or_else(|| GroupPollError::Read("null execution_id".to_owned()))?
        .parse()
        .map_err(|error: IdError| malformed(error))
}

/// Timestamp column `name` of `batch`.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the column is absent or not
/// microsecond timestamps.
fn times<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a TimestampMicrosecondArray, GroupPollError> {
    column(batch, name)?
        .as_any()
        .downcast_ref()
        .ok_or_else(|| GroupPollError::Read(format!("{name} is not a microsecond timestamp")))
}

/// Instant `row` of `times`.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the cell is null or out of range.
fn instant(times: &TimestampMicrosecondArray, row: usize) -> Result<DateTime<Utc>, GroupPollError> {
    (!times.is_null(row))
        .then(|| DateTime::from_timestamp_micros(times.value(row)))
        .flatten()
        .ok_or_else(|| GroupPollError::Read("null or out-of-range row time".to_owned()))
}

/// Variant cell `row` of column `name` as JSON.
///
/// # Errors
/// Returns [`GroupPollError::Read`] when the column is absent or the cell
/// does not decode.
fn variant(batch: &RecordBatch, name: &str, row: usize) -> Result<Value, GroupPollError> {
    wyrd_queue::variant::variant_cell_to_json(column(batch, name)?, row)
        .map_err(|error| GroupPollError::Read(format!("{name}: {error:?}")))
}

/// Pure end, ordering, timing-boundary and value-mapping checks of the
/// poller; its Bifrost reads are proved by the grouped user journeys.
#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use serde_json::json;
    use wyrd_spec::verification::TaskId;

    use super::*;

    /// `seconds` after the fixture epoch.
    ///
    /// # Panics
    /// Never: the epoch is a valid instant.
    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
            .single()
            .expect("invariant: the fixture epoch is valid")
            + TimeDelta::seconds(seconds)
    }

    /// Settings with idle 30s, max_age 100s, finality 60s and no sources.
    ///
    /// # Panics
    /// Never: the durations are valid.
    fn settings() -> GroupEndSettings {
        let duration = |text| TriggerDuration::parse(text).expect("invariant: valid duration");
        GroupEndSettings {
            idle: duration("30s"),
            max_age: duration("100s"),
            finality: duration("60s"),
            sources: BTreeMap::new(),
        }
    }

    /// No over-ceiling tuple counts.
    fn none() -> std::iter::Empty<&'static TupleCount> {
        std::iter::empty()
    }

    /// One input (`result` false) or result row of tuple 0.
    fn row(result: bool, execution_id: ExecutionId, seconds: i64) -> SourceRow {
        SourceRow {
            result,
            tuple: 0,
            execution_id,
            at: at(seconds),
        }
    }

    /// Ending honours idle, max_age and finality; members count distinct
    /// surface/execution rows; lists order by input time, or result time
    /// for a result-only execution, then execution ID, past finality
    /// excluded.
    ///
    /// # Panics
    /// Panics when a group ends, counts, or orders otherwise.
    #[test]
    fn groups_end_count_and_order_by_their_rows() {
        let mut ids = [(); 5].map(|()| ExecutionId::new_v7());
        ids.sort_unstable();
        let [a, b, c, d, late] = ids;
        let rows = [
            row(false, b, 10),
            row(true, b, 12),
            row(true, b, 12),
            row(true, a, 8),
            row(false, a, 10),
            row(true, c, 5),
            row(true, d, 11),
            row(false, late, 65),
        ];
        assert_eq!(Ended::judge(&rows, none(), &settings(), at(41)), None);
        let ended = Ended::judge(&rows, none(), &settings(), at(42)).expect("idle reached");
        assert_eq!(
            ended.through,
            at(12),
            "rows at first + finality never count"
        );
        assert_eq!(ended.members, 6);
        assert_eq!(ended.executions(&rows, 0), [c, a, b, d]);
        assert!(ended.executions(&rows, 1).is_empty());

        let active = [row(false, a, 0), row(false, b, 50)];
        assert!(Ended::judge(&active, none(), &settings(), at(79)).is_none());
        assert!(Ended::judge(&active, none(), &settings(), at(80)).is_some());
        let busy = [row(false, a, 0), row(false, b, 55), row(false, c, 59)];
        assert!(Ended::judge(&busy, none(), &settings(), at(88)).is_none());
        let aged = [row(false, a, 0), row(false, b, 59), row(false, c, 99)];
        assert!(
            Ended::judge(&aged, none(), &settings(), at(89)).is_some(),
            "a row past finality does not postpone idle"
        );
    }

    /// An over-ceiling tuple's count anchors the group's first row, extends
    /// its newest activity and adds its rows to the coverage.
    ///
    /// # Panics
    /// Panics when the count is judged otherwise.
    #[test]
    fn over_ceiling_counts_join_the_group_end() {
        let execution = ExecutionId::new_v7();
        let rows = [row(false, execution, 20)];
        let count = TupleCount {
            executions: 21,
            members: 40,
            first: at(0),
            newest: at(25),
        };
        let judge = |clock| Ended::judge(&rows, std::iter::once(&count), &settings(), at(clock));
        assert!(judge(54).is_none(), "the count's newest row postpones idle");
        let ended = judge(55).expect("idle reached from the count's newest row");
        assert_eq!((ended.through, ended.members), (at(25), 41));
        assert_eq!(
            ended.cutoff,
            Some(at(60)),
            "the count's first row anchors finality"
        );
        assert!(Ended::judge(&[], std::iter::once(&count), &settings(), at(55)).is_some());
    }

    /// Every accepted duration, up to the largest of each unit, judges
    /// without overflow and with the mathematically correct eligibility.
    ///
    /// # Panics
    /// Panics when a duration overflows or judges otherwise.
    #[test]
    fn largest_accepted_durations_judge_without_overflow() {
        let duration = |text| TriggerDuration::parse(text).expect("invariant: valid duration");
        let execution = ExecutionId::new_v7();
        let rows = [row(false, execution, 0)];
        for largest in ["999999999s", "999999999m", "999999999h", "999999999d"] {
            let never = GroupEndSettings {
                idle: duration(largest),
                max_age: duration(largest),
                finality: duration(largest),
                sources: BTreeMap::new(),
            };
            assert_eq!(
                Ended::judge(&rows, none(), &never, at(1_000)),
                None,
                "{largest}"
            );
            assert_eq!(
                Ended::judge(&rows, none(), &never, DateTime::<Utc>::MAX_UTC),
                (largest != "999999999d").then_some(Ended {
                    cutoff: at(0).checked_add_signed(span(&duration(largest))),
                    through: at(0),
                    members: 1,
                }),
                "{largest} ends exactly when its span has elapsed"
            );
            let quick = GroupEndSettings {
                idle: duration("1s"),
                ..never
            };
            let ended = Ended::judge(&rows, none(), &quick, at(1)).expect("idle reached");
            assert!(ended.eligible(&row(false, execution, 1_000_000)));
        }
        let mut exact = settings();
        exact.finality = duration("1s");
        let late = row(false, ExecutionId::new_v7(), 1);
        let ended = Ended::judge(&[rows[0], late], none(), &exact, at(30)).expect("idle reached");
        assert!(!ended.eligible(&late), "first + finality is past finality");
    }

    /// Values unwrap JSON and trace inputs, keep other declarations, read
    /// requirement outcomes, and are null when absent.
    ///
    /// # Panics
    /// Panics when a value maps otherwise.
    #[test]
    fn values_unwrap_inputs_and_outcomes() {
        let (seen, missing) = (ExecutionId::new_v7(), ExecutionId::new_v7());
        let file = json!({"kind": "file", "sha256": "ab", "size_bytes": 1, "format": "png"});
        let payloads = Payloads {
            inputs: BTreeMap::from([(
                seen,
                json!({
                    "turn": {"kind": "json", "value": {"q": 1}},
                    "trace": {"kind": "trace", "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736"},
                    "frame": file,
                }),
            )]),
            outcomes: BTreeMap::from([(
                seen,
                json!({"requirements": [{"id": "on-topic", "outcome": "failed"}]}),
            )]),
        };
        let input = |slot| SourceField::Input(SlotName::new(slot).expect("invariant: valid slot"));
        let outcome =
            |id| SourceField::Outcome(TaskId::new(id).expect("invariant: valid requirement"));
        assert_eq!(payloads.value(seen, &input("turn")), json!({"q": 1}));
        assert_eq!(
            payloads.value(seen, &input("trace")),
            json!("4bf92f3577b34da6a3ce929d0e0e4736")
        );
        assert_eq!(payloads.value(seen, &input("frame")), file);
        assert_eq!(payloads.value(seen, &input("absent")), Value::Null);
        assert_eq!(payloads.value(missing, &input("turn")), Value::Null);
        assert_eq!(payloads.value(seen, &outcome("on-topic")), json!("failed"));
        assert_eq!(payloads.value(missing, &outcome("on-topic")), Value::Null);
    }
}
