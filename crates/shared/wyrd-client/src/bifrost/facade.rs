//! The one Bifrost client: query any authorized table, write to the active one.
//!
//! Reads and writes already share one credential, one [`WyrdClient`], and one
//! connection pool, so they share one client type here too. What differs is
//! binding: `POST /v1/query` accepts SQL over any table the caller may see,
//! while an ingest batch carries exactly one table and one Arrow schema. That
//! asymmetry is the server's, and it is why [`Bifrost::sql`] takes a query and
//! [`Bifrost::insert`] takes only a row.

use std::sync::{Arc, Mutex};

use crate::WyrdClient;
use crate::config::ClientConfig;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use serde::de::DeserializeOwned;
use wyrd_queue::QueueConfig;
use wyrd_queue::{BatchSink, ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, BifrostTableDescription, CancelRunningQueryResponse, FreshnessPolicy,
    QueryTerminalFrame, RegisterOutcome, RegisterTableResponse, RunningQuerySummary,
    VisibilityMode,
};

use crate::bifrost::BifrostMetrics;
use crate::bifrost::grpc::BifrostGrpcTransport;
use crate::bifrost::handle::WriterPool;
use crate::bifrost::query::{
    BifrostClientError, CollectedQueryLimits, CollectedQueryResult, QueryClient, QueryResultStream,
};
use crate::bifrost::scope::ClientScope;
use crate::bifrost::sink::BifrostIngestSink;
use crate::bifrost::table::{Correlation, TableConfig};

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
    /// The tuning seam tests and long-running writers need — a zero flush
    /// interval for a deterministic journey, a smaller channel for a saturation
    /// test — without making [`QueueConfig`] part of the ordinary constructor.
    ///
    /// # Errors
    ///
    /// As [`Bifrost::connect`].
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
    /// Returns a transport error when the ingest channel cannot be dialled.
    async fn assemble(
        client: &WyrdClient,
        table: Option<TableConfig>,
        mut config: QueueConfig,
    ) -> Result<Self, BifrostClientError> {
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
    /// The batch is sent verbatim. Whether its columns satisfy the destination
    /// table's canonical contract is the server's judgement, and it answers
    /// with its own stable whole-batch refusal; this client does not pre-check
    /// it, because a second implementation of that contract is exactly what
    /// would drift.
    ///
    /// Unlike [`Self::insert`], durability is complete when this resolves — the
    /// batch is not buffered and needs no [`Self::flush`].
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] when the batch cannot be encoded,
    /// exceeds the accepted frame ceiling, or cannot fit this client's byte
    /// envelope, and the server's stable refusal when the batch is rejected.
    pub async fn write_batch(
        &self,
        table: &str,
        batch: &RecordBatch,
    ) -> Result<(), BifrostClientError> {
        self.writer
            .write_batch(table, batch)
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
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await
    }

    /// Start one query from a complete request and return its batches as they arrive.
    ///
    /// The raw form [`Self::stream`] and [`Self::sql`] wrap: the caller chooses
    /// visibility, freshness, and deadline. The request is validated before any
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
    /// it. An empty result writes no array at all, which is the zero-row case.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Arrow`] when the JSON projection fails and
    /// [`BifrostClientError::RowDeserialization`] when any row does not fit `T`.
    fn deserialize<T: DeserializeOwned>(&self) -> Result<Vec<T>, BifrostClientError> {
        let mut bytes = Vec::new();
        {
            let mut writer = arrow::json::ArrayWriter::new(&mut bytes);
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
/// existing chain exactly once: [`ClientConfig::from_env`] for the two
/// endpoints, `ClientConfig::resolve_credential` for the credential, whose
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
    let mut config = ClientConfig::from_env();
    if let Some(server_url) = server_url {
        config.http.base_url = server_url.trim_end_matches('/').to_owned();
    }
    if let Some(grpc_url) = grpc_url {
        config.grpc.endpoint = grpc_url.to_owned();
    }
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
