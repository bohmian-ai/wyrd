//! Remote result publication under the tenant's SYSTEM writer.
//!
//! [`ResultPublisher`] never touches a local Scribe. For every publication
//! attempt it mints a fresh Verifier-scoped SYSTEM token through the tenant
//! issuer, dials the configured Scribe-bearing gRPC endpoint through
//! [`wyrd_client::Bifrost`], and writes each batch of the payload in order,
//! awaiting each durable acknowledgement before the next.
//!
//! Within one bounded attempt, `Bifrost::write_batch` seals each batch once
//! under one UUIDv7 batch ID and, while its acknowledgement is ambiguous,
//! resends that identical table, batch ID, and Arrow frame until Scribe
//! acknowledges it or the attempt fails terminally. Scribe's batch fence
//! deduplicates such a replay. A later attempt calls `write_batch` again and
//! so seals a fresh batch under a new ID: that is a new write, never a replay.

use std::error::Error;
use std::fmt::{Debug, Formatter, Result as FmtResult};
#[cfg(feature = "test-support")]
use std::sync::{Arc, Mutex};

use secrecy::SecretString;
use wyrd_auth::issuance::{IssuanceError, TenantTokenIssuer};
use wyrd_client::bifrost::BifrostClientError;
#[cfg(feature = "test-support")]
use wyrd_client::bifrost::{BifrostGrpcTransport, BifrostIngestSink, IngestTransport, QueueConfig};
use wyrd_client::config::{ClientConfig, TokenCacheMode};
use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::config::{GrpcConfig, HttpConfig};
use wyrd_client::{Bifrost, WyrdClient};
#[cfg(feature = "test-support")]
use wyrd_queue::{ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::CardRef;
use wyrd_sql::{SqlError, WyrdPostgres};

use super::results::ResultPayload;

/// Why one publication attempt did not acknowledge every batch.
///
/// Batches acknowledged before the failure remain durable and may be visible;
/// the run is never completed from a failed attempt.
#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    /// The tenant connection for minting could not be opened.
    #[error("tenant control connection failed")]
    Control(#[from] SqlError),
    /// The SYSTEM token could not be minted.
    #[error("SYSTEM result token could not be minted")]
    Mint(#[from] IssuanceError),
    /// The Bifrost client could not be built or connected.
    #[error("Bifrost result client could not connect")]
    Client(#[source] Box<dyn Error + Send + Sync>),
    /// A batch was not durably acknowledged.
    #[error("result batch for {table} was not acknowledged")]
    Write {
        /// The table whose batch failed.
        table: String,
        /// The client or server failure.
        #[source]
        source: BifrostClientError,
    },
}

impl From<WyrdClientError> for PublicationError {
    /// Wrap a client construction failure.
    fn from(error: WyrdClientError) -> Self {
        Self::Client(Box::new(error))
    }
}

/// Publishes result payloads to a remote Scribe as the tenant SYSTEM writer.
#[derive(Clone)]
pub struct ResultPublisher {
    /// Wyrd Postgres owner that opens the tenant transaction the issuer
    /// reads in.
    postgres: WyrdPostgres,
    /// The one tenant token issuer.
    issuer: TenantTokenIssuer,
    /// Scribe-bearing gRPC endpoint, `http://` or `https://`.
    endpoint: String,
    /// Test-only injected publication failures.
    #[cfg(feature = "test-support")]
    fault: Option<PublicationFault>,
}

impl Debug for ResultPublisher {
    /// Redacting debug: the issuer holds signing material.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ResultPublisher")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl ResultPublisher {
    /// Build a publisher that writes through `endpoint`, minting through
    /// tenant transactions opened on `postgres`.
    #[must_use]
    pub fn new(postgres: WyrdPostgres, issuer: TenantTokenIssuer, endpoint: String) -> Self {
        Self {
            postgres,
            issuer,
            endpoint,
            #[cfg(feature = "test-support")]
            fault: None,
        }
    }

    /// Attach a test-only publication fault.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_fault(mut self, fault: PublicationFault) -> Self {
        self.fault = Some(fault);
        self
    }

    /// Write every batch of `payload` for `tenant` as the SYSTEM writer scoped
    /// to `verifier`, in order, each durably acknowledged.
    ///
    /// Mints the token immediately before the attempt, so its five-minute
    /// lifetime always covers the attempt. Returns only after the last
    /// (summary) batch is acknowledged.
    ///
    /// # Errors
    /// Returns [`PublicationError`] when minting, connecting, or any batch
    /// write fails; earlier batches stay written.
    ///
    /// # Cancellation
    /// Dropping the future abandons the attempt; batches already acknowledged
    /// stay durable and the run is not completed.
    pub async fn publish(
        &self,
        tenant: DataTenantId,
        verifier: &CardRef,
        payload: &ResultPayload,
    ) -> Result<(), PublicationError> {
        let token = self.mint(tenant, verifier).await?;
        let client = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: self.endpoint.clone(),
                ..GrpcConfig::default()
            },
            http: HttpConfig::default(),
            credential: Some(token),
            tenant: None,
            token_cache: TokenCacheMode::InMemory,
            token_cache_path: None,
        })?;
        let bifrost = self.connect(&client).await?;
        for batch in payload.batches() {
            #[cfg(feature = "test-support")]
            if let Some(fault) = &self.fault {
                fault.before_write(&batch.table).await?;
            }
            bifrost
                .write_batch(&batch.table, &batch.batch)
                .await
                .map_err(|source| PublicationError::Write {
                    table: batch.table.clone(),
                    source,
                })?;
        }
        Ok(())
    }

    /// Dial the Bifrost write path for one attempt as `client`.
    ///
    /// Production always uses [`Bifrost::connect`]; a test-support build with
    /// an attached fault wraps the same gRPC transport so the fault can record
    /// and perturb acknowledgements without changing what is sent.
    ///
    /// # Errors
    /// Returns [`PublicationError::Client`] when the ingest channel cannot be
    /// dialled.
    async fn connect(&self, client: &WyrdClient) -> Result<Bifrost, PublicationError> {
        #[cfg(feature = "test-support")]
        if let Some(fault) = &self.fault {
            return fault.connect(client).await;
        }
        Bifrost::connect(client)
            .await
            .map_err(|error| PublicationError::Client(Box::new(error)))
    }

    /// Mint a SYSTEM token scoped to exactly `verifier` in `tenant`.
    ///
    /// The mint only reads, so the transaction is dropped without commit.
    ///
    /// # Errors
    /// Returns [`PublicationError::Control`] when the tenant connection fails
    /// and [`PublicationError::Mint`] when the issuer refuses.
    async fn mint(
        &self,
        tenant: DataTenantId,
        verifier: &CardRef,
    ) -> Result<SecretString, PublicationError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let token = self.issuer.issue_system_token(&mut conn, verifier).await?;
        Ok(token.access_token)
    }
}

/// Test-only publication faults around a named table's write.
///
/// `fail_on` refuses one table's batch before it is sent, as though it were
/// not acknowledged; `hang_on` stalls forever before it, modelling a crash
/// mid-publication; `lose_ack_on` lets Scribe durably accept one table's batch
/// and then reports the acknowledgement lost, so the facade must resend the
/// identical sealed batch. Each fault fires once. Every batch that reaches the
/// transport is recorded in [`Self::sent`].
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Default)]
pub struct PublicationFault {
    /// One-shot refusal before this table's write.
    fail_on: Arc<Mutex<Option<String>>>,
    /// One-shot stall before this table's write.
    hang_on: Arc<Mutex<Option<String>>>,
    /// One-shot lost acknowledgement after this table's durable write.
    lose_ack_on: Arc<Mutex<Option<String>>>,
    /// Every sealed batch sent, in transport order.
    sent: Arc<Mutex<Vec<SentBatch>>>,
}

/// One sealed batch exactly as the fault transport handed it to gRPC.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBatch {
    /// Destination table.
    pub table: String,
    /// The UUIDv7 batch ID Scribe deduplicates on.
    pub batch_id: [u8; 16],
    /// The sealed Arrow IPC frame.
    pub bytes: Vec<u8>,
}

#[cfg(feature = "test-support")]
impl PublicationFault {
    /// Refuse the next write to `table`.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn fail_next(&self, table: &str) {
        *self.fail_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Stall forever before the next write to `table`.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn hang_next(&self, table: &str) {
        *self.hang_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Report the next durably acknowledged write to `table` as lost.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    pub fn lose_ack_next(&self, table: &str) {
        *self.lose_ack_on.lock().expect("fault lock") = Some(table.to_owned());
    }

    /// Every sealed batch sent so far, in transport order.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    #[must_use]
    pub fn sent(&self) -> Vec<SentBatch> {
        self.sent.lock().expect("fault lock").clone()
    }

    /// Build the attempt's Bifrost client over a recording transport.
    ///
    /// Uses the same authenticated gRPC transport and default queue tuning as
    /// [`Bifrost::connect`], wrapped by [`FaultTransport`].
    ///
    /// # Errors
    /// Returns [`PublicationError::Client`] when the channel cannot be dialled.
    async fn connect(&self, client: &WyrdClient) -> Result<Bifrost, PublicationError> {
        let inner = BifrostGrpcTransport::connect(client).await?;
        let transport = FaultTransport {
            inner,
            fault: self.clone(),
        };
        Ok(Bifrost::with_sink(
            client,
            None,
            Arc::new(BifrostIngestSink::new(Arc::new(transport))),
            QueueConfig::default(),
        ))
    }

    /// Apply any armed pre-send fault for `table`.
    ///
    /// # Errors
    /// Returns a write failure when a refusal is armed for `table`.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    async fn before_write(&self, table: &str) -> Result<(), PublicationError> {
        let hang = take_if(&self.hang_on, table);
        if hang {
            std::future::pending::<()>().await;
        }
        if take_if(&self.fail_on, table) {
            return Err(PublicationError::Write {
                table: table.to_owned(),
                source: BifrostClientError::Protocol("test-injected refusal".to_owned()),
            });
        }
        Ok(())
    }
}

/// Test-only transport that records every sealed batch and can drop one ACK.
///
/// It forwards the exact borrowed sealed batch to the real gRPC transport, so
/// the bytes and batch ID Scribe sees are the ones recorded.
#[cfg(feature = "test-support")]
struct FaultTransport {
    /// The real authenticated gRPC transport.
    inner: BifrostGrpcTransport,
    /// Shared fault state.
    fault: PublicationFault,
}

#[cfg(feature = "test-support")]
#[async_trait::async_trait]
impl IngestTransport<ClientByteGuard> for FaultTransport {
    /// Record `batch`, send it for real, and report a lost acknowledgement
    /// when one is armed for its table.
    ///
    /// # Errors
    /// Propagates the real transport outcome, or a retryable
    /// `ServiceUnavailable` after a durable write whose ACK the fault dropped.
    ///
    /// # Panics
    /// Panics when the fault lock is poisoned.
    async fn insert_batch(
        &self,
        batch: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError> {
        self.fault.sent.lock().expect("fault lock").push(SentBatch {
            table: batch.table.clone(),
            batch_id: batch.batch_id,
            bytes: batch.bytes().to_vec(),
        });
        let ack = self.inner.insert_batch(batch).await?;
        if take_if(&self.fault.lose_ack_on, &batch.table) {
            return Err(SinkError::Retryable(
                wyrd_spec::error::WyrdError::ServiceUnavailable {
                    message: "test-injected lost acknowledgement".to_owned(),
                    details: serde_json::json!({ "table": batch.table }),
                },
            ));
        }
        Ok(ack)
    }
}

/// Clear `slot` and return true when it names `table`.
///
/// # Panics
/// Panics when the fault lock is poisoned.
#[cfg(feature = "test-support")]
fn take_if(slot: &Mutex<Option<String>>, table: &str) -> bool {
    let mut armed = slot.lock().expect("fault lock");
    if armed.as_deref() == Some(table) {
        *armed = None;
        return true;
    }
    false
}
