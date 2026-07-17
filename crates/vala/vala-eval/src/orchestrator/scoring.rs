//! Scenario and run scoring hand-off into the eval engine.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use wyrd_spec::reference::{CardRef, Ref};
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{EvalSpec, ScenarioId};

use crate::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, MediaBindings, TraceTaskExecutor,
};
use crate::{
    AggregationInput, EvalResults, Executors, InMemoryTraceSource, JudgeInvoker,
    MechanicSubjectInput, RecordTaskResult, RecordWithMedia, ResultsConfig, RunIdentity,
    ScenarioAggregationInput, ScenarioExecutionInputs, ScenarioExecutionResults, SubjectKey,
    TaskRegistry, TaskRunOutcome, TaskSummary, TraceSource, aggregate_run, execute_plan,
    execute_scenario,
};

use super::{OrchestratorError, ScenarioCursor};

/// Engine wrapper used by embedded and HTTP orchestration.
pub struct ScenarioScoring {
    spec: Arc<EvalSpec>,
    plan: wyrd_spec::vala::eval::ExecutionPlan,
    registry: TaskRegistry,
    executors: Executors,
}

impl ScenarioScoring {
    /// Construct a scoring engine from an eval spec and injected runtime deps.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when the eval DAG or task registry is invalid.
    pub fn new(
        spec: Arc<EvalSpec>,
        judge: Arc<dyn JudgeInvoker>,
        traces: Arc<dyn TraceSource>,
        trace_fetch_deadline: Duration,
    ) -> Result<Self, OrchestratorError> {
        let plan = spec
            .execution_plan()
            .map_err(|error| OrchestratorError::Plan {
                reason: error.to_string(),
            })?;
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone())?;
        let executors = Executors {
            assertion: Arc::new(AssertionTaskExecutor::new()),
            judge: Arc::new(JudgeTaskExecutor::new(judge)),
            trace: Arc::new(TraceTaskExecutor::new(traces, trace_fetch_deadline)),
            agent: Arc::new(AgentTaskExecutor::new()),
        };
        Ok(Self {
            spec,
            plan,
            registry,
            executors,
        })
    }

    /// Construct a scoring engine with empty trace source.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when planning fails.
    pub fn with_in_memory_traces(
        spec: Arc<EvalSpec>,
        judge: Arc<dyn JudgeInvoker>,
    ) -> Result<Self, OrchestratorError> {
        Self::new(
            spec,
            judge,
            Arc::new(InMemoryTraceSource::new()),
            Duration::from_millis(250),
        )
    }

    /// Execute mechanic and passenger scoring for one completed scenario.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when engine scoring fails.
    pub async fn score_scenario(
        &self,
        cursor: &ScenarioCursor,
    ) -> Result<ScenarioExecutionResults, OrchestratorError> {
        let records = records_with_media(cursor);
        let inputs = ScenarioExecutionInputs {
            scenario: &cursor.scenario,
            plan: &self.plan,
            registry: &self.registry,
            executors: &self.executors,
            records: &records,
            final_response: &cursor.final_response,
            run_id: cursor.run_id.clone(),
        };
        Ok(execute_scenario(inputs).await?)
    }

    /// Build aggregation input for a scored scenario.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when mechanic results require a subject_ref
    /// but the EvalSpec does not declare one.
    pub fn scenario_aggregation(
        &self,
        cursor: &ScenarioCursor,
        result: &ScenarioExecutionResults,
    ) -> Result<ScenarioAggregationInput, OrchestratorError> {
        let mechanic_results = mechanic_by_subject(
            self.spec.subject_ref.as_ref().and_then(Ref::as_card_ref),
            &result.mechanic,
        )?;
        let passenger_tasks = result
            .passenger
            .iter()
            .map(|result| TaskSummary {
                task_id: result.task_id.clone(),
                passed: result.passed,
                stage: result.stage,
                message: result.message.clone(),
                duration_ms: result.duration_ms,
            })
            .collect();
        let conversation_history = cursor
            .history
            .iter()
            .map(|turn| format!("{:?}: {}", turn.role, turn.content))
            .collect();
        let ended_at = cursor.completed_at.unwrap_or_else(Utc::now);
        let duration_ms = ended_at
            .signed_duration_since(cursor.started_at)
            .num_milliseconds()
            .max(0) as u64;

        Ok(ScenarioAggregationInput {
            scenario_id: result.scenario_id.clone(),
            mechanic_results,
            passenger_tasks,
            conversation_history,
            started_at: cursor.started_at,
            duration_ms,
        })
    }

    /// Aggregate all scenario results into run-level [`EvalResults`].
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when aggregation fails.
    pub fn finalize(
        &self,
        identity: RunIdentity,
        scenarios: Vec<ScenarioAggregationInput>,
    ) -> Result<EvalResults, OrchestratorError> {
        Ok(aggregate_run(AggregationInput {
            identity,
            scenarios,
            config: ResultsConfig {
                context_capture: self.spec.context_capture,
                pass_gate: self.spec.pass_gate.clone(),
            },
        })?)
    }

    /// Score a batch of pre-collected eval records without driving scenarios.
    ///
    /// This is the degenerate replay path used by `wyrd eval run --records`.
    /// Records do not carry scenario identity in the committed observation
    /// contract, so the batch is aggregated under one synthetic scenario row.
    ///
    /// # Errors
    /// Returns [`OrchestratorError`] when task execution or aggregation fails.
    pub async fn score_record_batch(
        &self,
        identity: RunIdentity,
        records: &[EvalRecordObservation],
    ) -> Result<EvalResults, OrchestratorError> {
        let scenario_id =
            ScenarioId::new("records").map_err(|source| OrchestratorError::Invariant {
                reason: format!("static records scenario id failed validation: {source}"),
            })?;
        let started_at = Utc::now();
        let mut mechanic = Vec::new();

        for record in records {
            let record_with_media = RecordWithMedia {
                record_id: record.record_id.clone(),
                trace_id: record.trace_id,
                context: record.context.clone(),
                media: MediaBindings::new(),
                required_media: Vec::new(),
            };
            let snapshot = crate::ContextSnapshot::new(
                Arc::new(record_with_media.context.clone()),
                crate::RecordIdentity {
                    run_id: identity.run_id.clone(),
                    record_id: record_with_media.record_id.clone(),
                    scenario_id: Some(scenario_id.clone()),
                },
            )
            .with_media(record_with_media.media, record_with_media.required_media);
            let snapshot = if let Some(trace_id) = record_with_media.trace_id {
                snapshot.with_trace_id(trace_id)
            } else {
                snapshot
            };
            let context = crate::ExecutionContext::from_snapshot(Arc::new(snapshot));
            let report =
                execute_plan(&self.plan, &context, &self.registry, &self.executors).await?;
            for outcome in report.outcomes {
                if let TaskRunOutcome::Ran(result) = outcome {
                    mechanic.push(RecordTaskResult {
                        record_id: record.record_id.clone(),
                        result,
                    });
                }
            }
        }

        let scenario = ScenarioExecutionResults {
            scenario_id: scenario_id.clone(),
            mechanic,
            passenger: Vec::new(),
        };
        let aggregation = ScenarioAggregationInput {
            scenario_id,
            mechanic_results: mechanic_by_subject(
                self.spec.subject_ref.as_ref().and_then(Ref::as_card_ref),
                &scenario.mechanic,
            )?,
            passenger_tasks: Vec::new(),
            conversation_history: Vec::new(),
            started_at,
            duration_ms: Utc::now()
                .signed_duration_since(started_at)
                .num_milliseconds()
                .max(0) as u64,
        };
        self.finalize(identity, vec![aggregation])
    }
}

fn records_with_media(cursor: &ScenarioCursor) -> Vec<RecordWithMedia> {
    cursor
        .emitted_records
        .iter()
        .map(|record| RecordWithMedia {
            record_id: record.record_id.clone(),
            trace_id: record.trace_id,
            context: record.context.clone(),
            media: MediaBindings::new(),
            required_media: Vec::new(),
        })
        .collect()
}

fn mechanic_by_subject(
    subject_ref: Option<&CardRef>,
    mechanic: &[RecordTaskResult],
) -> Result<BTreeMap<SubjectKey, MechanicSubjectInput>, OrchestratorError> {
    if mechanic.is_empty() {
        return Ok(BTreeMap::new());
    }
    let Some(subject_ref) = subject_ref else {
        return Err(OrchestratorError::MissingSubjectForMechanicResults);
    };
    let subject_key = SubjectKey::from_ref(subject_ref);
    let mut task_results = BTreeMap::new();
    for row in mechanic {
        task_results
            .entry(row.result.task_id.clone())
            .or_insert_with(Vec::new)
            .push(row.result.clone());
    }
    Ok(BTreeMap::from([(
        subject_key,
        MechanicSubjectInput {
            subject_ref: subject_ref.clone(),
            task_results,
        },
    )]))
}

#[allow(dead_code)]
fn _assert_value_is_send_sync(_: &Value) {}
