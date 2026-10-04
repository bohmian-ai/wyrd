//! Workflow model calls through the in-process governed gateway.
//!
//! [`ServerWyrdGatewayCaller`] serves `wyrd_gateway` steps of an accepted server
//! Workflow run. It submits each native Skald request to
//! [`GatewayInvocation::run`] as the run's captured caller, so every call
//! takes the gateway's own invoke decision, admission, routing, accounting,
//! and capture exactly as a public ingress call does. Nothing about one call
//! is stored on the caller, so concurrent steps cannot see each other's
//! fallback, deadline, or correlation; correlation travels only in this
//! call's tracing span.

use async_trait::async_trait;
use axum::http::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use skald_providers::{ProviderError, RemoteProblem};
use skald_spec::{ProviderRequest, ProviderResponse};
use skald_workflow::{WyrdGatewayCall, WyrdGatewayCaller};
use tokio_util::sync::CancellationToken;
use wyrd_gateway::{IngressDialect, ResponseBody};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{GatewayOperation, ModelRef};

use super::ingress::{anthropic_usage_bound, google_usage_bound};
use super::invocation::{GatewayCallRequest, GatewayInvocation};
use super::routes::usage_bound;
use crate::components::auth::Caller;
use crate::state::AppState;

/// Provider label on errors raised by in-process Workflow gateway calls.
const PROVIDER: &str = "wyrd_gateway";

/// Workflow gateway caller bound to one accepted run's captured authority.
///
/// `caller` is the verified identity, permission snapshot, and delegation the
/// run was accepted with; it carries no credential, so it cannot be renewed
/// or widened. Cloning is cheap.
#[derive(Clone)]
pub(crate) struct ServerWyrdGatewayCaller {
    /// Server state whose gateway pipeline serves every call.
    state: AppState,
    /// Authority captured when the run was accepted.
    caller: Caller,
}

impl ServerWyrdGatewayCaller {
    /// Bind a caller to `state`'s gateway and the run's captured `caller`.
    pub(crate) fn new(state: AppState, caller: Caller) -> Self {
        Self { state, caller }
    }
}

#[async_trait]
impl WyrdGatewayCaller for ServerWyrdGatewayCaller {
    /// Submit one governed model call to the in-process gateway.
    ///
    /// The request is projected before admission, so an unservable dialect
    /// reaches no gateway decision. The call is not pre-authorized: the
    /// gateway evaluates and audits the captured caller's invoke permission
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
    #[tracing::instrument(
        name = "workflow.gateway.call",
        skip_all,
        fields(
            wyrd.workflow.run_id = %call.correlation.run_id,
            wyrd.workflow.step_id = %call.correlation.step_id,
            wyrd.workflow.attempt = call.correlation.attempt,
        )
    )]
    async fn call(
        &self,
        call: WyrdGatewayCall,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
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
                IngressDialect::OpenAi => usage_bound(operation, &body),
                IngressDialect::AnthropicMessages => anthropic_usage_bound(&body),
                IngressDialect::GeminiGenerateContent | IngressDialect::VertexGenerateContent => {
                    google_usage_bound(&body)
                }
            },
            model: call.model,
            fallback: call.fallback,
            body,
            media: None,
            batch: None,
            deployment: None,
            stream: false,
            timeout: call.timeout,
        };
        let outcome = GatewayInvocation::new(&self.state)
            .run(&self.caller, request, false, cancellation)
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
    /// gateway routes them by `model` alone. A Google GenerateContent body
    /// enters as Vertex when `model` names the `vertex` provider, which the
    /// Workflow derives from a Prompt whose provider is Vertex.
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
                if model.provider.as_str() == "vertex" {
                    IngressDialect::VertexGenerateContent
                } else {
                    IngressDialect::GeminiGenerateContent
                },
                Answer::GeminiGenerateContent,
                encode(&body)?,
            ),
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
