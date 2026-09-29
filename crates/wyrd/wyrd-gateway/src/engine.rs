//! Bounded attempt execution over an admitted call plan.
//!
//! [`GatewayEngine::execute`] walks candidates in plan order. For each
//! candidate it drops deployments this replica recently saw fail, then
//! applies the plan's deterministic call-keyed weighted order to the healthy
//! set, so an unhealthy deployment's share is split by weight. Every attempt resolves its credential first and
//! drops the plaintext when the attempt ends. The caller deadline and
//! cancellation bound the whole call, including credential work.
//!
//! Retry and fallback rules:
//!
//! - a failure before dispatch (credential or connection) moves on and marks
//!   the deployment unhealthy;
//! - an upstream failure after dispatch moves on only for idempotent
//!   operations, and marks the deployment unhealthy;
//! - a rejection of the request or a failure after visible output ends the
//!   call without another attempt.

use std::fmt::{self, Debug, Formatter};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayCallId, GatewayCallOutcome, GatewayOperation, GatewayUsageAmount, ModelRef,
    ProviderDeployment,
};
use wyrd_spec::ids::ProviderDeploymentName;

use skald_providers::MediaAnswer;

use crate::adapter::{BatchAction, IngressDialect, MediaRequest};
use crate::credential::{CredentialResolver, ProviderSecret};
use crate::health::DeploymentHealth;
use crate::routing::{CallPlan, PlannedCandidate, weighted_order};
use crate::snapshot::GatewayTenantSnapshot;

/// One upstream attempt handed to the provider dispatch seam.
#[derive(Debug)]
pub struct ProviderAttempt<'a> {
    /// Logical call.
    pub call_id: GatewayCallId,
    /// One-based attempt ordinal within the call.
    pub ordinal: NonZeroU32,
    /// Requested operation.
    pub operation: GatewayOperation,
    /// Dialect of `body`.
    pub ingress: IngressDialect,
    /// Deployment to call.
    pub deployment: &'a ProviderDeployment,
    /// Resolved credential, when the deployment authenticates.
    pub credential: Option<&'a ProviderSecret>,
    /// Operation request body.
    pub body: &'a Value,
    /// Route and uploaded files of an Images or Audio call.
    pub media: Option<&'a MediaRequest>,
    /// Lifecycle action of a Batches call.
    pub batch: Option<&'a BatchAction>,
    /// Whether the caller asked for an incremental server-sent event answer.
    pub stream: bool,
    /// Whether the admitted capture policy selects response content, so the
    /// adapter builds a credential-scrubbed capture projection of the answer.
    pub capture: bool,
    /// Absolute caller deadline.
    pub deadline: Instant,
    /// Caller cancellation or server drain; a relayed stream observes it
    /// after the answer reaches the caller.
    pub cancel: &'a CancellationToken,
}

/// Successful answer of an attempt, carried to the caller without
/// reserialization.
#[derive(Debug)]
pub enum ResponseBody {
    /// Complete JSON answer: the provider's bytes for native passthrough, or
    /// the single serialization of a translated body.
    Json(Box<RawValue>),
    /// Server-sent event frames relayed as they arrive.
    Events(EventStream),
    /// Non-JSON media answer, such as a text transcript, bounded by the
    /// transport's answer limit and returned to the caller unchanged. When
    /// response capture is selected, a separate credential-free
    /// [`ResponseCapture::Media`] copy may be retained as a governed payload
    /// object; this body itself is never retained.
    Media(MediaAnswer),
    /// Media answer relayed chunk by chunk as it arrives, such as generated
    /// speech; it ends unterminated, so the caller aborts, when the answer
    /// exceeds the transport's answer limit or the relay stops early.
    MediaStream {
        /// The provider's `content-type`.
        content_type: String,
        /// Answer bytes in delivery order.
        events: EventStream,
    },
}

impl ResponseBody {
    /// Whether the answer is still being relayed, so its terminal result is
    /// known only once its [`StreamEnd`] arrives.
    #[must_use]
    pub const fn is_stream(&self) -> bool {
        matches!(self, Self::Events(_) | Self::MediaStream { .. })
    }
}

/// Incremental answer relayed from one upstream stream.
///
/// The frame channel is bounded, so a slow caller stops upstream reads, and
/// dropping the stream aborts the upstream request. [`Self::recv`] never lets
/// an incomplete stream look complete: the relay marks the stream terminated
/// only after the protocol's success terminator or a delivered terminal error
/// frame, and a stream that closes otherwise ends in [`StreamAborted`].
#[derive(Debug)]
pub struct EventStream {
    /// Server-sent event bytes in delivery order; closes when the relay ends.
    pub(crate) frames: mpsc::Receiver<Vec<u8>>,
    /// Set by the relay, before it closes `frames`, once the stream ended
    /// explicitly with success or a delivered terminal error.
    terminated: Arc<AtomicBool>,
    /// Whether [`Self::recv`] already reported the abort.
    aborted: bool,
    /// Terminal result the relay always sends once the stream ends; closes
    /// without a value only when the relay task itself was dropped, which the
    /// owner treats as cancellation. Taken by the owner that accounts the call.
    pub end: Option<oneshot::Receiver<StreamEnd>>,
}

/// Terminal result of one relayed stream, which settles the provisional
/// success recorded when the stream opened.
#[derive(Debug)]
pub struct StreamEnd {
    /// `Succeeded` only after the protocol's success terminator or a clean
    /// media end; `Failed` for upstream or protocol failure, a failed or
    /// incomplete Responses stream, and an oversized answer; `TimedOut` at the
    /// deadline; `Cancelled` on cancellation, drain, or caller detachment.
    pub outcome: GatewayCallOutcome,
    /// When the relay stopped.
    pub terminal_at: DateTime<Utc>,
    /// Usage the stream reported; unknown unless its terminator arrived.
    pub usage: AttemptUsage,
    /// Relayed content free of the attempt's credential, only when response
    /// capture was selected; `None` when unselected, when the content exceeded
    /// the capture ceiling, had no JSON form, or held the credential, or when
    /// a media stream did not end cleanly.
    pub capture: Option<ResponseCapture>,
}

/// Abnormal end of a stream that could not carry an in-band terminal error;
/// the caller's transport must abort instead of closing cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the gateway stream ended without a terminal event")]
pub struct StreamAborted;

impl EventStream {
    /// Wraps a relay's `frames`, its `terminated` flag, and its `end`.
    pub(crate) fn new(
        frames: mpsc::Receiver<Vec<u8>>,
        terminated: Arc<AtomicBool>,
        end: oneshot::Receiver<StreamEnd>,
    ) -> Self {
        Self {
            frames,
            terminated,
            aborted: false,
            end: Some(end),
        }
    }

    /// Waits for the next frame.
    ///
    /// Returns `None` once a terminated stream has delivered every frame.
    ///
    /// # Errors
    ///
    /// Yields [`StreamAborted`] once, after the last frame, when the relay
    /// closed without terminating the stream; the caller must abort its
    /// transport rather than end the response cleanly.
    pub async fn recv(&mut self) -> Option<Result<Vec<u8>, StreamAborted>> {
        if let Some(frame) = self.frames.recv().await {
            return Some(Ok(frame));
        }
        if self.aborted || self.terminated.load(Ordering::Acquire) {
            return None;
        }
        self.aborted = true;
        Some(Err(StreamAborted))
    }
}

/// Usage evidence one attempt produced; `None` fields are unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttemptUsage {
    /// Canonical bounded provider-native usage JSON.
    pub provider_usage_json: Option<String>,
    /// Normalized usage amounts.
    pub normalized: Option<Vec<GatewayUsageAmount>>,
}

/// Why an attempt failed, which decides retry and health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// The provider never received the request.
    BeforeDispatch,
    /// The provider may have received the request and failed retryably.
    Upstream,
    /// The provider rejected the request; another attempt would too.
    Rejected,
    /// Response content already reached the caller.
    AfterOutput,
}

impl FailureClass {
    /// Closed metric label of this class.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BeforeDispatch => "before_dispatch",
            Self::Upstream => "upstream",
            Self::Rejected => "rejected",
            Self::AfterOutput => "after_output",
        }
    }
}

/// Capture projection of an answer whose response capture was selected.
///
/// The adapter builds it beside the caller's unchanged answer, so content is
/// judged against the attempt's resolved credential before it can reach
/// capture, and the credential itself never leaves the adapter.
#[derive(Debug, PartialEq)]
pub enum ResponseCapture {
    /// JSON content with every exact occurrence of the credential removed and
    /// no string whose base64 content decodes to it.
    Json(Value),
    /// Raw media bytes, such as generated speech, that hold no exact
    /// occurrence of the credential. Binary content cannot be scrubbed without
    /// changing it, so bytes holding the credential are never projected.
    Media(MediaAnswer),
}

impl ResponseCapture {
    /// JSON projection of `value` with every exact occurrence of the
    /// non-empty `secret` removed, or `None` when a string still decodes, as
    /// base64 or a base64 `data:` URL, to bytes holding it.
    ///
    /// Capture persists decoded inline binary content as governed objects, so
    /// an encoded credential drops the whole projection rather than one value.
    pub(crate) fn json(value: Value, secret: Option<&str>) -> Option<Self> {
        let Some(secret) = secret.filter(|secret| !secret.is_empty()) else {
            return Some(Self::Json(value));
        };
        let value = crate::adapter::without_secret(value, secret);
        (!crate::adapter::holds_encoded_secret(&value, secret)).then_some(Self::Json(value))
    }

    /// Media projection of `answer`, or `None` when its bytes hold an exact
    /// occurrence of the non-empty `secret`.
    pub(crate) fn media(answer: MediaAnswer, secret: Option<&str>) -> Option<Self> {
        let leaked = secret
            .filter(|secret| !secret.is_empty())
            .is_some_and(|secret| {
                answer
                    .bytes
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes())
            });
        (!leaked).then_some(Self::Media(answer))
    }
}

/// Terminal result of one attempt reported by the dispatch seam.
#[derive(Debug)]
pub enum AttemptResult {
    /// The provider completed the operation or began streaming it.
    Completed {
        /// Operation response body.
        body: ResponseBody,
        /// Usage evidence known when the answer began; a stream reports its
        /// usage through [`EventStream::end`].
        usage: AttemptUsage,
        /// Credential-free projection of a buffered answer, present only when
        /// the attempt selected capture and a projection could be made; a
        /// stream reports its content through [`EventStream::end`].
        capture: Option<ResponseCapture>,
    },
    /// The attempt failed.
    Failed {
        /// Retry and health class.
        class: FailureClass,
        /// Usage evidence, possibly partial.
        usage: AttemptUsage,
    },
    /// The provider answered with a non-success status.
    Refused {
        /// Retry and health class.
        class: FailureClass,
        /// Provider HTTP status.
        status: u16,
        /// Answer rendered in the caller's dialect.
        body: Value,
        /// Usage evidence, possibly partial.
        usage: AttemptUsage,
    },
}

/// Provider refusal that ended a call, returned to the caller with the
/// provider's status.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRefusal {
    /// Provider HTTP status.
    pub status: u16,
    /// Answer rendered in the caller's dialect.
    pub body: Value,
}

/// Provider invocation seam. Task-owned adapters implement it; the engine
/// owns ordering, retry, deadline, and credential lifetime around it.
#[async_trait]
pub trait ProviderDispatch: Send + Sync {
    /// Performs one upstream attempt.
    async fn dispatch(&self, attempt: ProviderAttempt<'_>) -> AttemptResult;
}

/// Attributable evidence of one attempt for accounting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    /// One-based ordinal.
    pub ordinal: NonZeroU32,
    /// Deployment attempted.
    pub deployment: ProviderDeploymentName,
    /// Model attempted.
    pub model: ModelRef,
    /// Attempt outcome.
    pub outcome: GatewayCallOutcome,
    /// Whether the provider may have received the request and may bill it.
    pub billable: bool,
    /// Usage evidence.
    pub usage: AttemptUsage,
    /// When the attempt began, before credential resolution.
    pub started_at: DateTime<Utc>,
    /// When the attempt reached its terminal result.
    pub terminal_at: DateTime<Utc>,
}

/// Terminal result of one logical call.
#[derive(Debug)]
pub struct CallExecution {
    /// Call outcome.
    pub outcome: GatewayCallOutcome,
    /// Response of the completing attempt.
    pub response: Option<ResponseBody>,
    /// Model that produced the response.
    pub resolved: Option<ModelRef>,
    /// Every attempt in ordinal order.
    pub attempts: Vec<AttemptRecord>,
    /// Refusal of the last attempt when the call failed on it.
    pub refusal: Option<ProviderRefusal>,
    /// Credential-free capture projection of the completing attempt's
    /// buffered answer, when capture was selected and could be made.
    pub capture: Option<ResponseCapture>,
    /// `gateway.attempt` span of the completing attempt. A stream's outcome is
    /// provisional until its [`StreamEnd`], so the span stays open and
    /// unrecorded for the owner to record; otherwise it is already recorded.
    pub attempt_span: tracing::Span,
}

/// Inputs of one admitted call.
#[derive(Debug, Clone, Copy)]
pub struct CallInput<'a> {
    /// Verified tenant.
    pub tenant: DataTenantId,
    /// Admitted configuration.
    pub snapshot: &'a GatewayTenantSnapshot,
    /// Admitted candidates.
    pub plan: &'a CallPlan,
    /// Operation request body.
    pub body: &'a Value,
    /// Dialect of `body`.
    pub ingress: IngressDialect,
    /// Route and uploaded files of an Images or Audio call.
    pub media: Option<&'a MediaRequest>,
    /// Lifecycle action of a Batches call.
    pub batch: Option<&'a BatchAction>,
    /// Whether the caller asked for an incremental server-sent event answer.
    pub stream: bool,
    /// Whether the admitted capture policy selects response content.
    pub capture: bool,
    /// Absolute caller deadline.
    pub deadline: Instant,
    /// Caller cancellation or server drain.
    pub cancel: &'a CancellationToken,
}

/// Dependency-owning gateway execution component.
pub struct GatewayEngine {
    /// Per-attempt credential resolver.
    resolver: CredentialResolver,
    /// Replica-local deployment health.
    health: DeploymentHealth,
    /// Provider invocation seam.
    dispatch: Arc<dyn ProviderDispatch>,
}

impl Debug for GatewayEngine {
    /// Formats the engine with its health map; the resolver and dispatch seam
    /// carry no displayable state and are elided.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayEngine")
            .field("health", &self.health)
            .finish_non_exhaustive()
    }
}

/// Step taken after one attempt finishes.
enum Next {
    /// Try the next deployment or candidate.
    Continue,
    /// End the call with this outcome.
    Stop(GatewayCallOutcome),
}

impl GatewayEngine {
    /// Composes the engine from its resolver, health map, and dispatch seam.
    #[must_use]
    pub fn new(
        resolver: CredentialResolver,
        health: DeploymentHealth,
        dispatch: Arc<dyn ProviderDispatch>,
    ) -> Self {
        Self {
            resolver,
            health,
            dispatch,
        }
    }

    /// Executes an admitted call and returns its attempt evidence.
    ///
    /// Never fails: every terminal path, including an empty or wholly
    /// unhealthy plan, deadline expiry, and cancellation, is an outcome.
    /// A cancelled or timed-out in-flight attempt is recorded with unknown
    /// usage and is billable only if dispatch had begun. A failed call whose
    /// last attempt the provider refused carries that refusal.
    ///
    /// Each attempt runs in a `gateway.attempt` span that records its closed
    /// `outcome` and, when it failed, its `failure_class`, or when it timed out
    /// or was cancelled in flight, its stable `error.code`. A stream that
    /// opened is recorded `Succeeded` provisionally and its span is left for
    /// the owner to record from the stream's [`StreamEnd`].
    pub async fn execute(&self, input: CallInput<'_>) -> CallExecution {
        let mut attempts: Vec<AttemptRecord> = Vec::new();
        let mut refusal = None;
        let retry_safe = matches!(
            input.plan.operation,
            GatewayOperation::ChatCompletions
                | GatewayOperation::Responses
                | GatewayOperation::Embeddings
        );
        for candidate in &input.plan.candidates {
            let healthy = self.healthy(&input, candidate);
            for deployment in weighted_order(&healthy, input.plan.call_id) {
                if Instant::now() >= input.deadline {
                    return finish(GatewayCallOutcome::TimedOut, attempts, None, None, None);
                }
                if input.cancel.is_cancelled() {
                    return finish(GatewayCallOutcome::Cancelled, attempts, None, None, None);
                }
                let ordinal =
                    NonZeroU32::new(u32::try_from(attempts.len() + 1).unwrap_or(u32::MAX))
                        .unwrap_or(NonZeroU32::MIN);
                let dispatched = AtomicBool::new(false);
                let started_at = Utc::now();
                let span = attempt_span(ordinal, deployment);
                let result = tokio::select! {
                    biased;
                    () = input.cancel.cancelled() => Err(GatewayCallOutcome::Cancelled),
                    () = tokio::time::sleep_until(input.deadline) => Err(GatewayCallOutcome::TimedOut),
                    result = self
                        .attempt(&input, deployment, ordinal, &dispatched)
                        .instrument(span.clone()) => Ok(result),
                };
                let record = |outcome, billable, usage| AttemptRecord {
                    ordinal,
                    deployment: deployment.name.clone(),
                    model: candidate.model.clone(),
                    outcome,
                    billable,
                    usage,
                    started_at,
                    terminal_at: Utc::now(),
                };
                let (class, usage) = match result {
                    Err(outcome) => {
                        record_interrupted(&span, outcome);
                        attempts.push(record(
                            outcome,
                            dispatched.load(Ordering::Relaxed),
                            AttemptUsage::default(),
                        ));
                        return finish(outcome, attempts, None, None, None);
                    }
                    Ok(AttemptResult::Completed {
                        body,
                        usage,
                        capture,
                    }) => {
                        if !body.is_stream() {
                            span.record("outcome", outcome_name(GatewayCallOutcome::Succeeded));
                        }
                        attempts.push(record(GatewayCallOutcome::Succeeded, true, usage));
                        return finish(
                            GatewayCallOutcome::Succeeded,
                            attempts,
                            Some((body, candidate.model.clone(), capture)),
                            None,
                            Some(span),
                        );
                    }
                    Ok(AttemptResult::Failed { class, usage }) => {
                        refusal = None;
                        (class, usage)
                    }
                    Ok(AttemptResult::Refused {
                        class,
                        status,
                        body,
                        usage,
                    }) => {
                        refusal = Some(ProviderRefusal { status, body });
                        (class, usage)
                    }
                };
                record_failure(&span, class);
                attempts.push(record(
                    GatewayCallOutcome::Failed,
                    class != FailureClass::BeforeDispatch,
                    usage,
                ));
                match self.after_failure(input.tenant, deployment, class, retry_safe) {
                    Next::Continue => {}
                    Next::Stop(outcome) => return finish(outcome, attempts, None, refusal, None),
                }
            }
        }
        finish(GatewayCallOutcome::Failed, attempts, None, refusal, None)
    }

    /// Returns the healthy snapshot deployments serving `candidate`.
    ///
    /// Keeps snapshot order, which is the name order weighted selection
    /// expects.
    fn healthy<'a>(
        &self,
        input: &CallInput<'a>,
        candidate: &PlannedCandidate,
    ) -> Vec<&'a ProviderDeployment> {
        input
            .snapshot
            .deployments
            .iter()
            .filter(|deployment| {
                candidate
                    .deployments
                    .iter()
                    .any(|planned| planned.name == deployment.name)
                    && self.health.is_healthy(input.tenant, &deployment.name)
            })
            .collect()
    }

    /// Resolves the credential and dispatches one attempt.
    ///
    /// `dispatched` flips just before the provider call so cancellation can
    /// tell a billable in-flight attempt from credential work. A credential
    /// failure is reported as a before-dispatch failure with unknown usage.
    /// [`Self::execute`] runs it inside the attempt's `gateway.attempt` span
    /// carrying the ordinal, deployment, and `gen_ai` provider and
    /// request-model attributes.
    async fn attempt(
        &self,
        input: &CallInput<'_>,
        deployment: &ProviderDeployment,
        ordinal: NonZeroU32,
        dispatched: &AtomicBool,
    ) -> AttemptResult {
        let credential = match self
            .resolver
            .resolve(input.tenant, input.snapshot, deployment)
            .await
        {
            Ok(credential) => credential,
            Err(error) => {
                metrics::counter!("wyrd_gateway_secret_resolution_failures_total").increment(1);
                tracing::warn!(
                    deployment = deployment.name.as_str(),
                    %error,
                    "gateway credential resolution failed; deployment unavailable"
                );
                return AttemptResult::Failed {
                    class: FailureClass::BeforeDispatch,
                    usage: AttemptUsage::default(),
                };
            }
        };
        dispatched.store(true, Ordering::Relaxed);
        self.dispatch
            .dispatch(ProviderAttempt {
                call_id: input.plan.call_id,
                ordinal,
                operation: input.plan.operation,
                ingress: input.ingress,
                deployment,
                credential: credential.as_ref(),
                body: input.body,
                media: input.media,
                batch: input.batch,
                stream: input.stream,
                capture: input.capture,
                deadline: input.deadline,
                cancel: input.cancel,
            })
            .await
    }

    /// Applies health and retry rules to one failed attempt.
    fn after_failure(
        &self,
        tenant: DataTenantId,
        deployment: &ProviderDeployment,
        class: FailureClass,
        retry_safe: bool,
    ) -> Next {
        match class {
            FailureClass::BeforeDispatch => {
                self.health.mark_unhealthy(tenant, &deployment.name);
                Next::Continue
            }
            FailureClass::Upstream => {
                self.health.mark_unhealthy(tenant, &deployment.name);
                if retry_safe {
                    Next::Continue
                } else {
                    Next::Stop(GatewayCallOutcome::Failed)
                }
            }
            FailureClass::Rejected | FailureClass::AfterOutput => {
                Next::Stop(GatewayCallOutcome::Failed)
            }
        }
    }
}

/// Builds the call result from the completing attempt's body, model, and
/// capture projection, and its `gateway.attempt` span when one completed.
fn finish(
    outcome: GatewayCallOutcome,
    attempts: Vec<AttemptRecord>,
    completed: Option<(ResponseBody, ModelRef, Option<ResponseCapture>)>,
    refusal: Option<ProviderRefusal>,
    attempt_span: Option<tracing::Span>,
) -> CallExecution {
    let (response, resolved, capture) = match completed {
        Some((body, model, capture)) => (Some(body), Some(model), capture),
        None => (None, None, None),
    };
    CallExecution {
        outcome,
        response,
        resolved,
        attempts,
        refusal,
        capture,
        attempt_span: attempt_span.unwrap_or_else(tracing::Span::none),
    }
}

/// Opens the `gateway.attempt` span of attempt `ordinal` on `deployment`.
///
/// Its `outcome`, `failure_class`, and `error.code` fields start empty and are
/// recorded once the attempt, or for streams its relay, reaches a terminal
/// result.
fn attempt_span(ordinal: NonZeroU32, deployment: &ProviderDeployment) -> tracing::Span {
    tracing::info_span!(
        "gateway.attempt",
        attempt_ordinal = ordinal.get(),
        deployment = deployment.name.as_str(),
        gen_ai.provider.name = deployment.model.provider.as_str(),
        gen_ai.request.model = deployment.model.model.as_str(),
        outcome = tracing::field::Empty,
        failure_class = tracing::field::Empty,
        error.code = tracing::field::Empty,
    )
}

/// Closed snake-case label of `outcome`, shared by spans, metrics, and
/// capture diagnostics.
#[must_use]
pub const fn outcome_name(outcome: GatewayCallOutcome) -> &'static str {
    match outcome {
        GatewayCallOutcome::Succeeded => "succeeded",
        GatewayCallOutcome::Failed => "failed",
        GatewayCallOutcome::Cancelled => "cancelled",
        GatewayCallOutcome::TimedOut => "timed_out",
    }
}

/// Stable Wyrd error code a caller receives for a non-successful `outcome`,
/// shared by spans and capture: upstream unavailability for `Failed`, the
/// exceeded deadline for `TimedOut`, and service unavailability for
/// `Cancelled`; `None` for `Succeeded`.
#[must_use]
pub fn outcome_error_code(outcome: GatewayCallOutcome) -> Option<&'static str> {
    let (message, details) = (String::new(), Value::Null);
    let error = match outcome {
        GatewayCallOutcome::Succeeded => return None,
        GatewayCallOutcome::Failed => WyrdError::GatewayUpstreamUnavailable { message, details },
        GatewayCallOutcome::TimedOut => WyrdError::GatewayDeadlineExceeded { message, details },
        GatewayCallOutcome::Cancelled => WyrdError::ServiceUnavailable { message, details },
    };
    Some(error.code())
}

/// Records an attempt interrupted in flight by the call deadline or
/// cancellation on its `span`: the closed `outcome` and its stable
/// `error.code`.
fn record_interrupted(span: &tracing::Span, outcome: GatewayCallOutcome) {
    span.record("outcome", outcome_name(outcome));
    if let Some(code) = outcome_error_code(outcome) {
        span.record("error.code", code);
    }
}

/// Records a failed attempt of `class`: one provider-failure metric and the
/// `Failed` outcome with its failure class on `span`.
fn record_failure(span: &tracing::Span, class: FailureClass) {
    metrics::counter!("wyrd_gateway_provider_failures_total", "class" => class.label())
        .increment(1);
    span.record("outcome", outcome_name(GatewayCallOutcome::Failed));
    span.record("failure_class", class.label());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{
        CredentialAssignment, GatewayCredentialSnapshot, GatewayCredentialSource,
    };
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::Duration;
    use wyrd_spec::gateway::{GatewayCapturePolicy, ProviderCredentialState};
    use wyrd_spec::ids::{CredentialBindingName, ProviderCredentialName, ProviderId};
    use wyrd_spec::security::SecretRef;

    /// Scripted dispatch recording each attempt's deployment and credential.
    #[derive(Default)]
    struct Recording {
        /// Results returned in order; `None` blocks forever.
        script: Mutex<VecDeque<Option<AttemptResult>>>,
        /// `(deployment, credential)` per dispatch.
        seen: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Recording {
        /// Builds a dispatch returning `script` in order.
        fn new(script: Vec<Option<AttemptResult>>) -> Arc<Self> {
            Arc::new(Self {
                script: Mutex::new(script.into()),
                seen: Mutex::default(),
            })
        }

        /// Deployments dispatched so far.
        fn deployments(&self) -> Vec<String> {
            self.seen
                .lock()
                .expect("seen")
                .iter()
                .map(|(d, _)| d.clone())
                .collect()
        }
    }

    #[async_trait]
    impl ProviderDispatch for Recording {
        /// Records the attempt's deployment and exposed credential, then returns
        /// the next scripted result, blocking forever on a `None` entry.
        async fn dispatch(&self, attempt: ProviderAttempt<'_>) -> AttemptResult {
            self.seen.lock().expect("seen").push((
                attempt.deployment.name.as_str().to_owned(),
                attempt.credential.map(|c| c.expose().to_owned()),
            ));
            let next = self.script.lock().expect("script").pop_front().flatten();
            match next {
                Some(result) => result,
                None => std::future::pending().await,
            }
        }
    }

    /// Completed result with provider-native usage.
    fn completed() -> AttemptResult {
        AttemptResult::Completed {
            body: ResponseBody::Json(
                RawValue::from_string(r#"{"ok":true}"#.to_owned()).expect("json"),
            ),
            usage: AttemptUsage {
                provider_usage_json: Some(r#"{"total_tokens":3}"#.to_owned()),
                normalized: None,
            },
            capture: None,
        }
    }

    /// Failed result of `class` with unknown usage.
    fn failed(class: FailureClass) -> AttemptResult {
        AttemptResult::Failed {
            class,
            usage: AttemptUsage::default(),
        }
    }

    /// Deployment `name` of `model` authenticating with `credential`.
    fn deployment(name: &str, model: &str, credential: Option<&str>) -> ProviderDeployment {
        let auth = credential.map_or(json!("none"), |c| json!({"bearer": {"credential": c}}));
        serde_json::from_value(json!({
            "name": name,
            "model": ModelRef::from_projection(model).expect("model"),
            "adapter": {"openai_compatible": {"base_url": "https://acme.example/v1"}},
            "auth": auth,
            "capabilities": ["chat_completions", "images"],
            "routing_weight": 1,
        }))
        .expect("deployment")
    }

    /// Snapshot with unauthenticated deployments `dep-a1` and `dep-a2` of
    /// `acme/a` and `dep-b1` of `acme/b`, which uses the `bound` credential
    /// assigned to the system tenant and `acme.example`; `acme/a` falls back
    /// globally to `acme/b`.
    ///
    /// The binding reads `CARGO_PKG_NAME`, which the test runner always sets,
    /// so the resolved plaintext is `wyrd-gateway` without mutating the
    /// process environment.
    fn snapshot() -> GatewayTenantSnapshot {
        let acme = ProviderId::new("acme").expect("provider");
        GatewayTenantSnapshot {
            deployments: vec![
                deployment("dep-a1", "acme/a", None),
                deployment("dep-a2", "acme/a", None),
                deployment("dep-b1", "acme/b", Some("bound")),
            ],
            credentials: vec![GatewayCredentialSnapshot {
                name: ProviderCredentialName::new("bound").expect("name"),
                provider: acme.clone(),
                state: ProviderCredentialState::Active,
                source: GatewayCredentialSource::Environment {
                    binding: CredentialBindingName::new("acme-env").expect("binding"),
                    secret: Some(SecretRef::Env {
                        name: "CARGO_PKG_NAME".to_owned(),
                    }),
                },
                assignment: Some(CredentialAssignment {
                    tenant: DataTenantId::SYSTEM_OWNER,
                    provider: acme,
                    host: Some("acme.example".to_owned()),
                }),
            }],
            fallback: serde_json::from_value(json!({"rules": [{"scope": "global", "candidates": [
                ModelRef::from_projection("acme/b").expect("model")
            ]}]}))
            .expect("fallback"),
            governance: wyrd_spec::gateway::GatewayGovernancePolicy::default(),
            capture: serde_json::from_value::<GatewayCapturePolicy>(
                json!({"mode": "disabled", "payload_fields": [], "version": 1}),
            )
            .expect("capture"),
        }
    }

    /// Engine over `dispatch` with no Vault backends.
    fn engine(dispatch: Arc<Recording>) -> GatewayEngine {
        GatewayEngine::new(
            CredentialResolver::default(),
            DeploymentHealth::default(),
            dispatch,
        )
    }

    /// Runs `operation` against `snap` with a call id whose first `acme/a`
    /// deployment is `dep-a1`.
    async fn run(
        engine: &GatewayEngine,
        snap: &GatewayTenantSnapshot,
        operation: GatewayOperation,
        deadline: Duration,
        cancel: &CancellationToken,
    ) -> CallExecution {
        let call = GatewayCallId::from_uuid(uuid::Uuid::from_u128(7_u128 << 76));
        let plan = CallPlan::new(
            snap,
            call,
            operation,
            ModelRef::from_projection("acme/a").expect("model"),
            None,
        );
        assert_eq!(plan.candidates[0].deployments[0].name.as_str(), "dep-a1");
        engine
            .execute(CallInput {
                tenant: DataTenantId::SYSTEM_OWNER,
                snapshot: snap,
                plan: &plan,
                body: &json!({}),
                ingress: IngressDialect::OpenAi,
                batch: None,
                media: None,
                stream: false,
                capture: false,
                deadline: Instant::now() + deadline,
                cancel,
            })
            .await
    }

    /// Outcomes and billability per attempt.
    fn attempt_outcomes(execution: &CallExecution) -> Vec<(String, GatewayCallOutcome, bool)> {
        execution
            .attempts
            .iter()
            .map(|a| (a.deployment.as_str().to_owned(), a.outcome, a.billable))
            .collect()
    }

    /// Upstream failures of an idempotent call move through deployments and
    /// then cross to the fallback model with its resolved credential; the
    /// failed deployment is avoided by the next call.
    #[tokio::test]
    async fn idempotent_failures_fall_back_and_avoid_unhealthy_deployments() {
        let snap = snapshot();
        let dispatch = Recording::new(vec![
            Some(failed(FailureClass::Upstream)),
            Some(failed(FailureClass::BeforeDispatch)),
            Some(completed()),
            Some(completed()),
        ]);
        let engine = engine(Arc::clone(&dispatch));
        let cancel = CancellationToken::new();
        let first = run(
            &engine,
            &snap,
            GatewayOperation::ChatCompletions,
            Duration::from_secs(5),
            &cancel,
        )
        .await;
        assert_eq!(first.outcome, GatewayCallOutcome::Succeeded);
        assert_eq!(first.resolved, ModelRef::from_projection("acme/b").ok());
        assert_eq!(
            attempt_outcomes(&first),
            [
                ("dep-a1".to_owned(), GatewayCallOutcome::Failed, true),
                ("dep-a2".to_owned(), GatewayCallOutcome::Failed, false),
                ("dep-b1".to_owned(), GatewayCallOutcome::Succeeded, true),
            ]
        );
        assert_eq!(
            dispatch.seen.lock().expect("seen")[2],
            ("dep-b1".to_owned(), Some("wyrd-gateway".to_owned()))
        );
        let second = run(
            &engine,
            &snap,
            GatewayOperation::ChatCompletions,
            Duration::from_secs(5),
            &cancel,
        )
        .await;
        assert_eq!(
            attempt_outcomes(&second),
            [("dep-b1".to_owned(), GatewayCallOutcome::Succeeded, true)]
        );
    }

    /// Weighted selection runs over the healthy deployments only, so an
    /// unhealthy member's share is split by weight instead of passing wholly to
    /// its ring successor.
    #[tokio::test]
    async fn weighted_selection_skips_unhealthy_deployments_before_weighting() {
        const CALLS: u128 = 300;
        let mut snap = snapshot();
        snap.deployments
            .insert(2, deployment("dep-a3", "acme/a", None));
        let dispatch = Recording::new((0..CALLS).map(|_| Some(completed())).collect());
        let engine = engine(Arc::clone(&dispatch));
        let unhealthy = ProviderDeploymentName::new("dep-a1").expect("name");
        engine
            .health
            .mark_unhealthy(DataTenantId::SYSTEM_OWNER, &unhealthy);
        let cancel = CancellationToken::new();
        for low in 0..CALLS {
            let call = GatewayCallId::from_uuid(uuid::Uuid::from_u128((7_u128 << 76) | low));
            let plan = CallPlan::new(
                &snap,
                call,
                GatewayOperation::ChatCompletions,
                ModelRef::from_projection("acme/a").expect("model"),
                None,
            );
            let execution = engine
                .execute(CallInput {
                    tenant: DataTenantId::SYSTEM_OWNER,
                    snapshot: &snap,
                    plan: &plan,
                    body: &json!({}),
                    ingress: IngressDialect::OpenAi,
                    batch: None,
                    media: None,
                    stream: false,
                    capture: false,
                    deadline: Instant::now() + Duration::from_secs(5),
                    cancel: &cancel,
                })
                .await;
            assert_eq!(execution.outcome, GatewayCallOutcome::Succeeded);
        }
        let served = dispatch.deployments();
        let share = |name: &str| served.iter().filter(|d| d.as_str() == name).count();
        assert_eq!(
            (share("dep-a1"), share("dep-a2"), share("dep-a3")),
            (0, 150, 150)
        );
    }

    /// A credential failure never dispatches and falls through; non-idempotent
    /// upstream failures, rejections, and post-output failures never retry.
    #[tokio::test]
    async fn unsafe_failures_never_retry_and_credentials_fail_closed() {
        let mut snap = snapshot();
        snap.deployments[0] = deployment("dep-a1", "acme/a", Some("missing"));
        let dispatch = Recording::new(vec![Some(completed())]);
        let credential = run(
            &engine(Arc::clone(&dispatch)),
            &snap,
            GatewayOperation::ChatCompletions,
            Duration::from_secs(5),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(dispatch.deployments(), ["dep-a2"], "a1 is never dispatched");
        assert_eq!(credential.attempts[0].outcome, GatewayCallOutcome::Failed);
        assert!(!credential.attempts[0].billable);

        for (operation, class) in [
            (GatewayOperation::Images, FailureClass::Upstream),
            (GatewayOperation::ChatCompletions, FailureClass::Rejected),
            (GatewayOperation::ChatCompletions, FailureClass::AfterOutput),
        ] {
            let dispatch = Recording::new(vec![Some(failed(class)), Some(completed())]);
            let execution = run(
                &engine(Arc::clone(&dispatch)),
                &snapshot(),
                operation,
                Duration::from_secs(5),
                &CancellationToken::new(),
            )
            .await;
            assert_eq!(
                execution.outcome,
                GatewayCallOutcome::Failed,
                "{operation:?} {class:?}"
            );
            assert_eq!(
                dispatch.deployments(),
                ["dep-a1"],
                "{operation:?} {class:?}"
            );
        }
    }

    /// A provider refusal that ends the call is returned with its status and
    /// body; a later attempt that fails without an answer clears it.
    #[tokio::test]
    async fn terminal_refusals_are_returned_to_the_caller() {
        let refused = |class| {
            Some(AttemptResult::Refused {
                class,
                status: 400,
                body: json!({"error": "bad"}),
                usage: AttemptUsage::default(),
            })
        };
        let execution = run(
            &engine(Recording::new(vec![refused(FailureClass::Rejected)])),
            &snapshot(),
            GatewayOperation::ChatCompletions,
            Duration::from_secs(5),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(execution.outcome, GatewayCallOutcome::Failed);
        assert_eq!(
            execution.refusal,
            Some(ProviderRefusal {
                status: 400,
                body: json!({"error": "bad"})
            })
        );
        assert!(execution.attempts[0].billable);

        let execution = run(
            &engine(Recording::new(vec![
                refused(FailureClass::Upstream),
                Some(failed(FailureClass::Upstream)),
                Some(failed(FailureClass::Upstream)),
            ])),
            &snapshot(),
            GatewayOperation::ChatCompletions,
            Duration::from_secs(5),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(execution.attempts.len(), 3);
        assert_eq!(execution.refusal, None);
    }

    /// The deadline and cancellation end an in-flight attempt with an
    /// explicit outcome, unknown usage, and no further attempts.
    #[tokio::test(start_paused = true)]
    async fn deadline_and_cancellation_bound_in_flight_attempts() {
        let slow = engine(Recording::new(vec![None]));
        let timed_out = run(
            &slow,
            &snapshot(),
            GatewayOperation::ChatCompletions,
            Duration::from_millis(50),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(timed_out.outcome, GatewayCallOutcome::TimedOut);
        assert_eq!(
            attempt_outcomes(&timed_out),
            [("dep-a1".to_owned(), GatewayCallOutcome::TimedOut, true)]
        );
        assert_eq!(timed_out.attempts[0].usage, AttemptUsage::default());

        let blocked = engine(Recording::new(vec![None]));
        let cancel = CancellationToken::new();
        let snap = snapshot();
        let (execution, ()) = tokio::join!(
            run(
                &blocked,
                &snap,
                GatewayOperation::ChatCompletions,
                Duration::from_mins(1),
                &cancel
            ),
            async {
                tokio::task::yield_now().await;
                cancel.cancel();
            }
        );
        assert_eq!(execution.outcome, GatewayCallOutcome::Cancelled);
        assert_eq!(execution.attempts.len(), 1);
    }
}
