//! Concrete unary Bifrost ingest transport.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use uuid::{Uuid, Version};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthError;
use wyrd_client::error::{WyrdClientError, from_grpc_status};
use wyrd_client::transport::GrpcConnection;
use wyrd_spec::error::WyrdError;
use wyrd_tonic::tonic::{Code, Request, Status, metadata::MetadataValue};
use wyrd_tonic::wyrd::v1::InsertBatchRequest;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;

use crate::sink::IngestTransport;

/// Maximum Arrow IPC payload for one Bifrost batch after decompression.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// Protobuf and gRPC framing allowance added to the Arrow payload ceiling.
pub const PROTO_FRAME_OVERHEAD_BYTES: usize = 4 * 1024;

/// Maximum number of reconnect retries for one unacknowledged batch.
pub const MAX_FRAME_RETRIES: u32 = 8;

const RETRY_BACKOFF_MS: [u64; 3] = [100, 1_000, 5_000];

/// Bifrost transport retry configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostTransportConfig {
    /// Number of reconnect attempts after the initial batch attempt.
    pub max_frame_retries: u32,
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

/// One immutable batch submitted to the unary ingest RPC.
#[derive(Debug, Clone)]
pub struct BifrostFrame {
    /// Logical table FQN.
    pub table: String,
    /// UUIDv7 batch identity retained across retries.
    pub batch_id: [u8; 16],
    /// Arrow IPC payload for the sealed batch.
    pub arrow_ipc: Bytes,
}

/// Authenticated unary gRPC producer for Bifrost batches.
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
    /// Connect through the shared Wyrd client authentication path.
    pub async fn connect(client: &WyrdClient) -> Result<Self, WyrdClientError> {
        Self::connect_with_config(client, BifrostTransportConfig::default()).await
    }

    /// Connect with an explicit bounded retry budget.
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

    /// Send one sealed batch through the unary ingest RPC.
    pub async fn send_frame(&self, frame: BifrostFrame) -> Result<(), WyrdError> {
        <Self as IngestTransport>::insert_batch(
            self,
            &frame.table,
            frame.batch_id,
            frame.arrow_ipc.to_vec(),
        )
        .await
    }

    /// Send batches sequentially, preserving each batch identity.
    pub async fn send_frames(&self, frames: Vec<BifrostFrame>) -> Result<(), WyrdError> {
        for frame in frames {
            self.send_frame(frame).await?;
        }
        Ok(())
    }

    async fn send_once(&self, request: InsertBatchRequest) -> Result<(), AttemptError> {
        let expected_batch_id = request.wyrd_batch_id.clone();
        let mut request = Request::new(request);
        self.add_auth_metadata(&mut request)
            .await
            .map_err(AttemptError::terminal)?;
        let max_message_bytes = MAX_FRAME_BYTES + PROTO_FRAME_OVERHEAD_BYTES;
        let mut client = BifrostIngestServiceClient::new(self.connection.channel())
            .max_decoding_message_size(max_message_bytes)
            .max_encoding_message_size(max_message_bytes);
        let response = client
            .insert_batch(request)
            .await
            .map_err(AttemptError::from_status)?
            .into_inner();
        if response.wyrd_batch_id == expected_batch_id {
            Ok(())
        } else {
            Err(AttemptError::terminal(WyrdError::Internal {
                message: "bifrost ACK does not match the submitted batch".to_owned(),
                details: serde_json::json!({
                    "expected_batch_id": hex::encode(&expected_batch_id),
                    "actual_batch_id": hex::encode(&response.wyrd_batch_id),
                }),
            }))
        }
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
        arrow_ipc: Vec<u8>,
    ) -> Result<(), WyrdError> {
        validate_frame(table, batch_id, arrow_ipc.len())?;
        let request = InsertBatchRequest {
            table: table.to_owned(),
            arrow_ipc: Bytes::from(arrow_ipc),
            wyrd_batch_id: Bytes::copy_from_slice(&batch_id),
        };
        let mut retry_number = 0_u32;
        loop {
            match self.send_once(request.clone()).await {
                Ok(()) => return Ok(()),
                Err(error) if error.retryable && retry_number < self.config.max_frame_retries => {
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

    fn from_status(status: Status) -> Self {
        if retryable_status(&status) {
            Self {
                error: transport_unavailable("unary ingest transport failed before ACK"),
                retryable: true,
            }
        } else {
            Self::terminal(from_grpc_status(&status))
        }
    }
}

fn validate_frame(table: &str, batch_id: [u8; 16], arrow_bytes: usize) -> Result<(), WyrdError> {
    if table.is_empty() {
        return Err(WyrdError::Validation {
            message: "bifrost batch table must not be empty".to_owned(),
            details: serde_json::json!({ "field": "table" }),
        });
    }
    if arrow_bytes > MAX_FRAME_BYTES {
        return Err(WyrdError::PayloadTooLarge {
            message: format!("bifrost batch exceeds {MAX_FRAME_BYTES} Arrow IPC bytes"),
            details: serde_json::json!({
                "field": "arrow_ipc",
                "actual_bytes": arrow_bytes,
                "limit_bytes": MAX_FRAME_BYTES,
            }),
        });
    }
    if Uuid::from_bytes(batch_id).get_version() != Some(Version::SortRand) {
        return Err(WyrdError::Validation {
            message: "bifrost batch_id must be UUIDv7".to_owned(),
            details: serde_json::json!({ "field": "wyrd_batch_id" }),
        });
    }
    Ok(())
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

fn transport_unavailable(stage: &str) -> WyrdError {
    WyrdError::ServiceUnavailable {
        message: "bifrost ingest transport unavailable before batch ACK".to_owned(),
        details: serde_json::json!({ "stage": stage }),
    }
}

fn auth_error_to_wyrd(error: AuthError) -> WyrdError {
    match error {
        AuthError::Server(error) => error,
        AuthError::Client(error) => {
            tracing::warn!(error = %error, "bifrost gRPC authentication transport unavailable");
            transport_unavailable("authentication")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_limit_is_enforced() {
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
        assert!(!retryable_status(&Status::new(
            Code::InvalidArgument,
            "bad batch"
        )));
    }
}
