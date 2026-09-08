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
use wyrd_queue::{ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};

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

    /// Copies a borrowed external frame into the owned Rust transport boundary.
    ///
    /// This is the only ordinary-Rust borrowed-byte copy boundary. Queue-owned
    /// [`Bytes`] frames bypass it and are sent by reference-counted ownership.
    ///
    /// # Errors
    ///
    /// Returns a stable payload error when the borrowed frame exceeds the
    /// accepted Bifrost frame ceiling.
    pub fn copy_external_frame(&self, frame: &[u8]) -> Result<Bytes, WyrdError> {
        if frame.len() > MAX_FRAME_BYTES {
            return Err(WyrdError::PayloadTooLarge {
                message: format!("bifrost batch exceeds {MAX_FRAME_BYTES} Arrow IPC bytes"),
                details: serde_json::json!({ "field": "arrow_ipc", "actual_bytes": frame.len(), "limit_bytes": MAX_FRAME_BYTES }),
            });
        }
        Ok(Bytes::copy_from_slice(frame))
    }

    /// Sends one externally supplied batch through the unary ingest RPC.
    ///
    /// External callers cross [`Self::copy_external_frame`] before the internal
    /// owned-byte path; queue-owned frames use [`IngestTransport`] directly.
    ///
    /// The batch identity is minted here rather than supplied. It is the
    /// server's idempotency key, and the only retries that can observe it are
    /// the ones [`Self::send_owned_bytes`] performs against this single call,
    /// which already reuse one identity. Returning it lets a caller correlate
    /// the durable batch without having to mint a `UUIDv7` of its own.
    ///
    /// # Errors
    ///
    /// Returns transport, validation, or terminal server errors. A retryable
    /// call retries its stable identity inside this transport.
    pub async fn insert_batch(&self, table: &str, arrow_ipc: Vec<u8>) -> Result<Uuid, WyrdError> {
        let batch_id = Uuid::now_v7();
        validate_frame(table, arrow_ipc.len())?;
        let bytes = self.copy_external_frame(&arrow_ipc)?;
        self.send_owned_bytes(table, batch_id.into_bytes(), bytes)
            .await?;
        Ok(batch_id)
    }

    /// Sends one sealed batch through the unary ingest RPC.
    pub async fn send_frame(&self, frame: BifrostFrame) -> Result<(), WyrdError> {
        validate_frame(&frame.table, frame.arrow_ipc.len())?;
        validate_batch_id(frame.batch_id)?;
        self.send_owned_bytes(&frame.table, frame.batch_id, frame.arrow_ipc)
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

    /// Retries an internally owned frame without copying its payload bytes.
    async fn send_owned_bytes(
        &self,
        table: &str,
        batch_id: [u8; 16],
        arrow_ipc: Bytes,
    ) -> Result<(), WyrdError> {
        let mut retry_number = 0_u32;
        loop {
            let request = InsertBatchRequest {
                table: table.to_owned(),
                arrow_ipc: arrow_ipc.clone(),
                wyrd_batch_id: Bytes::copy_from_slice(&batch_id),
            };
            match self.send_once(request).await {
                Ok(()) => return Ok(()),
                Err(error) if error.retryable && retry_number < self.config.max_frame_retries => {
                    tokio::time::sleep(retry_delay(retry_number)).await;
                    retry_number = retry_number.saturating_add(1);
                }
                Err(error) => return Err(error.error),
            }
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
impl IngestTransport<ClientByteGuard> for BifrostGrpcTransport {
    /// Borrows a sealed owner and shares its frame with gRPC without copying.
    ///
    /// The queue's `Arc<Vec<u8>>` shell becomes an owned [`Bytes`] transport
    /// frame through `Bytes::from_owner` without copying. The queue keeps its
    /// guard and stable UUIDv7 while this cancellable borrowed attempt runs.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Retryable`] for ambiguous transport unavailability
    /// and for stable `WYRD_VALA_429_INGEST_BUSY`, whose no-write contract
    /// requires retrying the exact borrowed batch owner. Returns
    /// [`SinkError::Terminal`] for frame validation and every other server
    /// refusal, including permanent `RESOURCE_EXHAUSTED` identities.
    /// Cancellation before an ACK leaves the queue's borrowed UUID, bytes, and
    /// permit intact for retry resolution.
    async fn insert_batch(
        &self,
        batch: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError> {
        if let Err(error) = validate_frame(&batch.table, batch.bytes().len())
            .and_then(|()| validate_batch_id(batch.batch_id))
        {
            return Err(SinkError::Terminal(error));
        }
        let bytes = Bytes::from_owner(batch.frame.shared_bytes());
        let result = self
            .send_owned_bytes(&batch.table, batch.batch_id, bytes)
            .await;
        match result {
            Ok(()) => Ok(DurableBatchAck {
                batch_id: batch.batch_id,
                rows: batch.rows,
            }),
            Err(error @ WyrdError::ServiceUnavailable { .. })
            | Err(
                error @ WyrdError::Vala {
                    error: wyrd_spec::vala::error::BifrostError::IngestBusy { .. },
                },
            ) => Err(SinkError::Retryable(error)),
            Err(error) => Err(SinkError::Terminal(error)),
        }
    }
}

#[derive(Debug)]
struct AttemptError {
    /// Typed stable error returned to the caller when retries stop.
    error: WyrdError,
    /// Whether this attempt is ambiguous or carries stable ingest-busy identity.
    retryable: bool,
}

impl AttemptError {
    /// Wraps a failure that must settle without another transport attempt.
    fn terminal(error: WyrdError) -> Self {
        Self {
            error,
            retryable: false,
        }
    }

    /// Classifies a gRPC failure by stable reason before considering transport loss.
    fn from_status(status: Status) -> Self {
        let stable = from_grpc_status(&status);
        if stable.code() == "WYRD_VALA_429_INGEST_BUSY" {
            Self {
                error: stable,
                retryable: true,
            }
        } else if retryable_status(&status) {
            Self {
                error: transport_unavailable("unary ingest transport failed before ACK"),
                retryable: true,
            }
        } else {
            Self::terminal(stable)
        }
    }
}

fn validate_frame(table: &str, arrow_bytes: usize) -> Result<(), WyrdError> {
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
    Ok(())
}

/// Refuses a queue-owned frame identity that is not a `UUIDv7`.
///
/// Only [`GrpcBifrostTransport::send_frame`] needs this: its identity is
/// supplied by the producer that sealed the frame, while
/// [`GrpcBifrostTransport::insert_batch`] mints its own and cannot be wrong.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when `batch_id` is not a `UUIDv7`.
fn validate_batch_id(batch_id: [u8; 16]) -> Result<(), WyrdError> {
    if Uuid::from_bytes(batch_id).get_version() != Some(Version::SortRand) {
        return Err(WyrdError::Validation {
            message: "bifrost batch_id must be UUIDv7".to_owned(),
            details: serde_json::json!({ "field": "wyrd_batch_id" }),
        });
    }
    Ok(())
}

/// Returns whether a status represents ambiguous transport loss before ACK.
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use secrecy::SecretString;
    use tokio::sync::Mutex;
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::HttpTransport;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_tonic::tonic::Response;
    use wyrd_tonic::tonic::transport::Server;
    use wyrd_tonic::wyrd::v1::InsertBatchResponse;
    use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
        BifrostIngestService, BifrostIngestServiceServer,
    };

    /// Scripted real gRPC service that returns busy, ACK, then permanent capacity.
    #[derive(Clone, Default)]
    struct ScriptedIngest {
        /// Ordered stable batch identities observed at the RPC boundary.
        batch_ids: Arc<Mutex<Vec<Vec<u8>>>>,
        /// Selects the next scripted response without serializing request recording.
        attempts: Arc<AtomicUsize>,
    }

    #[wyrd_tonic::tonic::async_trait]
    impl BifrostIngestService for ScriptedIngest {
        /// Returns stable busy once, acknowledges its retry, then refuses permanently.
        ///
        /// # Errors
        ///
        /// Returns scripted stable busy and WAL-capacity statuses so the real
        /// client transport proves its retry and terminal classification.
        async fn insert_batch(
            &self,
            request: Request<InsertBatchRequest>,
        ) -> Result<Response<InsertBatchResponse>, Status> {
            let request = request.into_inner();
            self.batch_ids
                .lock()
                .await
                .push(request.wyrd_batch_id.to_vec());
            match self.attempts.fetch_add(1, Ordering::AcqRel) {
                0 => {
                    use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt};
                    let mut status = Status::with_error_details(
                        Code::ResourceExhausted,
                        "ingest coordinator busy for table events — local buffer full",
                        ErrorDetails::with_error_info(
                            "WYRD_VALA_429_INGEST_BUSY",
                            "wyrd.dev",
                            [] as [(String, String); 0],
                        ),
                    );
                    status
                        .metadata_mut()
                        .insert("retry-after-ms", MetadataValue::from_static("1000"));
                    Err(status)
                }
                1 => Ok(Response::new(InsertBatchResponse {
                    wyrd_batch_id: request.wyrd_batch_id,
                })),
                _ => {
                    use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt};
                    Err(Status::with_error_details(
                        Code::ResourceExhausted,
                        "ingest WAL storage is unavailable",
                        ErrorDetails::with_error_info(
                            "WYRD_VALA_507_WAL_DISK_FULL",
                            "wyrd.dev",
                            [] as [(String, String); 0],
                        ),
                    ))
                }
            }
        }
    }

    #[test]
    fn frame_limit_is_enforced() {
        assert!(validate_frame("events", MAX_FRAME_BYTES).is_ok());
        let error = validate_frame("events", MAX_FRAME_BYTES + 1)
            .expect_err("one byte above the frame limit must fail");
        assert_eq!(error.code(), "WYRD_SPEC_413_PAYLOAD_TOO_LARGE");
    }

    #[test]
    fn invalid_batch_id_is_rejected_before_transport() {
        let error = validate_batch_id([0; 16]).expect_err("non-v7 ID must fail");
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

    /// Stable busy retries while every other capacity status terminalizes.
    #[tokio::test]
    async fn ingest_busy_retries_same_batch_and_permanent_capacity_is_terminal() {
        use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt};

        let service = ScriptedIngest::default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test address resolves");
        drop(listener);
        let server = tokio::spawn(
            Server::builder()
                .add_service(BifrostIngestServiceServer::new(service.clone()))
                .serve(address),
        );
        tokio::task::yield_now().await;

        let config = ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: format!("http://{address}"),
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from("test-token")),
        )
        .expect("test auth builds");
        let http = HttpTransport::new(&config.http, auth.clone()).expect("test HTTP layer builds");
        let client = WyrdClient::from_parts(auth, http, config.grpc);
        let transport = BifrostGrpcTransport::connect_with_config(
            &client,
            BifrostTransportConfig::with_max_frame_retries(3),
        )
        .await
        .expect("real test gRPC transport connects");
        let batch_id = transport
            .insert_batch("events", vec![1, 2, 3])
            .await
            .expect("stable busy retries to concrete ACK")
            .into_bytes();
        let observed = service.batch_ids.lock().await.clone();
        assert_eq!(observed, vec![batch_id.to_vec(), batch_id.to_vec()]);

        let terminal = transport
            .insert_batch("events", vec![4])
            .await
            .expect_err("permanent capacity terminalizes without retry");
        assert_ne!(terminal.code(), "WYRD_VALA_429_INGEST_BUSY");
        assert_eq!(service.attempts.load(Ordering::Acquire), 3);
        server.abort();

        // Pin the classifier independently so malformed capacity identities
        // cannot become transport retries if the scripted server changes.
        let busy = Status::with_error_details(
            Code::ResourceExhausted,
            "ingest coordinator busy for table events — local buffer full",
            ErrorDetails::with_error_info(
                "WYRD_VALA_429_INGEST_BUSY",
                "wyrd.dev",
                [] as [(String, String); 0],
            ),
        );
        assert!(AttemptError::from_status(busy).retryable);

        for reason in [
            "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
            "WYRD_VALA_413_INGEST_OVERSIZED",
            "WYRD_VALA_507_WAL_DISK_FULL",
        ] {
            let status = Status::with_error_details(
                Code::ResourceExhausted,
                "permanent capacity refusal",
                ErrorDetails::with_error_info(reason, "wyrd.dev", [] as [(String, String); 0]),
            );
            assert!(!AttemptError::from_status(status).retryable, "{reason}");
        }
    }
}
