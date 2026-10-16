//! Agent model calls through the in-process governed gateway.
//!
//! [`ServerWyrdGatewayCaller`] serves server-run Agent judges. It submits each
//! native Skald request to [`GatewayInvocation::run`] as the operation's
//! captured caller, so every call takes the gateway's own invoke decision,
//! admission, routing, accounting, and capture exactly as a public ingress
//! call does. Nothing about one call is stored on the caller, so concurrent
//! calls cannot see each other's deadline. A caller built for a direct
//! verification carries the Card, and optional Run, its agent-check calls
//! are attributed to.

use async_trait::async_trait;
use axum::http::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use skald_agent::{WyrdGatewayCall, WyrdGatewayCaller};
use skald_providers::{ProviderError, RemoteProblem};
use skald_spec::{ProviderRequest, ProviderResponse};
use tokio_util::sync::CancellationToken;
use wyrd_gateway::{IngressDialect, ResponseBody};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{GatewayOperation, ModelRef};

use super::ingress::{anthropic_usage_bound, google_usage_bound};
use super::invocation::{
    GatewayCallAttribution, GatewayCallRequest, GatewayCallSubject, GatewayInvocation,
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
    subject: Option<GatewayCallSubject>,
}

impl ServerWyrdGatewayCaller {
    /// Bind a caller to `state`'s gateway and the run's captured `caller`,
    /// making uncorrelated calls.
    pub(crate) fn new(state: AppState, caller: Caller) -> Self {
        Self {
            state,
            caller,
            subject: None,
        }
    }

    /// Attribute every call to `subject`, a Card and optional Run the
    /// originating operation already resolved in the caller's tenant, such as
    /// a direct verification's subject and Run. Admission records it without
    /// re-authorizing the Card; `None` leaves calls uncorrelated.
    pub(crate) fn attributed(mut self, subject: Option<GatewayCallSubject>) -> Self {
        self.subject = subject;
        self
    }
}

#[async_trait]
impl WyrdGatewayCaller for ServerWyrdGatewayCaller {
    /// Submit one governed model call to the in-process gateway.
    ///
    /// The request is projected before admission, so an unservable dialect
    /// reaches no gateway decision. The gateway evaluates and audits the captured caller's invoke permission
    /// for the requested model and every fallback candidate. Its cancellation
    /// is a child of `cancellation`; the gateway owns any settlement that
    /// continues after this future returns.
    ///
    /// # Errors
    /// Returns [`ProviderError::BadRequest`] for a request dialect the gateway
    /// does not serve or a body that does not serialize;
    /// [`ProviderError::Timeout`] when the run is cancelled;
    /// [`ProviderError::RemoteProblem`] for a gateway refusal or a provider's
    /// non-success answer; and [`ProviderError::Decode`] when a success is not
    /// the dialect's native response.
    ///
    /// The call's span and the gateway spans under it are routed to the
    /// tenant under the captured caller's own pair, so an agent-check call SYSTEM
    /// performs for the request's subject is not recorded as the request.
    #[tracing::instrument(name = "agent.gateway.call", skip_all)]
    async fn call(
        &self,
        call: WyrdGatewayCall,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        wyrd_telemetry::bind_operation_route(wyrd_telemetry::TenantSpanRoute {
            tenant: self.caller.data_tenant_id,
            attribution: self.caller.attribution(),
        });
        let NativeCall {
            operation,
            ingress,
            body,
            answer,
        } = NativeCall::project(call.request, &call.model)?;
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
            timeout: call.timeout,
            subject: self.subject.clone().map(GatewayCallAttribution::Resolved),
            prompt_provider: Some(call.provider),
        };
        let outcome = GatewayInvocation::new(&self.state)
            .run(&self.caller, request, cancellation)
            .await;
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
    /// Provider-bound request body.
    body: Value,
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
    /// # Errors
    /// Returns [`ProviderError::BadRequest`] for a request dialect the
    /// gateway does not serve or a body that does not serialize.
    fn project(request: ProviderRequest, model: &ModelRef) -> Result<Self, ProviderError> {
        let (operation, ingress, answer, body) = match request {
            ProviderRequest::OpenAiChatCompletion(mut body) => {
                body.model = model.to_string();
                (
                    GatewayOperation::ChatCompletions,
                    IngressDialect::OpenAi,
                    Answer::OpenAiChat,
                    encode(&body)?,
                )
            }
            ProviderRequest::OpenAiResponses(mut body) => {
                body.model = model.to_string();
                (
                    GatewayOperation::Responses,
                    IngressDialect::OpenAi,
                    Answer::OpenAiResponses,
                    encode(&body)?,
                )
            }
            ProviderRequest::AnthropicMessage(mut body) => {
                body.model = model.model.to_string();
                (
                    GatewayOperation::ChatCompletions,
                    IngressDialect::AnthropicMessages,
                    Answer::AnthropicMessages,
                    encode(&body)?,
                )
            }
            ProviderRequest::GeminiGenerateContent(body) => (
                GatewayOperation::ChatCompletions,
                IngressDialect::GeminiGenerateContent,
                Answer::GeminiGenerateContent,
                encode(&body)?,
            ),
            ProviderRequest::JevSystemOne(mut body) => {
                body.model = model.model.to_string();
                (
                    GatewayOperation::SystemOne,
                    IngressDialect::Jev,
                    Answer::Jev,
                    encode(&body)?,
                )
            }
            _ => {
                return Err(ProviderError::bad_request(
                    PROVIDER,
                    "the Wyrd gateway does not serve this request dialect",
                ));
            }
        };
        Ok(Self {
            operation,
            ingress,
            body,
            answer,
        })
    }
}

/// Serialize a native request body.
///
/// # Errors
/// Returns [`ProviderError::BadRequest`] when the body does not serialize.
fn encode(body: &impl Serialize) -> Result<Value, ProviderError> {
    serde_json::to_value(body)
        .map_err(|_| ProviderError::bad_request(PROVIDER, "request does not serialize"))
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
