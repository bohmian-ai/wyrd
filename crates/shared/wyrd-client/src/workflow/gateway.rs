//! Workflow model calls through the public Wyrd gateway ingress.
//!
//! [`PublicWyrdGatewayCaller`] serves `wyrd_gateway` Workflow routes outside
//! the server. Each call projects its native Skald request onto the matching
//! protocol ingress, carries its own fallback in the `wyrd-gateway-fallback`
//! header, bounds itself by its own remaining time, and stops when the run is
//! cancelled. Nothing about one call is stored on the caller, so concurrent
//! steps cannot see each other's fallback, deadline, or correlation.
//! Correlation travels only in this call's tracing span. Refusals in the
//! protocol's native error envelope are normalized to a redacted
//! [`RemoteProblem`].

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use skald_providers::{ProviderError, RemoteProblem};
use skald_spec::{ProviderRequest, ProviderResponse};
use skald_workflow::{WyrdGatewayCall, WyrdGatewayCaller};
use tokio_util::sync::CancellationToken;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::native::{AnthropicErrorEnvelope, GoogleErrorEnvelope};
use wyrd_spec::gateway::openai::OpenAiErrorEnvelope;
use wyrd_spec::gateway::{FALLBACK_HEADER, ModelRef};

use crate::WyrdClient;

/// Provider label on errors raised by public gateway calls.
const PROVIDER: &str = "wyrd_gateway";

/// Workflow gateway caller over one shared [`WyrdClient`].
///
/// The client's authentication, token cache, and connection pool serve every
/// call; cloning is cheap.
#[derive(Debug, Clone)]
pub struct PublicWyrdGatewayCaller {
    /// Authenticated transport every call is sent through.
    client: WyrdClient,
}

impl PublicWyrdGatewayCaller {
    /// Bind the caller to an assembled client.
    #[must_use]
    pub fn new(client: WyrdClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WyrdGatewayCaller for PublicWyrdGatewayCaller {
    /// Submit one governed model call to its protocol ingress.
    ///
    /// The request is projected before any IO, so an unservable dialect
    /// dispatches nothing. The HTTP exchange is raced against `cancellation`
    /// and bounded by `call.timeout`; either drops the in-flight request. A
    /// request already sent may have been accepted and dispatched by the
    /// gateway; dropping it does not roll that back, and it is never resent.
    ///
    /// # Errors
    /// Returns [`ProviderError::BadRequest`] for a dialect the public gateway
    /// does not serve (including Vertex) or an unencodable request or
    /// fallback; [`ProviderError::Timeout`] when the call's time runs out or
    /// the run is cancelled; [`ProviderError::Connect`] for a transport
    /// failure; [`ProviderError::RemoteProblem`] for a refusal or an
    /// authentication failure; and [`ProviderError::Decode`] when a success is
    /// not the dialect's native response.
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
        let native = NativeCall::project(call.request, &call.model)?;
        let fallback = call
            .fallback
            .as_ref()
            .map(|fallback| {
                fallback.to_header_value().map_err(|_| {
                    ProviderError::bad_request(PROVIDER, "fallback override does not encode")
                })
            })
            .transpose()?;
        let headers: Vec<(&str, &str)> = fallback
            .iter()
            .map(|value| (FALLBACK_HEADER, value.as_str()))
            .collect();
        let send = self
            .client
            .http()
            .post_native(&native.path, native.body, &headers);
        let (status, body) = tokio::select! {
            biased;
            () = cancellation.cancelled() => return Err(ProviderError::timeout(PROVIDER)),
            answer = tokio::time::timeout(call.timeout, send) => match answer {
                Err(_) => return Err(ProviderError::timeout(PROVIDER)),
                Ok(Err(WyrdError::Internal { .. })) => {
                    return Err(ProviderError::Connect {
                        provider: PROVIDER.to_owned(),
                        detail: "Wyrd gateway transport failed".to_owned(),
                    });
                }
                Ok(Err(error)) => {
                    return Err(ProviderError::RemoteProblem(Box::new(wyrd_problem(&error))));
                }
                Ok(Ok(answer)) => answer,
            },
        };
        if status.is_success() {
            native.ingress.decode(&body)
        } else {
            Err(ProviderError::RemoteProblem(Box::new(
                native.ingress.problem(status, &body),
            )))
        }
    }
}

/// Public ingress protocol a projected call is sent to.
#[derive(Debug, Clone, Copy)]
enum Ingress {
    /// `OpenAI` Chat Completions at `/v1/chat/completions`.
    OpenAiChat,
    /// `OpenAI` Responses at `/v1/responses`.
    OpenAiResponses,
    /// Anthropic Messages at `/v1/messages`.
    AnthropicMessages,
    /// Gemini `GenerateContent` under `/v1beta/models`.
    GeminiGenerateContent,
}

impl Ingress {
    /// Decode a successful answer as this protocol's native response.
    ///
    /// # Errors
    /// Returns [`ProviderError::Decode`] with a fixed message when the body
    /// is not the protocol's response shape.
    fn decode(self, body: &[u8]) -> Result<ProviderResponse, ProviderError> {
        match self {
            Self::OpenAiChat => decode(body).map(ProviderResponse::OpenAiChatCompletion),
            Self::OpenAiResponses => decode(body).map(ProviderResponse::OpenAiResponses),
            Self::AnthropicMessages => decode(body).map(ProviderResponse::AnthropicMessage),
            Self::GeminiGenerateContent => {
                decode(body).map(ProviderResponse::GeminiGenerateContent)
            }
        }
    }

    /// Normalize a refusal in this protocol's error envelope.
    ///
    /// Only the status, the envelope's stable Wyrd code, and `OpenAI`'s
    /// `param` are kept. A recognized code takes its message and remediation
    /// from the Wyrd error catalog, never from the body, because a relayed
    /// provider envelope can carry a Wyrd-looking code beside upstream text.
    /// An answer without a Wyrd code gets the provider status code and a
    /// fixed message, so no upstream text is retained.
    fn problem(self, status: StatusCode, body: &[u8]) -> RemoteProblem {
        let (code, field) = match self {
            Self::OpenAiChat | Self::OpenAiResponses => {
                serde_json::from_slice::<OpenAiErrorEnvelope>(body)
                    .ok()
                    .map(|envelope| (envelope.error.code, envelope.error.param))
            }
            Self::AnthropicMessages => serde_json::from_slice::<AnthropicErrorEnvelope>(body)
                .ok()
                .map(|envelope| (envelope.error.code, None)),
            Self::GeminiGenerateContent => serde_json::from_slice::<GoogleErrorEnvelope>(body)
                .ok()
                .map(|envelope| {
                    let code = envelope
                        .error
                        .details
                        .into_iter()
                        .next()
                        .map(|info| info.reason);
                    (code, None)
                }),
        }
        .unwrap_or_default();
        let fixed_message = format!("Wyrd gateway answered HTTP {}", status.as_u16());
        match code.filter(|code| WyrdError::codes().contains(&code.as_str())) {
            Some(code) => {
                let catalog = WyrdError::from_code(&code, String::new(), serde_json::json!({}));
                RemoteProblem {
                    message: catalog
                        .as_ref()
                        .map_or(fixed_message, |error| error.title().to_owned()),
                    remediation: catalog
                        .as_ref()
                        .map_or_else(provider_remediation, |error| error.remediation().to_owned()),
                    code,
                    status: status.as_u16(),
                    field,
                }
            }
            None => RemoteProblem {
                code: ProviderError::from_status(PROVIDER, status, "", None)
                    .code()
                    .to_owned(),
                status: status.as_u16(),
                message: fixed_message,
                field,
                remediation: provider_remediation(),
            },
        }
    }
}

/// One call projected onto its public ingress.
struct NativeCall {
    /// Protocol of the target ingress.
    ingress: Ingress,
    /// Ingress route below the Wyrd base URL.
    path: String,
    /// Serialized native request body.
    body: Bytes,
}

impl NativeCall {
    /// Project `request` for `model` onto its public ingress.
    ///
    /// The model always comes from `model`, never from the request body:
    /// `OpenAI` bodies carry its `<provider>/<model>` projection, Anthropic
    /// bodies its provider-native model, and Gemini puts it in the path.
    ///
    /// # Errors
    /// Returns [`ProviderError::BadRequest`] for a dialect the public gateway
    /// does not serve, including Vertex, or a body that does not serialize.
    fn project(request: ProviderRequest, model: &ModelRef) -> Result<Self, ProviderError> {
        let (ingress, path, body) = match request {
            ProviderRequest::OpenAiChatCompletion(mut body)
            | ProviderRequest::OpenAiChatCompatible {
                request: mut body, ..
            } => {
                body.model = model.to_string();
                (
                    Ingress::OpenAiChat,
                    "/v1/chat/completions".to_owned(),
                    encode(&body)?,
                )
            }
            ProviderRequest::OpenAiResponses(mut body) => {
                body.model = model.to_string();
                (
                    Ingress::OpenAiResponses,
                    "/v1/responses".to_owned(),
                    encode(&body)?,
                )
            }
            ProviderRequest::AnthropicMessage(mut body) => {
                body.model = model.model.to_string();
                (
                    Ingress::AnthropicMessages,
                    "/v1/messages".to_owned(),
                    encode(&body)?,
                )
            }
            ProviderRequest::GeminiGenerateContent(body) => (
                Ingress::GeminiGenerateContent,
                format!(
                    "/v1beta/models/{}:generateContent",
                    urlencoding::encode(model.model.as_str())
                ),
                encode(&body)?,
            ),
            _ => {
                return Err(ProviderError::bad_request(
                    PROVIDER,
                    "the public Wyrd gateway does not serve this request dialect",
                ));
            }
        };
        Ok(Self {
            ingress,
            path,
            body,
        })
    }
}

/// Serialize a native request body.
///
/// # Errors
/// Returns [`ProviderError::BadRequest`] when the body does not serialize.
fn encode(body: &impl Serialize) -> Result<Bytes, ProviderError> {
    serde_json::to_vec(body)
        .map(Bytes::from)
        .map_err(|_| ProviderError::bad_request(PROVIDER, "request does not serialize"))
}

/// Decode a native response body.
///
/// # Errors
/// Returns [`ProviderError::Decode`] with a fixed message, never quoting the
/// body.
fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, ProviderError> {
    serde_json::from_slice(body).map_err(|_| {
        ProviderError::decode(
            PROVIDER,
            "Wyrd gateway answer is not the dialect's native response",
        )
    })
}

/// Normalize a Wyrd error raised before the gateway answered, such as an
/// authentication failure, keeping only `details.field`.
fn wyrd_problem(error: &WyrdError) -> RemoteProblem {
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
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        remediation: error.remediation().to_owned(),
    }
}

/// Remediation of the provider-call category, used for uncoded refusals.
fn provider_remediation() -> String {
    WyrdError::AgentProviderCall {
        message: String::new(),
        details: serde_json::json!({}),
    }
    .remediation()
    .to_owned()
}
