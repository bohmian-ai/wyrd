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

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use uuid::Uuid;
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::BifrostGrpcTransport;
use wyrd_client::bifrost::BifrostOptions;
use wyrd_client::bifrost::{Bifrost, Correlation, IngestTransport};
use wyrd_client::config::ClientConfig;
use wyrd_queue::{ClientByteBudget, OwnedIpcBytes, QueueConfig, SealedBatch, SinkError};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::ids::{RunId, TableVersion};

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
    /// `card_ref` is the Card this writer acts as: when it carries a UID the
    /// client selects it as the exact workload root, and every row this writer
    /// enqueues is correlated to that UID.
    ///
    /// # Errors
    ///
    /// Returns the stable client error when the credential cannot be resolved,
    /// the client cannot be built, the eager credential exchange is refused or
    /// the authorization server is unreachable, or the gRPC transport cannot
    /// connect. The exchange runs before native Bifrost assembly, so no
    /// producer exists when it fails.
    pub async fn connect(config: ClientConfig, card_ref: CardRef) -> Result<Self, WyrdError> {
        let client =
            WyrdClient::with_config(config).map_err(|error| harness_error(&error.to_string()))?;
        let client = match &card_ref.uid {
            Some(uid) => client.for_workload(uid.clone()),
            None => client,
        };
        let handle = Bifrost::connect_with(
            &client,
            BifrostOptions {
                table: None,
                queue: QueueConfig {
                    linger_ms: 60_000,
                    ..QueueConfig::default()
                },
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

    /// Describe `table` once and enqueue one JSON row without sealing it.
    ///
    /// The first row for a table describes it for its table version and
    /// builds the pooled producer from `schema`;
    /// later rows reuse it, so rebinding an already-written table reuses its
    /// producer rather than replacing it. The row is correlated to this
    /// writer's Card and, when given, to `run_id`, which the server stamps as
    /// the row's managed `run_id`.
    ///
    /// # Errors
    ///
    /// Returns the describe refusal (unknown or unauthorized table) or the
    /// queue-domain refusal (queue-full, reserved column, schema parse),
    /// projected onto the stable catalog.
    pub async fn enqueue(
        &self,
        table: &str,
        schema: &SchemaRef,
        row: Vec<u8>,
        run_id: Option<&RunId>,
    ) -> Result<(), WyrdError> {
        let table = self
            .handle
            .writer_table(table)
            .await?
            .with_schema(schema.clone());
        self.handle
            .insert_into(
                &table,
                row,
                Correlation {
                    subject_card_uid: self.card_ref.uid.clone(),
                    run_id: run_id.cloned(),
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

    /// Write one already-built Arrow batch and await its durability.
    ///
    /// This is the public batch door a canonical-signal caller uses: the batch
    /// already carries the table's own writable schema, so nothing here
    /// re-derives a row shape.
    ///
    /// # Errors
    ///
    /// Returns the stable client error when the batch is refused or the
    /// durable acknowledgement fails.
    pub async fn write_batch(
        &self,
        table: &str,
        batch: &arrow::record_batch::RecordBatch,
    ) -> Result<(), WyrdError> {
        self.handle
            .write_batch(table, batch)
            .await
            .map_err(WyrdError::from)
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
            self.enqueue(table, schema, row, None).await?;
        }
        self.flush().await
    }
}

/// Byte budget every clone of one [`RawIngest`] shares.
///
/// Holds one frame at the largest wire ceiling a journey configures (48 MiB)
/// with room to spare, or several default 16 MiB frames from concurrent load
/// writers. A frame above it is a fixture fault, not a server refusal.
const RAW_INGEST_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// The client ingest transport under a caller-chosen batch identity.
///
/// Cloning shares the authenticated channel, the byte budget, and the
/// describe cache, so concurrent load writers submit through one connection
/// and describe each table once.
#[derive(Clone)]
pub struct RawIngest {
    transport: BifrostGrpcTransport,
    budget: ClientByteBudget,
    /// Read-only client whose describe cache supplies each frame's table version.
    describer: Arc<Bifrost>,
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
            budget: ClientByteBudget::new(RAW_INGEST_BUDGET_BYTES),
            describer: Arc::new(Bifrost::query_only(client)),
        })
    }

    /// Submit one Arrow IPC frame under `batch_id` and return its request id.
    ///
    /// The request id is minted here and sent on every retry of the frame, so
    /// it is the `wyrd_request_id` the server stamps on the frame's rows; a
    /// journey reads rows back by it.
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
    /// The frame names the table version a describe returns, cached per table
    /// for this ingest's lifetime. A table the credential cannot describe is
    /// sent at [`TableVersion::INITIAL`], so the server's own ingest refusal is
    /// what the journey observes.
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
    ) -> Result<RequestId, WyrdError> {
        let table_version = self
            .describer
            .writer_table(table)
            .await
            .map_or(TableVersion::INITIAL, |described| described.table_version());
        self.insert_at(table, table_version, batch_id, arrow_ipc)
            .await
    }

    /// [`Self::insert`] under a caller-chosen table version.
    ///
    /// A writer retries an accepted batch at the version it was accepted
    /// under, not whatever the table's current version is; a journey that
    /// retries across a definition edit, or sends a stale new batch, names
    /// that version here.
    ///
    /// # Errors
    ///
    /// Returns the server's stable refusal, or the transport's stable error
    /// once its retry budget is spent.
    ///
    /// # Panics
    ///
    /// Panics when the frame does not fit the harness byte budget.
    pub async fn insert_at(
        &self,
        table: &str,
        table_version: TableVersion,
        batch_id: Uuid,
        arrow_ipc: Vec<u8>,
    ) -> Result<RequestId, WyrdError> {
        let guard = self
            .budget
            .reserve_sealed(arrow_ipc.len())
            .expect("harness frame fits the client byte budget");
        let request_id = RequestId::now_v7();
        let batch = SealedBatch {
            table: table.to_owned(),
            table_version,
            batch_id: batch_id.into_bytes(),
            frame: OwnedIpcBytes::new(arrow_ipc, guard),
            rows: 0,
            request_id: Some(request_id.clone()),
            producer_traces: Vec::new(),
        };
        self.transport
            .insert_batch(&batch)
            .await
            .map(|_ack| request_id)
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
