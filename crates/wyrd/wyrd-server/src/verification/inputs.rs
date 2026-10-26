//! One verification execution's input record and the one resolver that
//! turns it into the evidence the execution judges.
//!
//! Every execution has exactly one logical `bifrost.verification.execution_inputs`
//! record. A direct or submitted record holds the exact named input
//! declarations; a group record holds a [`GroupSelector`] naming each source
//! slot's ordered executions and selected results; a scheduled drift record
//! holds a [`ScheduleSelector`] freezing the window's observation membership.
//! No record copies another record's values. [`InputResolver`] is the one
//! reader the runner, the group poller, and result readback share: it reads a
//! record by execution ID and resolves a group selection from its sources'
//! input and output records, recursively for a chained group. A record or
//! source not visible yet resolves to `None`, never to fabricated evidence.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use arrow::array::{Array, RecordBatch};
use arrow::util::display::array_value_to_string;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wyrd_spec::card::agent::ExecutionRoute;
use wyrd_spec::card::verifier::{SourceField, SourceText};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, ExecutionId, GroupId, SlotName, VerificationResultId};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::verification::execution::{InputDeclaration, VerificationOutcome};
use wyrd_spec::verification::{DriftWindow, GroupSnapshot, GroupSourceSnapshot, TaskId};

use super::drift::ObservationWindow;
use crate::query::scheduled::ScheduledQueryCaller;

/// Error code of a read of a Bifrost table the tenant never created.
pub(crate) const TABLE_NOT_FOUND: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

/// How an execution was created, recorded as its input record's `origin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOrigin {
    /// A synchronous direct verification, which has no queue row.
    Direct,
    /// A queued caller submission.
    Submitted,
    /// A `group_end` binding's judgment of one settled selection.
    Group,
    /// A scheduled drift occurrence.
    Schedule,
}

impl InputOrigin {
    /// The stored `origin` spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Submitted => "submitted",
            Self::Group => "group",
            Self::Schedule => "schedule",
        }
    }

    /// Decode a stored `origin`, `None` outside the closed set.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        [Self::Direct, Self::Submitted, Self::Group, Self::Schedule]
            .into_iter()
            .find(|origin| origin.as_str() == stored)
    }
}

/// A group execution's settled selection: the half-open admission range it
/// was selected over and each source slot's ordered references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSelector {
    /// The admission range the membership was selected over.
    pub range: SelectionRange,
    /// Each source slot's ordered selection.
    pub slots: BTreeMap<SlotName, SlotSelection>,
}

impl GroupSelector {
    /// The public status projection: each slot's authored source and its
    /// executions in list order.
    #[must_use]
    pub fn snapshot(&self) -> GroupSnapshot {
        GroupSnapshot {
            sources: self
                .slots
                .iter()
                .map(|(slot, selection)| {
                    (
                        slot.clone(),
                        GroupSourceSnapshot {
                            reference: selection.reference.clone(),
                            execution_ids: selection
                                .executions
                                .iter()
                                .map(|source| source.execution_id)
                                .collect(),
                        },
                    )
                })
                .collect(),
        }
    }
}

/// A half-open `[start, end)` range of managed `wyrd_ingested_at` admission
/// times; `start` equals `end` when nothing was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionRange {
    /// The earliest selected admission time.
    pub start: DateTime<Utc>,
    /// The exclusive cutoff: the group's PostgreSQL due time.
    pub end: DateTime<Utc>,
}

/// One source slot's selection: its authored source and, in list order, each
/// source execution with its selected result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSelection {
    /// The authored source declaration, naming the field the slot reads.
    pub reference: SourceText,
    /// Each source execution in list order.
    pub executions: Vec<SourceRef>,
}

/// One selected source execution and its selected result, `None` when none
/// was visible, so its outcome value is null.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    /// The source execution.
    pub execution_id: ExecutionId,
    /// The selected result revision.
    pub result_id: Option<VerificationResultId>,
}

/// A scheduled drift execution's frozen population: its window and the
/// observation `record_id`s selected in it at preparation, possibly none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleSelector {
    /// The occurrence's `created_at` window.
    pub window: DriftWindow,
    /// The selected observations, in `record_id` order.
    pub record_ids: Vec<String>,
}

/// The part of a submitted record's `activation` a queued execution runs
/// with: the accepted per-task route overrides.
#[derive(Debug, Deserialize)]
struct StoredActivation {
    /// Per-task Agent route overrides the submission carried.
    route_overrides: BTreeMap<TaskId, ExecutionRoute>,
}

/// One execution's input record as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct InputRow {
    /// How the execution was created.
    pub origin: InputOrigin,
    /// The `inputs` Variant: declarations or a selector.
    pub inputs: Value,
    /// The `activation` Variant of a submitted record, else null.
    pub activation: Value,
}

impl InputRow {
    /// A submitted record's accepted route overrides; empty for any other
    /// origin.
    ///
    /// # Errors
    /// Returns an internal error when the stored activation is malformed.
    pub fn routes(&self) -> Result<BTreeMap<TaskId, ExecutionRoute>, WyrdError> {
        if self.activation.is_null() {
            return Ok(BTreeMap::new());
        }
        let stored: StoredActivation = decode(self.activation.clone())?;
        Ok(stored.route_overrides)
    }

    /// A schedule record's frozen selector.
    ///
    /// # Errors
    /// Returns an internal error when the record is not a well-formed
    /// schedule selector.
    pub fn schedule(&self) -> Result<ScheduleSelector, WyrdError> {
        match self.origin {
            InputOrigin::Schedule => decode(self.inputs.clone()),
            origin => Err(invalid(format!(
                "a {origin:?} record has no schedule selector"
            ))),
        }
    }

    /// A group record's selector, `None` for any other origin.
    ///
    /// # Errors
    /// Returns an internal error when the stored selector is malformed.
    pub fn group(&self) -> Result<Option<GroupSelector>, WyrdError> {
        match self.origin {
            InputOrigin::Group => decode(self.inputs.clone()).map(Some),
            _ => Ok(None),
        }
    }
}

/// The published summary identities of one direct execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectResult {
    /// Its one result.
    pub result_id: VerificationResultId,
    /// The exact Verifier.
    pub verifier_card_uid: CardUid,
    /// The Card verified.
    pub card_uid: CardUid,
    /// The caller group, when the request named one.
    pub group_id: Option<GroupId>,
    /// The deployment environment tag the request carried.
    pub deployment_environment: Option<String>,
}

/// The boxed future of a recursive resolution.
type Resolving<'a, T> = Pin<Box<dyn Future<Output = Result<T, WyrdError>> + Send + 'a>>;

/// The one reader of input and output records, bound to the query caller
/// whose authority every statement runs under.
pub struct InputResolver<'q> {
    /// The authenticated caller or internal read principal.
    query: &'q ScheduledQueryCaller,
    /// Deadline of one statement, in milliseconds; `None` for the default.
    deadline_ms: Option<i64>,
}

impl<'q> InputResolver<'q> {
    /// Read through `query`, bounding each statement by `deadline_ms`.
    #[must_use]
    pub const fn new(query: &'q ScheduledQueryCaller, deadline_ms: Option<i64>) -> Self {
        Self { query, deadline_ms }
    }

    /// The input record of `execution`, `None` while it is not visible.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when the
    /// stored record is malformed.
    pub async fn record(&self, execution: ExecutionId) -> Result<Option<InputRow>, WyrdError> {
        let rows = self
            .records(&format!("execution_id = {}", quote(&execution.to_string())))
            .await?;
        Ok(rows.into_values().next())
    }

    /// The evidence `row` judges: a supplied record's declarations, a
    /// group's resolved slot lists, or a schedule's empty map. `None` while a
    /// group source's input or selected output is not visible.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when a
    /// stored record or declaration is malformed.
    pub fn evidence<'a>(
        &'a self,
        row: &'a InputRow,
    ) -> Resolving<'a, Option<BTreeMap<SlotName, InputDeclaration>>> {
        Box::pin(async move {
            match row.origin {
                InputOrigin::Direct | InputOrigin::Submitted => {
                    decode(row.inputs.clone()).map(Some)
                }
                InputOrigin::Schedule => Ok(Some(BTreeMap::new())),
                InputOrigin::Group => {
                    let selector: GroupSelector = decode(row.inputs.clone())?;
                    self.slots(&selector.slots).await
                }
            }
        })
    }

    /// Resolve `slots` into one ordered JSON list per slot, one value per
    /// source in list order: a JSON input unwraps to its value, a trace
    /// input to its trace ID, a file or Artifact input is its declaration,
    /// and an outcome is the named requirement's outcome in the selected
    /// result, null when the source has none. `None` while any source's input
    /// record or selected output is not visible.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when a
    /// source declaration or stored record is malformed.
    pub async fn slots(
        &self,
        slots: &BTreeMap<SlotName, SlotSelection>,
    ) -> Result<Option<BTreeMap<SlotName, InputDeclaration>>, WyrdError> {
        let sources: Vec<SourceRef> = slots
            .values()
            .flat_map(|slot| slot.executions.iter().copied())
            .collect();
        let mut payloads = Payloads::default();
        if !sources.is_empty() {
            // ponytail: one IN list per resolution, growing with the group;
            // page it if a large group's statement exceeds the query bounds.
            let executions = list(sources.iter().map(|source| source.execution_id.to_string()));
            for (execution, row) in self
                .records(&format!("execution_id IN ({executions})"))
                .await?
            {
                let value = match row.origin {
                    InputOrigin::Group => match self.evidence(&row).await? {
                        Some(evidence) => serde_json::to_value(evidence)
                            .map_err(|error| invalid(error.to_string()))?,
                        None => return Ok(None),
                    },
                    _ => row.inputs,
                };
                payloads.inputs.insert(execution, value);
            }
            let selected: BTreeMap<VerificationResultId, ExecutionId> = sources
                .iter()
                .filter_map(|source| Some((source.result_id?, source.execution_id)))
                .collect();
            if !selected.is_empty() {
                let results = list(selected.keys().map(ToString::to_string));
                let outputs = self
                    .rows(&format!(
                        "SELECT result_id, output FROM bifrost.verification.execution_outputs \
                         WHERE result_id IN ({results})"
                    ))
                    .await?;
                for batch in &outputs {
                    for row in 0..batch.num_rows() {
                        let result: VerificationResultId =
                            text(batch, "result_id", row)?.parse().map_err(
                                |error: wyrd_spec::ids::IdError| invalid(error.to_string()),
                            )?;
                        if let Some(execution) = selected.get(&result) {
                            payloads
                                .outcomes
                                .insert(*execution, variant(batch, "output", row)?);
                        }
                    }
                }
                if selected
                    .values()
                    .any(|execution| !payloads.outcomes.contains_key(execution))
                {
                    return Ok(None);
                }
            }
            if sources
                .iter()
                .any(|source| !payloads.inputs.contains_key(&source.execution_id))
            {
                return Ok(None);
            }
        }
        let mut resolved = BTreeMap::new();
        for (slot, selection) in slots {
            let field = selection
                .reference
                .parse()
                .map_err(|error| invalid(error.to_string()))?
                .field;
            let values = selection
                .executions
                .iter()
                .map(|source| payloads.value(source.execution_id, &field))
                .collect();
            resolved.insert(
                slot.clone(),
                InputDeclaration::Json {
                    value: Value::Array(values),
                },
            );
        }
        Ok(Some(resolved))
    }

    /// One published result's application Run and full outcome: the verdict
    /// from its summary, requirements and steps from its output record, and
    /// evidence resolved from its execution's input record. `None` until all
    /// three records and every group source are visible, including a tenant
    /// whose tables no result has created.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when a
    /// stored record does not decode.
    pub async fn result(
        &self,
        result_id: VerificationResultId,
    ) -> Result<Option<(Option<RunId>, VerificationOutcome)>, WyrdError> {
        let batches = self
            .rows(&format!(
                "SELECT r.verdict, r.run_id, o.output, i.origin, i.inputs, i.activation \
                 FROM bifrost.verification.results r \
                 JOIN bifrost.verification.execution_outputs o ON o.result_id = r.result_id \
                 JOIN bifrost.verification.execution_inputs i \
                 ON i.execution_id = r.execution_id \
                 WHERE r.result_id = {} LIMIT 1",
                quote(&result_id.to_string())
            ))
            .await?;
        let Some(batch) = batches.iter().find(|batch| batch.num_rows() > 0) else {
            return Ok(None);
        };
        let row = InputRow {
            origin: InputOrigin::parse(&text(batch, "origin", 0)?)
                .ok_or_else(|| invalid("unknown input origin".to_owned()))?,
            inputs: variant(batch, "inputs", 0)?,
            activation: Value::Null,
        };
        let Some(evidence) = self.evidence(&row).await? else {
            return Ok(None);
        };
        let mut outcome = variant(batch, "output", 0)?;
        let Value::Object(fields) = &mut outcome else {
            return Err(invalid("output is not an object".to_owned()));
        };
        fields.insert(
            "verdict".to_owned(),
            Value::String(text(batch, "verdict", 0)?),
        );
        fields.insert(
            "evidence".to_owned(),
            serde_json::to_value(evidence).map_err(|error| invalid(error.to_string()))?,
        );
        let run_id = (!column(batch, "run_id")?.is_null(0))
            .then(|| text(batch, "run_id", 0).map(RunId::from_string))
            .transpose()?;
        Ok(Some((run_id, decode(outcome)?)))
    }

    /// The published summary of a direct execution, which has no queue row:
    /// only a result whose input record has the `direct` origin, so a queued
    /// execution is never read this way. `None` until both are visible.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when a
    /// stored identity is malformed.
    pub async fn direct(&self, execution: ExecutionId) -> Result<Option<DirectResult>, WyrdError> {
        let batches = self
            .rows(&format!(
                "SELECT r.result_id, r.verifier_card_uid, r.subject_card_uid, r.group_id, \
                 r.deployment_environment FROM bifrost.verification.results r \
                 JOIN bifrost.verification.execution_inputs i \
                 ON i.execution_id = r.execution_id \
                 WHERE r.execution_id = {} AND i.origin = 'direct' LIMIT 1",
                quote(&execution.to_string())
            ))
            .await?;
        let Some(batch) = batches.iter().find(|batch| batch.num_rows() > 0) else {
            return Ok(None);
        };
        let optional = |name: &str| {
            (!column(batch, name)?.is_null(0))
                .then(|| text(batch, name, 0))
                .transpose()
        };
        let card = |name: &str| {
            CardUid::new(text(batch, name, 0)?).map_err(|error| invalid(error.to_string()))
        };
        Ok(Some(DirectResult {
            result_id: text(batch, "result_id", 0)?
                .parse()
                .map_err(|error: wyrd_spec::ids::IdError| invalid(error.to_string()))?,
            verifier_card_uid: card("verifier_card_uid")?,
            card_uid: card("subject_card_uid")?,
            group_id: optional("group_id")?
                .map(|group| GroupId::new(group).map_err(|error| invalid(error.to_string())))
                .transpose()?,
            deployment_environment: optional("deployment_environment")?,
        }))
    }

    /// The input records matching `filter`, first visible row per execution.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when a
    /// row is malformed.
    async fn records(&self, filter: &str) -> Result<BTreeMap<ExecutionId, InputRow>, WyrdError> {
        let batches = self
            .rows(&format!(
                "SELECT execution_id, origin, inputs, activation \
                 FROM bifrost.verification.execution_inputs WHERE {filter}"
            ))
            .await?;
        let mut records = BTreeMap::new();
        for batch in &batches {
            for row in 0..batch.num_rows() {
                let execution: ExecutionId = text(batch, "execution_id", row)?
                    .parse()
                    .map_err(|error: wyrd_spec::ids::IdError| invalid(error.to_string()))?;
                if records.contains_key(&execution) {
                    continue;
                }
                let origin = InputOrigin::parse(&text(batch, "origin", row)?)
                    .ok_or_else(|| invalid("unknown input origin".to_owned()))?;
                let activation = if column(batch, "activation")?.is_null(row) {
                    Value::Null
                } else {
                    variant(batch, "activation", row)?
                };
                records.insert(
                    execution,
                    InputRow {
                        origin,
                        inputs: variant(batch, "inputs", row)?,
                        activation,
                    },
                );
            }
        }
        Ok(records)
    }

    /// Run `sql`, collecting its batches; a table the tenant never created
    /// reads as no rows.
    ///
    /// # Errors
    /// Returns the query refusal or failure.
    async fn rows(&self, sql: &str) -> Result<Vec<RecordBatch>, WyrdError> {
        let mut batches = Vec::new();
        let read = self.query.run_with(
            BifrostQueryRequest {
                params: Vec::new(),
                sql: sql.to_owned(),
                deadline_ms: self.deadline_ms,
            },
            |batch| {
                batches.push(batch);
                Ok(())
            },
        );
        match read.await {
            Err(error) if error.code() == TABLE_NOT_FOUND => Ok(Vec::new()),
            Err(error) => Err(error),
            Ok(_) => Ok(batches),
        }
    }
}

/// The resolved source values of one selection, by execution.
#[derive(Default)]
pub(super) struct Payloads {
    /// Each execution's input declarations, as JSON.
    pub(super) inputs: BTreeMap<ExecutionId, Value>,
    /// Each execution's selected output, as JSON.
    pub(super) outcomes: BTreeMap<ExecutionId, Value>,
}

impl Payloads {
    /// The value `execution` contributes for `field`, null when absent.
    ///
    /// A JSON input unwraps to its value and a trace input to its trace ID;
    /// a file or Artifact input is its declaration. An outcome is the named
    /// requirement's outcome in the selected result's output.
    pub(super) fn value(&self, execution: ExecutionId, field: &SourceField) -> Value {
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

/// `values` as a comma-separated list of escaped SQL string literals.
fn list(values: impl Iterator<Item = String>) -> String {
    values
        .map(|value| quote(&value))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `value` as an escaped SQL string literal.
fn quote(value: &str) -> String {
    ObservationWindow::text(value)
}

/// Decode a stored JSON value as `T`.
///
/// # Errors
/// Returns an internal error naming the decode failure.
fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, WyrdError> {
    serde_json::from_value(value).map_err(|error| invalid(error.to_string()))
}

/// A malformed stored record as an internal error.
fn invalid(error: String) -> WyrdError {
    WyrdError::internal(format!("stored verification record is invalid: {error}"))
}

/// The `name` column of `batch`.
///
/// # Errors
/// Returns an internal error when the column is absent.
fn column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a dyn Array, WyrdError> {
    batch
        .column_by_name(name)
        .map(AsRef::as_ref)
        .ok_or_else(|| invalid(format!("no {name} column")))
}

/// Non-null text cell `row` of column `name`.
///
/// # Errors
/// Returns an internal error when the column is absent or the cell is null
/// or cannot be displayed.
fn text(batch: &RecordBatch, name: &str, row: usize) -> Result<String, WyrdError> {
    let column = column(batch, name)?;
    if column.is_null(row) {
        return Err(invalid(format!("null {name}")));
    }
    array_value_to_string(column, row).map_err(|error| invalid(error.to_string()))
}

/// Variant cell `row` of column `name` as JSON.
///
/// # Errors
/// Returns an internal error when the column is absent or the cell does not
/// decode.
fn variant(batch: &RecordBatch, name: &str, row: usize) -> Result<Value, WyrdError> {
    wyrd_queue::variant::variant_cell_to_json(column(batch, name)?, row)
        .map_err(|error| invalid(format!("{name}: {error:?}")))
}

#[cfg(test)]
mod tests {
    //! Pure value mapping of resolved sources.

    use serde_json::json;

    use super::*;

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

    /// Origins round-trip through their stored spelling.
    ///
    /// # Panics
    /// Panics when an origin spells or parses otherwise.
    #[test]
    fn origins_use_their_stored_spelling() {
        for (origin, stored) in [
            (InputOrigin::Direct, "direct"),
            (InputOrigin::Submitted, "submitted"),
            (InputOrigin::Group, "group"),
            (InputOrigin::Schedule, "schedule"),
        ] {
            assert_eq!(origin.as_str(), stored);
            assert_eq!(InputOrigin::parse(stored), Some(origin));
        }
    }
}
