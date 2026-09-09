//! The two Bifrost write doors tests are allowed to use.
//!
//! [`BifrostWriter`] is the ordinary client path — `Bifrost::insert` into a
//! pooled `wyrd-queue` producer, sealed and shipped through the SDK's
//! `IngestTransport`. Every positive journey writes through it, because that is
//! the only write path a real caller has. It binds the table per call rather
//! than once at construction, because a journey routinely writes several
//! tables through one credential; the SDK pools a producer per table, so
//! rebinding costs nothing and strands nothing.
//!
//! [`RawIngest`] is the same transport under a caller-chosen batch identity.
//! `Bifrost::insert` deliberately mints its own UUIDv7 and refuses to expose
//! that choice, so replay-dedup, fencing, and per-row correlation assertions —
//! which need to *choose* the frame — seal their own batch and hand it to the
//! SDK's `IngestTransport`. They bypass the queue's batching, not its wire
//! contract: frame validation, auth metadata, and the retry that owns
//! `WYRD_VALA_429_INGEST_BUSY` all still apply.

use arrow::datatypes::SchemaRef;
use uuid::Uuid;
use vala_sdk::grpc::BifrostGrpcTransport;
use vala_sdk::{Bifrost, Correlation, IngestTransport, TableConfig};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_queue::{ClientByteBudget, OwnedIpcBytes, QueueConfig, SealedBatch, SinkError};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

/// The client write door, wired for tests.
///
/// Owns one [`Bifrost`] handle over a real authenticated gRPC transport plus
/// the `CardRef` its credential is scoped to, so a caller supplies only the
/// table, its user schema, and JSON rows.
pub struct BifrostWriter {
    client: WyrdClient,
    handle: Bifrost,
    card_ref: CardRef,
}

impl BifrostWriter {
    /// Connect one pooled-producer writer for `config`'s credential.
    ///
    /// `config` is consumed because [`ClientConfig`] is not cloneable; the
    /// [`WyrdClient`] built from it stays reachable through [`Self::client`] so
    /// a journey reads back through the same credential it wrote with.
    ///
    /// `card_ref` is the Card the credential was minted against; every row this
    /// writer enqueues is correlated to it, which is what the ingest route
    /// admits.
    ///
    /// # Errors
    ///
    /// Returns the stable client error when the credential cannot be resolved,
    /// the client cannot be built, or the gRPC transport cannot connect.
    pub async fn connect(config: ClientConfig, card_ref: CardRef) -> Result<Self, WyrdError> {
        let client =
            WyrdClient::with_config(config).map_err(|error| harness_error(&error.to_string()))?;
        let handle = Bifrost::connect_with_config(
            &client,
            None,
            QueueConfig {
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
        )
        .await?;
        Ok(Self {
            client,
            handle,
            card_ref,
        })
    }

    /// The authenticated client this writer was built from.
    #[must_use]
    pub fn client(&self) -> &WyrdClient {
        &self.client
    }

    /// Bind `table` and enqueue one JSON row without sealing it.
    ///
    /// The first row for a table builds the pooled producer from `schema`;
    /// later rows reuse it, so rebinding an already-written table reuses its
    /// producer rather than replacing it.
    ///
    /// # Errors
    ///
    /// Returns the queue-domain refusal (queue-full, reserved column, schema
    /// parse) or the table-name rejection, projected onto the stable catalog.
    pub fn enqueue(&self, table: &str, schema: &SchemaRef, row: Vec<u8>) -> Result<(), WyrdError> {
        self.handle
            .use_table(TableConfig::from_arrow(table, schema.clone())?);
        self.handle
            .insert(
                row,
                Correlation {
                    card_ref: Some(self.card_ref.clone()),
                    run_id: None,
                },
            )
            .map_err(WyrdError::from)
    }

    /// Seal and ship every pooled producer, waiting for each durable ACK.
    ///
    /// # Errors
    ///
    /// Returns the first producer or sink failure, including the server's own
    /// stable refusal of the sealed batch.
    pub async fn flush(&self) -> Result<(), WyrdError> {
        self.handle.flush().await.map_err(WyrdError::from)
    }

    /// Enqueue every row for one table and flush them as one durable write.
    ///
    /// # Errors
    ///
    /// Returns the first enqueue or flush failure.
    pub async fn write(
        &self,
        table: &str,
        schema: &SchemaRef,
        rows: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), WyrdError> {
        for row in rows {
            self.enqueue(table, schema, row)?;
        }
        self.flush().await
    }
}

/// The client ingest transport under a caller-chosen batch identity.
///
/// Cloning shares the authenticated channel and the byte budget, so concurrent
/// load writers submit through one connection.
#[derive(Clone)]
pub struct RawIngest {
    transport: BifrostGrpcTransport,
    budget: ClientByteBudget,
}

impl RawIngest {
    /// Connect the SDK ingest transport over an already-authenticated client.
    ///
    /// # Errors
    ///
    /// Returns the stable client error when the channel cannot be dialled.
    pub async fn connect(client: &WyrdClient) -> Result<Self, WyrdError> {
        Ok(Self {
            transport: BifrostGrpcTransport::connect(client)
                .await
                .map_err(|error| harness_error(&error.to_string()))?,
            budget: ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT),
        })
    }

    /// Submit one Arrow IPC frame under `batch_id`.
    ///
    /// The frame is sealed exactly as a producer seals one, so it inherits the
    /// transport's frame validation, auth metadata, and bounded retry of
    /// `WYRD_VALA_429_INGEST_BUSY` — a busy refusal wrote nothing, so retrying
    /// the identical identity is the contract, not a second write. Choosing to
    /// *replay* an already-acknowledged identity stays the caller's decision,
    /// which is what the dedup journeys assert.
    ///
    /// The sealed row count is zero because it is echoed only in the ACK this
    /// method discards; the server counts the rows it decodes from the frame.
    ///
    /// # Errors
    ///
    /// Returns the server's stable refusal, or the transport's stable error
    /// once its retry budget is spent.
    ///
    /// # Panics
    ///
    /// Panics when the frame does not fit the harness byte budget.
    pub async fn insert(
        &self,
        table: &str,
        batch_id: Uuid,
        arrow_ipc: Vec<u8>,
    ) -> Result<(), WyrdError> {
        let guard = self
            .budget
            .reserve_sealed(arrow_ipc.len())
            .expect("harness frame fits the client byte budget");
        let batch = SealedBatch {
            table: table.to_owned(),
            batch_id: batch_id.into_bytes(),
            frame: OwnedIpcBytes::new(arrow_ipc, guard),
            rows: 0,
        };
        self.transport
            .insert_batch(&batch)
            .await
            .map(|_ack| ())
            .map_err(|error| match error {
                SinkError::Retryable(error) | SinkError::Terminal(error) => error,
            })
    }
}

/// Projects one harness-side setup failure onto the stable catalog.
fn harness_error(message: &str) -> WyrdError {
    WyrdError::Internal {
        message: message.to_owned(),
        details: serde_json::json!({ "surface": "wyrd-testing::bifrost::write" }),
    }
}
