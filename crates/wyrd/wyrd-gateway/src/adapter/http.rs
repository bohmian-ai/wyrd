//! Provider dispatch for the built-in and OpenAI-compatible adapters.
//!
//! Every provider exchange goes through a Skald provider client, which owns
//! provider URLs, authentication headers, the HTTP exchange, bounded reads,
//! and wire decoding. This module authenticates the client of a deployment,
//! screens tenant-supplied base URLs, and maps the outcome to attempt
//! evidence.

use async_trait::async_trait;
use reqwest::header::HeaderName;
use serde_json::value::{RawValue, to_raw_value};
use skald_providers::auth::{AnthropicAuth, GoogleApiKeyAuth, GoogleOAuth, OpenAiAuth, VertexAuth};
use skald_providers::{
    AnthropicClient, GoogleClient, HttpTransport, MediaAnswer, OpenAiClient, OpenAiMediaRoute,
    OpenAiRoute, ProviderByteStream, ProviderError, ProviderResult, RetryPolicy, VertexClient,
};
use skald_spec::wire::vertex_generate::VertexGenerateContentRequest;
use skald_spec::{ProviderRequest, ProviderResponse};
use url::Url;
use wyrd_spec::gateway::{GatewayOperation, ProviderAdapter, ProviderAuth, ProviderDeployment};

use super::stream::{ChatChunks, Frames, MediaCapture, StreamCapture, relay, relay_media};
use super::{
    BatchAction, MediaRequest, Prepared, Wire, complete, faithful_embeddings, prepare, refusal,
};
use crate::credential::ProviderSecret;
use crate::endpoint::EndpointPolicy;
use crate::engine::{
    AttemptResult, AttemptUsage, FailureClass, ProviderAttempt, ProviderDispatch, ResponseBody,
    ResponseCapture,
};

/// Base URL overrides of the built-in adapters.
///
/// `None` keeps the Skald client's provider endpoint, which production uses;
/// tests point the built-ins at local mock servers. OpenAI-compatible
/// deployments always use their own declared base URL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuiltinEndpoints {
    /// `OpenAI` API base, including its `/v1` segment.
    pub openai: Option<Url>,
    /// Anthropic API base.
    pub anthropic: Option<Url>,
    /// Gemini API base.
    pub gemini: Option<Url>,
    /// Vertex AI base; `None` selects the regional endpoint of each
    /// deployment's location.
    pub vertex: Option<Url>,
}

/// Provider dispatch through Skald provider clients under an endpoint policy.
///
/// Each attempt prepares the adapter request, authenticates a Skald client
/// with only the deployment's resolved provider credential, and maps the
/// provider answer back to the caller's dialect. Caller headers never reach
/// the provider. Skald retries are off because the engine owns retry and
/// fallback.
#[derive(Debug, Clone)]
pub struct HttpProviderDispatch {
    /// Policy-enforcing transport shared by every client.
    transport: HttpTransport,
    /// Policy screening tenant-supplied and overridden base URLs.
    policy: EndpointPolicy,
    /// Built-in adapter base URL overrides.
    endpoints: BuiltinEndpoints,
}

impl HttpProviderDispatch {
    /// Composes dispatch enforcing `policy`, with built-in `endpoints`.
    ///
    /// # Errors
    ///
    /// Returns the Skald transport error when the policy transport cannot be
    /// built.
    pub fn new(policy: EndpointPolicy, endpoints: BuiltinEndpoints) -> ProviderResult<Self> {
        Ok(Self {
            transport: policy.transport()?,
            policy,
            endpoints,
        })
    }

    /// Builds the Skald client of `deployment`, authenticated with
    /// `credential`.
    ///
    /// `OpenAI` takes bearer authentication; compatible endpoints take any
    /// style; Anthropic takes its `x-api-key` header; Gemini its
    /// `x-goog-api-key` header; Vertex a bearer access token. Returns `None`
    /// for any other combination, a base URL the policy does not admit, a
    /// missing credential, or a credential or header name that is not a valid
    /// header.
    fn client(
        &self,
        deployment: &ProviderDeployment,
        credential: Option<&ProviderSecret>,
    ) -> Option<Client> {
        let base = match &deployment.adapter {
            ProviderAdapter::OpenAi => self.endpoints.openai.clone(),
            ProviderAdapter::Anthropic => self.endpoints.anthropic.clone(),
            ProviderAdapter::Gemini => self.endpoints.gemini.clone(),
            ProviderAdapter::Vertex { .. } => self.endpoints.vertex.clone(),
            ProviderAdapter::OpenAiCompatible { base_url } => {
                Some(Url::parse(base_url.as_str()).ok()?)
            }
        };
        let base = match base {
            Some(url) if !self.policy.admits(&url) => return None,
            base => base.map(|url| url.as_str().trim_end_matches('/').to_owned()),
        };
        let key = || credential.map(ProviderSecret::expose);
        let model = deployment.model.model.as_str();
        let retry = RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        };
        let openai = |auth: OpenAiAuth| {
            let auth = match &base {
                Some(base) => auth.with_base_url(base.as_str()),
                None => auth,
            };
            Client::OpenAi(OpenAiClient::with_transport(
                auth,
                self.transport.clone(),
                retry.clone(),
            ))
        };
        let client = match (&deployment.adapter, &deployment.auth) {
            (
                ProviderAdapter::OpenAi | ProviderAdapter::OpenAiCompatible { .. },
                ProviderAuth::Bearer { .. },
            ) => openai(OpenAiAuth::new(key()?)),
            (ProviderAdapter::OpenAiCompatible { .. }, ProviderAuth::None) => {
                openai(OpenAiAuth::unauthenticated())
            }
            (
                ProviderAdapter::OpenAiCompatible { .. },
                ProviderAuth::ApiKeyHeader { header, .. },
            ) => {
                let name = HeaderName::from_bytes(header.as_str().as_bytes()).ok()?;
                openai(OpenAiAuth::with_key_header(name, key()?))
            }
            (ProviderAdapter::Anthropic, ProviderAuth::ApiKeyHeader { header, .. })
                if header.as_str() == "x-api-key" =>
            {
                let auth = AnthropicAuth::new(key()?);
                let auth = match base {
                    Some(base) => auth.with_base_url(base),
                    None => auth,
                };
                Client::Anthropic(AnthropicClient::with_transport(
                    auth,
                    self.transport.clone(),
                    retry,
                ))
            }
            (ProviderAdapter::Gemini, ProviderAuth::ApiKeyHeader { header, .. })
                if header.as_str() == "x-goog-api-key" =>
            {
                let auth = GoogleApiKeyAuth::new(key()?);
                let auth = match base {
                    Some(base) => auth.with_base_url(base),
                    None => auth,
                };
                Client::Google(GoogleClient::with_transport(
                    auth,
                    model,
                    self.transport.clone(),
                    retry,
                ))
            }
            (ProviderAdapter::Vertex { project, location }, ProviderAuth::Bearer { .. }) => {
                // ponytail: builds GoogleOAuth's own reqwest client per
                // attempt; share one when Vertex attempt latency matters.
                let oauth = GoogleOAuth::from_access_token(key()?).ok()?;
                let auth = VertexAuth::new(project.as_str(), location.as_str(), oauth);
                let auth = match base {
                    Some(base) => auth.with_base_url(base),
                    None => auth,
                };
                Client::Vertex(VertexClient::with_transport(
                    auth,
                    model,
                    self.transport.clone(),
                    retry,
                ))
            }
            _ => return None,
        };
        Some(client)
    }
}

/// Skald client of one deployment's adapter.
enum Client {
    /// `OpenAI` or an OpenAI-compatible endpoint.
    OpenAi(OpenAiClient),
    /// Anthropic Messages.
    Anthropic(AnthropicClient),
    /// Gemini `GenerateContent`.
    Google(GoogleClient),
    /// Vertex `GenerateContent`.
    Vertex(VertexClient),
}

impl Client {
    /// Opens a streamed answer to `prepared` for `operation`.
    ///
    /// Native bodies stream from the client's raw route unchanged; translated
    /// requests serialize their typed body, which preparation marked as
    /// streaming, to the same raw streaming route.
    ///
    /// # Errors
    ///
    /// Returns the Skald provider error of opening the exchange, including a
    /// non-success status, or [`ProviderError::VariantMismatch`] for a request
    /// this client cannot carry, which preparation rules out.
    async fn open(
        self,
        operation: GatewayOperation,
        prepared: Prepared,
    ) -> ProviderResult<ProviderByteStream> {
        let body = match &prepared {
            Prepared::Native(body) => to_raw_value(body),
            Prepared::Anthropic(request) => to_raw_value(request),
            Prepared::Google(request) => to_raw_value(request),
        }
        .map_err(|error| ProviderError::decode("gateway", error))?;
        match (self, prepared) {
            (Self::OpenAi(client), Prepared::Native(_)) => {
                client.stream_raw(route(operation)?, &body).await
            }
            (Self::Anthropic(client), Prepared::Native(_) | Prepared::Anthropic(_)) => {
                client.stream_raw(&body).await
            }
            (Self::Google(client), Prepared::Native(_) | Prepared::Google(_)) => {
                client.stream_raw(&body).await
            }
            (Self::Vertex(client), Prepared::Native(_) | Prepared::Google(_)) => {
                client.stream_raw(&body).await
            }
            (_, _) => Err(mismatch()),
        }
    }

    /// Sends a prepared Images or Audio request on its `OpenAI` media route.
    ///
    /// # Errors
    ///
    /// Returns the Skald provider error of the exchange, or
    /// [`ProviderError::VariantMismatch`] for a client other than `OpenAI` or a
    /// translated request, which preparation rules out.
    async fn media(self, media: &MediaRequest, prepared: Prepared) -> ProviderResult<MediaAnswer> {
        match (self, prepared) {
            (Self::OpenAi(client), Prepared::Native(body)) => {
                client.send_media(media.route, &body, &media.files).await
            }
            (_, _) => Err(mismatch()),
        }
    }

    /// Opens a prepared Images or Audio request on its `OpenAI` media route for
    /// an incrementally read answer.
    ///
    /// # Errors
    ///
    /// Returns the Skald provider error of opening the exchange, including a
    /// non-success status, or [`ProviderError::VariantMismatch`] for a client
    /// other than `OpenAI` or a translated request, which preparation rules
    /// out.
    async fn open_media(
        self,
        media: &MediaRequest,
        prepared: Prepared,
    ) -> ProviderResult<ProviderByteStream> {
        match (self, prepared) {
            (Self::OpenAi(client), Prepared::Native(body)) => {
                client.stream_media(media.route, &body, &media.files).await
            }
            (_, _) => Err(mismatch()),
        }
    }

    /// Sends a prepared Batches lifecycle `action`, uploading its input file
    /// with every line's model rewritten to `native`.
    ///
    /// # Errors
    ///
    /// Returns the Skald provider error of the exchange, or
    /// [`ProviderError::VariantMismatch`] for a client other than `OpenAI` or a
    /// translated request, which preparation rules out.
    async fn batch(
        self,
        action: &BatchAction,
        native: &str,
        prepared: Prepared,
    ) -> ProviderResult<MediaAnswer> {
        match (self, prepared) {
            (Self::OpenAi(client), Prepared::Native(body)) => {
                let (route, file) = action.route(native);
                client.send_batch(&route, &body, file.as_ref()).await
            }
            (_, _) => Err(mismatch()),
        }
    }

    /// Sends `prepared` for `operation`.
    ///
    /// Native bodies go to the client's raw route and come back unchanged as
    /// [`ProviderResponse::RawV1`]; translated requests go through the typed
    /// route and come back typed.
    ///
    /// # Errors
    ///
    /// Returns the Skald provider error of the exchange, or
    /// [`ProviderError::VariantMismatch`] for a request this client cannot
    /// carry, which preparation rules out.
    async fn send(
        self,
        operation: GatewayOperation,
        prepared: Prepared,
    ) -> ProviderResult<ProviderResponse> {
        let raw = |body: &serde_json::Value| {
            to_raw_value(body).map_err(|error| ProviderError::decode("gateway", error))
        };
        let response = match (self, prepared) {
            (Self::OpenAi(client), Prepared::Native(body)) => {
                ProviderResponse::RawV1(client.send_raw(route(operation)?, &raw(&body)?).await?)
            }
            (Self::Anthropic(client), Prepared::Native(body)) => {
                ProviderResponse::RawV1(client.send_raw(&raw(&body)?).await?)
            }
            (Self::Google(client), Prepared::Native(body)) => {
                ProviderResponse::RawV1(client.send_raw(&raw(&body)?).await?)
            }
            (Self::Vertex(client), Prepared::Native(body)) => {
                ProviderResponse::RawV1(client.send_raw(&raw(&body)?).await?)
            }
            (Self::Anthropic(client), Prepared::Anthropic(request)) => {
                client
                    .send_native(ProviderRequest::AnthropicMessage(*request))
                    .await?
            }
            (Self::Google(client), Prepared::Google(request)) => {
                client
                    .send_native(ProviderRequest::GeminiGenerateContent(*request))
                    .await?
            }
            (Self::Vertex(client), Prepared::Google(request)) => {
                client
                    .send_native(ProviderRequest::Vertex(VertexGenerateContentRequest(
                        *request,
                    )))
                    .await?
            }
            (_, _) => return Err(mismatch()),
        };
        Ok(response)
    }
}

/// `OpenAI` raw route of `operation`.
///
/// # Errors
///
/// Returns [`ProviderError::VariantMismatch`] for an operation without one.
fn route(operation: GatewayOperation) -> ProviderResult<OpenAiRoute> {
    match operation {
        GatewayOperation::ChatCompletions => Ok(OpenAiRoute::ChatCompletions),
        GatewayOperation::Responses => Ok(OpenAiRoute::Responses),
        GatewayOperation::Embeddings => Ok(OpenAiRoute::Embeddings),
        _ => Err(ProviderError::variant_mismatch(
            "openai",
            format!("{operation:?}"),
        )),
    }
}

/// Whether `content_type` names JSON, ignoring parameters and case.
fn json(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
}

/// Error of a translated request prepared for another adapter's client.
fn mismatch() -> ProviderError {
    ProviderError::variant_mismatch("gateway", "translated request for another adapter")
}

/// Retry class of a non-success provider status: timeouts, conflicts, rate
/// limits, and server errors may succeed elsewhere; other refusals would not.
const fn refusal_class(status: u16) -> FailureClass {
    match status {
        408 | 409 | 425 | 429 | 500..=599 => FailureClass::Upstream,
        _ => FailureClass::Rejected,
    }
}

/// Attempt evidence of a failed provider exchange.
///
/// A non-redirect status is a refusal relayed with its status and body, with
/// the attempt's resolved `credential` removed.
/// Failures before any connection never dispatched. Timeouts, broken
/// exchanges, redirects (never followed), and undecodable answers may have
/// reached the provider; an invalid credential header also lands there
/// because Skald reports it as a decode failure.
fn failure(
    error: ProviderError,
    translated: bool,
    credential: Option<&ProviderSecret>,
) -> AttemptResult {
    let class = match error {
        ProviderError::Status { status, body, .. } if !(300..400).contains(&status) => {
            return AttemptResult::Refused {
                class: refusal_class(status),
                status,
                body: refusal(translated, status, &body, credential),
                usage: AttemptUsage::default(),
            };
        }
        ProviderError::Connect { .. }
        | ProviderError::Auth { .. }
        | ProviderError::BadRequest { .. }
        | ProviderError::VariantMismatch { .. } => FailureClass::BeforeDispatch,
        ProviderError::Status { .. }
        | ProviderError::Timeout { .. }
        | ProviderError::Upstream { .. }
        | ProviderError::Decode { .. } => FailureClass::Upstream,
    };
    AttemptResult::Failed {
        class,
        usage: AttemptUsage::default(),
    }
}

/// Capture projection of a successful JSON answer `raw`, only when `attempt`
/// selected capture: the decoded answer with every exact occurrence of the
/// attempt's credential removed, so a provider echoing its credential never
/// reaches capture while the caller's bytes stay unchanged.
///
/// Returns `None` when capture is unselected, `raw` does not decode, or a
/// string decodes from base64 to the credential.
fn captured(attempt: &ProviderAttempt<'_>, raw: &RawValue) -> Option<ResponseCapture> {
    if !attempt.capture {
        return None;
    }
    ResponseCapture::json(
        serde_json::from_str(raw.get()).ok()?,
        attempt.credential.map(ProviderSecret::expose),
    )
}

/// Capture projection of a buffered raw media `answer`, only when `attempt`
/// selected capture: a copy of its bytes, or `None` when they hold the
/// attempt's exact credential, which binary content cannot be scrubbed of.
/// The caller's answer is never changed.
fn media_captured(attempt: &ProviderAttempt<'_>, answer: &MediaAnswer) -> Option<ResponseCapture> {
    if !attempt.capture {
        return None;
    }
    ResponseCapture::media(
        answer.clone(),
        attempt.credential.map(ProviderSecret::expose),
    )
}

/// Frame relay of a streamed `attempt`: `OpenAI` chat chunks with call `id`
/// and served `model` for a translated stream, or native passthrough.
fn frames(attempt: &ProviderAttempt<'_>, translated: bool, id: String, model: &str) -> Frames {
    if translated {
        let include_usage = attempt.body.pointer("/stream_options/include_usage")
            == Some(&serde_json::Value::Bool(true));
        Frames::Chat(ChatChunks::new(id, model, include_usage))
    } else {
        Frames::Native {
            responses: attempt.operation == GatewayOperation::Responses,
        }
    }
}

impl Client {
    /// Opens the streamed call of `attempt` and relays its `wire` events as
    /// `frames` within `limit` total bytes.
    ///
    /// Usage arrives on the stream itself. A failure to open maps like any
    /// call's, translated when `frames` translate chat chunks.
    async fn stream(
        self,
        attempt: &ProviderAttempt<'_>,
        prepared: Prepared,
        wire: Wire,
        frames: Frames,
        limit: usize,
    ) -> AttemptResult {
        let translated = matches!(frames, Frames::Chat(_));
        match self.open(attempt.operation, prepared).await {
            Ok(upstream) => AttemptResult::Completed {
                body: ResponseBody::Events(relay(
                    upstream,
                    wire,
                    frames,
                    limit,
                    attempt.deadline,
                    attempt.cancel.clone(),
                    attempt
                        .capture
                        .then(|| StreamCapture::new(attempt.credential)),
                )),
                usage: AttemptUsage::default(),
                capture: None,
            },
            Err(error) => {
                tracing::warn!(
                    deployment = attempt.deployment.name.as_str(),
                    code = error.code(),
                    "gateway provider stream failed to open"
                );
                failure(error, translated, attempt.credential)
            }
        }
    }

    /// Sends the speech request of `attempt` on the `media` route and relays
    /// its audio chunk by chunk within `limit` total bytes.
    ///
    /// When the attempt selected capture, the relay keeps the delivered bytes
    /// for a credential-free media capture reported at the stream's end.
    ///
    /// Speech answers carry no usage. A failure to open maps like any native
    /// call's, since media bodies are never translated.
    async fn speak(
        self,
        media: &MediaRequest,
        attempt: &ProviderAttempt<'_>,
        prepared: Prepared,
        limit: usize,
    ) -> AttemptResult {
        match self.open_media(media, prepared).await {
            Ok(upstream) => AttemptResult::Completed {
                body: ResponseBody::MediaStream {
                    content_type: upstream.content_type().to_owned(),
                    events: {
                        let capture = attempt.capture.then(|| {
                            MediaCapture::new(attempt.credential, upstream.content_type())
                        });
                        relay_media(
                            upstream,
                            limit,
                            attempt.deadline,
                            attempt.cancel.clone(),
                            capture,
                        )
                    },
                },
                usage: AttemptUsage::default(),
                capture: None,
            },
            Err(error) => {
                tracing::warn!(
                    deployment = attempt.deployment.name.as_str(),
                    code = error.code(),
                    "gateway provider speech failed to open"
                );
                failure(error, false, attempt.credential)
            }
        }
    }

    /// Sends the Batches lifecycle `action` of `attempt` over this client.
    ///
    /// Lifecycle answers carry no usage: a JSON object returns unchanged as
    /// JSON, with its capture projection when selected, and file content keeps
    /// its content type. A failure maps like any
    /// native call's, since Batches bodies are never translated.
    async fn lifecycle(
        self,
        action: &BatchAction,
        attempt: &ProviderAttempt<'_>,
        prepared: Prepared,
    ) -> AttemptResult {
        let deployment = attempt.deployment;
        match self
            .batch(action, deployment.model.model.as_str(), prepared)
            .await
        {
            Ok(answer) => match serde_json::from_slice::<Box<RawValue>>(&answer.bytes) {
                Ok(raw) if json(&answer.content_type) => AttemptResult::Completed {
                    capture: captured(attempt, &raw),
                    body: ResponseBody::Json(raw),
                    usage: AttemptUsage::default(),
                },
                _ => AttemptResult::Completed {
                    capture: media_captured(attempt, &answer),
                    body: ResponseBody::Media(answer),
                    usage: AttemptUsage::default(),
                },
            },
            Err(error) => {
                tracing::warn!(
                    deployment = deployment.name.as_str(),
                    code = error.code(),
                    "gateway batch lifecycle call failed"
                );
                failure(error, false, attempt.credential)
            }
        }
    }
}

#[async_trait]
impl ProviderDispatch for HttpProviderDispatch {
    async fn dispatch(&self, attempt: ProviderAttempt<'_>) -> AttemptResult {
        let deployment = attempt.deployment;
        let failed = |class| AttemptResult::Failed {
            class,
            usage: AttemptUsage::default(),
        };
        let prepared = match tracing::info_span!("gateway.translate", direction = "request")
            .in_scope(|| {
                prepare(
                    attempt.ingress,
                    attempt.operation,
                    attempt.stream,
                    attempt.body,
                    deployment,
                    attempt.media,
                )
            }) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::warn!(deployment = deployment.name.as_str(), %error, "gateway request not representable");
                return failed(FailureClass::Rejected);
            }
        };
        let Some(client) = self.client(deployment, attempt.credential) else {
            tracing::warn!(
                deployment = deployment.name.as_str(),
                "gateway deployment endpoint, authentication, or credential is not usable"
            );
            return failed(FailureClass::BeforeDispatch);
        };
        let translated = !matches!(prepared, Prepared::Native(_));
        let wire = Wire::of(&deployment.adapter);
        let id = format!("chatcmpl-{}", attempt.call_id.as_uuid().simple());
        let model = deployment.model.model.as_str();
        if attempt.stream {
            let frames = frames(&attempt, translated, id, model);
            let limit = self.transport.config().max_response_bytes;
            return client.stream(&attempt, prepared, wire, frames, limit).await;
        }
        if let Some(action) = attempt.batch {
            return client.lifecycle(action, &attempt, prepared).await;
        }
        if let Some(media) = attempt
            .media
            .filter(|media| media.route == OpenAiMediaRoute::AudioSpeech)
        {
            let limit = self.transport.config().max_response_bytes;
            return client.speak(media, &attempt, prepared, limit).await;
        }
        let response = match attempt.media {
            // A JSON media answer maps like any raw answer; other media
            // content returns unchanged with its content type.
            Some(media) => match client.media(media, prepared).await {
                Ok(answer) if json(&answer.content_type) => {
                    serde_json::from_slice::<Box<RawValue>>(&answer.bytes)
                        .map(ProviderResponse::RawV1)
                        .map_err(|error| ProviderError::decode("gateway", error))
                }
                Ok(answer) => {
                    return AttemptResult::Completed {
                        capture: media_captured(&attempt, &answer),
                        body: ResponseBody::Media(answer),
                        usage: AttemptUsage::default(),
                    };
                }
                Err(error) => Err(error),
            },
            None => client.send(attempt.operation, prepared).await,
        };
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                // Only the stable code is logged: Skald error text can carry
                // provider bodies and transport detail.
                tracing::warn!(
                    deployment = deployment.name.as_str(),
                    code = error.code(),
                    "gateway provider call failed"
                );
                return failure(error, translated, attempt.credential);
            }
        };
        let mapped = tracing::info_span!("gateway.translate", direction = "response")
            .in_scope(|| complete(response, wire, &id, model));
        match mapped {
            Ok((body, usage))
                if attempt.operation != GatewayOperation::Embeddings
                    || faithful_embeddings(attempt.body, &body) =>
            {
                AttemptResult::Completed {
                    capture: captured(&attempt, &body),
                    body: ResponseBody::Json(body),
                    usage,
                }
            }
            // An answer that cannot be mapped, or an embeddings answer that
            // ignores its request, fails rather than returning as success.
            Ok((_, usage)) | Err(usage) => AttemptResult::Failed {
                class: FailureClass::Rejected,
                usage,
            },
        }
    }
}
