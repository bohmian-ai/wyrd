//! OpenAI Chat, Responses, Embeddings, Images, Audio, Files, and Batches client.

use std::fmt::{self, Debug, Formatter};
use std::io::{Cursor, SeekFrom};
use std::sync::Arc;

use async_trait::async_trait;
use rand::Rng;
use rand::distr::Alphanumeric;
use reqwest::Method;
use reqwest::header::HeaderValue;
use serde_json::value::RawValue;
use serde_json::{Map, Value};
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
    /// Answer bytes, unchanged; a single-chunk answer shares its chunk.
    pub bytes: bytes::Bytes,
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

/// A request body as the encoders take it: a JSON value, or a borrowed JSON
/// object with an optional `model` set as it encodes.
///
/// Setting the model never copies the object: a present `model` member keeps
/// its position with the new value, an absent one follows the other members,
/// and every other member encodes exactly as stored. JSON and multipart
/// encoding share this one member order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestBody<'a> {
    /// A JSON value encoded as is.
    Json(&'a Value),
    /// A JSON object's members, with the model to set.
    Object {
        /// The object's members, as submitted.
        members: &'a Map<String, Value>,
        /// The model to set, or `None` to encode the members unchanged.
        model: Option<&'a str>,
    },
}

impl<'a> From<&'a Value> for RequestBody<'a> {
    /// Takes a JSON object as its members, any other value as is.
    fn from(body: &'a Value) -> Self {
        match body {
            Value::Object(members) => Self::Object {
                members,
                model: None,
            },
            other => Self::Json(other),
        }
    }
}

/// One member value of a [`RequestBody`] object in encoding order.
#[derive(Clone, Copy)]
enum Member<'a> {
    /// A stored member value.
    Value(&'a Value),
    /// The model set in place of the stored one.
    Model(&'a str),
}

impl serde::Serialize for Member<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value(value) => value.serialize(serializer),
            Self::Model(model) => serializer.serialize_str(model),
        }
    }
}

impl RequestBody<'_> {
    /// Serializes the body as compact JSON bytes.
    ///
    /// The bytes are written once into a buffer reserved from
    /// [`Self::len_hint`], so a body without escapes never grows it, and the
    /// buffer moves into the transport as is, spare capacity included.
    ///
    /// # Errors
    ///
    /// Returns the serialization error of a member serde cannot write.
    pub fn to_json(self) -> serde_json::Result<Vec<u8>> {
        let mut json = Vec::with_capacity(self.len_hint());
        serde_json::to_writer(&mut json, &self)?;
        Ok(json)
    }

    /// Compact JSON length of the body with its model applied, assuming no
    /// string needs escapes and every float takes its longest form.
    ///
    /// Only a capacity hint: escapes make the output longer, and shorter
    /// floats or a replaced model make it shorter.
    #[must_use]
    pub fn len_hint(self) -> usize {
        match self {
            Self::Json(value) => json_len_hint(value),
            Self::Object { members, model } => {
                let model = model.map_or(0, |model| model.len() + r#","model":"""#.len());
                object_len_hint(members) + model
            }
        }
    }
}

/// Unescaped compact JSON length of `value`; see [`RequestBody::len_hint`].
fn json_len_hint(value: &Value) -> usize {
    match value {
        Value::Null | Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => match (number.as_u64(), number.as_i64()) {
            (Some(natural), _) => natural
                .checked_ilog10()
                .map_or(1, |digits| digits as usize + 1),
            (None, Some(negative)) => negative.unsigned_abs().ilog10() as usize + 2,
            // The longest shortest-round-trip f64, such as -2.2250738585072014e-308.
            (None, None) => 24,
        },
        Value::String(text) => text.len() + 2,
        Value::Array(items) => {
            2 + items
                .iter()
                .map(|item| json_len_hint(item) + 1)
                .sum::<usize>()
        }
        Value::Object(members) => object_len_hint(members),
    }
}

/// Unescaped compact JSON length of the object `members`; see [`json_len_hint`].
fn object_len_hint(members: &Map<String, Value>) -> usize {
    2 + members
        .iter()
        .map(|(key, value)| key.len() + 4 + json_len_hint(value))
        .sum::<usize>()
}

/// The `members` of a [`RequestBody::Object`] in encoding order with `model`
/// applied.
fn entries<'a>(
    members: &'a Map<String, Value>,
    model: Option<&'a str>,
) -> impl Iterator<Item = (&'a str, Member<'a>)> {
    let absent = model.filter(|_| !members.contains_key("model"));
    let stored = members.iter().map(move |(name, value)| match model {
        Some(model) if name == "model" => (name.as_str(), Member::Model(model)),
        _ => (name.as_str(), Member::Value(value)),
    });
    stored.chain(absent.map(|model| ("model", Member::Model(model))))
}

/// The multipart text fields of `members` in encoding order: each scalar
/// member once and each array member once per item.
fn fields<'a>(
    members: &'a Map<String, Value>,
    model: Option<&'a str>,
) -> impl Iterator<Item = (&'a str, Member<'a>)> {
    entries(members, model).flat_map(|(name, value)| {
        let (scalar, items): (_, &[Value]) = match value {
            Member::Value(Value::Array(items)) => (None, items),
            value => (Some((name, value)), &[]),
        };
        scalar
            .into_iter()
            .chain(items.iter().map(move |item| (name, Member::Value(item))))
    })
}

impl serde::Serialize for RequestBody<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::Json(value) => value.serialize(serializer),
            Self::Object { members, model } => serializer.collect_map(entries(members, model)),
        }
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
        body: Vec<u8>,
    ) -> ProviderResult<Box<RawValue>> {
        let answer = self.send_raw_text(route, body).await?;
        raw::answer_json("openai", answer)
    }

    /// Posts a caller-built body exactly as [`Self::send_raw`] does and
    /// returns the successful answer as text without validating it as JSON,
    /// for a caller that validates it in its own single pass.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Self::send_raw`] other than the answer's JSON
    /// [`ProviderError::Decode`].
    pub async fn send_raw_text(&self, route: OpenAiRoute, body: Vec<u8>) -> ProviderResult<String> {
        raw::send_raw_text(
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
        body: Vec<u8>,
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
        body: impl Into<RequestBody<'_>>,
        files: &[UploadFile],
    ) -> ProviderResult<MediaAnswer> {
        let payload = media_payload(route, body.into(), files).await?;
        self.send_media_payload(route, payload).await
    }

    /// Sends one Images or Audio request on a JSON `route` with `json`, the
    /// request already encoded as JSON bytes, like [`Self::send_media`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::BadRequest`] for a multipart `route`, and the
    /// auth and exchange errors of [`Self::send_media`].
    pub async fn send_media_json(
        &self,
        route: OpenAiMediaRoute,
        json: Vec<u8>,
    ) -> ProviderResult<MediaAnswer> {
        self.send_media_payload(route, json_media_payload(route, json)?)
            .await
    }

    /// Sends `payload` on the media `route` once.
    async fn send_media_payload(
        &self,
        route: OpenAiMediaRoute,
        payload: Payload,
    ) -> ProviderResult<MediaAnswer> {
        let (content_type, bytes) = self
            .transport
            .send_once(
                "openai",
                Method::POST,
                &format!("{}{}", self.auth.base_url(), route.path()),
                self.auth.headers()?,
                Some(payload),
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
        body: impl Into<RequestBody<'_>>,
        files: &[UploadFile],
    ) -> ProviderResult<ProviderByteStream> {
        let payload = media_payload(route, body.into(), files).await?;
        self.open_media_payload(route, payload).await
    }

    /// Opens one Images or Audio request on a JSON `route` with `json`, the
    /// request already encoded as JSON bytes, like [`Self::stream_media`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::BadRequest`] for a multipart `route`, and the
    /// auth, transport, and refusal errors of [`Self::stream_media`].
    pub async fn stream_media_json(
        &self,
        route: OpenAiMediaRoute,
        json: Vec<u8>,
    ) -> ProviderResult<ProviderByteStream> {
        self.open_media_payload(route, json_media_payload(route, json)?)
            .await
    }

    /// Opens `payload` on the media `route` once for incremental reading.
    async fn open_media_payload(
        &self,
        route: OpenAiMediaRoute,
        payload: Payload,
    ) -> ProviderResult<ProviderByteStream> {
        let response = self
            .transport
            .open_once(
                "openai",
                Method::POST,
                &format!("{}{}", self.auth.base_url(), route.path()),
                self.auth.headers()?,
                Some(payload),
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
        body: impl Into<RequestBody<'_>>,
        file: Option<&UploadFile>,
    ) -> ProviderResult<MediaAnswer> {
        let (method, path) = route.request()?;
        let body = body.into();
        let payload = match route {
            OpenAiBatchRoute::UploadFile => {
                let file = file.ok_or_else(|| {
                    ProviderError::bad_request("openai", "an upload needs a file")
                })?;
                Some(multipart(body, std::slice::from_ref(file)).await?)
            }
            OpenAiBatchRoute::CreateBatch => Some(Payload::bytes(
                HeaderValue::from_static("application/json"),
                body.to_json()
                    .map_err(|error| ProviderError::decode("openai", error))?,
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
                let body = serde_json::to_vec(&body)
                    .map_err(|error| ProviderError::decode("openai", error))?;
                let url = format!("{}/raw", self.auth.base_url());
                let response = raw::send_raw(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    body,
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
/// percent-encodings, as browsers do. Framing and member text are written
/// straight into the payload, so no member text is copied first.
///
/// # Errors
///
/// Returns [`ProviderError::BadRequest`] for a non-object body, a nested
/// object or array member, or a file content type that is not a valid header
/// value.
async fn multipart(body: RequestBody<'_>, files: &[UploadFile]) -> ProviderResult<Payload> {
    let rejected = |detail: &str| ProviderError::bad_request("openai", detail);
    let RequestBody::Object { members, model } = body else {
        return Err(rejected("a multipart body must be a JSON object"));
    };
    let boundary: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(BOUNDARY_LEN)
        .map(char::from)
        .collect();
    let disposition = |payload: &mut Vec<u8>, name: &str| {
        payload.extend_from_slice(b"--");
        payload.extend_from_slice(boundary.as_bytes());
        payload.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
        escape_into(payload, name);
        payload.push(b'"');
    };
    let mut payload = Vec::with_capacity(multipart_len_hint(members, model, files));
    let mut text = |name: &str, value: Member<'_>| {
        let value = match value {
            Member::Value(Value::Null) => return Ok(()),
            Member::Value(Value::Array(_) | Value::Object(_)) => {
                return Err(rejected(
                    "multipart fields must be scalars or scalar arrays",
                ));
            }
            value => value,
        };
        disposition(&mut payload, name);
        payload.extend_from_slice(b"\r\n\r\n");
        match value {
            Member::Value(Value::String(text)) => payload.extend_from_slice(text.as_bytes()),
            Member::Model(model) => payload.extend_from_slice(model.as_bytes()),
            Member::Value(Value::Number(number)) => {
                payload.extend_from_slice(number.to_string().as_bytes());
            }
            Member::Value(Value::Bool(flag)) => {
                payload.extend_from_slice(if *flag { b"true" } else { b"false" });
            }
            Member::Value(Value::Null | Value::Array(_) | Value::Object(_)) => {
                unreachable!("rejected above")
            }
        }
        payload.extend_from_slice(b"\r\n");
        Ok(())
    };
    for (name, value) in fields(members, model) {
        text(name, value)?;
    }
    // In-memory parts accumulate in `payload`; each spooled file closes the
    // current segment so its content streams between in-memory segments.
    let mut segments: Vec<Box<dyn AsyncRead + Send + Sync + Unpin>> = Vec::new();
    let mut len = 0_u64;
    for file in files {
        HeaderValue::from_str(&file.content_type)
            .map_err(|_| rejected("a file content type must be a valid header value"))?;
        disposition(&mut payload, &file.field);
        payload.extend_from_slice(b"; filename=\"");
        escape_into(&mut payload, &file.filename);
        payload.extend_from_slice(b"\"\r\nContent-Type: ");
        payload.extend_from_slice(file.content_type.as_bytes());
        payload.extend_from_slice(b"\r\n\r\n");
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

/// Number of random alphanumerics in a multipart boundary.
const BOUNDARY_LEN: usize = 40;

/// Framing bytes of one multipart part besides its name and content: the
/// boundary line, the disposition, and the blank and closing lines.
const PART_FRAMING: usize =
    "--\r\nContent-Disposition: form-data; name=\"\"\r\n\r\n\r\n".len() + BOUNDARY_LEN;

/// Length of the in-memory multipart payload of `members` and `files`,
/// assuming no name needs escapes and every number takes its longest form.
///
/// Only a capacity hint, so the payload is written once without growing:
/// escapes make the output longer, and spooled files stream outside it.
fn multipart_len_hint(
    members: &Map<String, Value>,
    model: Option<&str>,
    files: &[UploadFile],
) -> usize {
    let text = fields(members, model)
        .map(|(name, value)| {
            let value = match value {
                Member::Value(Value::Null | Value::Array(_) | Value::Object(_)) => return 0,
                Member::Value(Value::String(text)) => text.len(),
                Member::Model(model) => model.len(),
                Member::Value(Value::Number(_) | Value::Bool(_)) => 24,
            };
            PART_FRAMING + name.len() + value
        })
        .sum::<usize>();
    let files = files
        .iter()
        .map(|file| {
            let content = match &file.content {
                UploadContent::Bytes(bytes) => bytes.len(),
                UploadContent::Spooled { .. } => 0,
            };
            PART_FRAMING
                + "; filename=\"\"\r\nContent-Type: ".len()
                + file.field.len()
                + file.filename.len()
                + file.content_type.len()
                + content
        })
        .sum::<usize>();
    text + files + BOUNDARY_LEN + "----\r\n".len()
}

/// Writes `text` into `payload` with `"`, CR, and LF percent-encoded, as
/// browsers encode form names; every other byte is written unchanged.
fn escape_into(payload: &mut Vec<u8>, text: &str) {
    for byte in text.bytes() {
        match byte {
            b'"' => payload.extend_from_slice(b"%22"),
            b'\r' => payload.extend_from_slice(b"%0D"),
            b'\n' => payload.extend_from_slice(b"%0A"),
            byte => payload.push(byte),
        }
    }
}

/// Encodes a media request: multipart routes as a form, JSON routes as JSON.
///
/// # Errors
///
/// Returns the errors of [`multipart`], or [`ProviderError::Decode`] when a
/// JSON `body` cannot serialize.
async fn media_payload(
    route: OpenAiMediaRoute,
    body: RequestBody<'_>,
    files: &[UploadFile],
) -> ProviderResult<Payload> {
    if route.multipart() {
        return multipart(body, files).await;
    }
    let payload = body
        .to_json()
        .map_err(|error| ProviderError::decode("openai", error))?;
    json_media_payload(route, payload)
}

/// The JSON payload of an Images or Audio request on `route`, already encoded
/// as `json`.
///
/// # Errors
///
/// Returns [`ProviderError::BadRequest`] for a multipart `route`.
fn json_media_payload(route: OpenAiMediaRoute, json: Vec<u8>) -> ProviderResult<Payload> {
    if route.multipart() {
        return Err(ProviderError::bad_request(
            "openai",
            "a multipart media route takes form fields",
        ));
    }
    Ok(Payload::bytes(
        HeaderValue::from_static("application/json"),
        json,
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
                let answer = super::send_bytes_with_retry(
                    &self.transport,
                    "openai",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let chunks =
                    decode_sse_events("openai", String::from_utf8_lossy(&answer).as_bytes())?;
                Ok(ProviderStream::OpenAiChat(chunks))
            }
            ProviderRequest::OpenAiResponses(request) => {
                let url = self.url(OpenAiRoute::Responses);
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("openai", error))?;
                let answer = super::send_bytes_with_retry(
                    &self.transport,
                    "openai",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let events =
                    decode_sse_events("openai", String::from_utf8_lossy(&answer).as_bytes())?;
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

        let response = common::openai_client(&server.uri())
            .send_raw(OpenAiRoute::ChatCompletions, request.as_bytes().to_vec())
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
        assert_eq!(transcript.bytes, &b"{\"text\":\"hi\"}"[..]);
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
