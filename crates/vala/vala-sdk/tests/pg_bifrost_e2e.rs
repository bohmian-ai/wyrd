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

    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use secrecy::ExposeSecret;
    use tokio::sync::Notify;
    use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::resources::ORACLE_MAX_BATCH_SIZE;
    use vala_sdk::{
        Bifrost, BifrostGrpcTransport, BifrostIngestSink, BifrostTransportConfig, Correlation,
        IngestTransport, QueryClient, TableConfig, observe,
    };
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_queue::{
        BatchSink, ClientByteGuard, DurableBatchAck, MockSink, QueueConfig, SealedBatch, SinkError,
        WyrdQueueError,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, RegisterOutcome, VisibilityMode,
    };
    use wyrd_testing::bifrost::write::{BifrostWriter, RawIngest};
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
            credential: Some(srv.api_key().expose_secret().to_owned().into()),
            ..ClientConfig::default()
        }
    }

    /// The `id`/`value` table declaration these journeys write through.
    fn table(fqn: &str) -> TableConfig {
        TableConfig::from_arrow(fqn, schema()).expect("declared journey table")
    }

    /// One row correlated to the journey card.
    fn correlated() -> Correlation {
        Correlation {
            card_ref: Some(card()),
            run_id: None,
        }
    }

    /// A public client over `sink`, bound to `fqn`.
    ///
    /// `with_sink` is the same seam production uses for its gRPC sink, so the
    /// pooling, backpressure, and drop-counting behavior under test is the
    /// production behavior; only the destination is a mock.
    fn client_over(
        srv: &WyrdTestServer,
        fqn: &str,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
    ) -> Bifrost {
        let client = WyrdClient::with_config(client_config(srv)).expect("journey client");
        Bifrost::with_sink(&client, Some(table(fqn)), sink, config)
    }

    /// Returns durable lifecycle audit facts for one tenant and operation in sequence order.
    ///
    /// # Panics
    ///
    /// Panics when the tenant connection, audit query, or read transaction fails.
    async fn lifecycle_audit_rows(
        srv: &WyrdTestServer,
        tenant: DataTenantId,
        operation: &str,
    ) -> Vec<(String, String, String, String)> {
        let mut conn = srv
            .tenant_conn_for(tenant)
            .await
            .expect("tenant lifecycle audit connection");
        let rows = sqlx::query_as(
            "SELECT resource, decision, result, payload_summary \
             FROM vala.audit_outbox WHERE operation = $1 ORDER BY seq",
        )
        .bind(operation)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("read lifecycle audit rows");
        conn.commit().await.expect("commit lifecycle audit read");
        rows
    }

    /// Adds one Wyrd access token to a typed public gRPC request.
    fn authenticated_request<T>(value: T, bearer: &str) -> wyrd_tonic::tonic::Request<T> {
        let mut request = wyrd_tonic::tonic::Request::new(value);
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {bearer}").parse().expect("metadata"),
        );
        request
    }

    /// Starts one real server with a published queryable table and authenticated SDK client.
    async fn lifecycle_fixture() -> (
        WyrdTestServer,
        WyrdClient,
        WyrdClient,
        WyrdClient,
        DataTenantId,
        String,
    ) {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("lifecycle test server start");
        let table_name = format!("sdk_lifecycle_{}", uuid::Uuid::now_v7().simple());
        let table_fqn = format!("vala.bifrost.{table_name}");
        srv.state()
            .bifrost_catalog()
            .expect("Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![Field::new("value", DataType::Int64, false)],
                tenant: srv.data_tenant_id(),
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("register lifecycle table");
        srv.seed_bifrost_rows(&table_fqn, &[1])
            .await
            .expect("publish lifecycle row");
        let bootstrap = srv
            .bootstrap_service("sdk-query-lifecycle", &["admin"])
            .await
            .expect("bootstrap lifecycle caller");
        let config = ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        };
        let client = WyrdClient::with_config(config).expect("lifecycle SDK client");
        let other_tenant = srv
            .seed_tenant(&format!(
                "sdk-lifecycle-other-{}",
                uuid::Uuid::now_v7().simple()
            ))
            .await
            .expect("seed second lifecycle tenant");
        let other = srv
            .bootstrap_service_in_tenant(other_tenant, "sdk-query-lifecycle-other", &["admin"])
            .await
            .expect("bootstrap second lifecycle caller");
        let other_config = ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(other.api_key().expect("second machine API key").clone()),
            ..ClientConfig::default()
        };
        let other_client =
            WyrdClient::with_config(other_config).expect("second lifecycle SDK client");
        let denied = srv
            .bootstrap_service_in_tenant(other_tenant, "sdk-query-lifecycle-denied", &[])
            .await
            .expect("bootstrap under-privileged lifecycle caller");
        let denied_config = ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(denied.api_key().expect("denied machine API key").clone()),
            ..ClientConfig::default()
        };
        let denied_client =
            WyrdClient::with_config(denied_config).expect("under-privileged lifecycle SDK client");
        (
            srv,
            client,
            other_client,
            denied_client,
            other_tenant,
            table_fqn,
        )
    }

    /// Runs one HTTP query into the deterministic schema stall and returns its owner task.
    async fn stalled_query(
        srv: &WyrdTestServer,
        query: &QueryClient,
        table_fqn: &str,
    ) -> (RequestId, tokio::task::JoinHandle<()>) {
        srv.stall_next_query_after_schema();
        let stream = query
            .query(&BifrostQueryRequest {
                sql: format!("SELECT value FROM {table_fqn}"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(30_000),
            })
            .await
            .expect("query starts");
        let request_id = stream.request_id().clone();
        let task = tokio::spawn(async move {
            let mut stream = stream;
            let _ = stream.next_batch().await;
        });
        let stalled = srv.wait_query_schema_stall().await.expect("query stalls");
        assert_eq!(stalled, request_id.as_str());
        (request_id, task)
    }

    /// Real HTTP list/status/cancel controls preserve request identity and idempotency.
    pub(super) async fn oracle_query_status_cancel_impl() {
        let (srv, client, other_client, denied_client, other_tenant, table_fqn) =
            lifecycle_fixture().await;
        let query = QueryClient::new(&client);
        let other_query = QueryClient::new(&other_client);
        let denied_query = QueryClient::new(&denied_client);
        let (request_id, task) = stalled_query(&srv, &query, &table_fqn).await;

        let running = query.running().await.expect("running list");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].request_id, request_id);
        assert_eq!(
            query.status(&request_id).await.expect("status").request_id,
            request_id
        );
        assert!(
            other_query
                .running()
                .await
                .expect("foreign running list")
                .is_empty()
        );
        let unknown_id = RequestId::now_v7();
        let foreign_status = other_query
            .status(&request_id)
            .await
            .expect_err("foreign tenant cannot inspect query");
        let absent_status = other_query
            .status(&unknown_id)
            .await
            .expect_err("unknown query remains opaque");
        assert_eq!(
            foreign_status.code(),
            "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND"
        );
        assert_eq!(foreign_status.code(), absent_status.code());
        assert_eq!(foreign_status.detail(), absent_status.detail());
        let foreign_cancel = other_query
            .cancel(&request_id)
            .await
            .expect_err("foreign tenant cannot cancel query");
        let absent_cancel = other_query
            .cancel(&unknown_id)
            .await
            .expect_err("unknown cancellation remains opaque");
        assert_eq!(
            foreign_cancel.code(),
            "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND"
        );
        assert_eq!(foreign_cancel.code(), absent_cancel.code());
        assert_eq!(foreign_cancel.detail(), absent_cancel.detail());
        let denied = denied_query
            .running()
            .await
            .expect_err("under-privileged lifecycle list is denied");
        assert_eq!(denied.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        let denied_status = denied_query
            .status(&request_id)
            .await
            .expect_err("under-privileged lifecycle status is denied");
        assert_eq!(denied_status.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        let controls = srv
            .state()
            .bifrost
            .query_controls()
            .expect("query controls");
        let before_failed_audit = controls
            .get(srv.data_tenant_id(), request_id.clone())
            .await
            .expect("query remains active before audit fault");
        assert!(!before_failed_audit.cancellation_requested);
        srv.fail_query_cancel_attempt_audit();
        let audit_failure = query
            .cancel(&request_id)
            .await
            .expect_err("pre-dispatch audit failure refuses cancellation");
        assert_eq!(audit_failure.code(), "WYRD_VALA_500_AUDIT_UNAVAILABLE");
        srv.restore_query_cancel_attempt_audit();
        let after_failed_audit = controls
            .get(srv.data_tenant_id(), request_id.clone())
            .await
            .expect("query remains active after audit fault");
        assert!(!after_failed_audit.cancellation_requested);
        assert!(
            lifecycle_audit_rows(&srv, srv.data_tenant_id(), "vala.query.running.cancel")
                .await
                .is_empty()
        );
        assert!(
            lifecycle_audit_rows(
                &srv,
                srv.data_tenant_id(),
                "vala.query.running.cancel.attempt",
            )
            .await
            .is_empty()
        );
        assert!(
            query
                .cancel(&request_id)
                .await
                .expect("cancel")
                .cancellation_started
        );
        assert!(
            !query
                .cancel(&request_id)
                .await
                .expect("idempotent cancel")
                .cancellation_started
        );
        let owner_cancel_audits =
            lifecycle_audit_rows(&srv, srv.data_tenant_id(), "vala.query.running.cancel").await;
        assert_eq!(owner_cancel_audits.len(), 2);
        assert!(owner_cancel_audits.iter().all(|row| {
            row.0 == format!("vala.query.lifecycle/{request_id}")
                && row.1 == "allow"
                && row.2 == "success"
                && row.3 == "running query control succeeded"
        }));
        let owner_cancel_attempts = lifecycle_audit_rows(
            &srv,
            srv.data_tenant_id(),
            "vala.query.running.cancel.attempt",
        )
        .await;
        assert_eq!(owner_cancel_attempts.len(), 2);
        assert!(owner_cancel_attempts.iter().all(|row| {
            row.0 == format!("vala.query.lifecycle/{request_id}")
                && row.1 == "allow"
                && row.2 == "success"
                && row.3 == "running query cancellation dispatch authorized"
        }));
        let foreign_cancel_audits =
            lifecycle_audit_rows(&srv, other_tenant, "vala.query.running.cancel").await;
        assert_eq!(foreign_cancel_audits.len(), 2);
        assert_eq!(
            foreign_cancel_audits,
            vec![
                (
                    format!("vala.query.lifecycle/{request_id}"),
                    "allow".to_owned(),
                    "failure".to_owned(),
                    "running query control failed".to_owned(),
                ),
                (
                    format!("vala.query.lifecycle/{unknown_id}"),
                    "allow".to_owned(),
                    "failure".to_owned(),
                    "running query control failed".to_owned(),
                ),
            ]
        );
        let foreign_cancel_attempts =
            lifecycle_audit_rows(&srv, other_tenant, "vala.query.running.cancel.attempt").await;
        assert_eq!(foreign_cancel_attempts.len(), 2);
        assert!(foreign_cancel_attempts.iter().all(|row| {
            row.1 == "allow"
                && row.2 == "success"
                && row.3 == "running query cancellation dispatch authorized"
        }));
        let foreign_status_audits =
            lifecycle_audit_rows(&srv, other_tenant, "vala.query.running.get").await;
        assert_eq!(foreign_status_audits.len(), 3);
        assert!(foreign_status_audits[..2].iter().all(|row| {
            row.1 == "allow" && row.2 == "failure" && row.3 == "running query control failed"
        }));
        assert_eq!(
            foreign_status_audits[2],
            (
                format!("vala.query.lifecycle/{request_id}"),
                "deny".to_owned(),
                "failure".to_owned(),
                "rbac permission denied".to_owned(),
            )
        );
        assert_eq!(
            lifecycle_audit_rows(&srv, srv.data_tenant_id(), "vala.query.running.get").await,
            vec![(
                format!("vala.query.lifecycle/{request_id}"),
                "allow".to_owned(),
                "success".to_owned(),
                "running query control succeeded".to_owned(),
            )]
        );

        task.abort();
        let _ = task.await;
        srv.shutdown().await.expect("server shutdown");
    }

    /// Public gRPC controls use the same active registry and cancellation semantics.
    pub(super) async fn oracle_query_grpc_status_cancel_impl() {
        use wyrd_tonic::wyrd::v1 as proto;
        use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;

        let (srv, client, other_client, denied_client, other_tenant, table_fqn) =
            lifecycle_fixture().await;
        let query = QueryClient::new(&client);
        let (request_id, task) = stalled_query(&srv, &query, &table_fqn).await;
        let channel =
            wyrd_tonic::tonic::transport::Endpoint::from_shared(srv.grpc_url().expect("gRPC URL"))
                .expect("endpoint")
                .connect()
                .await
                .expect("gRPC connect");
        let bearer = client.auth().bearer().await.expect("bearer");
        let other_bearer = other_client.auth().bearer().await.expect("second bearer");
        let denied_bearer = denied_client.auth().bearer().await.expect("denied bearer");
        let mut grpc = BifrostQueryServiceClient::new(channel);
        let listed = grpc
            .list_running_queries(authenticated_request(
                proto::ListRunningQueriesRequest {},
                bearer.expose(),
            ))
            .await
            .expect("gRPC list")
            .into_inner();
        assert_eq!(listed.queries.len(), 1);
        assert_eq!(listed.queries[0].request_id, request_id.as_str());
        let got = grpc
            .get_running_query(authenticated_request(
                proto::GetRunningQueryRequest {
                    request_id: request_id.to_string(),
                },
                bearer.expose(),
            ))
            .await
            .expect("gRPC status")
            .into_inner();
        assert_eq!(got.request_id, request_id.as_str());
        let foreign_status = grpc
            .get_running_query(authenticated_request(
                proto::GetRunningQueryRequest {
                    request_id: request_id.to_string(),
                },
                other_bearer.expose(),
            ))
            .await
            .expect_err("foreign tenant cannot inspect query");
        let unknown_id = RequestId::now_v7();
        let absent_status = grpc
            .get_running_query(authenticated_request(
                proto::GetRunningQueryRequest {
                    request_id: unknown_id.to_string(),
                },
                other_bearer.expose(),
            ))
            .await
            .expect_err("unknown query remains opaque");
        assert_eq!(foreign_status.code(), wyrd_tonic::tonic::Code::NotFound);
        assert_eq!(foreign_status.code(), absent_status.code());
        assert_eq!(foreign_status.message(), absent_status.message());
        let foreign_cancel = grpc
            .cancel_running_query(authenticated_request(
                proto::CancelRunningQueryRequest {
                    request_id: request_id.to_string(),
                },
                other_bearer.expose(),
            ))
            .await
            .expect_err("foreign tenant cannot cancel query");
        let absent_cancel = grpc
            .cancel_running_query(authenticated_request(
                proto::CancelRunningQueryRequest {
                    request_id: unknown_id.to_string(),
                },
                other_bearer.expose(),
            ))
            .await
            .expect_err("unknown cancellation remains opaque");
        assert_eq!(foreign_cancel.code(), wyrd_tonic::tonic::Code::NotFound);
        assert_eq!(foreign_cancel.code(), absent_cancel.code());
        assert_eq!(foreign_cancel.message(), absent_cancel.message());
        let denied = grpc
            .list_running_queries(authenticated_request(
                proto::ListRunningQueriesRequest {},
                denied_bearer.expose(),
            ))
            .await
            .expect_err("under-privileged lifecycle list is denied");
        assert_eq!(denied.code(), wyrd_tonic::tonic::Code::PermissionDenied);
        let first = grpc
            .cancel_running_query(authenticated_request(
                proto::CancelRunningQueryRequest {
                    request_id: request_id.to_string(),
                },
                bearer.expose(),
            ))
            .await
            .expect("gRPC cancel")
            .into_inner();
        assert!(first.cancellation_started);
        let second = grpc
            .cancel_running_query(authenticated_request(
                proto::CancelRunningQueryRequest {
                    request_id: request_id.to_string(),
                },
                bearer.expose(),
            ))
            .await
            .expect("gRPC idempotent cancel")
            .into_inner();
        assert!(!second.cancellation_started);
        assert_eq!(
            lifecycle_audit_rows(&srv, srv.data_tenant_id(), "vala.query.running.cancel")
                .await
                .len(),
            2,
        );
        assert_eq!(
            lifecycle_audit_rows(&srv, other_tenant, "vala.query.running.cancel")
                .await
                .len(),
            2,
        );

        task.abort();
        let _ = task.await;
        srv.shutdown().await.expect("server shutdown");
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
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
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
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
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
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
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
            .bifrost_catalog()
            .expect("Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                physical_layout: None,
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
            credential: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        };
        config.grpc.max_message_bytes = 32 * 1024 * 1024;
        let writer = BifrostWriter::connect(
            config,
            bootstrap
                .card_ref()
                .expect("service bootstrap has a Card scope")
                .clone(),
        )
        .await
        .expect("public SDK write door");
        writer
            .write(&table_fqn, &schema(), [row(7), row(8)])
            .await
            .expect("durable ingest ACK");
        let client = writer.client();
        srv.flush_bifrost().await.expect("flush Scribe");

        let request = BifrostQueryRequest {
            sql: format!("SELECT id, value FROM {table_fqn} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        };
        let mut stream = QueryClient::new(client)
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

    /// A multi-batch query carries one schema and closes with one explicit EOS.
    ///
    /// The public query stream is one Arrow IPC stream split across Wyrd frames,
    /// so a result with several batches repeats neither the schema nor the
    /// stream prefix. This journey drives the real SDK against a real server and
    /// proves the wire consequence directly: the query's total Arrow bytes are
    /// strictly fewer than re-encoding the same batches as standalone streams,
    /// the stream is explicitly closed, and the client never retains more than
    /// one fragment at a time.
    #[tokio::test]
    async fn pg_bifrost_multi_batch_query_stream_reuses_schema() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let table_name = format!("sdk_multi_batch_{}", uuid::Uuid::now_v7().simple());
        let table_fqn = format!("vala.bifrost.{table_name}");
        srv.state()
            .bifrost_catalog()
            .expect("Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("register multi-batch journey table");
        let bootstrap = srv
            .bootstrap_service("sdk-multi-batch-query", &["admin"])
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
            credential: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        };
        config.grpc.max_message_bytes = 32 * 1024 * 1024;
        let writer = BifrostWriter::connect(
            config,
            bootstrap
                .card_ref()
                .expect("service bootstrap has a Card scope")
                .clone(),
        )
        .await
        .expect("public SDK write door");
        let client = writer.client();
        // One ingest larger than the widest admitted DataFusion batch size
        // guarantees the result spans several batches on one shared IPC stream,
        // independent of how many files or partitions the scan happens to use.
        let row_count = ORACLE_MAX_BATCH_SIZE + 1;
        let ids: Vec<i64> = (0..row_count)
            .map(|id| i64::try_from(id).expect("row id fits i64"))
            .collect();
        // The rows arrive in several requests so each stays well under the
        // server's canonical ingest ceiling; one flush seals them all before the
        // query runs.
        for chunk in ids.chunks(512) {
            writer
                .write(
                    &table_fqn,
                    &schema(),
                    chunk.iter().map(|id| row(*id)).collect::<Vec<_>>(),
                )
                .await
                .expect("durable ingest ACK");
            srv.flush_bifrost().await.expect("flush Scribe");
        }

        let request = BifrostQueryRequest {
            sql: format!("SELECT id, value FROM {table_fqn} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        };
        let mut stream = QueryClient::new(client)
            .query(&request)
            .await
            .expect("Oracle query starts");
        let mut batches = Vec::new();
        while let Some(batch) = stream.next_batch().await.expect("valid terminal stream") {
            batches.push(batch);
        }
        let schema = stream.schema().expect("authoritative schema").clone();
        assert!(
            batches.len() >= 2,
            "the journey needs a multi-batch result to prove schema reuse"
        );
        let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(rows, row_count);
        let terminal = stream.terminal().expect("validated terminal");
        assert_eq!(
            terminal.row_count,
            u64::try_from(row_count).expect("row count fits u64")
        );
        assert!(
            !terminal.arrow_ipc_eos.is_empty(),
            "a successful terminal carries its end-of-stream delta"
        );
        assert!(stream.arrow_ipc_closed(), "the IPC stream is closed");

        // Re-encoding each decoded batch as its own stream reproduces exactly
        // what the wire carried before this task: schema, batch, and terminator
        // per batch.
        let per_batch: Vec<usize> = batches
            .iter()
            .map(|batch| {
                let mut bytes = Vec::new();
                let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref())
                    .expect("standalone writer starts");
                writer.write(batch).expect("standalone batch writes");
                writer.finish().expect("standalone writer finishes");
                drop(writer);
                bytes.len()
            })
            .collect();
        let standalone: usize = per_batch.iter().sum();
        assert!(
            stream.arrow_ipc_bytes() < standalone,
            "one shared stream must carry fewer Arrow bytes than per-batch streams: {} vs {standalone}",
            stream.arrow_ipc_bytes()
        );
        // Every fragment kind is strictly smaller than the standalone stream
        // carrying the widest batch: a continuation fragment is that batch's
        // message without the schema prefix and terminator a standalone adds,
        // the schema fragment is embedded in every standalone, and the
        // end-of-stream is eight bytes. The widest standalone encoding is
        // therefore a real ceiling on whatever the client retains at once,
        // whatever this fixture's schema happens to cost.
        let widest_batch_stream = per_batch.iter().copied().max().expect("result has batches");
        assert!(
            stream.peak_pending_frame_bytes() <= widest_batch_stream,
            "the client retains one fragment at a time: {} vs {widest_batch_stream}",
            stream.peak_pending_frame_bytes()
        );
        srv.shutdown().await.expect("server shutdown");
    }

    /// Describe reaches the exact stored physical schema for both table kinds.
    ///
    /// The whole point of describe is that a writer never keeps its own copy of
    /// the physical contract. For a canonical signal table that means the
    /// description's projected Arrow schema is the registry's physical schema —
    /// every name, type, nullability, and stable field id — plus the one Gate
    /// input (`card_ref`) the write path resolves rather than stores, and the
    /// exact canonical physical fingerprint. Types are compared through the
    /// storage layer's own round-trip equivalence, which is what "the stored
    /// schema" actually means once Iceberg has widened a byte or string column. For a dynamic table it means a
    /// batch built straight from the description is accepted by the real ingest
    /// wire and lands durably, with each correlation column appended exactly
    /// once.
    ///
    /// # Panics
    ///
    /// Panics when a described schema diverges from the stored physical schema,
    /// when the canonical fingerprint does not match the registry, or when a
    /// description-built batch is rejected or fails to land.
    #[tokio::test]
    async fn described_canonical_and_dynamic_schemas_reach_exact_physical_schema() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let table_name = format!("described_{}", uuid::Uuid::now_v7().simple());
        srv.state()
            .bifrost_catalog()
            .expect("Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("register the dynamic journey table");
        srv.ensure_traces_spans_table_for_test(srv.data_tenant_id())
            .await
            .expect("provision the canonical span table");

        let bootstrap = srv
            .bootstrap_service("sdk-describe-writer", &["admin"])
            .await
            .expect("bootstrap the describing writer");
        let api_key = bootstrap.api_key().expect("machine API key").clone();
        let writer_card = bootstrap
            .card_ref()
            .expect("machine principal card ref")
            .clone();
        let mut config = client_config(&srv);
        config.credential = Some(api_key);
        config.grpc.endpoint = srv.grpc_url().expect("gRPC URL");
        config.grpc.connect_retries = 0;
        let client = WyrdClient::with_config(config).expect("SDK client");
        let query = QueryClient::new(&client);

        // The canonical signal table: its description must reproduce the
        // registry's own physical schema rather than an approximation of it.
        let canonical = query
            .describe_table("vala.traces", "spans")
            .await
            .expect("describe the canonical span table");
        let definition = vala_bifrost_redux::tables::builtin_table("traces", "spans")
            .expect("the spans built-in resolves");
        assert_eq!(
            canonical.canonical_physical_fingerprint.as_deref(),
            Some(
                (definition.canonical_physical_fingerprint)()
                    .expect("a canonical table has a physical fingerprint")
                    .to_hex()
                    .as_str()
            ),
            "the description publishes the registry's exact canonical fingerprint"
        );

        let described_schema = wyrd_queue::schema::writable_schema(&canonical, true)
            .expect("the canonical description projects an Arrow schema");
        let physical = (definition.schema)();
        for stored in physical.fields() {
            let name = stored.name().as_str();
            // Server-resolved columns are not a writer's to supply, so they are
            // deliberately absent from every description; `wyrd_event_time` and
            // `run_id` are the two the writer may still send.
            if (wyrd_spec::vala::is_reserved_managed_column(name)
                || wyrd_spec::vala::is_reserved_correlation_column(name))
                && name != "wyrd_event_time"
                && name != "run_id"
            {
                continue;
            }
            let described = described_schema
                .field_with_name(stored.name())
                .unwrap_or_else(|_| panic!("described schema keeps `{}`", stored.name()));
            assert!(
                vala_bifrost_redux::tables::arrow_type_shape_matches(
                    stored.data_type(),
                    described.data_type()
                ),
                "`{}` keeps its stored type shape: stored {:?}, described {:?}",
                stored.name(),
                stored.data_type(),
                described.data_type()
            );
            assert_eq!(
                described.is_nullable(),
                stored.is_nullable(),
                "`{}` keeps its stored nullability",
                stored.name()
            );
            assert!(
                described.metadata().is_empty(),
                "`{}` sends no field id: the id is the server's physical \
                 identity, which it assigns and ignores on an incoming batch",
                stored.name()
            );
        }
        // The tag lives on the description, not on the projected Arrow schema:
        // `writable_schema` drops field metadata at every depth so a writer
        // never repeats server-owned identity onto the wire.
        assert_eq!(
            canonical
                .correlation_fields
                .iter()
                .find(|field| field.name == "card_ref")
                .expect("the Gate correlation input is described")
                .metadata
                .get(wyrd_spec::vala::api::INPUT_CLASS_KEY)
                .map(String::as_str),
            Some(wyrd_spec::vala::api::INPUT_CLASS_GATE_CORRELATION),
            "card_ref is a resolved Gate input, not a stored column"
        );
        for name in ["card_ref", "run_id", "wyrd_event_time"] {
            assert_eq!(
                described_schema
                    .fields()
                    .iter()
                    .filter(|field| field.name() == name)
                    .count(),
                1,
                "`{name}` is declared exactly once"
            );
        }

        // The dynamic table: a batch built straight from its description is
        // accepted by the real ingest wire and lands durably.
        let dynamic = query
            .describe_table("vala.bifrost", &table_name)
            .await
            .expect("describe the dynamic journey table");
        assert!(
            dynamic.canonical_physical_fingerprint.is_none(),
            "a dynamic table publishes no canonical physical fingerprint"
        );
        assert!(
            wyrd_queue::schema::writable_schema(&dynamic, false)
                .expect("the dynamic description projects an Arrow schema")
                .field_with_name("card_ref")
                .expect("the Gate correlation input is described")
                .is_nullable(),
            "Card correlation is optional, so describe must not demand it"
        );
        let mut builder = wyrd_queue::batch_builder::BatchBuilder::from_description(&dynamic)
            .expect("the dynamic description builds a row builder");
        builder
            .append_json_row(
                r#"{"id": 1, "value": "described"}"#,
                Some(&writer_card),
                None,
            )
            .expect("a described row is accepted");
        builder
            .append_json_row(r#"{"id": 2, "value": "uncorrelated"}"#, None, None)
            .expect("a described row without Card correlation is accepted");
        let ipc = builder.finish_ipc().expect("seal the described batch");

        RawIngest::connect(&client)
            .await
            .expect("connect the public ingest wire")
            .insert(
                &format!("vala.bifrost.{table_name}"),
                uuid::Uuid::now_v7(),
                ipc,
            )
            .await
            .expect("the described batch is accepted by the real wire");
        srv.flush_bifrost()
            .await
            .expect("flush server-owned Scribe");

        let mut conn = srv
            .tenant_conn_for(srv.data_tenant_id())
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
        conn.commit().await.expect("commit tenant-scoped read");
        assert_eq!(
            row_count, 2,
            "the description-built batch reached the durable read boundary"
        );

        // The uncorrelated row is stored under the authenticated principal with
        // no Card, which is exactly what optional correlation has to mean.
        let mut stream = query
            .query(&BifrostQueryRequest {
                sql: format!(
                    "SELECT id, card_uid, principal_id \
                     FROM vala.bifrost.{table_name} ORDER BY id"
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: None,
            })
            .await
            .expect("Oracle reads the stored correlation columns");
        let mut correlation = Vec::new();
        while let Some(batch) = stream.next_batch().await.expect("valid terminal stream") {
            let card_uid = batch.column_by_name("card_uid").expect("card_uid column");
            let principal_id = batch
                .column_by_name("principal_id")
                .expect("principal_id column");
            for row in 0..batch.num_rows() {
                correlation.push((card_uid.is_null(row), principal_id.is_null(row)));
            }
        }
        assert_eq!(
            correlation,
            vec![(false, false), (true, false)],
            "the correlated row keeps its card_uid; the uncorrelated row stores \
             a null card_uid and still carries its authenticated principal"
        );
        srv.shutdown().await.expect("server shutdown");
    }

    #[tokio::test]
    async fn public_sdk_bidi_write_ack_and_durable_readback() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let table_name = format!("sdk_roundtrip_{}", uuid::Uuid::now_v7().simple());
        srv.state()
            .bifrost_catalog()
            .expect("Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                physical_layout: None,
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
            credential: Some(api_key),
            ..ClientConfig::default()
        };
        config.grpc.connect_retries = 0;
        BifrostWriter::connect(
            config,
            bootstrap
                .card_ref()
                .expect("service bootstrap has a Card scope")
                .clone(),
        )
        .await
        .expect("public SDK write door")
        .write(
            &format!("vala.bifrost.{table_name}"),
            &schema(),
            [row(7), row(8)],
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
            .with_wal_sync_delay(Duration::from_millis(75))
            .start_bound()
            .await
            .expect("delayed-WAL test server start");
        let table_name = format!("sdk_timeout_retry_{}", uuid::Uuid::now_v7().simple());
        let table_fqn = format!("vala.bifrost.{table_name}");
        srv.state()
            .bifrost_catalog()
            .expect("Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: srv.data_tenant_id(),
                physical_layout: None,
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
            credential: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        };
        let client = WyrdClient::with_config(config).expect("public SDK client");
        let transport = BifrostGrpcTransport::connect_with_config(
            &client,
            BifrostTransportConfig::with_max_frame_retries(0),
        )
        .await
        .expect("connect timeout-retry transport");
        let recording = Arc::new(RecordingTransport::new(transport));
        let bifrost = Bifrost::with_sink(
            &client,
            Some(table(&table_fqn)),
            Arc::new(BifrostIngestSink::new(recording.clone())),
            QueueConfig {
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
        );

        bifrost
            .insert(
                row(41),
                Correlation {
                    card_ref: Some(target),
                    run_id: None,
                },
            )
            .expect("enqueue one owned JSON row");
        let first_error = bifrost.flush().await.expect_err(
            "short deadline after server receipt must leave the durable result ambiguous",
        );
        let vala_sdk::ValaSdkError::Queue(WyrdQueueError::Sink(first_sink_error)) = &first_error
        else {
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
        bifrost
            .flush()
            .await
            .expect("retry resolves the post-receipt ambiguity through durable dedup");
        let attempts = recording.attempts();
        assert!(attempts.len() >= 2, "one deadline then at least one retry");
        assert!(
            attempts.iter().all(|attempt| attempt.0 == attempts[0].0),
            "every public sink retry used the exact same stable batch ID"
        );
        assert!(
            attempts.iter().all(|attempt| attempt.1 == attempts[0].1),
            "every retry reused the owned-byte allocation without copying"
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
        bifrost.shutdown().await.expect("settled producer shutdown");
        assert_eq!(
            bifrost.metrics().total_reserved_bytes,
            0,
            "shutdown releases the producer's fixed queue reservation"
        );
        srv.shutdown().await.expect("server shutdown");
    }

    /// Happy-path: insert via Bifrost handle + observe path, no drops, real server
    /// credentials validate the scope construction path.
    #[tokio::test]
    async fn observe_and_bifrost_roundtrip() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let bifrost = client_over(
            &srv,
            "test.roundtrip",
            Arc::new(MockSink::new()),
            QueueConfig::default(),
        );
        let schema = schema();

        for i in 0..500i64 {
            bifrost
                .insert(row(i), correlated())
                .expect("insert into the active table");
        }
        assert_eq!(
            bifrost.dropped(),
            0,
            "Bifrost handle: no drops on happy path"
        );

        // An uncorrelated row is a valid write: the server stores it against
        // the authenticated principal with a null card_uid.
        bifrost
            .insert(row(500), Correlation::default())
            .expect("an omitted card_ref is accepted");

        for i in 0..500i64 {
            observe::record(&bifrost, "test.roundtrip", &schema, row(i), correlated());
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
        let stall = Arc::new(StallSink::default());
        let bifrost = client_over(
            &srv,
            "test.backpressure",
            stall.clone(),
            saturating_config(),
        );
        let schema = schema();

        // Prime the stall: first insert starts draining then parks forever.
        bifrost
            .insert(row(0), correlated())
            .expect("first accepted");
        wait_until_started(&stall);

        // Flood until rejection.
        let mut saw_queue_full = false;
        for i in 1..=100 {
            match bifrost.insert(row(i), correlated()) {
                Ok(()) => {}
                Err(error) => {
                    assert_eq!(
                        error.code(),
                        "WYRD_CLIENT_429_QUEUE_FULL",
                        "unexpected error: {error}"
                    );
                    saw_queue_full = true;
                }
            }
        }
        assert!(
            saw_queue_full,
            "saturated queue must propagate WYRD_CLIENT_429_QUEUE_FULL"
        );

        // Observe path swallows queue-full.
        let bifrost2 = client_over(
            &srv,
            "test.backpressure.obs",
            stall.clone(),
            saturating_config(),
        );
        observe::record(
            &bifrost2,
            "test.backpressure.obs",
            &schema,
            row(0),
            correlated(),
        );
        wait_until_started(&stall);
        for i in 1..=100 {
            observe::record(
                &bifrost2,
                "test.backpressure.obs",
                &schema,
                row(i),
                correlated(),
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
        let sink = Arc::new(ReleasingSink::default());
        let bifrost = client_over(
            &srv,
            "test.downstream_stall",
            sink.clone(),
            saturating_config(),
        );

        bifrost
            .insert(row(0), correlated())
            .expect("first accepted");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !sink.started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "releasing sink never started");
            std::thread::sleep(Duration::from_millis(5));
        }

        let mut accepted = 1_usize;
        let mut rejected = 0_usize;
        for i in 1..=100 {
            match bifrost.insert(row(i), correlated()) {
                Ok(()) => accepted += 1,
                Err(error) => {
                    assert_eq!(
                        error.code(),
                        "WYRD_CLIENT_429_QUEUE_FULL",
                        "unexpected queue error: {error}"
                    );
                    rejected += 1;
                }
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
            .insert(row(101), correlated())
            .expect("post-drain write accepted");
        wait_until_sent(&sink, accepted + 1);
        assert_eq!(sink.sent_rows.load(Ordering::SeqCst), accepted + 1);

        drop(bifrost);
        srv.shutdown().await.expect("server shutdown");
    }

    /// A `WyrdClient` for one freshly bootstrapped admin service on `srv`.
    ///
    /// Both planes are configured from the live harness: HTTP for register,
    /// describe, and query; gRPC for ingest. `connect_retries: 0` keeps a
    /// journey failure immediate instead of retried.
    async fn admin_client(srv: &WyrdTestServer, service: &str) -> WyrdClient {
        let bootstrap = srv
            .bootstrap_service(service, &["admin"])
            .await
            .expect("bootstrap journey service");
        WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().expect("gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        })
        .expect("journey SDK client")
    }

    /// A caller-owned table name in the namespace users may register into.
    fn owned_fqn(prefix: &str) -> String {
        format!("vala.datasets.{prefix}_{}", uuid::Uuid::now_v7().simple())
    }

    /// Flatten one collected result's `(id, value)` pairs in row order.
    fn id_value_rows(batches: &[RecordBatch]) -> Vec<(i64, String)> {
        let mut rows = Vec::new();
        for batch in batches {
            let ids = batch
                .column_by_name("id")
                .expect("id column")
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .expect("id is Int64");
            let values = batch
                .column_by_name("value")
                .expect("value column")
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .expect("value is Utf8");
            for index in 0..batch.num_rows() {
                rows.push((ids.value(index), values.value(index).to_owned()));
            }
        }
        rows
    }

    /// The whole Rust unified-client journey against one real server.
    ///
    /// Register, insert, swap the active table before flushing, insert again,
    /// drain, then read both tables back — one through `sql` and one through
    /// `stream`, so the collected and streamed doors are both exercised on the
    /// rows this journey actually wrote. The blocking facade's advanced query
    /// escape hatch is reached on the same client, off the runtime's worker
    /// threads, because that surface has no async twin to stand in for it.
    #[tokio::test]
    async fn unified_client_registers_writes_swaps_and_reads_both_tables() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let client = admin_client(&srv, "sdk-unified-journey").await;

        let first_fqn = owned_fqn("journey_first");
        let second_fqn = owned_fqn("journey_second");
        let bifrost = Bifrost::connect_with_table(&client, table(&first_fqn))
            .await
            .expect("unified client connects");

        assert_eq!(
            bifrost.register().await.expect("register the first table"),
            RegisterOutcome::Created
        );
        assert_eq!(
            bifrost
                .register()
                .await
                .expect("re-register an equal declaration"),
            RegisterOutcome::AlreadyExists,
            "an equal schema is idempotent, not a conflict"
        );
        let resolved = bifrost
            .table()
            .expect("the first table stays bound")
            .resolved()
            .cloned()
            .expect("register resolves the server identity");
        assert!(!resolved.table_uid.is_empty() && !resolved.fingerprint.is_empty());

        let first_rows = vec![(4i64, "fourth".to_owned()), (5, "fifth".to_owned())];
        for (id, value) in &first_rows {
            bifrost
                .insert(
                    format!(r#"{{"id": {id}, "value": "{value}"}}"#).into_bytes(),
                    Correlation::default(),
                )
                .expect("insert into the first table");
        }

        // Swap before flushing: the swapped-away producer must still drain, so
        // the rows above are not stranded by the rebinding.
        bifrost.use_table(table(&second_fqn));
        assert_eq!(
            bifrost.register().await.expect("register the second table"),
            RegisterOutcome::Created
        );
        bifrost
            .insert(
                br#"{"id": 7, "value": "second-table"}"#.to_vec(),
                Correlation::default(),
            )
            .expect("insert into the second table");
        assert_eq!(
            bifrost.producer_count(),
            2,
            "the swapped-away producer is still pooled"
        );

        bifrost.flush().await.expect("flush every pooled producer");
        bifrost.shutdown().await.expect("shutdown drains and stops");
        srv.flush_bifrost()
            .await
            .expect("publish the server-owned Scribe");

        let collected = bifrost
            .sql(&format!("SELECT id, value FROM {first_fqn} ORDER BY id"))
            .await
            .expect("collect the first table");
        assert_eq!(collected.terminal().outcome, QueryTerminalOutcome::Success);
        assert_eq!(id_value_rows(collected.batches()), first_rows);

        let mut stream = bifrost
            .stream(&format!("SELECT id, value FROM {second_fqn} ORDER BY id"))
            .await
            .expect("stream the second table");
        let mut streamed = Vec::new();
        while let Some(batch) = stream.next_batch().await.expect("stream the next batch") {
            streamed.push(batch);
        }
        assert_eq!(
            id_value_rows(&streamed),
            vec![(7, "second-table".to_owned())]
        );
        assert!(
            stream.terminal().is_some(),
            "a complete stream carries its validated terminal"
        );

        // The blocking facade's escape hatch is the async client's own
        // `QueryClient`, so describing the registered table through it must
        // answer the same identity register minted.
        let blocking_client = client.clone();
        let described_fqn = first_fqn.clone();
        let described = tokio::task::spawn_blocking(move || {
            let blocking =
                vala_sdk::blocking::Bifrost::connect(&blocking_client).expect("blocking facade");
            let (namespace, name) = described_fqn
                .rsplit_once('.')
                .expect("the journey table is `<namespace>.<name>`");
            wyrd_runtime::runtime()
                .block_on(blocking.query().describe_table(namespace, name))
                .expect("describe through the blocking escape hatch")
        })
        .await
        .expect("blocking describe task joins");
        assert_eq!(described.entry.table_uid, resolved.table_uid);
        assert_eq!(described.entry.fingerprint, resolved.fingerprint);

        srv.shutdown().await.expect("server shutdown");
    }

    /// A table swapped in while registration is in flight keeps its own identity.
    ///
    /// `register` releases the active-table lock for its network round trip, so
    /// the response can arrive after `use_table` has rebound the client. The
    /// swap here is deterministic rather than timed: the register future is
    /// polled exactly once, which runs its synchronous request-building section
    /// and parks on the HTTP response, and only then is the binding replaced.
    #[tokio::test]
    async fn register_never_stamps_one_tables_identity_onto_another() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let client = admin_client(&srv, "sdk-register-race").await;

        let bound_fqn = owned_fqn("race_bound");
        let inflight_fqn = owned_fqn("race_inflight");
        let bifrost = Bifrost::connect_with_table(&client, table(&bound_fqn))
            .await
            .expect("unified client connects");
        assert_eq!(
            bifrost.register().await.expect("register the bound table"),
            RegisterOutcome::Created
        );
        let bound = bifrost.table().expect("the bound table stays bound");
        let bound_identity = bound
            .resolved()
            .cloned()
            .expect("the bound table is resolved");

        // Bind the second declaration, start its registration, and let it reach
        // its first await before rebinding to the already-resolved first table.
        bifrost.use_table(table(&inflight_fqn));
        let register = bifrost.register();
        tokio::pin!(register);
        tokio::select! {
            biased;
            _ = &mut register => panic!("register cannot settle before the server answers"),
            () = std::future::ready(()) => {}
        }
        bifrost.use_table(bound);
        assert_eq!(
            register
                .await
                .expect("the in-flight registration completes"),
            RegisterOutcome::Created
        );

        let active = bifrost.table().expect("the rebound table stays bound");
        assert_eq!(active.fqn(), bound_fqn);
        assert_eq!(
            active.resolved(),
            Some(&bound_identity),
            "a response must never overwrite the identity of a different table"
        );

        // The in-flight declaration was genuinely created server-side, and
        // re-registering it resolves its own distinct identity.
        bifrost.use_table(table(&inflight_fqn));
        assert_eq!(
            bifrost
                .register()
                .await
                .expect("re-register the in-flight table"),
            RegisterOutcome::AlreadyExists
        );
        let inflight_identity = bifrost
            .table()
            .expect("the in-flight table is bound")
            .resolved()
            .cloned()
            .expect("re-registering resolves it");
        assert_ne!(
            inflight_identity.table_uid, bound_identity.table_uid,
            "two tables must not share one server-minted uid"
        );

        srv.shutdown().await.expect("server shutdown");
    }
}

/// Real HTTP list/status/cancel controls preserve tenant-scoped lifecycle identity.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn oracle_query_status_cancel_is_tenant_scoped() {
    pg_tests::oracle_query_status_cancel_impl().await;
}

/// Public gRPC controls project the same tenant-scoped lifecycle owner.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn oracle_query_grpc_status_cancel_is_tenant_scoped() {
    pg_tests::oracle_query_grpc_status_cancel_impl().await;
}
