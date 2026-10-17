//! Turns ended groups of a `group_end` binding into group executions.
//!
//! [`GroupPoller`] serves one binding claimed by
//! [`VerifierExecutionQueue::claim_due_group`]. It discovers, in Bifrost, the
//! groups whose newest source input or result row is newer than the
//! binding's own latest visible group result, reads each candidate's source
//! row identities, and decides in Rust whether the group ended (idle or
//! max_age) within its finality bound. A due group whose newest row no
//! queued group execution covers gets one pinned snapshot: per source slot
//! one ordered, deduplicated list read from the source executions' inputs
//! or authoritative results, or, past the deployment ceiling, no read at all.
//! Every read runs as the tenant SYSTEM principal serving the binding owner.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use arrow::array::{Array, RecordBatch, TimestampMicrosecondArray};
use arrow::util::display::array_value_to_string;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::tables::DomainTable;
use vala_bifrost_redux::tables::verification::{ExecutionInputsTable, ResultsTable};
use wyrd_spec::DataTenantId;
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

use super::authority::SystemReadAuthority;
use super::drift::ObservationWindow;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::state::AppState;

/// Error code of a read of a Bifrost table the tenant never created.
const TABLE_NOT_FOUND: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

/// Candidate groups one poll of one binding examines, least recently active
/// first.
// ponytail: a binding with more ended, unjudged groups than this drains them
// over successive polls; page by newest row time if one poll must take more.
const GROUPS_PER_POLL: usize = 64;

/// How far past `idle` discovery still looks for a group's newest row.
// ponytail: a group that ended more than this before any poll ran (a
// scheduler outage) is not rediscovered; widen it or persist a watermark if
// outages that long must still be judged.
const DISCOVERY_SLACK: TimeDelta = TimeDelta::hours(1);

/// Source row identities one group may hold; a larger group is skipped,
/// never judged truncated.
const ROWS_PER_GROUP: usize = 100_000;

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
    /// claimed binding `due` in `tenant`, under `authority`.
    ///
    /// Discovery reads at most [`GROUPS_PER_POLL`] candidates; each candidate
    /// is skipped when it is not yet due, has an outstanding group execution,
    /// or is already covered by the latest one. Rows of a submission the
    /// queue refused, such as an idempotency conflict, are not source rows
    /// and are dropped before the end is judged. A candidate whose rows exceed
    /// [`ROWS_PER_GROUP`] is logged and skipped. The caller commits the
    /// returned enqueues together with the binding's advanced cursor.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Sql`] when a Postgres statement fails and
    /// [`GroupPollError::Read`] when a Bifrost read fails or a stored
    /// source declaration or row is malformed; enqueues made before the
    /// failure stay in the caller's transaction.
    pub async fn poll(
        &self,
        conn: &mut TenantConn<'_>,
        queue: &VerifierExecutionQueue,
        tenant: DataTenantId,
        due: &DueGroupBinding,
        authority: &ExecutionAuthority,
    ) -> Result<Vec<EnqueueOutcome>, GroupPollError> {
        let read = SystemReadAuthority::resolve(
            &self.state,
            tenant,
            Some(authority.attribution().act_principal_id),
            &[
                TableRef::new(BifrostNamespace::Verification, ExecutionInputsTable::NAME),
                TableRef::new(BifrostNamespace::Verification, ResultsTable::NAME),
            ],
        )
        .await
        .map_err(|error| GroupPollError::Read(error.to_string()))?
        .serving(authority.authority());
        if read.context().principal.effective_permissions.is_empty() {
            return Ok(Vec::new());
        }
        let mut poll = BindingPoll {
            query: ScheduledQueryCaller::new(
                self.state.clone(),
                read.into_context(),
                CancellationToken::new(),
            ),
            deadline_ms: i64::try_from(self.query_timeout.as_millis()).unwrap_or(i64::MAX),
            sources: Sources::new(&due.settings, &due.owner_card_uid.to_string())?,
            ceiling: self.ceiling as usize,
            due,
            results: true,
        };
        let mut enqueued = Vec::new();
        for group in poll.candidates().await? {
            let mut rows = poll.identities(&group).await?;
            if rows.len() > ROWS_PER_GROUP {
                tracing::warn!(binding_id = %due.binding_id, group_id = %group, "group exceeds its source row bound; not judged");
                continue;
            }
            let ids: Vec<ExecutionId> = rows.iter().map(|row| row.execution_id).collect();
            let refused = queue.refused_submissions(conn, &ids).await?;
            rows.retain(|row| !refused.contains(&row.execution_id));
            let Some(ended) = Ended::judge(&rows, &due.settings, due.clock) else {
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
            let snapshot = poll.snapshot(conn, queue, group, &rows, &ended).await?;
            enqueued.push(
                queue
                    .enqueue_group(conn, due.binding_id, &snapshot, authority)
                    .await?,
            );
        }
        Ok(enqueued)
    }
}

/// One claimed binding's poll: its SYSTEM reader, sources and filters.
struct BindingPoll<'a> {
    /// The authorized caller every statement runs through.
    query: ScheduledQueryCaller,
    /// Deadline of one statement, in milliseconds.
    deadline_ms: i64,
    /// The binding's parsed source slots.
    sources: Sources,
    /// The deployment source-execution ceiling of one source reference.
    ceiling: usize,
    /// The claimed binding.
    due: &'a DueGroupBinding,
    /// Whether the tenant's results table exists; until it does, sources
    /// have inputs only and the binding has no group result.
    results: bool,
}

impl BindingPoll<'_> {
    /// Groups with a source row in the discovery window newer than the
    /// binding's latest visible result for them, least recently active
    /// first, at most [`GROUPS_PER_POLL`].
    ///
    /// A tenant without a results table is discovered from inputs alone,
    /// and the poll remembers that for its later reads.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a read fails or a group ID is
    /// malformed.
    async fn candidates(&mut self) -> Result<Vec<GroupId>, GroupPollError> {
        let since = ObservationWindow::instant(
            self.due.clock - span(&self.due.settings.idle) - DISCOVERY_SLACK,
        );
        let window = |column: &str| format!("{column} >= {since}");
        let judged = format!(
            "SELECT s.group_id, MAX(s.at) AS newest FROM ({rows}) s \
             LEFT JOIN (SELECT group_id, MAX(ended_at) AS judged FROM vala.verification.results \
             WHERE binding_id = {binding} AND verifier_card_uid = {verifier} \
             AND group_id IS NOT NULL AND ended_at >= {since} GROUP BY group_id) j \
             ON s.group_id = j.group_id \
             GROUP BY s.group_id, j.judged HAVING j.judged IS NULL OR MAX(s.at) > j.judged \
             ORDER BY newest, s.group_id LIMIT {GROUPS_PER_POLL}",
            rows = self.surfaces(window),
            binding = ObservationWindow::text(&self.due.binding_id.to_string()),
            verifier = ObservationWindow::text(&self.due.verifier_card_uid.to_string()),
        );
        let batches = match self.run(&judged).await {
            Err(error) if error.code() == TABLE_NOT_FOUND => {
                self.results = false;
                self.query(&format!(
                    "SELECT group_id, MAX(at) AS newest FROM ({}) s GROUP BY group_id \
                     ORDER BY newest, group_id LIMIT {GROUPS_PER_POLL}",
                    self.surfaces(window)
                ))
                .await?
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

    /// Every visible source row identity of `group`, one more than
    /// [`ROWS_PER_GROUP`] at most.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when the read fails or a row is
    /// malformed.
    async fn identities(&self, group: &GroupId) -> Result<Vec<SourceRow>, GroupPollError> {
        let group = ObservationWindow::text(group.as_str());
        let batches = self
            .query(&format!(
                "{} LIMIT {}",
                self.surfaces(|_| format!("group_id = {group}")),
                ROWS_PER_GROUP + 1
            ))
            .await?;
        self.sources.rows(&batches)
    }

    /// Pin the snapshot of one ended `group`: per source binding its
    /// eligible executions in list order, then per slot one aligned list.
    ///
    /// A source binding over the ceiling reads nothing: each of its slots
    /// pins an empty list marked with the ceiling. Otherwise its inputs
    /// and authoritative results are read once and every slot takes its
    /// value per execution, null when absent.
    ///
    /// # Errors
    /// Returns [`GroupPollError`] when a read fails or a row is malformed.
    async fn snapshot(
        &self,
        conn: &mut TenantConn<'_>,
        queue: &VerifierExecutionQueue,
        group: GroupId,
        rows: &[SourceRow],
        ended: &Ended,
    ) -> Result<GroupSnapshot, GroupPollError> {
        let lists: Vec<Vec<ExecutionId>> = (0..self.sources.tuples.len())
            .map(|tuple| ended.executions(rows, tuple))
            .collect();
        let readable: BTreeSet<ExecutionId> = lists
            .iter()
            .filter(|list| list.len() <= self.ceiling)
            .flatten()
            .copied()
            .collect();
        let payloads = if readable.is_empty() {
            Payloads::default()
        } else {
            let ids: Vec<ExecutionId> = readable.iter().copied().collect();
            let selected = queue.selected_results(conn, &ids).await?;
            self.payloads(&group, ended.cutoff, &readable, &selected)
                .await?
        };
        let mut inputs = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for slot in &self.sources.slots {
            let list = &lists[slot.tuple];
            let over = list.len() > self.ceiling;
            let (values, execution_ids) = if over {
                (Vec::new(), Vec::new())
            } else {
                (
                    list.iter()
                        .map(|execution| payloads.value(*execution, &slot.field))
                        .collect(),
                    list.clone(),
                )
            };
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
                    execution_ids,
                    exceeded_ceiling: over.then(|| u32::try_from(self.ceiling).unwrap_or(u32::MAX)),
                },
            );
        }
        Ok(GroupSnapshot {
            group_id: group,
            inputs,
            coverage: GroupCoverage {
                through: ended.through,
                members: ended.members,
                sources,
            },
        })
    }

    /// Read the inputs and results of `readable` executions of `group`
    /// timed before `cutoff`.
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
        cutoff: DateTime<Utc>,
        readable: &BTreeSet<ExecutionId>,
        selected: &BTreeMap<ExecutionId, Option<VerificationResultId>>,
    ) -> Result<Payloads, GroupPollError> {
        let mut payloads = Payloads::default();
        let needs = |input: bool| {
            self.sources
                .slots
                .iter()
                .any(|slot| matches!(slot.field, SourceField::Input(_)) == input)
        };
        let select = |columns: &str, table: &str, at: &str| {
            format!(
                "SELECT {columns}, {at} FROM vala.verification.{table} WHERE {} \
                 AND group_id = {} AND {at} < {} LIMIT {}",
                self.sources.eligible,
                ObservationWindow::text(group.as_str()),
                ObservationWindow::instant(cutoff),
                ROWS_PER_GROUP + 1
            )
        };
        if needs(true) {
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
                    if readable.contains(&execution)
                        && earliest.get(&execution).is_none_or(|known| at < *known)
                    {
                        earliest.insert(execution, at);
                        payloads
                            .inputs
                            .insert(execution, variant(&batch, "inputs", row)?);
                    }
                }
            }
        }
        if needs(false) && self.results {
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
                    if readable.contains(&execution) && authoritative {
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

    /// Both source surfaces as one `(surface, group_id, card_uid,
    /// verifier_card_uid, execution_id, at)` row set, each restricted by
    /// `filter` of its time column; inputs alone without a results table.
    fn surfaces(&self, filter: impl Fn(&str) -> String) -> String {
        let inputs = format!(
            "SELECT 'i' AS surface, group_id, card_uid, verifier_card_uid, execution_id, \
             recorded_at AS at FROM vala.verification.execution_inputs WHERE {} AND {}",
            self.sources.eligible,
            filter("recorded_at"),
        );
        if !self.results {
            return inputs;
        }
        format!(
            "{inputs} UNION ALL SELECT 'r' AS surface, group_id, card_uid, verifier_card_uid, \
             execution_id, ended_at AS at FROM vala.verification.results WHERE {} AND {}",
            self.sources.eligible,
            filter("ended_at"),
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
        let any = tuples
            .iter()
            .map(|(card, verifier)| {
                format!(
                    "(card_uid = {} AND verifier_card_uid = {})",
                    ObservationWindow::text(card),
                    ObservationWindow::text(verifier)
                )
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        let eligible = format!(
            "owner_card_uid = {} AND group_id IS NOT NULL AND ({any})",
            ObservationWindow::text(owner)
        );
        Ok(Self {
            slots,
            tuples,
            eligible,
        })
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
                let card = text(batch, "card_uid", row)?;
                let verifier = text(batch, "verifier_card_uid", row)?;
                let Some(tuple) = self.tuples.iter().position(|(c, v)| {
                    card.as_deref() == Some(c.as_str()) && verifier.as_deref() == Some(v.as_str())
                }) else {
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
    /// Rows at or after this instant are past finality and never count.
    cutoff: DateTime<Utc>,
    /// Newest eligible row time.
    through: DateTime<Utc>,
    /// Distinct eligible `(surface, execution)` rows.
    members: u64,
}

impl Ended {
    /// Whether the group of `rows` ended at `clock` under `settings`.
    ///
    /// The first row anchors max_age and finality; rows at or after first +
    /// finality are dropped; the group ended when its newest eligible row is
    /// at least `idle` old or its first is at least `max_age` old.
    fn judge(
        rows: &[SourceRow],
        settings: &GroupEndSettings,
        clock: DateTime<Utc>,
    ) -> Option<Self> {
        let first = rows.iter().map(|row| row.at).min()?;
        let cutoff = first + span(&settings.finality);
        let eligible = rows.iter().filter(|row| row.at < cutoff);
        let through = eligible.clone().map(|row| row.at).max()?;
        let members = eligible
            .map(|row| (row.result, row.execution_id))
            .collect::<BTreeSet<_>>()
            .len();
        (through + span(&settings.idle) <= clock || first + span(&settings.max_age) <= clock)
            .then_some(Self {
                cutoff,
                through,
                members: u64::try_from(members).unwrap_or(u64::MAX),
            })
    }

    /// The eligible executions of `tuple` in list order: by earliest input
    /// time, or earliest result time for a result-only execution, then
    /// execution ID.
    fn executions(&self, rows: &[SourceRow], tuple: usize) -> Vec<ExecutionId> {
        let mut order: BTreeMap<ExecutionId, (bool, DateTime<Utc>)> = BTreeMap::new();
        for row in rows
            .iter()
            .filter(|row| row.tuple == tuple && row.at < self.cutoff)
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

/// `duration` as a chrono span.
fn span(duration: &TriggerDuration) -> TimeDelta {
    TimeDelta::seconds(i64::try_from(duration.seconds()).unwrap_or(i64::MAX))
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
        assert_eq!(Ended::judge(&rows, &settings(), at(41)), None);
        let ended = Ended::judge(&rows, &settings(), at(42)).expect("idle reached");
        assert_eq!(
            ended.through,
            at(12),
            "rows at first + finality never count"
        );
        assert_eq!(ended.members, 6);
        assert_eq!(ended.executions(&rows, 0), [c, a, b, d]);
        assert!(ended.executions(&rows, 1).is_empty());

        let active = [row(false, a, 0), row(false, b, 50)];
        assert!(Ended::judge(&active, &settings(), at(79)).is_none());
        assert!(Ended::judge(&active, &settings(), at(80)).is_some());
        let busy = [row(false, a, 0), row(false, b, 55), row(false, c, 59)];
        assert!(Ended::judge(&busy, &settings(), at(88)).is_none());
        let aged = [row(false, a, 0), row(false, b, 59), row(false, c, 99)];
        assert!(
            Ended::judge(&aged, &settings(), at(89)).is_some(),
            "a row past finality does not postpone idle"
        );
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
