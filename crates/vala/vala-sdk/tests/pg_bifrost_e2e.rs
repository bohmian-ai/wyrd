//! Rust e2e mirror for the happy-path and backpressure/drain journeys.
//!
//! These tests require a live `WyrdTestServer` (embedded Postgres + real server
//! socket), so they live in `mod pg_tests`: the fast family lane skips them via
//! `--skip pg_tests`; `mise run test:e2e` (Postgres up) runs them.
//!
//! The current write path drains into a `MockSink` — the gRPC ingest transport
//! is wired in a later stage. Until then these tests exercise the SDK lifecycle,
//! producer-pool identity, and the asymmetric backpressure contract against
//! real server credentials.

mod pg_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use arrow::array::{Int64Array, StringArray};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use secrecy::ExposeSecret;
    use tokio::sync::Notify;
    use vala_bifrost::catalog::CreateTableRequest;
    use vala_bifrost::catalog::namespaces::BifrostNamespace;
    use vala_bifrost::types::TableScope;
    use vala_sdk::{
        Bifrost, BifrostGrpcTransport, ClientScope, IngestTransport, SinkKind, observe,
    };
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_queue::{BatchSink, MockSink, QueueConfig, SealedBatch, WyrdQueueError};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::reference::CardRef;
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
        started: AtomicBool,
    }

    #[async_trait]
    impl BatchSink for StallSink {
        async fn send(&self, _batch: SealedBatch) -> Result<u64, WyrdError> {
            self.started.store(true, Ordering::SeqCst);
            std::future::pending::<()>().await;
            unreachable!()
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
    impl BatchSink for ReleasingSink {
        async fn send(&self, batch: SealedBatch) -> Result<u64, WyrdError> {
            self.started.store(true, Ordering::SeqCst);
            loop {
                if self.released.load(Ordering::SeqCst) {
                    let rows = usize::try_from(batch.rows).unwrap_or(usize::MAX);
                    self.sent_rows.fetch_add(rows, Ordering::SeqCst);
                    return Ok(batch.rows);
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

    #[tokio::test]
    async fn public_sdk_bidi_write_ack_and_durable_readback() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let table_name = format!("sdk_roundtrip_{}", uuid::Uuid::now_v7().simple());
        srv.state()
            .bifrost
            .create_table(CreateTableRequest {
                ns: BifrostNamespace::Bifrost,
                name: &table_name,
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                scope: TableScope::TenantOwned,
                tenant: srv.data_tenant_id(),
                partition_columns: &[],
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
