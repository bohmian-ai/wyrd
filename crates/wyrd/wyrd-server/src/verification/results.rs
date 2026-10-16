//! Arrow payloads for one Verifier execution: pure mapping, no transport.
//!
//! [`ResultPayloadBuilder`] maps one engine [`VerificationOutcome`] onto the
//! authored schemas of `vala.verification.results` and
//! `vala.drift.result_features`, and the execution's pinned inputs onto
//! `vala.verification.execution_inputs`, taken from the tables' own declarations so
//! the payload can never drift from the registered contract. Every batch also
//! carries the two correlation columns Scribe admits from a native payload:
//! `card_uid` (the Card verified; the Verifier is the authored
//! `verifier_card_uid`) and `run_id` (the application Run judged, null when
//! none, as for scheduled drift).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::error::ArrowError;
use arrow::record_batch::RecordBatch;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Error as JsonError;
use vala_bifrost_redux::tables::verification::{
    ACTIVATION, ExecutionInputsTable, INPUTS, OUTCOME, ResultsTable,
};
use vala_bifrost_redux::tables::{DomainTable, ResultFeaturesTable};
use wyrd_queue::variant::{EncodedVariant, VariantColumnBuilder};
use wyrd_runtime::request_context::AcceptedAuthority;
use wyrd_spec::card::agent::ExecutionRoute;
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, SlotName, VerificationResultId};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::UTC_TIME_ZONE;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::vala::managed_columns::{CARD_UID, RUN_ID};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    CallProvenance, InputDeclaration, RequirementDetail, RequirementResult, VerificationOutcome,
};
use wyrd_spec::verification::{
    DriftReport, DriftVerdict, DriftWindow, FeatureDriftReport, FrozenTarget, VerificationVerdict,
};
use wyrd_sql::queries::verifier_executions::Submission;

/// Why an outcome could not be mapped to result batches.
///
/// Every variant is a defect of the outcome, not of the transport, so the
/// runner settles it `errored` rather than retrying.
#[derive(Debug, thiserror::Error)]
pub enum ResultPayloadError {
    /// An outcome value could not be encoded as canonical JSON.
    #[error("result JSON encoding failed")]
    Json(#[from] JsonError),
    /// An outcome value exceeds a Variant limit and cannot be stored.
    #[error("result Variant value refused")]
    Variant(#[from] BifrostError),
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
/// Drift feature rows precede the summary; a result with no drift detail
/// holds only the summary. Built once and staged on the Scribe outbox, which
/// resends identical rows on every retry.
#[derive(Debug, Clone)]
pub struct ResultPayload {
    /// The result identity every row carries.
    result_id: VerificationResultId,
    /// The overall verdict the summary records.
    verdict: VerificationVerdict,
    /// Details first, summary last; never an empty batch.
    batches: Vec<ResultBatch>,
}

impl ResultPayload {
    /// The result identity the execution completes with.
    #[must_use]
    pub fn result_id(&self) -> VerificationResultId {
        self.result_id
    }

    /// The overall verdict of this result.
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

/// The accepted request context of a submitted execution, recorded as its
/// inputs row's `activation` Variant.
#[derive(Serialize)]
struct Activation<'a> {
    /// Per-task Agent route overrides the submission carried.
    route_overrides: &'a BTreeMap<TaskId, ExecutionRoute>,
    /// The submission's `Idempotency-Key`, when it carried one.
    idempotency_key: Option<&'a str>,
    /// Hex SHA-256 of the canonical request, when keyed.
    request_sha256: Option<String>,
    /// The submitting caller's verified effective authority.
    authority: &'a AcceptedAuthority,
}

/// The identities of the execution a result belongs to.
///
/// A borrowed view of the fields result rows record, separated from the
/// lease so the mapping can be exercised without a claim.
#[derive(Debug, Clone, Copy)]
pub struct ResultRun<'a> {
    /// The Verifier execution, recorded as `execution_id`. It correlates
    /// result rows with their execution; each built payload mints its own
    /// result identity.
    pub execution_id: ExecutionId,
    /// The application Run judged, stored as the managed `run_id`; `None`
    /// when there is none, as for scheduled drift.
    pub run_id: Option<&'a RunId>,
    /// Exact Verifier Card version used.
    pub verifier_version: &'a str,
    /// The Card verified, stored as the managed `card_uid`.
    pub card_uid: &'a CardUid,
    /// Binding owner; `None` for an execution with no binding.
    pub owner_card_uid: Option<&'a CardUid>,
    /// Binding; `None` for an execution with no binding.
    pub binding_id: Option<BindingId>,
    /// Frozen Trigger; `None` for an execution with no binding.
    pub trigger: Option<&'a FrozenTarget>,
    /// The scheduled sample window; `None` for Run evidence.
    pub window: Option<&'a DriftWindow>,
}

/// One `vala.drift.result_features` row before encoding.
struct DriftRow<'r> {
    /// The drift requirement that scored the feature.
    requirement: &'r str,
    /// The requirement's report, which names the method.
    report: &'r DriftReport,
    /// The window the feature was scored over.
    window: DriftWindow,
    /// The scored feature.
    feature: &'r FeatureDriftReport,
}

/// Maps one completed execution's outcome onto its result batches.
///
/// Owns the identities every row repeats — result, execution, Verifier,
/// subject, owner, binding — plus the execution interval, so each table
/// mapping reads them from one place.
#[derive(Debug, Clone, Copy)]
pub struct ResultPayloadBuilder<'a> {
    /// The execution whose identities the rows record.
    run: ResultRun<'a>,
    /// The Verifier's UID, recorded as `verifier_card_uid`.
    verifier_uid: &'a CardUid,
    /// Verifier execution start.
    started_at: DateTime<Utc>,
    /// Verifier execution end.
    ended_at: DateTime<Utc>,
}

impl<'a> ResultPayloadBuilder<'a> {
    /// Build a mapper for the one result of `run`.
    #[must_use]
    pub fn new(
        run: ResultRun<'a>,
        verifier_uid: &'a CardUid,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
    ) -> Self {
        Self {
            run,
            verifier_uid,
            started_at,
            ended_at,
        }
    }

    /// Map `outcome` to its ordered batches under a newly minted result
    /// identity.
    ///
    /// Each call produces a distinct [`VerificationResultId`], carried by
    /// every row of the payload, so two attempts of one execution never share
    /// a result identity; the execution correlates them through
    /// `execution_id`. Retrying the staged payload resends the same rows and
    /// identity. Every drift requirement writes one `result_features` row per scored
    /// feature, keyed by its `requirement_id`, all in one batch before the
    /// summary. The one `results` row records the overall verdict and the
    /// whole outcome as the `outcome` Variant.
    ///
    /// # Errors
    /// Returns [`ResultPayloadError::ColumnMismatch`] when authored columns
    /// do not name each table field exactly once,
    /// [`ResultPayloadError::Variant`] when the outcome exceeds a Variant
    /// limit, and a JSON or Arrow error when encoding fails.
    pub fn build(
        &self,
        outcome: &VerificationOutcome,
    ) -> Result<ResultPayload, ResultPayloadError> {
        let result_id = VerificationResultId::new_v7();
        let mut batches = Vec::with_capacity(2);
        if let Some(features) = self.drift_features(result_id, &outcome.requirements)? {
            batches.push(self.finish::<ResultFeaturesTable>(features)?);
        }
        batches.push(self.finish::<ResultsTable>(self.summary(result_id, outcome)?)?);
        Ok(ResultPayload {
            result_id,
            verdict: outcome.verdict,
            batches,
        })
    }

    /// Author the named columns of the one `vala.verification.results` row
    /// identified by `result_id`.
    ///
    /// # Errors
    /// Returns a JSON error when the Trigger or outcome cannot be encoded and
    /// [`ResultPayloadError::Variant`] when the outcome exceeds a Variant
    /// limit.
    fn summary(
        &self,
        result_id: VerificationResultId,
        outcome: &VerificationOutcome,
    ) -> Result<Vec<NamedColumn>, ResultPayloadError> {
        let trigger = self.run.trigger.map(canonical_json).transpose()?;
        let verdict: &'static str = outcome.verdict.into();
        let outcome = serde_json::to_value(outcome)?;
        let window = self.run.window;
        Ok(vec![
            ("result_id", text([Some(result_id.to_string())])),
            (
                "execution_id",
                text([Some(self.run.execution_id.to_string())]),
            ),
            ("execution_status", text([Some("completed")])),
            ("verdict", text([Some(verdict)])),
            ("verifier_version", text([Some(self.run.verifier_version)])),
            ("owner_card_uid", text([self.owner()])),
            (
                "verifier_card_uid",
                text([Some(self.verifier_uid.as_str())]),
            ),
            ("binding_id", text([self.binding()])),
            ("trigger_identity", text([trigger])),
            (
                "window_start",
                timestamps([window.map(|window| window.start)]),
            ),
            ("window_end", timestamps([window.map(|window| window.end)])),
            ("started_at", timestamps([Some(self.started_at)])),
            ("ended_at", timestamps([Some(self.ended_at)])),
            (
                OUTCOME,
                VariantColumnBuilder::encode(OUTCOME, [Some(&outcome)], EncodedVariant::from_json)?,
            ),
        ])
    }

    /// Map the execution's pinned `inputs` to its one
    /// `vala.verification.execution_inputs` row, recorded at `started_at`.
    ///
    /// A `submission` marks the row `queue_execution` and records its
    /// accepted request context — route overrides, idempotency key and
    /// request digest, and the accepted authority — as the `activation`
    /// Variant; a direct execution passes `None` and its row can never queue
    /// work.
    ///
    /// # Errors
    /// Returns a JSON or Variant error when the inputs or activation cannot
    /// be encoded, and the [`Self::finish`] errors.
    pub fn inputs(
        &self,
        inputs: &BTreeMap<SlotName, InputDeclaration>,
        submission: Option<&Submission>,
    ) -> Result<ResultBatch, ResultPayloadError> {
        let activation = submission
            .map(|submission| {
                serde_json::to_value(Activation {
                    route_overrides: &submission.request.execution_route_overrides,
                    idempotency_key: submission.key.as_ref().map(|key| key.key.as_str()),
                    request_sha256: submission
                        .key
                        .as_ref()
                        .map(|key| hex::encode(key.request_sha256)),
                    authority: &submission.authority,
                })
            })
            .transpose()?;
        let inputs = serde_json::to_value(inputs)?;
        self.finish::<ExecutionInputsTable>(vec![
            (
                "execution_id",
                text([Some(self.run.execution_id.to_string())]),
            ),
            (
                "verifier_card_uid",
                text([Some(self.verifier_uid.as_str())]),
            ),
            ("binding_id", text([self.binding()])),
            (
                "queue_execution",
                Arc::new(BooleanArray::from(vec![submission.is_some()])),
            ),
            (
                INPUTS,
                VariantColumnBuilder::encode(INPUTS, [Some(&inputs)], EncodedVariant::from_json)?,
            ),
            (
                ACTIVATION,
                VariantColumnBuilder::encode(
                    ACTIVATION,
                    [activation.as_ref()],
                    EncodedVariant::from_json,
                )?,
            ),
            ("recorded_at", timestamps([Some(self.started_at)])),
        ])
    }

    /// Author the named `vala.drift.result_features` columns: one row per
    /// scored feature of every drift requirement, each keyed by `result_id`.
    ///
    /// A row's window is its requirement's scored window, else the
    /// execution's scheduled window, else the execution interval. Returns
    /// `None` when no requirement scored a feature, so no empty batch is ever
    /// written. A non-finite score or threshold becomes null.
    ///
    /// # Errors
    /// Returns a JSON error when a method or verdict cannot be encoded.
    fn drift_features(
        &self,
        result_id: VerificationResultId,
        requirements: &[RequirementResult],
    ) -> Result<Option<Vec<NamedColumn>>, ResultPayloadError> {
        let mut rows: Vec<DriftRow<'_>> = Vec::new();
        for requirement in requirements {
            let Some(RequirementDetail::Drift(report)) = &requirement.detail else {
                continue;
            };
            let window = match &requirement.provenance {
                Some(CallProvenance::Drift {
                    window: Some(window),
                }) => *window,
                _ => self.run.window.copied().unwrap_or(DriftWindow {
                    start: self.started_at,
                    end: self.ended_at,
                }),
            };
            rows.extend(report.features.values().map(|feature| DriftRow {
                requirement: requirement.id.as_str(),
                report,
                window,
                feature,
            }));
        }
        if rows.is_empty() {
            return Ok(None);
        }
        let count = rows.len();
        let methods = rows
            .iter()
            .map(|row| serialized_name(&row.report.method))
            .collect::<Result<Vec<_>, _>>()?;
        let verdicts = rows
            .iter()
            .map(|row| Some(drift_verdict(row.feature.verdict)));
        Ok(Some(vec![
            (
                "result_id",
                text(std::iter::repeat_n(Some(result_id.to_string()), count)),
            ),
            (
                "requirement_id",
                text(rows.iter().map(|row| Some(row.requirement))),
            ),
            (
                "owner_card_uid",
                text(std::iter::repeat_n(self.owner(), count)),
            ),
            (
                "verifier_card_uid",
                text(std::iter::repeat_n(Some(self.verifier_uid.as_str()), count)),
            ),
            (
                "binding_id",
                text(std::iter::repeat_n(self.binding(), count)),
            ),
            (
                "window_start",
                timestamps(rows.iter().map(|row| Some(row.window.start))),
            ),
            (
                "window_end",
                timestamps(rows.iter().map(|row| Some(row.window.end))),
            ),
            ("method", text(methods)),
            (
                "feature",
                text(rows.iter().map(|row| Some(row.feature.feature.as_str()))),
            ),
            (
                "score",
                Arc::new(
                    rows.iter()
                        .map(|row| row.feature.score.filter(|score| score.is_finite()))
                        .collect::<Float64Array>(),
                ),
            ),
            (
                "threshold",
                Arc::new(
                    rows.iter()
                        .map(|row| row.feature.threshold.filter(|value| value.is_finite()))
                        .collect::<Float64Array>(),
                ),
            ),
            ("verdict", text(verdicts)),
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
    /// schema is `fields` followed by `card_uid` and `run_id`, each repeated
    /// for the authored row count.
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
        let mut arrays = Vec::with_capacity(fields.len() + 2);
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
            Field::new(CARD_UID, DataType::Utf8, true),
            Field::new(RUN_ID, DataType::Utf8, true),
        ]);
        arrays.push(text(std::iter::repeat_n(
            Some(self.run.card_uid.as_str()),
            rows,
        )));
        arrays.push(text(std::iter::repeat_n(
            self.run.run_id.map(RunId::as_str),
            rows,
        )));
        Ok(RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)?)
    }

    /// The binding owner Card UID, null for an execution with no binding.
    fn owner(&self) -> Option<String> {
        self.run.owner_card_uid.map(ToString::to_string)
    }

    /// The binding identity, null for an execution with no binding.
    fn binding(&self) -> Option<String> {
        self.run.binding_id.map(|binding| binding.to_string())
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

/// The serialized name of a unit enum value, such as `Psi` or `NoDrift`.
///
/// # Errors
/// Returns a JSON error when the value cannot be serialized.
fn serialized_name<T: Serialize>(value: &T) -> Result<Option<String>, JsonError> {
    Ok(serde_json::to_value(value)?.as_str().map(str::to_owned))
}

/// The stored value of a Drift feature verdict: the snake_case spelling the
/// published `vala.drift.result_features.verdict` column carries, matching
/// `vala.verification.results.verdict`.
const fn drift_verdict(verdict: DriftVerdict) -> &'static str {
    match verdict {
        DriftVerdict::NoDrift => "no_drift",
        DriftVerdict::Drift => "drift",
        DriftVerdict::Inconclusive => "inconclusive",
    }
}

/// Build a nullable UTF-8 column.
fn text<S: AsRef<str>>(values: impl IntoIterator<Item = Option<S>>) -> ArrayRef {
    Arc::new(StringArray::from_iter(values))
}

/// Build a nullable microsecond timestamp column in the stored
/// [`UTC_TIME_ZONE`] spelling every built-in result table declares.
fn timestamps(values: impl IntoIterator<Item = Option<DateTime<Utc>>>) -> ArrayRef {
    Arc::new(
        TimestampMicrosecondArray::from_iter(
            values
                .into_iter()
                .map(|value| value.map(|at| at.timestamp_micros())),
        )
        .with_timezone(UTC_TIME_ZONE),
    )
}

#[cfg(test)]
mod tests {
    //! Outcome-to-row mapping of the result tables.

    use std::collections::BTreeMap;

    use arrow::array::{Array, AsArray};
    use arrow::datatypes::Float64Type;
    use chrono::TimeZone;
    use serde_json::Value;
    use wyrd_spec::card::drift::DriftMethod;
    use wyrd_spec::card::verifier::Check;
    use wyrd_spec::ids::FeatureName;
    use wyrd_spec::verification::TaskId;
    use wyrd_spec::verification::execution::RequirementOutcome;

    use super::*;

    /// Owned identities of one binding execution, borrowed as a [`ResultRun`].
    struct TestRun {
        /// The execution.
        execution_id: ExecutionId,
        /// The application Run judged.
        run_id: RunId,
        /// Subject Card.
        subject: CardUid,
        /// Binding owner Card.
        owner: CardUid,
        /// The Verifier the result is written under.
        verifier: CardUid,
        /// Binding identity.
        binding: BindingId,
        /// Frozen inline Trigger.
        trigger: FrozenTarget,
        /// Scheduled window.
        window: DriftWindow,
    }

    impl TestRun {
        /// A binding execution over one fixed day.
        fn new() -> Self {
            Self {
                execution_id: ExecutionId::new_v7(),
                run_id: RunId::new(),
                subject: uid(),
                owner: uid(),
                verifier: uid(),
                binding: BindingId::new_v7(),
                trigger: FrozenTarget::Digest("sha256:t".to_owned()),
                window: DriftWindow {
                    start: at(-86_400),
                    end: at(0),
                },
            }
        }

        /// A builder for this execution with fixed times.
        fn builder(&self) -> ResultPayloadBuilder<'_> {
            ResultPayloadBuilder::new(
                ResultRun {
                    execution_id: self.execution_id,
                    run_id: Some(&self.run_id),
                    verifier_version: "1.2.0",
                    card_uid: &self.subject,
                    owner_card_uid: Some(&self.owner),
                    binding_id: Some(self.binding),
                    trigger: Some(&self.trigger),
                    window: Some(&self.window),
                },
                &self.verifier,
                at(10),
                at(20),
            )
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

    /// A failed outcome with one two-feature drift requirement, one with a
    /// non-finite score, and one assertion requirement with no detail.
    fn outcome() -> VerificationOutcome {
        let feature = |name: &str, score: f64, verdict| {
            let name = FeatureName::new(name).expect("valid feature name");
            (
                name.clone(),
                FeatureDriftReport {
                    feature: name,
                    score: Some(score),
                    threshold: Some(0.2),
                    verdict,
                    evidence: None,
                },
            )
        };
        let requirement = |id: &str, detail| RequirementResult {
            id: TaskId::new(id).expect("valid task id"),
            step: None,
            statement: id.to_owned(),
            outcome: RequirementOutcome::Failed,
            reason: "drifted".to_owned(),
            actual: None,
            citations: Vec::new(),
            detail,
            check: serde_json::from_value::<Check>(serde_json::json!({
                "kind": "assert", "path": "inputs.x", "operator": "equals", "expected": true
            }))
            .expect("assert check decodes"),
            provenance: None,
        };
        VerificationOutcome {
            verdict: VerificationVerdict::Failed,
            requirements: vec![
                requirement(
                    "income_stable",
                    Some(RequirementDetail::Drift(DriftReport {
                        method: DriftMethod::Psi,
                        features: BTreeMap::from([
                            feature("dti", f64::NAN, DriftVerdict::Inconclusive),
                            feature("income", 0.4, DriftVerdict::Drift),
                        ]),
                        verdict: DriftVerdict::Drift,
                    })),
                ),
                requirement("asserted", None),
            ],
            steps: Vec::new(),
            evidence: BTreeMap::new(),
        }
    }

    /// Read one UTF-8 cell.
    fn cell(batch: &RecordBatch, column: &str, row: usize) -> Option<String> {
        let array = batch
            .column_by_name(column)
            .unwrap_or_else(|| panic!("{column} column exists"))
            .as_string::<i32>();
        array.is_valid(row).then(|| array.value(row).to_owned())
    }

    /// Drift feature rows carry their requirement and precede the summary,
    /// a non-finite score is null, and the summary stores the whole outcome;
    /// every row carries the payload's result identity, and the summary also
    /// names the execution.
    ///
    /// # Panics
    /// Panics when the outcome maps to other rows.
    #[test]
    fn outcome_writes_requirement_features_then_summary() {
        let run = TestRun::new();
        let outcome = outcome();
        let payload = run.builder().build(&outcome).expect("outcome maps");
        let [features, summary] = payload.batches() else {
            panic!("features then summary");
        };
        assert_eq!(features.table, "vala.drift.result_features");
        assert_eq!(summary.table, "vala.verification.results");
        let rows = &features.batch;
        assert_eq!(rows.num_rows(), 2);
        for row in 0..2 {
            assert_eq!(
                cell(rows, "requirement_id", row).as_deref(),
                Some("income_stable")
            );
            assert_eq!(cell(rows, "method", row).as_deref(), Some("Psi"));
            assert_eq!(
                cell(rows, "result_id", row),
                Some(payload.result_id().to_string())
            );
        }
        assert_eq!(cell(rows, "feature", 0).as_deref(), Some("dti"));
        assert_eq!(cell(rows, "verdict", 1).as_deref(), Some("drift"));
        let scores = rows
            .column_by_name("score")
            .expect("score column")
            .as_primitive::<Float64Type>();
        assert!(scores.is_null(0));
        assert!((scores.value(1) - 0.4).abs() < f64::EPSILON);
        let summary = &summary.batch;
        let execution = run.execution_id.to_string();
        assert_eq!(
            cell(summary, "result_id", 0),
            Some(payload.result_id().to_string())
        );
        assert_eq!(cell(summary, "execution_id", 0), Some(execution));
        assert_eq!(cell(summary, "verdict", 0).as_deref(), Some("failed"));
        assert_eq!(
            cell(summary, RUN_ID, 0).as_deref(),
            Some(run.run_id.as_str())
        );
        let stored = wyrd_queue::variant::variant_cell_to_json(
            summary
                .column_by_name(OUTCOME)
                .expect("outcome column")
                .as_ref(),
            0,
        )
        .expect("outcome Variant decodes");
        let mut expected = serde_json::to_value(&outcome).expect("outcome encodes");
        expected["requirements"][0]["detail"]["drift"]["features"]["dti"]["score"] = Value::Null;
        assert_eq!(stored, expected);
    }

    /// An outcome with no drift detail writes only the summary.
    ///
    /// # Panics
    /// Panics when a feature batch is written.
    #[test]
    fn outcome_without_drift_writes_only_the_summary() {
        let run = TestRun::new();
        let mut outcome = outcome();
        outcome.requirements.remove(0);
        let payload = run.builder().build(&outcome).expect("outcome maps");
        assert_eq!(payload.batches().len(), 1);
        assert_eq!(payload.verdict(), VerificationVerdict::Failed);
    }

    /// Two attempts of one execution each mint their own result identity
    /// while naming the same execution, so a reclaimed attempt's result can
    /// never be read in place of the winning one.
    ///
    /// # Panics
    /// Panics when the two payloads share a result identity or name
    /// different executions.
    #[test]
    fn each_built_payload_mints_its_own_result_identity() {
        let run = TestRun::new();
        let outcome = outcome();
        let first = run.builder().build(&outcome).expect("outcome maps");
        let second = run.builder().build(&outcome).expect("outcome maps");
        assert_ne!(first.result_id(), second.result_id());
        let execution = |payload: &ResultPayload| {
            let summary = payload.batches().last().expect("a summary batch");
            cell(&summary.batch, "execution_id", 0)
        };
        assert_eq!(execution(&first), Some(run.execution_id.to_string()));
        assert_eq!(execution(&first), execution(&second));
    }

    /// One refusal case: the expected problem, the mutation applied to valid
    /// summary columns, and the column name the refusal reports.
    type ColumnCase = (&'static str, fn(&mut Vec<NamedColumn>), &'static str);

    /// Authored columns that omit, repeat, or invent a table field name are
    /// refused with the named problem before any batch is assembled.
    ///
    /// # Panics
    /// Panics when a mismatch is not refused as named.
    #[test]
    fn missing_duplicate_and_unexpected_columns_are_refused() {
        let run = TestRun::new();
        let builder = run.builder();
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
            let mut columns = builder
                .summary(VerificationResultId::new_v7(), &outcome())
                .expect("summary authors");
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
