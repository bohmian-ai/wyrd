//! The one Bifrost client: query any authorized table, write to the active one.
//!
//! Reads and writes already share one credential, one [`WyrdClient`], and one
//! connection pool, so they share one client type here too. What differs is
//! binding: `POST /v1/query` accepts SQL over any table the caller may see,
//! while an ingest batch carries exactly one table and one Arrow schema. That
//! asymmetry is the server's, and it is why [`Bifrost::sql`] takes a query and
//! [`Bifrost::insert`] takes only a row.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::WyrdClient;
use crate::config::ClientConfig;
use arrow::datatypes::SchemaRef;
use arrow::json::WriterBuilder;
use arrow::json::writer::JsonArray;
use arrow::record_batch::RecordBatch;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex as AsyncMutex;
use wyrd_queue::QueueConfig;
use wyrd_queue::variant::VariantJsonEncoderFactory;
use wyrd_queue::{BatchSink, ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, BifrostTableDescription, CancelRunningQueryResponse, QueryTerminalFrame,
    RegisterOutcome, RegisterTableResponse, RunningQuerySummary,
};

use crate::bifrost::BifrostMetrics;
use crate::bifrost::grpc::BifrostGrpcTransport;
use crate::bifrost::handle::WriterPool;
use crate::bifrost::query::{
    BifrostClientError, CollectedQueryLimits, CollectedQueryResult, QueryClient, QueryResultStream,
};
use crate::bifrost::scope::ClientScope;
use crate::bifrost::sink::BifrostIngestSink;
use crate::bifrost::table::{Correlation, TableConfig, WriterTable};

/// The one Bifrost client: query any authorized table, write to the active one.
///
/// Writes are bound because an ingest batch carries exactly one table and one
/// Arrow schema; switching targets is [`Bifrost::use_table`], and a
/// swapped-away table's pooled producer keeps draining rather than being
/// discarded, so no buffered row is stranded by a swap.
pub struct Bifrost {
    /// The read plane, sharing the client's auth and HTTP connection pools.
    query: QueryClient,
    /// The write plane: one pooled producer per table this client has written.
    ///
    /// Shared because the blocking drains run on a worker thread, which needs
    /// an owned handle rather than a borrow of this client.
    writer: Arc<WriterPool>,
    /// The table [`Bifrost::insert`] enqueues into, if one is bound.
    active: Mutex<Option<TableConfig>>,
    /// Described user schemas, keyed by fully-qualified table name.
    ///
    /// Populated by [`Bifrost::writer_table`] and never evicted: one schema is
    /// authoritative per table name for this connected writer's lifetime, so a
    /// describe is paid once per table rather than once per observation.
    described: Mutex<HashMap<Arc<str>, WriterTable>>,
    /// Serializes cache-miss describes so racing first uses of one table pay
    /// one describe (and one authorization decision), not one each.
    ///
    /// ponytail: one gate for every table, so first misses of different
    /// tables also queue behind each other; per-table gating only if distinct
    /// first uses ever contend on a hot path.
    describe_gate: AsyncMutex<()>,
}

impl Bifrost {
    /// Build a client with no arguments, resolving its own transport.
    ///
    /// Endpoints come from `WYRD_GRPC_URL` / `WYRD_SERVER_URL` or the compiled
    /// defaults, and the credential from the `ClientConfig::resolve_credential`
    /// chain whose floor is `~/.config/wyrd/credentials.toml` `[default].api_key`.
    /// This is [`WyrdClient::from_env`] plus a connect; it adds no resolution of
    /// its own, so an explicitly built client and this one reach the same place.
    ///
    /// # Errors
    ///
    /// Returns the stable no-credentials error when the chain yields nothing,
    /// or a transport error when the ingest channel cannot be dialled.
    pub async fn from_env() -> Result<Self, BifrostClientError> {
        let client = client_from_env()?;
        Self::connect(&client).await
    }

    /// Build a query-only client over an explicitly supplied transport.
    ///
    /// `client` is an argument, not an owner: it carries the credential, the
    /// connection pool, and the API-key-to-bearer exchange the read and write
    /// planes already share.
    ///
    /// # Errors
    ///
    /// Returns a transport error when the ingest channel cannot be dialled.
    pub async fn connect(client: &WyrdClient) -> Result<Self, BifrostClientError> {
        Self::assemble(client, None, QueueConfig::default()).await
    }

    /// Build a client already bound to `table` for writes.
    ///
    /// # Errors
    ///
    /// As [`Bifrost::connect`].
    pub async fn connect_with_table(
        client: &WyrdClient,
        table: TableConfig,
    ) -> Result<Self, BifrostClientError> {
        Self::assemble(client, Some(table), QueueConfig::default()).await
    }

    /// Build a client with explicit producer tuning.
    ///
    /// The tuning seam for a larger or smaller handle byte budget, a longer
    /// linger for a deterministic journey, or a different send concurrency,
    /// without making [`QueueConfig`] part of the ordinary constructor.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] carrying
    /// `WYRD_CLIENT_400_CONFIG_INVALID` before dialling when `config` fails
    /// [`QueueConfig::validate`], and otherwise as [`Bifrost::connect`].
    pub async fn connect_with_config(
        client: &WyrdClient,
        table: Option<TableConfig>,
        config: QueueConfig,
    ) -> Result<Self, BifrostClientError> {
        Self::assemble(client, table, config).await
    }

    /// Build a client over a caller-supplied batch sink.
    ///
    /// The seam that was [`WriterPool::new`] before the facade existed: it
    /// performs no IO, so a `wyrd-queue` mock or stall sink stands in for the
    /// gRPC transport while the pooling, backpressure, and drop-counting
    /// behavior under test stays the production one.
    #[must_use]
    pub fn with_sink(
        client: &WyrdClient,
        table: Option<TableConfig>,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
    ) -> Self {
        Self {
            query: QueryClient::new(client),
            writer: Arc::new(WriterPool::new(
                ClientScope::from_client(client),
                sink,
                config,
            )),
            active: Mutex::new(table),
            described: Mutex::new(HashMap::new()),
            describe_gate: AsyncMutex::new(()),
        }
    }

    /// Build a read-only client that never dials the ingest channel.
    ///
    /// For callers that only query, describe, or manage query lifecycle — the
    /// `wyrd query` command, for example — and must not require a reachable
    /// gRPC endpoint. It performs no IO. Any write reaches a sink that refuses
    /// it with a stable validation error instead of sending a batch.
    #[must_use]
    pub fn query_only(client: &WyrdClient) -> Self {
        Self::with_sink(
            client,
            None,
            Arc::new(QueryOnlySink),
            QueueConfig::default(),
        )
    }

    /// Dial the ingest channel and assemble both planes over one client.
    ///
    /// The transport is the single owner of a batch's attempt budget, so the
    /// producer's send deadline is raised past that whole budget; a shorter
    /// deadline would cancel the transport and restart its attempts later.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] when `config` fails
    /// [`QueueConfig::validate`], and a transport error when the ingest
    /// channel cannot be dialled.
    async fn assemble(
        client: &WyrdClient,
        table: Option<TableConfig>,
        mut config: QueueConfig,
    ) -> Result<Self, BifrostClientError> {
        config.validate().map_err(BifrostClientError::Queue)?;
        let transport = BifrostGrpcTransport::connect(client)
            .await
            .map_err(BifrostClientError::from)?;
        let deadline_ms = u64::try_from(transport.send_deadline().as_millis()).unwrap_or(u64::MAX);
        config.flush_timeout_ms = config.flush_timeout_ms.max(deadline_ms);
        Ok(Self {
            query: QueryClient::new(client),
            writer: Arc::new(WriterPool::new(
                ClientScope::from_client(client),
                Arc::new(BifrostIngestSink::new(Arc::new(transport))),
                config,
            )),
            active: Mutex::new(table),
            described: Mutex::new(HashMap::new()),
            describe_gate: AsyncMutex::new(()),
        })
    }

    // ── table lifecycle ────────────────────────────────────────────────────

    /// Create the active table, returning whether it was created or matched.
    ///
    /// Idempotent: `POST /v1/bifrost/tables` answers
    /// [`RegisterOutcome::AlreadyExists`] for a matching fingerprint. On either
    /// outcome the active config is resolved in place, so
    /// `table().resolved()` reports the server's uid and fingerprint
    /// afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::NoActiveTable`] when no table is bound, the
    /// stable fingerprint-mismatch error when a table of the same name exists
    /// with different columns, and the stable reserved-column or validation
    /// error the server reports for a rejected declaration.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future may leave the table created server-side with the
    /// local config still unresolved; re-registering is idempotent and
    /// resolves it.
    ///
    /// # Panics
    ///
    /// Panics if the active-table lock is poisoned.
    pub async fn register(&self) -> Result<RegisterOutcome, BifrostClientError> {
        let request = {
            let active = self.active.lock().expect("active table lock poisoned");
            active
                .as_ref()
                .ok_or(BifrostClientError::NoActiveTable)?
                .register_request()
        };
        let response: RegisterTableResponse = self
            .query
            .client()
            .request_json(reqwest::Method::POST, "/v1/bifrost/tables", Some(&request))
            .await?;
        // The lock is released for the network round trip, so `use_table` may
        // have replaced the binding meanwhile. An identity is only ever the
        // answer to the declaration that asked for it: apply it when the still-
        // active table would send this exact request, and otherwise leave the
        // new binding with its own resolution state rather than stamping one
        // table's uid and fingerprint onto another.
        let mut active = self.active.lock().expect("active table lock poisoned");
        if let Some(table) = active.as_mut()
            && table.register_request() == request
        {
            table.resolve(&response);
        }
        Ok(response.outcome)
    }

    /// Bind `table` as the write target, returning the previous binding.
    ///
    /// Cheap and synchronous: the producer for the new table is built lazily on
    /// its first row. The previous table's producer stays in the pool, so its
    /// buffered rows still flush.
    ///
    /// # Panics
    ///
    /// Panics if the active-table lock is poisoned, which indicates an
    /// invariant-breaking panic in another client operation.
    pub fn use_table(&self, table: TableConfig) -> Option<TableConfig> {
        self.active
            .lock()
            .expect("active table lock poisoned")
            .replace(table)
    }

    /// Bind an already-registered table by name, describing it first.
    ///
    /// # Errors
    ///
    /// As [`TableConfig::describe`].
    pub async fn use_table_by_name(&self, fqn: &str) -> Result<(), BifrostClientError> {
        let table = TableConfig::describe(self.query.client(), fqn).await?;
        self.use_table(table);
        Ok(())
    }

    /// Describe `fqn` once and return its cached destination for writes.
    ///
    /// This is the routing door for a caller that writes more than one table
    /// from one client — an instrumented application emitting observations
    /// alongside its own records. It deliberately does **not** touch the active
    /// binding: [`Self::use_table_by_name`] performs a remote describe *and*
    /// replaces the handle's shared active table, so two concurrent scoped
    /// callers would race each other's destination. Returning a
    /// [`WriterTable`] instead gives each caller its own immutable route.
    ///
    /// The first call for a table describes it; later calls return the cached
    /// schema without network IO. A miss takes this writer's describe gate and
    /// rechecks the cache under it, so concurrent first calls for one table
    /// perform one describe and every caller receives that one schema and its
    /// one pooled producer for this writer's lifetime.
    ///
    /// Describing proves only that the table exists and is accessible. Row
    /// values are checked against the schema when the queue seals a batch, and
    /// the server checks the registered fingerprint, so this returning is not a
    /// durability or whole-row-validation acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns the stable not-found, authentication, authorization,
    /// availability, or protocol error the server reported for an unknown,
    /// unauthorized, or unavailable table, or a schema-parse error when the
    /// description cannot be mapped back to Arrow.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future leaves no server state behind and caches nothing;
    /// describe is a read, and dropping the future releases the gate to the
    /// next waiter, which describes in its place.
    ///
    /// # Panics
    ///
    /// Panics if the described-table lock is poisoned.
    pub async fn writer_table(&self, fqn: &str) -> Result<WriterTable, BifrostClientError> {
        if let Some(table) = self.cached_writer_table(fqn) {
            return Ok(table);
        }
        let _gate = self.describe_gate.lock().await;
        if let Some(table) = self.cached_writer_table(fqn) {
            return Ok(table);
        }
        let config = TableConfig::describe(self.query.client(), fqn).await?;
        let described = WriterTable::new(fqn, config.user_schema().clone());
        self.described
            .lock()
            .expect("described table lock poisoned")
            .insert(Arc::from(fqn), described.clone());
        Ok(described)
    }

    /// The cached destination for `fqn`, if this writer has described it.
    ///
    /// # Panics
    ///
    /// Panics if the described-table lock is poisoned.
    #[must_use]
    pub fn cached_writer_table(&self, fqn: &str) -> Option<WriterTable> {
        self.described
            .lock()
            .expect("described table lock poisoned")
            .get(fqn)
            .cloned()
    }

    /// Enqueue one JSON row into an explicitly named described table.
    ///
    /// The routing counterpart to [`Self::insert`]: the destination travels
    /// with the row rather than coming from the handle's shared active binding,
    /// so concurrent callers writing different tables never disturb each other.
    /// Correlation columns are added by the queue; they are not fields in
    /// `row`.
    ///
    /// Synchronous for the same reason [`Self::insert`] is — enqueueing is a
    /// bounded, non-blocking channel send — and durable only after
    /// [`Self::flush`] or [`Self::shutdown`] resolves.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] with `WYRD_CLIENT_429_QUEUE_FULL`
    /// when the producer channel, its staging ring, and the byte budget are all
    /// occupied.
    pub fn insert_into(
        &self,
        table: &WriterTable,
        row: Vec<u8>,
        correlation: Correlation,
    ) -> Result<(), BifrostClientError> {
        self.writer
            .insert(
                table.fqn(),
                table.user_schema(),
                row,
                correlation.card_ref,
                correlation.run_id,
            )
            .map_err(Into::into)
    }

    /// Enqueue every JSON row of one logical record into a described table, or
    /// none of them.
    ///
    /// The multi-row counterpart to [`Self::insert_into`]: every row carries
    /// the same correlation, and admission is all-or-none, so after a
    /// `WYRD_CLIENT_429_QUEUE_FULL` refusal the caller may drain with
    /// [`Self::flush`] or back off and resubmit the whole record without
    /// duplicating any row. Durable only after [`Self::flush`] or
    /// [`Self::shutdown`] resolves.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] with `WYRD_CLIENT_429_QUEUE_FULL`
    /// when the producer cannot admit every row now, and with
    /// `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE` when the record has more rows than
    /// one producer can ever admit at once.
    pub fn insert_rows_into(
        &self,
        table: &WriterTable,
        rows: Vec<Vec<u8>>,
        correlation: Correlation,
    ) -> Result<(), BifrostClientError> {
        self.writer
            .insert_rows(
                table.fqn(),
                table.user_schema(),
                rows,
                correlation.card_ref,
                correlation.run_id,
            )
            .map_err(Into::into)
    }

    /// The active write binding, if any.
    ///
    /// # Panics
    ///
    /// Panics if the active-table lock is poisoned.
    #[must_use]
    pub fn table(&self) -> Option<TableConfig> {
        self.active
            .lock()
            .expect("active table lock poisoned")
            .clone()
    }

    // ── write ──────────────────────────────────────────────────────────────

    /// Enqueue one JSON row into the active table, propagating queue-full.
    ///
    /// Synchronous in every language surface because enqueueing is a bounded,
    /// non-blocking channel send. The row is durable only after
    /// [`Self::flush`] or [`Self::shutdown`] resolves.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::NoActiveTable`] when no table is bound, or
    /// [`BifrostClientError::Queue`] with `WYRD_CLIENT_429_QUEUE_FULL` when the
    /// producer channel, its staging ring, and the byte budget are all occupied.
    ///
    /// # Panics
    ///
    /// Panics if the active-table lock is poisoned.
    pub fn insert(&self, row: Vec<u8>, correlation: Correlation) -> Result<(), BifrostClientError> {
        let (table, schema) = {
            let active = self.active.lock().expect("active table lock poisoned");
            let table = active.as_ref().ok_or(BifrostClientError::NoActiveTable)?;
            (table.fqn(), table.user_schema().clone())
        };
        self.writer
            .insert(
                &table,
                &schema,
                row,
                correlation.card_ref,
                correlation.run_id,
            )
            .map_err(Into::into)
    }

    /// Write one already-built Arrow batch to `table` and await its durability.
    ///
    /// This is the write door for data whose columns the JSON row path cannot
    /// express — binary payloads, fixed-size identities, and nested list or
    /// struct columns — which is what the canonical signal tables are made of.
    /// Build the batch from the table's own published contract
    /// ([`Self::describe`] plus `wyrd_queue::schema::writable_schema`) rather
    /// than from a restated schema.
    ///
    /// The table is named explicitly instead of taken from the bound active
    /// table: a batch of this kind targets one specific existing table, and
    /// binding one would invite a registration this caller does not want.
    ///
    /// The call first describes `table` — one authoritative describe per
    /// call, never cached — and conforms the batch to the declared columns
    /// through [`wyrd_queue::RowPreflight::prepare_batch`], the same owner
    /// and rules as row insertion: columns match by name in any order, an omitted nullable
    /// column is sent as nulls, and a declared Variant column may be the
    /// `arrow.parquet.variant` extension or `Utf8`/`LargeUtf8` JSON text,
    /// which is encoded to it. Every supplied non-Variant column keeps its
    /// type; whether it satisfies the destination's canonical contract is the
    /// server's judgement, and it answers with its own stable whole-batch
    /// refusal.
    ///
    /// Unlike [`Self::insert`], durability is complete when this resolves — the
    /// batch is not buffered and needs no [`Self::flush`].
    ///
    /// # Errors
    ///
    /// Returns the describe error for an unknown, unauthorized, or unavailable
    /// table and any conformance refusal (`BIFROST_UNDECLARED_FIELD`,
    /// `SCHEMA_PARSE` for an omitted required column,
    /// `BIFROST_UNSUPPORTED_TYPE`, or a catalogued Variant error) before
    /// admission, so neither changes queue,
    /// budget, or direct-send state; then [`BifrostClientError::Queue`] when
    /// the batch cannot be encoded, exceeds the accepted frame ceiling, or
    /// cannot fit this client's byte envelope, and the server's stable refusal
    /// when the batch is rejected.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future during describe or conformance leaves no
    /// state; once the send starts it follows the direct-send lifecycle.
    pub async fn write_batch(
        &self,
        table: &str,
        batch: &RecordBatch,
    ) -> Result<(), BifrostClientError> {
        let description = self.describe(table).await?;
        let batch =
            wyrd_queue::RowPreflight::from_description(&description)?.prepare_batch(batch)?;
        self.writer
            .write_batch(table, &batch)
            .await
            .map_err(Into::into)
    }

    /// Enqueue one owned Arrow batch for `table` without waiting for publication.
    ///
    /// This is the bounded, fire-and-return counterpart of
    /// [`Self::write_batch`] for Rust-native callers that must never delay
    /// their own work on Bifrost, such as the embedded server capture path.
    /// Admission charges this client's byte budget and one bounded producer
    /// slot, then returns. The background producer encodes the batch, seals it
    /// alone under a stable batch identity, and publishes it under `request_id`
    /// when supplied (so a server call's observations share its request), and retries, flushes,
    /// and drains it like buffered rows; it is durable only after that owner,
    /// [`Self::flush`], or [`Self::shutdown`] settles it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] with backpressure or
    /// `WYRD_CLIENT_429_QUEUE_FULL` when the producer, channel, or byte
    /// envelope cannot admit the batch, or the client is shutting down.
    pub fn enqueue_batch(
        &self,
        table: &str,
        batch: RecordBatch,
        request_id: Option<RequestId>,
    ) -> Result<(), BifrostClientError> {
        self.writer
            .enqueue_batch(table, batch, request_id)
            .map_err(Into::into)
    }

    /// Flush every pooled producer and await each durable acknowledgement.
    ///
    /// Every table this client has written is drained, not only the active one,
    /// so a swap followed by a flush loses nothing.
    ///
    /// # Errors
    ///
    /// Returns the first producer or sink failure after every producer has been
    /// attempted, including the server's own stable refusal of a sealed batch.
    ///
    /// # Panics
    ///
    /// Panics when the blocking drain task cannot be joined.
    pub async fn flush(&self) -> Result<(), BifrostClientError> {
        self.drain(Drain::Flush).await
    }

    /// Drain every producer and stop its background task.
    ///
    /// # Errors
    ///
    /// As [`Self::flush`].
    ///
    /// # Panics
    ///
    /// Panics when the blocking drain task cannot be joined.
    pub async fn shutdown(&self) -> Result<(), BifrostClientError> {
        self.drain(Drain::Shutdown).await
    }

    /// Run one blocking producer drain off the async runtime's worker threads.
    ///
    /// The pool's drains block on channel acknowledgement, so they must not run
    /// on a runtime thread that the sink's own IO needs to make progress.
    ///
    /// # Errors
    ///
    /// Returns the first producer or sink failure.
    ///
    /// # Panics
    ///
    /// Panics when the blocking drain task cannot be joined.
    async fn drain(&self, kind: Drain) -> Result<(), BifrostClientError> {
        let writer = Arc::clone(&self.writer);
        tokio::task::spawn_blocking(move || match kind {
            Drain::Flush => writer.flush(),
            Drain::Shutdown => writer.shutdown(),
        })
        .await
        .expect("bifrost drain task joins")
        .map_err(Into::into)
    }

    // ── read ───────────────────────────────────────────────────────────────

    /// Run one SQL SELECT and collect every batch.
    ///
    /// Any authorized table, not just the active one. Bounded by the server's
    /// query floor; use [`Self::stream`] for a result set larger than memory.
    ///
    /// # Errors
    ///
    /// Returns the stable invalid-SQL error, the floor's non-SELECT or
    /// oversized refusal, an incomplete-stream failure when the response ends
    /// without its required terminal, and stable authentication, authorization,
    /// availability, or protocol errors.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future abandons the request; the server cancels the query
    /// when the response body is dropped.
    pub async fn sql(&self, query: &str) -> Result<QueryResult, BifrostClientError> {
        let mut stream = self.stream(query).await?;
        let mut batches = Vec::new();
        while let Some(batch) = stream.next_batch().await? {
            batches.push(batch);
        }
        let schema = stream.schema().cloned().ok_or_else(|| {
            BifrostClientError::Protocol("query stream omitted its schema".to_owned())
        })?;
        let terminal = stream
            .terminal()
            .cloned()
            .ok_or(BifrostClientError::IncompleteQueryStream)?;
        Ok(QueryResult {
            batches,
            schema,
            terminal,
        })
    }

    /// Run one SQL SELECT and deserialize every row into `T`.
    ///
    /// The typed counterpart of [`Self::sql`]: the same query, authorization,
    /// limits, and validated terminal, projected onto the caller's own type
    /// after the result is complete. `T` describes the columns the query
    /// selects; it is never sent to the server and says nothing about how a
    /// table is stored.
    ///
    /// # Errors
    ///
    /// As [`Self::sql`], plus [`BifrostClientError::RowDeserialization`] when any row
    /// does not fit `T`. That failure is total: no partially converted result
    /// is returned.
    ///
    /// # Cancellation
    ///
    /// As [`Self::sql`]; conversion happens only after the query completes.
    pub async fn sql_as<T: DeserializeOwned>(
        &self,
        query: &str,
    ) -> Result<Vec<T>, BifrostClientError> {
        self.sql(query).await?.deserialize()
    }

    /// Run one SQL SELECT and return its batches as they arrive.
    ///
    /// The stream owns the HTTP response body: dropping it propagates
    /// cancellation. The terminal frame is required, so a stream that ends
    /// without one fails rather than presenting partial rows as success.
    ///
    /// # Errors
    ///
    /// As [`Self::sql`], plus a decode error on a malformed Arrow IPC frame.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future abandons the request before any row is read.
    pub async fn stream(&self, query: &str) -> Result<QueryResultStream, BifrostClientError> {
        self.query(&BifrostQueryRequest {
            sql: query.to_owned(),
            deadline_ms: None,
        })
        .await
    }

    /// Start one query from a complete request and return its batches as they arrive.
    ///
    /// The raw form [`Self::stream`] and [`Self::sql`] wrap: the caller chooses
    /// the deadline. The request is validated before any
    /// IO, and the returned stream owns the HTTP response body.
    ///
    /// # Errors
    ///
    /// Returns the stable invalid-SQL error for an invalid request, and the
    /// authentication, authorization, availability, or protocol error the
    /// transport reported.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future abandons the request; dropping the returned stream
    /// cancels response-body consumption.
    pub async fn query(
        &self,
        request: &BifrostQueryRequest,
    ) -> Result<QueryResultStream, BifrostClientError> {
        self.query.query(request).await
    }

    /// Run one query and collect it within explicit row and encoded-byte limits.
    ///
    /// # Errors
    ///
    /// As [`Self::query`], plus failed-terminal, incomplete-stream, Arrow, and
    /// bounds errors. Exceeding a bound drops the live stream and never returns
    /// truncated success.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future drops the response stream and its HTTP body.
    pub async fn collect_bounded(
        &self,
        request: &BifrostQueryRequest,
        limits: CollectedQueryLimits,
    ) -> Result<CollectedQueryResult, BifrostClientError> {
        self.query.collect_bounded(request, limits).await
    }

    /// List the active queries visible to the authenticated tenant.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, audit, availability, or
    /// protocol errors.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future abandons the request without client-owned state.
    pub async fn running(&self) -> Result<Vec<RunningQuerySummary>, BifrostClientError> {
        self.query.running().await
    }

    /// Get one active query visible to the authenticated tenant.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, not-found, availability,
    /// or protocol errors.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future leaves the active query unchanged.
    pub async fn status(
        &self,
        request_id: &RequestId,
    ) -> Result<RunningQuerySummary, BifrostClientError> {
        self.query.status(request_id).await
    }

    /// Request server-side cancellation of one active query.
    ///
    /// Does not close any local response stream.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, not-found, availability,
    /// or protocol errors.
    ///
    /// # Cancellation
    ///
    /// Once the server accepts cancellation, abandoning the future does not
    /// reverse the server-side transition.
    pub async fn cancel(
        &self,
        request_id: &RequestId,
    ) -> Result<CancelRunningQueryResponse, BifrostClientError> {
        self.query.cancel(request_id).await
    }

    /// Read one registered table's server-owned description.
    ///
    /// The canonical introspection door on this client: schema, identity, and
    /// physical layout exactly as the server holds them. `fqn` is
    /// `<namespace>.<name>`, the same form [`Bifrost::use_table_by_name`] and
    /// SQL take, so a caller never restates the name in two shapes.
    ///
    /// # Errors
    ///
    /// Returns a schema-parse error when `fqn` is not `<namespace>.<name>`, and
    /// the stable not-found, authentication, authorization, availability, or
    /// protocol error the server reported.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future leaves no server state behind; describe is a read.
    pub async fn describe(&self, fqn: &str) -> Result<BifrostTableDescription, BifrostClientError> {
        let (namespace, name) = crate::bifrost::table::split_fqn(fqn)?;
        self.query.describe_table(&namespace, &name).await
    }

    /// The client scope every pooled producer is keyed under.
    #[must_use]
    pub fn scope(&self) -> &ClientScope {
        self.writer.scope()
    }

    /// Number of distinct table producers currently pooled.
    ///
    /// One per table this client has written since construction, including
    /// tables it has since swapped away from — which is what makes a swap
    /// lossless.
    #[must_use]
    pub fn producer_count(&self) -> usize {
        self.writer.producer_count()
    }

    /// Rows dropped by the fire-and-forget [`crate::bifrost::observe::record`] path.
    ///
    /// Always zero for [`Self::insert`], which refuses rather than drops.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.writer.dropped()
    }

    /// Point-in-time bounded-ownership accounting for this client's producers.
    #[must_use]
    pub fn metrics(&self) -> BifrostMetrics {
        self.writer.metrics()
    }

    /// Registers `observer` to receive the row count of every accepted row
    /// this client settles as lost after admission, such as a terminal
    /// publication refusal or a retry slot it could not retain, as the loss
    /// settles. The loss's bytes and retry slot are already released, so
    /// [`Self::metrics`] read from the observer reports settled ownership.
    ///
    /// The observer runs on the producer task and must not block. Only the
    /// first registration takes effect.
    pub fn observe_losses(&self, observer: impl Fn(u64) + Send + Sync + 'static) {
        self.writer.observe_losses(observer);
    }

    /// The producer pool this client writes through.
    ///
    /// Crate-private so [`crate::bifrost::observe::record`] can reach the drop-counting
    /// path without the pool becoming a second public write door.
    pub(crate) fn writer(&self) -> &WriterPool {
        &self.writer
    }
}

/// Which terminal drain [`Bifrost::drain`] should run.
///
/// The two differ only in whether the producer is stopped afterwards, so they
/// share one blocking-offload path rather than duplicating it.
#[derive(Debug, Clone, Copy)]
enum Drain {
    /// Seal and acknowledge, leaving every producer running.
    Flush,
    /// Seal, acknowledge, and stop every producer's background task.
    Shutdown,
}

/// A collected query result: the Arrow batches plus the validated terminal.
///
/// Held as decoded [`RecordBatch`]es rather than raw IPC so Rust callers do not
/// re-decode; the Python and TypeScript surfaces re-encode once at their
/// boundary, which is the only place a language-native table can be built.
#[derive(Debug)]
pub struct QueryResult {
    /// Every batch the query produced, in arrival order.
    batches: Vec<RecordBatch>,
    /// The schema decoded from the stream's required initial schema frame.
    schema: SchemaRef,
    /// The validated success terminal; a failed terminal never reaches here.
    terminal: QueryTerminalFrame,
}

impl QueryResult {
    /// Every batch the query produced, in arrival order.
    #[must_use]
    pub fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }

    /// The authoritative result schema.
    #[must_use]
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// The validated terminal frame the server closed the stream with.
    #[must_use]
    pub fn terminal(&self) -> &QueryTerminalFrame {
        &self.terminal
    }

    /// Total decoded rows across every batch.
    #[must_use]
    pub fn num_rows(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
    }

    /// Deserialize every row into `T` through the Arrow JSON projection.
    ///
    /// Arrow's own writer produces one JSON array over the retained batches, so
    /// the column names and types `T` sees are exactly the schema the server
    /// sent — there is no second, hand-written type mapping to disagree with
    /// it. A Variant column renders as its JSON value through the shared
    /// [`VariantJsonEncoderFactory`],
    /// so `T` reads it as a `serde_json::Value` or any type that value fits.
    /// An empty result writes no array at all, which is the zero-row case.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Arrow`] when the JSON projection fails and
    /// [`BifrostClientError::RowDeserialization`] when any row does not fit `T`.
    fn deserialize<T: DeserializeOwned>(&self) -> Result<Vec<T>, BifrostClientError> {
        let mut bytes = Vec::new();
        {
            let mut writer = WriterBuilder::new()
                .with_encoder_factory(Arc::new(VariantJsonEncoderFactory))
                .build::<_, JsonArray>(&mut bytes);
            for batch in &self.batches {
                writer
                    .write(batch)
                    .map_err(|error| BifrostClientError::Arrow(error.to_string()))?;
            }
            writer
                .finish()
                .map_err(|error| BifrostClientError::Arrow(error.to_string()))?;
        }
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| BifrostClientError::RowDeserialization(error.to_string()))
    }

    /// Encode the whole result as one Arrow IPC stream.
    ///
    /// This is how the Python and TypeScript boundaries receive it: one stream
    /// their own Arrow implementation reads, rather than a per-language rebuild
    /// of the schema and arrays.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Arrow`] when IPC encoding fails.
    pub fn to_ipc(&self) -> Result<Vec<u8>, BifrostClientError> {
        let mut buffer = Vec::new();
        {
            let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &self.schema)
                .map_err(|error| BifrostClientError::Arrow(error.to_string()))?;
            for batch in &self.batches {
                writer
                    .write(batch)
                    .map_err(|error| BifrostClientError::Arrow(error.to_string()))?;
            }
            writer
                .finish()
                .map_err(|error| BifrostClientError::Arrow(error.to_string()))?;
        }
        Ok(buffer)
    }
}

/// The SDK-facing spelling of one register outcome.
///
/// The wire enum serializes as `Created`/`AlreadyExists`; every language
/// client presents the snake_case names its users read. Naming them once here
/// is what keeps the Python and TypeScript surfaces from disagreeing about
/// what a successful registration answered.
#[must_use]
pub fn register_outcome_name(outcome: RegisterOutcome) -> &'static str {
    match outcome {
        RegisterOutcome::Created => "created",
        RegisterOutcome::AlreadyExists => "already_exists",
    }
}

/// Assemble the ambient [`WyrdClient`] every no-argument constructor uses.
///
/// Delegates to [`client_from_options`] with nothing overridden, so
/// [`Bifrost::from_env`] and [`crate::bifrost::TableConfig::describe_from_env`] cannot
/// resolve their endpoints or credential differently from each other or from an
/// explicitly-configured client.
///
/// # Errors
///
/// Returns the stable no-credentials error when the chain yields nothing, or a
/// transport error when the HTTP client cannot be built.
pub(crate) fn client_from_env() -> Result<WyrdClient, BifrostClientError> {
    client_from_options(None, None, None)
}

/// Assemble a [`WyrdClient`] from optionally-overridden transport values.
///
/// Every argument is optional and every omitted one falls through to the
/// existing chain exactly once: [`ClientConfig::from_global_with_overrides`]
/// for the two endpoints (an omitted gRPC endpoint derives from the effective
/// `server_url`), `ClientConfig::resolve_credential` for the credential, whose
/// floor is `~/.config/wyrd/credentials.toml` `[default].api_key`. An explicit
/// value is written into the config's tier-0 slot, so it wins over the
/// environment rather than racing it.
///
/// This is the one door the Python and TypeScript constructors use, so their
/// "omitted resolves, explicit overrides" behavior is the Rust one and cannot
/// drift per language.
///
/// # Errors
///
/// Returns the stable no-credentials error when nothing in the chain yields a
/// credential, or a transport error when the HTTP client cannot be built.
pub fn client_from_options(
    server_url: Option<&str>,
    credential: Option<&str>,
    grpc_url: Option<&str>,
) -> Result<WyrdClient, BifrostClientError> {
    let mut config = ClientConfig::from_global_with_overrides(
        &crate::global_config::GlobalConfig::default(),
        server_url,
        grpc_url,
    );
    if let Some(credential) = credential {
        config.credential = Some(secrecy::SecretString::from(credential.to_owned()));
    }
    WyrdClient::with_config(config).map_err(BifrostClientError::from)
}

/// Ingest sink behind [`Bifrost::query_only`]: refuses every batch.
///
/// A read-only client has no ingest channel, so a write that reaches the sink
/// settles terminally with a stable validation error rather than dialling a
/// transport or reporting success.
struct QueryOnlySink;

#[async_trait::async_trait]
impl BatchSink<ClientByteGuard> for QueryOnlySink {
    /// Refuse `batch` without sending it.
    ///
    /// # Errors
    ///
    /// Always returns a terminal `WYRD_SPEC_400_VALIDATION` naming the table.
    async fn send(
        &self,
        batch: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError> {
        Err(wyrd_queue::SinkError::Terminal(
            wyrd_spec::error::WyrdError::Validation {
                message: "this Bifrost client was built with Bifrost::query_only and cannot write"
                    .to_owned(),
                details: serde_json::json!({ "table": batch.table }),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::{ArrayRef, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::reader::StreamReader;
    use serde_json::json;
    use wyrd_queue::MockSink;
    use wyrd_queue::variant::{
        EncodedVariant, VariantColumnBuilder, is_variant, variant_cell_to_json, variant_field,
    };
    use wyrd_spec::error::WyrdError;

    use super::*;
    use crate::bifrost::query::tests::recording_server;

    /// Describe body for `vala.bifrost.events`: a Variant `payload` beside a
    /// plain text `note`.
    const DESCRIBED: &str = r#"{
        "entry": {
            "namespace": "vala.bifrost",
            "name": "events",
            "table_uid": "0102030405060708090a0b0c0d0e0f10",
            "status": "Active",
            "fingerprint": "aa",
            "registered_at": "2026-07-01T00:00:00Z",
            "updated_at": "2026-07-01T00:00:00Z"
        },
        "user_fields": [
            { "name": "payload", "data_type": "Variant", "nullable": true, "metadata": {} },
            { "name": "note", "data_type": "Utf8", "nullable": true, "metadata": {} }
        ],
        "correlation_fields": [],
        "managed_candidates": [],
        "physical_layout": { "partition_granularity": "hour" }
    }"#;

    /// A client over `base_url` whose writes land in `sink`.
    ///
    /// # Panics
    ///
    /// Panics when the static configuration does not build a client.
    fn bifrost(base_url: &str, sink: &Arc<MockSink>) -> Bifrost {
        let config = ClientConfig {
            credential: Some(secrecy::SecretString::from("test-key")),
            http: crate::transport::config::HttpConfig {
                base_url: base_url.to_owned(),
                timeout_ms: 2_000,
                ..crate::transport::config::HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let client = WyrdClient::with_config(config).expect("static config builds a client");
        Bifrost::with_sink(
            &client,
            None,
            Arc::clone(sink) as Arc<dyn BatchSink<ClientByteGuard>>,
            QueueConfig::default(),
        )
    }

    /// A one-row batch of `payload` beside a JSON-looking `note`.
    ///
    /// # Panics
    ///
    /// Panics when the columns do not assemble.
    fn batch(payload: Field, column: ArrayRef) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                payload,
                Field::new("note", DataType::Utf8, true),
            ])),
            vec![
                column,
                Arc::new(StringArray::from(vec![r#"{"kept":"text"}"#])),
            ],
        )
        .expect("batch assembles")
    }

    /// Every `write_batch` performs one authoritative describe before
    /// admission. Utf8 and LargeUtf8 JSON text and the extension reach the
    /// server as the Variant extension, while a same-shaped text column the
    /// table does not declare as Variant stays text. Invalid JSON, a wrong wire
    /// type, a column supplied twice, and a failed describe refuse before any
    /// send or byte reservation.
    ///
    /// # Panics
    ///
    /// Panics when a call takes the wrong path or the sent wire differs.
    #[tokio::test]
    async fn variant_batch_describes_before_admission() {
        let (base_url, seen) = recording_server(DESCRIBED);
        let sink = Arc::new(MockSink::new());
        let client = bifrost(&base_url, &sink);
        let text = Field::new("payload", DataType::Utf8, true);
        let encoded = EncodedVariant::from_json(&json!({"a": 1})).expect("encodes");
        let accepted = [
            batch(
                text.clone(),
                Arc::new(StringArray::from(vec![r#"{"a":1}"#])),
            ),
            batch(
                Field::new("payload", DataType::LargeUtf8, true),
                Arc::new(arrow::array::LargeStringArray::from(vec![r#"{"a":1}"#])),
            ),
            batch(
                variant_field("payload", true),
                VariantColumnBuilder::from_iter([Some(&encoded)]).finish(),
            ),
        ];
        for (sent, input) in accepted.iter().enumerate() {
            client
                .write_batch("vala.bifrost.events", input)
                .await
                .expect("a declared Variant input is accepted");
            assert_eq!(
                seen.lock().expect("recording").len(),
                sent + 1,
                "one describe per call"
            );
            let receipt = sink.received().pop().expect("the batch was sent");
            let wire = StreamReader::try_new(receipt.bytes.as_slice(), None)
                .expect("frame decodes")
                .next()
                .expect("one batch")
                .expect("batch decodes");
            assert!(
                is_variant(wire.schema().field(0)),
                "the server receives the extension"
            );
            assert_eq!(
                variant_cell_to_json(wire.column(0).as_ref(), 0).expect("renders"),
                json!({"a": 1})
            );
            assert_eq!(wire.schema().field(1).data_type(), &DataType::Utf8);
        }
        assert!(
            seen.lock()
                .expect("recording")
                .iter()
                .all(|line| line.contains("/vala.bifrost/events")),
            "{:?}",
            seen.lock().expect("recording")
        );

        let refused = [
            (
                batch(text, Arc::new(StringArray::from(vec!["{not json"]))),
                "WYRD_VALA_400_VARIANT_INVALID_JSON",
            ),
            (
                batch(
                    Field::new("payload", DataType::Int64, true),
                    Arc::new(Int64Array::from(vec![1])),
                ),
                "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE",
            ),
            (
                RecordBatch::try_from_iter([
                    (
                        "payload",
                        Arc::new(StringArray::from(vec!["1"])) as ArrayRef,
                    ),
                    ("payload", Arc::new(StringArray::from(vec!["2"]))),
                ])
                .expect("Arrow allows duplicate names"),
                "WYRD_VALA_400_SCHEMA_PARSE",
            ),
        ];
        for (input, code) in &refused {
            let error = client
                .write_batch("vala.bifrost.events", input)
                .await
                .expect_err("the batch is refused before admission");
            assert_eq!(WyrdError::from(&error).code(), *code);
        }
        let offline = Arc::new(MockSink::new());
        let error = bifrost("http://127.0.0.1:1", &offline)
            .write_batch("vala.bifrost.events", &accepted[0])
            .await
            .expect_err("describe failure refuses the write");
        assert!(!matches!(error, BifrostClientError::Queue(_)), "{error:?}");
        assert!(offline.attempted().is_empty());
        assert_eq!(
            sink.attempted().len(),
            accepted.len(),
            "refusals send nothing"
        );
        assert_eq!(client.metrics().owned_bytes, 0, "no bytes stay reserved");
    }
}
