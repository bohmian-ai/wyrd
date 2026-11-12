//! Agent model calls through the in-process governed gateway.
//!
//! [`ServerWyrdGatewayCaller`] serves server-run Agent judges. It submits each
//! native Skald request to [`GatewayInvocation::run`] as the operation's
//! captured caller, so every call takes the gateway's own invoke decision,
//! admission, routing, accounting, and capture exactly as a public ingress
//! call does. Nothing about one call is stored on the caller, so concurrent
//! calls cannot see each other's deadline. A caller built for a direct
//! verification carries the Card, and optional Run, its agent-check calls
//! are attributed to. A caller built for a verification lane first waits for
//! one of that lane's model-call permits, so the lane's concurrency is bounded
//! before the call reaches the gateway's own admission and accounting.

use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use skald_agent::{WyrdGatewayCall, WyrdGatewayCaller};
use skald_providers::{ProviderError, RemoteProblem};
use skald_spec::{ProviderRequest, ProviderResponse};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wyrd_gateway::{IngressDialect, RequestJson, ResponseBody};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{GatewayOperation, ModelRef};

use super::ingress::{anthropic_usage_bound, google_usage_bound};
use super::invocation::{
    GatewayCallAttribution, GatewayCallContext, GatewayCallRequest, GatewayInvocation,
};
use super::routes::usage_bound;
use crate::components::auth::Caller;
use crate::state::AppState;

/// Provider label on errors raised by in-process gateway calls.
const PROVIDER: &str = "wyrd_gateway";

/// Agent gateway caller bound to one operation's captured authority.
///
/// `caller` is the verified identity, permission snapshot, and delegation the
/// operation was accepted with; it carries no credential, so it cannot be renewed
/// or widened. Cloning is cheap.
#[derive(Clone)]
pub(crate) struct ServerWyrdGatewayCaller {
    /// Server state whose gateway pipeline serves every call.
    state: AppState,
    /// Authority captured when the operation was accepted.
    caller: Caller,
    /// Run and Card every call is attributed to, already resolved by the
    /// operation the calls serve; `None` leaves calls uncorrelated.
    subject: Option<GatewayCallContext>,
    /// The verification lane label and its model-call permits each call
    /// holds while it runs; `None` admits calls without a lane bound.
    lane: Option<(&'static str, Arc<Semaphore>)>,
}

impl ServerWyrdGatewayCaller {
    /// Bind a caller to `state`'s gateway and the run's captured `caller`,
    /// making uncorrelated calls.
    pub(crate) fn new(state: AppState, caller: Caller) -> Self {
        Self {
            state,
            caller,
            subject: None,
            lane: None,
        }
    }

    /// Attribute every call to `subject`, a Card and optional Run the
    /// originating operation already resolved in the caller's tenant, such as
    /// a direct verification's subject and Run. Admission records it without
    /// re-authorizing the Card; `None` leaves calls uncorrelated.
    pub(crate) fn attributed(mut self, subject: Option<GatewayCallContext>) -> Self {
        self.subject = subject;
        self
    }

    /// Hold one of `permits`, the model-call capacity of verification lane
    /// `lane`, for each call, waiting for one before the gateway is reached;
    /// the wait and the gateway's admission spend the call's own timeout.
    pub(crate) fn admitted(mut self, lane: &'static str, permits: Arc<Semaphore>) -> Self {
        self.lane = Some((lane, permits));
        self
    }
}

#[async_trait]
impl WyrdGatewayCaller for ServerWyrdGatewayCaller {
    /// Submit one governed model call to the in-process gateway.
    ///
    /// A lane-bound caller first waits, until `cancellation` or the call's
    /// timeout, for one of its lane's model-call permits and holds it until
    /// the call returns. Its deadline is fixed before the wait and bounds the
    /// gateway's snapshot, authorization, routing, and capture-slot wait as
    /// well, so no admission step restarts it and a call admitted to its lane
    /// reaches the provider with that same deadline. The
    /// request is projected before admission, so an unservable dialect
    /// reaches no gateway decision. The gateway evaluates and audits the captured caller's invoke permission
    /// for the requested model and every fallback candidate. Its cancellation
    /// is a child of `cancellation`; the gateway owns any settlement that
    /// continues after this future returns.
    ///
    /// # Errors
    /// Returns [`ProviderError::BadRequest`] for a request dialect the gateway
    /// does not serve, a body that does not serialize, or a lane-bound
    /// timeout no deadline can represent;
    /// [`ProviderError::Timeout`] when the run is cancelled, including while
    /// it waits for its lane, or the call's timeout passes before it is
    /// admitted to its lane;
    /// [`ProviderError::RemoteProblem`] for a gateway refusal or a provider's
    /// non-success answer; and [`ProviderError::Decode`] when a success is not
    /// the dialect's native response.
    ///
    /// The call opens no span of its own: it runs inside the Agent's
    /// inference span, and [`GatewayInvocation::run`] routes its
    /// `gateway.call` span to the captured caller's own pair.
    async fn call(
        &self,
        call: WyrdGatewayCall<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        let NativeCall {
            operation,
            ingress,
            body,
            answer,
        } = NativeCall::project(call.request, &call.model)?;
        // A lane-bound call fixes its deadline before waiting, so the wait and
        // the gateway's admission spend one budget and no step restarts it.
        let (timeout, deadline, _permit) = match &self.lane {
            Some((lane, permits)) => {
                let deadline = tokio::time::Instant::now()
                    .checked_add(call.timeout)
                    .ok_or_else(|| {
                        ProviderError::bad_request(PROVIDER, "timeout is out of range")
                    })?;
                let permit = if let Ok(permit) = Arc::clone(permits).try_acquire_owned() {
                    permit
                } else {
                    tracing::info!(lane, "the model call waits for its lane's admission");
                    tokio::select! {
                        permit = tokio::time::timeout_at(
                            deadline,
                            Arc::clone(permits).acquire_owned(),
                        ) => match permit {
                            Ok(Ok(permit)) => permit,
                            _ => return Err(ProviderError::timeout(PROVIDER)),
                        },
                        () = cancellation.cancelled() => {
                            return Err(ProviderError::timeout(PROVIDER));
                        }
                    }
                };
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    return Err(ProviderError::timeout(PROVIDER));
                }
                (remaining, Some(deadline), Some(permit))
            }
            None => (call.timeout, None, None),
        };
        let request = GatewayCallRequest {
            operation,
            ingress,
            usage_bound: match ingress {
                IngressDialect::OpenAi | IngressDialect::Jev => usage_bound(operation, &body),
                IngressDialect::AnthropicMessages => anthropic_usage_bound(&body),
                IngressDialect::GeminiGenerateContent | IngressDialect::VertexGenerateContent => {
                    google_usage_bound(&body)
                }
            },
            model: call.model,
            fallback: None,
            body,
            media: None,
            batch: None,
            deployment: None,
            stream: false,
            timeout,
            subject: self.subject.clone().map(GatewayCallAttribution::Resolved),
            prompt_provider: Some(call.provider),
        };
        let invocation = GatewayInvocation::new(&self.state);
        let invocation = match deadline {
            Some(deadline) => invocation.bounded(deadline),
            None => invocation,
        };
        let outcome = invocation.run(&self.caller, request, cancellation).await;
        if cancellation.is_cancelled() {
            return Err(ProviderError::timeout(PROVIDER));
        }
        let response = outcome
            .map_err(|failure| ProviderError::RemoteProblem(Box::new(problem(&failure.error))))?;
        if response.status != StatusCode::OK.as_u16() {
            return Err(ProviderError::RemoteProblem(Box::new(provider_refusal(
                response.status,
            ))));
        }
        match response.body {
            ResponseBody::Json(raw) => answer.decode(raw.get()),
            _ => Err(ProviderError::decode(
                PROVIDER,
                "Wyrd gateway answer is not a buffered JSON response",
            )),
        }
    }
}

/// Native response shape a projected call decodes its answer as.
#[derive(Debug, Clone, Copy)]
enum Answer {
    /// `OpenAI` Chat Completions.
    OpenAiChat,
    /// `OpenAI` Responses.
    OpenAiResponses,
    /// Anthropic Messages.
    AnthropicMessages,
    /// Google `GenerateContent`, from Gemini or Vertex.
    GeminiGenerateContent,
    /// Jev `systemone` typed answers.
    Jev,
}

impl Answer {
    /// Decode a successful answer as this call's native response.
    ///
    /// # Errors
    /// Returns [`ProviderError::Decode`] with a fixed message when the body
    /// is not the dialect's response shape.
    fn decode(self, body: &str) -> Result<ProviderResponse, ProviderError> {
        match self {
            Self::OpenAiChat => decode(body).map(ProviderResponse::OpenAiChatCompletion),
            Self::OpenAiResponses => decode(body).map(ProviderResponse::OpenAiResponses),
            Self::AnthropicMessages => decode(body).map(ProviderResponse::AnthropicMessage),
            Self::GeminiGenerateContent => {
                decode(body).map(ProviderResponse::GeminiGenerateContent)
            }
            Self::Jev => decode(body).map(ProviderResponse::JevSystemOne),
        }
    }
}

/// One Skald request projected onto a gateway operation and dialect.
struct NativeCall {
    /// Gateway operation the call performs.
    operation: GatewayOperation,
    /// Dialect of `body`.
    ingress: IngressDialect,
    /// Provider-bound request body, kept as the compact text it was
    /// serialized to.
    body: RequestJson,
    /// Shape the successful answer decodes as.
    answer: Answer,
}

impl NativeCall {
    /// Project `request` for `model` onto its gateway dialect.
    ///
    /// The model always comes from `model`, never from the request body:
    /// `OpenAI` bodies carry its `<provider>/<model>` projection, Anthropic
    /// bodies its provider-native model, and Google bodies none, since the
    /// gateway routes them by `model` alone. Jev bodies carry the
    /// provider-native model and enter the native `systemone` route. A Google
    /// GenerateContent body
    /// enters as Vertex when `model` names the `vertex` provider, which the
    /// Agent derives from a Prompt whose provider is Vertex.
    ///
    /// The borrowed request is serialized once and its text kept, so the
    /// gateway reads and forwards it without decoding or copying it again.
    ///
    /// # Errors
    /// Returns [`ProviderError::BadRequest`] for a request dialect the
    /// gateway does not serve or a body that does not serialize.
    fn project(request: &ProviderRequest, model: &ModelRef) -> Result<Self, ProviderError> {
        let (operation, ingress, answer, body, model) = match request {
            ProviderRequest::OpenAiChatCompletion(body) => (
                GatewayOperation::ChatCompletions,
                IngressDialect::OpenAi,
                Answer::OpenAiChat,
                encode(body)?,
                Some(model.to_string()),
            ),
            ProviderRequest::OpenAiResponses(body) => (
                GatewayOperation::Responses,
                IngressDialect::OpenAi,
                Answer::OpenAiResponses,
                encode(body)?,
                Some(model.to_string()),
            ),
            ProviderRequest::AnthropicMessage(body) => (
                GatewayOperation::ChatCompletions,
                IngressDialect::AnthropicMessages,
                Answer::AnthropicMessages,
                encode(body)?,
                Some(model.model.to_string()),
            ),
            ProviderRequest::GeminiGenerateContent(body) => (
                GatewayOperation::ChatCompletions,
                IngressDialect::GeminiGenerateContent,
                Answer::GeminiGenerateContent,
                encode(body)?,
                None,
            ),
            ProviderRequest::JevSystemOne(body) => (
                GatewayOperation::SystemOne,
                IngressDialect::Jev,
                Answer::Jev,
                encode(body)?,
                Some(model.model.to_string()),
            ),
            _ => {
                return Err(ProviderError::bad_request(
                    PROVIDER,
                    "the Wyrd gateway does not serve this request dialect",
                ));
            }
        };
        let body = match model {
            Some(model) => body.with_model(&model).map_err(|_| unserializable())?,
            None => body,
        };
        Ok(Self {
            operation,
            ingress,
            body,
            answer,
        })
    }
}

/// Serialize a native request body to compact text the gateway keeps; see
/// [`RequestJson::serialize`].
///
/// # Errors
/// Returns [`ProviderError::BadRequest`] when the body does not serialize.
fn encode(body: &impl Serialize) -> Result<RequestJson, ProviderError> {
    RequestJson::serialize(body).map_err(|_| unserializable())
}

/// The refusal of a request body that does not serialize.
fn unserializable() -> ProviderError {
    ProviderError::bad_request(PROVIDER, "request does not serialize")
}

/// Decode a native response body.
///
/// # Errors
/// Returns [`ProviderError::Decode`] with a fixed message, never quoting the
/// body.
fn decode<T: DeserializeOwned>(body: &str) -> Result<T, ProviderError> {
    serde_json::from_str(body).map_err(|_| {
        ProviderError::decode(
            PROVIDER,
            "Wyrd gateway answer is not the dialect's native response",
        )
    })
}

/// Normalize a gateway refusal, keeping only `details.field` of its details.
fn problem(error: &WyrdError) -> RemoteProblem {
    let problem = error.as_problem_json();
    RemoteProblem {
        code: error.code().to_owned(),
        status: error.status(),
        message: problem["detail"]
            .as_str()
            .unwrap_or_else(|| error.title())
            .to_owned(),
        field: problem
            .pointer("/details/field")
            .and_then(Value::as_str)
            .map(str::to_owned),
        remediation: error.remediation().to_owned(),
    }
}

/// Normalize a provider's non-success answer by its `status` alone.
///
/// The relayed body is upstream text, so none of it is kept: the code is the
/// provider category the status maps to, with a fixed message and the
/// provider-call remediation.
fn provider_refusal(status: u16) -> RemoteProblem {
    let status_code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    RemoteProblem {
        code: ProviderError::from_status(PROVIDER, status_code, "", None)
            .code()
            .to_owned(),
        status,
        message: format!("Wyrd gateway answered HTTP {status}"),
        field: None,
        remediation: WyrdError::AgentProviderCall {
            message: String::new(),
            details: serde_json::json!({}),
        }
        .remediation()
        .to_owned(),
    }
}
