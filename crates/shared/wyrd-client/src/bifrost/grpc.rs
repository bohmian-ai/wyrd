//! Concrete unary Bifrost ingest transport.

use std::time::Duration;

use crate::WyrdClient;
use crate::auth::AuthError;
use crate::error::{WyrdClientError, from_grpc_status};
use crate::transport::GrpcConnection;
use async_trait::async_trait;
use bytes::Bytes;
use uuid::{Uuid, Version};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::{Code, Request, Status, metadata::MetadataValue};
use wyrd_tonic::wyrd::v1::InsertBatchRequest;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;

use crate::bifrost::sink::IngestTransport;
use wyrd_queue::{ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};

/// Maximum number of reconnect retries for one unacknowledged batch.
#[cfg(any(test, feature = "test-support"))]
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
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_max_frame_retries(max_frame_retries: u32) -> Self {
        Self {
            max_frame_retries: max_frame_retries.min(MAX_FRAME_RETRIES),
        }
    }
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

    /// Returns the longest one batch can spend inside this transport.
    ///
    /// Every permitted attempt — the initial send, each configured retry, and
    /// the one resend after a forced credential refresh — may run to the
    /// channel's per-call deadline, and every retry first waits its backoff.
    /// Authentication may exchange a token twice: once for the first bearer
    /// and once for the forced refresh. [`IngestTransport::insert_batch`]
    /// enforces this bound over the whole operation, so gate waits and token
    /// exchanges spend the same budget rather than extending it.
    fn attempt_budget(&self) -> Duration {
        let attempts = self.config.max_frame_retries.saturating_add(2);
        let backoff = (0..self.config.max_frame_retries)
            .map(retry_delay)
            .fold(Duration::ZERO, Duration::saturating_add);
        self.connection
            .call_timeout()
            .saturating_mul(attempts)
            .saturating_add(backoff)
            .saturating_add(self.connection.auth().exchange_timeout().saturating_mul(2))
    }

    /// Returns the shortest send deadline a caller may put around
    /// [`IngestTransport::insert_batch`] without cancelling this transport.
    ///
    /// The transport resolves every batch within its attempt budget; one more
    /// per-call deadline of slack lets that result, terminal or retained
    /// ambiguity, reach the caller before the caller's own deadline cancels
    /// the transport mid-budget.
    #[must_use]
    pub(crate) fn send_deadline(&self) -> Duration {
        self.attempt_budget()
            .saturating_add(self.connection.call_timeout())
    }

    /// Sends one authenticated ingest attempt and verifies its ACK identity.
    ///
    /// The attempt carries `request_id` when supplied, otherwise a fresh one.
    ///
    /// # Errors
    ///
    /// Returns a terminal attempt error when credentials or metadata cannot be
    /// attached or the ACK names another batch, and the classified gRPC
    /// refusal otherwise.
    async fn send_once(
        &self,
        request: InsertBatchRequest,
        request_id: Option<&RequestId>,
    ) -> Result<(), AttemptError> {
        let expected_batch_id = request.wyrd_batch_id.clone();
        let mut request = Request::new(request);
        self.add_auth_metadata(&mut request, request_id)
            .await
            .map_err(AttemptError::terminal)?;
        // The server owns the configured ingest wire ceiling and refuses a
        // larger frame with its typed payload-too-large error; tonic's default
        // encoding size is unbounded, so the client sends what it was given.
        let mut client = BifrostIngestServiceClient::new(self.connection.channel());
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
    ///
    /// Transport loss and stable ingest-busy retry within the bounded budget.
    /// An `Unauthenticated` refusal forces one credential refresh through the
    /// shared [`crate::auth::AuthMiddleware`] and resends the same batch once,
    /// so an expired or revoked cached token recovers without a second refresh
    /// owner; a repeated refusal is terminal. Every attempt carries the same
    /// `request_id` when one is supplied.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Retryable`] when the attempts end on an ambiguous
    /// or busy refusal, or after any earlier attempt was ambiguous, because
    /// the server may then hold the batch and only a resend of the same
    /// identity can tell. Returns [`SinkError::Terminal`] for a definite
    /// refusal with no earlier ambiguity, including a failed forced refresh.
    async fn send_owned_bytes(
        &self,
        table: &str,
        batch_id: [u8; 16],
        request_id: Option<&RequestId>,
        arrow_ipc: Bytes,
    ) -> Result<(), SinkError> {
        let mut retry_number = 0_u32;
        let mut refreshed = false;
        let mut uncertain = false;
        let settle = |error: WyrdError, retryable: bool| {
            if retryable {
                SinkError::Retryable(error)
            } else {
                SinkError::Terminal(error)
            }
        };
        loop {
            let request = InsertBatchRequest {
                table: table.to_owned(),
                arrow_ipc: arrow_ipc.clone(),
                wyrd_batch_id: Bytes::copy_from_slice(&batch_id),
            };
            match self.send_once(request, request_id).await {
                Ok(()) => return Ok(()),
                Err(error) if error.unauthenticated && !refreshed => {
                    refreshed = true;
                    self.connection
                        .auth()
                        .force_refresh()
                        .await
                        .map_err(|error| settle(auth_error_to_wyrd(error), uncertain))?;
                }
                Err(error) if error.retryable && retry_number < self.config.max_frame_retries => {
                    uncertain = true;
                    tokio::time::sleep(retry_delay(retry_number)).await;
                    retry_number = retry_number.saturating_add(1);
                }
                Err(error) => return Err(settle(error.error, error.retryable || uncertain)),
            }
        }
    }

    /// Attaches the bearer token and `wyrd-request-id` to one attempt.
    ///
    /// A supplied `request_id` is forwarded unchanged through the shared
    /// [`crate::auth::AuthMiddleware::request_id`]; otherwise one is minted.
    ///
    /// # Errors
    ///
    /// Returns the mapped authentication failure when no bearer is available,
    /// and [`WyrdError::Internal`] when a value cannot be gRPC metadata.
    async fn add_auth_metadata<T>(
        &self,
        request: &mut Request<T>,
        request_id: Option<&RequestId>,
    ) -> Result<(), WyrdError> {
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

        let request_id = self
            .connection
            .auth()
            .request_id(request_id.map(RequestId::as_str));
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
    /// Returns [`SinkError::Terminal`] for frame validation and every definite
    /// server refusal. Returns [`SinkError::Retryable`] with the last ambiguous
    /// unavailability or stable `WYRD_VALA_429_INGEST_BUSY` once this
    /// transport's bounded retry budget is exhausted with the same UUID and
    /// bytes, and when the whole operation, including authentication waits
    /// and refresh, outlives that budget: the outcome is then unknown, so the
    /// queue retains the batch and resends the same identity later rather
    /// than counting a possibly durable batch as lost. Cancellation before an
    /// ACK likewise leaves the queue's borrowed UUID, bytes, and permit intact
    /// for retry resolution.
    async fn insert_batch(
        &self,
        batch: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError> {
        if let Err(error) =
            validate_table(&batch.table).and_then(|()| validate_batch_id(batch.batch_id))
        {
            return Err(SinkError::Terminal(error));
        }
        let bytes = Bytes::from_owner(batch.frame.shared_bytes());
        let send = self.send_owned_bytes(
            &batch.table,
            batch.batch_id,
            batch.request_id.as_ref(),
            bytes,
        );
        tokio::time::timeout(self.attempt_budget(), send)
            .await
            .unwrap_or_else(|_| {
                Err(SinkError::Retryable(transport_unavailable(
                    "attempt budget exhausted before ACK",
                )))
            })
            .map(|()| DurableBatchAck {
                batch_id: batch.batch_id,
                rows: batch.rows,
            })
    }
}

#[derive(Debug)]
struct AttemptError {
    /// Typed stable error returned to the caller when retries stop.
    error: WyrdError,
    /// Whether this attempt is ambiguous or carries stable ingest-busy identity.
    retryable: bool,
    /// Whether the server refused the attempt's credential, which earns one
    /// forced refresh before the refusal becomes terminal.
    unauthenticated: bool,
}

impl AttemptError {
    /// Wraps a failure that must settle without another transport attempt.
    fn terminal(error: WyrdError) -> Self {
        Self {
            error,
            retryable: false,
            unauthenticated: false,
        }
    }

    /// Classifies a gRPC failure by stable reason before considering transport loss.
    fn from_status(status: Status) -> Self {
        let stable = from_grpc_status(&status);
        if stable.code() == "WYRD_VALA_429_INGEST_BUSY" {
            Self {
                error: stable,
                retryable: true,
                unauthenticated: false,
            }
        } else if retryable_status(&status) {
            Self {
                error: transport_unavailable("unary ingest transport failed before ACK"),
                retryable: true,
                unauthenticated: false,
            }
        } else {
            Self {
                unauthenticated: status.code() == Code::Unauthenticated,
                ..Self::terminal(stable)
            }
        }
    }
}

/// Refuses a batch that names no table before it reaches the transport.
///
/// Frame size is deliberately not checked here: the ingest wire ceiling is an
/// operator-configured server setting, and the server refuses a larger frame
/// with its typed payload-too-large error.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when `table` is empty.
fn validate_table(table: &str) -> Result<(), WyrdError> {
    if table.is_empty() {
        return Err(WyrdError::Validation {
            message: "bifrost batch table must not be empty".to_owned(),
            details: serde_json::json!({ "field": "table" }),
        });
    }
    Ok(())
}

/// Refuses a queue-owned frame identity that is not a `UUIDv7`.
///
/// The identity is supplied by the producer that sealed the frame, so the
/// transport verifies it rather than trusting the queue's caller.
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
    use crate::bifrost::BifrostClientError;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use wyrd_queue::{ClientByteBudget, OwnedIpcBytes, QueueConfig, WyrdQueueError};

    use crate::auth::AuthMiddleware;
    use crate::config::ClientConfig;
    use crate::transport::HttpTransport;
    use crate::transport::credential::ResolvedCredential;
    use secrecy::SecretString;
    use tokio::sync::Mutex;
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
        /// Ordered `wyrd-request-id` metadata observed at the RPC boundary.
        request_ids: Arc<Mutex<Vec<String>>>,
        /// Selects the next scripted response without serializing request recording.
        attempts: Arc<AtomicUsize>,
        /// Answers every attempt with stable busy, standing in for a sink that
        /// never recovers.
        always_busy: Arc<AtomicBool>,
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
            let request_id = request
                .metadata()
                .get("wyrd-request-id")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            self.request_ids.lock().await.push(request_id);
            let request = request.into_inner();
            self.batch_ids
                .lock()
                .await
                .push(request.wyrd_batch_id.to_vec());
            let attempt = self.attempts.fetch_add(1, Ordering::AcqRel);
            match if self.always_busy.load(Ordering::Acquire) {
                0
            } else {
                attempt
            } {
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

    /// Seals one queue-owned batch so the test drives the real transport door.
    ///
    /// The transport only accepts a [`SealedBatch`]; minting the identity and
    /// the byte guard here is what a producer does before it hands the batch to
    /// the sink.
    fn sealed_batch(budget: &ClientByteBudget, bytes: Vec<u8>) -> SealedBatch<ClientByteGuard> {
        let guard = budget
            .reserve(bytes.len())
            .expect("test frame fits the budget");
        SealedBatch {
            table: "events".to_owned(),
            batch_id: Uuid::now_v7().into_bytes(),
            frame: OwnedIpcBytes::new(bytes, guard),
            rows: 1,
            request_id: None,
        }
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

    /// Stable busy retries while every other capacity status terminalizes, and
    /// a sink that stays busy settles terminally after exactly the configured
    /// attempts under one UUID and request ID.
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
            grpc: crate::transport::GrpcConfig {
                endpoint: format!("http://{address}"),
                connect_retries: 0,
                ..crate::transport::GrpcConfig::default()
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
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let request_id = RequestId::now_v7();
        let retried = SealedBatch {
            request_id: Some(request_id.clone()),
            ..sealed_batch(&budget, vec![1, 2, 3])
        };
        let batch_id = retried.batch_id;
        transport
            .insert_batch(&retried)
            .await
            .expect("stable busy retries to concrete ACK");
        let observed = service.batch_ids.lock().await.clone();
        assert_eq!(observed, vec![batch_id.to_vec(), batch_id.to_vec()]);
        assert_eq!(
            *service.request_ids.lock().await,
            vec![request_id.to_string(), request_id.to_string()],
            "every retry carries the batch's originating request"
        );

        let terminal = transport
            .insert_batch(&sealed_batch(&budget, vec![4]))
            .await
            .expect_err("permanent capacity terminalizes without retry");
        assert_ne!(terminal.error().code(), "WYRD_VALA_429_INGEST_BUSY");
        assert!(
            matches!(terminal, SinkError::Terminal(_)),
            "permanent capacity must not ask the queue to retry: {terminal:?}"
        );
        assert_eq!(service.attempts.load(Ordering::Acquire), 3);

        service.always_busy.store(true, Ordering::Release);
        let exhausting = BifrostGrpcTransport::connect_with_config(
            &client,
            BifrostTransportConfig::with_max_frame_retries(1),
        )
        .await
        .expect("real test gRPC transport connects");
        let exhausted = SealedBatch {
            request_id: Some(request_id.clone()),
            ..sealed_batch(&budget, vec![5])
        };
        let error = exhausting
            .insert_batch(&exhausted)
            .await
            .expect_err("a sink that stays busy exhausts the retry budget");
        assert!(
            matches!(&error, SinkError::Retryable(error) if error.code() == "WYRD_VALA_429_INGEST_BUSY"),
            "exhaustion asks the queue to retain the batch with the last refusal: {error:?}"
        );
        assert_eq!(
            service.attempts.load(Ordering::Acquire),
            5,
            "one retry only"
        );
        assert_eq!(
            service.batch_ids.lock().await[3..],
            [exhausted.batch_id.to_vec(), exhausted.batch_id.to_vec()]
        );
        assert_eq!(
            service.request_ids.lock().await[3..],
            [request_id.to_string(), request_id.to_string()]
        );
        drop((retried, exhausted));
        assert_eq!(
            budget.used_bytes(),
            0,
            "settled batches release their bytes"
        );
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

    /// Real gRPC service that counts each attempt and never answers it, so the
    /// client's per-call deadline ends every call.
    #[derive(Clone, Default)]
    struct HeldIngest {
        /// Attempts that reached the RPC boundary.
        attempts: Arc<AtomicUsize>,
    }

    #[wyrd_tonic::tonic::async_trait]
    impl BifrostIngestService for HeldIngest {
        /// Counts the attempt and holds it open until the client's deadline
        /// cancels it.
        ///
        /// # Errors
        ///
        /// Never returns; the client abandons the call at its deadline.
        async fn insert_batch(
            &self,
            _request: Request<InsertBatchRequest>,
        ) -> Result<Response<InsertBatchResponse>, Status> {
            self.attempts.fetch_add(1, Ordering::AcqRel);
            std::future::pending().await
        }
    }

    /// Sends one one-row batch through a real facade authenticated by a
    /// static bearer against `service`, whose producer asks for a 1 ms send
    /// deadline, then shuts the facade down.
    ///
    /// Every send ends without an acknowledgement, so the outcome is
    /// ambiguous: the queue must retain the oldest batch with its bytes and
    /// retry slot, report no loss, and refuse to report shutdown as drained.
    ///
    /// Returns the RPC attempts `attempts` counted when the flush returned and
    /// the loss observer's reports.
    ///
    /// # Panics
    ///
    /// Panics when the service, facade, or batch cannot be built, when a
    /// flush returns an unexpected error, the ambiguous batch releases its
    /// ownership, or shutdown reports the unacknowledged batch as drained.
    async fn retain_held_batch<S: BifrostIngestService>(
        service: S,
        attempts: &AtomicUsize,
    ) -> (usize, Vec<u64>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test address resolves");
        drop(listener);
        let server = tokio::spawn(
            Server::builder()
                .add_service(BifrostIngestServiceServer::new(service))
                .serve(address),
        );
        tokio::task::yield_now().await;

        let config = ClientConfig {
            grpc: crate::transport::GrpcConfig {
                endpoint: format!("http://{address}"),
                connect_retries: 0,
                timeout_ms: 50,
                ..crate::transport::GrpcConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from("test-token")),
        )
        .expect("test auth builds");
        let http = HttpTransport::new(&config.http, auth.clone()).expect("test HTTP layer builds");
        let client = WyrdClient::from_parts(Arc::clone(&auth), http, config.grpc);
        let bifrost = crate::bifrost::Bifrost::connect_with_config(
            &client,
            None,
            QueueConfig {
                linger_ms: 60_000,
                flush_timeout_ms: 1,
                ..QueueConfig::default()
            },
        )
        .await
        .expect("real test gRPC transport connects");
        let losses = Arc::new(Mutex::new(Vec::new()));
        bifrost.observe_losses({
            let losses = Arc::clone(&losses);
            move |rows| {
                losses
                    .try_lock()
                    .expect("loss observer is uncontended")
                    .push(rows)
            }
        });

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1]))])
            .expect("test batch builds");
        bifrost
            .enqueue_batch("events", batch, None)
            .expect("batch admitted");
        match bifrost.flush().await {
            Ok(())
            | Err(BifrostClientError::Queue(
                WyrdQueueError::Sink(WyrdError::ServiceUnavailable { .. })
                | WyrdQueueError::FlushTimeout,
            )) => {}
            Err(error) => panic!("unexpected flush failure: {error}"),
        }
        let flushed = attempts.load(Ordering::Acquire);
        let metrics = bifrost.metrics();
        assert!(metrics.owned_bytes > 0, "ambiguity retains the batch bytes");
        assert!(metrics.live_batches >= 1, "ambiguity retains the batch");
        assert_eq!(
            metrics.retry_entries, 1,
            "the oldest ambiguity holds the retry slot"
        );

        bifrost
            .shutdown()
            .await
            .expect_err("an unacknowledged batch is not drained");
        server.abort();
        let losses = losses.lock().await.clone();
        (flushed, losses)
    }

    /// A producer whose own send deadline is shorter than one call cannot
    /// cancel and restart the transport's attempt budget: the facade raises it,
    /// so a flushed batch held at every call's deadline receives the full
    /// configured attempt set, and the ambiguous outcome retains the batch
    /// instead of reporting a loss.
    ///
    /// The upper bound allows the retained batch's scheduled retry, which may
    /// reach the service before the count is read, but never a second whole
    /// attempt set inside the one flush.
    ///
    /// # Panics
    ///
    /// Panics when the flush covers other than one attempt set, a loss is
    /// reported, or the batch's ownership is released.
    #[tokio::test]
    async fn held_calls_exhaust_one_transport_budget_then_retain() {
        let service = HeldIngest::default();
        let attempts = Arc::clone(&service.attempts);
        let (flushed, losses) = retain_held_batch(service, &attempts).await;
        let configured = BifrostTransportConfig::default().max_frame_retries as usize + 1;
        assert!(
            (configured..2 * configured).contains(&flushed),
            "one attempt set: {flushed}"
        );
        assert!(losses.is_empty(), "ambiguity is not a loss: {losses:?}");
    }

    /// Ingest service that accepts only the refreshed `token-2` credential.
    #[derive(Clone, Default)]
    struct RefreshGatedIngest {
        /// Credentials observed in attempt order.
        tokens: Arc<Mutex<Vec<String>>>,
    }

    #[wyrd_tonic::tonic::async_trait]
    impl BifrostIngestService for RefreshGatedIngest {
        /// Refuses every credential but `token-2` as unauthenticated.
        ///
        /// # Errors
        ///
        /// Returns `Unauthenticated` for a stale credential.
        async fn insert_batch(
            &self,
            request: Request<InsertBatchRequest>,
        ) -> Result<Response<InsertBatchResponse>, Status> {
            let token = request
                .metadata()
                .get("x-wyrd-access-token")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            self.tokens.lock().await.push(token.clone());
            if token == "Bearer token-2" {
                Ok(Response::new(InsertBatchResponse {
                    wyrd_batch_id: request.into_inner().wyrd_batch_id,
                }))
            } else {
                Err(Status::unauthenticated("stale credential"))
            }
        }
    }

    /// An unauthenticated refusal forces exactly one shared refresh, resends
    /// the same batch with the replacement token, and never loops.
    ///
    /// # Panics
    ///
    /// Panics when the refused batch is not resent once with a freshly
    /// exchanged token.
    #[tokio::test]
    async fn unauthenticated_refusal_forces_one_shared_refresh_and_resends() {
        let service = RefreshGatedIngest::default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test port binds");
        let address = listener.local_addr().expect("test address resolves");
        drop(listener);
        let server = tokio::spawn(
            Server::builder()
                .add_service(BifrostIngestServiceServer::new(service.clone()))
                .serve(address),
        );
        tokio::task::yield_now().await;

        let exchange = wiremock::MockServer::start().await;
        for token in ["token-1", "token-2"] {
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "access_token": token,
                        "token_type": "Bearer",
                        "expires_in": 900,
                    }),
                ))
                .up_to_n_times(1)
                .mount(&exchange)
                .await;
        }
        let config = ClientConfig {
            grpc: crate::transport::GrpcConfig {
                endpoint: format!("http://{address}"),
                connect_retries: 0,
                ..crate::transport::GrpcConfig::default()
            },
            http: crate::transport::HttpConfig {
                base_url: exchange.uri(),
                ..crate::transport::HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::ApiKey(SecretString::from("wyrd_sk_tenant_visible_secret")),
        )
        .expect("test auth builds");
        let http = HttpTransport::new(&config.http, auth.clone()).expect("test HTTP layer builds");
        let client = WyrdClient::from_parts(auth, http, config.grpc);
        let transport = BifrostGrpcTransport::connect(&client)
            .await
            .expect("real test gRPC transport connects");
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        transport
            .insert_batch(&sealed_batch(&budget, vec![1]))
            .await
            .expect("refreshed credential acknowledges");
        assert_eq!(
            exchange
                .received_requests()
                .await
                .expect("requests are recorded")
                .len(),
            2,
            "one initial exchange and one forced refresh"
        );
        assert_eq!(
            *service.tokens.lock().await,
            vec!["Bearer token-1".to_owned(), "Bearer token-2".to_owned()]
        );
        server.abort();
    }
}
