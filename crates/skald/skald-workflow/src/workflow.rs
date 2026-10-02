//! Bounded Workflow execution.
//!
//! [`WorkflowExecutor`] owns one run: it schedules ready steps in
//! `(stage, step ID)` order into an owned Tokio `JoinSet` bounded by the
//! concurrency limit, resolves each step's bindings from completed results in
//! the parent, and settles every outcome into the [`RunLedger`]. An ordinary
//! step failure stops new scheduling and drains running peers; explicit
//! cancellation or the total deadline aborts and drains the set. Dropping the
//! executor's future drops the `JoinSet`, which aborts every step task, so no
//! step outlives its run.

use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use skald_observer::{Observer, current};
use skald_runtime::ProviderRegistry;
use tokio::task::{Id, JoinSet};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_spec::card::workflow::WorkflowRun;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::WorkflowRunId;
use wyrd_spec::reference::CardRef;

use crate::attempt::AttemptOutcome;
use crate::error::WorkflowResult;
use crate::plan::ExecutionPlan;
use crate::route::{AttemptRouteContext, WorkflowGatewayCorrelation};
use crate::run::{RunEnding, RunLedger};

/// Default number of steps that may execute concurrently.
pub const DEFAULT_MAX_CONCURRENCY: NonZeroUsize = match NonZeroUsize::new(8) {
    Some(value) => value,
    None => unreachable!(),
};

/// Upper bound on one Workflow backoff delay.
const MAX_BACKOFF_MS: u64 = 30_000;

/// Execution limits for one run. Local defaults cap only concurrency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowExecutionLimits {
    /// Maximum concurrently executing steps.
    pub max_concurrency: NonZeroUsize,
    /// Total run deadline measured from execution start.
    pub deadline: Option<Duration>,
    /// Maximum JCS size of the resolved invocation input.
    pub max_input_bytes: Option<usize>,
    /// Maximum size of one step result: UTF-8 text plus JCS structured output.
    pub max_step_result_bytes: Option<usize>,
    /// Maximum JCS size of the complete run snapshot.
    pub max_run_bytes: Option<usize>,
}

impl Default for WorkflowExecutionLimits {
    /// Eight concurrent steps and no deadline or size caps.
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            deadline: None,
            max_input_bytes: None,
            max_step_result_bytes: None,
            max_run_bytes: None,
        }
    }
}

/// Per-invocation options for [`crate::Workflow::run_with_options`].
#[derive(Debug, Clone, Default)]
pub struct WorkflowRunOptions {
    /// Execution limits.
    pub limits: WorkflowExecutionLimits,
    /// Cancels the run; interrupted steps become `cancelled`.
    pub cancellation: CancellationToken,
}

/// Owner of one Workflow run's scheduling and settlement.
pub(crate) struct WorkflowExecutor {
    /// Workflow identifier reported to observers.
    workflow_id: String,
    /// Immutable plan shared with step tasks.
    plan: Arc<ExecutionPlan>,
    /// Native provider registry shared with step tasks.
    native: Arc<ProviderRegistry>,
    /// Snapshot owner.
    ledger: RunLedger,
    /// Attempts begun per plan index, kept even when a task is aborted.
    attempts: Arc<[AtomicU32]>,
    /// Remaining unsatisfied dependencies per plan index.
    waiting: Vec<usize>,
    /// Ready plan indices; the smallest is scheduled first.
    ready: BTreeSet<usize>,
    /// Step limits and run cancellation.
    options: WorkflowRunOptions,
}

/// What a finished step task reports.
enum StepReport {
    /// The final attempt's outcome.
    Finished(AttemptOutcome),
    /// Cancellation or the total deadline interrupted the step.
    Interrupted,
}

/// Everything one step task needs, owned so the task is `'static`.
struct StepTask {
    /// Immutable plan.
    plan: Arc<ExecutionPlan>,
    /// Plan index of the step.
    index: usize,
    /// Native provider registry.
    native: Arc<ProviderRegistry>,
    /// Prompt variable values resolved by the parent.
    pairs: Vec<(String, String)>,
    /// Run identity.
    run_id: WorkflowRunId,
    /// Total run deadline.
    deadline: Option<Instant>,
    /// Run cancellation.
    cancellation: CancellationToken,
    /// Shared attempt counters.
    attempts: Arc<[AtomicU32]>,
    /// Step result size limit.
    max_step_result_bytes: Option<usize>,
    /// Observer active for the run.
    observer: Arc<dyn Observer>,
}

impl WorkflowExecutor {
    /// Prepare the executor and its queued snapshot.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_413_GRAPH_TOO_LARGE` when the terminal reserve
    /// cannot fit the run size limit.
    pub(crate) fn new(
        workflow_id: String,
        workflow: Option<CardRef>,
        plan: ExecutionPlan,
        native: &ProviderRegistry,
        options: WorkflowRunOptions,
    ) -> WorkflowResult<Self> {
        let ledger = RunLedger::new(
            WorkflowRunId::new_v7(),
            workflow,
            &plan,
            options.limits.max_run_bytes,
        )?;
        let waiting: Vec<usize> = plan.steps.iter().map(|step| step.dependency_count).collect();
        let ready = waiting
            .iter()
            .enumerate()
            .filter(|(_, count)| **count == 0)
            .map(|(index, _)| index)
            .collect();
        let attempts = plan.steps.iter().map(|_| AtomicU32::new(0)).collect();
        Ok(Self {
            workflow_id,
            plan: Arc::new(plan),
            native: Arc::new(native.clone()),
            ledger,
            attempts,
            waiting,
            ready,
            options,
        })
    }

    /// Execute the run to a terminal snapshot.
    ///
    /// Never fails after preparation: step, cancellation, deadline, and size
    /// outcomes are all recorded in the returned snapshot.
    pub(crate) async fn execute(mut self) -> WorkflowRun {
        let observer = current();
        let run_id = self.ledger.run_id().to_string();
        let started = Instant::now();
        observer
            .on_workflow_start(&run_id, &self.workflow_id, self.plan.steps.len())
            .await;
        self.ledger.start();
        let deadline = self.options.limits.deadline.map(|limit| started + limit);
        let cancellation = self.options.cancellation.clone();
        let mut tasks: JoinSet<StepReport> = JoinSet::new();
        let mut running: HashMap<Id, usize> = HashMap::new();
        let mut stopping = false;
        let mut ending = RunEnding::Settled;
        loop {
            if ending == RunEnding::Settled {
                if cancellation.is_cancelled() {
                    ending = RunEnding::Cancelled;
                } else if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    ending = RunEnding::TimedOut;
                }
                if ending != RunEnding::Settled {
                    tasks.abort_all();
                }
            }
            while ending == RunEnding::Settled
                && !stopping
                && tasks.len() < self.options.limits.max_concurrency.get()
                && let Some(index) = self.ready.pop_first()
            {
                match self.bind(index) {
                    Ok(pairs) => {
                        self.ledger.step_started(index);
                        let task = StepTask {
                            plan: Arc::clone(&self.plan),
                            index,
                            native: Arc::clone(&self.native),
                            pairs,
                            run_id: self.ledger.run_id().clone(),
                            deadline,
                            cancellation: cancellation.clone(),
                            attempts: Arc::clone(&self.attempts),
                            max_step_result_bytes: self.options.limits.max_step_result_bytes,
                            observer: Arc::clone(&observer),
                        };
                        let scoped = skald_observer::with_observer(Arc::clone(&observer), task.run());
                        let handle = tasks.spawn(scoped);
                        running.insert(handle.id(), index);
                    }
                    Err(error) => {
                        let step_id = &self.plan.steps[index].id;
                        self.attempts[index].store(1, Ordering::Release);
                        observer.on_workflow_step_attempt(&run_id, step_id, 1).await;
                        observer
                            .on_workflow_step_result(&run_id, step_id, 1, Some(error.code()))
                            .await;
                        self.ledger
                            .step_failed(index, wyrd_spec::card::workflow::WorkflowRunError::from_wyrd(&error), 1);
                        stopping = true;
                    }
                }
            }
            if tasks.is_empty() {
                break;
            }
            let joined = tokio::select! {
                biased;
                () = cancellation.cancelled(), if ending == RunEnding::Settled => {
                    ending = RunEnding::Cancelled;
                    tasks.abort_all();
                    continue;
                }
                () = sleep_until_deadline(deadline), if ending == RunEnding::Settled && deadline.is_some() => {
                    ending = RunEnding::TimedOut;
                    tasks.abort_all();
                    continue;
                }
                joined = tasks.join_next_with_id() => joined,
            };
            let Some(joined) = joined else { break };
            let (id, report) = match joined {
                Ok((id, report)) => (id, Ok(report)),
                Err(error) => (error.id(), Err(error)),
            };
            let Some(index) = running.remove(&id) else { continue };
            let attempts = self.attempts[index].load(Ordering::Acquire);
            match report {
                Ok(StepReport::Finished(AttemptOutcome::Succeeded(payload))) => {
                    if self.ledger.step_succeeded(index, payload, attempts) {
                        self.release_dependents(index);
                    } else {
                        stopping = true;
                    }
                }
                Ok(StepReport::Finished(AttemptOutcome::Failed { error, .. })) => {
                    self.ledger.step_failed(index, error, attempts);
                    stopping = true;
                }
                Ok(StepReport::Interrupted) => self.ledger.step_cancelled(index, attempts),
                Err(error) if error.is_cancelled() => self.ledger.step_cancelled(index, attempts),
                Err(_panic) => {
                    let error = WyrdError::WorkflowInternal {
                        message: format!("step '{}' task panicked", self.plan.steps[index].id),
                        details: serde_json::json!({ "step": self.plan.steps[index].id }),
                    };
                    self.ledger.step_failed(
                        index,
                        wyrd_spec::card::workflow::WorkflowRunError::from_wyrd(&error),
                        attempts,
                    );
                    stopping = true;
                }
            }
        }
        let run = self.ledger.finish(ending, &self.plan);
        observer
            .on_workflow_finish(&run_id, &self.workflow_id, started.elapsed())
            .await;
        run
    }

    /// Resolve the Prompt variable values for the step at `index`.
    ///
    /// Strings pass unchanged, `null` becomes empty text, and any other JSON
    /// value becomes compact JSON text.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_422_MISSING_PARAMETER` when a selected runtime
    /// value is absent.
    fn bind(&self, index: usize) -> Result<Vec<(String, String)>, WyrdError> {
        let step = &self.plan.steps[index];
        step.bindings
            .iter()
            .map(|(name, binding)| {
                let value = self.ledger.select(&self.plan.input, binding).ok_or_else(|| {
                    WyrdError::WorkflowMissingParameter {
                        message: format!("step '{}' binding '{name}' selects a missing value", step.id),
                        details: serde_json::json!({
                            "step": step.id,
                            "field": format!("inputs.{name}"),
                            "binding": binding.as_str(),
                        }),
                    }
                })?;
                let text = match value {
                    serde_json::Value::String(text) => text,
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                };
                Ok((name.clone(), text))
            })
            .collect()
    }

    /// Make dependents of a succeeded step ready once all their dependencies
    /// succeeded.
    fn release_dependents(&mut self, index: usize) {
        for &child in &self.plan.steps[index].dependents {
            self.waiting[child] -= 1;
            if self.waiting[child] == 0 {
                self.ready.insert(child);
            }
        }
    }
}

impl StepTask {
    /// Run attempts until success, a terminal failure, retry exhaustion, or
    /// interruption.
    ///
    /// Each attempt races, in order, cancellation, the total deadline, the
    /// step attempt timeout, and the attempt itself. Retryable failures wait
    /// the deterministic backoff, which also races cancellation and the total
    /// deadline. No attempt begins after either fires.
    async fn run(self) -> StepReport {
        let step = &self.plan.steps[self.index];
        let run_id = self.run_id.to_string();
        let mut attempt: u32 = 0;
        loop {
            if self.cancellation.is_cancelled()
                || self.deadline.is_some_and(|deadline| Instant::now() >= deadline)
            {
                return StepReport::Interrupted;
            }
            attempt += 1;
            self.attempts[self.index].store(attempt, Ordering::Release);
            self.observer
                .on_workflow_step_attempt(&run_id, &step.id, attempt)
                .await;
            let attempt_deadline = step.timeout.map(|timeout| Instant::now() + timeout);
            let outcome = tokio::select! {
                biased;
                () = self.cancellation.cancelled() => return StepReport::Interrupted,
                () = sleep_until_deadline(self.deadline), if self.deadline.is_some() => {
                    return StepReport::Interrupted;
                }
                () = sleep_until_deadline(attempt_deadline), if attempt_deadline.is_some() => {
                    AttemptOutcome::failed(
                        &WyrdError::WorkflowStepTimeout {
                            message: format!("step '{}' attempt exceeded its timeout", step.id),
                            details: serde_json::json!({ "step": step.id }),
                        },
                        true,
                    )
                }
                outcome = self.attempt(attempt, attempt_deadline) => outcome,
            };
            let (error, retryable) = match outcome {
                AttemptOutcome::Succeeded(payload) => {
                    self.observer
                        .on_workflow_step_result(&run_id, &step.id, attempt, None)
                        .await;
                    return StepReport::Finished(AttemptOutcome::Succeeded(payload));
                }
                AttemptOutcome::Failed { error, retryable } => (error, retryable),
            };
            self.observer
                .on_workflow_step_result(&run_id, &step.id, attempt, Some(&error.code))
                .await;
            if !retryable || attempt > step.max_retries {
                return StepReport::Finished(AttemptOutcome::Failed { error, retryable });
            }
            let delay = backoff(step.initial_backoff_ms, attempt);
            self.observer
                .on_workflow_step_backoff(&run_id, &step.id, attempt + 1, delay)
                .await;
            tokio::select! {
                biased;
                () = self.cancellation.cancelled() => return StepReport::Interrupted,
                () = sleep_until_deadline(self.deadline), if self.deadline.is_some() => {
                    return StepReport::Interrupted;
                }
                () = tokio::time::sleep(delay) => {}
            }
        }
    }

    /// Execute one attempt through the existing Agent loop.
    ///
    /// Gateway routes run a copy of the Agent whose provider registry holds
    /// only this attempt's route adapter; native routes use the Agent as is.
    async fn attempt(&self, attempt: u32, attempt_deadline: Option<Instant>) -> AttemptOutcome {
        let step = &self.plan.steps[self.index];
        let agent_deadline = step
            .agent
            .run_config
            .timeout
            .map(|timeout| Instant::now() + timeout);
        let deadline = [attempt_deadline, agent_deadline, self.deadline]
            .into_iter()
            .flatten()
            .min();
        let prompt = step.agent.prompt.native();
        let context = AttemptRouteContext {
            provider: prompt.request.provider(),
            model: prompt.model.clone(),
            deadline,
            cancellation: self.cancellation.clone(),
            correlation: WorkflowGatewayCorrelation {
                run_id: self.run_id.clone(),
                step_id: step.id.clone(),
                attempt,
            },
        };
        let agent = match step.route.attempt_registry(context) {
            None => Arc::clone(&step.agent),
            Some(registry) => Arc::new(
                step.agent
                    .as_ref()
                    .clone()
                    .with_provider_registry(Arc::new(registry)),
            ),
        };
        let pairs: Vec<(&str, &str)> = self
            .pairs
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let run_id = self.run_id.to_string();
        let result = agent
            .run_prompt(&self.native, &agent.prompt, &pairs, Some(&run_id))
            .await;
        AttemptOutcome::from_agent(
            &step.id,
            result,
            step.validator.as_ref(),
            self.max_step_result_bytes,
        )
    }
}

/// Deterministic Workflow backoff before retry number `retry` (one-based):
/// `min(initial_ms * 2^(retry - 1), 30_000)` milliseconds, saturating.
pub(crate) fn backoff(initial_ms: u64, retry: u32) -> Duration {
    let factor = 2u64.saturating_pow(retry.saturating_sub(1));
    Duration::from_millis(initial_ms.saturating_mul(factor).min(MAX_BACKOFF_MS))
}

/// Sleep until `deadline`, or forever when there is none.
async fn sleep_until_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use serde_json::json;
    use wyrd_spec::card::common::ParameterValue;
    use wyrd_spec::card::workflow::{WorkflowRunStatus, WorkflowStepStatus};

    use super::{WorkflowExecutionLimits, WorkflowRunOptions};
    use crate::route::WorkflowExecutionDependencies;
    use crate::test_support::{
        RecordingObserver, Reply, ScriptedProvider, agent, bindings, string_schema,
    };
    use crate::workflow_surface::Workflow;

    /// Declared string inputs with the given defaults.
    fn string_inputs(pairs: &[(&str, &str)]) -> BTreeMap<String, ParameterValue> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), ParameterValue::Str((*value).to_owned())))
            .collect()
    }

    /// Run `workflow` against `provider` with default local options.
    async fn run_local(
        workflow: &Workflow,
        provider: &Arc<ScriptedProvider>,
        input: serde_json::Value,
    ) -> wyrd_spec::card::workflow::WorkflowRun {
        let serde_json::Value::Object(input) = input else {
            panic!("fixture input must be an object");
        };
        workflow
            .run_with_options(
                &WorkflowExecutionDependencies::new(provider.registry()),
                input,
                WorkflowRunOptions::default(),
            )
            .await
            .expect("workflow passes pre-dispatch validation")
    }

    /// Scenario 2: parallel steps with the same output key stay namespaced;
    /// downstream bindings select original input, whole objects, nested fields,
    /// and text from visible completed steps only; final outputs keep JSON
    /// types; dependencies alone inject nothing; a missing runtime field fails
    /// the dependent before its provider call; the primary error is chosen by
    /// stage then step ID regardless of completion order.
    #[tokio::test(start_paused = true)]
    async fn explicit_namespaced_results() {
        let detail_schema = json!({
            "type": "object",
            "properties": {
                "summary": { "type": "string" },
                "detail": { "type": "object", "properties": { "n": { "type": "integer" } } }
            },
            "required": ["summary", "detail"]
        });
        let workflow = Workflow::builder("namespaced")
            .add(agent("alpha", "alpha about ${topic}", Some(detail_schema.clone())))
            .and_then(|b| b.add(agent("beta", "beta about ${topic}", Some(string_schema(&["summary"])))))
            .and_then(|b| b.add_after(agent("echo", "echo static", None), ["alpha"]))
            .and_then(|b| {
                b.add_after(
                    agent("final", "final ${orig} | ${a_sum} | ${b_all} | ${n}", None),
                    ["alpha", "beta"],
                )
            })
            .and_then(|b| b.with_inputs(string_inputs(&[("topic", "rust")])))
            .and_then(|b| b.with_step_inputs("alpha", bindings(&[("topic", "input.topic")])))
            .and_then(|b| b.with_step_inputs("beta", bindings(&[("topic", "input.topic")])))
            .and_then(|b| {
                b.with_step_inputs(
                    "final",
                    bindings(&[
                        ("orig", "input.topic"),
                        ("a_sum", "steps.alpha.output.structured.summary"),
                        ("b_all", "steps.beta.output.structured"),
                        ("n", "steps.alpha.output.structured.detail.n"),
                    ]),
                )
            })
            .and_then(|b| {
                b.with_outputs(bindings(&[
                    ("report", "steps.final.output.text"),
                    ("alpha_detail", "steps.alpha.output.structured.detail"),
                    ("topic", "input.topic"),
                ]))
            })
            .and_then(|b| b.build())
            .expect("explicit workflow builds");

        let provider = ScriptedProvider::new();
        provider.on(
            "alpha about",
            vec![Reply::After(
                std::time::Duration::from_millis(30),
                Box::new(Reply::Text(r#"{"summary":"A-sum","detail":{"n":7}}"#.to_owned())),
            )],
        );
        provider.on("beta about", vec![Reply::Text(r#"{"summary":"B-sum"}"#.to_owned())]);
        provider.on("echo static", vec![Reply::Text("echoed".to_owned())]);
        provider.on("final", vec![Reply::Text("the report".to_owned())]);

        let run = run_local(&workflow, &provider, json!({ "topic": "go" })).await;

        assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
        assert!(run.error.is_none());
        assert_eq!(run.outputs["report"], json!("the report"));
        assert_eq!(run.outputs["alpha_detail"], json!({ "n": 7 }));
        assert_eq!(run.outputs["topic"], json!("go"));
        assert_eq!(
            run.steps["alpha"].structured_output,
            Some(json!({ "summary": "A-sum", "detail": { "n": 7 } }))
        );
        assert_eq!(run.steps["beta"].structured_output, Some(json!({ "summary": "B-sum" })));
        assert!(run.steps["alpha"].text.is_none());
        assert_eq!(run.steps["final"].text.as_deref(), Some("the report"));
        assert!(run.steps["final"].structured_output.is_none());
        for step in run.steps.values() {
            assert_eq!(step.status, WorkflowStepStatus::Succeeded);
            assert_eq!(step.attempts, 1);
            assert!(step.error.is_none() && step.started_at.is_some() && step.ended_at.is_some());
        }
        let requests = provider.requests();
        assert!(
            requests.contains(&r#"final go | A-sum | {"summary":"B-sum"} | 7"#.to_owned()),
            "{requests:?}"
        );
        assert!(requests.contains(&"echo static".to_owned()), "{requests:?}");

        // A selected nested field that the completed upstream lacks fails the
        // dependent before its provider call.
        let missing = Workflow::builder("missing")
            .add(agent("alpha", "alpha about ${topic}", Some(detail_schema)))
            .and_then(|b| b.add_after(agent("final", "final ${gone}", None), ["alpha"]))
            .and_then(|b| b.with_inputs(string_inputs(&[("topic", "rust")])))
            .and_then(|b| b.with_step_inputs("alpha", bindings(&[("topic", "input.topic")])))
            .and_then(|b| {
                b.with_step_inputs("final", bindings(&[("gone", "steps.alpha.output.structured.absent")]))
            })
            .and_then(|b| b.with_outputs(bindings(&[("report", "steps.final.output.text")])))
            .and_then(|b| b.build())
            .expect("missing-field workflow builds");
        let provider = ScriptedProvider::new();
        provider.on(
            "alpha about",
            vec![Reply::Text(r#"{"summary":"A","detail":{}}"#.to_owned())],
        );
        let run = run_local(&missing, &provider, json!({})).await;
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert!(run.outputs.is_empty());
        let final_step = &run.steps["final"];
        assert_eq!(final_step.status, WorkflowStepStatus::Failed);
        assert_eq!(final_step.attempts, 1);
        let error = final_step.error.as_ref().expect("failed step carries its error");
        assert_eq!(error.code, "WYRD_WORKFLOW_422_MISSING_PARAMETER");
        assert_eq!(run.error.as_ref().map(|e| e.code.as_str()), Some(error.code.as_str()));
        assert_eq!(provider.count("final"), 0);

        // Same-stage failures complete in reverse ID order; the primary error
        // is still the smallest step ID, every failed peer keeps its error, and
        // the later stage stays unstarted.
        let peers = Workflow::builder("peers")
            .add(agent("a_first", "first peer", None))
            .and_then(|b| b.add(agent("z_last", "last peer", None)))
            .and_then(|b| b.add_after(agent("after", "after peers", None), ["a_first", "z_last"]))
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.after.output.text")])))
            .and_then(|b| b.build())
            .expect("peer workflow builds");
        let provider = ScriptedProvider::new();
        let remote = |code: &str| skald_providers::ProviderError::RemoteProblem {
            code: code.to_owned(),
            status: 403,
            message: format!("{code} refused"),
            field: None,
            remediation: "fix it".to_owned(),
        };
        provider.on(
            "first peer",
            vec![Reply::After(
                std::time::Duration::from_millis(50),
                Box::new(Reply::Fail(remote("TEST_A_FORBIDDEN"))),
            )],
        );
        provider.on("last peer", vec![Reply::Fail(remote("TEST_Z_FORBIDDEN"))]);
        let run = run_local(&peers, &provider, json!({})).await;
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(run.error.as_ref().map(|e| e.code.as_str()), Some("TEST_A_FORBIDDEN"));
        assert_eq!(
            run.steps["z_last"].error.as_ref().map(|e| e.code.as_str()),
            Some("TEST_Z_FORBIDDEN")
        );
        assert_eq!(run.steps["a_first"].attempts, 1, "remote problems are terminal");
        let after = &run.steps["after"];
        assert_eq!(after.status, WorkflowStepStatus::Unstarted);
        assert_eq!(after.attempts, 0);
        assert!(after.started_at.is_none() && after.ended_at.is_none());
    }
}
