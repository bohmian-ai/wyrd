//! Thin napi projection of Vala's Rust-owned Oracle query client.

#![deny(missing_docs)]

use std::sync::{Arc, Mutex};

use arrow::record_batch::RecordBatch;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use secrecy::SecretString;
use tokio::sync::Mutex as AsyncMutex;
use vala_sdk::grpc::BifrostGrpcTransport;
use vala_sdk::{
    BifrostIngestSink, ClientScope, QueryClient, QueryResultStream, SinkKind, ValaSdkError,
    schema_from_json_schema,
};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_queue::QueueConfig;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};

/// Rust-owned stream implementation used by the native owner.
enum NativeStreamOwner {
    /// Production Vala stream.
    Production(Box<QueryResultStream>),
    #[cfg(test)]
    /// Injectable owner used only by deterministic native-owner tests.
    Test(TestStreamOwner),
}

impl NativeStreamOwner {
    /// Polls one decoded batch or terminal state from the underlying owner.
    ///
    /// # Errors
    ///
    /// Returns the owner's protocol, Arrow, transport, terminal, or
    /// incomplete-stream error without changing its retained terminal state.
    ///
    /// # Cancellation
    ///
    /// Cancelling the production future abandons the in-flight response poll;
    /// the caller still owns cleanup of the stream slot.
    async fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        match self {
            Self::Production(stream) => stream.next_batch().await,
            #[cfg(test)]
            Self::Test(stream) => stream.next_batch(),
        }
    }

    /// Returns terminal metadata retained by the underlying owner.
    fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        match self {
            Self::Production(stream) => stream.terminal(),
            #[cfg(test)]
            Self::Test(stream) => stream.terminal(),
        }
    }
}

#[cfg(test)]
/// Deterministic stream owner used to exercise native cleanup branches.
struct TestStreamOwner {
    /// Results returned by successive native polls.
    next: std::collections::VecDeque<Result<Option<RecordBatch>, ValaSdkError>>,
    /// Terminal metadata exposed while projecting a failed or successful end.
    terminal: Option<wyrd_spec::vala::api::QueryTerminalFrame>,
    /// Sentinel set when the native owner drops this stream.
    dropped: Arc<std::sync::atomic::AtomicBool>,
    /// Counts calls into the injected owner so closed polls prove no re-entry.
    polls: Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
impl TestStreamOwner {
    /// Creates an owner with injected poll results and terminal metadata.
    fn new(
        next: std::collections::VecDeque<Result<Option<RecordBatch>, ValaSdkError>>,
        terminal: Option<wyrd_spec::vala::api::QueryTerminalFrame>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
        polls: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            next,
            terminal,
            dropped,
            polls,
        }
    }

    /// Returns the next injected result.
    ///
    /// # Errors
    ///
    /// Returns the injected error, or [`ValaSdkError::IncompleteQueryStream`]
    /// after all injected results have been consumed.
    fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        self.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.next
            .pop_front()
            .unwrap_or(Err(ValaSdkError::IncompleteQueryStream))
    }

    /// Returns injected terminal metadata.
    fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        self.terminal.as_ref()
    }
}

#[cfg(test)]
impl Drop for TestStreamOwner {
    /// Marks the sentinel before the native method returns its error.
    fn drop(&mut self) {
        self.dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// JavaScript query request projected onto the pure Wyrd contract.
#[napi(object)]
pub struct NativeQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// `published_only` or `fused`.
    pub visibility: String,
    /// `strict` or `allow_degraded`.
    pub freshness: String,
    /// Optional positive query deadline.
    pub deadline_ms: Option<u32>,
}

/// One raw native iterator step consumed by the TypeScript Arrow facade.
#[napi(object)]
pub struct NativeQueryStep {
    /// Complete Arrow IPC stream containing exactly one record batch.
    pub ipc: Option<Buffer>,
    /// Serialized validated terminal, present only on the final step.
    pub terminal_json: Option<String>,
    /// Stable SDK error code for a failed native step.
    pub error_code: Option<String>,
    /// HTTP-equivalent status for a failed native step.
    pub error_status: Option<u32>,
    /// Stable title for a failed native step.
    pub error_title: Option<String>,
    /// Scrubbed SDK error detail for a failed native step.
    pub error_detail: Option<String>,
    /// Operator-facing remediation for a failed native step.
    pub error_remediation: Option<String>,
    /// Serialized JSON-safe structured details for a failed native step.
    pub error_details_json: Option<String>,
}

/// Structured native result for one live-query lifecycle control.
#[napi(object)]
pub struct NativeLifecycleResult {
    /// Canonical JSON payload when the control succeeded.
    pub value_json: Option<String>,
    /// Stable SDK error code when the control failed.
    pub error_code: Option<String>,
    /// HTTP-equivalent status when the control failed.
    pub error_status: Option<u32>,
    /// Stable title when the control failed.
    pub error_title: Option<String>,
    /// Scrubbed detail when the control failed.
    pub error_detail: Option<String>,
    /// Operator-facing remediation when the control failed.
    pub error_remediation: Option<String>,
    /// Serialized JSON-safe structured details when the control failed.
    pub error_details_json: Option<String>,
}

impl NativeLifecycleResult {
    /// Builds one successful JSON lifecycle projection.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the canonical lifecycle value cannot be serialized.
    fn success(value: &serde_json::Value) -> napi::Result<Self> {
        Ok(Self {
            value_json: Some(serde_json::to_string(&value).map_err(napi_error)?),
            error_code: None,
            error_status: None,
            error_title: None,
            error_detail: None,
            error_remediation: None,
            error_details_json: None,
        })
    }

    /// Builds one failed lifecycle projection with stable Wyrd metadata.
    fn failure(error: &ValaSdkError) -> Self {
        Self {
            value_json: None,
            error_code: Some(error.code().to_owned()),
            error_status: Some(u32::from(error.status())),
            error_title: Some(error.title().to_owned()),
            error_detail: Some(error.detail()),
            error_remediation: Some(error.remediation().to_owned()),
            error_details_json: error
                .safe_details()
                .and_then(|value| serde_json::to_string(&value).ok()),
        }
    }
}

/// Structured result of starting a native terminal-safe query.
#[napi]
pub struct NativeQueryStart {
    /// Rust-owned stream when startup succeeded.
    stream: Option<NativeBifrostQueryStream>,
    /// Stable SDK error code when startup failed.
    error_code: Option<String>,
    /// HTTP-equivalent status when startup failed.
    error_status: Option<u32>,
    /// Stable title when startup failed.
    error_title: Option<String>,
    /// Scrubbed SDK error detail when startup failed.
    error_detail: Option<String>,
    /// Operator-facing remediation when startup failed.
    error_remediation: Option<String>,
    /// Serialized JSON-safe structured details when startup failed.
    error_details_json: Option<String>,
}

impl NativeQueryStart {
    /// Builds the successful side of the closed startup result.
    fn success(stream: QueryResultStream) -> Self {
        Self {
            stream: Some(NativeBifrostQueryStream::new(stream)),
            error_code: None,
            error_status: None,
            error_title: None,
            error_detail: None,
            error_remediation: None,
            error_details_json: None,
        }
    }

    /// Builds the failed side with stable metadata retained as independent fields.
    fn failure(error: &ValaSdkError) -> Self {
        Self {
            stream: None,
            error_code: Some(error.code().to_owned()),
            error_status: Some(u32::from(error.status())),
            error_title: Some(error.title().to_owned()),
            error_detail: Some(error.detail()),
            error_remediation: Some(error.remediation().to_owned()),
            error_details_json: error
                .safe_details()
                .and_then(|value| serde_json::to_string(&value).ok()),
        }
    }
}

#[napi]
impl NativeQueryStart {
    /// Moves the Rust-owned stream out after a successful startup.
    #[napi]
    pub fn take_stream(&mut self) -> Option<NativeBifrostQueryStream> {
        self.stream.take()
    }

    /// Returns the stable SDK error code when startup failed.
    #[napi(getter)]
    pub fn error_code(&self) -> Option<String> {
        self.error_code.clone()
    }

    /// Returns the HTTP-equivalent status when startup failed.
    #[napi(getter)]
    pub fn error_status(&self) -> Option<u32> {
        self.error_status
    }

    /// Returns the stable title when startup failed.
    #[napi(getter)]
    pub fn error_title(&self) -> Option<String> {
        self.error_title.clone()
    }

    /// Returns the scrubbed SDK error detail when startup failed.
    #[napi(getter)]
    pub fn error_detail(&self) -> Option<String> {
        self.error_detail.clone()
    }

    /// Returns operator-facing remediation when startup failed.
    #[napi(getter)]
    pub fn error_remediation(&self) -> Option<String> {
        self.error_remediation.clone()
    }

    /// Returns serialized JSON-safe structured details when startup failed.
    #[napi(getter)]
    pub fn error_details_json(&self) -> Option<String> {
        self.error_details_json.clone()
    }
}

/// Authenticated native query client sharing Wyrd's HTTP and auth pools.
#[napi]
pub struct NativeBifrostQueryClient {
    /// Rust-owned Oracle query client.
    client: QueryClient,
}

#[napi]
impl NativeBifrostQueryClient {
    /// Constructs a bearer-token query client without performing IO.
    #[napi(constructor)]
    pub fn new(
        mut server_url: String,
        token: String,
        grpc_url: Option<String>,
    ) -> napi::Result<Self> {
        if server_url.trim().is_empty() || token.is_empty() {
            return Err(napi::Error::from_reason(
                "serverUrl and token must not be empty".to_owned(),
            ));
        }
        let base_url_len = server_url.trim_end_matches('/').len();
        server_url.truncate(base_url_len);
        let mut config = ClientConfig {
            http: HttpConfig {
                base_url: server_url,
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        if let Some(grpc_url) = grpc_url.filter(|value| !value.trim().is_empty()) {
            config.grpc.endpoint = grpc_url;
        }
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from(token)),
        )
        .map_err(napi_error)?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth)).map_err(napi_error)?;
        let client = WyrdClient::from_parts(auth, http, config.grpc);
        Ok(Self {
            client: QueryClient::new(&client),
        })
    }

    /// Starts one terminal-safe query through the Rust SDK owner.
    ///
    /// Expected request, Gate, authentication, and transport failures are
    /// returned in [`NativeQueryStart`] so JavaScript never has to parse an
    /// exception message to recover the public Wyrd error contract.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when napi cannot project the structured
    /// startup result itself.
    #[napi]
    pub async fn query(&self, request: NativeQueryRequest) -> napi::Result<NativeQueryStart> {
        let request = BifrostQueryRequest {
            sql: request.sql,
            visibility: match parse_visibility(&request.visibility) {
                Ok(visibility) => visibility,
                Err(error) => return Ok(NativeQueryStart::failure(&error)),
            },
            freshness: match parse_freshness(&request.freshness) {
                Ok(freshness) => freshness,
                Err(error) => return Ok(NativeQueryStart::failure(&error)),
            },
            deadline_ms: request.deadline_ms.map(u64::from),
        };
        Ok(match self.client.query(&request).await {
            Ok(stream) => NativeQueryStart::success(stream),
            Err(error) => NativeQueryStart::failure(&error),
        })
    }

    /// Lists active queries for the authenticated tenant.
    ///
    /// Cancelling the JavaScript promise abandons the pending HTTP request and
    /// does not create client-owned lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn running(&self) -> napi::Result<NativeLifecycleResult> {
        match self.client.running().await {
            Ok(queries) => {
                NativeLifecycleResult::success(&serde_json::to_value(queries).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Gets one active query by canonical request ID.
    ///
    /// Cancelling the JavaScript promise abandons the pending HTTP request and
    /// does not alter the active query.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// validation and Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn status(&self, request_id: String) -> napi::Result<NativeLifecycleResult> {
        let request_id = match parse_request_id(&request_id) {
            Ok(request_id) => request_id,
            Err(error) => {
                return Ok(NativeLifecycleResult::failure(&error));
            }
        };
        match self.client.status(&request_id).await {
            Ok(summary) => {
                NativeLifecycleResult::success(&serde_json::to_value(summary).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Requests server-side cancellation without closing a local stream.
    ///
    /// Once the server accepts cancellation, abandoning the JavaScript promise
    /// does not reverse the server-side lifecycle transition.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// validation and Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn cancel(&self, request_id: String) -> napi::Result<NativeLifecycleResult> {
        let request_id = match parse_request_id(&request_id) {
            Ok(request_id) => request_id,
            Err(error) => {
                return Ok(NativeLifecycleResult::failure(&error));
            }
        };
        match self.client.cancel(&request_id).await {
            Ok(response) => {
                NativeLifecycleResult::success(&serde_json::to_value(response).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Describes one registered table's stored physical schema.
    ///
    /// The description is the server's own projection: the user fields a caller
    /// declares, the correlation inputs the write path resolves, the managed
    /// candidates it may supply, and a canonical table's physical fingerprint.
    /// JavaScript builds its insertable schema from this rather than from a
    /// local copy of the table contract.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn describe_table(
        &self,
        namespace: String,
        name: String,
    ) -> napi::Result<NativeLifecycleResult> {
        match self.client.describe_table(&namespace, &name).await {
            Ok(description) => NativeLifecycleResult::success(
                &serde_json::to_value(description).map_err(napi_error)?,
            ),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Reads one complete authorized cut of a single trace.
    ///
    /// Trace detail has no continuation token: `since` and `until` bound the
    /// scanned window only, and each span carries its own events and links.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// window, authorization, and transport failures are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub async fn get_trace(
        &self,
        trace_id: String,
        since: Option<String>,
        until: Option<String>,
    ) -> napi::Result<NativeLifecycleResult> {
        let request = wyrd_spec::vala::api::GetTraceRequest {
            trace_id,
            since: match parse_window_bound(since.as_deref(), "since") {
                Ok(bound) => bound,
                Err(error) => {
                    return Ok(NativeLifecycleResult::failure(&ValaSdkError::Transport(
                        error,
                    )));
                }
            },
            until: match parse_window_bound(until.as_deref(), "until") {
                Ok(bound) => bound,
                Err(error) => {
                    return Ok(NativeLifecycleResult::failure(&ValaSdkError::Transport(
                        error,
                    )));
                }
            },
        };
        match self.client.get_trace(&request).await {
            Ok(response) => {
                NativeLifecycleResult::success(&serde_json::to_value(response).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Reads one page of `GenAI` generation records.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// window, authorization, and transport failures are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub async fn query_genai(
        &self,
        request: NativeGenAiRequest,
    ) -> napi::Result<NativeLifecycleResult> {
        let query = wyrd_spec::vala::api::QueryGenAiRequest {
            window: wyrd_spec::vala::api::QueryWindow {
                since: match parse_window_bound(request.since.as_deref(), "since") {
                    Ok(bound) => bound,
                    Err(error) => {
                        return Ok(NativeLifecycleResult::failure(&ValaSdkError::Transport(
                            error,
                        )));
                    }
                },
                until: match parse_window_bound(request.until.as_deref(), "until") {
                    Ok(bound) => bound,
                    Err(error) => {
                        return Ok(NativeLifecycleResult::failure(&ValaSdkError::Transport(
                            error,
                        )));
                    }
                },
                limit: request.limit,
                page_token: request.page_token,
            },
            conversation_id: request.conversation_id,
            model: request.model,
            provider: request.provider,
        };
        match self.client.query_genai(&query).await {
            Ok(response) => {
                NativeLifecycleResult::success(&serde_json::to_value(response).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Projects one described table's writable Arrow schema as schema-only IPC.
    ///
    /// The buffer carries no batch, so JavaScript decodes it with its installed
    /// Arrow implementation instead of reimplementing the description's
    /// field/type conversion. `include_event_time` selects whether the caller
    /// intends to supply the managed event-time column.
    ///
    /// # Errors
    ///
    /// Returns a napi error when `description_json` is not one describe
    /// response, when the description declares a column the write path already
    /// appends, or when the schema cannot be encoded.
    // justification: napi boundary; a JavaScript string is primitive and cannot
    // be passed by reference, so the generated binding requires an owned String
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn writable_schema_ipc(
        &self,
        description_json: String,
        include_event_time: bool,
    ) -> napi::Result<Buffer> {
        let description: wyrd_spec::vala::api::BifrostTableDescription =
            serde_json::from_str(&description_json).map_err(napi_error)?;
        let schema = wyrd_queue::schema::writable_schema(&description, include_event_time)
            .map_err(napi_error)?;
        let mut buffer = Vec::new();
        {
            let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &schema)
                .map_err(napi_error)?;
            writer.finish().map_err(napi_error)?;
        }
        Ok(Buffer::from(buffer))
    }
}

/// `GenAI` filter request as JavaScript sends it.
///
/// Window bounds arrive as RFC 3339 text because napi has no native chrono
/// projection; every field is optional so an unfiltered page is the default.
#[napi(object)]
pub struct NativeGenAiRequest {
    /// Inclusive lower bound on event time, RFC 3339.
    pub since: Option<String>,
    /// Exclusive upper bound on event time, RFC 3339.
    pub until: Option<String>,
    /// Requested page size.
    pub limit: Option<u32>,
    /// Continuation token from a prior page.
    pub page_token: Option<String>,
    /// Conversation-id filter.
    pub conversation_id: Option<String>,
    /// Model-name filter.
    pub model: Option<String>,
    /// Provider filter.
    pub provider: Option<String>,
}

/// Parses one optional RFC 3339 window bound at the napi boundary.
///
/// # Errors
///
/// Returns the stable Wyrd validation error naming the offending field when
/// the text is not an RFC 3339 timestamp.
fn parse_window_bound(
    value: Option<&str>,
    field: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, wyrd_spec::error::WyrdError> {
    value
        .map(|text| {
            text.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|error| wyrd_spec::error::WyrdError::Validation {
                    message: format!("{field} must be an RFC 3339 timestamp"),
                    details: serde_json::json!({"field": field, "reason": error.to_string()}),
                })
        })
        .transpose()
}

impl NativeBifrostQueryStream {
    /// Wraps one Rust-owned query stream for napi iteration.
    fn new(stream: QueryResultStream) -> Self {
        let request_id = stream.request_id().as_str().to_owned();
        Self {
            request_id,
            stream: Arc::new(AsyncMutex::new(Some(NativeStreamOwner::Production(
                Box::new(stream),
            )))),
            terminal_json: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    /// Wraps an injectable owner for deterministic native cleanup tests.
    fn new_for_test(owner: TestStreamOwner) -> Self {
        Self {
            request_id: RequestId::now_v7().to_string(),
            stream: Arc::new(AsyncMutex::new(Some(NativeStreamOwner::Test(owner)))),
            terminal_json: Arc::new(Mutex::new(None)),
        }
    }
}

/// Native query stream that retains Rust terminal validation and emits raw IPC.
#[napi]
pub struct NativeBifrostQueryStream {
    /// Canonical lifecycle request identity available before body polling.
    request_id: String,
    /// Mutable Rust query stream serialized across JavaScript `next` calls.
    stream: Arc<AsyncMutex<Option<NativeStreamOwner>>>,
    /// Validated serialized terminal retained after the Rust stream is released.
    terminal_json: Arc<Mutex<Option<String>>>,
}

#[napi]
impl NativeBifrostQueryStream {
    /// Returns the canonical server lifecycle request identity.
    #[napi(getter)]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Returns one Arrow IPC batch or the final validated terminal.
    ///
    /// # Errors
    ///
    /// Returns a napi error carrying the stable Wyrd code for failed,
    /// incomplete, malformed, or transport-terminated streams.
    #[napi]
    pub async fn next(&self) -> napi::Result<NativeQueryStep> {
        let mut stream_slot = self.stream.lock().await;
        let stream = stream_slot
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("query stream is closed".to_owned()))?;
        match stream.next_batch().await {
            Ok(Some(batch)) => match encode_batch(&batch) {
                Ok(ipc) => Ok(NativeQueryStep {
                    ipc: Some(Buffer::from(ipc)),
                    terminal_json: None,
                    error_code: None,
                    error_status: None,
                    error_title: None,
                    error_detail: None,
                    error_remediation: None,
                    error_details_json: None,
                }),
                Err(error) => {
                    *stream_slot = None;
                    Err(error)
                }
            },
            Ok(None) => {
                let terminal = if let Some(terminal) = stream.terminal() {
                    match serde_json::to_string(terminal) {
                        Ok(terminal) => terminal,
                        Err(error) => {
                            *stream_slot = None;
                            return Err(napi_error(error));
                        }
                    }
                } else {
                    *stream_slot = None;
                    return Err(sdk_error(&ValaSdkError::IncompleteQueryStream));
                };
                *stream_slot = None;
                *self
                    .terminal_json
                    .lock()
                    .map_err(|_| napi::Error::from_reason("terminal lock poisoned".to_owned()))? =
                    Some(terminal.clone());
                Ok(NativeQueryStep {
                    ipc: None,
                    terminal_json: Some(terminal),
                    error_code: None,
                    error_status: None,
                    error_title: None,
                    error_detail: None,
                    error_remediation: None,
                    error_details_json: None,
                })
            }
            Err(error) => {
                let terminal = stream
                    .terminal()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(napi_error);
                *stream_slot = None;
                let terminal = terminal?;
                if let Some(terminal) = terminal {
                    *self.terminal_json.lock().map_err(|_| {
                        napi::Error::from_reason("terminal lock poisoned".to_owned())
                    })? = Some(terminal);
                }
                Ok(NativeQueryStep {
                    ipc: None,
                    terminal_json: None,
                    error_code: Some(error.code().to_owned()),
                    error_status: Some(u32::from(error.status())),
                    error_title: Some(error.title().to_owned()),
                    error_detail: Some(error.detail()),
                    error_remediation: Some(error.remediation().to_owned()),
                    error_details_json: error
                        .safe_details()
                        .and_then(|value| serde_json::to_string(&value).ok()),
                })
            }
        }
    }

    /// Drops the response stream so Rust transport cancellation propagates.
    #[napi]
    pub async fn close(&self) {
        *self.stream.lock().await = None;
    }

    /// Returns serialized terminal metadata after validated completion.
    #[napi(getter)]
    pub fn terminal_json(&self) -> napi::Result<Option<String>> {
        self.terminal_json
            .lock()
            .map(|terminal| terminal.clone())
            .map_err(|_| napi::Error::from_reason("terminal lock poisoned".to_owned()))
    }
}

/// Authenticated native write handle over Vala's pooled producer path.
///
/// Mirrors the Python `Bifrost` binding: both are thin conversions over the one
/// [`vala_sdk::Bifrost`] handle, so the durable batching, backpressure, and
/// ingest contract have exactly one owner. The handle is shared through an
/// [`Arc`] so a blocking drain can move it onto a blocking worker without
/// stalling the Node event loop.
#[napi]
pub struct NativeBifrost {
    /// Rust-owned pooled write handle shared with the observe projection.
    handle: Arc<vala_sdk::Bifrost>,
}

/// Connects one authenticated write handle to the configured gRPC ingest plane.
///
/// This is a free function rather than a constructor because connecting is
/// asynchronous and a napi constructor cannot be. The Python binding blocks
/// inside `__new__` instead; both reach the same handle.
///
/// `serverUrl` selects the HTTP authentication plane and `grpcUrl` overrides
/// the ingest endpoint for split-plane and local test deployments.
///
/// # Errors
///
/// Returns a napi error for empty configuration, an unresolvable credential, or
/// a gRPC transport that cannot connect.
#[napi]
pub async fn connect_bifrost(
    mut server_url: String,
    api_key: String,
    grpc_url: Option<String>,
) -> napi::Result<NativeBifrost> {
    if server_url.trim().is_empty() || api_key.is_empty() {
        return Err(napi::Error::from_reason(
            "serverUrl and apiKey must not be empty".to_owned(),
        ));
    }
    let base_url_len = server_url.trim_end_matches('/').len();
    server_url.truncate(base_url_len);
    let mut config = ClientConfig {
        http: HttpConfig {
            base_url: server_url,
            ..HttpConfig::default()
        },
        api_key: Some(SecretString::from(api_key)),
        ..ClientConfig::default()
    };
    if let Some(grpc_url) = grpc_url.filter(|value| !value.trim().is_empty()) {
        config.grpc.endpoint = grpc_url;
    }
    let scope = ClientScope::from_config(&config).map_err(napi_error)?;
    let client = WyrdClient::with_config(config).map_err(napi_error)?;
    let transport = BifrostGrpcTransport::connect(&client)
        .await
        .map_err(napi_error)?;
    Ok(NativeBifrost {
        handle: Arc::new(vala_sdk::Bifrost::new(
            scope,
            Arc::new(BifrostIngestSink::new(Arc::new(transport))),
            QueueConfig::default(),
        )),
    })
}

#[napi]
impl NativeBifrost {
    /// Explicit write path: enqueue one JSON row and propagate backpressure.
    ///
    /// Stays synchronous because the producer push is a bounded, non-blocking
    /// queue operation; only the drain needs a worker thread.
    ///
    /// # Errors
    ///
    /// Returns a napi error for a malformed JSON schema, an unsupported schema
    /// node, an invalid card reference, or a full or draining producer queue.
    // justification: napi boundary; a JavaScript string is primitive and cannot
    // be passed by reference, so the generated binding requires an owned String
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn insert(
        &self,
        table: String,
        schema: String,
        row: String,
        card_ref: String,
        run_id: Option<String>,
    ) -> napi::Result<()> {
        let inputs = WriteInputs::parse(&schema, &card_ref, run_id)?;
        self.handle
            .insert(
                SinkKind::Record,
                &table,
                &inputs.schema,
                row.into_bytes(),
                inputs.card_ref,
                inputs.run_id,
            )
            .map_err(napi_error)
    }

    /// Fire-and-forget observe path: enqueue one row, counting saturation.
    ///
    /// Queue saturation is deliberately swallowed and surfaced through
    /// [`Self::dropped`] rather than raised, so telemetry never fails a caller.
    ///
    /// # Errors
    ///
    /// Returns a napi error only for a malformed JSON schema or card reference,
    /// which are caller mistakes rather than backpressure.
    // justification: napi boundary; a JavaScript string is primitive and cannot
    // be passed by reference, so the generated binding requires an owned String
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn record(
        &self,
        table: String,
        schema: String,
        row: String,
        card_ref: String,
        run_id: Option<String>,
    ) -> napi::Result<()> {
        let inputs = WriteInputs::parse(&schema, &card_ref, run_id)?;
        vala_sdk::observe::record(
            &self.handle,
            SinkKind::Record,
            &table,
            &inputs.schema,
            row.into_bytes(),
            inputs.card_ref,
            inputs.run_id,
        );
        Ok(())
    }

    /// Flush every pooled producer and await each durable acknowledgement.
    ///
    /// The native drain blocks, so it runs on a blocking worker; awaiting it
    /// keeps the Node event loop free. The Python binding releases the GIL for
    /// the same reason.
    ///
    /// # Errors
    ///
    /// Returns a napi error when a batch cannot be sealed or the server refuses
    /// it, preserving the stable error text at the boundary.
    ///
    /// # Panics
    ///
    /// Panics when the blocking drain task cannot be joined.
    #[napi]
    pub async fn flush(&self) -> napi::Result<()> {
        let handle = Arc::clone(&self.handle);
        tokio::task::spawn_blocking(move || handle.flush())
            .await
            .expect("bifrost flush task joins")
            .map_err(napi_error)
    }

    /// Drain queued rows and stop every producer background task.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the terminal drain cannot complete or a
    /// server acknowledgement reports a write failure.
    ///
    /// # Panics
    ///
    /// Panics when the blocking drain task cannot be joined.
    #[napi]
    pub async fn shutdown(&self) -> napi::Result<()> {
        let handle = Arc::clone(&self.handle);
        tokio::task::spawn_blocking(move || handle.shutdown())
            .await
            .expect("bifrost shutdown task joins")
            .map_err(napi_error)
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[napi(getter)]
    pub fn dropped(&self) -> i64 {
        i64::try_from(self.handle.dropped()).unwrap_or(i64::MAX)
    }

    /// Number of distinct producers currently pooled.
    #[napi(getter)]
    pub fn producer_count(&self) -> u32 {
        u32::try_from(self.handle.producer_count()).unwrap_or(u32::MAX)
    }
}

/// The parsed contract values both write paths need before enqueueing a row.
struct WriteInputs {
    /// Arrow schema resolved from the caller's JSON Schema document.
    schema: arrow::datatypes::SchemaRef,
    /// Card the row is correlated to at the ingest Gate.
    card_ref: wyrd_spec::reference::CardRef,
    /// Optional run correlation carried alongside the card.
    run_id: Option<wyrd_spec::vala::ids::RunId>,
}

impl WriteInputs {
    /// Converts the boundary's JSON strings into native contract values.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the schema is not JSON, describes an
    /// unsupported node, or the card reference does not parse.
    fn parse(schema: &str, card_ref: &str, run_id: Option<String>) -> napi::Result<Self> {
        let schema_value: serde_json::Value = serde_json::from_str(schema)
            .map_err(|error| napi::Error::from_reason(format!("invalid JSON schema: {error}")))?;
        Ok(Self {
            schema: schema_from_json_schema(&schema_value).map_err(napi_error)?,
            card_ref: card_ref
                .parse()
                .map_err(|error| napi::Error::from_reason(format!("invalid cardRef: {error}")))?,
            run_id: run_id.map(wyrd_spec::vala::ids::RunId::from_string),
        })
    }
}

/// Parses the native visibility spelling.
///
/// # Errors
///
/// Returns a structured SDK protocol error for an unknown value.
fn parse_visibility(value: &str) -> Result<VisibilityMode, ValaSdkError> {
    match value {
        "published_only" => Ok(VisibilityMode::PublishedOnly),
        "fused" => Ok(VisibilityMode::Fused),
        _ => Err(ValaSdkError::Protocol(
            "visibility must be published_only or fused".to_owned(),
        )),
    }
}

/// Parses the native freshness spelling.
///
/// # Errors
///
/// Returns a structured SDK protocol error for an unknown value.
fn parse_freshness(value: &str) -> Result<FreshnessPolicy, ValaSdkError> {
    match value {
        "strict" => Ok(FreshnessPolicy::Strict),
        "allow_degraded" => Ok(FreshnessPolicy::AllowDegraded),
        _ => Err(ValaSdkError::Protocol(
            "freshness must be strict or allow_degraded".to_owned(),
        )),
    }
}

/// Parses one canonical request ID at the Node boundary.
///
/// # Errors
///
/// Returns the stable public validation error when the value is not a `UUIDv7` request ID.
fn parse_request_id(value: &str) -> Result<RequestId, ValaSdkError> {
    RequestId::parse(value).map_err(|error| {
        ValaSdkError::Transport(WyrdError::Validation {
            message: error.to_string(),
            details: serde_json::json!({"field": "request_id"}),
        })
    })
}

/// Encodes one decoded Rust record batch for TypeScript Apache Arrow.
///
/// # Errors
///
/// Returns a napi error when Arrow IPC encoding fails.
fn encode_batch(batch: &arrow::record_batch::RecordBatch) -> napi::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, batch.schema().as_ref())
        .map_err(napi_error)?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(napi_error)?;
    Ok(bytes)
}

/// Projects an SDK error with its stable code intact.
fn sdk_error(error: &ValaSdkError) -> napi::Error {
    napi::Error::from_reason(format!("[{}] {error}", error.code()))
}

/// Converts an arbitrary boundary error into a napi failure.
fn napi_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::api::{
        QueryErrorDetail, QueryExecutionPath, QueryFreshness, QuerySource, QueryTerminalError,
        QueryTerminalErrorCode, QueryTerminalFrame, QueryTerminalOutcome, SourceCompletion,
        SourceCompletionOutcome,
    };
    use wyrd_spec::vala::error::BifrostError;

    use super::*;

    /// Malformed lifecycle identifiers remain caller validation failures.
    #[test]
    fn lifecycle_request_id_validation_is_public_and_stable() {
        let error = parse_request_id("not-a-request-id").expect_err("malformed ID must fail");
        assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION");
        assert_eq!(error.status(), 400);
    }

    /// Verifies one SDK error is copied into independent native metadata fields.
    fn assert_start_failure(error: &ValaSdkError) {
        let code = error.code().to_owned();
        let status = u32::from(error.status());
        let title = error.title().to_owned();
        let detail = error.detail();
        let remediation = error.remediation().to_owned();
        let start = NativeQueryStart::failure(error);

        assert!(start.stream.is_none());
        assert_eq!(start.error_code.as_deref(), Some(code.as_str()));
        assert_eq!(start.error_status, Some(status));
        assert_eq!(start.error_title.as_deref(), Some(title.as_str()));
        assert_eq!(start.error_detail.as_deref(), Some(detail.as_str()));
        assert_eq!(
            start.error_remediation.as_deref(),
            Some(remediation.as_str())
        );
        assert_eq!(
            start.error_details_json,
            error
                .safe_details()
                .and_then(|value| serde_json::to_string(&value).ok())
        );
    }

    /// Gate, auth, transport, and request failures retain structured startup metadata.
    #[test]
    fn bifrost_query_native_start_preserves_structured_error_metadata() {
        assert_start_failure(&ValaSdkError::Transport(WyrdError::PermissionDeniedRbac {
            message: "principal lacks bifrost_query:read".to_owned(),
            details: serde_json::json!({}),
        }));
        assert_start_failure(&ValaSdkError::Transport(
            WyrdError::PermissionUnauthenticated {
                message: "access token is invalid".to_owned(),
                details: serde_json::json!({}),
            },
        ));
        assert_start_failure(&ValaSdkError::Transport(WyrdError::ServiceUnavailable {
            message: "query transport is unavailable".to_owned(),
            details: serde_json::json!({}),
        }));
        assert_start_failure(&ValaSdkError::Transport(WyrdError::from(
            BifrostError::QueryInvalidSql {
                detail: "query request failed validation".to_owned(),
            },
        )));
    }

    /// Failed terminals retain diagnostics and drop their native owner before return.
    #[tokio::test]
    async fn native_owner_failed_terminal_drops_before_error_and_closes() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let terminal = QueryTerminalFrame {
            execution_path: QueryExecutionPath::Interactive,
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            row_count: 0,
            warnings: Vec::new(),
            source_completion: vec![
                SourceCompletion {
                    source: QuerySource::Iceberg,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::HotSealed,
                    outcome: SourceCompletionOutcome::Complete,
                },
            ],
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: Some(QueryErrorDetail::new("native sentinel failure").expect("detail")),
            }),
            // A failed stream never calls `finish`, so it has no end-of-stream.
            arrow_ipc_eos: Vec::new(),
        };
        let owner = NativeBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Err(ValaSdkError::FailedTerminal {
                terminal: terminal.clone(),
            })]),
            Some(terminal.clone()),
            Arc::clone(&dropped),
            Arc::clone(&polls),
        ));
        let step = owner.next().await.expect("failed terminal is projected");
        assert_eq!(
            step.error_code.as_deref(),
            Some("WYRD_VALA_500_QUERY_EXECUTION_FAILED")
        );
        assert!(step.error_detail.is_some());
        assert_eq!(
            step.error_details_json
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
                .transpose()
                .expect("details JSON parses"),
            Some(serde_json::to_value(&terminal).expect("terminal serializes"))
        );
        assert!(
            owner
                .terminal_json()
                .expect("terminal lock remains healthy")
                .expect("terminal diagnostics retained")
                .contains("native sentinel failure")
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        let Err(closed) = owner.next().await else {
            panic!("second poll unexpectedly yielded a step")
        };
        assert!(closed.reason.contains("query stream is closed"));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// Protocol, Arrow, EOF, and source errors all release ownership before projection.
    #[tokio::test]
    async fn native_owner_projection_errors_drop_before_error_and_close() {
        for (name, result) in [
            (
                "protocol",
                Err(ValaSdkError::Protocol("malformed".to_owned())),
            ),
            (
                "arrow",
                Err(ValaSdkError::Arrow("malformed Arrow".to_owned())),
            ),
        ] {
            let dropped = Arc::new(AtomicBool::new(false));
            let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let owner = NativeBifrostQueryStream::new_for_test(TestStreamOwner::new(
                std::collections::VecDeque::from([result]),
                None,
                Arc::clone(&dropped),
                Arc::clone(&polls),
            ));
            let step = owner.next().await.expect("structured projection error");
            assert_eq!(
                step.error_code.as_deref(),
                Some("WYRD_VALA_502_QUERY_STREAM_PROTOCOL"),
                "{name} is coded"
            );
            assert!(dropped.load(Ordering::SeqCst), "{name} owner dropped");
            assert_eq!(polls.load(Ordering::SeqCst), 1, "{name} polled once");
            assert!(owner.next().await.is_err(), "{name} second poll closes");
            assert_eq!(polls.load(Ordering::SeqCst), 1, "{name} does not re-poll");
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = NativeBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Ok(None)]),
            None,
            Arc::clone(&dropped),
            Arc::clone(&polls),
        ));
        let Err(error) = owner.next().await else {
            panic!("EOF without terminal unexpectedly succeeded")
        };
        assert!(
            error
                .reason
                .contains("WYRD_VALA_502_QUERY_STREAM_INCOMPLETE")
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(owner.next().await.is_err(), "EOF second poll closes");
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// A source transport error is projected with its code after owner cleanup.
    #[tokio::test]
    async fn native_owner_transport_error_drops_before_error_and_closes() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = NativeBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Err(ValaSdkError::Transport(
                WyrdError::ServiceUnavailable {
                    message: "body transport failed".to_owned(),
                    details: serde_json::json!({}),
                },
            ))]),
            None,
            Arc::clone(&dropped),
            Arc::clone(&polls),
        ));
        let step = owner.next().await.expect("transport error is structured");
        assert_eq!(
            step.error_code.as_deref(),
            Some("WYRD_SERVER_503_SERVICE_UNAVAILABLE")
        );
        assert_eq!(step.error_status, Some(503));
        assert_eq!(step.error_details_json.as_deref(), Some("{}"));
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(
            owner.next().await.is_err(),
            "second transport poll is closed"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// Poisoned terminal metadata storage still releases the stream before failure.
    #[tokio::test]
    async fn native_owner_terminal_lock_failure_drops_before_error_and_closes() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = NativeBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Ok(None)]),
            Some(QueryTerminalFrame {
                execution_path: QueryExecutionPath::Interactive,
                outcome: QueryTerminalOutcome::Success,
                freshness: QueryFreshness::Complete,
                row_count: 0,
                warnings: Vec::new(),
                source_completion: vec![
                    SourceCompletion {
                        source: QuerySource::Iceberg,
                        outcome: SourceCompletionOutcome::Complete,
                    },
                    SourceCompletion {
                        source: QuerySource::HotSealed,
                        outcome: SourceCompletionOutcome::Complete,
                    },
                ],
                error: None,
                arrow_ipc_eos: vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0],
            }),
            Arc::clone(&dropped),
            Arc::clone(&polls),
        ));
        let terminal_lock = Arc::clone(&owner.terminal_json);
        std::thread::spawn(move || {
            let _guard = terminal_lock.lock().expect("terminal lock acquires");
            panic!("poison terminal lock for owner test");
        })
        .join()
        .expect_err("poisoning thread panics");
        let Err(error) = owner.next().await else {
            panic!("terminal lock failure unexpectedly yielded a step")
        };
        assert!(error.reason.contains("terminal lock poisoned"));
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(owner.next().await.is_err(), "second poll is closed");
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }
}
