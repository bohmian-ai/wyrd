//! Turns one expired pending group of a `group_end` binding into a group
//! execution.
//!
//! [`GroupPoller`] serves one pending group claimed by
//! [`VerifierExecutionQueue::claim_due_group`]. One Bifrost statement reads
//! every visible input and result row of the group for the binding's exact
//! source tuples, with the values its slots read. Inputs deduplicate by
//! execution and results by result ID at their earliest ingest; each
//! execution's selected result is its latest visible one by
//! `(wyrd_ingested_at, result_id)`. Per source slot one list holds every
//! source execution with a visible input or result, ordered by the group
//! Verifier's pinned `order_by` key when it declares one, then by the input's
//! server-recorded `recorded_at`, then execution ID; an absent value is null.
//! Ingest time never orders a list: Scribe stamps it once per batch and
//! promises no cross-batch order. Inputs are staged before results, so a visible
//! result without its input means the read caught rows mid-publication; that
//! read pins nothing and the group is retried. The pinned snapshot's content digest makes an
//! unchanged re-read enqueue nothing. Every row time is Bifrost's managed
//! `wyrd_ingested_at`, stamped from PostgreSQL. Every read runs as the
//! tenant's internal read principal ([`SystemReadAuthority`]), granted only
//! the two verification tables and serving the binding owner, so no owner
//! grant can block it; the owner filter stays in the statement.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::{Duration, Instant};

use arrow::array::{Array, RecordBatch, TimestampMicrosecondArray};
use arrow::util::display::array_value_to_string;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::card::verifier::{OrderBy, PathSegment, SourceField, SourceText};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ExecutionId, IdError, SlotName, VerificationResultId};
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalOutcome};
use wyrd_spec::verification::GroupEndSettings;
use wyrd_spec::verification::execution::InputDeclaration;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::{
    DueGroup, EnqueueOutcome, ExecutionAuthority, GroupSelection, PinnedGroup,
    VerifierExecutionQueue,
};

use super::authority::SystemReadAuthority;
use super::drift::ObservationWindow;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::state::AppState;

/// Error code of a read of a Bifrost table the tenant never created.
const TABLE_NOT_FOUND: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

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
    /// The read saw results of these source executions without their
    /// inputs, which are staged first, so it caught rows mid-publication;
    /// nothing was pinned and a later read sees them.
    #[error("group snapshot saw results without inputs: {0:?}")]
    Incomplete(Vec<ExecutionId>),
}

impl From<WyrdError> for GroupPollError {
    /// Every query failure, a refusal of the internal reader included, is a
    /// retryable [`Self::Read`] naming its stable code.
    fn from(error: WyrdError) -> Self {
        Self::Read(format!("{}: {error}", error.code()))
    }
}

/// Owner of the Bifrost read that pins one pending group's snapshot.
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

    /// Pin one snapshot of the claimed pending group `due` and enqueue its
    /// group execution under the owner's accepted `authority`.
    ///
    /// The statement runs as the tenant's `system` principal holding
    /// `bifrost_query:read` on exactly the registered verification inputs and
    /// results tables, recorded as serving the owner; the owner's own grants
    /// play no part in the read. A tenant without a results table is read
    /// from inputs alone. An unchanged snapshot answers with its existing
    /// execution, and the enqueued execution keeps `authority` for its work.
    /// A read showing a result without its execution's input pins nothing.
    /// The caller consumes the pending row and commits.
    ///
    /// # Errors
    /// Returns [`GroupPollError::Sql`] when a Postgres statement fails and
    /// [`GroupPollError::Read`] when the read authority cannot be built, the
    /// read fails, is refused or degraded, or a stored declaration or row is
    /// malformed, and [`GroupPollError::Incomplete`] naming the executions
    /// whose results were visible without their inputs.
    ///
    /// The `verification.group.snapshot` span carries the tenant, binding,
    /// group, rows read, and source executions pinned; the Bifrost read is
    /// timed into the group snapshot histogram by outcome and the pinned
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
    ) -> Result<EnqueueOutcome, GroupPollError> {
        let tables = ["execution_inputs", "results"]
            .map(|name| TableRef::new(BifrostNamespace::Verification, name));
        let read =
            SystemReadAuthority::resolve(&self.state, conn.data_tenant_id(), Some(system), &tables)
                .await
                .map_err(|error| GroupPollError::Read(error.to_string()))?
                .serving(authority.authority());
        let reader = Reader {
            query: ScheduledQueryCaller::new(
                self.state.clone(),
                read.into_context(),
                CancellationToken::new(),
            ),
            deadline_ms: i64::try_from(self.query_timeout.as_millis()).unwrap_or(i64::MAX),
        };
        let sources = Sources::new(&due.settings, &due.owner_card_uid.to_string())?;
        let group = format!(
            "group_id = {}",
            ObservationWindow::text(due.group_id.as_str())
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
        let missing = visible.missing_inputs();
        if !missing.is_empty() {
            return Err(GroupPollError::Incomplete(missing));
        }
        span.record("sources", visible.inputs.len());
        metrics::histogram!(crate::app::metrics::VERIFICATION_GROUP_SNAPSHOT_SOURCES)
            .record(u32::try_from(visible.inputs.len()).map_or(f64::from(u32::MAX), f64::from));
        let pinned = visible.pin(&sources, due).map_err(GroupPollError::Sql)?;
        Ok(queue
            .enqueue_group(conn, due.binding_id, &pinned, authority)
            .await?)
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
/// source tuple's input values.
struct Order {
    /// Index into [`Sources::tuples`] of the ordered source binding.
    tuple: usize,
    /// The source input the key is read from.
    field: SourceField,
    /// Path from that input's value to the key.
    path: Vec<PathSegment>,
}

impl Order {
    /// Stably sort `list` by each execution's key in `payloads`, so equal
    /// keys keep their default order.
    fn sort(&self, list: &mut [(ExecutionId, Option<VerificationResultId>)], payloads: &Payloads) {
        let values: BTreeMap<ExecutionId, Value> = list
            .iter()
            .map(|(execution, _)| (*execution, payloads.value(*execution, &self.field)))
            .collect();
        list.sort_by(|(left, _), (right, _)| {
            Self::compare(self.key(&values[left]), self.key(&values[right]))
        });
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
    /// The authored declaration, pinned into the snapshot.
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
    /// parses or the pinned `order_by` names no source slot.
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
            "owner_card_uid = {} AND ({})",
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
                        field: slot.field.clone(),
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

    /// The one statement reading every visible source row matching `group`:
    /// `(surface, card_uid, verifier_card_uid, execution_id, result_id, at,
    /// recorded, payload)`, inputs alone without a `results` table. `at` is
    /// the managed `wyrd_ingested_at`; `recorded` is the server-recorded
    /// input time, or a result's execution start; `payload` is the input
    /// declarations or result outcome, read only for a surface some slot
    /// reads.
    // ponytail: reads every row of the group, growing with its size; page it
    // if the group-end benchmark shows one read matters.
    fn read(&self, group: &str, results: bool) -> String {
        let input = |slot: &SourceSlot| matches!(slot.field, SourceField::Input(_));
        let inputs = format!(
            "SELECT 'i' AS surface, card_uid, verifier_card_uid, execution_id, \
             CAST(NULL AS VARCHAR) AS result_id, wyrd_ingested_at AS at, \
             recorded_at AS recorded, CASE WHEN {} THEN inputs END AS payload \
             FROM vala.verification.execution_inputs WHERE {} AND {group}",
            self.slots.iter().any(input),
            self.eligible,
        );
        if !results {
            return inputs;
        }
        format!(
            "{inputs} UNION ALL SELECT 'r' AS surface, card_uid, verifier_card_uid, \
             execution_id, result_id, wyrd_ingested_at AS at, \
             started_at AS recorded, CASE WHEN {} THEN outcome END AS payload \
             FROM vala.verification.results WHERE {} AND {group}",
            self.slots.iter().any(|slot| !input(slot)),
            self.eligible,
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
            let payload = column(batch, "payload")?;
            for row in 0..batch.num_rows() {
                let Some(tuple) = self.tuple(batch, row)? else {
                    continue;
                };
                let execution = id(batch, row)?;
                let at = instant(times, row)?;
                let recorded = instant(recorded, row)?;
                let value = if payload.is_null(row) {
                    Value::Null
                } else {
                    variant(batch, "payload", row)?
                };
                if text(batch, "surface", row)?.as_deref() == Some("r") {
                    let result = text(batch, "result_id", row)?
                        .ok_or_else(|| GroupPollError::Read("null result_id".to_owned()))?
                        .parse()
                        .map_err(|error: IdError| malformed(error))?;
                    visible.result(result, tuple, execution, at, recorded, value);
                } else {
                    visible.input(tuple, execution, at, recorded, value);
                }
            }
        }
        Ok(visible)
    }
}

/// One visible row at its earliest ingest.
#[derive(Debug, Clone, PartialEq)]
struct Earliest {
    /// Its earliest managed `wyrd_ingested_at`.
    at: DateTime<Utc>,
    /// That row's server-recorded input time, or its result's execution
    /// start, which orders lists.
    recorded: DateTime<Utc>,
    /// The payload of that earliest row.
    value: Value,
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
}

impl Visible {
    /// Record one input row of `execution` under `tuple`, keyed by its
    /// source execution ID.
    ///
    /// An execution can appear in several Bifrost rows (replays, later
    /// files); only the payload with the earliest `wyrd_ingested_at` is kept,
    /// so the pinned input and the list order are stable across re-reads.
    fn input(
        &mut self,
        tuple: usize,
        execution: ExecutionId,
        at: DateTime<Utc>,
        recorded: DateTime<Utc>,
        value: Value,
    ) {
        let row = Earliest {
            at,
            recorded,
            value,
        };
        Earliest::keep(&mut self.inputs, execution, tuple, row);
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
        value: Value,
    ) {
        let row = Earliest {
            at,
            recorded,
            value,
        };
        Earliest::keep(&mut self.results, result, (tuple, execution), row);
    }

    /// Source executions with a visible result but no visible input, in ID
    /// order. Inputs are staged before results, so any such execution means
    /// the read missed rows still being published.
    fn missing_inputs(&self) -> Vec<ExecutionId> {
        let mut missing: Vec<_> = self
            .results
            .values()
            .map(|((_, execution), _)| *execution)
            .filter(|execution| !self.inputs.contains_key(execution))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        missing
    }

    /// Each execution of `tuple` in default list order with its selected
    /// result: by its input's recorded time, else its earliest result's
    /// execution start, then execution ID. The selected result is the
    /// greatest `(at, result_id)`.
    fn executions(&self, tuple: usize) -> Vec<(ExecutionId, Option<VerificationResultId>)> {
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

    /// Pin the snapshot of `due` from these rows: per slot one list aligned
    /// to its source's executions, each value null when absent. The ordered
    /// tuple's list is stably re-sorted by the declared key, so ties keep the
    /// default order.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Encode`] when the snapshot cannot be digested.
    fn pin(&self, sources: &Sources, due: &DueGroup) -> Result<PinnedGroup, sqlx::Error> {
        let mut payloads = Payloads {
            inputs: self
                .inputs
                .iter()
                .map(|(execution, (_, row))| (*execution, row.value.clone()))
                .collect(),
            outcomes: BTreeMap::new(),
        };
        let mut lists: Vec<_> = (0..sources.tuples.len())
            .map(|tuple| self.executions(tuple))
            .collect();
        if let Some(order) = &sources.order {
            order.sort(&mut lists[order.tuple], &payloads);
        }
        for (execution, result) in lists.iter().flatten() {
            if let Some(result) = result {
                payloads
                    .outcomes
                    .insert(*execution, self.results[result].1.value.clone());
            }
        }
        let mut inputs = BTreeMap::new();
        let mut selections = BTreeMap::new();
        for slot in &sources.slots {
            let executions = lists[slot.tuple].clone();
            let values = executions
                .iter()
                .map(|(execution, _)| payloads.value(*execution, &slot.field))
                .collect();
            inputs.insert(
                slot.name.clone(),
                InputDeclaration::Json {
                    value: Value::Array(values),
                },
            );
            selections.insert(
                slot.name.clone(),
                GroupSelection {
                    reference: slot.reference.clone(),
                    executions,
                },
            );
        }
        PinnedGroup::new(due.binding_id, due.group_id.clone(), inputs, selections)
    }
}

/// The selected source values of one snapshot, by execution.
#[derive(Default)]
struct Payloads {
    /// Each execution's earliest visible input declarations, as JSON.
    inputs: BTreeMap<ExecutionId, Value>,
    /// Each execution's selected result outcome, as JSON.
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

/// Pure selection, ordering, and value-mapping checks of the poller; its
/// Bifrost read is proved by the grouped user journeys.
#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone as _};
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
        visible.input(0, b, at(10), at(10), json!("b-late"));
        visible.input(0, b, at(9), at(9), json!("b"));
        visible.result(early, 0, b, at(12), at(12), json!("first"));
        visible.result(late, 0, b, at(15), at(15), json!("second"));
        visible.result(late, 0, b, at(20), at(20), json!("replayed"));
        visible.input(0, a, at(10), at(10), json!("a"));
        visible.result(tied, 0, c, at(5), at(5), json!("only"));
        visible.input(0, d, at(10), at(10), json!("d"));
        visible.input(1, other, at(1), at(1), json!("other"));
        assert_eq!(
            visible.executions(0),
            [(c, Some(tied)), (b, Some(late)), (a, None), (d, None)]
        );
        assert_eq!(visible.inputs[&b].1.value, json!("b"));
        assert_eq!(visible.results[&late].1.value, json!("second"));
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
        visible.input(0, third, at(10), at(1), json!(null));
        visible.input(0, first, at(10), at(3), json!(null));
        visible.input(0, second, at(5), at(2), json!(null));
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
        let turn = |value: Value| json!({"turn": {"kind": "json", "value": {"turn": value}}});
        let values = [
            turn(json!("b")),
            json!({}),
            turn(json!(10)),
            turn(json!(2)),
            turn(json!("a")),
            turn(json!(2)),
            turn(json!(true)),
        ];
        let payloads = Payloads {
            inputs: ids.iter().copied().zip(values).collect(),
            outcomes: BTreeMap::new(),
        };
        let order = Order {
            tuple: 0,
            field: SourceField::Input("turn".parse().expect("slot name")),
            path: vec![PathSegment::Field("turn".to_owned())],
        };
        let mut list: Vec<_> = ids.iter().map(|id| (*id, None)).collect();
        order.sort(&mut list, &payloads);
        let [b, missing, ten, two, a, two_again, other] = ids;
        assert_eq!(
            list.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [two, two_again, ten, a, b, missing, other]
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
