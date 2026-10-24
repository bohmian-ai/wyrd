//! Public gateway inference ingress.
//!
//! Every inference route authenticates through [`GatewayIngress`], which
//! reads the Wyrd access token, or a Wyrd API key it exchanges for one, from
//! the headers its protocol's SDKs send and renders refusals in that
//! protocol's error envelope. The `OpenAI`-compatible
//! handlers live in [`super::routes`]; the Anthropic Messages and Gemini
//! `GenerateContent` handlers here take each provider's native body and
//! answer with its native response or stream, and the Jev `systemone` handler
//! takes Jev's question body and answers with Jev's answer, authenticating and
//! refusing like the `OpenAI`-compatible routes. All of them run through the one
//! governed [`GatewayInvocation`], which never forwards caller headers to a
//! provider.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, RawQuery, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use skald_spec::ProviderName;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_gateway::IngressDialect;
use wyrd_spec::auth::DEPLOYMENT_ENVIRONMENT_HEADER;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::native::{
    AnthropicErrorEnvelope, GatewayAnthropicMessage, GatewayAnthropicMessagesRequest,
    GatewayGeminiGenerateContentRequest, GatewayGeminiGenerateContentResponse, GatewayJevRequest,
    GatewayJevResponse, GoogleErrorEnvelope,
};
use wyrd_spec::gateway::openai::OpenAiErrorEnvelope;
use wyrd_spec::gateway::{
    CARD_UID_HEADER, FALLBACK_HEADER, GatewayContractError, GatewayFallbackOverride,
    GatewayOperation, GatewayUsageAmount, MAX_RUN_HEADER_BYTES, ModelRef, PROMPT_PROVIDER_HEADER,
    RUN_HEADER,
};
use wyrd_spec::ids::CardUid;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::ids::RunId;

use super::invocation::{
    GatewayCallAttribution, GatewayCallContext, GatewayCallRequest, GatewayInvocation,
    invalid_request,
};
use super::routes::{self, PUBLIC_CALL_TIMEOUT, openai_error, relay, token_bound, typed_body};
use super::telemetry::IngressClock;
use crate::components::auth::Caller;
use crate::components::auth::token_extract::{
    ANTHROPIC_API_KEY_HEADER, GOOGLE_API_KEY_HEADER, extract_gateway_access_token,
    verify_access_token, workload_uid_header,
};
use crate::http::error::WyrdErrorResponse;
use crate::http::middleware::metrics::RequestArrival;
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
    /// The single accepted credential header is classified by
    /// [`GatewayCredential::classify`]: a Wyrd API key is exchanged through
    /// [`super::GatewayApiKeys`], anything else is verified as a Wyrd access token,
    /// either under the optional `x-wyrd-workload-uid` root, and the principal
    /// is inserted for the handler's [`Caller`]. A missing,
    /// ambiguous, URL-borne, or invalid credential, or a key the exchange
    /// refuses, is refused in the protocol's envelope without reading the
    /// body; a refused key never falls back to token verification. An
    /// authenticated request also carries the [`IngressClock`] its handler
    /// uses to time the call from the request's arrival.
    async fn authenticate(self, state: AppState, mut request: Request, next: Next) -> Response {
        let authenticating = std::time::Instant::now();
        let native = self.native_header();
        let credential = extract_gateway_access_token(
            request.headers(),
            native.as_ref(),
            request.uri().query(),
            self.query_names(&request),
        )
        .map(GatewayCredential::classify);
        let verified = match (credential, workload_uid_header(request.headers())) {
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(GatewayCredential::AccessToken(token)), Ok(root)) => {
                match verify_access_token(
                    state.auth.token_verifier.as_deref(),
                    token,
                    wyrd_auth_verify::TokenAudience::Wyrd,
                ) {
                    Ok(principal) => principal.select_root(&state, root.as_ref()).await,
                    Err(WyrdErrorResponse(error)) => Err(error),
                }
            }
            (Ok(GatewayCredential::ApiKey(api_key)), Ok(root)) => {
                let request_id = request
                    .extensions()
                    .get::<RequestId>()
                    .cloned()
                    .unwrap_or_else(RequestId::now_v7);
                state
                    .gateway
                    .api_keys()
                    .authenticate(&state, &api_key, root, request_id.as_str())
                    .await
            }
        };
        match verified {
            Ok(principal) => {
                let arrival = request.extensions().get::<RequestArrival>().copied();
                let extensions = request.extensions_mut();
                extensions.insert(principal);
                extensions.insert(IngressClock::authenticated(arrival, authenticating));
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

/// Prefix every Wyrd API key starts with.
const API_KEY_PREFIX: &str = "wyrd_sk_";

/// The one credential a public gateway ingress request presented.
#[derive(Debug)]
enum GatewayCredential {
    /// A Wyrd API key, exchanged for an access token of its own principal.
    ApiKey(SecretString),
    /// A Wyrd access token, verified as presented.
    AccessToken(SecretString),
}

impl GatewayCredential {
    /// Classifies an extracted credential by its value alone.
    ///
    /// Every value starting with `wyrd_sk_`, well-formed or not, is an API
    /// key, so a malformed key earns the API-key error; every other value is
    /// an access token, as before API keys were accepted.
    fn classify(credential: SecretString) -> Self {
        if credential.expose_secret().starts_with(API_KEY_PREFIX) {
            Self::ApiKey(credential)
        } else {
            Self::AccessToken(credential)
        }
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
        .routes(routes!(routes::cancel_batch))
        .routes(routes!(jev_systemone));
    let anthropic = OpenApiRouter::new().routes(routes!(anthropic_messages));
    let gemini = OpenApiRouter::new()
        .routes(routes!(gemini_generate_content))
        .route(
            "/v1beta/models/{target}",
            axum::routing::post(gemini_generate_content_without_deployment),
        );
    GatewayIngress::OpenAi
        .layer(openai, state)
        .merge(GatewayIngress::AnthropicMessages.layer(anthropic, state))
        .merge(GatewayIngress::GeminiGenerateContent.layer(gemini, state))
}

/// Parses the `<deployment>/<model>` address a native route carries.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `model` when `model` is not a valid
/// address.
fn native_model(model: &str) -> Result<ModelRef, WyrdError> {
    ModelRef::from_projection(model).map_err(invalid_request)
}

/// OpenAPI description of the optional `wyrd-gateway-fallback` request header
/// on every governed inference route that accepts it.
pub(super) const FALLBACK_HEADER_DOC: &str = "Optional per-call fallback override: unpadded \
    base64url over the JCS UTF-8 serialization of a GatewayFallbackOverride, at most 8 KiB \
    encoded and 4 KiB decoded, whose candidates are non-empty, duplicate-free, and exclude the \
    requested model. Absent means tenant fallback policy applies. A repeated, malformed, \
    oversized, or invalid value is refused before dispatch with \
    WYRD_GATEWAY_400_INVALID_REQUEST naming `fallback`. The header never reaches a provider.";

/// Reads the optional `wyrd-gateway-fallback` override of a call to `model`.
///
/// The header is consumed here and never reaches a provider; its absence
/// leaves tenant fallback policy in effect.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `fallback` when the header repeats
/// or its value is not a valid override for `model`, as decided by
/// [`GatewayFallbackOverride::from_header_value`].
pub(super) fn requested_fallback(
    headers: &HeaderMap,
    model: &ModelRef,
) -> Result<Option<GatewayFallbackOverride>, WyrdError> {
    let mut values = headers.get_all(FALLBACK_HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(invalid_request(GatewayContractError::new(
            "fallback",
            "must appear at most once",
        )));
    }
    GatewayFallbackOverride::from_header_value(value.as_bytes(), model)
        .map(Some)
        .map_err(invalid_request)
}

/// OpenAPI description of the optional `wyrd-run-id` request header.
pub(super) const RUN_HEADER_DOC: &str = "Optional application Run the call belongs to: 1 to 128 \
    bytes of visible ASCII, independent of `wyrd-card-uid`. A repeated, empty, or oversized \
    value is refused before dispatch with WYRD_GATEWAY_400_INVALID_REQUEST naming the header. \
    The header never reaches a provider.";

/// OpenAPI description of the optional `wyrd-card-uid` request header.
pub(super) const CARD_UID_HEADER_DOC: &str = "Optional Card the call is attributed to, as its \
    Card UID, independent of `wyrd-run-id`. A repeated or malformed value is refused with \
    WYRD_GATEWAY_400_INVALID_REQUEST; a UID outside the caller's signed Card scope, or, for a \
    caller bound to no Card, one naming no registered observation-target Card of the tenant, \
    with WYRD_VALA_403_BIFROST_CARD_SCOPE. Every refusal precedes dispatch. The header never \
    reaches a provider.";

/// OpenAPI description of the optional `wyrd-prompt-provider` request header.
pub(super) const PROMPT_PROVIDER_HEADER_DOC: &str = "Optional Prompt provider of an Agent \
    call: `openai`, `anthropic`, `google`, `vertex`, `jev`, or a custom provider name. The \
    requested deployment's adapter must be compatible with it (`openai` and custom: `openai` or \
    `openai_compatible`; `anthropic`: `anthropic`; `google`: `gemini`; `vertex`: `vertex`; \
    `jev`: `jev`), or the call is refused before dispatch with \
    WYRD_GATEWAY_400_INVALID_REQUEST naming `model`; fallback candidates keep ordinary \
    conversion. A repeated or invalid value is refused naming the header. Absent means a direct \
    client call. The header never reaches a provider.";

/// Reads the optional Prompt provider of an Agent call from its
/// `wyrd-prompt-provider` header.
///
/// The header is consumed here and never reaches a provider; its absence
/// marks a direct client call that keeps ordinary protocol translation.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming the header when it repeats or its
/// value is not a provider name.
pub(super) fn requested_prompt_provider(
    headers: &HeaderMap,
) -> Result<Option<ProviderName>, WyrdError> {
    let invalid =
        |reason| invalid_request(GatewayContractError::new(PROMPT_PROVIDER_HEADER, reason));
    let mut values = headers.get_all(PROMPT_PROVIDER_HEADER).iter();
    let value = values.next();
    if values.next().is_some() {
        return Err(invalid("must appear at most once"));
    }
    value
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|name| name.parse().ok())
                .ok_or_else(|| invalid("must be a Prompt provider name"))
        })
        .transpose()
}

/// Reads the optional Run, Card, and deployment environment of a call from
/// its independent `wyrd-run-id`, `wyrd-card-uid`, and
/// `x-wyrd-deployment-environment` headers.
///
/// Every header is consumed here and never reaches a provider; each may
/// appear alone, and their joint absence leaves the request context empty.
/// The Card is only parsed here, as a [`GatewayCallAttribution::Requested`]
/// context admission authorizes against the caller and completes with the
/// caller's verified workload root. The environment is descriptive and grants
/// nothing.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming the offending header when any
/// repeats, the Run or Card is not visible ASCII, the Run is empty or longer
/// than [`MAX_RUN_HEADER_BYTES`], the Card is not a Card UID, or the
/// environment is empty or not UTF-8.
pub(super) fn requested_context(
    headers: &HeaderMap,
) -> Result<Option<GatewayCallAttribution>, WyrdError> {
    let invalid = |field, reason| invalid_request(GatewayContractError::new(field, reason));
    let single = |name: &'static str| {
        let mut values = headers.get_all(name).iter();
        let value = values.next();
        if values.next().is_some() {
            return Err(invalid(name, "must appear at most once"));
        }
        value
            .map(|value| {
                value
                    .to_str()
                    .map_err(|_| invalid(name, "must be visible ASCII"))
            })
            .transpose()
    };
    let run_id = single(RUN_HEADER)?
        .map(|run| {
            if run.is_empty() || run.len() > MAX_RUN_HEADER_BYTES {
                return Err(invalid(RUN_HEADER, "must be 1 to 128 bytes"));
            }
            Ok(RunId::from_string(run.to_owned()))
        })
        .transpose()?;
    let card_uid = single(CARD_UID_HEADER)?
        .map(|card| {
            card.parse::<CardUid>()
                .map_err(|_| invalid(CARD_UID_HEADER, "must be a Card UID"))
        })
        .transpose()?;
    let deployment_environment = wyrd_spec::auth::deployment_environment_selector(
        headers
            .get_all(DEPLOYMENT_ENVIRONMENT_HEADER)
            .iter()
            .map(HeaderValue::as_bytes),
    )
    .map_err(|_| {
        invalid(
            DEPLOYMENT_ENVIRONMENT_HEADER,
            "must appear at most once as one non-empty UTF-8 value",
        )
    })?;
    Ok(
        (run_id.is_some() || card_uid.is_some() || deployment_environment.is_some()).then_some(
            GatewayCallAttribution::Requested(GatewayCallContext {
                workload_card_uid: None,
                subject_card_uid: card_uid,
                run_id,
                deployment_environment,
            }),
        ),
    )
}

#[utoipa::path(
    post,
    path = "/v1/messages",
    params(
        ("wyrd-gateway-fallback" = Option<String>, Header, description = FALLBACK_HEADER_DOC),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC),
        ("wyrd-prompt-provider" = Option<String>, Header, description = PROMPT_PROVIDER_HEADER_DOC)
    ),
    request_body(content = GatewayAnthropicMessagesRequest, description = "Anthropic Messages request whose `model` is an exact Anthropic model id; the Wyrd access token, or a `wyrd_sk_` Wyrd API key the gateway exchanges for one, travels in `x-api-key` or an `Authorization` bearer"),
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
/// The body's `model` is a `<deployment>/<model>` address, and only
/// Anthropic-adapter deployments serve it, so fallback never leaves the
/// protocol. A body that does not match [`GatewayAnthropicMessagesRequest`]
/// is refused before dispatch; an accepted body, with every extension member,
/// reaches the provider as sent apart from its model and
/// the answer returns unchanged; `stream: true` relays Anthropic events and a
/// stream that ends without `message_stop` ends with an `error` event or an
/// aborted transport. Gateway-originated failures render as the Anthropic
/// error envelope.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn anthropic_messages(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
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
                    .and_then(native_model)?;
                Ok(GatewayCallRequest {
                    operation: GatewayOperation::ChatCompletions,
                    ingress: IngressDialect::AnthropicMessages,
                    fallback: requested_fallback(&headers, &model)?,
                    model,
                    stream: body.get("stream") == Some(&Value::Bool(true)),
                    usage_bound: anthropic_usage_bound(&body),
                    body,
                    media: None,
                    batch: None,
                    deployment: None,
                    timeout: PUBLIC_CALL_TIMEOUT,
                    subject: requested_context(&headers)?,
                    prompt_provider: requested_prompt_provider(&headers)?,
                })
            });
            match call {
                Ok(call) => {
                    GatewayInvocation::new(&state)
                        .parsed(clock.map(|Extension(clock)| clock), call.operation)
                        .invoke(&caller, call)
                        .await
                }
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
    path = "/v1/systemone",
    params(
        ("wyrd-gateway-fallback" = Option<String>, Header, description = FALLBACK_HEADER_DOC),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC),
        ("wyrd-prompt-provider" = Option<String>, Header, description = PROMPT_PROVIDER_HEADER_DOC)
    ),
    request_body(content = GatewayJevRequest, description = "Jev systemone request whose `model` is an exact Jev model id, such as `jev-latest`, with keyed `noul`, `choice`, or `score` questions about `state`"),
    responses(
        (status = 200, description = "Jev answer: the resolved model, intact keyed answers, and usage", body = GatewayJevResponse),
        (status = 400, description = "Invalid request or a broken question bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized Jev deployment serves the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded: Jev answers carry no output bound, so a token limit or budget refuses them, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/systemone`: one governed Jev question call.
///
/// The body's `model` is a `<deployment>/<model>` address, and only
/// deployments serving `system_one` serve it, so fallback never leaves the
/// protocol. A body that does not match [`GatewayJevRequest`], or breaks a
/// documented question bound, is refused before dispatch; an accepted body
/// reaches Jev with only its model rewritten and the answer returns
/// unchanged. Jev never streams. Gateway-originated failures render as the
/// `OpenAI` error envelope with a stable Wyrd code; a provider refusal relays
/// its status and body through the shared refusal path, credential removed.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn jev_systemone(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    let answer = match caller {
        Err(WyrdErrorResponse(error)) => Err(error),
        Ok(caller) => match jev_call(&headers, request) {
            Ok(call) => {
                GatewayInvocation::new(&state)
                    .parsed(clock.map(|Extension(clock)| clock), call.operation)
                    .invoke(&caller, call)
                    .await
            }
            Err(error) => Err(error),
        },
    };
    match answer {
        Ok(answer) => relay(answer.status, answer.body),
        Err(error) => GatewayIngress::OpenAi.error(&error),
    }
}

/// Builds the governed call of a Jev `systemone` body.
///
/// The call has no usage bound: no Jev request member bounds its answer.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `body` or the missing member for a
/// body that is not a Jev request, `model` for an invalid model id, and every
/// error of [`requested_fallback`], [`requested_context`], and [`requested_prompt_provider`].
fn jev_call(
    headers: &HeaderMap,
    request: Result<Json<Value>, JsonRejection>,
) -> Result<GatewayCallRequest, WyrdError> {
    let body = typed_body::<GatewayJevRequest>(request)?;
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_request(GatewayContractError::new("model", "is required")))
        .and_then(native_model)?;
    Ok(GatewayCallRequest {
        operation: GatewayOperation::SystemOne,
        ingress: IngressDialect::Jev,
        fallback: requested_fallback(headers, &model)?,
        model,
        stream: false,
        usage_bound: None,
        body,
        media: None,
        batch: None,
        deployment: None,
        timeout: PUBLIC_CALL_TIMEOUT,
        subject: requested_context(headers)?,
        prompt_provider: requested_prompt_provider(headers)?,
    })
}

#[utoipa::path(
    post,
    path = "/v1beta/models/{deployment}/{target}",
    params(
        ("deployment" = String, Path, description = "Named gateway deployment that uses a Google adapter"),
        ("target" = String, Path, description = "`<model>:generateContent`, or `<model>:streamGenerateContent` with `alt=sse`"),
        ("alt" = Option<String>, Query, description = "`sse` for streamGenerateContent; `json` or absent for generateContent"),
        ("wyrd-gateway-fallback" = Option<String>, Header, description = FALLBACK_HEADER_DOC),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC),
        ("wyrd-prompt-provider" = Option<String>, Header, description = PROMPT_PROVIDER_HEADER_DOC)
    ),
    request_body(content = GatewayGeminiGenerateContentRequest, description = "Gemini GenerateContent request; the Wyrd access token, or a `wyrd_sk_` Wyrd API key the gateway exchanges for one, travels in `x-goog-api-key` or an `Authorization` bearer"),
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
/// `POST /v1beta/models/{deployment}/{model}:generateContent` and
/// `:streamGenerateContent`: one governed Gemini `GenerateContent` call.
///
/// The path names a `<deployment>/<model>` address whose deployment uses
/// a Google adapter. A body that does not match
/// [`GatewayGeminiGenerateContentRequest`] is refused before dispatch; an
/// accepted body reaches the provider as sent and the answer returns unchanged; the streaming method requires
/// `alt=sse` and relays Gemini events, and a stream that ends before every
/// candidate finishes aborts its transport. Gateway-originated failures render
/// as the Google error envelope.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn gemini_generate_content(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    Path((deployment, target)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    let answer = match caller {
        Err(WyrdErrorResponse(error)) => Err(error),
        Ok(caller) => match gemini_call(
            &format!("{deployment}/{target}"),
            query.as_deref(),
            &headers,
            request,
        ) {
            Ok(call) => {
                GatewayInvocation::new(&state)
                    .parsed(clock.map(|Extension(clock)| clock), call.operation)
                    .invoke(&caller, call)
                    .await
            }
            Err(error) => Err(error),
        },
    };
    match answer {
        Ok(answer) => relay(answer.status, answer.body),
        Err(error) => GatewayIngress::GeminiGenerateContent.error(&error),
    }
}

/// `POST /v1beta/models/{model}:<method>`: a Gemini call that names no
/// deployment, refused as an invalid `model`.
///
/// Google SDKs address `models/<model>`, but the gateway serves
/// `<deployment>/<model>`. Routing the bare form under the Gemini ingress
/// keeps its refusal in the Gemini protocol: the credential is read from
/// `x-goog-api-key` and the error renders as a Google envelope, rather than
/// the request falling through to Wyrd API authentication and a misleading
/// missing-token 401. It is not documented in OpenAPI because it serves no
/// call.
async fn gemini_generate_content_without_deployment() -> Response {
    GatewayIngress::GeminiGenerateContent.error(&invalid_request(GatewayContractError::new(
        "model",
        "must be <deployment>/<model>; address the call as /v1beta/models/<deployment>/<model>:<method>",
    )))
}

/// Builds the governed call of a Gemini `target` (`<deployment>/<model>:<method>`) with
/// its raw `query`, request `headers`, and body.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `method` for a method other than
/// `generateContent` or `streamGenerateContent`, `alt` when it repeats or the
/// response format does not match the method, `model` for an invalid model id,
/// `body` or the missing member for a body that is not a `GenerateContent`
/// request, and every error of [`requested_fallback`], [`requested_context`],
/// and [`requested_prompt_provider`].
fn gemini_call(
    target: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    request: Result<Json<Value>, JsonRejection>,
) -> Result<GatewayCallRequest, WyrdError> {
    let (model, method) = target.rsplit_once(':').unwrap_or((target, ""));
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
    let model = native_model(model)?;
    let body = typed_body::<GatewayGeminiGenerateContentRequest>(request)?;
    Ok(GatewayCallRequest {
        operation: GatewayOperation::ChatCompletions,
        ingress: IngressDialect::GeminiGenerateContent,
        fallback: requested_fallback(headers, &model)?,
        model,
        stream,
        usage_bound: google_usage_bound(&body),
        body,
        media: None,
        batch: None,
        deployment: None,
        timeout: PUBLIC_CALL_TIMEOUT,
        subject: requested_context(headers)?,
        prompt_provider: requested_prompt_provider(headers)?,
    })
}

/// Token usage bound of an Anthropic Messages `body`, whose output is bounded
/// by its required `max_tokens`, or `None` when it is unbounded.
pub(super) fn anthropic_usage_bound(body: &Value) -> Option<Vec<GatewayUsageAmount>> {
    let output = body.get("max_tokens").and_then(Value::as_u64)?;
    token_bound(body, Some(output))
}

/// Token usage bound of a Google `GenerateContent` `body`, or `None` when it
/// is unbounded.
///
/// Output is bounded by `maxOutputTokens` times `candidateCount` (default
/// one) of its generation config, in either camel or snake case; a body
/// without that maximum is unbounded.
pub(super) fn google_usage_bound(body: &Value) -> Option<Vec<GatewayUsageAmount>> {
    let config = body
        .get("generationConfig")
        .or_else(|| body.get("generation_config"));
    let field = |camel: &str, snake: &str| {
        config
            .and_then(|config| config.get(camel).or_else(|| config.get(snake)))
            .and_then(Value::as_u64)
    };
    let output = field("maxOutputTokens", "max_output_tokens")?
        .checked_mul(field("candidateCount", "candidate_count").unwrap_or(1))?;
    token_bound(body, Some(output))
}

#[cfg(test)]
mod tests {
    //! Pure decoding of the paired Run and Card attribution headers.

    use super::*;

    /// Headers holding each `(name, value)` pair in order.
    ///
    /// # Panics
    /// Panics when a value is not a valid header value.
    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, value.parse().expect("header value"));
        }
        headers
    }

    /// Every `wyrd_sk_` value, malformed or not, selects the API-key
    /// exchange, and every other value, including one merely containing the
    /// prefix, stays an access token.
    ///
    /// # Panics
    /// Panics when a credential is classified differently than documented.
    #[test]
    fn gateway_credentials_select_key_exchange_or_token_verification() {
        for value in [
            "wyrd_sk_01890f287c4a7cc398e74f4a3c2d1b00_1a2b3c4d_secret",
            "wyrd_sk_",
            "wyrd_sk_not-a-key",
        ] {
            let credential = GatewayCredential::classify(SecretString::from(value));
            assert!(
                matches!(&credential, GatewayCredential::ApiKey(key) if key.expose_secret() == value),
                "{value}"
            );
        }
        for value in [
            "eyJhbGciOiJFZERTQSJ9.e30.sig",
            "jwt",
            "Wyrd_sk_x",
            "x_wyrd_sk_y",
        ] {
            let credential = GatewayCredential::classify(SecretString::from(value));
            assert!(
                matches!(&credential, GatewayCredential::AccessToken(token) if token.expose_secret() == value),
                "{value}"
            );
        }
    }

    /// Absent headers leave the request context empty, each header decodes
    /// alone or with the others into its Run, Card, and environment, and every
    /// repeated, empty, oversized, or malformed value is refused naming the
    /// offending header. A workload is never read from these headers.
    ///
    /// # Panics
    /// Panics when a header set decodes differently than documented.
    #[test]
    fn context_headers_decode_independently() {
        let card = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00";
        assert!(
            requested_context(&HeaderMap::new())
                .expect("absent")
                .is_none()
        );
        for (pairs, run, uid, environment) in [
            (
                vec![
                    (RUN_HEADER, "run-1"),
                    (CARD_UID_HEADER, card),
                    (DEPLOYMENT_ENVIRONMENT_HEADER, "production"),
                ],
                Some("run-1"),
                Some(card),
                Some("production"),
            ),
            (vec![(RUN_HEADER, "run-1")], Some("run-1"), None, None),
            (vec![(CARD_UID_HEADER, card)], None, Some(card), None),
            (
                vec![(DEPLOYMENT_ENVIRONMENT_HEADER, "staging")],
                None,
                None,
                Some("staging"),
            ),
        ] {
            let Some(GatewayCallAttribution::Requested(context)) =
                requested_context(&headers(&pairs)).expect("a valid header set")
            else {
                panic!("{pairs:?} is a requested context");
            };
            assert_eq!(context.run_id.as_ref().map(RunId::as_str), run, "{pairs:?}");
            assert_eq!(
                context.subject_card_uid.as_ref().map(CardUid::as_str),
                uid,
                "{pairs:?}"
            );
            assert_eq!(
                context.deployment_environment.as_deref(),
                environment,
                "{pairs:?}"
            );
            assert!(context.workload_card_uid.is_none(), "{pairs:?}");
        }
        let long = "r".repeat(MAX_RUN_HEADER_BYTES + 1);
        for (pairs, field) in [
            (vec![(RUN_HEADER, "")], RUN_HEADER),
            (vec![(CARD_UID_HEADER, "not-a-uid")], CARD_UID_HEADER),
            (vec![(RUN_HEADER, ""), (CARD_UID_HEADER, card)], RUN_HEADER),
            (
                vec![(RUN_HEADER, long.as_str()), (CARD_UID_HEADER, card)],
                RUN_HEADER,
            ),
            (
                vec![
                    (RUN_HEADER, "run-1"),
                    (CARD_UID_HEADER, "Agent/support-agent@1.0.0"),
                ],
                CARD_UID_HEADER,
            ),
            (
                vec![
                    (RUN_HEADER, "a"),
                    (RUN_HEADER, "b"),
                    (CARD_UID_HEADER, card),
                ],
                RUN_HEADER,
            ),
            (
                vec![(DEPLOYMENT_ENVIRONMENT_HEADER, "")],
                DEPLOYMENT_ENVIRONMENT_HEADER,
            ),
            (
                vec![
                    (DEPLOYMENT_ENVIRONMENT_HEADER, "production"),
                    (DEPLOYMENT_ENVIRONMENT_HEADER, "staging"),
                ],
                DEPLOYMENT_ENVIRONMENT_HEADER,
            ),
        ] {
            let error = requested_context(&headers(&pairs)).expect_err("refused");
            assert_eq!(error.code(), "WYRD_GATEWAY_400_INVALID_REQUEST");
            assert_eq!(
                error.as_problem_json()["details"]["field"],
                field,
                "{pairs:?}"
            );
        }
    }
}
