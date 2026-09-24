//! Arrow payloads for one Verifier result: pure mapping, no transport.
//!
//! [`ResultPayloadBuilder`] maps an existing engine report onto the authored
//! schemas of `vala.verification.results`, `vala.drift.result_features`, and
//! `vala.eval.result_items`, taken from the tables' own declarations so the
//! payload can never drift from the registered contract. Every batch also
//! carries the three correlation columns Scribe admits from a native payload:
//! `card_ref` (the Verifier, resolved against the SYSTEM token's signed scope),
//! `run_id` (the Verifier run), and `wyrd_event_time` (the one server-chosen
//! event time shared by the summary and every detail row).

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Float64Array, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::error::ArrowError;
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Error as JsonError;
use vala_bifrost_redux::tables::verification::ResultsTable;
use vala_bifrost_redux::tables::{DomainTable, ResultFeaturesTable, ResultItemsTable};
use vala_drift::{DriftReport, DriftVerdict};
use vala_eval::executor::{EvalReport, SkipReason, TaskRunOutcome};
use wyrd_spec::ids::{BindingId, CardUid, VerificationResultId, VerificationRunId};
use wyrd_spec::vala::managed_columns::{CARD_REF, RUN_ID, WYRD_EVENT_TIME};
use wyrd_spec::verification::{DriftWindow, FrozenTarget, VerificationVerdict};
use wyrd_sql::queries::verifier_runs::{ClaimedRun, RunInput};

use super::engines::VerifierReport;

/// Why a report could not be mapped to result batches.
///
/// Every variant is a defect of the report or the claimed run, not of the
/// transport, so the runner settles it `errored` rather than retrying.
#[derive(Debug, thiserror::Error)]
pub enum ResultPayloadError {
    /// The report's implementation cannot consume the run's frozen input,
    /// such as a Drift report for an Eval record run.
    #[error("{implementation} result does not match the run's frozen input")]
    InputMismatch {
        /// The report's implementation.
        implementation: &'static str,
    },
    /// A report value does not fit its Arrow column.
    #[error("result value {column} is out of range")]
    OutOfRange {
        /// The column whose value overflowed.
        column: &'static str,
    },
    /// A report value could not be encoded as canonical JSON.
    #[error("result JSON encoding failed")]
    Json(#[from] JsonError),
    /// The mapped columns did not form a valid batch.
    #[error("result batch assembly failed")]
    Arrow(#[from] ArrowError),
    /// The authored columns do not name each table field exactly once.
    ///
    /// Raised before assembly when an authored column is absent from, repeated
    /// against, or unknown to the destination table's declared fields.
    #[error("result column {column} is {problem}")]
    ColumnMismatch {
        /// The offending column name.
        column: String,
        /// `missing`, `duplicate`, or `unexpected`.
        problem: &'static str,
    },
}

/// One authored result column: the table field name it fills and its values.
///
/// The name, not the position, binds the values to the destination table's
/// field, so a reordered table declaration cannot relabel evidence.
type NamedColumn = (&'static str, ArrayRef);

/// One sealed-ready batch and the table it is written to.
#[derive(Debug, Clone)]
pub struct ResultBatch {
    /// Fully qualified destination table, such as `vala.verification.results`.
    pub table: String,
    /// The rows, including the correlation columns.
    pub batch: RecordBatch,
}

/// Every batch of one result, in required write order.
///
/// Non-empty detail batches precede the summary; a result with no detail rows
/// holds only the summary. Built once per publication attempt and reused
/// verbatim, so an unacknowledged retry resends identical rows.
#[derive(Debug, Clone)]
pub struct ResultPayload {
    /// The result identity every row carries.
    result_id: VerificationResultId,
    /// The common verdict the summary records.
    verdict: VerificationVerdict,
    /// Details first, summary last; never an empty batch.
    batches: Vec<ResultBatch>,
}

impl ResultPayload {
    /// The result identity the run completes with.
    #[must_use]
    pub fn result_id(&self) -> VerificationResultId {
        self.result_id
    }

    /// The common verdict of this result.
    #[must_use]
    pub fn verdict(&self) -> VerificationVerdict {
        self.verdict
    }

    /// Every batch in write order: details, then the summary.
    #[must_use]
    pub fn batches(&self) -> &[ResultBatch] {
        &self.batches
    }
}

/// The frozen identities and input of the run a result belongs to.
///
/// A borrowed view of the fields of a [`ClaimedRun`] that result rows record,
/// separated from the lease so the mapping can be exercised without a claim.
#[derive(Debug, Clone, Copy)]
pub struct ResultRun<'a> {
    /// The Verifier run, stored as the managed `run_id`.
    pub run_id: VerificationRunId,
    /// Exact Verifier Card version used.
    pub verifier_version: &'a str,
    /// Verified subject Card.
    pub subject_card_uid: &'a CardUid,
    /// Binding owner; `None` for a direct run.
    pub owner_card_uid: Option<&'a CardUid>,
    /// Binding; `None` for a direct run.
    pub binding_id: Option<BindingId>,
    /// Frozen Trigger; `None` for a direct run.
    pub trigger: Option<&'a FrozenTarget>,
    /// The frozen input the Verifier analyzed.
    pub input: &'a RunInput,
}

impl<'a> From<&'a ClaimedRun> for ResultRun<'a> {
    /// Borrow the result-relevant identities of a claimed run.
    fn from(run: &'a ClaimedRun) -> Self {
        Self {
            run_id: run.lease.run_id,
            verifier_version: &run.verifier_version,
            subject_card_uid: &run.subject_card_uid,
            owner_card_uid: run.owner_card_uid.as_ref(),
            binding_id: run.binding_id,
            trigger: run.trigger.as_ref(),
            input: &run.input,
        }
    }
}

/// Maps one completed run's report onto its result batches.
///
/// Owns the identities every row repeats — result, run, Verifier reference,
/// subject, owner, binding — plus the execution interval and the single
/// server-chosen event time, so each table mapping reads them from one place.
#[derive(Debug, Clone, Copy)]
pub struct ResultPayloadBuilder<'a> {
    /// The run whose frozen identities and input the rows record.
    run: ResultRun<'a>,
    /// Verifier `CardRef` text without a UID, authorized by Scribe against scope.
    verifier_ref: &'a str,
    /// The new result's identity.
    result_id: VerificationResultId,
    /// The one event time shared by every row of this result.
    event_time: DateTime<Utc>,
    /// Verifier execution start.
    started_at: DateTime<Utc>,
    /// Verifier execution end.
    ended_at: DateTime<Utc>,
}

impl<'a> ResultPayloadBuilder<'a> {
    /// Build a mapper for one result of `run`.
    ///
    /// `event_time` is chosen by the server once and stamped on the summary
    /// and every detail row, so a result and its details share one UTC-day
    /// partition.
    #[must_use]
    pub fn new(
        run: ResultRun<'a>,
        verifier_ref: &'a str,
        result_id: VerificationResultId,
        event_time: DateTime<Utc>,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
    ) -> Self {
        Self {
            run,
            verifier_ref,
            result_id,
            event_time,
            started_at,
            ended_at,
        }
    }

    /// Map `report` to its ordered batches.
    ///
    /// Drift writes one `result_features` row per scored feature and records
    /// the report (or null when unscored) as `details`; Eval writes one
    /// `result_items` row per task outcome and records the workflow summary
    /// as `details`. A report with no detail rows writes only the summary.
    ///
    /// # Errors
    /// Returns [`ResultPayloadError::InputMismatch`] when the report's
    /// implementation cannot consume the run's frozen input,
    /// [`ResultPayloadError::OutOfRange`] when a value overflows its column,
    /// [`ResultPayloadError::ColumnMismatch`] when authored columns do not
    /// name each table field exactly once, and a JSON or Arrow error when
    /// encoding fails.
    pub fn build(&self, report: &VerifierReport) -> Result<ResultPayload, ResultPayloadError> {
        let implementation = report.implementation();
        let verdict = report.verdict();
        let mut batches = Vec::with_capacity(2);
        let (window, record_id, details) = match (report, self.run.input) {
            (VerifierReport::Drift(drift), RunInput::DriftWindow(window)) => {
                if let Some(drift) = drift
                    && let Some(features) = self.drift_features(drift, window)?
                {
                    batches.push(self.finish::<ResultFeaturesTable>(features)?);
                }
                let details = drift.as_ref().map(canonical_json).transpose()?;
                (Some(window), None, details)
            }
            (VerifierReport::Eval { report, .. }, RunInput::EvalRecord { record_id, .. }) => {
                if let Some(items) = self.eval_items(report, record_id)? {
                    batches.push(self.finish::<ResultItemsTable>(items)?);
                }
                let details = canonical_json(&report.workflow_summary())?;
                (None, Some(record_id.as_str()), Some(details))
            }
            _ => return Err(ResultPayloadError::InputMismatch { implementation }),
        };
        let summary = self.summary(implementation, verdict, window, record_id, details)?;
        batches.push(self.finish::<ResultsTable>(summary)?);
        Ok(ResultPayload {
            result_id: self.result_id,
            verdict,
            batches,
        })
    }

    /// Author the named columns of the one `vala.verification.results` row.
    ///
    /// # Errors
    /// Returns a JSON error when the frozen Trigger cannot be encoded.
    fn summary(
        &self,
        implementation: &'static str,
        verdict: VerificationVerdict,
        window: Option<&DriftWindow>,
        record_id: Option<&str>,
        details: Option<String>,
    ) -> Result<Vec<NamedColumn>, ResultPayloadError> {
        let trigger = self.run.trigger.map(canonical_json).transpose()?;
        let verdict: &'static str = verdict.into();
        Ok(vec![
            ("result_id", text([Some(self.result_id.to_string())])),
            ("implementation", text([Some(implementation)])),
            ("execution_status", text([Some("completed")])),
            ("verdict", text([Some(verdict)])),
            ("verifier_version", text([Some(self.run.verifier_version)])),
            ("owner_card_uid", text([self.owner()])),
            (
                "subject_card_uid",
                text([Some(self.run.subject_card_uid.to_string())]),
            ),
            ("binding_id", text([self.binding()])),
            ("trigger_identity", text([trigger])),
            ("source_record_id", text([record_id])),
            (
                "window_start",
                timestamps([window.map(|window| window.start)]),
            ),
            ("window_end", timestamps([window.map(|window| window.end)])),
            ("started_at", timestamps([Some(self.started_at)])),
            ("ended_at", timestamps([Some(self.ended_at)])),
            ("details", text([details])),
        ])
    }

    /// Author the named `vala.drift.result_features` columns, one row per
    /// scored feature.
    ///
    /// Returns `None` for a report with no features, so no empty batch is
    /// ever written. An engine `NaN` score or threshold becomes null.
    ///
    /// # Errors
    /// Returns a JSON error when the method cannot be encoded.
    fn drift_features(
        &self,
        report: &DriftReport,
        window: &DriftWindow,
    ) -> Result<Option<Vec<NamedColumn>>, ResultPayloadError> {
        let rows = report.features.len();
        if rows == 0 {
            return Ok(None);
        }
        let method = serde_json::to_value(report.method)?;
        let method = method.as_str().unwrap_or_default();
        let features = report.features.values();
        Ok(Some(vec![
            (
                "result_id",
                text(std::iter::repeat_n(Some(self.result_id.to_string()), rows)),
            ),
            (
                "owner_card_uid",
                text(std::iter::repeat_n(self.owner(), rows)),
            ),
            (
                "subject_card_uid",
                text(std::iter::repeat_n(
                    Some(self.run.subject_card_uid.to_string()),
                    rows,
                )),
            ),
            (
                "binding_id",
                text(std::iter::repeat_n(self.binding(), rows)),
            ),
            (
                "window_start",
                timestamps(std::iter::repeat_n(Some(window.start), rows)),
            ),
            (
                "window_end",
                timestamps(std::iter::repeat_n(Some(window.end), rows)),
            ),
            ("method", text(std::iter::repeat_n(Some(method), rows))),
            (
                "feature",
                text(
                    features
                        .clone()
                        .map(|feature| Some(feature.feature.as_str())),
                ),
            ),
            (
                "score",
                Arc::new(Float64Array::from_iter(
                    features.clone().map(|feature| finite(feature.score)),
                )),
            ),
            (
                "threshold",
                Arc::new(Float64Array::from_iter(
                    features.clone().map(|feature| finite(feature.threshold)),
                )),
            ),
            (
                "verdict",
                text(features.map(|feature| Some(drift_verdict(feature.verdict)))),
            ),
        ]))
    }

    /// Author the named `vala.eval.result_items` columns, one row per task
    /// outcome.
    ///
    /// Executed tasks fill the `Ran` columns and skipped tasks fill only
    /// `skip_reason` and `upstream_task_id`. Returns `None` for a sampled-out
    /// run with no outcomes, so no empty batch is ever written.
    ///
    /// # Errors
    /// Returns [`ResultPayloadError::OutOfRange`] when a stage or duration
    /// overflows its column, or a JSON error when encoding fails.
    fn eval_items(
        &self,
        report: &EvalReport,
        record_id: &str,
    ) -> Result<Option<Vec<NamedColumn>>, ResultPayloadError> {
        let rows = report.outcomes.len();
        if rows == 0 {
            return Ok(None);
        }
        let items = report
            .outcomes
            .iter()
            .map(EvalItem::from_outcome)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(vec![
            (
                "result_id",
                text(std::iter::repeat_n(Some(self.result_id.to_string()), rows)),
            ),
            (
                "owner_card_uid",
                text(std::iter::repeat_n(self.owner(), rows)),
            ),
            (
                "subject_card_uid",
                text(std::iter::repeat_n(
                    Some(self.run.subject_card_uid.to_string()),
                    rows,
                )),
            ),
            (
                "binding_id",
                text(std::iter::repeat_n(self.binding(), rows)),
            ),
            (
                "source_record_id",
                text(std::iter::repeat_n(Some(record_id), rows)),
            ),
            ("task_id", text(items.iter().map(|item| Some(item.task_id)))),
            (
                "outcome_kind",
                text(items.iter().map(|item| Some(item.outcome_kind))),
            ),
            (
                "passed",
                Arc::new(BooleanArray::from_iter(
                    items.iter().map(|item| item.passed),
                )),
            ),
            (
                "actual",
                text(items.iter().map(|item| item.actual.as_deref())),
            ),
            (
                "expected",
                text(items.iter().map(|item| item.expected.as_deref())),
            ),
            (
                "operator",
                text(items.iter().map(|item| item.operator.as_deref())),
            ),
            ("message", text(items.iter().map(|item| item.message))),
            (
                "stage",
                Arc::new(Int32Array::from_iter(items.iter().map(|item| item.stage))),
            ),
            (
                "started_at",
                timestamps(items.iter().map(|item| item.started_at)),
            ),
            (
                "duration_ms",
                Arc::new(Int64Array::from_iter(
                    items.iter().map(|item| item.duration_ms),
                )),
            ),
            (
                "skip_reason",
                text(items.iter().map(|item| item.skip_reason)),
            ),
            (
                "upstream_task_id",
                text(items.iter().map(|item| item.upstream_task_id)),
            ),
        ]))
    }

    /// Assemble table `T`'s batch from its named authored columns.
    ///
    /// `T::arrow_fields()` is the only schema authority: the columns are
    /// ordered by its field names (see [`Self::assemble`]) and the batch is
    /// addressed to the table's fully qualified name.
    ///
    /// # Errors
    /// Returns [`ResultPayloadError::ColumnMismatch`] when the columns do not
    /// name each field exactly once, or an Arrow error when a column does not
    /// match its field.
    fn finish<T: DomainTable>(
        &self,
        columns: Vec<NamedColumn>,
    ) -> Result<ResultBatch, ResultPayloadError> {
        Ok(ResultBatch {
            table: format!("vala.{}.{}", T::NAMESPACE, T::NAME),
            batch: self.assemble(T::arrow_fields(), columns)?,
        })
    }

    /// Order named columns by `fields`, then append the correlation columns.
    ///
    /// Every authored column must fill exactly one of `fields` by name; the
    /// arrays are placed in the order of `fields`, never in authoring order,
    /// so a reordered declaration keeps each value under its own name. The
    /// schema is `fields` followed by `card_ref`, `run_id`, and
    /// `wyrd_event_time`, each repeated for the authored row count.
    ///
    /// # Errors
    /// Returns [`ResultPayloadError::ColumnMismatch`] with problem
    /// `duplicate` when a name is authored twice, `missing` when a field has
    /// no column, or `unexpected` when a column names no field; returns an
    /// Arrow error when a column does not match its field's type or length.
    fn assemble(
        &self,
        mut fields: Vec<Field>,
        columns: Vec<NamedColumn>,
    ) -> Result<RecordBatch, ResultPayloadError> {
        let rows = columns.first().map_or(0, |(_, array)| array.len());
        let mut named: HashMap<&'static str, ArrayRef> = HashMap::with_capacity(columns.len());
        for (name, array) in columns {
            if named.insert(name, array).is_some() {
                return Err(ResultPayloadError::ColumnMismatch {
                    column: name.to_owned(),
                    problem: "duplicate",
                });
            }
        }
        let mut arrays = Vec::with_capacity(fields.len() + 3);
        for field in &fields {
            let array = named.remove(field.name().as_str()).ok_or_else(|| {
                ResultPayloadError::ColumnMismatch {
                    column: field.name().clone(),
                    problem: "missing",
                }
            })?;
            arrays.push(array);
        }
        if let Some(name) = named.into_keys().next() {
            return Err(ResultPayloadError::ColumnMismatch {
                column: name.to_owned(),
                problem: "unexpected",
            });
        }
        fields.extend([
            Field::new(CARD_REF, DataType::Utf8, true),
            Field::new(RUN_ID, DataType::Utf8, true),
            Field::new(
                WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]);
        arrays.push(text(std::iter::repeat_n(Some(self.verifier_ref), rows)));
        arrays.push(text(std::iter::repeat_n(
            Some(self.run.run_id.to_string()),
            rows,
        )));
        arrays.push(timestamps(std::iter::repeat_n(Some(self.event_time), rows)));
        Ok(RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)?)
    }

    /// The binding owner Card UID, null for a direct run.
    fn owner(&self) -> Option<String> {
        self.run.owner_card_uid.map(ToString::to_string)
    }

    /// The binding identity, null for a direct run.
    fn binding(&self) -> Option<String> {
        self.run.binding_id.map(|binding| binding.to_string())
    }
}

/// One `result_items` row derived from a task outcome.
///
/// Borrowed text stays borrowed; only JSON renderings are owned.
struct EvalItem<'a> {
    /// Executed or skipped task identity.
    task_id: &'a str,
    /// `ran` or `skipped`.
    outcome_kind: &'static str,
    /// Whether an executed task passed.
    passed: Option<bool>,
    /// JSON of the captured actual value; `null` text for a captured null.
    actual: Option<String>,
    /// Canonical JSON of the expected value.
    expected: Option<String>,
    /// Canonical JSON of the comparison operator.
    operator: Option<String>,
    /// Executed task's message.
    message: Option<&'a str>,
    /// Executed task's topological stage.
    stage: Option<i32>,
    /// Executed task's start time.
    started_at: Option<DateTime<Utc>>,
    /// Executed task's duration.
    duration_ms: Option<i64>,
    /// `condition_false` or `dependency_skipped` for a skipped task.
    skip_reason: Option<&'static str>,
    /// The skipped upstream dependency, for a dependency skip.
    upstream_task_id: Option<&'a str>,
}

impl<'a> EvalItem<'a> {
    /// Map one executor outcome onto its row.
    ///
    /// # Errors
    /// Returns [`ResultPayloadError::OutOfRange`] when the stage or duration
    /// overflows its signed column, or a JSON error when a value cannot be
    /// encoded.
    fn from_outcome(outcome: &'a TaskRunOutcome) -> Result<Self, ResultPayloadError> {
        Ok(match outcome {
            TaskRunOutcome::Ran(result) => Self {
                task_id: result.task_id.as_str(),
                outcome_kind: "ran",
                passed: Some(result.passed),
                actual: result.actual.as_ref().map(canonical_json).transpose()?,
                expected: Some(canonical_json(&result.expected)?),
                operator: Some(canonical_json(&result.operator)?),
                message: result.message.as_deref(),
                stage: Some(
                    i32::try_from(result.stage)
                        .map_err(|_| ResultPayloadError::OutOfRange { column: "stage" })?,
                ),
                started_at: Some(result.started_at),
                duration_ms: Some(i64::try_from(result.duration_ms).map_err(|_| {
                    ResultPayloadError::OutOfRange {
                        column: "duration_ms",
                    }
                })?),
                skip_reason: None,
                upstream_task_id: None,
            },
            TaskRunOutcome::Skipped { task_id, reason } => {
                let (skip_reason, upstream) = match reason {
                    SkipReason::ConditionFalse => ("condition_false", None),
                    SkipReason::DependencySkipped { upstream } => {
                        ("dependency_skipped", Some(upstream.as_str()))
                    }
                };
                Self {
                    task_id: task_id.as_str(),
                    outcome_kind: "skipped",
                    passed: None,
                    actual: None,
                    expected: None,
                    operator: None,
                    message: None,
                    stage: None,
                    started_at: None,
                    duration_ms: None,
                    skip_reason: Some(skip_reason),
                    upstream_task_id: upstream,
                }
            }
        })
    }
}

/// Encode `value` as canonical (RFC 8785) JSON.
///
/// Routes through [`serde_json::Value`] first so a non-finite float, which
/// canonical JSON cannot represent, becomes `null` instead of failing.
///
/// # Errors
/// Returns the JSON error when `value` cannot be serialized.
fn canonical_json<T: Serialize>(value: &T) -> Result<String, JsonError> {
    serde_jcs::to_string(&serde_json::to_value(value)?)
}

/// The stored value of a Drift feature verdict.
const fn drift_verdict(verdict: DriftVerdict) -> &'static str {
    match verdict {
        DriftVerdict::NoDrift => "no_drift",
        DriftVerdict::Drift => "drift",
        DriftVerdict::Inconclusive => "inconclusive",
    }
}

/// A finite engine score, or null for the engine's `NaN`/infinite marker.
fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// Build a nullable UTF-8 column.
fn text<S: AsRef<str>>(values: impl IntoIterator<Item = Option<S>>) -> ArrayRef {
    Arc::new(StringArray::from_iter(values))
}

/// Build a nullable UTC microsecond timestamp column.
fn timestamps(values: impl IntoIterator<Item = Option<DateTime<Utc>>>) -> ArrayRef {
    Arc::new(
        TimestampMicrosecondArray::from_iter(
            values
                .into_iter()
                .map(|value| value.map(|at| at.timestamp_micros())),
        )
        .with_timezone("UTC"),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use arrow::array::{Array, AsArray};
    use arrow::datatypes::TimestampMicrosecondType;
    use chrono::TimeZone;
    use vala_drift::FeatureDriftReport;
    use wyrd_spec::card::drift::DriftMethod;
    use wyrd_spec::ids::FeatureName;
    use wyrd_spec::vala::eval::ids::TaskId;
    use wyrd_spec::vala::eval::operator::ComparisonOperator;
    use wyrd_spec::vala::eval::result::AssertionResult;

    use super::*;

    /// Owned identities of one binding run, borrowed as a [`ResultRun`].
    struct TestRun {
        /// The Verifier run.
        run_id: VerificationRunId,
        /// Subject Card.
        subject: CardUid,
        /// Binding owner Card.
        owner: CardUid,
        /// Binding identity.
        binding: BindingId,
        /// Frozen inline Trigger.
        trigger: FrozenTarget,
        /// Frozen input.
        input: RunInput,
    }

    impl TestRun {
        /// Borrow this run as the builder's view.
        fn view(&self) -> ResultRun<'_> {
            ResultRun {
                run_id: self.run_id,
                verifier_version: "1.2.0",
                subject_card_uid: &self.subject,
                owner_card_uid: Some(&self.owner),
                binding_id: Some(self.binding),
                trigger: Some(&self.trigger),
                input: &self.input,
            }
        }
    }

    /// Fixed instant `seconds` after 2026-09-22T00:00:00Z.
    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 0, 0, 0)
            .single()
            .expect("valid instant")
            + chrono::Duration::seconds(seconds)
    }

    /// Mint a Card UID.
    fn uid() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::now_v7()).expect("UUIDv7 is a valid Card UID")
    }

    /// A binding run over `input`.
    fn run(input: RunInput) -> TestRun {
        TestRun {
            run_id: VerificationRunId::new_v7(),
            subject: uid(),
            owner: uid(),
            binding: BindingId::new_v7(),
            trigger: FrozenTarget::Digest("sha256:t".to_owned()),
            input,
        }
    }

    /// A Drift run over one fixed day.
    fn drift_run() -> TestRun {
        run(RunInput::DriftWindow(DriftWindow {
            start: at(-86_400),
            end: at(0),
        }))
    }

    /// An Eval run over one committed record.
    fn eval_run() -> TestRun {
        run(RunInput::EvalRecord {
            record_id: "record-1".to_owned(),
            event_time: at(-5),
        })
    }

    /// A builder for `run` with fixed identities and times.
    fn builder(run: &TestRun) -> ResultPayloadBuilder<'_> {
        ResultPayloadBuilder::new(
            run.view(),
            "prod/Verifier/check@1.2.0",
            VerificationResultId::new_v7(),
            at(30),
            at(10),
            at(20),
        )
    }

    /// Map `report` for `run` with fixed identities and times.
    ///
    /// # Panics
    /// Panics when the report does not map.
    fn build(run: &TestRun, report: &VerifierReport) -> ResultPayload {
        builder(run).build(report).expect("report maps to batches")
    }

    /// The named summary columns of an unscored Drift result over `run`.
    ///
    /// # Panics
    /// Panics when the run is not a Drift run or the Trigger cannot encode.
    fn drift_summary_columns(builder: &ResultPayloadBuilder<'_>) -> Vec<NamedColumn> {
        let RunInput::DriftWindow(window) = builder.run.input else {
            panic!("summary columns need a Drift run");
        };
        builder
            .summary(
                "drift",
                VerificationVerdict::Inconclusive,
                Some(window),
                None,
                None,
            )
            .expect("summary columns author")
    }

    /// Read one UTF-8 cell.
    fn cell(batch: &RecordBatch, column: &str, row: usize) -> Option<String> {
        let array = batch
            .column_by_name(column)
            .unwrap_or_else(|| panic!("{column} column exists"))
            .as_string::<i32>();
        array.is_valid(row).then(|| array.value(row).to_owned())
    }

    /// Read every row's event time.
    fn event_times(batch: &RecordBatch) -> Vec<i64> {
        batch
            .column_by_name(WYRD_EVENT_TIME)
            .expect("event time column exists")
            .as_primitive::<TimestampMicrosecondType>()
            .values()
            .to_vec()
    }

    /// A two-feature Drift report writes its features before the summary, all
    /// rows share one event time and identity, and NaN scores become null.
    #[test]
    fn drift_result_writes_features_then_summary_with_nan_as_null() {
        let run = drift_run();
        let mut features = BTreeMap::new();
        for (name, score, verdict) in [
            ("latency", 0.4, DriftVerdict::Drift),
            ("tokens", f64::NAN, DriftVerdict::Inconclusive),
        ] {
            let feature = FeatureName::new(name).expect("feature name");
            features.insert(
                feature.clone(),
                FeatureDriftReport {
                    feature,
                    score,
                    threshold: 0.2,
                    verdict,
                    evidence: None,
                },
            );
        }
        let report = VerifierReport::Drift(Some(DriftReport {
            method: DriftMethod::Psi,
            features,
            verdict: DriftVerdict::Drift,
        }));
        let payload = build(&run, &report);
        assert_eq!(payload.verdict(), VerificationVerdict::Failed);
        let tables: Vec<_> = payload.batches().iter().map(|b| b.table.as_str()).collect();
        assert_eq!(
            tables,
            ["vala.drift.result_features", "vala.verification.results"]
        );
        let features = &payload.batches()[0].batch;
        let summary = &payload.batches()[1].batch;
        assert_eq!(features.num_rows(), 2);
        assert_eq!(summary.num_rows(), 1);
        let result_id = payload.result_id().to_string();
        for batch in [features, summary] {
            assert_eq!(
                event_times(batch),
                vec![at(30).timestamp_micros(); batch.num_rows()]
            );
            for row in 0..batch.num_rows() {
                assert_eq!(
                    cell(batch, "result_id", row).as_deref(),
                    Some(result_id.as_str())
                );
                assert_eq!(cell(batch, RUN_ID, row), Some(run.run_id.to_string()));
                assert_eq!(
                    cell(batch, CARD_REF, row).as_deref(),
                    Some("prod/Verifier/check@1.2.0")
                );
            }
        }
        assert_eq!(cell(features, "method", 0).as_deref(), Some("Psi"));
        assert_eq!(
            cell(features, "verdict", 1).as_deref(),
            Some("inconclusive")
        );
        let scores = features
            .column_by_name("score")
            .expect("score column")
            .as_primitive::<arrow::datatypes::Float64Type>();
        assert!(scores.is_valid(0) && scores.is_null(1), "NaN score is null");
        assert_eq!(cell(summary, "implementation", 0).as_deref(), Some("drift"));
        assert_eq!(
            cell(summary, "execution_status", 0).as_deref(),
            Some("completed")
        );
        assert_eq!(cell(summary, "verdict", 0).as_deref(), Some("failed"));
        assert_eq!(cell(summary, "source_record_id", 0), None);
        assert_eq!(
            cell(summary, "trigger_identity", 0).as_deref(),
            Some(r#"{"digest":"sha256:t"}"#)
        );
        let details: serde_json::Value =
            serde_json::from_str(&cell(summary, "details", 0).expect("details present"))
                .expect("details are JSON");
        assert_eq!(
            details["features"]["tokens"]["score"],
            serde_json::Value::Null
        );
        assert_eq!(details["verdict"], "Drift");
    }

    /// An unscored Drift execution writes only an inconclusive summary with
    /// null details, and never an empty detail batch.
    #[test]
    fn unscored_drift_writes_only_the_summary() {
        let run = drift_run();
        let payload = build(&run, &VerifierReport::Drift(None));
        assert_eq!(payload.batches().len(), 1);
        let summary = &payload.batches()[0].batch;
        assert_eq!(payload.batches()[0].table, "vala.verification.results");
        assert_eq!(cell(summary, "verdict", 0).as_deref(), Some("inconclusive"));
        assert_eq!(cell(summary, "details", 0), None);
    }

    /// Eval writes one item per ran and skipped outcome before the summary,
    /// whose details are the executed-only workflow summary.
    #[test]
    fn eval_result_writes_ran_and_skipped_items_then_summary() {
        let run = eval_run();
        let task = |id: &str| TaskId::new(id).expect("task id");
        let report = EvalReport {
            outcomes: vec![
                TaskRunOutcome::Ran(Box::new(AssertionResult {
                    task_id: task("exact"),
                    passed: true,
                    actual: Some(serde_json::Value::Null),
                    expected: serde_json::json!({"b": 2, "a": 1}),
                    operator: ComparisonOperator::Equals,
                    message: None,
                    stage: 0,
                    started_at: at(11),
                    duration_ms: 7,
                })),
                TaskRunOutcome::Skipped {
                    task_id: task("gated"),
                    reason: SkipReason::DependencySkipped {
                        upstream: task("exact"),
                    },
                },
            ],
        };
        let payload = build(
            &run,
            &VerifierReport::Eval {
                report,
                verdict: VerificationVerdict::Passed,
            },
        );
        let tables: Vec<_> = payload.batches().iter().map(|b| b.table.as_str()).collect();
        assert_eq!(
            tables,
            ["vala.eval.result_items", "vala.verification.results"]
        );
        let items = &payload.batches()[0].batch;
        assert_eq!(cell(items, "outcome_kind", 0).as_deref(), Some("ran"));
        assert_eq!(cell(items, "actual", 0).as_deref(), Some("null"));
        assert_eq!(
            cell(items, "expected", 0).as_deref(),
            Some(r#"{"a":1,"b":2}"#)
        );
        assert_eq!(
            cell(items, "source_record_id", 1).as_deref(),
            Some("record-1")
        );
        assert_eq!(cell(items, "outcome_kind", 1).as_deref(), Some("skipped"));
        assert_eq!(
            cell(items, "skip_reason", 1).as_deref(),
            Some("dependency_skipped")
        );
        assert_eq!(cell(items, "upstream_task_id", 1).as_deref(), Some("exact"));
        assert_eq!(cell(items, "expected", 1), None);
        let summary = &payload.batches()[1].batch;
        assert_eq!(
            cell(summary, "source_record_id", 0).as_deref(),
            Some("record-1")
        );
        let details: serde_json::Value =
            serde_json::from_str(&cell(summary, "details", 0).expect("details present"))
                .expect("details are JSON");
        assert_eq!(details["total_tasks"], 1);
        assert_eq!(details["passed_tasks"], 1);
    }

    /// A sampled-out Eval writes only the summary with the zero-count details.
    #[test]
    fn sampled_out_eval_writes_zero_count_summary_only() {
        let run = eval_run();
        let payload = build(
            &run,
            &VerifierReport::Eval {
                report: EvalReport::default(),
                verdict: VerificationVerdict::Inconclusive,
            },
        );
        assert_eq!(payload.batches().len(), 1);
        let details = cell(&payload.batches()[0].batch, "details", 0).expect("details present");
        assert_eq!(
            details,
            r#"{"duration_ms":0,"failed_tasks":0,"pass_rate":0,"passed_tasks":0,"total_tasks":0}"#
        );
    }

    /// A report whose implementation cannot consume the run's input is refused.
    #[test]
    fn mismatched_report_and_input_is_refused() {
        let run = eval_run();
        let error = ResultPayloadBuilder::new(
            run.view(),
            "prod/Verifier/check@1.2.0",
            VerificationResultId::new_v7(),
            at(30),
            at(10),
            at(20),
        )
        .build(&VerifierReport::Drift(None))
        .expect_err("a Drift report cannot complete an Eval record run");
        assert!(matches!(
            error,
            ResultPayloadError::InputMismatch {
                implementation: "drift"
            }
        ));
    }

    /// A reordered table declaration keeps every authored value under its own
    /// field name: adjacent same-typed columns such as `implementation`,
    /// `execution_status`, `verdict`, and `verifier_version` are not relabeled.
    #[test]
    fn reordered_table_fields_keep_values_bound_to_names() {
        let run = drift_run();
        let builder = builder(&run);
        let declared = builder
            .finish::<ResultsTable>(drift_summary_columns(&builder))
            .expect("declared order assembles")
            .batch;
        let mut reversed = ResultsTable::arrow_fields();
        reversed.reverse();
        let reordered = builder
            .assemble(reversed.clone(), drift_summary_columns(&builder))
            .expect("reversed order assembles");
        let order: Vec<_> = reordered
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect();
        let expected: Vec<_> = reversed
            .iter()
            .map(|field| field.name().clone())
            .chain([CARD_REF, RUN_ID, WYRD_EVENT_TIME].map(str::to_owned))
            .collect();
        assert_eq!(order, expected);
        for field in ResultsTable::arrow_fields() {
            assert_eq!(
                reordered.column_by_name(field.name()),
                declared.column_by_name(field.name()),
                "{} keeps its values",
                field.name()
            );
        }
        assert_eq!(
            cell(&reordered, "implementation", 0).as_deref(),
            Some("drift")
        );
        assert_eq!(
            cell(&reordered, "execution_status", 0).as_deref(),
            Some("completed")
        );
        assert_eq!(
            cell(&reordered, "verdict", 0).as_deref(),
            Some("inconclusive")
        );
        assert_eq!(
            cell(&reordered, "verifier_version", 0).as_deref(),
            Some("1.2.0")
        );
    }

    /// One refusal case: the expected problem, the mutation applied to valid
    /// summary columns, and the column name the refusal reports.
    type ColumnCase = (&'static str, fn(&mut Vec<NamedColumn>), &'static str);

    /// Authored columns that omit, repeat, or invent a table field name are
    /// refused with the named problem before any batch is assembled.
    #[test]
    fn missing_duplicate_and_unexpected_columns_are_refused() {
        let run = drift_run();
        let builder = builder(&run);
        let cases: [ColumnCase; 3] = [
            (
                "missing",
                |columns| columns.retain(|(name, _)| *name != "verdict"),
                "verdict",
            ),
            (
                "duplicate",
                |columns| columns.push(("verdict", text([Some("passed")]))),
                "verdict",
            ),
            (
                "unexpected",
                |columns| columns.push(("bogus", text([Some("x")]))),
                "bogus",
            ),
        ];
        for (expected_problem, mutate, expected_column) in cases {
            let mut columns = drift_summary_columns(&builder);
            mutate(&mut columns);
            let error = builder
                .finish::<ResultsTable>(columns)
                .expect_err("mismatched columns are refused");
            assert!(
                matches!(
                    &error,
                    ResultPayloadError::ColumnMismatch { column, problem }
                        if column == expected_column && *problem == expected_problem
                ),
                "{expected_problem}: got {error:?}"
            );
        }
    }
}
