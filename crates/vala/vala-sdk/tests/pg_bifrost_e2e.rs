//! Rust e2e mirror for the happy-path and backpressure/drain journeys.
//!
//! These tests require a live `WyrdTestServer` (embedded Postgres + real server
//! socket), so they live in `mod pg_tests`: the fast family lane skips them via
//! `--skip pg_tests`; `mise run test:e2e` (Postgres up) runs them.
//!
//! The suite combines mock-sink queue saturation checks with real gRPC ingest
//! journeys. The timeout journey uses a delayed server WAL to prove that the
//! public SDK retains its owned batch through ambiguous transport settlement.

mod pg_tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use arrow::array::{Int64Array, StringArray};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use secrecy::ExposeSecret;
    use tokio::sync::Notify;
    use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_sdk::{
        Bifrost, BifrostGrpcTransport, BifrostIngestSink, BifrostTransportConfig, ClientScope,
        IngestTransport, QueryClient, SinkKind, observe,
    };
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_queue::{
        BatchSink, ClientByteGuard, DurableBatchAck, MockSink, QueueConfig, SealedBatch, SinkError,
        WyrdQueueError,
    };
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
    use wyrd_testing::server::WyrdTestServer;

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
        ]))
    }

    fn card() -> CardRef {
        "prod/Service/genai_pipeline@1.0.0"
            .parse()
            .expect("valid card ref")
    }

    fn row(i: i64) -> Vec<u8> {
        format!(r#"{{"id": {i}, "value": "batch"}}"#).into_bytes()
    }

    fn client_config(srv: &WyrdTestServer) -> ClientConfig {
        ClientConfig {
            http: HttpConfig {
                base_url: srv.base_url().unwrap_or("").to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(srv.api_key().expose_secret().to_owned().into()),
            ..ClientConfig::default()
        }
    }

    /// A stall sink that parks forever once a batch arrives — used to saturate the
    /// bounded producer channel for backpressure tests.
    #[derive(Default)]
    struct StallSink {
        /// Signals that at least one producer reached the downstream sink.
        started: AtomicBool,
        /// Allows blocked producers to finish before the test server shuts down.
        released: AtomicBool,
        /// Wakes blocked producers after [`Self::release`] is called.
        wake: Notify,
    }

    impl StallSink {
        /// Release all batches currently parked at the sink.
        fn release(&self) {
            self.released.store(true, Ordering::SeqCst);
            self.wake.notify_waiters();
        }
    }

    #[async_trait]
    impl BatchSink<ClientByteGuard> for StallSink {
        /// Block accepted batches until the test explicitly releases the sink.
        ///
        /// # Errors
        /// This test sink has no error condition and always returns `Ok` after
        /// release; the result type is required by the [`BatchSink`] contract.
        async fn send(
            &self,
            batch: SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError<ClientByteGuard>> {
            self.started.store(true, Ordering::SeqCst);
            while !self.released.load(Ordering::SeqCst) {
                let notified = self.wake.notified();
                if self.released.load(Ordering::SeqCst) {
                    break;
                }
                notified.await;
            }
            Ok(DurableBatchAck {
                batch_id: batch.batch_id,
                rows: batch.rows,
            })
        }
    }

    #[derive(Default)]
    struct ReleasingSink {
        started: AtomicBool,
        released: AtomicBool,
        sent_rows: AtomicUsize,
        wake: Notify,
    }

    #[async_trait]
    impl BatchSink<ClientByteGuard> for ReleasingSink {
        async fn send(
            &self,
            batch: SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError<ClientByteGuard>> {
            self.started.store(true, Ordering::SeqCst);
            loop {
                if self.released.load(Ordering::SeqCst) {
                    let rows = usize::try_from(batch.rows).unwrap_or(usize::MAX);
                    self.sent_rows.fetch_add(rows, Ordering::SeqCst);
                    return Ok(DurableBatchAck {
                        batch_id: batch.batch_id,
                        rows: batch.rows,
                    });
                }
                let notified = self.wake.notified();
                if self.released.load(Ordering::SeqCst) {
                    continue;
                }
                notified.await;
            }
        }
    }

    fn wait_until_started(sink: &StallSink) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !sink.started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "stall sink never started");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_until_sent(sink: &ReleasingSink, rows: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while sink.sent_rows.load(Ordering::SeqCst) < rows {
            assert!(Instant::now() < deadline, "released sink did not drain");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

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

    fn native_ipc() -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![7, 8])),
                Arc::new(StringArray::from(vec!["first", "second"])),
            ],
        )
        .expect("valid native SDK batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
        writer.write(&batch).expect("write IPC batch");
        writer.finish().expect("finish IPC stream");
        bytes
    }

    /// Real gRPC transport wrapper that records the batch identity at the SDK sink seam.
    ///
    /// The wrapper performs no retry or payload transformation: it forwards the
    /// exact non-cloneable sealed batch to [`BifrostGrpcTransport`] so the journey
    /// can prove the queue returns the same owner after an ambiguous deadline.
    struct RecordingTransport {
        /// The authenticated real gRPC transport under test.
        inner: BifrostGrpcTransport,
        /// Batch IDs and owned-byte addresses observed before each real attempt.
        attempts: Mutex<Vec<([u8; 16], usize)>>,
    }

    impl RecordingTransport {
        /// Builds a recording wrapper around the real ordinary-Rust gRPC transport.
        #[must_use]
        fn new(inner: BifrostGrpcTransport) -> Self {
            Self {
                inner,
                attempts: Mutex::new(Vec::new()),
            }
        }

        /// Returns ordered `(batch ID, byte address)` observations at the sink boundary.
        ///
        /// # Panics
        ///
        /// Panics if the test-only attempt recorder mutex is poisoned.
        fn attempts(&self) -> Vec<([u8; 16], usize)> {
            self.attempts
                .lock()
                .expect("attempt recorder lock poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl IngestTransport<ClientByteGuard> for RecordingTransport {
        /// Records then forwards the exact owned batch through the real gRPC path.
        ///
        /// # Errors
        ///
        /// Propagates the real transport outcome, including its retained owner
        /// for an ambiguous deadline.
        ///
        /// # Panics
        ///
        /// Panics if the test-only attempt recorder mutex is poisoned.
        async fn insert_batch(
            &self,
            batch: SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError<ClientByteGuard>> {
            let byte_address = batch.bytes().as_ptr() as usize;
            self.attempts
                .lock()
                .expect("attempt recorder lock poisoned")
                .push((batch.batch_id, byte_address));
            IngestTransport::insert_batch(&self.inner, batch).await
        }
    }

    /// Runs the public Rust SDK through HTTP Gate and Oracle after a real gRPC ingest.
    #[tokio::test]
    async fn oracle_query_returns_arrow_batches_and_terminal() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let table_name = format!("sdk_oracle_{}", uuid::Uuid::now_v7().simple());
        let table_fqn = format!("vala.bifrost.{table_name}");
        srv.state()
            .bifrost
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                audit: None,
            })
            .await
            .expect("register Oracle journey table");
        let bootstrap = srv
            .bootstrap_service("sdk-oracle-query", &["admin"])
            .await
            .expect("bootstrap SDK caller");
        let mut config = ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        };
        config.grpc.max_message_bytes = 32 * 1024 * 1024;
        let client = WyrdClient::with_config(config).expect("public SDK client");
        BifrostGrpcTransport::connect(&client)
            .await
            .expect("connect ingest")
            .insert_batch(&table_fqn, uuid::Uuid::now_v7().into_bytes(), native_ipc())
            .await
            .expect("durable ingest ACK");
        srv.flush_bifrost().await.expect("flush Scribe");

        let request = BifrostQueryRequest {
            sql: format!("SELECT id, value FROM {table_fqn} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        };
        let mut stream = QueryClient::new(&client)
            .query(&request)
            .await
            .expect("Oracle query starts");
        let mut rows = 0;
        while let Some(batch) = stream.next_batch().await.expect("valid terminal stream") {
            rows += batch.num_rows();
        }
        assert_eq!(rows, 2);
        assert_eq!(stream.terminal().expect("validated terminal").row_count, 2);
        srv.shutdown().await.expect("server shutdown");
    }

    #[tokio::test]
    async fn public_sdk_bidi_write_ack_and_durable_readback() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let table_name = format!("sdk_roundtrip_{}", uuid::Uuid::now_v7().simple());
        srv.state()
            .bifrost
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                audit: None,
            })
            .await
            .expect("register SDK journey table");

        let bootstrap = srv
            .bootstrap_service("sdk-bifrost-writer", &["admin"])
            .await
            .expect("bootstrap SDK writer");
        let api_key = bootstrap.api_key().expect("machine API key").clone();
        let mut config = ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(api_key),
            ..ClientConfig::default()
        };
        config.grpc.connect_retries = 0;
        let client = WyrdClient::with_config(config).expect("SDK client");
        let transport = BifrostGrpcTransport::connect(&client)
            .await
            .expect("connect public SDK transport");
        let batch_id = uuid::Uuid::now_v7().into_bytes();
        transport
            .insert_batch(
                &format!("vala.bifrost.{table_name}"),
                batch_id,
                native_ipc(),
            )
            .await
            .expect("durable batch ACK");

        srv.flush_bifrost()
            .await
            .expect("flush server-owned Scribe");
        let tenant = srv.data_tenant_id();
        let mut conn = srv
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped read connection");
        let row_count: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE namespace = 'vala.bifrost' AND table_name = $1",
        )
        .bind(&table_name)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("read Scribe file-list rows");
        assert_eq!(row_count, 2, "durable read boundary contains exact rows");
        let file_path: String = sqlx::query_scalar(
            "SELECT file_path
               FROM vala.file_list
              WHERE namespace = 'vala.bifrost' AND table_name = $1
              ORDER BY file_path
              LIMIT 1",
        )
        .bind(&table_name)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("read sealed file path");
        conn.commit().await.expect("commit tenant-scoped read");
        srv.state()
            .storage
            .operator()
            .stat(&file_path)
            .await
            .expect("sealed Parquet object exists");
        srv.shutdown().await.expect("server shutdown");
    }

    /// Proves that an owned SDK batch survives an ambiguous post-receipt deadline,
    /// retries with its original UUIDv7, and settles after the server deduplicates
    /// the already-durable append.
    #[tokio::test]
    async fn public_sdk_owned_batch_timeout_retry_deduplicates_and_settles() {
        let srv = WyrdTestServer::builder()
            .with_wal_sync_delay(Duration::from_millis(250))
            .start_bound()
            .await
            .expect("delayed-WAL test server start");
        let table_name = format!("sdk_timeout_retry_{}", uuid::Uuid::now_v7().simple());
        let table_fqn = format!("vala.bifrost.{table_name}");
        srv.state()
            .bifrost
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                audit: None,
            })
            .await
            .expect("register timeout-retry table");

        let bootstrap = srv
            .bootstrap_service("sdk-timeout-retry-writer", &["admin"])
            .await
            .expect("bootstrap timeout-retry writer");
        let target = bootstrap
            .card_ref()
            .expect("service bootstrap has a card scope")
            .clone();
        let config = ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                timeout_ms: 25,
                connect_retries: 0,
                max_message_bytes: 32 * 1024 * 1024,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        };
        let scope = ClientScope::from_config(&config).expect("public SDK scope");
        let client = WyrdClient::with_config(config).expect("public SDK client");
        let transport = BifrostGrpcTransport::connect_with_config(
            &client,
            BifrostTransportConfig::with_max_frame_retries(0),
        )
        .await
        .expect("connect timeout-retry transport");
        let recording = Arc::new(RecordingTransport::new(transport));
        let bifrost = Arc::new(Bifrost::new(
            scope,
            Arc::new(BifrostIngestSink::new(recording.clone())),
            QueueConfig {
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
        ));

        bifrost
            .insert(
                SinkKind::Record,
                &table_fqn,
                &schema(),
                row(41),
                target,
                None,
            )
            .expect("enqueue one owned JSON row");
        let first_error = tokio::task::spawn_blocking({
            let bifrost = Arc::clone(&bifrost);
            move || bifrost.flush()
        })
        .await
        .expect("first flush task joins")
        .expect_err("short deadline after server receipt must leave the durable result ambiguous");
        let WyrdQueueError::Sink(first_sink_error) = &first_error else {
            panic!("the queue must receive an ambiguous sink result: {first_error}");
        };
        assert!(
            matches!(
                first_sink_error,
                wyrd_spec::error::WyrdError::ServiceUnavailable { .. }
            ),
            "the deadline must be projected as a retryable ambiguous result: {first_sink_error:?}"
        );
        let retained = bifrost.metrics();
        assert!(
            retained.owned_bytes > 0,
            "retained batch still owns its bytes: {retained:?}"
        );
        assert_eq!(
            retained.live_batches, 1,
            "one sealed owner is retained: {retained:?}"
        );
        assert_eq!(
            retained.retry_entries, 1,
            "one retry entry retains that owner: {retained:?}"
        );
        assert_eq!(
            retained.pending_controls, 0,
            "the failed flush released its control slot"
        );

        tokio::time::sleep(Duration::from_millis(350)).await;
        tokio::task::spawn_blocking({
            let bifrost = Arc::clone(&bifrost);
            move || bifrost.flush()
        })
        .await
        .expect("retry flush task joins")
        .expect("retry resolves the post-receipt ambiguity through durable dedup");
        let attempts = recording.attempts();
        assert_eq!(attempts.len(), 2, "one deadline then one resolving retry");
        assert_eq!(
            attempts[0].0, attempts[1].0,
            "the public sink retried the exact same stable batch ID"
        );
        assert_eq!(
            attempts[0].1, attempts[1].1,
            "the retained batch reused its owned-byte allocation without copying"
        );
        let settled = bifrost.metrics();
        assert_eq!(settled.owned_bytes, 0, "durable ACK releases client bytes");
        assert_eq!(
            settled.live_batches, 0,
            "durable ACK releases the live batch slot"
        );
        assert_eq!(
            settled.retry_entries, 0,
            "durable ACK releases the retry slot"
        );
        assert_eq!(
            settled.pending_controls, 0,
            "durable ACK leaves no pending control"
        );

        srv.flush_bifrost()
            .await
            .expect("flush the server-owned durable append");
        let tenant = srv.data_tenant_id();
        let mut conn = srv
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped read connection");
        let row_count: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE namespace = 'vala.bifrost' AND table_name = $1",
        )
        .bind(&table_name)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("read durable deduplicated rows");
        assert_eq!(row_count, 1, "retry did not create a duplicate durable row");
        conn.commit().await.expect("commit tenant-scoped read");
        tokio::task::spawn_blocking({
            let bifrost = Arc::clone(&bifrost);
            move || bifrost.shutdown()
        })
        .await
        .expect("shutdown task joins")
        .expect("settled producer shutdown");
        srv.shutdown().await.expect("server shutdown");
    }

    /// Happy-path: insert via Bifrost handle + observe path, no drops, real server
    /// credentials validate the scope construction path.
    #[tokio::test]
    async fn observe_and_bifrost_roundtrip() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let config = client_config(&srv);
        let scope = ClientScope::from_config(&config).expect("scope");
        let bifrost = Bifrost::new(scope, Arc::new(MockSink::new()), QueueConfig::default());
        let schema = schema();
        let target = card();

        for i in 0..500i64 {
            bifrost
                .insert(
                    SinkKind::Record,
                    "test.roundtrip",
                    &schema,
                    row(i),
                    target.clone(),
                    None,
                )
                .expect("insert via Bifrost handle");
        }
        assert_eq!(
            bifrost.dropped(),
            0,
            "Bifrost handle: no drops on happy path"
        );

        for i in 0..500i64 {
            observe::record(
                &bifrost,
                SinkKind::Record,
                "test.roundtrip",
                &schema,
                row(i),
                target.clone(),
                None,
            );
        }
        assert_eq!(bifrost.dropped(), 0, "observe path: no drops on happy path");
        assert_eq!(bifrost.producer_count(), 1, "one producer for one table");

        srv.shutdown().await.expect("server shutdown");
    }

    /// Backpressure + drain: a saturated queue propagates WYRD_CLIENT_429_QUEUE_FULL
    /// from the insert path; observe swallows and counts.
    #[tokio::test]
    async fn backpressure_and_drain_no_silent_drops() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let config = client_config(&srv);
        let scope = ClientScope::from_config(&config).expect("scope");
        let stall = Arc::new(StallSink::default());
        let bifrost = Bifrost::new(scope, stall.clone(), saturating_config());
        let schema = schema();
        let target = card();

        // Prime the stall: first insert starts draining then parks forever.
        bifrost
            .insert(
                SinkKind::Record,
                "test.backpressure",
                &schema,
                row(0),
                target.clone(),
                None,
            )
            .expect("first accepted");
        wait_until_started(&stall);

        // Flood until rejection.
        let mut saw_queue_full = false;
        for i in 1..=100 {
            match bifrost.insert(
                SinkKind::Record,
                "test.backpressure",
                &schema,
                row(i),
                target.clone(),
                None,
            ) {
                Ok(()) => {}
                Err(WyrdQueueError::QueueFull) => {
                    saw_queue_full = true;
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
        assert!(
            saw_queue_full,
            "saturated queue must propagate WYRD_CLIENT_429_QUEUE_FULL"
        );

        // Observe path swallows queue-full.
        let scope2 = ClientScope::from_config(&client_config(&srv)).expect("scope2");
        let bifrost2 = Bifrost::new(scope2, stall.clone(), saturating_config());
        observe::record(
            &bifrost2,
            SinkKind::Record,
            "test.backpressure.obs",
            &schema,
            row(0),
            target.clone(),
            None,
        );
        wait_until_started(&stall);
        for i in 1..=100 {
            observe::record(
                &bifrost2,
                SinkKind::Record,
                "test.backpressure.obs",
                &schema,
                row(i),
                target.clone(),
                None,
            );
        }
        assert!(
            bifrost2.dropped() > 0,
            "observe path: drops counted on saturation"
        );

        // Release both producers before dropping the handles so their queue
        // tasks can drain and exit instead of surviving into the next journey.
        stall.release();
        drop(bifrost2);
        drop(bifrost);
        srv.shutdown().await.expect("server shutdown");
    }

    #[tokio::test]
    async fn downstream_stall_remains_bounded_and_recovers_after_drain() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let scope = ClientScope::from_config(&client_config(&srv)).expect("scope");
        let sink = Arc::new(ReleasingSink::default());
        let bifrost = Bifrost::new(scope, sink.clone(), saturating_config());
        let schema = schema();
        let target = card();

        bifrost
            .insert(
                SinkKind::Record,
                "test.downstream_stall",
                &schema,
                row(0),
                target.clone(),
                None,
            )
            .expect("first accepted");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !sink.started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "releasing sink never started");
            std::thread::sleep(Duration::from_millis(5));
        }

        let mut accepted = 1_usize;
        let mut rejected = 0_usize;
        for i in 1..=100 {
            match bifrost.insert(
                SinkKind::Record,
                "test.downstream_stall",
                &schema,
                row(i),
                target.clone(),
                None,
            ) {
                Ok(()) => accepted += 1,
                Err(WyrdQueueError::QueueFull) => rejected += 1,
                Err(error) => panic!("unexpected queue error: {error}"),
            }
        }
        assert!(
            rejected > 0,
            "downstream stall must produce bounded rejection"
        );

        sink.released.store(true, Ordering::SeqCst);
        sink.wake.notify_waiters();
        wait_until_sent(&sink, accepted);

        bifrost
            .insert(
                SinkKind::Record,
                "test.downstream_stall",
                &schema,
                row(101),
                target,
                None,
            )
            .expect("post-drain write accepted");
        wait_until_sent(&sink, accepted + 1);
        assert_eq!(sink.sent_rows.load(Ordering::SeqCst), accepted + 1);

        drop(bifrost);
        srv.shutdown().await.expect("server shutdown");
    }
}
