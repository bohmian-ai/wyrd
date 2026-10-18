//! Turns ended groups of a `group_end` binding into group executions.
//!
//! [`GroupPoller`] serves one binding claimed by
//! [`VerifierExecutionQueue::claim_due_group`]. It discovers, in Bifrost, the
//! groups with a source row still within the binding's finality, newest
//! activity first. Per candidate the queue names the group's refused
//! submissions and selected results, and one Bifrost statement then reads
//! every eligible source row of the group with its value: membership,
//! activity, the per-source execution count and the values all come from the
//! same rows, and a source over the deployment ceiling reads no value. Rust
//! decides whether the group ended (idle or max_age) before its finality
//! passed. A due group whose rows no queued group execution covers gets one
//! pinned snapshot: per source slot one ordered, deduplicated list of its
//! executions' inputs or authoritative results, or, past the ceiling, an
//! empty list marked with it. Every row time is Bifrost's managed
//! `wyrd_ingested_at`, stamped from PostgreSQL, and the tick clock is
//! PostgreSQL's. Every read runs under the binding owner's accepted
//! authority, executed by SYSTEM.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use arrow::array::{Array, Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::util::display::array_value_to_string;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wyrd_spec::card::trigger::TriggerDuration;
use wyrd_spec::card::verifier::{SourceField, SourceText};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ExecutionId, GroupId, IdError, SlotName};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::verification::execution::InputDeclaration;
use wyrd_spec::verification::{GroupCoverage, GroupEndSettings, GroupSourceSnapshot};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::{
    DueGroupBinding, EnqueueOutcome, ExecutionAuthority, GroupSnapshot, GroupSubmissions,
    VerifierExecutionQueue,
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
    /// candidates; each candidate is skipped when it is not yet due, is past
    /// finality, has an outstanding group execution, or is already covered
    /// by the latest one. Rows of a submission the queue refused, such as an
    /// idempotency conflict, and result rows a queued execution's settlement
    /// did not select are no source rows. The caller commits the returned
    /// enqueues together with the binding's advanced cursor.
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
            let submissions = queue.group_submissions(conn, &group).await?;
            let read = poll.read(&group, &submissions).await?;
            let Some(ended) = Ended::judge(&read.rows, &due.settings, due.clock) else {
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
            let snapshot = poll.snapshot(group, &read, &ended);
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
    /// Groups with a visible source row within the binding's finality of the
    /// tick clock, newest activity first, at most [`GROUPS_PER_POLL`].
    ///
    /// Finality is the only discovery bound; the queue's coverage decides
    /// what is new. A refused submission may be a group's earliest row, so
    /// discovery bounds the newest row and [`Ended::judge`] checks the first
    /// eligible one. A finality reaching before the earliest representable
    /// instant bounds nothing. A tenant without a results table is discovered from inputs
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
            .map(|since| format!(" HAVING MAX(at) >= {}", ObservationWindow::instant(since)))
            .unwrap_or_default();
        // ponytail: scans the binding's whole retained source history each
        // poll; prune by a time-partitioned semi-join if that outgrows a poll.
        let sql = |rows: String| {
            format!(
                "SELECT group_id, MAX(at) AS newest FROM ({rows}) s GROUP BY group_id{open} \
                 ORDER BY newest DESC, group_id LIMIT {GROUPS_PER_POLL}"
            )
        };
        let all = |results| self.sources.surfaces("true", "true", results);
        let batches = match self.run(&sql(all(true))).await {
            Err(error) if error.code() == TABLE_NOT_FOUND => {
                let batches = self.query(&sql(all(false))).await?;
                self.results = false;
                batches
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

    /// Read every eligible source row of `group` with its value, in one
    /// statement.
    ///
    /// `submissions` removes refused submissions and the result rows a
    /// queued execution's settlement did not select, so an unselected or
    /// abandoned attempt's result never covers the outcome the queue later
    /// selects. A source whose rows hold more distinct executions than the
    /// ceiling is marked over and its rows carry no value; every other row
    /// carries the value some slot reads. Membership and values come from the
    /// same rows, so a listed execution never lacks its visible value.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when the read fails or a row is
    /// malformed.
    async fn read(
        &self,
        group: &GroupId,
        submissions: &GroupSubmissions,
    ) -> Result<GroupRead, GroupPollError> {
        let sql = self
            .sources
            .read(group, submissions, self.results, self.ceiling);
        self.sources.decode(&self.query(&sql).await?, self.ceiling)
    }

    /// Pin the snapshot of one ended `group` from its `read`: per source
    /// binding its executions in list order, then per slot one aligned list.
    ///
    /// A source binding over the ceiling pins, for each of its slots, an
    /// empty list marked with the ceiling. Every other slot takes its value
    /// per execution, null when absent.
    fn snapshot(&self, group: GroupId, read: &GroupRead, ended: &Ended) -> GroupSnapshot {
        let ceiling = u32::try_from(self.ceiling).unwrap_or(u32::MAX);
        let mut inputs = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for slot in &self.sources.slots {
            let over = read.over.contains(&slot.tuple);
            let list = if over {
                Vec::new()
            } else {
                Ended::executions(&read.rows, slot.tuple)
            };
            let values = list
                .iter()
                .map(|execution| read.payloads.value(*execution, &slot.field))
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
                    exceeded_ceiling: over.then_some(ceiling),
                },
            );
        }
        GroupSnapshot {
            group_id: group,
            inputs,
            coverage: GroupCoverage {
                through: ended.through,
                members: ended.members,
                sources,
            },
        }
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

    /// Both source surfaces as one `(surface, group_id, card_uid,
    /// verifier_card_uid, execution_id, result_id, at, payload)` row set
    /// restricted by `filter`, result rows further by `authority`; inputs
    /// alone without a `results` table. `at` is the managed
    /// `wyrd_ingested_at` and `payload` the input declarations or result
    /// outcome.
    fn surfaces(&self, filter: &str, authority: &str, results: bool) -> String {
        let inputs = format!(
            "SELECT 'i' AS surface, group_id, card_uid, verifier_card_uid, execution_id, \
             CAST(NULL AS VARCHAR) AS result_id, wyrd_ingested_at AS at, inputs AS payload \
             FROM vala.verification.execution_inputs WHERE {} AND {filter}",
            self.eligible,
        );
        if !results {
            return inputs;
        }
        format!(
            "{inputs} UNION ALL SELECT 'r' AS surface, group_id, card_uid, verifier_card_uid, \
             execution_id, result_id, wyrd_ingested_at AS at, outcome AS payload \
             FROM vala.verification.results WHERE {} AND {filter} AND {authority}",
            self.eligible,
        )
    }

    /// The one statement reading every eligible source row of `group` with
    /// its distinct-execution count per source tuple, and its payload while
    /// that count is within `ceiling` and some slot reads its surface.
    ///
    /// Refused submissions are no rows; a queued execution's result row
    /// counts only when its settlement selected it. The count is a window
    /// over the same scan, so the count and the values agree with the rows.
    // ponytail: reads every identity of the group, growing with its size,
    // and inlines the group's queued identities; cap it beside whole-group
    // first/newest times if the group-end benchmark shows this read matters.
    fn read(
        &self,
        group: &GroupId,
        submissions: &GroupSubmissions,
        results: bool,
        ceiling: u64,
    ) -> String {
        let refused = among("execution_id", submissions.refused.iter());
        let queued = among("execution_id", submissions.selected.keys());
        let chosen = among("result_id", submissions.selected.values().flatten());
        let (inputs, outcomes) = self.reads();
        format!(
            "SELECT surface, card_uid, verifier_card_uid, execution_id, at, executions, \
             CASE WHEN executions <= {ceiling} \
             AND ((surface = 'i' AND {inputs}) OR (surface = 'r' AND {outcomes})) \
             THEN payload END AS payload FROM (SELECT *, COUNT(DISTINCT execution_id) \
             OVER (PARTITION BY card_uid, verifier_card_uid) AS executions FROM ({}) s) s",
            self.surfaces(
                &format!("{} AND NOT {refused}", same_group(group)),
                &format!("(NOT {queued} OR {chosen})"),
                results,
            )
        )
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

    /// Decode the rows of one group read: its source rows, the tuples
    /// whose execution count exceeds `ceiling`, and each execution's earliest
    /// input and result value; rows of no source tuple are dropped.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a column is missing or a value
    /// is malformed.
    fn decode(&self, batches: &[RecordBatch], ceiling: u64) -> Result<GroupRead, GroupPollError> {
        let mut read = GroupRead::default();
        let mut earliest = BTreeMap::new();
        for batch in batches {
            let times = times(batch, "at")?;
            let executions = column(batch, "executions")?
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| GroupPollError::Read("executions is not a count".to_owned()))?;
            let payload = column(batch, "payload")?;
            for row in 0..batch.num_rows() {
                let Some(tuple) = self.tuple(batch, row)? else {
                    continue;
                };
                let source = SourceRow {
                    result: text(batch, "surface", row)?.as_deref() == Some("r"),
                    tuple,
                    execution_id: id(batch, row)?,
                    at: instant(times, row)?,
                };
                if u64::try_from(executions.value(row)).unwrap_or(u64::MAX) > ceiling {
                    read.over.insert(tuple);
                }
                let key = (source.result, source.execution_id);
                if !payload.is_null(row)
                    && earliest.get(&key).is_none_or(|known| source.at < *known)
                {
                    earliest.insert(key, source.at);
                    let values = if source.result {
                        &mut read.payloads.outcomes
                    } else {
                        &mut read.payloads.inputs
                    };
                    values.insert(source.execution_id, variant(batch, "payload", row)?);
                }
                read.rows.push(source);
            }
        }
        Ok(read)
    }
}

/// One group's eligible source rows, read in one statement with their
/// values.
#[derive(Default)]
struct GroupRead {
    /// Every eligible source row; a replayed row may repeat.
    rows: Vec<SourceRow>,
    /// Source tuples with more distinct executions than the ceiling; their
    /// rows carry no value.
    over: BTreeSet<usize>,
    /// Each execution's earliest visible input and authoritative result
    /// value a slot reads.
    payloads: Payloads,
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
    /// Its managed `wyrd_ingested_at`, stamped from PostgreSQL.
    at: DateTime<Utc>,
}

/// A group that ended before its finality: the coverage it is judged at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ended {
    /// Newest row time.
    through: DateTime<Utc>,
    /// Distinct `(surface, execution)` rows; a replayed row counts once.
    members: u64,
}

impl Ended {
    /// Whether the group of `rows` ended at `clock` under `settings`, before
    /// its finality.
    ///
    /// The first row anchors max_age and finality. The group ended when its
    /// newest row is at least `idle` old or its first is at least `max_age`
    /// old. It is past finality, and never judged again, once the clock or
    /// any row reaches first + finality: every row time and the clock come
    /// from PostgreSQL, so a row past finality arrives only after it. Ages
    /// are compared as elapsed spans, so no accepted duration overflows.
    fn judge(
        rows: &[SourceRow],
        settings: &GroupEndSettings,
        clock: DateTime<Utc>,
    ) -> Option<Self> {
        let first = rows.iter().map(|row| row.at).min()?;
        let through = rows.iter().map(|row| row.at).max()?;
        // `None` when first + finality is past the latest representable
        // instant, so the group never reaches finality.
        let cutoff = first.checked_add_signed(span(&settings.finality));
        let members = rows
            .iter()
            .map(|row| (row.result, row.execution_id))
            .collect::<BTreeSet<_>>()
            .len();
        (cutoff.is_none_or(|cutoff| clock.max(through) < cutoff)
            && (clock.signed_duration_since(through) >= span(&settings.idle)
                || clock.signed_duration_since(first) >= span(&settings.max_age)))
        .then_some(Self {
            through,
            members: u64::try_from(members).unwrap_or(u64::MAX),
        })
    }

    /// The executions of `tuple` in `rows` in list order: by earliest input
    /// time, or earliest result time for a result-only execution, then
    /// execution ID.
    fn executions(rows: &[SourceRow], tuple: usize) -> Vec<ExecutionId> {
        let mut order: BTreeMap<ExecutionId, (bool, DateTime<Utc>)> = BTreeMap::new();
        for row in rows.iter().filter(|row| row.tuple == tuple) {
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

/// The predicate matching a `column` value among `values`; `false` for none.
fn among(column: &str, values: impl Iterator<Item = impl ToString>) -> String {
    let values: Vec<String> = values
        .map(|value| ObservationWindow::text(&value.to_string()))
        .collect();
    if values.is_empty() {
        return "false".to_owned();
    }
    format!("{column} IN ({})", values.join(", "))
}

/// The `group_id = <group>` predicate.
fn same_group(group: &GroupId) -> String {
    format!("group_id = {}", ObservationWindow::text(group.as_str()))
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
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::verification::{GroupSource, TaskId};

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

    /// Settings with idle 30s, max_age 100s, finality 200s and no sources.
    ///
    /// # Panics
    /// Never: the durations are valid.
    fn settings() -> GroupEndSettings {
        let duration = |text| TriggerDuration::parse(text).expect("invariant: valid duration");
        GroupEndSettings {
            idle: duration("30s"),
            max_age: duration("100s"),
            finality: duration("200s"),
            sources: BTreeMap::new(),
        }
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

    /// Two source slots of `owner` reading inputs of two source Verifiers.
    ///
    /// # Panics
    /// Never: the fixture declarations are valid.
    fn two_sources(owner: &CardUid) -> Sources {
        let uid = || CardUid::from_uuid(uuid::Uuid::now_v7()).expect("invariant: a v7 card uid");
        let (frames, fact) = (uid(), uid());
        let source = |verifier: &CardUid, text: &str| GroupSource {
            reference: serde_json::from_value(json!(text)).expect("invariant: source text"),
            subject_occurrence_key: "agent".to_owned(),
            card_uid: owner.clone(),
            verifier_card_uid: verifier.clone(),
        };
        let mut settings = settings();
        settings.sources = BTreeMap::from([
            (
                SlotName::new("frames").expect("invariant: slot"),
                source(&frames, "verifiers.frame.inputs.frame"),
            ),
            (
                SlotName::new("fact").expect("invariant: slot"),
                source(&fact, "verifiers.fact.inputs.fact"),
            ),
        ]);
        Sources::new(&settings, owner.as_str()).expect("sources parse")
    }

    /// Ending honours idle, max_age and finality; members count distinct
    /// surface/execution rows; lists order by input time, or result time
    /// for a result-only execution, then execution ID.
    ///
    /// # Panics
    /// Panics when a group ends, counts, or orders otherwise.
    #[test]
    fn groups_end_count_and_order_by_their_rows() {
        let mut ids = [(); 4].map(|()| ExecutionId::new_v7());
        ids.sort_unstable();
        let [a, b, c, d] = ids;
        let rows = [
            row(false, b, 10),
            row(true, b, 12),
            row(true, b, 12),
            row(true, a, 8),
            row(false, a, 10),
            row(true, c, 5),
            row(true, d, 11),
        ];
        assert_eq!(Ended::judge(&rows, &settings(), at(41)), None);
        let ended = Ended::judge(&rows, &settings(), at(42)).expect("idle reached");
        assert_eq!(ended.through, at(12));
        assert_eq!(ended.members, 6);
        assert_eq!(Ended::executions(&rows, 0), [c, a, b, d]);
        assert!(Ended::executions(&rows, 1).is_empty());

        let active = [row(false, a, 0), row(false, b, 50)];
        assert!(Ended::judge(&active, &settings(), at(79)).is_none());
        assert!(Ended::judge(&active, &settings(), at(80)).is_some());
        let busy = [row(false, a, 0), row(false, b, 90)];
        assert!(Ended::judge(&busy, &settings(), at(99)).is_none());
        assert!(
            Ended::judge(&busy, &settings(), at(100)).is_some(),
            "max_age ends a busy group"
        );
        assert!(
            Ended::judge(&active, &settings(), at(200)).is_none(),
            "the clock reached first + finality"
        );
        let late = [row(false, a, 0), row(false, b, 200)];
        assert!(
            Ended::judge(&late, &settings(), at(199)).is_none(),
            "a row reached first + finality"
        );
    }

    /// Every accepted duration, up to the largest of each unit, judges
    /// without overflow: finality is reached exactly at first + finality,
    /// and a finality past the latest instant is never reached.
    ///
    /// # Panics
    /// Panics when a duration overflows or judges otherwise.
    #[test]
    fn largest_accepted_durations_judge_without_overflow() {
        let duration = |text| TriggerDuration::parse(text).expect("invariant: valid duration");
        let rows = [row(false, ExecutionId::new_v7(), 0)];
        for largest in ["999999999s", "999999999m", "999999999h", "999999999d"] {
            let never = GroupEndSettings {
                idle: duration(largest),
                max_age: duration(largest),
                finality: duration(largest),
                sources: BTreeMap::new(),
            };
            assert_eq!(Ended::judge(&rows, &never, at(1_000)), None, "{largest}");
            let quick = GroupEndSettings {
                idle: duration("1s"),
                ..never
            };
            match at(0).checked_add_signed(span(&quick.finality)) {
                Some(cutoff) => {
                    assert!(Ended::judge(&rows, &quick, cutoff - TimeDelta::seconds(1)).is_some());
                    assert_eq!(Ended::judge(&rows, &quick, cutoff), None, "{largest}");
                }
                None => assert!(Ended::judge(&rows, &quick, DateTime::<Utc>::MAX_UTC).is_some()),
            }
        }
    }

    /// The one read selects the owner's source tuples of the group by
    /// ingest time, without refused submissions or unselected queued
    /// results, and gates every payload by its tuple's execution count.
    ///
    /// # Panics
    /// Panics when the statement selects other rows.
    #[test]
    fn one_read_selects_eligible_rows_and_gates_payloads() {
        let owner = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("invariant: a v7 card uid");
        let sources = two_sources(&owner);
        let group = GroupId::new("event-1").expect("invariant: group");
        let (refused, queued) = (ExecutionId::new_v7(), ExecutionId::new_v7());
        let chosen = wyrd_spec::ids::VerificationResultId::new_v7();
        let submissions = GroupSubmissions {
            refused: BTreeSet::from([refused]),
            selected: BTreeMap::from([(queued, Some(chosen))]),
        };
        let sql = sources.read(&group, &submissions, true, 7);
        assert_eq!(sql.matches("AND group_id = 'event-1'").count(), 2, "{sql}");
        assert_eq!(sql.matches(&sources.eligible).count(), 2, "{sql}");
        assert_eq!(
            sql.matches(&format!("NOT execution_id IN ('{refused}')"))
                .count(),
            2,
            "{sql}"
        );
        assert!(
            sql.contains(&format!(
                "(NOT execution_id IN ('{queued}') OR result_id IN ('{chosen}'))"
            )),
            "{sql}"
        );
        assert!(sql.contains("CASE WHEN executions <= 7"), "{sql}");
        assert!(sql.contains("(surface = 'r' AND false)"), "{sql}");
        assert!(
            !sql.contains("recorded_at") && !sql.contains("ended_at"),
            "{sql}"
        );
        assert!(
            !sql.contains("LIMIT") && !sql.contains("RANK"),
            "every identity of the group is read: {sql}"
        );
        let none = sources.read(&group, &GroupSubmissions::default(), false, 7);
        assert!(
            none.contains("NOT false") && !none.contains("results"),
            "{none}"
        );
    }

    /// Decoding marks a tuple over the ceiling by its window count, keeps
    /// replayed rows for deduplication, and drops rows of no source tuple.
    ///
    /// # Panics
    /// Panics when a row decodes otherwise.
    #[test]
    fn decoding_marks_tuples_over_the_ceiling() {
        use std::sync::Arc;

        use arrow::array::StringArray;

        let owner = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("invariant: a v7 card uid");
        let sources = two_sources(&owner);
        let (frames, fact) = (&sources.tuples[0], &sources.tuples[1]);
        let execution = ExecutionId::new_v7().to_string();
        let text = |values: [&str; 3]| Arc::new(StringArray::from(values.to_vec())) as _;
        let batch = RecordBatch::try_from_iter([
            ("surface", text(["i", "i", "i"])),
            ("card_uid", text([&frames.0, &fact.0, &fact.0])),
            ("verifier_card_uid", text([&frames.1, &fact.1, "unbound"])),
            ("execution_id", text([&execution, &execution, &execution])),
            (
                "at",
                Arc::new(TimestampMicrosecondArray::from(vec![0, 0, 0]).with_timezone("UTC")) as _,
            ),
            ("executions", Arc::new(Int64Array::from(vec![3, 2, 9])) as _),
            (
                "payload",
                Arc::new(StringArray::from(vec![None::<&str>; 3])) as _,
            ),
        ])
        .expect("invariant: aligned columns");
        let read = sources.decode(&[batch], 2).expect("rows decode");
        assert_eq!(read.over, BTreeSet::from([0]));
        assert_eq!(read.rows.len(), 2, "the unbound row is no source row");
        assert!(read.payloads.inputs.is_empty());
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
