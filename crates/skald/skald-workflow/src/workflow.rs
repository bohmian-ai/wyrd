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
//!
//! Telemetry is plain synchronous `tracing`: one `workflow.run` span per run,
//! one child `workflow.step` span per attempt, and a `workflow.step.backoff`
//! event before each retry. They carry identifiers, counts, statuses, and
//! stable Wyrd error codes, never payloads, and cannot affect the run.

use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use futures_util::FutureExt;
use skald_agent::AgentError;
use skald_runtime::ProviderRegistry;
use tokio::task::{Id, JoinError, JoinSet};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, Span, field, info_span};
use wyrd_spec::card::workflow::{WorkflowRun, WorkflowRunStatus};
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
    /// Workflow identifier reported on the `workflow.run` span.
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
    /// Absolute total run deadline, fixed when the run is prepared.
    deadline: Option<Instant>,
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
}

impl WorkflowExecutor {
    /// Prepare the executor and its queued snapshot, fixing the absolute run
    /// deadline from now.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` (field `deadline`) when the run
    /// deadline is not representable as an instant, and
    /// `WYRD_WORKFLOW_413_GRAPH_TOO_LARGE` when the terminal reserve cannot fit
    /// the run size limit.
    pub(crate) fn new(
        workflow_id: String,
        workflow: Option<CardRef>,
        plan: ExecutionPlan,
        native: &ProviderRegistry,
        options: WorkflowRunOptions,
    ) -> WorkflowResult<Self> {
        let deadline = options
            .limits
            .deadline
            .map(|limit| {
                Instant::now()
                    .checked_add(limit)
                    .ok_or_else(|| WyrdError::WorkflowRunRequest {
                        message: "the run deadline is not representable".to_owned(),
                        details: serde_json::json!({ "field": "deadline" }),
                    })
            })
            .transpose()?;
        let ledger = RunLedger::new(
            WorkflowRunId::new_v7(),
            workflow,
            &plan,
            options.limits.max_run_bytes,
        )?;
        let waiting: Vec<usize> = plan
            .steps
            .iter()
            .map(|step| step.dependency_count)
            .collect();
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
            deadline,
        })
    }

    /// The queued snapshot this executor will run.
    pub(crate) fn snapshot(&self) -> &WorkflowRun {
        self.ledger.snapshot()
    }

    /// The absolute total run deadline fixed by [`Self::new`], if the run has
    /// one.
    pub(crate) const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Execute the run to a terminal snapshot inside its `workflow.run` span.
    ///
    /// Never fails after preparation: step, cancellation, deadline, and size
    /// outcomes are all recorded in the returned snapshot. The span records
    /// the terminal status and, for an unsuccessful run, the primary error
    /// code. `on_transition` receives the complete non-terminal snapshot
    /// after every ledger transition, on the scheduling task, so it must not
    /// block; the terminal snapshot is only returned.
    pub(crate) async fn execute<F>(self, mut on_transition: F) -> WorkflowRun
    where
        F: FnMut(&WorkflowRun) + Send,
    {
        let span = info_span!(
            "workflow.run",
            wyrd.workflow.id = %self.workflow_id,
            wyrd.workflow.run_id = %self.ledger.run_id(),
            wyrd.workflow.step_count = self.plan.steps.len(),
            wyrd.workflow.status = field::Empty,
            error.r#type = field::Empty,
            otel.status_code = field::Empty,
        );
        let run = self
            .drive(&mut on_transition)
            .instrument(span.clone())
            .await;
        span.record("wyrd.workflow.status", status_name(run.status));
        if let Some(error) = &run.error {
            span.record("error.type", error.code.as_str());
        }
        if run.status != WorkflowRunStatus::Succeeded {
            span.record("otel.status_code", "ERROR");
        }
        run
    }

    /// Schedule, settle, and terminalize the run.
    ///
    /// Step tasks inherit the current `workflow.run` span so their attempt
    /// spans are its children. `on_transition` observes the snapshot after
    /// the run starts and after each step starts or settles.
    async fn drive<F>(mut self, on_transition: &mut F) -> WorkflowRun
    where
        F: FnMut(&WorkflowRun) + Send,
    {
        let deadline = self.deadline;
        let cancellation = self.options.cancellation.clone();
        self.ledger.start();
        on_transition(self.ledger.snapshot());
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
                        self.attempts[index].store(1, Ordering::Release);
                        self.ledger.step_started(index);
                        on_transition(self.ledger.snapshot());
                        let task = StepTask {
                            plan: Arc::clone(&self.plan),
                            index,
                            native: Arc::clone(&self.native),
                            pairs,
                            run_id: *self.ledger.run_id(),
                            deadline,
                            cancellation: cancellation.clone(),
                            attempts: Arc::clone(&self.attempts),
                            max_step_result_bytes: self.options.limits.max_step_result_bytes,
                        };
                        let handle = tasks.spawn(task.run().instrument(Span::current()));
                        running.insert(handle.id(), index);
                    }
                    Err(error) => {
                        self.attempts[index].store(1, Ordering::Release);
                        AttemptSpan::new(&self.plan.steps[index].id, 1).failed(error.code());
                        self.ledger.step_failed(
                            index,
                            wyrd_spec::card::workflow::WorkflowRunError::from_wyrd(&error),
                            1,
                        );
                        on_transition(self.ledger.snapshot());
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
                () = sleep_until(deadline), if ending == RunEnding::Settled && deadline.is_some() => {
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
            let Some(index) = running.remove(&id) else {
                continue;
            };
            stopping |= self.settle(index, report);
            on_transition(self.ledger.snapshot());
        }
        self.ledger.finish(ending, &self.plan)
    }

    /// Record one joined step task in the ledger; returns whether new
    /// scheduling must stop.
    ///
    /// Success releases dependents unless its payload overflows the run
    /// budget; failure and panic stop scheduling. An interrupted or aborted
    /// task is `cancelled` with its recorded attempts, which are at least one
    /// because scheduling reserves the first attempt before publishing the
    /// step as `running`.
    fn settle(&mut self, index: usize, report: Result<StepReport, JoinError>) -> bool {
        let attempts = self.attempts[index].load(Ordering::Acquire);
        match report {
            Ok(StepReport::Finished(AttemptOutcome::Succeeded(payload))) => {
                if self.ledger.step_succeeded(index, payload, attempts) {
                    self.release_dependents(index);
                    false
                } else {
                    true
                }
            }
            Ok(StepReport::Finished(AttemptOutcome::Failed { error, .. })) => {
                self.ledger.step_failed(index, error, attempts);
                true
            }
            Ok(StepReport::Interrupted) => {
                self.ledger.step_cancelled(index, attempts);
                false
            }
            Err(error) if error.is_cancelled() => {
                self.ledger.step_cancelled(index, attempts);
                false
            }
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
                true
            }
        }
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
                let value = self
                    .ledger
                    .select(&self.plan.input, binding)
                    .ok_or_else(|| WyrdError::WorkflowMissingParameter {
                        message: format!(
                            "step '{}' binding '{name}' selects a missing value",
                            step.id
                        ),
                        details: serde_json::json!({
                            "step": step.id,
                            "field": format!("inputs.{name}"),
                            "binding": binding.as_str(),
                        }),
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
    /// Each attempt fixes its own and the Agent's deadlines, opens its
    /// `workflow.step` span, then races, in order, cancellation, the total
    /// deadline, the step attempt timeout, the Agent timeout, and the attempt
    /// itself. The fixed Agent deadline bounds all Agent work, including
    /// normalization, output admission, and terminal journal settlement; on
    /// expiry the attempt is dropped and classified as the Agent's typed
    /// timeout. A panic inside the attempt is caught while its span is still
    /// open and becomes a non-retryable `WYRD_WORKFLOW_500_INTERNAL` failure,
    /// so the span and the run record the same outcome. Retryable
    /// failures emit a `workflow.step.backoff` event and wait the
    /// deterministic backoff, which also races cancellation and the total
    /// deadline. No attempt begins after either fires.
    async fn run(self) -> StepReport {
        let step = &self.plan.steps[self.index];
        let mut attempt: u32 = 0;
        loop {
            if self.cancellation.is_cancelled()
                || self
                    .deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
            {
                return StepReport::Interrupted;
            }
            attempt += 1;
            self.attempts[self.index].store(attempt, Ordering::Release);
            let deadlines = deadline_after(step.timeout)
                .and_then(|attempt| Ok((attempt, deadline_after(step.agent.run_config.timeout)?)));
            let span = AttemptSpan::new(&step.id, attempt);
            let Ok((attempt_deadline, agent_deadline)) = deadlines else {
                let error = WyrdError::WorkflowInternal {
                    message: format!("step '{}' timeout is not representable", step.id),
                    details: serde_json::json!({ "step": step.id }),
                };
                span.failed(error.code());
                return StepReport::Finished(AttemptOutcome::failed(&error, false));
            };
            let outcome = tokio::select! {
                biased;
                () = self.cancellation.cancelled() => return StepReport::Interrupted,
                () = sleep_until(self.deadline), if self.deadline.is_some() => {
                    return StepReport::Interrupted;
                }
                () = sleep_until(attempt_deadline), if attempt_deadline.is_some() => {
                    AttemptOutcome::failed(
                        &WyrdError::WorkflowStepTimeout {
                            message: format!("step '{}' attempt exceeded its timeout", step.id),
                            details: serde_json::json!({ "step": step.id }),
                        },
                        true,
                    )
                }
                () = sleep_until(agent_deadline), if agent_deadline.is_some() => {
                    let duration = step.agent.run_config.timeout.unwrap_or_default();
                    AttemptOutcome::from_agent(
                        &step.id,
                        Err(AgentError::Timeout { duration }),
                        step.validator.as_ref(),
                        self.max_step_result_bytes,
                    )
                }
                outcome = AssertUnwindSafe(
                    self.attempt(attempt, attempt_deadline, agent_deadline)
                        .instrument(span.span.clone()),
                )
                .catch_unwind() => outcome.unwrap_or_else(|_panic| {
                    AttemptOutcome::failed(
                        &WyrdError::WorkflowInternal {
                            message: format!("step '{}' attempt panicked", step.id),
                            details: serde_json::json!({ "step": step.id }),
                        },
                        false,
                    )
                }),
            };
            let (error, retryable) = match outcome {
                AttemptOutcome::Succeeded(payload) => {
                    span.succeeded();
                    return StepReport::Finished(AttemptOutcome::Succeeded(payload));
                }
                AttemptOutcome::Failed { error, retryable } => (error, retryable),
            };
            span.failed(&error.code);
            if !retryable || attempt > step.max_retries {
                return StepReport::Finished(AttemptOutcome::Failed { error, retryable });
            }
            let delay = backoff(step.initial_backoff_ms, attempt);
            tracing::info!(
                wyrd.workflow.step.id = %step.id,
                wyrd.workflow.step.next_attempt = attempt + 1,
                wyrd.workflow.step.delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                "workflow.step.backoff"
            );
            tokio::select! {
                biased;
                () = self.cancellation.cancelled() => return StepReport::Interrupted,
                () = sleep_until(self.deadline), if self.deadline.is_some() => {
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
    async fn attempt(
        &self,
        attempt: u32,
        attempt_deadline: Option<Instant>,
        agent_deadline: Option<Instant>,
    ) -> AttemptOutcome {
        let step = &self.plan.steps[self.index];
        let deadline = [attempt_deadline, agent_deadline, self.deadline]
            .into_iter()
            .flatten()
            .min();
        let prompt = step.agent.prompt.native();
        let context = AttemptRouteContext {
            provider: prompt.provider(),
            model: prompt.model.clone(),
            deadline,
            cancellation: self.cancellation.clone(),
            correlation: WorkflowGatewayCorrelation {
                run_id: self.run_id,
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
        let result = agent.run_prompt(&self.native, &agent.prompt, &pairs).await;
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

/// Absolute deadline `timeout` from now, or `Err` with the timeout when the
/// instant is not representable.
///
/// # Errors
///
/// Returns the timeout when `now + timeout` overflows the clock.
fn deadline_after(timeout: Option<Duration>) -> Result<Option<Instant>, Duration> {
    timeout
        .map(|timeout| Instant::now().checked_add(timeout).ok_or(timeout))
        .transpose()
}

/// Sleep until `deadline`, or forever when there is none.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Wire name of a run status, as recorded on the `workflow.run` span.
const fn status_name(status: WorkflowRunStatus) -> &'static str {
    match status {
        WorkflowRunStatus::Queued => "queued",
        WorkflowRunStatus::Running => "running",
        WorkflowRunStatus::Succeeded => "succeeded",
        WorkflowRunStatus::Failed => "failed",
        WorkflowRunStatus::Cancelled => "cancelled",
        WorkflowRunStatus::TimedOut => "timed_out",
    }
}

/// The `workflow.step` span of one attempt and whether its outcome is
/// recorded.
///
/// An attempt dropped before it settles — interrupted by cancellation or the
/// total deadline, or aborted with its task — records outcome `cancelled`
/// when the span closes, so every attempt span carries an outcome.
struct AttemptSpan {
    /// The attempt's span, a child of the current `workflow.run` span.
    span: Span,
    /// Whether [`Self::succeeded`] or [`Self::failed`] recorded the outcome.
    settled: bool,
}

impl AttemptSpan {
    /// Open the span for attempt `attempt` (one-based) of `step_id`.
    fn new(step_id: &str, attempt: u32) -> Self {
        Self {
            span: info_span!(
                "workflow.step",
                wyrd.workflow.step.id = step_id,
                wyrd.workflow.step.attempt = attempt,
                wyrd.workflow.step.outcome = field::Empty,
                error.r#type = field::Empty,
                otel.status_code = field::Empty,
            ),
            settled: false,
        }
    }

    /// Record a successful attempt.
    fn succeeded(mut self) {
        self.span.record("wyrd.workflow.step.outcome", "succeeded");
        self.settled = true;
    }

    /// Record a failed attempt with its stable Wyrd error `code`.
    fn failed(mut self, code: &str) {
        self.span.record("wyrd.workflow.step.outcome", "failed");
        self.span.record("error.type", code);
        self.span.record("otel.status_code", "ERROR");
        self.settled = true;
    }
}

impl Drop for AttemptSpan {
    /// Record outcome `cancelled` for an attempt that never settled.
    fn drop(&mut self) {
        if !self.settled {
            self.span.record("wyrd.workflow.step.outcome", "cancelled");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use secrecy::SecretString;
    use serde_json::json;
    use skald_prompt::{OpenAiResponsesOptions, openai_responses};
    use skald_spec::wire::openai_responses::{
        OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesResponse,
    };
    use skald_spec::{ProviderRequest, ProviderResponse};
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_spec::auth::AbsoluteUrl;
    use wyrd_spec::card::common::ParameterValue;
    use wyrd_spec::card::workflow::{
        ExternalGatewayProtocol, LlmRoute, WorkflowRetryPolicy, WorkflowRunStatus,
        WorkflowStepStatus,
    };
    use wyrd_spec::gateway::{GatewayFallbackOverride, ModelRef};
    use wyrd_spec::ids::CredentialBindingName;
    use wyrd_telemetry::{
        CapturedSpan, CapturedSpanStatus, TelemetryConfig, TelemetryGuard, TestTraceCapture,
        init_test_capture,
    };

    use super::{
        JoinError, JoinSet, StepReport, WorkflowExecutionLimits, WorkflowExecutor,
        WorkflowRunOptions, backoff,
    };
    use crate::attempt::{agent_error_retryable, project_agent_error};
    use crate::plan::ExecutionPlan;
    use crate::route::{
        DEFAULT_GATEWAY_CALL_TIMEOUT, ExternalEndpointProfile, ExternalGatewayBinding,
        ExternalGatewayBindings, WorkflowExecutionDependencies,
    };
    use crate::run::RunEnding;
    use crate::test_support::{
        RecordingTool, Reply, ScriptedProvider, agent, bindings, string_schema, text_response,
        tool_call_response,
    };
    use crate::workflow_surface::{Workflow, WorkflowInput};

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

    /// Preparing a run checks it and mints its ID without dispatching any
    /// step; execution reports complete snapshots under that ID after every
    /// transition and ends with the same ID.
    #[tokio::test(start_paused = true)]
    async fn prepared_run_keeps_its_id() {
        let workflow = Workflow::builder("prepared")
            .add(agent("first", "first static", None))
            .and_then(|b| b.add_after(agent("second", "second static", None), ["first"]))
            .and_then(|b| b.with_outputs(bindings(&[("text", "steps.second.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("prepared workflow builds");
        let provider = ScriptedProvider::new();
        provider.on("first static", vec![Reply::Text("one".to_owned())]);
        provider.on("second static", vec![Reply::Text("two".to_owned())]);
        let dependencies = WorkflowExecutionDependencies::new(provider.registry());

        let prepared = workflow
            .prepare(
                &dependencies,
                serde_json::Map::new(),
                WorkflowRunOptions::default(),
            )
            .expect("workflow passes pre-dispatch validation");
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(provider.requests().is_empty(), "prepare dispatches nothing");
        let queued = prepared.snapshot().clone();
        assert_eq!(queued.status, WorkflowRunStatus::Queued);
        assert_eq!(queued.steps.len(), 2, "the queued snapshot is complete");

        let mut observed = Vec::new();
        let run = prepared
            .execute(|snapshot| observed.push(snapshot.clone()))
            .await;

        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(
            run.run_id, queued.run_id,
            "the terminal run keeps the queued ID"
        );
        assert_eq!(provider.requests().len(), 2);
        let transitions: Vec<_> = observed
            .iter()
            .map(|snapshot| {
                assert_eq!(snapshot.run_id, queued.run_id);
                assert_eq!(snapshot.status, WorkflowRunStatus::Running);
                for step in snapshot.steps.values() {
                    if step.status == WorkflowStepStatus::Running {
                        assert_eq!(step.attempts, 1, "a published running step has begun");
                    }
                }
                (
                    snapshot.steps["first"].status,
                    snapshot.steps["second"].status,
                )
            })
            .collect();
        assert_eq!(
            transitions,
            [
                (WorkflowStepStatus::Pending, WorkflowStepStatus::Pending),
                (WorkflowStepStatus::Running, WorkflowStepStatus::Pending),
                (WorkflowStepStatus::Succeeded, WorkflowStepStatus::Pending),
                (WorkflowStepStatus::Succeeded, WorkflowStepStatus::Running),
                (WorkflowStepStatus::Succeeded, WorkflowStepStatus::Succeeded),
            ],
            "every transition is observed as a complete snapshot"
        );
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
            .add(agent(
                "alpha",
                "alpha about ${topic}",
                Some(detail_schema.clone()),
            ))
            .and_then(|b| {
                b.add(agent(
                    "beta",
                    "beta about ${topic}",
                    Some(string_schema(&["summary"])),
                ))
            })
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
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("explicit workflow builds");

        let provider = ScriptedProvider::new();
        provider.on(
            "alpha about",
            vec![Reply::After(
                std::time::Duration::from_millis(30),
                Box::new(Reply::Text(
                    r#"{"summary":"A-sum","detail":{"n":7}}"#.to_owned(),
                )),
            )],
        );
        provider.on(
            "beta about",
            vec![Reply::Text(r#"{"summary":"B-sum"}"#.to_owned())],
        );
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
        assert_eq!(
            run.steps["beta"].structured_output,
            Some(json!({ "summary": "B-sum" }))
        );
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
                b.with_step_inputs(
                    "final",
                    bindings(&[("gone", "steps.alpha.output.structured.absent")]),
                )
            })
            .and_then(|b| b.with_outputs(bindings(&[("report", "steps.final.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
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
        let error = final_step
            .error
            .as_ref()
            .expect("failed step carries its error");
        assert_eq!(error.code, "WYRD_WORKFLOW_422_MISSING_PARAMETER");
        assert_eq!(
            run.error.as_ref().map(|e| e.code.as_str()),
            Some(error.code.as_str())
        );
        assert_eq!(provider.count("final"), 0);

        // Same-stage failures complete in reverse ID order; the primary error
        // is still the smallest step ID, every failed peer keeps its error, and
        // the later stage stays unstarted.
        let peers = Workflow::builder("peers")
            .add(agent("a_first", "first peer", None))
            .and_then(|b| b.add(agent("z_last", "last peer", None)))
            .and_then(|b| b.add_after(agent("after", "after peers", None), ["a_first", "z_last"]))
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.after.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("peer workflow builds");
        let provider = ScriptedProvider::new();
        let remote = |code: &str| {
            skald_providers::ProviderError::RemoteProblem(Box::new(
                skald_providers::RemoteProblem {
                    code: code.to_owned(),
                    status: 403,
                    message: format!("{code} refused"),
                    field: None,
                    remediation: "fix it".to_owned(),
                },
            ))
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
        assert_eq!(
            run.error.as_ref().map(|e| e.code.as_str()),
            Some("TEST_A_FORBIDDEN")
        );
        assert_eq!(
            run.steps["z_last"].error.as_ref().map(|e| e.code.as_str()),
            Some("TEST_Z_FORBIDDEN")
        );
        assert_eq!(
            run.steps["a_first"].attempts, 1,
            "remote problems are terminal"
        );
        let after = &run.steps["after"];
        assert_eq!(after.status, WorkflowStepStatus::Unstarted);
        assert_eq!(after.attempts, 0);
        assert!(after.started_at.is_none() && after.ended_at.is_none());
    }

    /// Build a workflow of independent text steps named by `prompts`, with
    /// `out` selecting the first step's text.
    fn independent(name: &str, prompts: &[(&str, &str)]) -> Workflow {
        let mut builder = Workflow::builder(name);
        for (step, prompt) in prompts {
            builder = builder
                .add(agent(step, prompt, None))
                .expect("step appends");
        }
        let first = format!("steps.{}.output.text", prompts[0].0);
        builder
            .with_outputs(bindings(&[("out", first.as_str())]))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("independent workflow builds")
    }

    /// Return `workflow` with `retry` and `timeout_seconds` set on `step`.
    fn with_policy(
        mut workflow: Workflow,
        step: &str,
        max_retries: u32,
        initial_backoff_ms: Option<u64>,
        timeout_seconds: Option<u64>,
    ) -> Workflow {
        let target = workflow
            .spec
            .steps
            .iter_mut()
            .find(|candidate| candidate.id == step)
            .expect("step exists");
        target.retry = Some(wyrd_spec::card::workflow::WorkflowRetryPolicy {
            max_retries,
            initial_backoff_ms,
        });
        target.timeout_seconds = timeout_seconds;
        workflow
    }

    /// Provider HTTP status failure.
    fn status(code: u16) -> Reply {
        Reply::Fail(skald_providers::ProviderError::Status {
            provider: "scripted".to_owned(),
            status: code,
            body: "SECRET-BODY".to_owned(),
            retry_after_ms: None,
        })
    }

    /// Run with explicit limits and cancellation.
    async fn run_limited(
        workflow: &Workflow,
        provider: &Arc<ScriptedProvider>,
        limits: WorkflowExecutionLimits,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> wyrd_spec::card::workflow::WorkflowRun {
        workflow
            .run_with_options(
                &WorkflowExecutionDependencies::new(provider.registry()),
                serde_json::Map::new(),
                WorkflowRunOptions {
                    limits,
                    cancellation,
                },
            )
            .await
            .expect("workflow passes pre-dispatch validation")
    }

    /// Scenario 3: concurrency ceiling, retry classification and attempts,
    /// saturating backoff and its cap, step timeout precedence, ordinary peer
    /// drain, cancel and deadline abort-and-drain, and parent-drop cleanup,
    /// all on paused virtual time.
    #[tokio::test(start_paused = true)]
    async fn bounded_attempt_lifecycle() {
        // Concurrency ceiling: five ready steps never exceed two in flight.
        let names = ["s1", "s2", "s3", "s4", "s5"];
        let prompts: Vec<(&str, String)> = names
            .iter()
            .map(|name| (*name, format!("work {name}")))
            .collect();
        let prompt_refs: Vec<(&str, &str)> = prompts
            .iter()
            .map(|(name, prompt)| (*name, prompt.as_str()))
            .collect();
        let workflow = independent("ceiling", &prompt_refs);
        let provider = ScriptedProvider::new();
        for name in names {
            provider.on(
                &format!("work {name}"),
                vec![Reply::After(
                    Duration::from_millis(10),
                    Box::new(Reply::Text("ok".into())),
                )],
            );
        }
        let limits = WorkflowExecutionLimits {
            max_concurrency: std::num::NonZeroUsize::new(2).expect("two is nonzero"),
            ..WorkflowExecutionLimits::default()
        };
        let run = run_limited(&workflow, &provider, limits, CancellationToken::new()).await;
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(provider.peak(), 2);
        assert_eq!(WorkflowExecutionLimits::default().max_concurrency.get(), 8);

        // Eligible failures retry with exponential backoff.
        let workflow = with_policy(
            independent("retry", &[("flaky", "flaky call")]),
            "flaky",
            2,
            Some(100),
            None,
        );
        let provider = ScriptedProvider::new();
        provider.on(
            "flaky call",
            vec![status(503), status(429), Reply::Text("ok".into())],
        );
        let started = tokio::time::Instant::now();
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(run.steps["flaky"].attempts, 3);
        assert!(started.elapsed() >= Duration::from_millis(300));

        // Retry exhaustion keeps the final eligible error; the provider body
        // never reaches the snapshot.
        let workflow = with_policy(
            independent("exhaust", &[("down", "down call")]),
            "down",
            1,
            None,
            None,
        );
        let provider = ScriptedProvider::new();
        provider.on("down call", vec![status(500), status(502)]);
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        let step = &run.steps["down"];
        assert_eq!(
            (step.status, step.attempts),
            (WorkflowStepStatus::Failed, 2)
        );
        let error = step.error.as_ref().expect("failed step has an error");
        assert_eq!(error.code, "WYRD_AGENT_502_PROVIDER");
        assert_eq!(
            error.details,
            json!({ "provider_code": "SKALD_PROVIDERS_5XX_UPSTREAM" })
        );
        assert!(
            !serde_json::to_string(&run)
                .expect("run serializes")
                .contains("SECRET-BODY")
        );

        // Terminal failures never retry even when retries remain.
        let workflow = with_policy(
            independent("terminal", &[("denied", "denied call")]),
            "denied",
            3,
            None,
            None,
        );
        let provider = ScriptedProvider::new();
        provider.on(
            "denied call",
            vec![status(401), Reply::Text("unused".into())],
        );
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(run.steps["denied"].attempts, 1);
        assert_eq!(provider.count("denied call"), 1);

        // Classification table for the typed provider and gateway outcomes.
        let provider_error = |source| {
            skald_agent::AgentError::Provider(skald_runtime::SkaldRuntimeError::Provider {
                provider: skald_spec::ProviderName::OpenAi,
                source,
            })
        };
        let remote = |code: &str| {
            skald_providers::ProviderError::RemoteProblem(Box::new(
                skald_providers::RemoteProblem {
                    code: code.to_owned(),
                    status: 429,
                    message: "gateway".to_owned(),
                    field: Some("model".to_owned()),
                    remediation: "wait".to_owned(),
                },
            ))
        };
        for (error, retryable) in [
            (
                provider_error(remote("WYRD_GATEWAY_429_LIMIT_EXCEEDED")),
                true,
            ),
            (
                provider_error(remote("WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE")),
                true,
            ),
            (
                provider_error(remote("WYRD_GATEWAY_504_DEADLINE_EXCEEDED")),
                true,
            ),
            (
                provider_error(remote("WYRD_GATEWAY_429_BUDGET_EXCEEDED")),
                false,
            ),
            (provider_error(remote("WYRD_GATEWAY_403_FORBIDDEN")), false),
            (
                provider_error(skald_providers::ProviderError::Timeout {
                    provider: "p".into(),
                }),
                true,
            ),
            (
                provider_error(skald_providers::ProviderError::Connect {
                    provider: "p".into(),
                    detail: "refused".into(),
                }),
                true,
            ),
            (
                provider_error(skald_providers::ProviderError::Upstream {
                    provider: "p".into(),
                    status: 408,
                    body: String::new(),
                }),
                true,
            ),
            (
                provider_error(skald_providers::ProviderError::Status {
                    provider: "p".into(),
                    status: 404,
                    body: String::new(),
                    retry_after_ms: None,
                }),
                false,
            ),
            (
                skald_agent::AgentError::Provider(
                    skald_runtime::SkaldRuntimeError::ProviderNotRegistered {
                        provider: skald_spec::ProviderName::OpenAi,
                    },
                ),
                false,
            ),
            (
                skald_agent::AgentError::Timeout {
                    duration: Duration::from_secs(1),
                },
                true,
            ),
            (
                skald_agent::AgentError::StructuredOutputDecode {
                    agent: "a".into(),
                    detail: "bad".into(),
                },
                true,
            ),
            (skald_agent::AgentError::max_iterations("a", 3), false),
            (
                skald_agent::AgentError::ToolNotFound { name: "t".into() },
                false,
            ),
        ] {
            assert_eq!(agent_error_retryable(&error), retryable, "{error:?}");
        }
        let projected = project_agent_error(&provider_error(remote("WYRD_GATEWAY_403_FORBIDDEN")));
        assert_eq!(projected.code, "WYRD_GATEWAY_403_FORBIDDEN");
        assert_eq!(projected.details, json!({ "field": "model" }));
        assert_eq!(projected.remediation, "wait");

        // Saturating exponential backoff with a 30 s cap; zero is immediate.
        assert_eq!(backoff(0, 5), Duration::ZERO);
        assert_eq!(backoff(250, 1), Duration::from_millis(250));
        assert_eq!(backoff(250, 3), Duration::from_secs(1));
        assert_eq!(backoff(1_000, 10), Duration::from_secs(30));
        assert_eq!(backoff(u64::MAX, u32::MAX), Duration::from_secs(30));

        // The step attempt timeout wins over a hanging call and is retryable.
        let workflow = with_policy(
            independent("timeout", &[("slow", "slow call")]),
            "slow",
            1,
            None,
            Some(1),
        );
        let provider = ScriptedProvider::new();
        provider.on("slow call", vec![Reply::Hang, Reply::Hang]);
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        let step = &run.steps["slow"];
        assert_eq!(
            (step.status, step.attempts),
            (WorkflowStepStatus::Failed, 2)
        );
        assert_eq!(
            step.error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_WORKFLOW_504_STEP_TIMEOUT")
        );
        assert_eq!(provider.in_flight(), 0);
        assert_eq!(provider.abandoned(), 2);

        // An ordinary failure drains running peers and starts nothing new.
        let workflow = Workflow::builder("drain")
            .add(agent("fail_fast", "fail fast", None))
            .and_then(|b| b.add(agent("slow_ok", "slow ok", None)))
            .and_then(|b| b.add_after(agent("later", "later call", None), ["slow_ok"]))
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.later.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("drain workflow builds");
        let provider = ScriptedProvider::new();
        provider.on("fail fast", vec![status(400)]);
        provider.on(
            "slow ok",
            vec![Reply::After(
                Duration::from_secs(5),
                Box::new(Reply::Text("done".into())),
            )],
        );
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(run.steps["slow_ok"].status, WorkflowStepStatus::Succeeded);
        assert_eq!(run.steps["slow_ok"].text.as_deref(), Some("done"));
        assert_eq!(run.steps["later"].status, WorkflowStepStatus::Unstarted);
        assert_eq!(provider.count("later call"), 0);

        // Explicit cancellation aborts and drains: the active step is
        // cancelled with its attempt count, dependents are unstarted.
        let workflow = Workflow::builder("cancel")
            .add(agent("held", "held call", None))
            .and_then(|b| b.add(agent("quick", "quick call", None)))
            .and_then(|b| b.add_after(agent("next", "next call", None), ["held"]))
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.next.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("cancel workflow builds");
        let provider = ScriptedProvider::new();
        provider.on("held call", vec![Reply::Hang]);
        provider.on("quick call", vec![Reply::Text("fast".into())]);
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            trigger.cancel();
        });
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            cancellation,
        )
        .await;
        cancel_task.await.expect("cancel task completes");
        assert_eq!(run.status, WorkflowRunStatus::Cancelled);
        assert!(run.error.is_none() && run.outputs.is_empty() && run.ended_at.is_some());
        let held = &run.steps["held"];
        assert_eq!(
            (held.status, held.attempts),
            (WorkflowStepStatus::Cancelled, 1)
        );
        assert!(held.error.is_none() && held.started_at.is_some() && held.ended_at.is_some());
        assert_eq!(run.steps["quick"].status, WorkflowStepStatus::Succeeded);
        assert_eq!(run.steps["next"].status, WorkflowStepStatus::Unstarted);
        assert_eq!((provider.in_flight(), provider.abandoned()), (0, 1));

        // The total deadline aborts and drains with the exact timeout error,
        // and it wins over a later step attempt timeout and a pending backoff.
        let workflow = with_policy(
            independent("deadline", &[("held", "held call")]),
            "held",
            5,
            Some(10_000),
            Some(60),
        );
        let provider = ScriptedProvider::new();
        provider.on("held call", vec![Reply::Hang]);
        let limits = WorkflowExecutionLimits {
            deadline: Some(Duration::from_secs(2)),
            ..WorkflowExecutionLimits::default()
        };
        let run = run_limited(&workflow, &provider, limits, CancellationToken::new()).await;
        assert_eq!(run.status, WorkflowRunStatus::TimedOut);
        assert_eq!(
            run.error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_WORKFLOW_504_RUN_TIMEOUT")
        );
        assert_eq!(run.steps["held"].status, WorkflowStepStatus::Cancelled);
        assert_eq!(run.steps["held"].attempts, 1);
        assert_eq!(provider.in_flight(), 0);

        // Dropping the parent future aborts every owned step task.
        let workflow = independent("dropped", &[("held", "held call")]);
        let provider = ScriptedProvider::new();
        provider.on("held call", vec![Reply::Hang]);
        let dependencies = WorkflowExecutionDependencies::new(provider.registry());
        let outcome = tokio::time::timeout(
            Duration::from_secs(1),
            workflow.run_with_options(
                &dependencies,
                serde_json::Map::new(),
                WorkflowRunOptions::default(),
            ),
        )
        .await;
        assert!(
            outcome.is_err(),
            "the run is still held when its parent is dropped"
        );
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!((provider.in_flight(), provider.abandoned()), (0, 1));

        // A published running step task aborted before its first poll is
        // cancelled with its reserved first attempt and both timestamps, the
        // same as a task aborted after its attempt began.
        let workflow = independent("prepoll", &[("begun", "begun call"), ("idle", "idle call")]);
        let provider = ScriptedProvider::new();
        let dependencies = WorkflowExecutionDependencies::new(provider.registry());
        let plan = ExecutionPlan::build(
            &workflow.spec,
            &workflow.resolved_agents,
            &dependencies,
            WorkflowInput::from(serde_json::Map::new()),
            None,
        )
        .expect("prepoll plans");
        let mut executor = WorkflowExecutor::new(
            "prepoll".to_owned(),
            None,
            plan,
            dependencies.native(),
            WorkflowRunOptions::default(),
        )
        .expect("prepoll prepares");
        let mut aborted = Vec::new();
        for _ in 0..2 {
            let mut tasks = JoinSet::new();
            tasks.spawn(std::future::pending::<StepReport>());
            tasks.abort_all();
            let joined = tasks.join_next().await.expect("one aborted task");
            assert!(joined.as_ref().is_err_and(JoinError::is_cancelled));
            aborted.push(joined);
        }
        let (begun, idle) = (0, 1);
        executor.ledger.start();
        executor.ledger.step_started(begun);
        executor.ledger.step_started(idle);
        executor.attempts[begun].store(1, Ordering::Release);
        executor.attempts[idle].store(1, Ordering::Release);
        assert!(!executor.settle(idle, aborted.pop().expect("aborted idle")));
        assert!(!executor.settle(begun, aborted.pop().expect("aborted begun")));
        let run = executor.ledger.finish(RunEnding::Cancelled, &executor.plan);
        let idle = &run.steps["idle"];
        assert_eq!(
            (idle.status, idle.attempts),
            (WorkflowStepStatus::Cancelled, 1)
        );
        assert!(idle.started_at.is_some() && idle.ended_at.is_some());
        let begun = &run.steps["begun"];
        assert_eq!(
            (begun.status, begun.attempts),
            (WorkflowStepStatus::Cancelled, 1)
        );
        assert!(begun.started_at.is_some() && begun.ended_at.is_some());
    }

    /// Process-wide trace capture and the guard that keeps its provider alive.
    static CAPTURE: std::sync::OnceLock<(TelemetryGuard, TestTraceCapture)> =
        std::sync::OnceLock::new();

    /// Install the production-shaped capture pipeline once and return it.
    ///
    /// # Panics
    /// Panics when another global subscriber is already installed.
    fn capture() -> &'static TestTraceCapture {
        &CAPTURE
            .get_or_init(|| {
                init_test_capture(TelemetryConfig::default()).expect("trace capture installs once")
            })
            .1
    }

    /// The finished `workflow.run` span of `run` and its direct children.
    ///
    /// # Panics
    /// Panics unless exactly one `workflow.run` span carries the run's ID.
    fn run_spans(
        run: &wyrd_spec::card::workflow::WorkflowRun,
    ) -> (CapturedSpan, Vec<CapturedSpan>) {
        let run_id = run.run_id.to_string();
        let spans = capture().finished_since(0);
        let mut roots = spans.iter().filter(|span| {
            span.name == "workflow.run"
                && span.attributes.get("wyrd.workflow.run_id") == Some(&run_id)
        });
        let root = roots.next().expect("one workflow.run span").clone();
        assert!(roots.next().is_none(), "one workflow.run span per run");
        let children = spans
            .into_iter()
            .filter(|span| span.parent_span_id == root.span_id)
            .collect();
        (root, children)
    }

    /// `(attempt, outcome, error code)` of each `workflow.step` span, by
    /// attempt.
    fn attempts(children: &[CapturedSpan]) -> Vec<(String, String, Option<String>)> {
        let mut attempts: Vec<_> = children
            .iter()
            .filter(|span| span.name == "workflow.step")
            .map(|span| {
                (
                    span.attributes["wyrd.workflow.step.attempt"].clone(),
                    span.attributes["wyrd.workflow.step.outcome"].clone(),
                    span.attributes.get("error.type").cloned(),
                )
            })
            .collect();
        attempts.sort();
        attempts
    }

    /// REQ-053: a run emits one `workflow.run` span with its identity, step
    /// count, and terminal status; one child `workflow.step` span per attempt,
    /// so a retry is a sibling span carrying the failed attempt's stable code;
    /// a `workflow.step.backoff` event with the next attempt and delay; and
    /// the Agent's `invoke_agent` and `chat` spans beneath each attempt.
    /// Cancelled attempts close with outcome `cancelled`, and no span carries
    /// Workflow input or model output.
    #[tokio::test(start_paused = true)]
    async fn run_tracing_spans() {
        capture();
        let workflow = with_policy(
            Workflow::builder("traced")
                .add(agent("flaky", "flaky about ${topic}", None))
                .and_then(|b| b.with_inputs(string_inputs(&[("topic", "rust")])))
                .and_then(|b| b.with_step_inputs("flaky", bindings(&[("topic", "input.topic")])))
                .and_then(|b| b.with_outputs(bindings(&[("out", "steps.flaky.output.text")])))
                .and_then(super::super::workflow_surface::WorkflowBuilder::build)
                .expect("traced workflow builds"),
            "flaky",
            1,
            Some(100),
            None,
        );
        let provider = ScriptedProvider::new();
        provider.on(
            "flaky about",
            vec![status(503), Reply::Text("output-pii-marker".into())],
        );
        let run = run_local(&workflow, &provider, json!({ "topic": "input-pii-marker" })).await;
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);

        let (root, children) = run_spans(&run);
        for (key, value) in [
            ("wyrd.workflow.id", "traced"),
            ("wyrd.workflow.step_count", "1"),
            ("wyrd.workflow.status", "succeeded"),
        ] {
            assert_eq!(root.attributes.get(key).map(String::as_str), Some(value));
        }
        assert!(!root.attributes.contains_key("error.type"));
        assert_eq!(
            attempts(&children),
            [
                (
                    "1".to_owned(),
                    "failed".to_owned(),
                    Some("WYRD_AGENT_502_PROVIDER".to_owned())
                ),
                ("2".to_owned(), "succeeded".to_owned(), None),
            ]
        );
        let backoff: Vec<_> = root
            .events
            .iter()
            .filter(|event| event.name == "workflow.step.backoff")
            .map(|event| {
                (
                    event.attributes["wyrd.workflow.step.id"].as_str(),
                    event.attributes["wyrd.workflow.step.next_attempt"].as_str(),
                    event.attributes["wyrd.workflow.step.delay_ms"].as_str(),
                )
            })
            .collect();
        assert_eq!(backoff, [("flaky", "2", "100")]);
        let all = capture().finished_since(0);
        let trace: Vec<_> = all
            .iter()
            .filter(|span| span.trace_id == root.trace_id)
            .collect();
        for step in children.iter().filter(|span| span.name == "workflow.step") {
            let agents: Vec<_> = trace
                .iter()
                .filter(|span| span.parent_span_id == step.span_id)
                .collect();
            assert_eq!(agents.len(), 1);
            assert_eq!(agents[0].name, "invoke_agent");
            assert!(
                trace
                    .iter()
                    .any(|span| span.name == "chat" && span.parent_span_id == agents[0].span_id)
            );
        }
        for span in &trace {
            for value in span.attributes.values() {
                assert!(!value.contains("pii-marker"), "{}: {value}", span.name);
            }
            for event in &span.events {
                for value in event.attributes.values() {
                    assert!(!value.contains("pii-marker"), "{}: {value}", event.name);
                }
            }
        }

        // An attempt interrupted by cancellation closes as cancelled, and the
        // run span records the terminal status.
        let provider = ScriptedProvider::new();
        provider.on("flaky about", vec![Reply::Hang]);
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            trigger.cancel();
        });
        let run = run_limited(
            &workflow,
            &provider,
            WorkflowExecutionLimits::default(),
            cancellation,
        )
        .await;
        cancel_task.await.expect("cancel task completes");
        assert_eq!(run.status, WorkflowRunStatus::Cancelled);
        let (root, children) = run_spans(&run);
        assert_eq!(
            root.attributes
                .get("wyrd.workflow.status")
                .map(String::as_str),
            Some("cancelled")
        );
        assert!(matches!(root.status, CapturedSpanStatus::Error(_)));
        assert_eq!(
            attempts(&children),
            [("1".to_owned(), "cancelled".to_owned(), None)]
        );
    }

    /// Journal that accepts every event but never settles a terminal
    /// `AgentFinish` or `AgentError` append, counting those appends.
    #[derive(Default)]
    struct HeldTerminalJournal {
        /// Terminal appends begun.
        begun: AtomicUsize,
        /// Terminal appends begun whose future is still alive.
        held: AtomicUsize,
    }

    /// Decrements [`HeldTerminalJournal::held`] when a held terminal append's
    /// future is dropped.
    struct HeldAppend<'a>(&'a AtomicUsize);

    impl Drop for HeldAppend<'_> {
        /// Release the held count.
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[async_trait::async_trait]
    impl skald_agent::Journal for HeldTerminalJournal {
        /// Accept non-terminal events at once; hold a terminal event pending
        /// forever, so only an outer deadline or abort can end the append.
        async fn append(
            &self,
            event: skald_agent::JournalEvent,
        ) -> Result<(), skald_agent::JournalError> {
            if matches!(
                event,
                skald_agent::JournalEvent::AgentFinish { .. }
                    | skald_agent::JournalEvent::AgentError { .. }
            ) {
                self.begun.fetch_add(1, Ordering::SeqCst);
                self.held.fetch_add(1, Ordering::SeqCst);
                let _held = HeldAppend(&self.held);
                std::future::pending::<()>().await;
            }
            Ok(())
        }
    }

    /// Workflow of one step `settle` whose Agent has a one-second timeout and
    /// a journal that never settles its terminal append; the step allows one
    /// retry and an optional step timeout.
    fn held_settlement(journal: &Arc<HeldTerminalJournal>, step_timeout: Option<u64>) -> Workflow {
        let held = agent("settle", "settle call", None)
            .with_journal(Arc::clone(journal) as Arc<dyn skald_agent::Journal>)
            .with_run_config(skald_agent::RunConfig {
                timeout: Some(Duration::from_secs(1)),
                ..skald_agent::RunConfig::default()
            });
        with_policy(
            Workflow::builder("held_settlement")
                .add(held)
                .and_then(|b| b.with_outputs(bindings(&[("out", "steps.settle.output.text")])))
                .and_then(super::super::workflow_surface::WorkflowBuilder::build)
                .expect("held settlement workflow builds"),
            "settle",
            1,
            None,
            step_timeout,
        )
    }

    /// REQ-016/REQ-043: the fixed Agent deadline bounds terminal journal
    /// settlement. Each attempt fails with the retryable Agent timeout, the
    /// retry is exhausted, both attempt spans close failed with that code,
    /// and no held append survives. At an equal instant the step timeout and
    /// the total deadline each take precedence over the Agent deadline.
    #[tokio::test(start_paused = true)]
    async fn agent_deadline_bounds_settlement() {
        capture();
        let journal = Arc::new(HeldTerminalJournal::default());
        let provider = ScriptedProvider::new();
        provider.on(
            "settle call",
            vec![Reply::Text("done".into()), Reply::Text("done".into())],
        );
        let started = tokio::time::Instant::now();
        let run = run_limited(
            &held_settlement(&journal, None),
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        assert!(started.elapsed() >= Duration::from_secs(2));
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        let step = &run.steps["settle"];
        assert_eq!(
            (step.status, step.attempts),
            (WorkflowStepStatus::Failed, 2)
        );
        assert_eq!(
            step.error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_AGENT_504_TIMEOUT")
        );
        assert_eq!(journal.begun.load(Ordering::SeqCst), 2);
        assert_eq!(journal.held.load(Ordering::SeqCst), 0);
        assert_eq!(provider.in_flight(), 0);
        let (_, children) = run_spans(&run);
        let timed_out = Some("WYRD_AGENT_504_TIMEOUT".to_owned());
        assert_eq!(
            attempts(&children),
            [
                ("1".to_owned(), "failed".to_owned(), timed_out.clone()),
                ("2".to_owned(), "failed".to_owned(), timed_out),
            ]
        );

        // An equal step timeout wins over the Agent deadline.
        let journal = Arc::new(HeldTerminalJournal::default());
        let provider = ScriptedProvider::new();
        provider.on("settle call", vec![Reply::Hang, Reply::Hang]);
        let run = run_limited(
            &held_settlement(&journal, Some(1)),
            &provider,
            WorkflowExecutionLimits::default(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(
            run.steps["settle"].error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_WORKFLOW_504_STEP_TIMEOUT")
        );
        assert_eq!(provider.in_flight(), 0);

        // An equal total deadline wins over both.
        let provider = ScriptedProvider::new();
        provider.on("settle call", vec![Reply::Hang]);
        let limits = WorkflowExecutionLimits {
            deadline: Some(Duration::from_secs(1)),
            ..WorkflowExecutionLimits::default()
        };
        let run = run_limited(
            &held_settlement(&journal, Some(1)),
            &provider,
            limits,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(run.status, WorkflowRunStatus::TimedOut);
        assert_eq!(run.steps["settle"].status, WorkflowStepStatus::Cancelled);
        assert_eq!(provider.in_flight(), 0);
    }

    /// Local tool that panics when invoked.
    struct PanickingTool;

    #[async_trait::async_trait]
    impl skald_tool::AgentTool for PanickingTool {
        /// Name the model calls.
        fn name(&self) -> &'static str {
            "explode"
        }

        /// Fixed description.
        fn description(&self) -> &'static str {
            "panics on invocation"
        }

        /// Accepts any object.
        fn input_schema(&self) -> serde_json::Value {
            json!({ "type": "object" })
        }

        /// No declared output.
        fn output_schema(&self) -> serde_json::Value {
            json!({})
        }

        /// Panic inside the attempt.
        ///
        /// # Panics
        ///
        /// Always panics.
        async fn invoke(
            &self,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value, skald_tool::ToolError> {
            panic!("caller-supplied tool panicked")
        }
    }

    /// REQ-053: a panicking local tool fails the step and run with the
    /// non-retryable `WYRD_WORKFLOW_500_INTERNAL`, and its attempt span
    /// records the same failed outcome and code rather than `cancelled`.
    #[tokio::test(start_paused = true)]
    async fn attempt_panic_matches_span() {
        capture();
        let boom = agent("boom", "boom call", None).add_tool(Arc::new(PanickingTool));
        let workflow = with_policy(
            Workflow::builder("panicking")
                .add(boom)
                .and_then(|b| b.with_outputs(bindings(&[("out", "steps.boom.output.text")])))
                .and_then(super::super::workflow_surface::WorkflowBuilder::build)
                .expect("panicking workflow builds"),
            "boom",
            2,
            None,
            None,
        );
        let provider = ScriptedProvider::new();
        provider.on(
            "boom call",
            vec![Reply::ToolCall("explode".into(), json!({}))],
        );
        let run = run_local(&workflow, &provider, json!({})).await;
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        let step = &run.steps["boom"];
        assert_eq!(
            (step.status, step.attempts),
            (WorkflowStepStatus::Failed, 1)
        );
        assert_eq!(
            step.error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_WORKFLOW_500_INTERNAL")
        );
        let (_, children) = run_spans(&run);
        assert_eq!(
            attempts(&children),
            [(
                "1".to_owned(),
                "failed".to_owned(),
                Some("WYRD_WORKFLOW_500_INTERNAL".to_owned())
            )]
        );
    }

    /// Gateway fake answering per step from scripted replies and recording
    /// every call.
    #[derive(Default)]
    struct FakeGateway {
        /// Recorded calls.
        calls: std::sync::Mutex<Vec<crate::route::WyrdGatewayCall>>,
        /// Replies keyed by step ID.
        replies: std::sync::Mutex<
            BTreeMap<
                String,
                std::collections::VecDeque<
                    Result<skald_spec::ProviderResponse, skald_providers::ProviderError>,
                >,
            >,
        >,
    }

    #[async_trait::async_trait]
    impl crate::route::WyrdGatewayCaller for FakeGateway {
        /// Record the call and pop the step's next reply.
        async fn call(
            &self,
            call: crate::route::WyrdGatewayCall,
            _cancellation: &tokio_util::sync::CancellationToken,
        ) -> Result<skald_spec::ProviderResponse, skald_providers::ProviderError> {
            let step = call.correlation.step_id.clone();
            crate::test_support::lock(&self.calls).push(call);
            crate::test_support::lock(&self.replies)
                .get_mut(&step)
                .and_then(std::collections::VecDeque::pop_front)
                .unwrap_or_else(|| {
                    Err(skald_providers::ProviderError::bad_request(
                        "fake",
                        "unscripted",
                    ))
                })
        }
    }

    /// Scenario 4: step routes resolve with step-over-workflow precedence;
    /// each gateway call carries its Prompt's gateway model identity, its own
    /// immutable fallback, deadline-derived timeout, and run/step/attempt
    /// correlation; native steps never reach the
    /// gateway; tool declarations survive the route; an OpenAI Responses
    /// Agent keeps its native shape through its tool loop; remote problems keep
    /// only safe metadata; a missing gateway refuses the run before dispatch.
    #[tokio::test(start_paused = true)]
    async fn isolated_route_calls() {
        let tool = Arc::new(RecordingTool {
            name: "lookup".to_owned(),
            calls: AtomicUsize::new(0),
        });
        let responses_prompt = openai_responses(
            "gpt-test",
            OpenAiResponsesOptions {
                messages: vec!["responses call".to_owned()],
                ..OpenAiResponsesOptions::default()
            },
        )
        .expect("Responses prompt builds");
        let responses_answer = |output| {
            ProviderResponse::OpenAiResponses(OpenAiResponsesResponse {
                id: "resp".to_owned(),
                object: "response".to_owned(),
                model: "gpt-test".to_owned(),
                status: "completed".to_owned(),
                created_at: 0,
                output,
                usage: None,
                previous_response_id: None,
            })
        };
        let mut workflow = Workflow::builder("routes")
            .add(agent("routed", "routed call", None))
            .and_then(|b| b.add(agent("plain", "plain call", None)))
            .and_then(|b| b.add(agent("local", "local call", None)))
            .and_then(|b| b.add(agent("remote", "remote call", None)))
            .and_then(|b| b.add(agent("tooling", "tooling call", None).with_tool(tool.clone())))
            .and_then(|b| {
                b.add(
                    skald_agent::Agent::new(responses_prompt)
                        .name("responses")
                        .with_tool(tool.clone()),
                )
            })
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.routed.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("route workflow builds");
        let fallback = GatewayFallbackOverride {
            candidates: vec![ModelRef::from_projection("openai/gpt-backup").expect("model ref")],
        };
        workflow.spec.llm_route = Some(LlmRoute::WyrdGateway);
        for step in &mut workflow.spec.steps {
            match step.id.as_str() {
                "routed" => {
                    step.fallback = Some(fallback.clone());
                    step.timeout_seconds = Some(30);
                }
                "local" => step.llm_route = Some(LlmRoute::Native),
                "remote" => {
                    step.retry = Some(WorkflowRetryPolicy {
                        max_retries: 2,
                        initial_backoff_ms: None,
                    });
                }
                _ => {}
            }
        }
        workflow.validate().expect("routed workflow validates");

        let gateway = Arc::new(FakeGateway::default());
        {
            let mut replies = crate::test_support::lock(&gateway.replies);
            replies.insert("routed".into(), [Ok(text_response("via gateway"))].into());
            replies.insert("plain".into(), [Ok(text_response("plain gateway"))].into());
            replies.insert(
                "remote".into(),
                [Err(skald_providers::ProviderError::RemoteProblem(
                    Box::new(skald_providers::RemoteProblem {
                        code: "WYRD_GATEWAY_403_MODEL_FORBIDDEN".into(),
                        status: 403,
                        message: "model is not permitted".into(),
                        field: Some("model".into()),
                        remediation: "grant the model".into(),
                    }),
                ))]
                .into(),
            );
            replies.insert(
                "tooling".into(),
                [
                    Ok(tool_call_response("lookup", &json!({}))),
                    Ok(text_response("tool used")),
                ]
                .into(),
            );
            replies.insert(
                "responses".into(),
                [
                    Ok(responses_answer(vec![OpenAiResponseItem::FunctionCall {
                        call_id: "c1".to_owned(),
                        name: "lookup".to_owned(),
                        arguments: "{}".to_owned(),
                    }])),
                    Ok(responses_answer(vec![OpenAiResponseItem::Message {
                        role: "assistant".to_owned(),
                        content: vec![OpenAiResponseContentPart::OutputText {
                            text: "responses used".to_owned(),
                        }],
                    }])),
                ]
                .into(),
            );
        }
        let native = ScriptedProvider::new();
        native.on("local call", vec![Reply::Text("native answer".into())]);
        let dependencies = WorkflowExecutionDependencies::new(native.registry())
            .with_wyrd_gateway(gateway.clone());
        let run = workflow
            .run_with_options(
                &dependencies,
                serde_json::Map::new(),
                WorkflowRunOptions::default(),
            )
            .await
            .expect("routes are available");

        assert_eq!(native.requests(), vec!["local call".to_owned()]);
        assert_eq!(run.steps["local"].text.as_deref(), Some("native answer"));
        assert_eq!(run.steps["routed"].text.as_deref(), Some("via gateway"));
        assert_eq!(run.steps["tooling"].text.as_deref(), Some("tool used"));
        assert_eq!(
            run.steps["responses"].text.as_deref(),
            Some("responses used")
        );
        assert_eq!(tool.calls.load(Ordering::SeqCst), 2);

        {
            let calls = crate::test_support::lock(&gateway.calls);
            let by_step = |id: &str| -> Vec<&crate::route::WyrdGatewayCall> {
                calls
                    .iter()
                    .filter(|call| call.correlation.step_id == id)
                    .collect()
            };
            let routed = by_step("routed");
            assert_eq!(routed.len(), 1);
            assert_eq!(routed[0].fallback.as_ref(), Some(&fallback));
            assert_eq!(routed[0].model.to_string(), "openai/gpt-test");
            assert!(routed[0].timeout <= Duration::from_secs(30) && !routed[0].timeout.is_zero());
            assert_eq!(routed[0].correlation.run_id, run.run_id);
            assert_eq!(routed[0].correlation.attempt, 1);
            assert!(matches!(
                &routed[0].request,
                ProviderRequest::OpenAiChatCompletion(_)
            ));
            assert_eq!(
                crate::test_support::request_text(&routed[0].request),
                "routed call"
            );
            let plain = by_step("plain");
            assert_eq!(plain.len(), 1);
            assert!(plain[0].fallback.is_none());
            assert_eq!(plain[0].timeout, DEFAULT_GATEWAY_CALL_TIMEOUT);
            let tooling = by_step("tooling");
            assert_eq!(tooling.len(), 2);
            assert!(tooling.iter().all(|call| matches!(
                &call.request,
                ProviderRequest::OpenAiChatCompletion(request)
                    if request.tools.as_ref().is_some_and(|tools| !tools.is_empty())
            )));
            let responses = by_step("responses");
            assert_eq!(responses.len(), 2);
            assert!(responses.iter().all(|call| matches!(
                &call.request,
                ProviderRequest::OpenAiResponses(request)
                    if request.tools.as_ref().is_some_and(|tools| !tools.is_empty())
            )));
            let ProviderRequest::OpenAiResponses(second) = &responses[1].request else {
                unreachable!("asserted above");
            };
            assert!(second.input.items().iter().any(|item| matches!(
                item,
                OpenAiResponseItem::FunctionCallOutput { call_id, .. } if call_id == "c1"
            )));
            assert!(by_step("local").is_empty());
            assert_eq!(by_step("remote").len(), 1, "gateway refusals are terminal");
        }

        let remote = &run.steps["remote"];
        assert_eq!(
            (remote.status, remote.attempts),
            (WorkflowStepStatus::Failed, 1)
        );
        let error = remote.error.as_ref().expect("remote step failed");
        assert_eq!(error.code, "WYRD_GATEWAY_403_MODEL_FORBIDDEN");
        assert_eq!(error.message, "model is not permitted");
        assert_eq!(error.details, json!({ "field": "model" }));
        assert_eq!(error.remediation, "grant the model");
        assert_eq!(run.status, WorkflowRunStatus::Failed);

        // Without a gateway caller the run is refused before any dispatch.
        let native = ScriptedProvider::new();
        let refused = workflow
            .run_with_options(
                &WorkflowExecutionDependencies::new(native.registry()),
                serde_json::Map::new(),
                WorkflowRunOptions::default(),
            )
            .await
            .expect_err("gateway route needs a caller");
        assert_eq!(refused.code(), "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE");
        assert!(native.requests().is_empty());
    }

    /// Scenario 5: an external gateway route reaches only its bound origin
    /// with authored and secret headers, refuses missing bindings, protocol
    /// and origin mismatches, header collisions, plain HTTP in production, and
    /// dialect mismatches before dispatch, never follows redirects, and never
    /// displays secret values.
    #[tokio::test(flavor = "multi_thread")]
    async fn bound_external_gateway_security() {
        let server = MockServer::start().await;
        let origin = url::Url::parse(&server.uri()).expect("mock server uri parses");
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("x-org-secret", "s3cret"))
            .and(header("x-team", "ml"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "r",
                "object": "chat.completion",
                "created": 0,
                "model": "gpt-test",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "external answer" },
                    "finish_reason": "stop"
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let name = CredentialBindingName::new("corp").expect("binding name");
        let binding = |protocol, origin: &url::Url| ExternalGatewayBinding {
            name: name.clone(),
            protocol,
            origin: origin.clone(),
            secret_headers: [(
                http::HeaderName::from_static("x-org-secret"),
                SecretString::from("s3cret"),
            )]
            .into(),
        };
        let bindings_for = |protocol, origin: &url::Url| {
            let mut bindings = ExternalGatewayBindings::new();
            bindings
                .insert(binding(protocol, origin))
                .expect("binding inserts");
            bindings
        };
        let workflow_for = |protocol, base: &str, header_name: &str, binding_name: &str| {
            let mut workflow = Workflow::builder("external")
                .add(agent("ext", "external call", None))
                .and_then(|b| b.with_outputs(bindings(&[("out", "steps.ext.output.text")])))
                .and_then(super::super::workflow_surface::WorkflowBuilder::build)
                .expect("external workflow builds");
            workflow.spec.steps[0].llm_route = Some(LlmRoute::ExtGateway {
                protocol,
                base_url: AbsoluteUrl::new(base.to_owned()).expect("absolute url"),
                headers: [(header_name.to_owned(), "ml".to_owned())].into(),
                credential_binding: CredentialBindingName::new(binding_name).expect("binding name"),
            });
            workflow
        };
        let base = format!("{}/v1", server.uri());
        let native = ScriptedProvider::new();
        let deps = |bindings, profile| {
            WorkflowExecutionDependencies::new(native.registry())
                .with_external_gateways(bindings)
                .with_endpoint_profile(profile)
        };

        let good = workflow_for(ExternalGatewayProtocol::OpenAiChat, &base, "x-team", "corp");
        let run = good
            .run_with_options(
                &deps(
                    bindings_for(ExternalGatewayProtocol::OpenAiChat, &origin),
                    ExternalEndpointProfile::Local,
                ),
                serde_json::Map::new(),
                WorkflowRunOptions::default(),
            )
            .await
            .expect("bound route is available");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
        assert_eq!(run.steps["ext"].text.as_deref(), Some("external answer"));
        assert!(native.requests().is_empty());

        let debug = format!(
            "{:?}",
            binding(ExternalGatewayProtocol::OpenAiChat, &origin)
        );
        assert!(
            debug.contains("x-org-secret") && !debug.contains("s3cret"),
            "{debug}"
        );

        let refusals = [
            (
                good.clone(),
                ExternalGatewayBindings::new(),
                ExternalEndpointProfile::Local,
                "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
            ),
            (
                good.clone(),
                bindings_for(ExternalGatewayProtocol::AnthropicMessages, &origin),
                ExternalEndpointProfile::Local,
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
            ),
            (
                good.clone(),
                bindings_for(
                    ExternalGatewayProtocol::OpenAiChat,
                    &url::Url::parse("http://127.0.0.1:1").expect("origin parses"),
                ),
                ExternalEndpointProfile::Local,
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
            ),
            (
                workflow_for(
                    ExternalGatewayProtocol::OpenAiChat,
                    &base,
                    "X-Org-Secret",
                    "corp",
                ),
                bindings_for(ExternalGatewayProtocol::OpenAiChat, &origin),
                ExternalEndpointProfile::Local,
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
            ),
            (
                good.clone(),
                bindings_for(ExternalGatewayProtocol::OpenAiChat, &origin),
                ExternalEndpointProfile::Production,
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
            ),
            (
                workflow_for(
                    ExternalGatewayProtocol::AnthropicMessages,
                    &base,
                    "x-team",
                    "corp",
                ),
                bindings_for(ExternalGatewayProtocol::AnthropicMessages, &origin),
                ExternalEndpointProfile::Local,
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
            ),
        ];
        for (workflow, bindings, profile, code) in refusals {
            let error = workflow
                .run_with_options(
                    &deps(bindings, profile),
                    serde_json::Map::new(),
                    WorkflowRunOptions::default(),
                )
                .await
                .expect_err("route is refused before dispatch");
            assert_eq!(error.code(), code, "{error}");
        }
        server.verify().await;

        // A redirect is answered, never followed.
        let redirecting = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(307).insert_header(
                "location",
                format!("{}/elsewhere", redirecting.uri()).as_str(),
            ))
            .expect(1)
            .mount(&redirecting)
            .await;
        Mock::given(path("/elsewhere"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&redirecting)
            .await;
        let redirect_origin = url::Url::parse(&redirecting.uri()).expect("uri parses");
        let run = workflow_for(
            ExternalGatewayProtocol::OpenAiChat,
            &format!("{}/v1", redirecting.uri()),
            "x-team",
            "corp",
        )
        .run_with_options(
            &deps(
                bindings_for(ExternalGatewayProtocol::OpenAiChat, &redirect_origin),
                ExternalEndpointProfile::Local,
            ),
            serde_json::Map::new(),
            WorkflowRunOptions::default(),
        )
        .await
        .expect("bound route is available");
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(run.steps["ext"].attempts, 1);
        redirecting.verify().await;

        // Binding configuration is checked on insert.
        let mut bindings = bindings_for(ExternalGatewayProtocol::OpenAiChat, &origin);
        let duplicate = bindings
            .insert(binding(ExternalGatewayProtocol::OpenAiChat, &origin))
            .expect_err("duplicate binding name");
        assert_eq!(duplicate.code(), "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE");
        let mut fresh = ExternalGatewayBindings::new();
        let with_path = url::Url::parse(&base).expect("base parses");
        assert!(
            fresh
                .insert(binding(ExternalGatewayProtocol::OpenAiChat, &with_path))
                .is_err()
        );

        // Bindings may not set transport, routing, forwarding, proxy, or
        // Wyrd-internal names, but may set credential names a Card cannot.
        let with_header = |header_name: &str| ExternalGatewayBinding {
            secret_headers: [(
                http::HeaderName::from_bytes(header_name.as_bytes()).expect("header name"),
                SecretString::from("s3cret"),
            )]
            .into(),
            ..binding(ExternalGatewayProtocol::OpenAiChat, &origin)
        };
        for reserved in [
            "Host",
            "content-length",
            "Transfer-Encoding",
            "connection",
            "te",
            "upgrade",
            "forwarded",
            "X-Forwarded-For",
            "x-forwarded-host",
            "Proxy-Authorization",
            "x-wyrd-access-token",
            "wyrd-request-id",
        ] {
            let error = ExternalGatewayBindings::new()
                .insert(with_header(reserved))
                .expect_err("reserved binding header is refused");
            assert_eq!(
                error.code(),
                "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
                "{reserved}"
            );
            assert!(!format!("{error:?}").contains("s3cret"), "{reserved}");
        }
        for credential in ["Authorization", "x-api-key", "x-auth-token", "x-org-secret"] {
            ExternalGatewayBindings::new()
                .insert(with_header(credential))
                .expect("credential binding header is accepted");
        }

        // A gateway reflecting the bound credential in its refusal keeps its
        // status but never surfaces the credential.
        let reflecting = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad credential: s3cret"))
            .mount(&reflecting)
            .await;
        let reflecting_origin = url::Url::parse(&reflecting.uri()).expect("uri parses");
        let reflecting_base = format!("{}/v1", reflecting.uri());
        let client = skald_providers::ExternalGatewayClient::new(
            skald_providers::EndpointPolicy::new(false),
            url::Url::parse(&reflecting_base).expect("base parses"),
            [(
                http::HeaderName::from_static("x-org-secret"),
                http::HeaderValue::from_static("s3cret"),
            )]
            .into_iter()
            .collect(),
        )
        .expect("client builds");
        let error = client
            .send(
                "gpt-test",
                ProviderRequest::OpenAiChatCompletion(
                    serde_json::from_value(json!({
                        "model": "gpt-test",
                        "messages": [{ "role": "user", "content": "hi" }]
                    }))
                    .expect("chat request decodes"),
                ),
            )
            .await
            .expect_err("gateway refuses");
        assert!(
            matches!(
                error,
                skald_providers::ProviderError::Status { status: 401, .. }
            ),
            "{error:?}"
        );
        assert_eq!(error.code(), "SKALD_PROVIDERS_401_AUTH");
        assert!(!format!("{error}").contains("s3cret"), "{error}");
        assert!(!format!("{error:?}").contains("s3cret"), "{error:?}");
        let run = workflow_for(
            ExternalGatewayProtocol::OpenAiChat,
            &reflecting_base,
            "x-team",
            "corp",
        )
        .run_with_options(
            &deps(
                bindings_for(ExternalGatewayProtocol::OpenAiChat, &reflecting_origin),
                ExternalEndpointProfile::Local,
            ),
            serde_json::Map::new(),
            WorkflowRunOptions::default(),
        )
        .await
        .expect("bound route is available");
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(run.steps["ext"].attempts, 1, "401 stays non-retryable");
        let projected = serde_json::to_string(&run).expect("run serializes");
        assert!(!projected.contains("s3cret"), "{projected}");
    }

    /// A 2xx external gateway answer that fails to decode keeps its
    /// retryable decode code but never quotes the offending value, which may
    /// be a credential the gateway echoed.
    #[tokio::test(flavor = "multi_thread")]
    async fn external_gateway_decode_detail_withheld() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"id":"r","object":"chat.completion","created":"s3cret","model":"m","choices":[]}"#,
            ))
            .expect(1)
            .mount(&server)
            .await;
        let error = skald_providers::ExternalGatewayClient::new(
            skald_providers::EndpointPolicy::new(false),
            url::Url::parse(&format!("{}/v1", server.uri())).expect("base parses"),
            http::HeaderMap::new(),
        )
        .expect("client builds")
        .send(
            "gpt-test",
            ProviderRequest::OpenAiChatCompletion(
                serde_json::from_value(json!({
                    "model": "gpt-test",
                    "messages": [{ "role": "user", "content": "hi" }]
                }))
                .expect("chat request decodes"),
            ),
        )
        .await
        .expect_err("mistyped answer fails to decode");
        assert_eq!(error.code(), "SKALD_PROVIDERS_502_DECODE");
        assert!(
            !format!("{error} {error:?}").contains("s3cret"),
            "{error:?}"
        );
        server.verify().await;
    }

    /// Scenario 6: input, step-result, and run budgets use canonical sizes;
    /// the payload-free terminal reserve is checked before dispatch; an
    /// oversized payload is discarded with the exact 413 projection; and a
    /// cancelled or timed-out run near the ceiling still fits.
    #[tokio::test(start_paused = true)]
    async fn terminal_budget_reserve() {
        let big = "x".repeat(10_000);
        let workflow = Workflow::builder("budget")
            .add(agent("writer", "write ${topic}", None))
            .and_then(|b| b.with_inputs(string_inputs(&[("topic", "rust")])))
            .and_then(|b| b.with_step_inputs("writer", bindings(&[("topic", "input.topic")])))
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.writer.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .expect("budget workflow builds");
        let run_with = |limits: WorkflowExecutionLimits,
                        cancellation: CancellationToken,
                        provider: &Arc<ScriptedProvider>| {
            let dependencies = WorkflowExecutionDependencies::new(provider.registry());
            let workflow = workflow.clone();
            async move {
                workflow
                    .run_with_options(
                        &dependencies,
                        serde_json::Map::from_iter([("topic".to_owned(), json!("go"))]),
                        WorkflowRunOptions {
                            limits,
                            cancellation,
                        },
                    )
                    .await
            }
        };

        // Input above its cap is refused before dispatch.
        let provider = ScriptedProvider::new();
        let limits = WorkflowExecutionLimits {
            max_input_bytes: Some(8),
            ..WorkflowExecutionLimits::default()
        };
        let error = run_with(limits, CancellationToken::new(), &provider)
            .await
            .expect_err("input exceeds its cap");
        assert_eq!(error.code(), "WYRD_WORKFLOW_413_INPUT_TOO_LARGE");

        // A run cap below the payload-free reserve is refused before dispatch.
        let limits = WorkflowExecutionLimits {
            max_run_bytes: Some(4_096),
            ..WorkflowExecutionLimits::default()
        };
        let error = run_with(limits, CancellationToken::new(), &provider)
            .await
            .expect_err("graph cannot reserve its terminal snapshot");
        assert_eq!(error.code(), "WYRD_WORKFLOW_413_GRAPH_TOO_LARGE");
        assert!(provider.requests().is_empty());

        // A step result above its cap is terminal and discarded.
        let provider = ScriptedProvider::new();
        provider.on(
            "write go",
            vec![Reply::Text(big.clone()), Reply::Text(big.clone())],
        );
        let limits = WorkflowExecutionLimits {
            max_step_result_bytes: Some(100),
            ..WorkflowExecutionLimits::default()
        };
        let run = run_with(limits, CancellationToken::new(), &provider)
            .await
            .expect("runs");
        let writer = &run.steps["writer"];
        assert_eq!(
            (writer.status, writer.attempts),
            (WorkflowStepStatus::Failed, 1)
        );
        assert!(writer.text.is_none());
        assert_eq!(
            run.error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE")
        );

        // A payload that would overflow the run snapshot is discarded and the
        // aggregate-size error decides the run.
        let ceiling = 8_000;
        let provider = ScriptedProvider::new();
        provider.on("write go", vec![Reply::Text(big)]);
        let limits = WorkflowExecutionLimits {
            max_run_bytes: Some(ceiling),
            ..WorkflowExecutionLimits::default()
        };
        let run = run_with(limits, CancellationToken::new(), &provider)
            .await
            .expect("runs");
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(
            run.error.as_ref().map(|e| e.code.as_str()),
            Some("WYRD_WORKFLOW_413_RUN_TOO_LARGE")
        );
        assert!(run.steps["writer"].text.is_none());
        assert!(run.canonical_len() <= ceiling, "{}", run.canonical_len());

        // A small payload fits under the same ceiling.
        let provider = ScriptedProvider::new();
        provider.on("write go", vec![Reply::Text("short".into())]);
        let run = run_with(limits, CancellationToken::new(), &provider)
            .await
            .expect("runs");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert!(run.canonical_len() <= ceiling);

        // Text that JSON escaping expands is charged at its serialized size:
        // around the ceiling every run either retains it within the limit or
        // discards it with the aggregate-size error.
        let wide_ceiling = 60_000;
        let wide = WorkflowExecutionLimits {
            max_run_bytes: Some(wide_ceiling),
            ..WorkflowExecutionLimits::default()
        };
        let mut outcomes = BTreeSet::new();
        for repeats in (0..=3_000).step_by(100) {
            let provider = ScriptedProvider::new();
            provider.on("write go", vec![Reply::Text("\"\\\u{1}".repeat(repeats))]);
            let run = run_with(wide, CancellationToken::new(), &provider)
                .await
                .expect("runs");
            assert!(
                run.canonical_len() <= wide_ceiling,
                "{repeats}: {}",
                run.canonical_len()
            );
            if run.status == WorkflowRunStatus::Failed {
                assert_eq!(
                    run.error.as_ref().map(|e| e.code.as_str()),
                    Some("WYRD_WORKFLOW_413_RUN_TOO_LARGE")
                );
            } else {
                assert_eq!(run.status, WorkflowRunStatus::Succeeded);
            }
            outcomes.insert(run.status == WorkflowRunStatus::Succeeded);
        }
        assert_eq!(outcomes.len(), 2, "the sweep crosses the ceiling");

        // Cancellation and timeout near the ceiling still fit.
        for (deadline, cancel_after) in [(None, Some(1)), (Some(Duration::from_secs(1)), None)] {
            let provider = ScriptedProvider::new();
            provider.on("write go", vec![Reply::Hang]);
            let cancellation = CancellationToken::new();
            if let Some(seconds) = cancel_after {
                let trigger = cancellation.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                    trigger.cancel();
                });
            }
            let limits = WorkflowExecutionLimits {
                max_run_bytes: Some(ceiling),
                deadline,
                ..WorkflowExecutionLimits::default()
            };
            let run = run_with(limits, cancellation, &provider)
                .await
                .expect("runs");
            assert!(matches!(
                run.status,
                WorkflowRunStatus::Cancelled | WorkflowRunStatus::TimedOut
            ));
            assert!(run.canonical_len() <= ceiling);
        }

        // Local defaults: eight concurrent steps and no caps.
        assert_eq!(
            WorkflowExecutionLimits::default(),
            WorkflowExecutionLimits {
                max_concurrency: std::num::NonZeroUsize::new(8).expect("nonzero"),
                deadline: None,
                max_input_bytes: None,
                max_step_result_bytes: None,
                max_run_bytes: None,
            }
        );
    }
}
