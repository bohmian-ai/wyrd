//! Thin napi projection of Vala's Rust-owned Oracle query client.

#![deny(missing_docs)]

use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use secrecy::SecretString;
use tokio::sync::Mutex as AsyncMutex;
use vala_sdk::{QueryClient, QueryResultStream, ValaSdkError};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};

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
        }
    }

    /// Builds the failed side with stable metadata retained as independent fields.
    fn failure(error: ValaSdkError) -> Self {
        Self {
            stream: None,
            error_code: Some(error.code().to_owned()),
            error_status: Some(u32::from(error.status())),
            error_title: Some(error.title().to_owned()),
            error_detail: Some(error.to_string()),
            error_remediation: Some(error.remediation().to_owned()),
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
    pub fn new(server_url: String, token: String) -> napi::Result<Self> {
        if server_url.trim().is_empty() || token.is_empty() {
            return Err(napi::Error::from_reason(
                "serverUrl and token must not be empty".to_owned(),
            ));
        }
        let config = ClientConfig {
            http: HttpConfig {
                base_url: server_url.trim_end_matches('/').to_owned(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
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
                Err(error) => return Ok(NativeQueryStart::failure(error)),
            },
            freshness: match parse_freshness(&request.freshness) {
                Ok(freshness) => freshness,
                Err(error) => return Ok(NativeQueryStart::failure(error)),
            },
            deadline_ms: request.deadline_ms.map(u64::from),
        };
        Ok(match self.client.query(&request).await {
            Ok(stream) => NativeQueryStart::success(stream),
            Err(error) => NativeQueryStart::failure(error),
        })
    }
}

impl NativeBifrostQueryStream {
    /// Wraps one Rust-owned query stream for napi iteration.
    fn new(stream: QueryResultStream) -> Self {
        Self {
            stream: Arc::new(AsyncMutex::new(Some(stream))),
            terminal_json: Arc::new(Mutex::new(None)),
        }
    }
}

/// Native query stream that retains Rust terminal validation and emits raw IPC.
#[napi]
pub struct NativeBifrostQueryStream {
    /// Mutable Rust query stream serialized across JavaScript `next` calls.
    stream: Arc<AsyncMutex<Option<QueryResultStream>>>,
    /// Validated serialized terminal retained after the Rust stream is released.
    terminal_json: Arc<Mutex<Option<String>>>,
}

#[napi]
impl NativeBifrostQueryStream {
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
            Ok(Some(batch)) => Ok(NativeQueryStep {
                ipc: Some(Buffer::from(encode_batch(&batch)?)),
                terminal_json: None,
                error_code: None,
                error_status: None,
                error_title: None,
                error_detail: None,
                error_remediation: None,
            }),
            Ok(None) => {
                let terminal = serde_json::to_string(
                    stream
                        .terminal()
                        .ok_or_else(|| sdk_error(ValaSdkError::IncompleteQueryStream))?,
                )
                .map_err(napi_error)?;
                *self
                    .terminal_json
                    .lock()
                    .map_err(|_| napi::Error::from_reason("terminal lock poisoned".to_owned()))? =
                    Some(terminal.clone());
                *stream_slot = None;
                Ok(NativeQueryStep {
                    ipc: None,
                    terminal_json: Some(terminal),
                    error_code: None,
                    error_status: None,
                    error_title: None,
                    error_detail: None,
                    error_remediation: None,
                })
            }
            Err(error) => {
                if let Some(terminal) = stream.terminal() {
                    let terminal = serde_json::to_string(terminal).map_err(napi_error)?;
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
                    error_detail: Some(error.to_string()),
                    error_remediation: Some(error.remediation().to_owned()),
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
fn sdk_error(error: ValaSdkError) -> napi::Error {
    napi::Error::from_reason(format!("[{}] {error}", error.code()))
}

/// Converts an arbitrary boundary error into a napi failure.
fn napi_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

#[cfg(test)]
mod tests {
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::error::BifrostError;

    use super::*;

    /// Verifies one SDK error is copied into independent native metadata fields.
    fn assert_start_failure(error: ValaSdkError) {
        let code = error.code().to_owned();
        let status = u32::from(error.status());
        let title = error.title().to_owned();
        let detail = error.to_string();
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
    }

    /// Gate, auth, transport, and request failures retain structured startup metadata.
    #[test]
    fn bifrost_query_native_start_preserves_structured_error_metadata() {
        assert_start_failure(ValaSdkError::Transport(WyrdError::PermissionDeniedRbac {
            message: "principal lacks bifrost_query:read".to_owned(),
            details: serde_json::json!({}),
        }));
        assert_start_failure(ValaSdkError::Transport(
            WyrdError::PermissionUnauthenticated {
                message: "access token is invalid".to_owned(),
                details: serde_json::json!({}),
            },
        ));
        assert_start_failure(ValaSdkError::Transport(WyrdError::ServiceUnavailable {
            message: "query transport is unavailable".to_owned(),
            details: serde_json::json!({}),
        }));
        assert_start_failure(ValaSdkError::Transport(WyrdError::from(
            BifrostError::QueryInvalidSql {
                detail: "query request failed validation".to_owned(),
            },
        )));
    }
}
