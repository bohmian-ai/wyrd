//! OpenAI Chat, Responses, Embeddings, Images, Audio, Files, and Batches client.

use std::fmt::{self, Debug, Formatter};
use std::io::{Cursor, SeekFrom};
use std::sync::Arc;

use async_trait::async_trait;
use rand::Rng;
use rand::distr::Alphanumeric;
use reqwest::Method;
use reqwest::header::HeaderValue;
use serde_json::Value;
use serde_json::value::RawValue;
use skald_spec::{ProviderRequest, ProviderResponse};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use super::{Payload, ProviderByteStream};
use crate::auth::OpenAiAuth;
use crate::error::{ProviderError, ProviderResult};
use crate::raw;
use crate::retry::RetryPolicy;
use crate::stream::decode_sse_events;
use crate::trait_::{ProviderClient, ProviderStream};
use crate::transport::{HttpTransport, TransportConfig};

/// OpenAI JSON route of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiRoute {
    /// `POST {base}/chat/completions`.
    ChatCompletions,
    /// `POST {base}/responses`.
    Responses,
    /// `POST {base}/embeddings`.
    Embeddings,
}

impl OpenAiRoute {
    /// Path of the route under the configured base URL.
    const fn path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/chat/completions",
            Self::Responses => "/responses",
            Self::Embeddings => "/embeddings",
        }
    }
}

/// OpenAI Images or Audio route: a JSON or multipart request whose answer is
/// JSON, text, or binary media.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiMediaRoute {
    /// `POST {base}/images/generations` with a JSON body.
    ImageGenerations,
    /// `POST {base}/images/edits` with a multipart body.
    ImageEdits,
    /// `POST {base}/images/variations` with a multipart body.
    ImageVariations,
    /// `POST {base}/audio/speech` with a JSON body; answers with audio bytes.
    AudioSpeech,
    /// `POST {base}/audio/transcriptions` with a multipart body.
    AudioTranscriptions,
    /// `POST {base}/audio/translations` with a multipart body.
    AudioTranslations,
}

impl OpenAiMediaRoute {
    /// Path of the route under the configured base URL.
    const fn path(self) -> &'static str {
        match self {
            Self::ImageGenerations => "/images/generations",
            Self::ImageEdits => "/images/edits",
            Self::ImageVariations => "/images/variations",
            Self::AudioSpeech => "/audio/speech",
            Self::AudioTranscriptions => "/audio/transcriptions",
            Self::AudioTranslations => "/audio/translations",
        }
    }

    /// Whether the route takes `multipart/form-data` rather than JSON.
    #[must_use]
    pub const fn multipart(self) -> bool {
        !matches!(self, Self::ImageGenerations | Self::AudioSpeech)
    }
}

/// OpenAI Files or Batches lifecycle route; resource ids are the provider's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiBatchRoute {
    /// `POST {base}/files` with a multipart body.
    UploadFile,
    /// `GET {base}/files/{id}/content`.
    FileContent(String),
    /// `DELETE {base}/files/{id}`.
    DeleteFile(String),
    /// `POST {base}/batches` with a JSON body.
    CreateBatch,
    /// `GET {base}/batches/{id}`.
    RetrieveBatch(String),
    /// `POST {base}/batches/{id}/cancel`.
    CancelBatch(String),
}

impl OpenAiBatchRoute {
    /// Method and path of the route under the configured base URL.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::BadRequest`] when a resource id is empty or
    /// carries anything but ASCII letters, digits, `-`, and `_`, so an id can
    /// never change the path it names.
    fn request(&self) -> ProviderResult<(Method, String)> {
        let id = |id: &str| {
            if !id.is_empty()
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                Ok(id.to_owned())
            } else {
                Err(ProviderError::bad_request(
                    "openai",
                    "a resource id must be ASCII letters, digits, '-', or '_'",
                ))
            }
        };
        Ok(match self {
            Self::UploadFile => (Method::POST, "/files".to_owned()),
            Self::FileContent(file) => (Method::GET, format!("/files/{}/content", id(file)?)),
            Self::DeleteFile(file) => (Method::DELETE, format!("/files/{}", id(file)?)),
            Self::CreateBatch => (Method::POST, "/batches".to_owned()),
            Self::RetrieveBatch(batch) => (Method::GET, format!("/batches/{}", id(batch)?)),
            Self::CancelBatch(batch) => (Method::POST, format!("/batches/{}/cancel", id(batch)?)),
        })
    }
}

/// One file part of a multipart media request.
#[derive(Clone)]
pub struct UploadFile {
    /// Form field name, such as `image` or `file`.
    pub field: String,
    /// Client file name sent in the part's disposition.
    pub filename: String,
    /// Media type of `content`.
    pub content_type: String,
    /// File content.
    pub content: UploadContent,
}

/// Content of one uploaded file part.
#[derive(Debug, Clone)]
pub enum UploadContent {
    /// Content held in memory.
    Bytes(Vec<u8>),
    /// Content spooled to a file, streamed from its start when sent so a large
    /// upload never sits in memory. Clones share the file, whose cursor a send
    /// moves, so one content is sent by one request at a time.
    Spooled {
        /// Spooled file; an anonymous temporary file disappears once the last
        /// clone closes it.
        file: Arc<std::fs::File>,
        /// Number of content bytes from the file's start.
        len: u64,
    },
}

impl UploadContent {
    /// Content length in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::Spooled { len, .. } => *len,
        }
    }

    /// Whether the content is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reader over the content from its first byte.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Decode`] when a spooled file cannot be reopened
    /// or rewound.
    async fn reader(&self) -> ProviderResult<Box<dyn AsyncRead + Send + Sync + Unpin>> {
        let unreadable = |error: std::io::Error| ProviderError::decode("openai", error);
        match self {
            Self::Bytes(bytes) => Ok(Box::new(Cursor::new(bytes.clone()))),
            Self::Spooled { file, len } => {
                let mut reader = tokio::fs::File::from_std(file.try_clone().map_err(unreadable)?);
                reader.seek(SeekFrom::Start(0)).await.map_err(unreadable)?;
                Ok(Box::new(reader.take(*len)))
            }
        }
    }
}

impl Debug for UploadFile {
    /// Formats the part with its size instead of its content.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadFile")
            .field("field", &self.field)
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("size", &self.content.len())
            .finish()
    }
}

/// Successful media answer, read through the transport's body bound.
#[derive(Clone, PartialEq, Eq)]
pub struct MediaAnswer {
    /// The provider's `content-type`, or `application/octet-stream`.
    pub content_type: String,
    /// Answer bytes, unchanged.
    pub bytes: Vec<u8>,
}

impl Debug for MediaAnswer {
    /// Formats the answer with its size instead of its content.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaAnswer")
            .field("content_type", &self.content_type)
            .field("size", &self.bytes.len())
            .finish()
    }
}

/// OpenAI provider client.
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    auth: OpenAiAuth,
    transport: HttpTransport,
    retry: RetryPolicy,
}

impl OpenAiClient {
    /// Creates an OpenAI client from explicit auth.
    pub fn new(auth: OpenAiAuth) -> ProviderResult<Self> {
        Ok(Self::with_transport(
            auth,
            HttpTransport::new(TransportConfig::default())?,
            RetryPolicy::default(),
        ))
    }

    /// Creates an OpenAI client from environment variables.
    pub fn from_env() -> ProviderResult<Self> {
        Self::new(OpenAiAuth::from_env()?)
    }

    /// Creates an OpenAI client over a shared transport and retry policy.
    pub const fn with_transport(
        auth: OpenAiAuth,
        transport: HttpTransport,
        retry: RetryPolicy,
    ) -> Self {
        Self {
            auth,
            transport,
            retry,
        }
    }

    /// URL of `route` under the configured base URL; every request builds its
    /// URL here.
    fn url(&self, route: OpenAiRoute) -> String {
        format!("{}{}", self.auth.base_url(), route.path())
    }

    /// Posts a caller-built JSON `body` to `route` and returns the provider's
    /// answer bytes unchanged.
    ///
    /// Uses the same URL, auth headers, transport, retry policy, bounded read,
    /// and status errors as typed requests, but skips (de)serialization, so
    /// provider members the Skald wire types do not model survive unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Status`] for a non-success answer,
    /// [`ProviderError::Decode`] when the answer is not JSON or the auth
    /// headers are invalid, and [`ProviderError::Connect`],
    /// [`ProviderError::Timeout`], or [`ProviderError::Upstream`] when the
    /// exchange fails.
    pub async fn send_raw(
        &self,
        route: OpenAiRoute,
        body: &RawValue,
    ) -> ProviderResult<Box<RawValue>> {
        raw::send_raw(
            &self.transport,
            "openai",
            &self.url(route),
            self.auth.headers()?,
            body,
            &self.retry,
        )
        .await
    }

    /// Posts a caller-built streaming JSON `body` to `route` and returns the
    /// answer for incremental reading, its bytes unchanged.
    ///
    /// Uses the same URL and auth headers as [`Self::send_raw`] but never
    /// retries.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`super::open_stream`], or
    /// [`ProviderError::Decode`] when the auth headers are invalid.
    pub async fn stream_raw(
        &self,
        route: OpenAiRoute,
        body: &RawValue,
    ) -> ProviderResult<super::ProviderByteStream> {
        super::open_stream(
            &self.transport,
            "openai",
            &self.url(route),
            self.auth.headers()?,
            body,
        )
        .await
    }

    /// Sends one Images or Audio request and returns the answer bytes with
    /// their content type.
    ///
    /// JSON routes send `body` as JSON. Multipart routes send every member of
    /// the `body` object as a text field (an array member as repeated fields,
    /// `null` omitted), then `files` in order. Never retries, because media
    /// generation is not idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::BadRequest`] when a multipart `body` is not an
    /// object of scalar or scalar-array members or a file carries an invalid
    /// content type, [`ProviderError::Decode`] when `body` cannot serialize or
    /// the auth headers are invalid, and the errors of the bounded exchange:
    /// [`ProviderError::Status`], [`ProviderError::Connect`],
    /// [`ProviderError::Timeout`], or [`ProviderError::Upstream`].
    pub async fn send_media(
        &self,
        route: OpenAiMediaRoute,
        body: &Value,
        files: &[UploadFile],
    ) -> ProviderResult<MediaAnswer> {
        let (content_type, bytes) = self
            .transport
            .send_once(
                "openai",
                Method::POST,
                &format!("{}{}", self.auth.base_url(), route.path()),
                self.auth.headers()?,
                Some(media_payload(route, body, files).await?),
            )
            .await?;
        Ok(MediaAnswer {
            content_type,
            bytes,
        })
    }

    /// Sends one Images or Audio request like [`Self::send_media`] and returns
    /// the successful answer for incremental reading, so a large answer such as
    /// generated speech is relayed rather than collected.
    ///
    /// # Errors
    ///
    /// Returns the request encoding errors of [`Self::send_media`], the mapped
    /// transport error, and [`ProviderError::Status`] or
    /// [`ProviderError::Upstream`] for a refusal.
    pub async fn stream_media(
        &self,
        route: OpenAiMediaRoute,
        body: &Value,
        files: &[UploadFile],
    ) -> ProviderResult<ProviderByteStream> {
        let response = self
            .transport
            .open_once(
                "openai",
                Method::POST,
                &format!("{}{}", self.auth.base_url(), route.path()),
                self.auth.headers()?,
                Some(media_payload(route, body, files).await?),
            )
            .await?;
        Ok(ProviderByteStream {
            provider: "openai".to_owned(),
            response,
        })
    }

    /// Sends one Files or Batches lifecycle request and returns the answer
    /// bytes with their content type.
    ///
    /// A file upload sends `body` members as text fields and `file` as a
    /// multipart part; batch creation sends `body` as JSON; reads, content
    /// retrieval, cancellation, and deletion send no body. Never retries,
    /// because uploads and batch creation start provider work.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::BadRequest`] when an upload carries no file or
    /// an unencodable body, or a resource id is empty or not made of ASCII
    /// letters, digits, `-`, and `_`; [`ProviderError::Decode`] when `body`
    /// cannot serialize or the auth headers are invalid; and the errors of the
    /// bounded exchange: [`ProviderError::Status`], [`ProviderError::Connect`],
    /// [`ProviderError::Timeout`], or [`ProviderError::Upstream`].
    pub async fn send_batch(
        &self,
        route: &OpenAiBatchRoute,
        body: &Value,
        file: Option<&UploadFile>,
    ) -> ProviderResult<MediaAnswer> {
        let (method, path) = route.request()?;
        let payload = match route {
            OpenAiBatchRoute::UploadFile => {
                let file = file.ok_or_else(|| {
                    ProviderError::bad_request("openai", "an upload needs a file")
                })?;
                Some(multipart(body, std::slice::from_ref(file)).await?)
            }
            OpenAiBatchRoute::CreateBatch => Some(Payload::bytes(
                HeaderValue::from_static("application/json"),
                serde_json::to_vec(body).map_err(|error| ProviderError::decode("openai", error))?,
            )),
            _ => None,
        };
        let (content_type, bytes) = self
            .transport
            .send_once(
                "openai",
                method,
                &format!("{}{path}", self.auth.base_url()),
                self.auth.headers()?,
                payload,
            )
            .await?;
        Ok(MediaAnswer {
            content_type,
            bytes,
        })
    }

    /// Sends a native OpenAI request.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                let url = self.url(OpenAiRoute::ChatCompletions);
                let response = super::send_json_with_retry(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::OpenAiChatCompletion(response))
            }
            ProviderRequest::OpenAiResponses(request) => {
                let url = self.url(OpenAiRoute::Responses);
                let response = super::send_json_with_retry(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::OpenAiResponses(response))
            }
            ProviderRequest::OpenAiEmbeddings(request) => {
                let url = self.url(OpenAiRoute::Embeddings);
                let response = super::send_json_with_retry(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::OpenAiEmbeddings(response))
            }
            ProviderRequest::RawV1 { body, .. } => {
                let url = format!("{}/raw", self.auth.base_url());
                let response = raw::send_raw(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &body,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::RawV1(response))
            }
            other => Err(ProviderError::variant_mismatch(
                "openai",
                super::request_variant_label(&other),
            )),
        }
    }
}

/// Encodes `body` members and `files` as `multipart/form-data`.
///
/// The boundary is 40 random alphanumerics, which a part collides with only
/// by chance. Field names and file names escape `"`, CR, and LF as
/// percent-encodings, as browsers do.
///
/// # Errors
///
/// Returns [`ProviderError::BadRequest`] for a non-object body, a nested
/// object or array member, or a file content type that is not a valid header
/// value.
async fn multipart(body: &Value, files: &[UploadFile]) -> ProviderResult<Payload> {
    let rejected = |detail: &str| ProviderError::bad_request("openai", detail);
    let Value::Object(members) = body else {
        return Err(rejected("a multipart body must be a JSON object"));
    };
    let boundary: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(40)
        .map(char::from)
        .collect();
    let escape = |text: &str| {
        text.replace('"', "%22")
            .replace('\r', "%0D")
            .replace('\n', "%0A")
    };
    let mut payload = Vec::new();
    let mut text = |name: &str, value: &Value| {
        let value = match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            Value::Bool(flag) => flag.to_string(),
            Value::Null => return Ok(()),
            Value::Array(_) | Value::Object(_) => {
                return Err(rejected(
                    "multipart fields must be scalars or scalar arrays",
                ));
            }
        };
        payload.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{value}\r\n",
                escape(name)
            )
            .as_bytes(),
        );
        Ok(())
    };
    for (name, value) in members {
        match value {
            Value::Array(items) => {
                for item in items {
                    text(name, item)?;
                }
            }
            value => text(name, value)?,
        }
    }
    // In-memory parts accumulate in `payload`; each spooled file closes the
    // current segment so its content streams between in-memory segments.
    let mut segments: Vec<Box<dyn AsyncRead + Send + Sync + Unpin>> = Vec::new();
    let mut len = 0_u64;
    for file in files {
        HeaderValue::from_str(&file.content_type)
            .map_err(|_| rejected("a file content type must be a valid header value"))?;
        payload.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                escape(&file.field),
                escape(&file.filename),
                file.content_type
            )
            .as_bytes(),
        );
        match &file.content {
            UploadContent::Bytes(bytes) => payload.extend_from_slice(bytes),
            spooled @ UploadContent::Spooled { .. } => {
                len += payload.len() as u64 + spooled.len();
                segments.push(Box::new(Cursor::new(std::mem::take(&mut payload))));
                segments.push(spooled.reader().await?);
            }
        }
        payload.extend_from_slice(b"\r\n");
    }
    payload.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
        .map_err(|error| ProviderError::decode("openai", error))?;
    if segments.is_empty() {
        return Ok(Payload::bytes(content_type, payload));
    }
    len += payload.len() as u64;
    segments.push(Box::new(Cursor::new(payload)));
    let body = segments.into_iter().fold(
        Box::new(tokio::io::empty()) as Box<dyn AsyncRead + Send + Sync + Unpin>,
        |joined, segment| Box::new(joined.chain(segment)),
    );
    Ok(Payload::streamed(
        content_type,
        len,
        reqwest::Body::wrap_stream(ReaderStream::new(body)),
    ))
}

/// Encodes a media request: multipart routes as a form, JSON routes as JSON.
///
/// # Errors
///
/// Returns the errors of [`multipart`], or [`ProviderError::Decode`] when a
/// JSON `body` cannot serialize.
async fn media_payload(
    route: OpenAiMediaRoute,
    body: &Value,
    files: &[UploadFile],
) -> ProviderResult<Payload> {
    if route.multipart() {
        return multipart(body, files).await;
    }
    let payload =
        serde_json::to_vec(body).map_err(|error| ProviderError::decode("openai", error))?;
    Ok(Payload::bytes(
        HeaderValue::from_static("application/json"),
        payload,
    ))
}

#[async_trait]
impl ProviderClient for OpenAiClient {
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        self.send_native(request).await
    }

    async fn stream(&self, request: ProviderRequest) -> ProviderResult<ProviderStream> {
        match request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                let url = self.url(OpenAiRoute::ChatCompletions);
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("openai", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "openai",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let chunks = decode_sse_events("openai", text.as_bytes())?;
                Ok(ProviderStream::OpenAiChat(chunks))
            }
            ProviderRequest::OpenAiResponses(request) => {
                let url = self.url(OpenAiRoute::Responses);
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("openai", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "openai",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let events = decode_sse_events("openai", text.as_bytes())?;
                Ok(ProviderStream::OpenAiResponses(events))
            }
            other => Err(ProviderError::variant_mismatch(
                "openai",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[cfg(test)]
mod openai_chat {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{OpenAiChatRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn chat_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/chat_completion_request.json");
        let response_body = common::fixture("openai/chat_completion_response.json");
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiChatRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect("request succeeds");

        assert!(matches!(
            response,
            ProviderResponse::OpenAiChatCompletion(_)
        ));
        common::assert_received_body(&server, &request_body).await;
    }

    #[tokio::test]
    async fn chat_tool_calls_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/chat_completion_request.json");
        let response_body = common::fixture("openai/chat_completion_tool_call_response.json");
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiChatRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect("request succeeds");

        assert_eq!(response.adapter().tool_calls().len(), 1);
    }

    #[tokio::test]
    async fn chat_429_retries_then_succeeds() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/chat_completion_request.json");
        let response_body = common::fixture("openai/chat_completion_response.json");
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiChatRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect("retry succeeds");

        assert!(matches!(
            response,
            ProviderResponse::OpenAiChatCompletion(_)
        ));
    }
}

#[cfg(test)]
mod openai_raw_route {
    use crate::OpenAiRoute;
    use crate::common;
    use serde_json::value::RawValue;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A raw body reaches the typed route with its auth, byte for byte, and
    /// the answer returns unchanged, including members Skald does not model.
    #[tokio::test]
    async fn raw_route_sends_and_returns_bytes_unchanged() {
        let server = MockServer::start().await;
        let request = r#"{"model":"m","messages":[],"vendor":{"x":1}}"#;
        let answer = r#"{"id":"c","vendor_extra":true,"choices":[]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .and(body_string(request))
            .respond_with(ResponseTemplate::new(200).set_body_string(answer))
            .mount(&server)
            .await;

        let body = RawValue::from_string(request.to_owned()).expect("raw body");
        let response = common::openai_client(&server.uri())
            .send_raw(OpenAiRoute::ChatCompletions, &body)
            .await
            .expect("raw route succeeds");

        assert_eq!(response.get(), answer);
    }
}

#[cfg(test)]
mod openai_media {
    //! `OpenAI` Images and Audio routes: request encoding, bounded answers, and no retry.

    use crate::common;
    use std::io::Write as _;
    use std::sync::Arc;

    use crate::{
        HttpTransport, OpenAiClient, OpenAiMediaRoute, ProviderError, TransportConfig,
        UploadContent, UploadFile,
    };
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A JSON media route sends JSON and returns binary answer bytes with their
    /// content type, buffered or as an incremental stream; a multipart route
    /// sends text fields, repeated array fields, and a spooled file streamed
    /// with its name, type, and exact declared length.
    ///
    /// # Panics
    ///
    /// Panics when a request or answer differs from the expected encoding.
    #[tokio::test]
    async fn media_routes_send_json_or_multipart_and_return_bytes() {
        let server = MockServer::start().await;
        let audio = vec![0_u8, 159, 146, 150, 255];
        Mock::given(method("POST"))
            .and(path("/audio/speech"))
            .and(header("authorization", "Bearer sk-test"))
            .and(body_json(
                json!({"model": "tts-1", "input": "hi", "voice": "alloy"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(audio.clone(), "audio/mpeg"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("{\"text\":\"hi\"}", "application/json"),
            )
            .mount(&server)
            .await;
        let client = common::openai_client(&server.uri());

        let speech = client
            .send_media(
                OpenAiMediaRoute::AudioSpeech,
                &json!({"model": "tts-1", "input": "hi", "voice": "alloy"}),
                &[],
            )
            .await
            .expect("speech succeeds");
        assert_eq!(speech.content_type, "audio/mpeg");
        assert_eq!(speech.bytes, audio);
        let mut streamed = client
            .stream_media(
                OpenAiMediaRoute::AudioSpeech,
                &json!({"model": "tts-1", "input": "hi", "voice": "alloy"}),
                &[],
            )
            .await
            .expect("speech opens");
        assert_eq!(streamed.content_type(), "audio/mpeg");
        let mut relayed = Vec::new();
        while let Some(chunk) = streamed.chunk().await.expect("speech reads") {
            relayed.extend_from_slice(&chunk);
        }
        assert_eq!(relayed, audio);

        let mut spool = tempfile::tempfile().expect("spool file");
        spool.write_all(&audio).expect("spool writes");

        let transcript = client
            .send_media(
                OpenAiMediaRoute::AudioTranscriptions,
                &json!({"model": "whisper-1", "prompt": null, "temperature": 0, "timestamp_granularities[]": ["word", "segment"]}),
                &[UploadFile {
                    field: "file".to_owned(),
                    filename: "a\"b.wav".to_owned(),
                    content_type: "audio/wav".to_owned(),
                    content: UploadContent::Spooled {
                        file: Arc::new(spool),
                        len: audio.len() as u64,
                    },
                }],
            )
            .await
            .expect("transcription succeeds");
        assert_eq!(transcript.bytes, b"{\"text\":\"hi\"}");
        let requests = server.received_requests().await.expect("recording");
        let sent = requests.last().expect("multipart request");
        let content_type = sent.headers["content-type"].to_str().expect("header");
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("multipart content type");
        let field = |name: &str, value: &str| {
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
        };
        let mut expected = [
            field("model", "whisper-1"),
            field("temperature", "0"),
            field("timestamp_granularities[]", "word"),
            field("timestamp_granularities[]", "segment"),
        ]
        .concat()
        .into_bytes();
        expected.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a%22b.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes(),
        );
        expected.extend_from_slice(&audio);
        expected.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        assert_eq!(sent.body, expected);
    }

    /// Media answers past the transport bound fail, refusals keep their
    /// status and body without a retry, and nested multipart members are
    /// rejected before sending.
    ///
    /// # Panics
    ///
    /// Panics when an oversized answer, refusal, or invalid body is not
    /// reported as expected.
    #[tokio::test]
    async fn media_answers_are_bounded_and_never_retried() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/images/generations"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(vec![1_u8; 2048], "image/png"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/images/variations"))
            .respond_with(ResponseTemplate::new(429).set_body_string("busy"))
            .mount(&server)
            .await;
        let client = OpenAiClient::with_transport(
            crate::auth::OpenAiAuth::new("sk-test").with_base_url(server.uri()),
            HttpTransport::new(TransportConfig {
                max_response_bytes: 1024,
                ..TransportConfig::default()
            })
            .expect("transport builds"),
            common::retry_policy(),
        );
        let oversized = client
            .send_media(
                OpenAiMediaRoute::ImageGenerations,
                &json!({"prompt": "x"}),
                &[],
            )
            .await
            .expect_err("oversized answer fails");
        assert!(
            matches!(oversized, ProviderError::Upstream { status: 200, .. }),
            "{oversized:?}"
        );
        let refused = client
            .send_media(OpenAiMediaRoute::ImageVariations, &json!({"n": 1}), &[])
            .await
            .expect_err("refusal fails");
        assert!(
            matches!(&refused, ProviderError::Status { status: 429, body, .. } if body == "busy"),
            "{refused:?}"
        );
        let nested = client
            .send_media(
                OpenAiMediaRoute::ImageEdits,
                &json!({"extra": {"a": 1}}),
                &[],
            )
            .await
            .expect_err("nested member is rejected");
        assert!(
            matches!(nested, ProviderError::BadRequest { .. }),
            "{nested:?}"
        );
        let requests = server.received_requests().await.expect("recording");
        assert_eq!(
            requests.len(),
            2,
            "one request per call, none for the rejected body"
        );
    }
}

#[cfg(test)]
mod openai_batch {
    //! `OpenAI` Files and Batches routes: methods, bodies, and provider id validation.

    use crate::common;
    use crate::{OpenAiBatchRoute, ProviderError, UploadContent, UploadFile};
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Every lifecycle route uses its method and path: an upload is multipart
    /// with its text fields and file, creation is JSON, and reads, content,
    /// cancellation, and deletion send no body; answers return unchanged. An
    /// id that could change the path is rejected before sending.
    ///
    /// # Panics
    ///
    /// Panics when a request or answer differs from the expected exchange.
    #[tokio::test]
    async fn batch_routes_use_their_methods_and_bodies() {
        let server = MockServer::start().await;
        for (verb, route, answer) in [
            ("POST", "/files", r#"{"id":"file-up"}"#),
            ("GET", "/files/file-out/content", "{\"custom_id\":\"a\"}\n"),
            ("DELETE", "/files/file-up", r#"{"deleted":true}"#),
            ("GET", "/batches/batch_1", r#"{"id":"batch_1"}"#),
            (
                "POST",
                "/batches/batch_1/cancel",
                r#"{"status":"cancelling"}"#,
            ),
        ] {
            Mock::given(method(verb))
                .and(path(route))
                .and(header("authorization", "Bearer sk-test"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(answer, "application/json"))
                .mount(&server)
                .await;
        }
        Mock::given(method("POST"))
            .and(path("/batches"))
            .and(body_json(
                json!({"input_file_id": "file-up", "endpoint": "/v1/chat/completions"}),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(r#"{"id":"batch_1"}"#, "application/json"),
            )
            .mount(&server)
            .await;
        let client = common::openai_client(&server.uri());
        let file = UploadFile {
            field: "file".to_owned(),
            filename: "in.jsonl".to_owned(),
            content_type: "application/jsonl".to_owned(),
            content: UploadContent::Bytes(b"{}\n".to_vec()),
        };
        let none = json!({});
        let exchanges = [
            (
                OpenAiBatchRoute::UploadFile,
                json!({"purpose": "batch"}),
                Some(&file),
                r#"{"id":"file-up"}"#,
            ),
            (
                OpenAiBatchRoute::FileContent("file-out".to_owned()),
                none.clone(),
                None,
                "{\"custom_id\":\"a\"}\n",
            ),
            (
                OpenAiBatchRoute::DeleteFile("file-up".to_owned()),
                none.clone(),
                None,
                r#"{"deleted":true}"#,
            ),
            (
                OpenAiBatchRoute::CreateBatch,
                json!({"input_file_id": "file-up", "endpoint": "/v1/chat/completions"}),
                None,
                r#"{"id":"batch_1"}"#,
            ),
            (
                OpenAiBatchRoute::RetrieveBatch("batch_1".to_owned()),
                none.clone(),
                None,
                r#"{"id":"batch_1"}"#,
            ),
            (
                OpenAiBatchRoute::CancelBatch("batch_1".to_owned()),
                none.clone(),
                None,
                r#"{"status":"cancelling"}"#,
            ),
        ];
        for (route, body, file, answer) in exchanges {
            let sent = client
                .send_batch(&route, &body, file)
                .await
                .unwrap_or_else(|error| panic!("{route:?} succeeds: {error}"));
            assert_eq!(sent.bytes, answer.as_bytes(), "{route:?}");
        }
        let requests = server.received_requests().await.expect("recording");
        let upload = String::from_utf8_lossy(&requests[0].body);
        assert!(
            upload.contains("name=\"purpose\"\r\n\r\nbatch\r\n")
                && upload.contains("name=\"file\"; filename=\"in.jsonl\"\r\nContent-Type: application/jsonl\r\n\r\n{}\n\r\n"),
            "{upload}"
        );
        assert!(
            requests[1..]
                .iter()
                .filter(|request| request.method.as_str() != "POST"
                    || request.url.path().ends_with("/cancel"))
                .all(|request| request.body.is_empty()),
            "reads, content, cancellation, and deletion send no body"
        );
        for route in [
            OpenAiBatchRoute::RetrieveBatch("../files".to_owned()),
            OpenAiBatchRoute::FileContent(String::new()),
        ] {
            let rejected = client
                .send_batch(&route, &none, None)
                .await
                .expect_err("unsafe id");
            assert!(
                matches!(rejected, ProviderError::BadRequest { .. }),
                "{rejected:?}"
            );
        }
        let missing = client
            .send_batch(
                &OpenAiBatchRoute::UploadFile,
                &json!({"purpose": "batch"}),
                None,
            )
            .await
            .expect_err("an upload needs its file");
        assert!(
            matches!(missing, ProviderError::BadRequest { .. }),
            "{missing:?}"
        );
        assert_eq!(
            server.received_requests().await.expect("recording").len(),
            6,
            "rejected requests never send"
        );
    }
}

#[cfg(test)]
mod openai_embed {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{OpenAiEmbeddingsRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn openai_embeddings_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/embeddings_request.json");
        let response_body = common::fixture("openai/embeddings_response.json");
        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiEmbeddingsRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiEmbeddings(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::OpenAiEmbeddings(_)));
        common::assert_received_body(&server, &request_body).await;
    }
}

#[cfg(test)]
mod openai_responses {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{OpenAiResponsesRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn responses_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/responses_request.json");
        let response_body = common::fixture("openai/responses_response.json");
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiResponsesRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiResponses(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::OpenAiResponses(_)));
        common::assert_received_body(&server, &request_body).await;
    }
}

#[cfg(test)]
mod variant_mismatch {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{AnthropicMessagesRequest, ProviderRequest};

    #[tokio::test]
    async fn wrong_provider_request_variant_returns_variant_mismatch() {
        let request_body = common::fixture("anthropic/messages_request.json");
        let request: AnthropicMessagesRequest =
            serde_json::from_str(&request_body).expect("fixture parses");

        let error = common::openai_client("http://localhost")
            .send(ProviderRequest::AnthropicMessage(request))
            .await
            .expect_err("variant mismatch");

        assert_eq!(error.code(), "SKALD_PROVIDERS_400_VARIANT_MISMATCH");
    }
}
