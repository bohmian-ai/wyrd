//! Direct execution: one exact Verifier judged inline over supplied input.
//!
//! [`DirectExecutor`] runs the same Vala scorers and the same Eval execution
//! path as queued runs, but over input carried in the request instead of
//! Bifrost evidence. It never reads or writes Bifrost, creates a run, samples,
//! publishes, or dispatches: PostgreSQL is touched only for the fitted
//! baseline and judge Agent/Prompt resolution. Every refusal is the stable
//! [`WyrdError`] the HTTP and MCP surfaces return.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{ArrayRef, Float64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use chrono::Utc;
use serde_json::{Value, json};
use vala_drift::{DriftReport, FittedBaseline, score_drift};
use vala_eval::executor::{EvalReport, SkipReason, TaskRunOutcome};
use vala_eval::{EvalExecError, InMemoryTraceSource};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::{DriftProfile, DriftSpec};
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, VerificationExecutionId};
use wyrd_spec::vala::eval::EvalTask;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::ids::{RecordId, RunId};
use wyrd_spec::verification::{
    DirectVerificationInput, DriftSample, EvalReport as WireEvalReport,
    SkipReason as WireSkipReason, SkippedTask, VerificationExecutionDetail,
};

use super::drift::{BASELINE_LEGACY, BASELINE_NOT_READY, FittedBaselines};
use super::engines::{EngineOutcome, VerifierReport};
use super::eval::{EvalEngine, ScoreFailure};
use super::telemetry::ExecutionTelemetry;
use crate::state::AppState;

pub use wyrd_spec::verification::EXECUTION_DEADLINE;

/// Owner of the engine half of a direct execution.
///
/// Built per request from the process state: it holds the fitted-baseline
/// loader and the Eval engine judging through the state's judge providers.
pub struct DirectExecutor {
    /// Ready fitted baselines of PSI and SPC Verifiers.
    baselines: FittedBaselines,
    /// The one Eval execution path, shared with queued runs.
    eval: EvalEngine,
}

impl DirectExecutor {
    /// Build an executor over `state`.
    #[must_use]
    pub fn new(state: &AppState) -> Self {
        Self {
            baselines: FittedBaselines::new(state.clone()),
            eval: EvalEngine::new(
                state.clone(),
                Arc::clone(&state.judge_providers),
                Duration::ZERO,
            ),
        }
    }

    /// Judge `input` with `implementation` of `verifier_uid` for `tenant`.
    ///
    /// Drift scores the supplied columns with the shared `vala-drift` scorer;
    /// Eval scores one synthetic record through the queued Eval path.
    /// Preparation — baseline load, input conversion, plan construction —
    /// is the `prepare` phase on `telemetry`.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInputIncompatible`] when the input
    /// does not fit the Verifier, [`WyrdError::VerificationInputUnsupported`]
    /// for a Verifier direct execution cannot run,
    /// [`WyrdError::VerificationInputInvalid`] for a column mixing numbers
    /// and strings, the baseline readiness errors of
    /// [`DirectExecutor::baseline`], and the Eval errors of
    /// [`DirectExecutor::eval`].
    pub async fn execute(
        &self,
        tenant: DataTenantId,
        execution_id: VerificationExecutionId,
        verifier_uid: &CardUid,
        implementation: &VerifierImplementation,
        input: &DirectVerificationInput,
        telemetry: &ExecutionTelemetry,
    ) -> Result<VerifierReport, WyrdError> {
        match (implementation, input) {
            (
                VerifierImplementation::Drift(spec),
                DirectVerificationInput::DriftSamples { columns },
            ) => {
                self.drift(tenant, verifier_uid, spec, columns, telemetry)
                    .await
            }
            (
                VerifierImplementation::Eval(spec),
                DirectVerificationInput::EvalRecord { context, media },
            ) => {
                let record = EvalRecordObservation {
                    record_id: RecordId(execution_id.as_uuid()),
                    session_id: None,
                    context: Value::Object(context.clone()),
                    trace_id: None,
                    span_id: None,
                    created_at: Utc::now(),
                    media: media.clone(),
                };
                self.eval(tenant, execution_id, spec, &record, telemetry)
                    .await
            }
            (VerifierImplementation::Drift(_), _) => {
                Err(incompatible("a Drift Verifier judges drift_samples input"))
            }
            (VerifierImplementation::Eval(_), _) => {
                Err(incompatible("an Eval Verifier judges eval_record input"))
            }
        }
    }

    /// Score supplied Drift `columns` under `spec`.
    ///
    /// A null in any scored feature leaves the samples unscorable, exactly as
    /// an incomplete queued window: the report is inconclusive with no
    /// features rather than dropping or imputing values.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInputUnsupported`] for a profile-less
    /// spec, [`WyrdError::VerificationInputIncompatible`] for a missing
    /// feature or a scorer refusal, [`WyrdError::VerificationInputInvalid`]
    /// for a mixed-type column, and the errors of [`DirectExecutor::baseline`].
    async fn drift(
        &self,
        tenant: DataTenantId,
        verifier_uid: &CardUid,
        spec: &DriftSpec,
        columns: &std::collections::BTreeMap<String, Vec<Option<DriftSample>>>,
        telemetry: &ExecutionTelemetry,
    ) -> Result<VerifierReport, WyrdError> {
        let prepared = telemetry
            .prepare(async {
                let (baseline, features): (FittedBaseline, Vec<String>) = match &spec.profile {
                    Some(DriftProfile::Custom(profile)) => {
                        (FittedBaseline::Custom, vec![profile.metric_name.clone()])
                    }
                    Some(DriftProfile::Psi(_) | DriftProfile::Spc(_)) => {
                        let baseline = self.baseline(tenant, verifier_uid, telemetry).await?;
                        let features = match &baseline {
                            FittedBaseline::Psi(fitted) => {
                                fitted.features.keys().map(ToString::to_string).collect()
                            }
                            FittedBaseline::Spc(fitted) => {
                                fitted.features.keys().map(ToString::to_string).collect()
                            }
                            FittedBaseline::Custom => Vec::new(),
                        };
                        (baseline, features)
                    }
                    None => {
                        return Err(WyrdError::VerificationInputUnsupported {
                            message: "the Drift Verifier has no profile".to_owned(),
                            details: json!({}),
                        });
                    }
                };
                let mut complete = true;
                for feature in &features {
                    let values = columns.get(feature).ok_or_else(|| {
                        WyrdError::VerificationInputIncompatible {
                            message: format!("drift_samples lacks feature `{feature}`"),
                            details: json!({ "feature": feature }),
                        }
                    })?;
                    complete &= values.iter().all(Option::is_some);
                }
                let batch = complete.then(|| samples_batch(columns)).transpose()?;
                Ok((baseline, batch))
            })
            .await?;
        let report = match prepared {
            (_, None) => DriftReport::unscored(spec.method),
            (baseline, Some(batch)) => {
                let _score = tracing::info_span!("verification.score").entered();
                score_drift(&baseline, &batch, spec).map_err(|error| {
                    WyrdError::VerificationInputIncompatible {
                        message: format!("drift_samples cannot be scored: {error}"),
                        details: json!({}),
                    }
                })?
            }
        };
        Ok(VerifierReport::Drift(Some(report)))
    }

    /// Load the ready fitted baseline of `verifier_uid`.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationBaselineNotReady`] when none is ready,
    /// [`WyrdError::VerificationBaselineLegacy`] for an earlier format, a
    /// registry unavailability error when the read fails, and
    /// [`WyrdError::Internal`] when the stored baseline does not decode.
    async fn baseline(
        &self,
        tenant: DataTenantId,
        verifier_uid: &CardUid,
        telemetry: &ExecutionTelemetry,
    ) -> Result<FittedBaseline, WyrdError> {
        self.baselines
            .load(tenant, verifier_uid, telemetry)
            .await
            .map_err(|outcome| match outcome {
                EngineOutcome::Terminal(_, error) if error.code == BASELINE_NOT_READY => {
                    WyrdError::VerificationBaselineNotReady {
                        message: error.message,
                        details: json!({ "verifier_uid": verifier_uid }),
                    }
                }
                EngineOutcome::Terminal(_, error) if error.code == BASELINE_LEGACY => {
                    WyrdError::VerificationBaselineLegacy {
                        message: error.message,
                        details: json!({ "verifier_uid": verifier_uid }),
                    }
                }
                EngineOutcome::Retry(_) => {
                    WyrdError::registry_unavailable("the fitted baseline cannot be read")
                }
                _ => WyrdError::Internal {
                    message: "the fitted baseline cannot be decoded".to_owned(),
                    details: json!({}),
                },
            })
    }

    /// Score one supplied Eval `record` under `spec`.
    ///
    /// A spec with trace or agent assertions is refused before any task runs:
    /// direct input carries neither a trace nor an agent run.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInputUnsupported`] for trace or agent
    /// assertions, [`WyrdError::VerificationDependencyFailed`] when a judge
    /// fails after its own retries,
    /// [`WyrdError::VerificationInputIncompatible`] when a task cannot
    /// evaluate the record, and [`WyrdError::Internal`] for an unplannable
    /// spec or an uncapturable report.
    async fn eval(
        &self,
        tenant: DataTenantId,
        execution_id: VerificationExecutionId,
        spec: &EvalSpec,
        record: &EvalRecordObservation,
        telemetry: &ExecutionTelemetry,
    ) -> Result<VerifierReport, WyrdError> {
        if spec.tasks.values().any(|task| {
            matches!(
                task,
                EvalTask::TraceAssertion(_) | EvalTask::AgentAssertion(_)
            )
        }) {
            return Err(WyrdError::VerificationInputUnsupported {
                message: "direct execution cannot run trace or agent assertions".to_owned(),
                details: json!({}),
            });
        }
        let run = RunId::from_string(execution_id.to_string());
        self.eval
            .judge_record(
                tenant,
                run,
                spec,
                record,
                InMemoryTraceSource::new(),
                telemetry,
            )
            .await
            .map_err(|failure| match failure {
                ScoreFailure::Execute(
                    error @ (EvalExecError::JudgeUnavailable { .. }
                    | EvalExecError::JudgeRetriesExhausted { .. }
                    | EvalExecError::JudgeInvalidOutput { .. }),
                ) => {
                    tracing::warn!(%execution_id, cause = %error, "direct Eval judge failed");
                    WyrdError::VerificationDependencyFailed {
                        message: "the judge provider failed after the task's retries".to_owned(),
                        details: json!({}),
                    }
                }
                ScoreFailure::Execute(error) => incompatible(&error.to_string()),
                ScoreFailure::Plan(error) => {
                    tracing::warn!(%execution_id, cause = %error, "direct Eval plan failed");
                    WyrdError::Internal {
                        message: "the Eval spec cannot be planned".to_owned(),
                        details: json!({}),
                    }
                }
                ScoreFailure::Capture(error) => {
                    tracing::warn!(%execution_id, cause = %error, "direct Eval capture failed");
                    WyrdError::Internal {
                        message: "the Eval report cannot be captured".to_owned(),
                        details: json!({}),
                    }
                }
            })
    }
}

/// The 422 refusal of input that does not fit the Verifier.
fn incompatible(message: &str) -> WyrdError {
    WyrdError::VerificationInputIncompatible {
        message: message.to_owned(),
        details: json!({}),
    }
}

/// Build one Arrow batch with a column per supplied feature.
///
/// A column whose values are all numbers (or null) is `Float64`; all strings
/// (or null) is `Utf8`; an all-null column is a null `Float64`.
///
/// # Errors
/// Returns [`WyrdError::VerificationInputInvalid`] for a column mixing numbers
/// and strings, or columns of different lengths.
fn samples_batch(
    columns: &std::collections::BTreeMap<String, Vec<Option<DriftSample>>>,
) -> Result<RecordBatch, WyrdError> {
    let invalid = |message: String| WyrdError::VerificationInputInvalid {
        message,
        details: json!({}),
    };
    let mut fields = Vec::with_capacity(columns.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(columns.len());
    for (name, values) in columns {
        let text = values
            .iter()
            .flatten()
            .any(|value| matches!(value, DriftSample::Text(_)));
        let array: ArrayRef = if text {
            let strings = values
                .iter()
                .map(|value| match value {
                    None => Ok(None),
                    Some(DriftSample::Text(text)) => Ok(Some(text.as_str())),
                    Some(DriftSample::Number(_)) => Err(invalid(format!(
                        "drift_samples column `{name}` mixes numbers and strings"
                    ))),
                })
                .collect::<Result<StringArray, _>>()?;
            Arc::new(strings)
        } else {
            Arc::new(
                values
                    .iter()
                    .map(|value| match value {
                        Some(DriftSample::Number(number)) => Some(*number),
                        _ => None,
                    })
                    .collect::<Float64Array>(),
            )
        };
        let data_type = if text {
            DataType::Utf8
        } else {
            DataType::Float64
        };
        fields.push(Field::new(name, data_type, true));
        arrays.push(array);
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)
        .map_err(|error| invalid(format!("drift_samples columns are invalid: {error}")))
}

/// The wire detail of a direct execution's `report`.
///
/// Drift is the Vala report in its serialized spelling, read back into the
/// typed wire report; a non-finite score or threshold of an inconclusive
/// feature serializes as an absent value. Eval lists the ran assertion
/// results and the skipped tasks with their reasons.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the Drift report does not encode into
/// the wire report.
pub fn detail(report: &VerifierReport) -> Result<VerificationExecutionDetail, WyrdError> {
    Ok(match report {
        VerifierReport::Drift(report) => VerificationExecutionDetail::Drift(
            serde_json::to_value(report)
                .and_then(serde_json::from_value)
                .map_err(|error| WyrdError::Internal {
                    message: format!("the verification report cannot be encoded: {error}"),
                    details: json!({}),
                })?,
        ),
        VerifierReport::Eval { report, .. } => {
            VerificationExecutionDetail::Eval(eval_detail(report))
        }
    })
}

/// Project an Eval report into its ran results and skipped tasks.
fn eval_detail(report: &EvalReport) -> WireEvalReport {
    WireEvalReport {
        results: report.ran().cloned().collect(),
        skipped: report
            .outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                TaskRunOutcome::Skipped { task_id, reason } => Some(match reason {
                    SkipReason::ConditionFalse => SkippedTask {
                        task_id: task_id.clone(),
                        reason: WireSkipReason::ConditionFalse,
                        upstream_task_id: None,
                    },
                    SkipReason::DependencySkipped { upstream } => SkippedTask {
                        task_id: task_id.clone(),
                        reason: WireSkipReason::DependencySkipped,
                        upstream_task_id: Some(upstream.clone()),
                    },
                }),
                TaskRunOutcome::Ran(_) => None,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    //! Pure conversion of supplied samples and reports.

    use std::collections::BTreeMap;

    use super::*;

    /// Numbers become `Float64`, strings `Utf8`, and a mixed column is
    /// refused with the stable invalid-input code.
    ///
    /// # Panics
    /// Panics when a column converts to the wrong type or a mixed column passes.
    #[test]
    fn samples_convert_by_column_type_and_refuse_mixed_columns() {
        let columns = BTreeMap::from([
            (
                "score".to_owned(),
                vec![Some(DriftSample::Number(1.5)), None],
            ),
            (
                "region".to_owned(),
                vec![Some(DriftSample::Text("eu".to_owned())), None],
            ),
        ]);
        let batch = samples_batch(&columns).expect("typed columns convert");
        assert_eq!(
            batch
                .schema()
                .field_with_name("score")
                .map(|f| f.data_type().clone())
                .ok(),
            Some(DataType::Float64)
        );
        assert_eq!(
            batch
                .schema()
                .field_with_name("region")
                .map(|f| f.data_type().clone())
                .ok(),
            Some(DataType::Utf8)
        );
        let mixed = BTreeMap::from([(
            "x".to_owned(),
            vec![
                Some(DriftSample::Number(1.0)),
                Some(DriftSample::Text("a".to_owned())),
            ],
        )]);
        assert_eq!(
            samples_batch(&mixed)
                .expect_err("mixed column refused")
                .code(),
            "WYRD_VERIFICATION_400_INPUT_INVALID"
        );
    }
}
