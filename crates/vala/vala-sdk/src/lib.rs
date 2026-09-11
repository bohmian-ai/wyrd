//! Vala client SDK — the client-tier Bifrost ingest surface.
//!
//! `vala-sdk` is the one Bifrost client: it registers tables, ships buffered
//! JSON rows to the Wyrd ingest service as Record batches, and reads them back
//! with terminal-safe SQL. It owns:
//!
//! - [`Bifrost`] — the client. A [`TableConfig`] binds the write target; reads
//!   are unbound because `POST /v1/query` accepts SQL over any authorized
//!   table. [`blocking::Bifrost`] is the same client for callers with no
//!   runtime.
//! - [`TableConfig`] — one table's identity, declared user columns, and
//!   requested physical layout, built from Arrow or JSON Schema or fetched by
//!   name. The server stays authoritative for the uid and fingerprint it
//!   reports back through [`ResolvedTable`].
//! - [`BifrostIngestSink`] — the `wyrd-queue` [`BatchSink`] that maps one sealed
//!   batch onto the ingest RPC (`table`, `wyrd_batch_id`, Arrow IPC frames),
//!   preserving `batch_id` so the server's commit dedup holds across retries.
//! - [`observe`] — fire-and-forget telemetry that never breaks its caller.
//!
//! [`ClientScope`] is a credential fingerprint the **token-opaque** client tier
//! computes without ever decoding a JWT: `(server_url, SHA-256 of the resolved
//! credential's secret material)`. Backpressure is asymmetric — [`Bifrost::insert`]
//! propagates queue-full to the caller, while [`observe::record`] swallows it,
//! counts the drop, and warns.
//!
//! [`BatchSink`]: wyrd_queue::BatchSink

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod bifrost;
pub mod blocking;
pub mod grpc;
mod handle;
mod native_owner;
pub mod observe;
#[cfg(feature = "python")]
pub mod python;
pub mod query;
pub mod scope;
pub mod sink;
mod table;

pub use bifrost::{Bifrost, QueryResult, client_from_options, register_outcome_name};
pub use grpc::{
    BifrostGrpcTransport, BifrostTransportConfig, MAX_FRAME_BYTES, MAX_FRAME_RETRIES,
    PROTO_FRAME_OVERHEAD_BYTES,
};
pub use handle::BifrostMetrics;
pub use query::{
    CollectedQueryLimits, CollectedQueryResult, QueryClient, QueryResultStream, RawQueryStream,
    ValaSdkError,
};
pub use scope::{ClientScope, SinkKind};
pub use sink::{BifrostIngestSink, IngestTransport};
pub use table::{Correlation, ResolvedTable, TableConfig};

// C4a forward schema helpers, re-exported so SDK users build the user Arrow
// schema from a `FieldSpec` set or a JSON-Schema value without reaching into
// `wyrd-queue` directly.
pub use wyrd_queue::{fieldspec_to_arrow, json_schema_to_arrow};

/// Server-free unit lane for `vala-sdk`: scope identity, producer pooling, and
/// the asymmetric backpressure contract — all driven through mock/stall sinks.
#[cfg(test)]
mod sdk {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::HttpConfig;
    use wyrd_queue::{
        BatchSink, ClientByteBudget, ClientByteGuard, DurableBatchAck, MockSink, OwnedIpcBytes,
        QueueConfig, SealedBatch, SinkError,
    };
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::reference::CardRef;

    use crate::handle::WriterPool;
    use crate::{
        Bifrost, BifrostIngestSink, ClientScope, Correlation, IngestTransport, TableConfig, observe,
    };

    fn config_with_key(url: &str, key: &str) -> ClientConfig {
        ClientConfig {
            http: HttpConfig {
                base_url: url.to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(key.to_owned().into()),
            ..ClientConfig::default()
        }
    }

    /// The producer pool under test, keyed on one deterministic scope.
    fn pool(sink: Arc<dyn BatchSink<ClientByteGuard>>, config: QueueConfig) -> WriterPool {
        WriterPool::new(
            ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope"),
            sink,
            config,
        )
    }

    /// A client over `sink`, built without any IO so a mock sink can stand in
    /// for the gRPC transport the production constructor would dial.
    fn client_over(sink: Arc<dyn BatchSink<ClientByteGuard>>, config: QueueConfig) -> Bifrost {
        let client = wyrd_client::WyrdClient::with_config(config_with_key("http://x", "secret"))
            .expect("client assembles without IO");
        Bifrost::with_sink(&client, None, sink, config)
    }

    /// A config declaring one `id` column on `fqn`.
    fn table(fqn: &str) -> TableConfig {
        TableConfig::from_arrow(fqn, test_schema()).expect("declared table")
    }

    /// One row correlated to the harness card.
    fn correlated() -> Correlation {
        Correlation {
            card_ref: Some(card()),
            run_id: None,
        }
    }

    fn test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    fn card() -> CardRef {
        "prod/Service/alpha@1.0.0".parse().expect("valid card ref")
    }

    fn row() -> Vec<u8> {
        br#"{"id": 1}"#.to_vec()
    }

    /// Config that saturates fast: a stalled sink parks the drain, the tiny channel
    /// fills, and further enqueues return queue-full.
    fn saturating_config() -> QueueConfig {
        QueueConfig {
            channel_capacity: 2,
            staging_capacity: 8,
            flush_max_rows: 1,
            flush_interval_ms: 0,
            flush_timeout_ms: 60_000,
            max_message_bytes: 4 * 1024 * 1024,
            ..QueueConfig::default()
        }
    }

    /// A sink whose `send` parks forever after signalling it started — stalls the
    /// producer's drain so the bounded channel cannot empty.
    #[derive(Default)]
    struct StallSink {
        started: AtomicBool,
    }

    #[async_trait]
    impl BatchSink<ClientByteGuard> for StallSink {
        async fn send(
            &self,
            _batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
            self.started.store(true, Ordering::SeqCst);
            std::future::pending::<()>().await;
            unreachable!()
        }
    }

    /// Sink that fails one named table while recording every attempted drain.
    #[derive(Default)]
    struct LifecycleSink {
        attempts: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl BatchSink<ClientByteGuard> for LifecycleSink {
        async fn send(
            &self,
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
            self.attempts
                .lock()
                .expect("attempts lock")
                .push(batch.table.clone());
            if batch.table == "a" {
                return Err(SinkError::Terminal(WyrdError::Internal {
                    message: "a producer failed".to_owned(),
                    details: serde_json::json!({}),
                }));
            }
            Ok(DurableBatchAck {
                batch_id: batch.batch_id,
                rows: batch.rows,
            })
        }
    }

    fn wait_until_started(sink: &StallSink) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !sink.started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "stall sink never started");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn same_url_and_credential_collapse_to_one_scope() {
        let a = ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        let b = ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        assert_eq!(a, b, "same (url, credential) must produce one scope");
        assert_eq!(a.credential_fingerprint(), b.credential_fingerprint());
    }

    #[test]
    fn different_credential_yields_different_scope() {
        let a = ClientScope::from_config(&config_with_key("http://x", "secret-a")).expect("scope");
        let b = ClientScope::from_config(&config_with_key("http://x", "secret-b")).expect("scope");
        assert_ne!(a, b, "differing secrets must not collide");
        assert_ne!(a.credential_fingerprint(), b.credential_fingerprint());
        assert_eq!(a.server_url(), b.server_url(), "same base URL is shared");
        // Proof of token-opacity: the fingerprint is a raw SHA-256 hex of the secret
        // bytes (64 hex chars), never a decoded JWT claim.
        assert_eq!(a.credential_fingerprint().len(), 64);
        assert!(
            a.credential_fingerprint()
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        );
    }

    #[test]
    fn identical_key_reuses_one_pooled_producer() {
        let bifrost = pool(Arc::new(MockSink::new()), QueueConfig::default());
        let schema = test_schema();

        bifrost
            .insert("ns.tbl", &schema, row(), Some(card()), None)
            .expect("first accepted");
        bifrost
            .insert("ns.tbl", &schema, row(), Some(card()), None)
            .expect("second accepted");
        assert_eq!(
            bifrost.producer_count(),
            1,
            "identical (scope, kind, table) reuses the pooled producer"
        );

        bifrost
            .insert("ns.other", &schema, row(), Some(card()), None)
            .expect("distinct table accepted");
        assert_eq!(bifrost.producer_count(), 2, "a new table is a new producer");
    }

    /// An omitted `card_ref` is a valid write, not a client-side refusal.
    ///
    /// The server stores an uncorrelated row against the authenticated
    /// principal with a null `card_uid`; the pool must reach it, so this proves
    /// the client no longer forces a correlation the wire never required.
    #[test]
    fn omitted_card_ref_is_accepted() {
        let bifrost = pool(Arc::new(MockSink::new()), QueueConfig::default());
        bifrost
            .insert("ns.tbl", &test_schema(), row(), None, None)
            .expect("an uncorrelated row is a valid write");
    }

    /// Refuses a distinct producer before the handle grows beyond its configured cap.
    #[test]
    fn bifrost_producer_cap_refuses_before_registry_growth() {
        let bifrost = pool(
            Arc::new(MockSink::new()),
            QueueConfig {
                max_producers: 1,
                ..QueueConfig::default()
            },
        );
        let schema = test_schema();
        bifrost
            .insert("ns.first", &schema, row(), Some(card()), None)
            .expect("first producer accepted");
        assert!(matches!(
            bifrost.insert("ns.second", &schema, row(), Some(card()), None),
            Err(wyrd_queue::WyrdQueueError::Backpressure)
        ));
        assert_eq!(
            bifrost.producer_count(),
            1,
            "rejection precedes pool growth"
        );
    }

    /// Admits at most the default 64 producer envelopes inside one 32 MiB owner.
    #[test]
    fn default_producer_envelopes_charge_before_registry_growth() {
        let bifrost = pool(
            Arc::new(MockSink::new()),
            QueueConfig {
                // Disable timer work so the test isolates default cardinality
                // admission rather than concurrent frame construction.
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
        );
        let schema = test_schema();
        for index in 0..QueueConfig::MAX_LIVE_ENTRIES {
            bifrost
                .insert(
                    &format!("ns.capacity_{index}"),
                    &schema,
                    row(),
                    Some(card()),
                    None,
                )
                .expect("default producer envelope fits before construction");
        }
        let admitted = bifrost.metrics();
        assert_eq!(admitted.producers, QueueConfig::MAX_LIVE_ENTRIES);
        assert!(
            admitted.total_reserved_bytes <= QueueConfig::MAX_CLIENT_BYTE_LIMIT,
            "fixed and dynamic ownership stays inside the one 32 MiB budget: {admitted:?}"
        );
        assert!(matches!(
            bifrost.insert("ns.capacity_overflow", &schema, row(), Some(card()), None),
            Err(wyrd_queue::WyrdQueueError::Backpressure)
        ));
        let refused = bifrost.metrics();
        assert_eq!(
            refused.producers,
            QueueConfig::MAX_LIVE_ENTRIES,
            "refusal occurs before the registry can grow"
        );
        assert!(
            refused.total_reserved_bytes <= QueueConfig::MAX_CLIENT_BYTE_LIMIT,
            "refusal cannot oversubscribe the owner: {refused:?}"
        );
        let shutdown = bifrost.shutdown();
        assert!(
            shutdown.is_ok(),
            "default producer cleanup: {shutdown:?}; metrics={:?}",
            bifrost.metrics()
        );
        assert_eq!(
            bifrost.metrics().total_reserved_bytes,
            0,
            "shutdown removes producer fixed-storage charges"
        );
    }

    /// A saturated queue reaches the caller of the public client as a stable
    /// error, rather than being counted and swallowed.
    #[test]
    fn insert_propagates_queue_full() {
        let sink = Arc::new(StallSink::default());
        let bifrost = client_over(sink.clone(), saturating_config());
        bifrost.use_table(table("ns.tbl"));

        bifrost
            .insert(row(), correlated())
            .expect("first enqueue accepted");
        wait_until_started(&sink);

        let mut rejected = 0;
        for i in 0..40 {
            let payload = format!(r#"{{"id": {}}}"#, i + 1).into_bytes();
            if let Err(err) = bifrost.insert(payload, correlated()) {
                let projected = wyrd_spec::error::WyrdError::from(&err);
                assert_eq!(
                    projected.code(),
                    "WYRD_CLIENT_429_QUEUE_FULL",
                    "insert must surface queue-full to the caller"
                );
                assert_eq!(projected.status(), 429);
                rejected += 1;
            }
        }
        assert!(
            rejected > 0,
            "a saturated queue must reject on the write path"
        );
        assert_eq!(
            bifrost.dropped(),
            0,
            "the explicit write path refuses; it never drops"
        );
    }

    /// The Rust model door reaches exactly the columns the shared JSON-Schema
    /// mapper produces.
    ///
    /// `from_model` is the Rust twin of the Pydantic and Zod paths, so the
    /// proof that matters is that a `schemars`-derived type and the same
    /// document handed to `from_json_schema` land on one Arrow schema rather
    /// than two mappings that agree only today.
    #[test]
    fn table_config_from_a_schemars_model_matches_its_json_schema() {
        /// The user columns a Rust writer declares through `schemars`.
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code, reason = "fields exist to be reflected, never read")]
        struct Prediction {
            /// The model that produced the row.
            model: String,
            /// The scored value.
            score: f64,
            /// Billed tokens.
            tokens: i64,
        }

        let from_model = TableConfig::from_model::<Prediction>("genai.predictions")
            .expect("a flat schemars model declares a table");
        let document =
            serde_json::to_value(schemars::schema_for!(Prediction)).expect("model schema is JSON");
        let from_document = TableConfig::from_json_schema("genai.predictions", &document)
            .expect("the same document declares the same table");

        assert_eq!(from_model.fqn(), "genai.predictions");
        assert_eq!(from_model.user_schema(), from_document.user_schema());
        assert_eq!(
            from_model
                .user_schema()
                .fields()
                .iter()
                .map(|field| (field.name().as_str(), field.data_type().clone()))
                .collect::<Vec<_>>(),
            vec![
                ("model", DataType::Utf8),
                ("score", DataType::Float64),
                ("tokens", DataType::Int64),
            ],
            "the model door maps through the one shared JSON-Schema table"
        );
    }

    /// A write with nothing bound refuses instead of guessing a destination.
    #[test]
    fn insert_without_an_active_table_refuses() {
        let bifrost = client_over(Arc::new(MockSink::new()), QueueConfig::default());
        assert!(bifrost.table().is_none(), "a fresh client binds no table");
        let error = bifrost
            .insert(row(), Correlation::default())
            .expect_err("an unbound write must refuse");
        let projected = wyrd_spec::error::WyrdError::from(&error);
        assert_eq!(projected.code(), "WYRD_VALA_412_NO_ACTIVE_TABLE");
        assert_eq!(projected.status(), 412);
        // One catalog owner: the SDK reports the derive-backed metadata rather
        // than a second hand-written copy that could drift from it.
        let catalog = wyrd_spec::vala::error::BifrostError::NoActiveTable;
        assert_eq!(
            (
                projected.code(),
                projected.status(),
                projected.title(),
                projected.remediation()
            ),
            (
                catalog.code(),
                catalog.status(),
                catalog.title(),
                catalog.remediation()
            ),
            "no-active-table metadata must come from the wyrd-spec catalog"
        );
        assert_eq!(
            bifrost.producer_count(),
            0,
            "a refused write builds no producer"
        );
    }

    /// Swapping the active table keeps the previous table's producer, so its
    /// buffered rows still drain on the next flush.
    #[test]
    fn swapping_the_active_table_keeps_the_previous_producer() {
        let bifrost = client_over(Arc::new(MockSink::new()), QueueConfig::default());

        assert!(
            bifrost.use_table(table("ns.first")).is_none(),
            "the first binding replaces nothing"
        );
        bifrost.insert(row(), correlated()).expect("first accepted");

        let previous = bifrost
            .use_table(table("ns.second"))
            .expect("the swap returns the previous binding");
        assert_eq!(previous.fqn(), "ns.first");
        assert_eq!(
            bifrost.table().expect("a table stays bound").fqn(),
            "ns.second"
        );

        bifrost
            .insert(row(), correlated())
            .expect("second accepted");
        assert_eq!(
            bifrost.producer_count(),
            2,
            "the swapped-away table keeps its producer and its buffered rows"
        );
    }

    /// Flush visits every producer and shutdown releases fixed storage after terminal settlement.
    #[test]
    fn lifecycle_drains_all_producers_after_first_error() {
        let sink = Arc::new(LifecycleSink::default());
        let bifrost = pool(
            Arc::clone(&sink) as Arc<dyn BatchSink<ClientByteGuard>>,
            QueueConfig {
                flush_max_rows: 2,
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
        );
        let schema = test_schema();
        bifrost
            .insert("b", &schema, row(), Some(card()), None)
            .expect("later producer accepted");
        bifrost
            .insert("a", &schema, row(), Some(card()), None)
            .expect("earlier producer accepted");

        let flush_error = bifrost.flush().expect_err("a producer must fail");
        assert!(flush_error.to_string().contains("a producer failed"));
        let attempts_after_flush = sink.attempts.lock().expect("attempts lock").clone();
        assert!(attempts_after_flush.iter().any(|table| table == "a"));
        assert!(attempts_after_flush.iter().any(|table| table == "b"));

        bifrost
            .shutdown()
            .expect("terminally settled producers release their fixed storage on shutdown");
        assert!(
            bifrost
                .insert("b", &schema, row(), Some(card()), None)
                .expect_err("shutdown stops later producer")
                .to_string()
                .contains("queue full")
        );
    }

    #[test]
    fn observe_record_swallows_and_counts_overflow() {
        let sink = Arc::new(StallSink::default());
        let bifrost = client_over(sink.clone(), saturating_config());
        let schema = test_schema();

        // Prime the stall, then flood the same saturated producer via the telemetry
        // path. `record` returns unit — the caller is never handed an error.
        observe::record(&bifrost, "ns.tbl", &schema, row(), correlated());
        wait_until_started(&sink);

        for i in 0..40 {
            let payload = format!(r#"{{"id": {}}}"#, i + 1).into_bytes();
            observe::record(&bifrost, "ns.tbl", &schema, payload, correlated());
        }
        assert!(
            bifrost.dropped() > 0,
            "overflow on the telemetry path is counted, never surfaced"
        );
    }

    struct RecordingTransport {
        seen: Mutex<Vec<[u8; 16]>>,
    }

    #[async_trait]
    impl IngestTransport<ClientByteGuard> for RecordingTransport {
        async fn insert_batch(
            &self,
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
            self.seen.lock().expect("poisoned").push(batch.batch_id);
            Ok(DurableBatchAck {
                batch_id: batch.batch_id,
                rows: batch.rows,
            })
        }
    }

    #[test]
    fn ingest_sink_forwards_batch_id_unchanged() {
        let transport = Arc::new(RecordingTransport {
            seen: Mutex::new(Vec::new()),
        });
        let sink = BifrostIngestSink::new(transport.clone());
        let budget = ClientByteBudget::new(1024);
        let batch = || SealedBatch {
            table: "ns.tbl".to_owned(),
            batch_id: [7u8; 16],
            frame: OwnedIpcBytes::new(
                vec![1, 2, 3],
                budget.reserve_sealed(3).expect("batch budget"),
            ),
            rows: 1,
        };

        let rt = wyrd_runtime::runtime();
        let batch = batch();
        rt.block_on(sink.send(&batch)).expect("first send");
        rt.block_on(sink.send(&batch)).expect("retry send");

        let seen = transport.seen.lock().expect("poisoned");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], [7u8; 16]);
        assert_eq!(
            seen[1], [7u8; 16],
            "retry preserves batch_id for server dedup"
        );
    }

    /// A JSON-Schema model becomes the config's declared user columns.
    #[test]
    fn table_config_from_json_schema_declares_the_model_columns() {
        let json = serde_json::json!({
            "type": "object",
            "properties": { "id": { "type": "integer" } },
            "required": ["id"],
        });
        let config = TableConfig::from_json_schema("ns.tbl", &json).expect("schema builds");
        assert_eq!(config.fqn(), "ns.tbl");
        assert_eq!(config.user_schema().fields().len(), 1);
        assert_eq!(config.user_schema().field(0).name(), "id");
        assert!(
            config.resolved().is_none(),
            "identity is server-minted; a declared config is inert"
        );
    }

    /// A name that is not `<namespace>.<name>` is refused before any IO.
    #[test]
    fn table_config_requires_a_namespaced_name() {
        let error = TableConfig::from_arrow("predictions", test_schema())
            .expect_err("an unqualified name must refuse");
        assert_eq!(
            wyrd_spec::error::WyrdError::from(&error).code(),
            "WYRD_VALA_400_SCHEMA_PARSE"
        );
    }

    /// A server-owned column may not be declared as a user column.
    #[test]
    fn table_config_rejects_a_reserved_column() {
        let schema: SchemaRef = Arc::new(Schema::new(vec![Field::new(
            "card_ref",
            DataType::Utf8,
            true,
        )]));
        let error = TableConfig::from_arrow("ns.tbl", schema)
            .expect_err("a correlation column is not a user column");
        assert_eq!(
            wyrd_spec::error::WyrdError::from(&error).code(),
            "WYRD_VALA_400_BIFROST_RESERVED_COLUMN"
        );
    }
}

#[cfg(test)]
mod bounded_ipc_tests {
    //! Direct Vala consumer proof for the shared bounded Arrow IPC owner.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use wyrd_queue::{ArrowIpcMaterialFacts, BoundedArrowIpc};

    /// Client-owned test guard that records terminal release of admitted bytes.
    struct ClientGuard {
        /// Bytes charged to the simulated handle-wide client budget.
        bytes: usize,
        /// Shared counter used to prove exact terminal settlement.
        released: Arc<AtomicUsize>,
    }

    impl Drop for ClientGuard {
        /// Releases the guard's complete charge exactly once at the terminal owner.
        fn drop(&mut self) {
            self.released.fetch_add(self.bytes, Ordering::SeqCst);
        }
    }

    /// Vala retains the opaque client guard while filling and transferring fixed IPC storage.
    #[test]
    fn bounded_ipc_retains_client_authority_through_transfer() {
        let released = Arc::new(AtomicUsize::new(0));
        let mut reserved = 0;
        let facts = ArrowIpcMaterialFacts {
            retained_input_bytes: 32,
            destination_values_bytes: 16,
            destination_offsets_bytes: 8,
            destination_validity_bits: 8,
            ipc_metadata_bytes: 9,
            ipc_body_bytes: 17,
            ipc_prefix_bytes: 8,
            ipc_alignment: 8,
        };
        let mut owned = BoundedArrowIpc::try_new(facts, |peak| {
            reserved = peak;
            Ok::<_, ()>(ClientGuard {
                bytes: peak,
                released: Arc::clone(&released),
            })
        })
        .expect("client budget admits complete peak");

        assert_eq!(reserved, owned.plan().simultaneous_peak_bytes());
        owned.ipc_mut().fill(0x5a);
        let mut transferred = transfer(owned);
        assert_eq!(released.load(Ordering::SeqCst), 0);
        assert!(transferred.ipc_mut().iter().all(|byte| *byte == 0x5a));
        drop(transferred);
        assert_eq!(released.load(Ordering::SeqCst), reserved);
    }

    /// Moves the shared bounded owner through the Vala consumer boundary intact.
    fn transfer<G>(owned: BoundedArrowIpc<G>) -> BoundedArrowIpc<G> {
        owned
    }
}
