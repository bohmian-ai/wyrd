//! Public gateway inference ingress.
//!
//! Every inference route authenticates through [`GatewayIngress`], which
//! reads the Wyrd access token from the headers its protocol's SDKs send and
//! renders refusals in that protocol's error envelope. The `OpenAI`-compatible
//! handlers live in [`super::routes`]; the Anthropic Messages and Gemini
//! `GenerateContent` handlers here take each provider's native body and
//! answer with its native response or stream. All of them run through the one
//! governed [`GatewayInvocation`], which never forwards caller headers to a
//! provider.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, RawQuery, Request, State};
use axum::http::{HeaderName, Method};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_gateway::IngressDialect;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::native::{
    AnthropicErrorEnvelope, GatewayAnthropicMessage, GatewayAnthropicMessagesRequest,
    GatewayGeminiGenerateContentRequest, GatewayGeminiGenerateContentResponse, GoogleErrorEnvelope,
};
use wyrd_spec::gateway::{GatewayContractError, GatewayOperation, ModelRef};
use wyrd_spec::ids::{ModelId, ProviderId};

use super::invocation::{GatewayCallRequest, GatewayInvocation, invalid_request};
use super::routes::{self, PUBLIC_CALL_TIMEOUT, openai_error, relay, token_bound, typed_body};
use crate::components::auth::Caller;
use crate::components::auth::token_extract::{
    ANTHROPIC_API_KEY_HEADER, GOOGLE_API_KEY_HEADER, extract_gateway_access_token,
    verify_access_token,
};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Protocol of one group of public gateway inference routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayIngress {
    /// `OpenAI`-compatible `/v1` routes.
    OpenAi,
    /// Anthropic Messages at `POST /v1/messages`.
    AnthropicMessages,
    /// Gemini Developer API `GenerateContent` under `/v1beta/models`.
    GeminiGenerateContent,
}

impl GatewayIngress {
    /// Provider SDK header that may carry the Wyrd access token besides the
    /// `Bearer` headers.
    const fn native_header(self) -> Option<HeaderName> {
        match self {
            Self::OpenAi => None,
            Self::AnthropicMessages => Some(ANTHROPIC_API_KEY_HEADER),
            Self::GeminiGenerateContent => Some(GOOGLE_API_KEY_HEADER),
        }
    }

    /// Query parameters the route of `request` accepts; any other parameter,
    /// including a URL-borne token, is refused.
    ///
    /// Only `GET /v1/batches` paginates with `after` and `limit`, and only
    /// Gemini selects its response format with `alt`; every other route takes
    /// no query, so no parameter name can smuggle a token past extraction.
    fn query_names(self, request: &Request) -> &'static [&'static str] {
        match self {
            Self::OpenAi
                if request.method() == Method::GET && request.uri().path() == "/v1/batches" =>
            {
                &["after", "limit"]
            }
            Self::OpenAi | Self::AnthropicMessages => &[],
            Self::GeminiGenerateContent => &["alt"],
        }
    }

    /// Renders a gateway-originated `error` in the protocol's error envelope
    /// with the error's HTTP status and stable Wyrd code.
    pub(crate) fn error(self, error: &WyrdError) -> Response {
        let status = axum::http::StatusCode::from_u16(error.status())
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let problem = error.as_problem_json();
        let message = problem["detail"].as_str().unwrap_or_else(|| error.title());
        let body = match self {
            Self::OpenAi => return openai_error(error),
            Self::AnthropicMessages => {
                wyrd_gateway::anthropic_error_body(status.as_u16(), message, Some(error.code()))
            }
            Self::GeminiGenerateContent => {
                wyrd_gateway::google_error_body(status.as_u16(), message, error.code())
            }
        };
        (status, Json(body)).into_response()
    }

    /// Authenticates one request of this protocol before its handler runs.
    ///
    /// The single accepted credential header is verified as a Wyrd access
    /// token and the principal inserted for the handler's [`Caller`]; a
    /// missing, ambiguous, URL-borne, or invalid credential is refused in the
    /// protocol's envelope without reading the body.
    async fn authenticate(self, state: AppState, mut request: Request, next: Next) -> Response {
        let native = self.native_header();
        let verified = match extract_gateway_access_token(
            request.headers(),
            native.as_ref(),
            request.uri().query(),
            self.query_names(&request),
        ) {
            Ok(token) => verify_access_token(
                state.auth.token_verifier.as_deref(),
                token,
                wyrd_auth_verify::TokenAudience::Wyrd,
            )
            .map_err(|WyrdErrorResponse(error)| error),
            Err(error) => Err(error),
        };
        match verified {
            Ok(principal) => {
                request.extensions_mut().insert(principal);
                next.run(request).await
            }
            Err(error) => self.error(&error),
        }
    }

    /// Wraps `router` so each of its requests authenticates as this protocol.
    fn layer(self, router: OpenApiRouter<AppState>, state: &AppState) -> OpenApiRouter<AppState> {
        router.layer(middleware::from_fn_with_state(
            state.clone(),
            move |State(state): State<AppState>, request: Request, next: Next| {
                self.authenticate(state, request, next)
            },
        ))
    }
}

/// Builds every public gateway inference route with its protocol's
/// authentication.
pub fn gateway_ingress_router(state: &AppState) -> OpenApiRouter<AppState> {
    let openai = OpenApiRouter::new()
        .routes(routes!(routes::chat_completions))
        .routes(routes!(routes::responses))
        .routes(routes!(routes::embeddings))
        .routes(routes!(routes::image_generations))
        .routes(routes!(routes::image_edits))
        .routes(routes!(routes::image_variations))
        .routes(routes!(routes::audio_speech))
        .routes(routes!(routes::audio_transcriptions))
        .routes(routes!(routes::audio_translations))
        .routes(routes!(routes::models))
        .routes(routes!(routes::upload_file))
        .routes(routes!(routes::get_file, routes::delete_file))
        .routes(routes!(routes::file_content))
        .routes(routes!(routes::create_batch, routes::list_batches))
        .routes(routes!(routes::get_batch))
        .routes(routes!(routes::cancel_batch));
    let anthropic = OpenApiRouter::new().routes(routes!(anthropic_messages));
    let gemini = OpenApiRouter::new().routes(routes!(gemini_generate_content));
    GatewayIngress::OpenAi
        .layer(openai, state)
        .merge(GatewayIngress::AnthropicMessages.layer(anthropic, state))
        .merge(GatewayIngress::GeminiGenerateContent.layer(gemini, state))
}

/// Provider identity fixed by a native route.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `model` when `model` is not a valid
/// model identifier.
fn native_model(provider: &str, model: &str) -> Result<ModelRef, WyrdError> {
    let invalid = || {
        invalid_request(GatewayContractError::new(
            "model",
            "is not a valid model id",
        ))
    };
    Ok(ModelRef {
        provider: ProviderId::new(provider).map_err(|_| invalid())?,
        model: ModelId::new(model).map_err(|_| invalid())?,
    })
}

#[utoipa::path(
    post,
    path = "/v1/messages",
    request_body(content = GatewayAnthropicMessagesRequest, description = "Anthropic Messages request whose `model` is an exact Anthropic model id; the Wyrd access token travels in `x-api-key` or an `Authorization` bearer"),
    responses(
        (status = 200, description = "Anthropic message as JSON, or Anthropic server-sent events when `stream` is true", content(
            (GatewayAnthropicMessage = "application/json"),
            (String = "text/event-stream")
        )),
        (status = 400, description = "Invalid request, ambiguous or URL-borne credentials, or a request no authorized Anthropic deployment can represent, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 401, description = "Authentication required, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 404, description = "No authorized Anthropic deployment serves the model, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 503, description = "The server is draining, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an Anthropic error envelope", body = AnthropicErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an Anthropic error envelope, or the refusing provider's status with its error body", body = AnthropicErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/messages`: one governed Anthropic Messages call.
///
/// The body's `model` names an exact model of provider `anthropic`, and only
/// built-in Anthropic deployments serve it, so fallback never leaves the
/// protocol. A body that does not match [`GatewayAnthropicMessagesRequest`]
/// is refused before dispatch; an accepted body, with every extension member,
/// reaches the provider as sent apart from its model and
/// the answer returns unchanged; `stream: true` relays Anthropic events and a
/// stream that ends without `message_stop` ends with an `error` event or an
/// aborted transport. Gateway-originated failures render as the Anthropic
/// error envelope.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn anthropic_messages(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    let answer = match caller {
        Err(WyrdErrorResponse(error)) => Err(error),
        Ok(caller) => {
            let call = typed_body::<GatewayAnthropicMessagesRequest>(request).and_then(|body| {
                let model = body
                    .get("model")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        invalid_request(GatewayContractError::new("model", "is required"))
                    })
                    .and_then(|model| native_model("anthropic", model))?;
                let output = body.get("max_tokens").and_then(Value::as_u64);
                Ok(GatewayCallRequest {
                    operation: GatewayOperation::ChatCompletions,
                    ingress: IngressDialect::AnthropicMessages,
                    model,
                    fallback: None,
                    stream: body.get("stream") == Some(&Value::Bool(true)),
                    usage_bound: output.and_then(|output| token_bound(&body, Some(output))),
                    body,
                    media: None,
                    batch: None,
                    deployment: None,
                    timeout: PUBLIC_CALL_TIMEOUT,
                })
            });
            match call {
                Ok(call) => GatewayInvocation::new(&state).invoke(&caller, call).await,
                Err(error) => Err(error),
            }
        }
    };
    match answer {
        Ok(answer) => relay(answer.status, answer.body),
        Err(error) => GatewayIngress::AnthropicMessages.error(&error),
    }
}

#[utoipa::path(
    post,
    path = "/v1beta/models/{target}",
    params(
        ("target" = String, Path, description = "`<model>:generateContent`, or `<model>:streamGenerateContent` with `alt=sse`, where `<model>` is an exact Gemini model id"),
        ("alt" = Option<String>, Query, description = "`sse` for streamGenerateContent; `json` or absent for generateContent")
    ),
    request_body(content = GatewayGeminiGenerateContentRequest, description = "Gemini GenerateContent request; the Wyrd access token travels in `x-goog-api-key` or an `Authorization` bearer"),
    responses(
        (status = 200, description = "Gemini GenerateContent response as JSON, or its server-sent events for streamGenerateContent", content(
            (GatewayGeminiGenerateContentResponse = "application/json"),
            (String = "text/event-stream")
        )),
        (status = 400, description = "Invalid request or method, ambiguous or URL-borne credentials, or a request no authorized Gemini deployment can represent, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 401, description = "Authentication required, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 404, description = "No authorized Gemini deployment serves the model, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 503, description = "The server is draining, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as a Google error envelope", body = GoogleErrorEnvelope),
        (status = "default", description = "Other gateway refusal as a Google error envelope, or the refusing provider's status with its error body", body = GoogleErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1beta/models/{model}:generateContent` and
/// `:streamGenerateContent`: one governed Gemini `GenerateContent` call.
///
/// The path model names an exact model of provider `gemini`, and only
/// built-in Gemini deployments serve it. A body that does not match
/// [`GatewayGeminiGenerateContentRequest`] is refused before dispatch; an
/// accepted body reaches the provider as sent and the answer returns unchanged; the streaming method requires
/// `alt=sse` and relays Gemini events, and a stream that ends before every
/// candidate finishes aborts its transport. Gateway-originated failures render
/// as the Google error envelope.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn gemini_generate_content(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    Path(target): Path<String>,
    RawQuery(query): RawQuery,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    let answer = match caller {
        Err(WyrdErrorResponse(error)) => Err(error),
        Ok(caller) => match gemini_call(&target, query.as_deref(), request) {
            Ok(call) => GatewayInvocation::new(&state).invoke(&caller, call).await,
            Err(error) => Err(error),
        },
    };
    match answer {
        Ok(answer) => relay(answer.status, answer.body),
        Err(error) => GatewayIngress::GeminiGenerateContent.error(&error),
    }
}

/// Builds the governed call of a Gemini `target` (`<model>:<method>`) with
/// its raw `query` and body.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `method` for a method other than
/// `generateContent` or `streamGenerateContent`, `alt` when it repeats or the
/// response format does not match the method, `model` for an invalid model id,
/// and `body` or the missing member for a body that is not a `GenerateContent`
/// request.
fn gemini_call(
    target: &str,
    query: Option<&str>,
    request: Result<Json<Value>, JsonRejection>,
) -> Result<GatewayCallRequest, WyrdError> {
    let (model, method) = target.split_once(':').unwrap_or((target, ""));
    let stream = match method {
        "generateContent" => false,
        "streamGenerateContent" => true,
        _ => {
            return Err(invalid_request(GatewayContractError::new(
                "method",
                "must be generateContent or streamGenerateContent",
            )));
        }
    };
    let mut alts = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .filter(|(name, _)| name == "alt")
        .map(|(_, value)| value);
    let alt = alts.next();
    if alts.next().is_some() {
        return Err(invalid_request(GatewayContractError::new(
            "alt",
            "must appear at most once",
        )));
    }
    let expected = if stream { "sse" } else { "json" };
    if alt.as_deref().unwrap_or("json") != expected {
        return Err(invalid_request(GatewayContractError::new(
            "alt",
            if stream {
                "must be sse for streamGenerateContent"
            } else {
                "must be json or absent for generateContent"
            },
        )));
    }
    let model = native_model("gemini", model)?;
    let body = typed_body::<GatewayGeminiGenerateContentRequest>(request)?;
    let config = body
        .get("generationConfig")
        .or_else(|| body.get("generation_config"));
    let field = |camel: &str, snake: &str| {
        config
            .and_then(|config| config.get(camel).or_else(|| config.get(snake)))
            .and_then(Value::as_u64)
    };
    let output = field("maxOutputTokens", "max_output_tokens")
        .and_then(|max| max.checked_mul(field("candidateCount", "candidate_count").unwrap_or(1)));
    Ok(GatewayCallRequest {
        operation: GatewayOperation::ChatCompletions,
        ingress: IngressDialect::GeminiGenerateContent,
        model,
        fallback: None,
        stream,
        usage_bound: output.and_then(|output| token_bound(&body, Some(output))),
        body,
        media: None,
        batch: None,
        deployment: None,
        timeout: PUBLIC_CALL_TIMEOUT,
    })
}
