//! Turns one expired pending group of a `group_end` binding into a group
//! execution.
//!
//! [`GroupPoller`] serves one pending group claimed by
//! [`VerifierExecutionQueue::claim_due_group`]. One Bifrost statement reads
//! the identities of every input and result row of the group for the
//! binding's exact source tuples admitted before the group's PostgreSQL due
//! time, the exclusive cutoff of its cumulative history. Inputs deduplicate
//! by execution and results by result ID at their earliest ingest; each
//! execution's selected result is its latest visible one by
//! `(wyrd_ingested_at, result_id)`. Per source slot one list holds every
//! source execution with a selected input or result, ordered by the group
//! Verifier's pinned `order_by` key when it declares one, then by the input's
//! server-recorded `recorded_at`, then execution ID. Ingest time never orders
//! a list: Scribe stamps it once per batch and promises no cross-batch order.
//! Inputs are staged before results and outputs before their summaries, so a
//! visible result without its input or output means the read caught rows
//! mid-publication; that read selects nothing and the group is retried. The
//! selection is recorded once, as the group execution's own input record
//! ([`GroupSelector`]), and resolved into values only when the execution
//! runs; PostgreSQL keeps only its content digest, which makes an unchanged
//! re-read enqueue nothing, whatever its cutoff. Every row time is Bifrost's
//! managed `wyrd_ingested_at`, stamped from PostgreSQL. Every read runs as
//! the tenant's internal read principal ([`SystemReadAuthority`]), granted
//! only the three verification tables and serving the binding owner, so no
//! owner grant can block it; the owner filter stays in the statement.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use arrow::array::{Array, BooleanArray, RecordBatch, TimestampMicrosecondArray};
use arrow::util::display::array_value_to_string;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_runtime::outbox::{OutboxFull, Reservation};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::card::verifier::{OrderBy, PathSegment, SourceText};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ExecutionId, IdError, SlotName, VerificationResultId};
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalOutcome};
use wyrd_spec::verification::GroupEndSettings;
use wyrd_spec::verification::execution::InputDeclaration;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::{
    DueGroup, ExecutionAuthority, GroupSelection, GroupedAdmission, PinnedGroup, Prepared,
    VerifierExecutionQueue,
};

use super::authority::SystemReadAuthority;
use super::drift::ObservationWindow;
use super::inputs::{
    GroupSelector, InputResolver, SelectionRange, SlotSelection, SourceRef, TABLE_NOT_FOUND,
};
use super::results::{InputRecord, ResultPayloadBuilder, ResultRow, ResultRun};
use super::submissions::SubmissionSink;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::scribe_outbox::ScribeSink;
use crate::state::AppState;

/// Why one pending group was not handed off.
#[derive(Debug, thiserror::Error)]
pub enum GroupPollError {
    /// A Postgres statement failed; the caller's transaction is unusable.
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    /// The internal read authority could not be built, or a Bifrost read
    /// failed, was refused, was degraded, or returned malformed rows; the
    /// caller rolls back so the group stays pending.
    #[error("group snapshot read failed: {0}")]
    Read(String),
    /// The read saw results of these source executions without their inputs
    /// or outputs, which are staged first, so it caught rows
    /// mid-publication; nothing was selected and a later read sees them.
    #[error("group snapshot saw results without inputs or outputs: {0:?}")]
    Incomplete(Vec<ExecutionId>),
}

impl From<WyrdError> for GroupPollError {
    /// Every query failure, a refusal of the internal reader included, is a
    /// retryable [`Self::Read`] naming its stable code.
    fn from(error: WyrdError) -> Self {
        Self::Read(format!("{}: {error}", error.code()))
    }
}

/// Owner of the Bifrost read that selects one pending group's sources.
pub struct GroupPoller {
    /// Server state owning the Bifrost catalog and query service.
    state: AppState,
    /// Deadline of one Bifrost statement.
    query_timeout: Duration,
}

impl GroupPoller {
    /// Build a poller reading through `state`, bounding every statement by
    /// `query_timeout`.
    #[must_use]
    pub const fn new(state: AppState, query_timeout: Duration) -> Self {
        Self {
            state,
            query_timeout,
        }
    }

    /// Reserve the Scribe outbox slot a handoff's selector record is staged
    /// on and the queue slot that admits it once Scribe acknowledges it,
    /// before the pending group is claimed.
    ///
    /// # Errors
    /// Returns [`OutboxFull`] when either reserved share is full, so the
    /// group stays pending.
    pub fn reserve(
        &self,
    ) -> Result<(Reservation<ScribeSink>, Reservation<SubmissionSink>), OutboxFull> {
        Ok((
            self.state.bifrost.scribe_outbox().reserve(1)?,
            self.state.verification.submissions().reserve(1)?,
        ))
    }

    /// Select one snapshot of the claimed pending group `due` and prepare its
    /// group execution under the owner's accepted `authority`, writing
    /// nothing.
    ///
    /// The statement runs as the tenant's `system` principal holding
    /// `bifrost_query:read` on exactly the registered verification inputs,
    /// results, and outputs tables, recorded as serving the owner; the
    /// owner's own grants play no part in the read. It reads rows admitted
    /// before `due.due_at`. A tenant without a results table is read from
    /// inputs alone. An unchanged snapshot answers with its existing
    /// execution; a new one is a candidate that keeps `authority` for its
    /// work, consumes `due`'s pending generation when admitted, and comes
    /// with its selector input record for the caller to stage. A read
    /// showing a result without its execution's input or output selects
    /// nothing.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Sql`] when a Postgres statement fails and
    /// [`GroupPollError::Read`] when the read authority cannot be built, the
    /// read fails, is refused or degraded, a stored declaration or row is
    /// malformed, or the selector record cannot be encoded, and
    /// [`GroupPollError::Incomplete`] naming the executions whose results
    /// were visible without their inputs or outputs.
    ///
    /// The `verification.group.snapshot` span carries the tenant, binding,
    /// group, rows read, and source executions selected; the Bifrost read is
    /// timed into the group snapshot histogram by outcome and the selected
    /// source count into the snapshot sources histogram.
    #[tracing::instrument(
        name = "verification.group.snapshot",
        skip_all,
        fields(
            tenant = %conn.data_tenant_id(),
            binding_id = %due.binding_id,
            group_id = %due.group_id,
            rows = tracing::field::Empty,
            sources = tracing::field::Empty,
        )
    )]
    pub async fn poll(
        &self,
        conn: &mut TenantConn<'_>,
        queue: &VerifierExecutionQueue,
        due: &DueGroup,
        authority: &ExecutionAuthority,
        system: PrincipalId,
    ) -> Result<Prepared<(GroupedAdmission, ResultRow)>, GroupPollError> {
        let tables = ["execution_inputs", "results", "execution_outputs"]
            .map(|name| TableRef::new(BifrostNamespace::Verification, name));
        let read =
            SystemReadAuthority::resolve(&self.state, conn.data_tenant_id(), Some(system), &tables)
                .await
                .map_err(|error| GroupPollError::Read(error.to_string()))?
                .serving(authority.authority());
        let deadline_ms = i64::try_from(self.query_timeout.as_millis()).unwrap_or(i64::MAX);
        let reader = Reader {
            query: ScheduledQueryCaller::new(
                self.state.clone(),
                read.into_context(),
                CancellationToken::new(),
            ),
            deadline_ms,
        };
        let sources = Sources::new(&due.settings, &due.owner_card_uid.to_string())?;
        let group = format!(
            "group_id = {} AND wyrd_ingested_at < {}",
            ObservationWindow::text(due.group_id.as_str()),
            ObservationWindow::instant(due.due_at),
        );
        let started = Instant::now();
        let batches = match reader.run(&sources.read(&group, true)).await {
            Err(GroupPollError::Read(error)) if error.contains(TABLE_NOT_FOUND) => {
                match reader.run(&sources.read(&group, false)).await {
                    Err(GroupPollError::Read(error)) if error.contains(TABLE_NOT_FOUND) => {
                        Ok(Vec::new())
                    }
                    batches => batches,
                }
            }
            batches => batches,
        };
        metrics::histogram!(
            crate::app::metrics::VERIFICATION_GROUP_SNAPSHOT_SECONDS,
            "outcome" => if batches.is_ok() { "ok" } else { "error" }
        )
        .record(started.elapsed().as_secs_f64());
        let batches = batches?;
        let span = tracing::Span::current();
        span.record(
            "rows",
            batches.iter().map(RecordBatch::num_rows).sum::<usize>(),
        );
        let visible = sources.decode(&batches)?;
        let missing = visible.incomplete();
        if !missing.is_empty() {
            return Err(GroupPollError::Incomplete(missing));
        }
        span.record("sources", visible.inputs.len());
        metrics::histogram!(crate::app::metrics::VERIFICATION_GROUP_SNAPSHOT_SOURCES)
            .record(u32::try_from(visible.inputs.len()).map_or(f64::from(u32::MAX), f64::from));
        let resolver = InputResolver::new(&reader.query, Some(deadline_ms));
        let selector = visible.select(&sources, due, &resolver).await?;
        let pinned = PinnedGroup::new(
            due.binding_id,
            due.group_id.clone(),
            selector
                .slots
                .iter()
                .map(|(slot, selection)| {
                    let executions = selection
                        .executions
                        .iter()
                        .map(|source| (source.execution_id, source.result_id))
                        .collect();
                    let reference = selection.reference.clone();
                    (
                        slot.clone(),
                        GroupSelection {
                            reference,
                            executions,
                        },
                    )
                })
                .collect(),
        )?;
        Ok(
            match queue
                .prepare_group(
                    conn,
                    due.binding_id,
                    &pinned,
                    authority,
                    Some(due.generation),
                )
                .await?
            {
                Prepared::Candidate(admission) => {
                    let row = Self::record(due, admission.execution_id, &selector)?;
                    Prepared::Candidate((admission, row))
                }
                Prepared::Existing(execution_id) => Prepared::Existing(execution_id),
                Prepared::Refused(refusal) => Prepared::Refused(refusal),
            },
        )
    }

    /// The `origin = group` input record of new execution `execution_id`,
    /// holding `selector`.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when the selector cannot be encoded.
    fn record(
        due: &DueGroup,
        execution_id: ExecutionId,
        selector: &GroupSelector,
    ) -> Result<ResultRow, GroupPollError> {
        let now = Utc::now();
        ResultPayloadBuilder::new(
            ResultRun {
                execution_id,
                run_id: None,
                // The inputs row records no Verifier version.
                verifier_version: "",
                card_uid: &due.card_uid,
                owner_card_uid: Some(&due.owner_card_uid),
                group_id: Some(&due.group_id),
                deployment_environment: None,
                binding_id: Some(due.binding_id),
                trigger: None,
                window: None,
            },
            &due.verifier_card_uid,
            now,
            now,
        )
        .inputs(InputRecord::Group(selector))
        .map_err(|error| GroupPollError::Read(error.to_string()))
    }
}

/// The internal reader of one pending group.
struct Reader {
    /// The internal read principal every statement runs through.
    query: ScheduledQueryCaller,
    /// Deadline of one statement, in milliseconds.
    deadline_ms: i64,
}

impl Reader {
    /// Run `sql` and collect its batches.
    ///
    /// # Errors
    /// Returns the query's error as [`GroupPollError::from`] classifies it,
    /// or [`GroupPollError::Read`] when the terminal is degraded, since a snapshot missing a live source is not
    /// the visible evidence.
    async fn run(&self, sql: &str) -> Result<Vec<RecordBatch>, GroupPollError> {
        let mut batches = Vec::new();
        let outcome = self
            .query
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
            .await
            .map_err(GroupPollError::from)?;
        if outcome.terminal.outcome != QueryTerminalOutcome::Success {
            return Err(GroupPollError::Read(format!(
                "terminal {:?}",
                outcome.terminal.outcome
            )));
        }
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
    /// The pinned input-key `order_by`, when the group Verifier declares
    /// one; `recorded_at` is the default order and needs none.
    order: Option<Order>,
}

/// A group Verifier's declared input-key list order: a key read from one
/// source slot's values.
struct Order {
    /// Index into [`Sources::tuples`] of the ordered source binding.
    tuple: usize,
    /// The ordered slot and its authored source, whose values hold the key.
    slot: (SlotName, SourceText),
    /// Path from that slot's value to the key.
    path: Vec<PathSegment>,
}

/// One source execution and its selected result, in list position.
type Listed = (ExecutionId, Option<VerificationResultId>);

impl Order {
    /// Stably sort `list` by each execution's key in `values`, aligned to
    /// `list`, so equal keys keep their default order.
    fn sort(&self, list: &mut Vec<Listed>, values: Vec<Value>) {
        let mut keyed: Vec<(Listed, Value)> = list.drain(..).zip(values).collect();
        keyed.sort_by(|(_, left), (_, right)| Self::compare(self.key(left), self.key(right)));
        list.extend(keyed.into_iter().map(|(listed, _)| listed));
    }

    /// The key `value` sorts by: the value at [`Self::path`], absent when
    /// the path does not resolve.
    fn key<'v>(&self, value: &'v Value) -> Option<&'v Value> {
        self.path
            .iter()
            .try_fold(value, |value, segment| match segment {
                PathSegment::Field(name) => value.get(name),
                PathSegment::Index(index) => value.get(index),
                PathSegment::Wildcard => None,
            })
    }
    /// Compares two keys: numbers ascending, then strings ascending, then
    /// any other or missing key, which compare equal.
    fn compare(left: Option<&Value>, right: Option<&Value>) -> std::cmp::Ordering {
        /// The class of a key and its number, for cross-class ordering.
        fn class(key: Option<&Value>) -> (u8, f64) {
            match key {
                Some(Value::Number(number)) => (0, number.as_f64().unwrap_or(f64::NAN)),
                Some(Value::String(_)) => (1, 0.0),
                _ => (2, 0.0),
            }
        }
        let ((left_class, left_number), (right_class, right_number)) = (class(left), class(right));
        left_class
            .cmp(&right_class)
            .then(left_number.total_cmp(&right_number))
            .then_with(|| match (left, right) {
                (Some(Value::String(left)), Some(Value::String(right))) => left.cmp(right),
                _ => std::cmp::Ordering::Equal,
            })
    }
}

/// One source slot: what it reads, from which source tuple.
struct SourceSlot {
    /// The group Verifier slot it fills.
    name: SlotName,
    /// The authored declaration, recorded in the selector.
    reference: SourceText,
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
    /// parses or the pinned `order_by` names no source slot.
    fn new(settings: &GroupEndSettings, owner: &str) -> Result<Self, GroupPollError> {
        let mut tuples: Vec<(String, String)> = Vec::new();
        let mut slots = Vec::new();
        for (name, source) in &settings.sources {
            source.reference.parse().map_err(malformed)?;
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
                tuple,
            });
        }
        let any = tuples
            .iter()
            .map(|(card, verifier)| {
                format!(
                    "(subject_card_uid = {} AND verifier_card_uid = {})",
                    ObservationWindow::text(card),
                    ObservationWindow::text(verifier)
                )
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        let eligible = format!(
            "workload_card_uid = {} AND ({})",
            ObservationWindow::text(owner),
            if any.is_empty() { "false" } else { &any }
        );
        let order = match &settings.order_by {
            Some(OrderBy::Input(order)) => Some(order),
            Some(OrderBy::RecordedAt) | None => None,
        };
        let order = order
            .map(|order| {
                slots
                    .iter()
                    .find(|slot| Some(&slot.name) == order.slot())
                    .map(|slot| Order {
                        tuple: slot.tuple,
                        slot: (slot.name.clone(), slot.reference.clone()),
                        path: order.path.clone(),
                    })
                    .ok_or_else(|| malformed(format!("order_by {order} names no source slot")))
            })
            .transpose()?;
        Ok(Self {
            slots,
            tuples,
            eligible,
            order,
        })
    }

    /// The one statement reading the identities of every source row
    /// matching `group`: `(surface, subject_card_uid, verifier_card_uid,
    /// execution_id, result_id, at, recorded, complete)`, inputs alone
    /// without a `results` table. `at` is the managed `wyrd_ingested_at`;
    /// `recorded` is the server-recorded input time, or a result's execution
    /// start; `complete` is whether a result's output record is visible. No
    /// payload is read: values are resolved when the execution runs.
    // ponytail: reads every row identity of the group's history, growing
    // with its size; page it if the group-end benchmark shows one read matters.
    fn read(&self, group: &str, results: bool) -> String {
        let inputs = format!(
            "SELECT 'i' AS surface, subject_card_uid, verifier_card_uid, execution_id, \
             CAST(NULL AS VARCHAR) AS result_id, wyrd_ingested_at AS at, \
             recorded_at AS recorded, true AS complete \
             FROM bifrost.verification.execution_inputs WHERE {} AND {group}",
            self.eligible,
        );
        if !results {
            return inputs;
        }
        format!(
            "{inputs} UNION ALL SELECT 'r' AS surface, subject_card_uid, verifier_card_uid, \
             execution_id, result_id, wyrd_ingested_at AS at, started_at AS recorded, \
             result_id IN (SELECT result_id FROM bifrost.verification.execution_outputs) \
             AS complete \
             FROM bifrost.verification.results WHERE {} AND {group}",
            self.eligible,
        )
    }

    /// The source tuple of `row` in `batch`, `None` for a row of no tuple.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a tuple column is missing or
    /// malformed.
    fn tuple(&self, batch: &RecordBatch, row: usize) -> Result<Option<usize>, GroupPollError> {
        let card = text(batch, "subject_card_uid", row)?;
        let verifier = text(batch, "verifier_card_uid", row)?;
        Ok(self.tuples.iter().position(|(c, v)| {
            card.as_deref() == Some(c.as_str()) && verifier.as_deref() == Some(v.as_str())
        }))
    }

    /// Decode the rows of one group read into its logical rows; rows of no
    /// source tuple are dropped.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a column is missing or a value
    /// is malformed.
    fn decode(&self, batches: &[RecordBatch]) -> Result<Visible, GroupPollError> {
        let mut visible = Visible::default();
        for batch in batches {
            let times = times(batch, "at")?;
            let recorded = self::times(batch, "recorded")?;
            let complete: &BooleanArray = column(batch, "complete")?
                .as_any()
                .downcast_ref()
                .ok_or_else(|| GroupPollError::Read("complete is not a boolean".to_owned()))?;
            for row in 0..batch.num_rows() {
                let Some(tuple) = self.tuple(batch, row)? else {
                    continue;
                };
                let execution = id(batch, row)?;
                let at = instant(times, row)?;
                let recorded = instant(recorded, row)?;
                if text(batch, "surface", row)?.as_deref() == Some("r") {
                    let result = text(batch, "result_id", row)?
                        .ok_or_else(|| GroupPollError::Read("null result_id".to_owned()))?
                        .parse()
                        .map_err(|error: IdError| malformed(error))?;
                    visible.result(result, tuple, execution, at, recorded);
                    if !(complete.is_valid(row) && complete.value(row)) {
                        visible.unfinished.insert(execution);
                    }
                } else {
                    visible.input(tuple, execution, at, recorded);
                }
            }
        }
        Ok(visible)
    }
}

/// One visible row at its earliest ingest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Earliest {
    /// Its earliest managed `wyrd_ingested_at`.
    at: DateTime<Utc>,
    /// That row's server-recorded input time, or its result's execution
    /// start, which orders lists.
    recorded: DateTime<Utc>,
}

impl Earliest {
    /// Insert `row` under `key`, or keep the earlier-ingested row.
    fn keep<K: Ord, T>(rows: &mut BTreeMap<K, (T, Self)>, key: K, owner: T, row: Self) {
        match rows.entry(key) {
            Entry::Vacant(vacant) => {
                vacant.insert((owner, row));
            }
            Entry::Occupied(mut known) if row.at < known.get().1.at => {
                known.get_mut().1 = row;
            }
            Entry::Occupied(_) => {}
        }
    }
}

/// One group's logical visible rows: inputs by execution and results by
/// result ID, each at its earliest ingest.
#[derive(Debug, Default)]
struct Visible {
    /// Each source execution's tuple and earliest input.
    inputs: BTreeMap<ExecutionId, (usize, Earliest)>,
    /// Each result's tuple and execution, and its earliest row.
    results: BTreeMap<VerificationResultId, ((usize, ExecutionId), Earliest)>,
    /// Executions with a visible result whose output record is not visible.
    unfinished: BTreeSet<ExecutionId>,
}

impl Visible {
    /// Record one input row of `execution` under `tuple`, keyed by its
    /// source execution ID.
    ///
    /// An execution can appear in several Bifrost rows (replays, later
    /// files); only the earliest `wyrd_ingested_at` is kept, so the list
    /// order is stable across re-reads.
    fn input(
        &mut self,
        tuple: usize,
        execution: ExecutionId,
        at: DateTime<Utc>,
        recorded: DateTime<Utc>,
    ) {
        Earliest::keep(
            &mut self.inputs,
            execution,
            tuple,
            Earliest { at, recorded },
        );
    }

    /// Record one result row of `execution` under `tuple`, keyed by its
    /// `result` ID rather than the execution.
    ///
    /// An execution may own several results (later revisions); each result
    /// is kept separately so [`Self::executions`] can select the greatest,
    /// and each keeps only its earliest-ingested row like [`Self::input`].
    fn result(
        &mut self,
        result: VerificationResultId,
        tuple: usize,
        execution: ExecutionId,
        at: DateTime<Utc>,
        recorded: DateTime<Utc>,
    ) {
        let row = Earliest { at, recorded };
        Earliest::keep(&mut self.results, result, (tuple, execution), row);
    }

    /// Source executions with a visible result but no visible input or
    /// output, in ID order. Inputs are staged before results and outputs
    /// before summaries, so any such execution means the read missed rows
    /// still being published.
    fn incomplete(&self) -> Vec<ExecutionId> {
        let mut missing: Vec<_> = self
            .results
            .values()
            .map(|((_, execution), _)| *execution)
            .filter(|execution| !self.inputs.contains_key(execution))
            .chain(self.unfinished.iter().copied())
            .collect();
        missing.sort_unstable();
        missing.dedup();
        missing
    }

    /// Each execution of `tuple` in default list order with its selected
    /// result: by its input's recorded time, else its earliest result's
    /// execution start, then execution ID. The selected result is the
    /// greatest `(at, result_id)`.
    fn executions(&self, tuple: usize) -> Vec<Listed> {
        /// An execution's two optional ordering anchors: its input's recorded
        /// time, then its earliest result's execution start. Either is absent
        /// when the snapshot holds no such row; the input anchor wins.
        type Firsts = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);
        let mut order: BTreeMap<ExecutionId, Firsts> = BTreeMap::new();
        let mut selected: BTreeMap<ExecutionId, (DateTime<Utc>, VerificationResultId)> =
            BTreeMap::new();
        for (execution, (_, input)) in self.inputs.iter().filter(|(_, (t, _))| *t == tuple) {
            order.entry(*execution).or_default().0 = Some(input.recorded);
        }
        for (result, ((_, execution), row)) in
            self.results.iter().filter(|(_, ((t, _), _))| *t == tuple)
        {
            let first = &mut order.entry(*execution).or_default().1;
            *first = Some(first.map_or(row.recorded, |known| known.min(row.recorded)));
            let pick = (row.at, *result);
            selected
                .entry(*execution)
                .and_modify(|known| *known = (*known).max(pick))
                .or_insert(pick);
        }
        let mut list: Vec<_> = order
            .into_iter()
            .map(|(execution, (input, result))| (input.or(result), execution))
            .collect();
        list.sort_unstable();
        list.into_iter()
            .map(|(_, execution)| (execution, selected.get(&execution).map(|(_, id)| *id)))
            .collect()
    }

    /// The earliest admission time of the selected rows: every input and
    /// each execution's selected result; `None` when nothing is selected.
    fn start(&self, lists: &[Vec<Listed>]) -> Option<DateTime<Utc>> {
        let inputs = self.inputs.values().map(|(_, row)| row.at);
        let results = lists
            .iter()
            .flatten()
            .filter_map(|(_, result)| Some(self.results.get(result.as_ref()?)?.1.at));
        inputs.chain(results).min()
    }

    /// The selector of `due` from these rows: per slot one list aligned to
    /// its source's executions over `[start, due_at)`. The ordered tuple's
    /// list is stably re-sorted by the declared key, its values resolved
    /// through `resolver` as the execution will resolve them, so ties keep
    /// the default order.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Read`] when a value read fails and
    /// [`GroupPollError::Incomplete`] when an ordered source's records are
    /// not visible yet.
    async fn select(
        &self,
        sources: &Sources,
        due: &DueGroup,
        resolver: &InputResolver<'_>,
    ) -> Result<GroupSelector, GroupPollError> {
        let mut lists: Vec<_> = (0..sources.tuples.len())
            .map(|tuple| self.executions(tuple))
            .collect();
        if let Some(order) = &sources.order {
            let list = &mut lists[order.tuple];
            let (slot, reference) = &order.slot;
            let probe = BTreeMap::from([(
                slot.clone(),
                SlotSelection {
                    reference: reference.clone(),
                    executions: list.iter().map(|listed| source(*listed)).collect(),
                },
            )]);
            let Some(mut resolved) = resolver.slots(&probe).await? else {
                return Err(GroupPollError::Incomplete(
                    list.iter().map(|(execution, _)| *execution).collect(),
                ));
            };
            let values = match resolved.remove(slot) {
                Some(InputDeclaration::Json {
                    value: Value::Array(values),
                }) => values,
                _ => Vec::new(),
            };
            order.sort(list, values);
        }
        let end = due.due_at;
        Ok(GroupSelector {
            range: SelectionRange {
                start: self.start(&lists).unwrap_or(end),
                end,
            },
            slots: sources
                .slots
                .iter()
                .map(|slot| {
                    let executions = lists[slot.tuple].iter().map(|listed| source(*listed));
                    (
                        slot.name.clone(),
                        SlotSelection {
                            reference: slot.reference.clone(),
                            executions: executions.collect(),
                        },
                    )
                })
                .collect(),
        })
    }
}

/// `listed` as a selector reference.
const fn source((execution_id, result_id): Listed) -> SourceRef {
    SourceRef {
        execution_id,
        result_id,
    }
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

/// Pure selection, ordering, and value-mapping checks of the poller; its
/// Bifrost read is proved by the grouped user journeys.
#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone as _};
    use serde_json::json;

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

    /// Replayed rows deduplicate at their earliest ingest; the latest
    /// visible result is selected; lists order by input time, else result
    /// time, then execution ID; executions of another tuple are excluded.
    ///
    /// # Panics
    /// Panics when a row selects or orders otherwise.
    #[test]
    fn visible_rows_select_and_order() {
        let mut ids = [(); 5].map(|()| ExecutionId::new_v7());
        ids.sort_unstable();
        let [a, b, c, d, other] = ids;
        let mut results = [(); 3].map(|()| VerificationResultId::new_v7());
        results.sort_unstable();
        let [early, late, tied] = results;
        let mut visible = Visible::default();
        visible.input(0, b, at(10), at(10));
        visible.input(0, b, at(9), at(9));
        visible.result(early, 0, b, at(12), at(12));
        visible.result(late, 0, b, at(15), at(15));
        visible.result(late, 0, b, at(20), at(20));
        visible.input(0, a, at(10), at(10));
        visible.result(tied, 0, c, at(5), at(5));
        visible.input(0, d, at(10), at(10));
        visible.input(1, other, at(1), at(1));
        assert_eq!(
            visible.executions(0),
            [(c, Some(tied)), (b, Some(late)), (a, None), (d, None)]
        );
        assert_eq!(visible.inputs[&b].1.at, at(9));
        assert_eq!(visible.results[&late].1.at, at(15));
        assert_eq!(visible.start(&[visible.executions(0)]), Some(at(1)));
        visible.unfinished.insert(d);
        assert_eq!(visible.incomplete(), [c, d]);
        assert_eq!(visible.executions(1), [(other, None)]);
    }

    /// Lists order by recorded input time, not ingest time: rows of one
    /// batch share an ingest time, and a later batch may be recorded first.
    ///
    /// # Panics
    /// Panics when a list orders by ingest time or execution ID first.
    #[test]
    fn recorded_time_orders_lists() {
        let mut ids = [(); 3].map(|()| ExecutionId::new_v7());
        ids.sort_unstable();
        let [first, second, third] = ids;
        let mut visible = Visible::default();
        visible.input(0, third, at(10), at(1));
        visible.input(0, first, at(10), at(3));
        visible.input(0, second, at(5), at(2));
        assert_eq!(
            visible.executions(0),
            [(third, None), (second, None), (first, None)]
        );
    }

    /// A declared input key orders numbers, then strings, then missing or
    /// other keys; equal keys keep their default order.
    ///
    /// # Panics
    /// Panics when the declared order sorts otherwise or is unstable.
    #[test]
    fn declared_key_orders_lists() {
        let ids = [(); 7].map(|()| ExecutionId::new_v7());
        let turn = |value: Value| json!({"turn": value});
        let values = vec![
            turn(json!("b")),
            Value::Null,
            turn(json!(10)),
            turn(json!(2)),
            turn(json!("a")),
            turn(json!(2)),
            turn(json!(true)),
        ];
        let order = Order {
            tuple: 0,
            slot: (
                "turns".parse().expect("slot name"),
                serde_json::from_value(json!("verifiers.chat.inputs.turn")).expect("source text"),
            ),
            path: vec![PathSegment::Field("turn".to_owned())],
        };
        let mut list: Vec<_> = ids.iter().map(|id| (*id, None)).collect();
        order.sort(&mut list, values);
        let [b, missing, ten, two, a, two_again, other] = ids;
        assert_eq!(
            list.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [two, two_again, ten, a, b, missing, other]
        );
    }
}
