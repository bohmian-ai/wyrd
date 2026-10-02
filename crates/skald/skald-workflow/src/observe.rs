//! Best-effort Workflow observation.
//!
//! [`RunEvents`] is the one boundary through which a run reports its Workflow
//! events. Each callback runs as its own Tokio task, so a panicking observer is
//! contained in that task's join result and logged, never unwinding the run.
//! Nonterminal callbacks race the run's cancellation and the applicable
//! absolute deadline and are aborted when either fires first; the terminal
//! finish callback races the same bounds and is left to complete detached
//! instead of holding the completed snapshot.
//!
//! [`StepResultCeiling`] wraps the observer a step's Agent loop sees so model
//! results whose payload exceeds the Workflow step-result ceiling never reach
//! a payload-bearing observation.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use skald_observer::Observer;
use skald_spec::{ProviderRequest, ProviderResponse};
use tokio::task::JoinError;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

use crate::attempt::StepPayload;

/// Best-effort delivery of one run's Workflow events.
#[derive(Clone)]
pub(crate) struct RunEvents {
    /// Observer active for the run.
    observer: Arc<dyn Observer>,
    /// Run identity reported with every event.
    run_id: Arc<str>,
    /// Run cancellation, which ends any wait on a callback.
    cancellation: CancellationToken,
    /// Total run deadline, which ends any wait on a callback.
    deadline: Option<Instant>,
}

impl RunEvents {
    /// Bind event delivery to the run's observer, identity, and bounds.
    pub(crate) fn new(
        observer: Arc<dyn Observer>,
        run_id: &str,
        cancellation: CancellationToken,
        deadline: Option<Instant>,
    ) -> Self {
        Self {
            observer,
            run_id: Arc::from(run_id),
            cancellation,
            deadline,
        }
    }

    /// Report the run start with its planned step count.
    pub(crate) async fn start(&self, workflow_id: &str, step_count: usize) {
        let (observer, run_id) = self.parts();
        let workflow_id = workflow_id.to_owned();
        self.deliver(None, async move {
            observer
                .on_workflow_start(&run_id, &workflow_id, step_count)
                .await;
        })
        .await;
    }

    /// Report that attempt `attempt` (one-based) of `step_id` began.
    ///
    /// `bound` is the attempt's own deadline, so a slow callback consumes the
    /// attempt's wall-clock budget rather than extending it.
    pub(crate) async fn attempt(&self, step_id: &str, attempt: u32, bound: Option<Instant>) {
        let (observer, run_id) = self.parts();
        let step_id = step_id.to_owned();
        self.deliver(bound, async move {
            observer
                .on_workflow_step_attempt(&run_id, &step_id, attempt)
                .await;
        })
        .await;
    }

    /// Report that an attempt ended; `error_code` is the stable Wyrd code of a
    /// failed attempt and `None` for success.
    pub(crate) async fn result(&self, step_id: &str, attempt: u32, error_code: Option<&str>) {
        let (observer, run_id) = self.parts();
        let step_id = step_id.to_owned();
        let error_code = error_code.map(str::to_owned);
        self.deliver(None, async move {
            observer
                .on_workflow_step_result(&run_id, &step_id, attempt, error_code.as_deref())
                .await;
        })
        .await;
    }

    /// Report the backoff `delay` scheduled before `next_attempt` begins.
    pub(crate) async fn backoff(&self, step_id: &str, next_attempt: u32, delay: Duration) {
        let (observer, run_id) = self.parts();
        let step_id = step_id.to_owned();
        self.deliver(None, async move {
            observer
                .on_workflow_step_backoff(&run_id, &step_id, next_attempt, delay)
                .await;
        })
        .await;
    }

    /// Report the terminal run.
    ///
    /// Waits for the callback only until it completes, the run is cancelled,
    /// or the run deadline passes; otherwise the callback keeps running
    /// detached so it can never withhold the completed snapshot.
    pub(crate) async fn finish(&self, workflow_id: &str, duration: Duration) {
        let (observer, run_id) = self.parts();
        let workflow_id = workflow_id.to_owned();
        let mut handle = tokio::spawn(async move {
            observer
                .on_workflow_finish(&run_id, &workflow_id, duration)
                .await;
        });
        tokio::select! {
            biased;
            joined = &mut handle => log_panic(joined),
            () = self.cancellation.cancelled() => {}
            () = sleep_until(self.deadline) => {}
        }
    }

    /// Owned observer and run identity for a `'static` callback task.
    fn parts(&self) -> (Arc<dyn Observer>, Arc<str>) {
        (Arc::clone(&self.observer), Arc::clone(&self.run_id))
    }

    /// Run `callback` as its own task until it completes, the run is
    /// cancelled, or the earlier of the run deadline and `bound` passes.
    ///
    /// A losing or dropped wait aborts the callback task. A panic is logged
    /// and otherwise ignored.
    async fn deliver<F>(&self, bound: Option<Instant>, callback: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let deadline = [self.deadline, bound].into_iter().flatten().min();
        let mut handle = AbortOnDropHandle::new(tokio::spawn(callback));
        tokio::select! {
            biased;
            joined = &mut handle => log_panic(joined),
            () = self.cancellation.cancelled() => {}
            () = sleep_until(deadline) => {}
        }
    }
}

/// Log a panicked callback; a completed or aborted one needs nothing.
fn log_panic(joined: Result<(), JoinError>) {
    if let Err(error) = joined
        && error.is_panic()
    {
        tracing::warn!(%error, "workflow observer callback panicked");
    }
}

/// Sleep until `deadline`, or forever when there is none.
pub(crate) async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Step-scoped observer that withholds over-ceiling model results.
///
/// Every Agent event is forwarded unchanged except `on_model_result`, which is
/// dropped when the response's step payload — text length, or the JCS size of
/// the parsed structured value for a JSON-schema step — exceeds the Workflow
/// `max_step_result_bytes`. Such a result is later rejected with
/// `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE`, so its payload never crosses the
/// observation boundary.
pub(crate) struct StepResultCeiling {
    /// Observer the run would otherwise use.
    inner: Arc<dyn Observer>,
    /// Workflow step-result ceiling in bytes.
    limit: usize,
    /// Whether the step keeps structured output rather than text.
    structured: bool,
}

impl StepResultCeiling {
    /// Wrap `inner` with the ceiling `limit` for a text or structured step.
    pub(crate) fn new(inner: Arc<dyn Observer>, limit: usize, structured: bool) -> Self {
        Self {
            inner,
            limit,
            structured,
        }
    }

    /// Whether `response` carries a payload above the ceiling.
    ///
    /// Charges exactly as the Workflow does for the retained payload; a
    /// structured step whose text does not parse is charged its raw length.
    fn exceeds(&self, response: &ProviderResponse) -> bool {
        let Some(text) = response.adapter().text() else {
            return false;
        };
        let payload = if self.structured {
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => StepPayload {
                    text: None,
                    structured: Some(value),
                },
                Err(_) => return text.len() > self.limit,
            }
        } else {
            StepPayload {
                text: Some(text.into_owned()),
                structured: None,
            }
        };
        payload.charged_bytes() > self.limit
    }
}

#[async_trait]
impl Observer for StepResultCeiling {
    /// Forward unchanged.
    async fn on_agent_start(
        &self,
        run_id: &str,
        parent_run_id: Option<&str>,
        agent_id: &str,
        input: &str,
        session_id: Option<&str>,
    ) {
        self.inner
            .on_agent_start(run_id, parent_run_id, agent_id, input, session_id)
            .await;
    }

    /// Forward unchanged.
    async fn on_iteration(&self, run_id: &str, agent_id: &str, index: u32) {
        self.inner.on_iteration(run_id, agent_id, index).await;
    }

    /// Forward unchanged.
    async fn on_model_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        provider: &str,
        model: &str,
        request: &ProviderRequest,
    ) {
        self.inner
            .on_model_call(run_id, agent_id, iteration, provider, model, request)
            .await;
    }

    /// Forward only when the response payload fits the step-result ceiling.
    async fn on_model_result(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        finish_reason: &str,
        synthetic: bool,
        response: &ProviderResponse,
    ) {
        if self.exceeds(response) {
            return;
        }
        self.inner
            .on_model_result(
                run_id,
                agent_id,
                iteration,
                finish_reason,
                synthetic,
                response,
            )
            .await;
    }

    /// Forward unchanged.
    async fn on_tool_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        tool_name: &str,
    ) {
        self.inner
            .on_tool_call(run_id, agent_id, iteration, call_id, tool_name)
            .await;
    }

    /// Forward unchanged.
    async fn on_tool_result(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        ok: bool,
    ) {
        self.inner
            .on_tool_result(run_id, agent_id, iteration, call_id, ok)
            .await;
    }

    /// Forward unchanged.
    async fn on_agent_finish(
        &self,
        run_id: &str,
        agent_id: &str,
        finish_reason: &str,
        iterations: u32,
        duration: Duration,
    ) {
        self.inner
            .on_agent_finish(run_id, agent_id, finish_reason, iterations, duration)
            .await;
    }

    /// Forward unchanged.
    async fn on_agent_error(&self, run_id: &str, agent_id: &str, code: &str, message: &str) {
        self.inner
            .on_agent_error(run_id, agent_id, code, message)
            .await;
    }

    /// Forward unchanged.
    async fn on_workflow_start(&self, run_id: &str, workflow_id: &str, step_count: usize) {
        self.inner
            .on_workflow_start(run_id, workflow_id, step_count)
            .await;
    }

    /// Forward unchanged.
    async fn on_workflow_finish(&self, run_id: &str, workflow_id: &str, duration: Duration) {
        self.inner
            .on_workflow_finish(run_id, workflow_id, duration)
            .await;
    }

    /// Forward unchanged.
    async fn on_workflow_step_attempt(&self, run_id: &str, step_id: &str, attempt: u32) {
        self.inner
            .on_workflow_step_attempt(run_id, step_id, attempt)
            .await;
    }

    /// Forward unchanged.
    async fn on_workflow_step_result(
        &self,
        run_id: &str,
        step_id: &str,
        attempt: u32,
        error_code: Option<&str>,
    ) {
        self.inner
            .on_workflow_step_result(run_id, step_id, attempt, error_code)
            .await;
    }

    /// Forward unchanged.
    async fn on_workflow_step_backoff(
        &self,
        run_id: &str,
        step_id: &str,
        next_attempt: u32,
        delay: Duration,
    ) {
        self.inner
            .on_workflow_step_backoff(run_id, step_id, next_attempt, delay)
            .await;
    }
}
