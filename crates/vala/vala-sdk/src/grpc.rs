//! Concrete bidirectional Bifrost ingest transport.
//!
//! The transport owns only wire delivery. The server remains responsible for
//! authentication decisions, validation, admission, deduplication, and
//! durability. A send attempt retains one original frame, sends it through a
//! bounded request stream, and returns only after the matching response ACK is
//! consumed. A disconnected stream may therefore retry the same frame without
//! regenerating its batch ID or sequence.

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::{Uuid, Version};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthError;
use wyrd_client::error::{WyrdClientError, from_grpc_status};
use wyrd_client::transport::GrpcConnection;
use wyrd_spec::error::WyrdError;
use wyrd_tonic::tonic::{Code, Request, Status, metadata::MetadataValue};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

use crate::sink::IngestTransport;

/// Maximum Arrow IPC payload for one Bifrost frame after decompression.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// Protobuf and gRPC framing allowance added to the Arrow payload ceiling.
pub const PROTO_FRAME_OVERHEAD_BYTES: usize = 4 * 1024;

/// Maximum number of reconnect retries for one unacknowledged frame.
pub const MAX_FRAME_RETRIES: u32 = 8;

const REQUEST_STREAM_CAPACITY: usize = 1;
const RETRY_BACKOFF_MS: [u64; 3] = [100, 1_000, 5_000];

/// Bifrost transport retry configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostTransportConfig {
    /// Number of reconnect attempts after the initial frame attempt.
    pub max_frame_retries: u32,
}

/// One immutable frame submitted by [`BifrostGrpcTransport::insert_batch_stream`].
#[derive(Debug, Clone)]
pub struct BifrostFrame {
    /// Logical table FQN repeated on every frame.
    pub table: String,
    /// UUIDv7 batch identity retained across retries.
    pub batch_id: [u8; 16],
    /// Zero-based contiguous stream sequence.
    pub frame_sequence: u64,
    /// Arrow IPC payload for this frame.
    pub arrow_ipc: Vec<u8>,
}

impl Default for BifrostTransportConfig {
    fn default() -> Self {
        Self {
            max_frame_retries: 3,
        }
    }
}

impl BifrostTransportConfig {
    /// Build configuration with a bounded retry budget.
    #[must_use]
    pub fn with_max_frame_retries(max_frame_retries: u32) -> Self {
        Self {
            max_frame_retries: max_frame_retries.min(MAX_FRAME_RETRIES),
        }
    }
}

/// Authenticated bidi gRPC producer/consumer for Bifrost frames.
///
/// The channel is multiplexed and reused across attempts. Each attempt uses a
/// fresh bounded request stream because a tonic stream cannot be rewound after
/// a disconnect. The frame held by the retry loop is the original immutable
/// identity and payload; no retry gets a new ID or sequence.
#[derive(Clone)]
pub struct BifrostGrpcTransport {
    connection: GrpcConnection,
    config: BifrostTransportConfig,
}

impl std::fmt::Debug for BifrostGrpcTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BifrostGrpcTransport")
            .field("connection", &self.connection)
            .field("config", &self.config)
            .finish()
    }
}

impl BifrostGrpcTransport {
    /// Connect a Bifrost transport through the shared Wyrd client auth path.
    ///
    /// The returned transport reuses the client's gRPC channel and token cache;
    /// it does not create a second credential or decode the access token.
    ///
    /// # Errors
    /// Returns [`WyrdClientError`] when the shared gRPC connection cannot be
    /// established after its configured connection attempts.
    pub async fn connect(client: &WyrdClient) -> Result<Self, WyrdClientError> {
        Self::connect_with_config(client, BifrostTransportConfig::default()).await
    }

    /// Connect a Bifrost transport with an explicit bounded retry budget.
    ///
    /// # Errors
    /// Returns [`WyrdClientError`] when the shared gRPC connection cannot be
    /// established after its configured connection attempts.
    pub async fn connect_with_config(
        client: &WyrdClient,
        config: BifrostTransportConfig,
    ) -> Result<Self, WyrdClientError> {
        let connection = client.connect_grpc().await?;
        Ok(Self::new(connection, config))
    }

    /// Build a transport over an already connected, authenticated channel.
    #[must_use]
    pub fn new(connection: GrpcConnection, config: BifrostTransportConfig) -> Self {
        Self { connection, config }
    }

    /// Return the transport retry configuration.
    #[must_use]
    pub fn config(&self) -> BifrostTransportConfig {
        self.config
    }

    /// Send a bounded sequence of frames and consume each ACK before sending
    /// the next frame. The request stream remains open while ACKs arrive, so a
    /// caller can observe per-frame admission before half-close.
    ///
    /// # Errors
    /// Returns the first transport, validation, authentication, or server
    /// error. Frames already acknowledged remain valid and may be retried by
    /// the caller starting at the first unacknowledged sequence.
    pub async fn insert_batch_stream(
        &self,
        frames: Vec<BifrostFrame>,
    ) -> Result<Vec<u64>, WyrdError> {
        if frames.is_empty() {
            return Ok(Vec::new());
        }
        let (sender, receiver) = mpsc::channel(REQUEST_STREAM_CAPACITY);
        let mut request = Request::new(ReceiverStream::new(receiver));
        self.add_auth_metadata(&mut request).await?;
        let max_message_bytes = MAX_FRAME_BYTES + PROTO_FRAME_OVERHEAD_BYTES;
        let mut client = BifrostIngestServiceClient::new(self.connection.channel())
            .max_decoding_message_size(max_message_bytes)
            .max_encoding_message_size(max_message_bytes);
        let response_fut = client.insert_batch(request);
        tokio::pin!(response_fut);
        let first = frames.first().expect("non-empty frame stream");
        validate_frame(&first.table, first.batch_id, first.arrow_ipc.len())?;
        let first_request = InsertBatchRequest {
            table: first.table.clone(),
            arrow_ipc: first.arrow_ipc.clone(),
            wyrd_batch_id: first.batch_id.to_vec(),
            frame_sequence: first.frame_sequence,
        };
        sender.send(first_request.clone()).await.map_err(|_| {
            transport_unavailable(
                "request stream closed before frame send",
                first.frame_sequence,
            )
        })?;
        let response = response_fut
            .await
            .map_err(|status| from_grpc_status(&status))?;
        let mut responses = response.into_inner();
        let mut accepted = Vec::with_capacity(frames.len());
        let first_ack = responses
            .message()
            .await
            .map_err(|status| from_grpc_status(&status))?
            .ok_or_else(|| {
                transport_unavailable(
                    "ingest response stream ended before ACK",
                    first.frame_sequence,
                )
            })?;
        accepted.push(accepted_rows(&first_request, first_ack)?);

        for frame in frames.into_iter().skip(1) {
            validate_frame(&frame.table, frame.batch_id, frame.arrow_ipc.len())?;
            let request = InsertBatchRequest {
                table: frame.table,
                arrow_ipc: frame.arrow_ipc,
                wyrd_batch_id: frame.batch_id.to_vec(),
                frame_sequence: frame.frame_sequence,
            };
            sender.send(request.clone()).await.map_err(|_| {
                transport_unavailable(
                    "request stream closed before frame send",
                    frame.frame_sequence,
                )
            })?;
            let ack = responses
                .message()
                .await
                .map_err(|status| from_grpc_status(&status))?
                .ok_or_else(|| {
                    transport_unavailable(
                        "ingest response stream ended before ACK",
                        frame.frame_sequence,
                    )
                })?;
            accepted.push(accepted_rows(&request, ack)?);
        }
        drop(sender);
        Ok(accepted)
    }

    async fn send_once(&self, frame: &InsertBatchRequest) -> Result<u64, AttemptError> {
        let (sender, receiver) = mpsc::channel(REQUEST_STREAM_CAPACITY);
        let mut request = Request::new(ReceiverStream::new(receiver));
        self.add_auth_metadata(&mut request)
            .await
            .map_err(AttemptError::terminal)?;

        let max_message_bytes = MAX_FRAME_BYTES + PROTO_FRAME_OVERHEAD_BYTES;
        let mut client = BifrostIngestServiceClient::new(self.connection.channel())
            .max_decoding_message_size(max_message_bytes)
            .max_encoding_message_size(max_message_bytes);
        let response_fut = client.insert_batch(request);
        tokio::pin!(response_fut);

        sender.send(frame.clone()).await.map_err(|_| {
            AttemptError::retryable(transport_unavailable(
                "request stream closed before frame send",
                frame.frame_sequence,
            ))
        })?;
        drop(sender);

        let response = response_fut.await.map_err(AttemptError::from_status)?;

        let mut responses = response.into_inner();
        let Some(ack) = responses
            .message()
            .await
            .map_err(AttemptError::from_status)?
        else {
            return Err(AttemptError::retryable(transport_unavailable(
                "ingest response stream ended before ACK",
                frame.frame_sequence,
            )));
        };

        accepted_rows(frame, ack).map_err(AttemptError::terminal)
    }

    async fn add_auth_metadata<T>(&self, request: &mut Request<T>) -> Result<(), WyrdError> {
        let bearer = self
            .connection
            .auth()
            .bearer()
            .await
            .map_err(auth_error_to_wyrd)?;
        let access_token = format!("Bearer {}", bearer.expose());
        let access_token =
            MetadataValue::try_from(access_token.as_str()).map_err(|_| WyrdError::Internal {
                message: "bifrost access token cannot be represented as gRPC metadata".to_owned(),
                details: serde_json::json!({}),
            })?;
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", access_token);

        let request_id = self.connection.auth().request_id(None);
        let request_id =
            MetadataValue::try_from(request_id.as_str()).map_err(|_| WyrdError::Internal {
                message: "bifrost request ID cannot be represented as gRPC metadata".to_owned(),
                details: serde_json::json!({}),
            })?;
        request.metadata_mut().insert("wyrd-request-id", request_id);
        Ok(())
    }
}

#[async_trait]
impl IngestTransport for BifrostGrpcTransport {
    async fn insert_batch(
        &self,
        table: &str,
        batch_id: [u8; 16],
        frame_sequence: u64,
        arrow_ipc: Vec<u8>,
    ) -> Result<u64, WyrdError> {
        validate_frame(table, batch_id, arrow_ipc.len())?;
        let frame = InsertBatchRequest {
            table: table.to_owned(),
            arrow_ipc,
            wyrd_batch_id: batch_id.to_vec(),
            frame_sequence,
        };

        let mut retry_number = 0_u32;
        loop {
            match self.send_once(&frame).await {
                Ok(rows) => return Ok(rows),
                Err(error) if error.retryable && retry_number < self.config.max_frame_retries => {
                    tracing::debug!(
                        frame_sequence,
                        retry_number,
                        "retrying unacknowledged Bifrost frame"
                    );
                    tokio::time::sleep(retry_delay(retry_number)).await;
                    retry_number = retry_number.saturating_add(1);
                }
                Err(error) => return Err(error.error),
            }
        }
    }
}

#[derive(Debug)]
struct AttemptError {
    error: WyrdError,
    retryable: bool,
}

impl AttemptError {
    fn terminal(error: WyrdError) -> Self {
        Self {
            error,
            retryable: false,
        }
    }

    fn retryable(error: WyrdError) -> Self {
        Self {
            error,
            retryable: true,
        }
    }

    fn from_status(status: Status) -> Self {
        if retryable_status(&status) {
            Self::retryable(transport_unavailable(
                "ingest stream transport failed before ACK",
                0,
            ))
        } else {
            Self::terminal(from_grpc_status(&status))
        }
    }
}

fn validate_frame(table: &str, batch_id: [u8; 16], arrow_bytes: usize) -> Result<(), WyrdError> {
    if table.is_empty() {
        return Err(WyrdError::Validation {
            message: "bifrost frame table must not be empty".to_owned(),
            details: serde_json::json!({ "field": "table" }),
        });
    }
    if arrow_bytes > MAX_FRAME_BYTES {
        return Err(WyrdError::PayloadTooLarge {
            message: format!("bifrost frame exceeds {MAX_FRAME_BYTES} Arrow IPC bytes"),
            details: serde_json::json!({
                "field": "arrow_ipc",
                "actual_bytes": arrow_bytes,
                "limit_bytes": MAX_FRAME_BYTES,
            }),
        });
    }
    let batch_id = Uuid::from_bytes(batch_id);
    if batch_id.get_version() != Some(Version::SortRand) {
        return Err(WyrdError::Validation {
            message: "bifrost frame batch_id must be UUIDv7".to_owned(),
            details: serde_json::json!({ "field": "wyrd_batch_id" }),
        });
    }
    Ok(())
}

fn accepted_rows(frame: &InsertBatchRequest, ack: InsertBatchResponse) -> Result<u64, WyrdError> {
    if ack.wyrd_batch_id != frame.wyrd_batch_id || ack.frame_sequence != frame.frame_sequence {
        return Err(WyrdError::Internal {
            message: "bifrost ACK does not match the submitted frame".to_owned(),
            details: serde_json::json!({
                "expected_frame_sequence": frame.frame_sequence,
                "actual_frame_sequence": ack.frame_sequence,
                "expected_batch_id": hex::encode(&frame.wyrd_batch_id),
                "actual_batch_id": hex::encode(&ack.wyrd_batch_id),
            }),
        });
    }
    Ok(ack.rows_accepted)
}

fn retryable_status(status: &Status) -> bool {
    matches!(
        status.code(),
        Code::Cancelled | Code::DeadlineExceeded | Code::Unavailable | Code::Unknown
    )
}

fn retry_delay(retry_number: u32) -> Duration {
    Duration::from_millis(
        RETRY_BACKOFF_MS
            .get(retry_number as usize)
            .copied()
            .unwrap_or(5_000),
    )
}

fn transport_unavailable(stage: &str, frame_sequence: u64) -> WyrdError {
    WyrdError::ServiceUnavailable {
        message: "bifrost ingest transport unavailable before frame ACK".to_owned(),
        details: serde_json::json!({
            "stage": stage,
            "frame_sequence": frame_sequence,
        }),
    }
}

fn auth_error_to_wyrd(error: AuthError) -> WyrdError {
    match error {
        AuthError::Server(error) => error,
        AuthError::Client(error) => {
            tracing::warn!(error = %error, "bifrost gRPC authentication transport unavailable");
            transport_unavailable("authentication", 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_limit_is_independent_of_stream_retry_state() {
        let batch_id = Uuid::now_v7().into_bytes();
        assert!(validate_frame("events", batch_id, MAX_FRAME_BYTES).is_ok());
        let error = validate_frame("events", batch_id, MAX_FRAME_BYTES + 1)
            .expect_err("one byte above the frame limit must fail");
        assert_eq!(error.code(), "WYRD_SPEC_413_PAYLOAD_TOO_LARGE");
    }

    #[test]
    fn invalid_batch_id_is_rejected_before_transport() {
        let error = validate_frame("events", [0; 16], 0).expect_err("non-v7 ID must fail");
        assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION");
    }

    #[test]
    fn only_transport_loss_statuses_are_retryable() {
        assert!(retryable_status(&Status::new(Code::Unavailable, "down")));
        assert!(retryable_status(&Status::new(
            Code::DeadlineExceeded,
            "late"
        )));
        assert!(!retryable_status(&Status::new(
            Code::InvalidArgument,
            "bad frame"
        )));
        assert!(!retryable_status(&Status::new(
            Code::ResourceExhausted,
            "full"
        )));
    }

    #[test]
    fn mismatched_ack_is_terminal_and_carries_identity_details() {
        let frame = InsertBatchRequest {
            table: "events".to_owned(),
            arrow_ipc: Vec::new(),
            wyrd_batch_id: Uuid::now_v7().as_bytes().to_vec(),
            frame_sequence: 4,
        };
        let ack = InsertBatchResponse {
            wyrd_batch_id: frame.wyrd_batch_id.clone(),
            frame_sequence: 5,
            rows_accepted: 1,
        };
        let error = accepted_rows(&frame, ack).expect_err("wrong sequence must fail");
        assert_eq!(error.code(), "WYRD_SPEC_500_INTERNAL");
        assert!(error.to_string().contains("ACK does not match"));
    }
}
