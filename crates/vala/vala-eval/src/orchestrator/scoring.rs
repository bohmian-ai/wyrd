//! Scenario and run scoring hand-off into the eval engine.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::EvalSpec;

use crate::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, MediaBindings, TraceTaskExecutor,
};
use crate::{
    AggregationInput, EvalResults, Executors, InMemoryTraceSource, JudgeInvoker,
    MechanicSubjectInput, RecordTaskResult, RecordWithMedia, ResultsConfig, RunIdentity,
    ScenarioAggregationInput, ScenarioExecutionInputs, ScenarioExecutionResults, SubjectKey,
    TaskRegistry, TaskSummary, TraceSource, aggregate_run, execute_scenario,
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
        let mechanic_results =
            mechanic_by_subject(self.spec.subject_ref.as_ref(), &result.mechanic)?;
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
