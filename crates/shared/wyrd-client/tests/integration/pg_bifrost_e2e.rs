//! The `wyrd_client::Bifrost` public client against a real server.
//!
//! Each test registers, writes, and reads through the public client exactly as
//! a caller does. These tests require a live `WyrdTestServer` (repository-
//! managed Postgres plus a real server socket), so they live in `mod pg_tests`
//! and run in the `test:bifrost:journey:sdk` lane.
//!
//! Queue, sink, and register-race mechanics are unit tests in
//! `wyrd_client::bifrost`; ingest reliability and describe-schema fidelity are
//! Scribe journeys in `wyrd-testing`; lifecycle and describe audit evidence is
//! asserted by the `wyrd-testing` server journeys.

mod pg_tests {
    use std::sync::Arc;

    use arrow::array::Array;
    use arrow::datatypes::ArrowPrimitiveType;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use serde::{Deserialize, Serialize};
    use wyrd_client::WyrdClient;
    use wyrd_client::bifrost::{Bifrost, BifrostClientError, Correlation, TableConfig};
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalOutcome, RegisterOutcome};
    use wyrd_testing::bifrost::canonical_signals as fixture;
    use wyrd_testing::server::WyrdTestServer;

    /// The stable catalog code one SDK error projects onto.
    fn sdk_code(error: &BifrostClientError) -> &'static str {
        wyrd_spec::error::WyrdError::from(error).code()
    }

    /// The scrubbed public detail one SDK error projects onto.
    fn sdk_detail(error: &BifrostClientError) -> String {
        wyrd_spec::error::WyrdError::from(error)
            .as_problem_json()
            .get("detail")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    /// A client for one freshly bootstrapped service on `srv` holding `roles`.
    ///
    /// Both planes come from the live harness: HTTP for register, describe, and
    /// query; gRPC for ingest. `connect_retries: 0` keeps a failure immediate.
    ///
    /// # Panics
    ///
    /// Panics when the service cannot be bootstrapped or the client assembled.
    async fn service_client(srv: &WyrdTestServer, service: &str, roles: &[&str]) -> WyrdClient {
        let bootstrap = srv
            .bootstrap_service(service, roles)
            .await
            .expect("bootstrap the journey service");
        client_for(srv, bootstrap.api_key().expect("machine API key").clone())
    }

    /// A client for `srv` authenticating with `credential`.
    ///
    /// # Panics
    ///
    /// Panics when the client cannot be assembled.
    fn client_for(srv: &WyrdTestServer, credential: secrecy::SecretString) -> WyrdClient {
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
            credential: Some(credential),
            ..ClientConfig::default()
        })
        .expect("journey client")
    }

    /// A caller-owned table name in the namespace users may register into.
    fn owned_fqn(prefix: &str) -> String {
        format!("vala.datasets.{prefix}_{}", uuid::Uuid::now_v7().simple())
    }

    /// One row of the `id`/`value` journey tables, as a caller declares it.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct IdValueRow {
        /// The declared `id` column.
        id: i64,
        /// The declared `value` column.
        value: String,
    }

    impl IdValueRow {
        /// One row with the given id and value.
        fn new(id: i64, value: &str) -> Self {
            Self {
                id,
                value: value.to_owned(),
            }
        }
    }

    /// The `id`/`value` Arrow schema [`IdValueRow`] serializes into.
    fn id_value_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
        ]))
    }

    /// The `id`/`value` table declaration these journeys write through.
    fn table(fqn: &str) -> TableConfig {
        TableConfig::from_arrow(fqn, id_value_schema()).expect("declared journey table")
    }

    /// One typed row as the JSON bytes `Bifrost::insert` accepts.
    fn json_row<T: Serialize>(row: &T) -> Vec<u8> {
        serde_json::to_vec(row).expect("a journey row serializes")
    }

    /// Flatten one collected result's `(id, value)` rows in order.
    fn id_value_rows(batches: &[RecordBatch]) -> Vec<IdValueRow> {
        let ids = primitive_col::<arrow::datatypes::Int64Type>(batches, "id");
        ids.into_iter()
            .zip(string_col(batches, "value"))
            .map(|(id, value)| IdValueRow {
                id: id.expect("id is non-null"),
                value,
            })
            .collect()
    }

    /// Flatten one primitive column across every batch, preserving nulls.
    ///
    /// # Panics
    ///
    /// Panics when the column is absent or is not the requested Arrow type.
    fn primitive_col<T: ArrowPrimitiveType>(
        batches: &[RecordBatch],
        name: &str,
    ) -> Vec<Option<T::Native>> {
        let mut values = Vec::new();
        for batch in batches {
            let array = batch
                .column_by_name(name)
                .unwrap_or_else(|| panic!("column `{name}`"))
                .as_any()
                .downcast_ref::<arrow::array::PrimitiveArray<T>>()
                .unwrap_or_else(|| panic!("column `{name}` is not the expected primitive type"))
                .clone();
            for index in 0..array.len() {
                values.push(array.is_valid(index).then(|| array.value(index)));
            }
        }
        values
    }

    /// Flatten one non-null string column across every batch.
    ///
    /// # Panics
    ///
    /// Panics when the column is absent or is not `Utf8`.
    fn string_col(batches: &[RecordBatch], name: &str) -> Vec<String> {
        let mut values = Vec::new();
        for batch in batches {
            let array = batch
                .column_by_name(name)
                .unwrap_or_else(|| panic!("column `{name}`"))
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .unwrap_or_else(|| panic!("column `{name}` is not Utf8"))
                .clone();
            for index in 0..array.len() {
                values.push(array.value(index).to_owned());
            }
        }
        values
    }

    /// Register `fqn`, write `rows`, and publish them so a reader can see them.
    ///
    /// Rows are queryable only after the client drains its producers and the
    /// server-owned Scribe publishes, so both halves belong to one helper.
    ///
    /// # Panics
    ///
    /// Panics when connecting, registering, inserting, draining, or publishing
    /// fails; each is a harness failure rather than a behavior under test.
    async fn publish_rows<T: Serialize>(
        srv: &WyrdTestServer,
        client: &WyrdClient,
        fqn: &str,
        schema: SchemaRef,
        rows: &[T],
    ) {
        let config = TableConfig::from_arrow(fqn, schema).expect("declared table");
        let bifrost = Bifrost::connect_with_table(client, config)
            .await
            .expect("the writer connects");
        assert_eq!(
            bifrost.register().await.expect("register the table"),
            RegisterOutcome::Created
        );
        for row in rows {
            bifrost
                .insert(json_row(row), Correlation::default())
                .expect("insert a row");
        }
        bifrost.flush().await.expect("the durable ACK settles");
        bifrost.shutdown().await.expect("shutdown drains and stops");
        srv.flush_bifrost()
            .await
            .expect("publish the server-owned Scribe");
    }

    /// The SDK's durable write ACK is followed by an Oracle read of exactly
    /// those rows, streamed in Arrow batches and closed by a validated terminal.
    ///
    /// # Panics
    ///
    /// Panics when the write is not acknowledged or the stream returns other
    /// rows or no terminal.
    #[tokio::test]
    async fn oracle_query_returns_arrow_batches_and_terminal() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let client = service_client(&srv, "sdk-oracle-query", &["admin"]).await;
        let fqn = owned_fqn("oracle");
        let rows = [IdValueRow::new(7, "seventh"), IdValueRow::new(8, "eighth")];
        publish_rows(&srv, &client, &fqn, id_value_schema(), &rows).await;

        let mut stream = Bifrost::query_only(&client)
            .query(&BifrostQueryRequest {
                params: Vec::new(),
                sql: format!("SELECT id, value FROM {fqn} ORDER BY id"),
                deadline_ms: None,
            })
            .await
            .expect("the Oracle query starts");
        let mut batches = Vec::new();
        while let Some(batch) = stream.next_batch().await.expect("valid terminal stream") {
            batches.push(batch);
        }
        assert_eq!(id_value_rows(&batches), rows);
        assert_eq!(stream.terminal().expect("validated terminal").row_count, 2);
        srv.shutdown().await.expect("server shutdown");
    }

    /// Owner, foreign-tenant, and role-less callers of one running query.
    struct LifecycleCallers {
        /// Admin of the default tenant, which owns the running query.
        owner: Bifrost,
        /// Admin of a second tenant.
        other: Bifrost,
        /// Role-less service of the second tenant.
        denied: Bifrost,
    }

    impl LifecycleCallers {
        /// Registers and publishes one table, then bootstraps the three callers.
        ///
        /// Returns the callers and the published table the owner queries.
        ///
        /// # Panics
        ///
        /// Panics when the table, the second tenant, or a caller cannot be
        /// provisioned.
        async fn provision(srv: &WyrdTestServer) -> (Self, String) {
            let owner = service_client(srv, "sdk-query-lifecycle", &["admin"]).await;
            let fqn = owned_fqn("lifecycle");
            publish_rows(
                srv,
                &owner,
                &fqn,
                id_value_schema(),
                &[IdValueRow::new(1, "one")],
            )
            .await;
            let other_tenant = srv
                .seed_tenant(&format!(
                    "sdk-lifecycle-other-{}",
                    uuid::Uuid::now_v7().simple()
                ))
                .await
                .expect("seed the second tenant");
            let mut callers = Vec::new();
            for (name, roles) in [
                ("sdk-query-lifecycle-other", &["admin"][..]),
                ("sdk-query-lifecycle-denied", &[][..]),
            ] {
                let bootstrap = srv
                    .bootstrap_service_in_tenant(other_tenant, name, roles)
                    .await
                    .expect("bootstrap the second-tenant caller");
                callers.push(Bifrost::query_only(&client_for(
                    srv,
                    bootstrap.api_key().expect("machine API key").clone(),
                )));
            }
            let denied = callers.pop().expect("denied caller");
            let other = callers.pop().expect("other caller");
            (
                Self {
                    owner: Bifrost::query_only(&owner),
                    other,
                    denied,
                },
                fqn,
            )
        }
    }

    /// HTTP list, status, and cancel are tenant-opaque, RBAC-gated, and
    /// idempotent for a running query.
    ///
    /// The query is parked after its schema by the harness so it is reliably
    /// running. A foreign tenant's status or cancel is indistinguishable from
    /// an unknown id; a role-less caller is refused; the owner's second cancel
    /// reports nothing started.
    ///
    /// # Panics
    ///
    /// Panics when any control answers differently.
    #[tokio::test]
    #[ignore = "requires the controlled Postgres journey harness"]
    async fn oracle_query_status_cancel_is_tenant_scoped() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let (callers, fqn) = LifecycleCallers::provision(&srv).await;
        srv.stall_next_query_after_schema();
        let stream = callers
            .owner
            .query(&BifrostQueryRequest {
                params: Vec::new(),
                sql: format!("SELECT value FROM {fqn}"),
                deadline_ms: Some(30_000),
            })
            .await
            .expect("the owner query starts");
        let request_id = stream.request_id().clone();
        let task = tokio::spawn(async move {
            let mut stream = stream;
            let _ = stream.next_batch().await;
        });
        assert_eq!(
            srv.wait_query_schema_stall().await.expect("query stalls"),
            request_id.as_str()
        );

        let running = callers.owner.running().await.expect("running list");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].request_id, request_id);
        assert_eq!(
            callers
                .owner
                .status(&request_id)
                .await
                .expect("status")
                .request_id,
            request_id
        );
        assert!(
            callers
                .other
                .running()
                .await
                .expect("foreign running list")
                .is_empty()
        );
        let unknown_id = RequestId::now_v7();
        let foreign = callers
            .other
            .status(&request_id)
            .await
            .expect_err("a foreign tenant cannot inspect the query");
        let absent = callers
            .other
            .status(&unknown_id)
            .await
            .expect_err("an unknown query is not found");
        assert_eq!(sdk_code(&foreign), "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND");
        assert_eq!(
            (sdk_code(&foreign), sdk_detail(&foreign)),
            (sdk_code(&absent), sdk_detail(&absent))
        );
        let foreign = callers
            .other
            .cancel(&request_id)
            .await
            .expect_err("a foreign tenant cannot cancel the query");
        let absent = callers
            .other
            .cancel(&unknown_id)
            .await
            .expect_err("an unknown cancellation is not found");
        assert_eq!(sdk_code(&foreign), "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND");
        assert_eq!(
            (sdk_code(&foreign), sdk_detail(&foreign)),
            (sdk_code(&absent), sdk_detail(&absent))
        );
        let denied = callers
            .denied
            .running()
            .await
            .expect_err("a role-less caller cannot list");
        assert_eq!(sdk_code(&denied), "WYRD_PERMISSION_403_DENIED_RBAC");
        let denied = callers
            .denied
            .status(&request_id)
            .await
            .expect_err("a role-less caller cannot inspect");
        assert_eq!(sdk_code(&denied), "WYRD_PERMISSION_403_DENIED_RBAC");
        for started in [true, false] {
            assert_eq!(
                callers
                    .owner
                    .cancel(&request_id)
                    .await
                    .expect("the owner cancels")
                    .cancellation_started,
                started
            );
        }

        task.abort();
        let _ = task.await;
        srv.shutdown().await.expect("server shutdown");
    }

    /// The whole unified-client journey against one real server.
    ///
    /// Register, insert, swap the active table before flushing, insert again,
    /// drain, then read both tables back — one through `sql` and one through
    /// `stream`, so the collected and streamed doors are both exercised on the
    /// rows this journey wrote. A second writer holding a stale declaration of
    /// the first table is refused by the server's schema fingerprint fence,
    /// both for its rows and for registering its schema as a replacement, so
    /// none of its rows land.
    ///
    /// # Panics
    ///
    /// Panics when any step fails or either table reads back other rows.
    #[tokio::test]
    async fn unified_client_registers_writes_swaps_and_reads_both_tables() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let client = service_client(&srv, "sdk-unified-journey", &["admin"]).await;

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

        let first_rows = vec![IdValueRow::new(4, "fourth"), IdValueRow::new(5, "fifth")];
        for row in &first_rows {
            bifrost
                .insert(json_row(row), Correlation::default())
                .expect("insert into the first table");
        }

        // Swap before flushing: the swapped-away producer must still drain, so
        // the rows above are not stranded by the rebinding.
        bifrost.use_table(table(&second_fqn));
        assert_eq!(
            bifrost.register().await.expect("register the second table"),
            RegisterOutcome::Created
        );
        let second_row = IdValueRow::new(7, "second-table");
        bifrost
            .insert(json_row(&second_row), Correlation::default())
            .expect("insert into the second table");
        assert_eq!(
            bifrost.producer_count(),
            2,
            "the swapped-away producer is still pooled"
        );

        bifrost.flush().await.expect("flush every pooled producer");
        bifrost.shutdown().await.expect("shutdown drains and stops");
        assert_stale_writer_is_fenced(&client, &first_fqn).await;
        srv.flush_bifrost()
            .await
            .expect("publish the server-owned Scribe");

        let collected = bifrost
            .sql(
                &format!("SELECT id, value FROM {first_fqn} ORDER BY id"),
                &[],
            )
            .await
            .expect("collect the first table");
        assert_eq!(collected.terminal().outcome, QueryTerminalOutcome::Success);
        assert_eq!(id_value_rows(collected.batches()), first_rows);

        let mut stream = bifrost
            .stream(
                &format!("SELECT id, value FROM {second_fqn} ORDER BY id"),
                &[],
                None,
            )
            .await
            .expect("stream the second table");
        let mut streamed = Vec::new();
        while let Some(batch) = stream.next_batch().await.expect("stream the next batch") {
            streamed.push(batch);
        }
        assert_eq!(id_value_rows(&streamed), vec![second_row]);
        assert!(
            stream.terminal().is_some(),
            "a complete stream carries its validated terminal"
        );

        srv.shutdown().await.expect("server shutdown");
    }

    /// One row of a stale declaration carrying a column the table never had.
    #[derive(Serialize)]
    struct StaleRow {
        /// The registered `id` column.
        id: i64,
        /// The registered `value` column.
        value: &'static str,
        /// The column the registered table never declared.
        note: &'static str,
    }

    /// Prove a writer whose declaration of `fqn` is stale cannot land a row.
    ///
    /// The stale declaration adds a `note` column the registered table never
    /// had. Its sealed batch reaches the server, which refuses it at the schema
    /// fingerprint fence with the stable public code, and the same code refuses
    /// registering the stale declaration as a replacement schema.
    ///
    /// # Panics
    ///
    /// Panics when the stale writer cannot connect or insert, or when either
    /// refusal is missing or carries a different stable code.
    async fn assert_stale_writer_is_fenced(client: &WyrdClient, fqn: &str) {
        let stale_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
            Field::new("note", DataType::Utf8, true),
        ]));
        let stale = Bifrost::connect_with_table(
            client,
            TableConfig::from_arrow(fqn, stale_schema).expect("stale declaration"),
        )
        .await
        .expect("stale writer connects");
        stale
            .insert(
                json_row(&StaleRow {
                    id: 6,
                    value: "stale",
                    note: "outdated",
                }),
                Correlation::default(),
            )
            .expect("the stale row is admitted locally");
        let refused = stale
            .flush()
            .await
            .expect_err("the server fences a batch built from a stale schema");
        assert_eq!(
            sdk_code(&refused),
            "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
        );
        let replacement = stale
            .register()
            .await
            .expect_err("a stale declaration never replaces the registered schema");
        assert_eq!(
            sdk_code(&replacement),
            "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
        );
        stale
            .shutdown()
            .await
            .expect("the refused batch was settled, leaving nothing to drain");
    }

    /// The blocking facade's own journey: register, write, drain, read back.
    ///
    /// A plain `#[test]` rather than a `#[tokio::test]`, because the facade
    /// panics inside an async context by design. Only the harness steps with no
    /// synchronous door — starting the server, minting the client, publishing
    /// the Scribe — are driven on the shared runtime; every Bifrost call is the
    /// synchronous one a caller with no runtime of its own would write.
    ///
    /// # Panics
    ///
    /// Panics when any step fails or a read returns other rows.
    #[test]
    fn blocking_client_registers_writes_and_reads_back() {
        let runtime = wyrd_runtime::runtime();
        let srv = runtime
            .block_on(WyrdTestServer::start_bound())
            .expect("test server start");
        let client = runtime.block_on(service_client(&srv, "sdk-blocking-journey", &["admin"]));

        let fqn = owned_fqn("blocking_journey");
        let bifrost =
            wyrd_client::bifrost::blocking::Bifrost::connect_with_table(&client, table(&fqn))
                .expect("blocking client connects");

        assert_eq!(
            bifrost.register().expect("register the table"),
            RegisterOutcome::Created
        );
        let resolved = bifrost
            .table()
            .expect("the table stays bound")
            .resolved()
            .cloned()
            .expect("register resolves the server identity");

        let rows = vec![IdValueRow::new(1, "first"), IdValueRow::new(2, "second")];
        for row in &rows {
            bifrost
                .insert(json_row(row), Correlation::default())
                .expect("insert into the blocking table");
        }
        bifrost.flush().expect("flush every pooled producer");
        bifrost.shutdown().expect("shutdown drains and stops");
        runtime
            .block_on(srv.flush_bifrost())
            .expect("publish the server-owned Scribe");

        let select = format!("SELECT id, value FROM {fqn} ORDER BY id");
        let collected = bifrost
            .sql(&select, &[])
            .expect("collect through the blocking door");
        assert_eq!(collected.terminal().outcome, QueryTerminalOutcome::Success);
        assert_eq!(id_value_rows(collected.batches()), rows);

        let streamed = bifrost
            .stream(&select, &[], None)
            .expect("stream through the blocking door")
            .collect::<Result<Vec<_>, _>>()
            .expect("every streamed batch");
        assert_eq!(id_value_rows(&streamed), rows);

        let typed: Vec<IdValueRow> = bifrost
            .sql_as(&select, &[])
            .expect("deserialize through the blocking door");
        assert_eq!(typed, rows);

        let (namespace, name) = fqn.rsplit_once('.').expect("the fqn names a namespace");
        let described = bifrost
            .describe_table(namespace, name)
            .expect("describe through the blocking door");
        assert_eq!(described.entry.table_uid, resolved.table_uid);
        assert_eq!(described.entry.fingerprint, resolved.fingerprint);

        runtime.block_on(srv.shutdown()).expect("server shutdown");
    }

    /// The served-inference fact table the typed and analytical journeys read.
    ///
    /// `call_id` rather than `run_id` because the write path owns `run_id`; a
    /// declaration that restates a server-owned column cannot be registered.
    fn inference_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("call_id", DataType::Int64, false),
            Field::new("model", DataType::Utf8, false),
            Field::new("tokens", DataType::Int64, false),
            Field::new("latency_ms", DataType::Float64, false),
            Field::new("status", DataType::Utf8, false),
        ]))
    }

    /// One served inference as a caller declares it.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct InferenceRow {
        /// The fact table's caller-owned call identity.
        call_id: i64,
        /// The served model name.
        model: String,
        /// Tokens the call consumed.
        tokens: i64,
        /// Observed latency in milliseconds.
        latency_ms: f64,
        /// Terminal call status.
        status: String,
    }

    impl InferenceRow {
        /// One inference row.
        fn new(call_id: i64, model: &str, tokens: i64, latency_ms: f64, status: &str) -> Self {
            Self {
                call_id,
                model: model.to_owned(),
                tokens,
                latency_ms,
                status: status.to_owned(),
            }
        }
    }

    /// A row type whose `model` field cannot hold the column's `Utf8` value.
    #[derive(Debug, Deserialize)]
    struct MistypedRow {
        /// Declared as an integer against a string column, so every row fails.
        #[expect(
            dead_code,
            reason = "the field exists to force a deserialization failure"
        )]
        model: i64,
    }

    /// Typed SQL over a written table: rows, the empty result, and a mismatch.
    ///
    /// The projection is local and post-query, so this asserts the three things
    /// a caller can observe about it: a complete result becomes typed rows, a
    /// query matching nothing becomes an empty list rather than an error, and a
    /// row that does not fit the declared type fails the whole read instead of
    /// returning the rows that happened to convert.
    ///
    /// # Panics
    ///
    /// Panics when the typed rows differ, the empty read errors, or the
    /// mismatch does not fail with the stable deserialization code.
    #[tokio::test]
    async fn typed_sql_projects_rows_and_refuses_a_mismatch() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let client = service_client(&srv, "sdk-typed-journey", &["admin"]).await;

        let facts = owned_fqn("typed_inference");
        let inferences = vec![
            InferenceRow::new(1, "opus", 100, 120.5, "ok"),
            InferenceRow::new(2, "haiku", 50, 30.0, "error"),
        ];
        publish_rows(&srv, &client, &facts, inference_schema(), &inferences).await;

        let reader = Bifrost::connect(&client).await.expect("reader connects");
        let select = format!(
            "SELECT call_id, model, tokens, latency_ms, status FROM {facts} ORDER BY call_id"
        );

        // Raw `sql` is unchanged: the same query still collects Arrow batches.
        let raw = reader.sql(&select, &[]).await.expect("raw Arrow result");
        assert_eq!(raw.terminal().outcome, QueryTerminalOutcome::Success);
        assert_eq!(raw.num_rows(), 2);

        let typed: Vec<InferenceRow> = reader.sql_as(&select, &[]).await.expect("typed rows");
        assert_eq!(typed, inferences);

        let empty: Vec<InferenceRow> = reader
            .sql_as(
                &format!(
                    "SELECT call_id, model, tokens, latency_ms, status FROM {facts} \
                     WHERE call_id = 9999"
                ),
                &[],
            )
            .await
            .expect("a query matching nothing is an empty typed result");
        assert!(empty.is_empty());

        let mismatch = reader
            .sql_as::<MistypedRow>(&format!("SELECT model FROM {facts}"), &[])
            .await
            .expect_err("a row that does not fit the declared type fails the read");
        assert_eq!(sdk_code(&mismatch), "WYRD_CLIENT_422_ROW_DESERIALIZATION");
        assert_eq!(wyrd_spec::error::WyrdError::from(&mismatch).status(), 422);

        srv.shutdown().await.expect("server shutdown");
    }

    /// One model dimension row the analytical journey joins against.
    #[derive(Serialize)]
    struct ModelInfoRow {
        /// The model name the facts join on.
        model: &'static str,
        /// The model's vendor.
        vendor: &'static str,
    }

    /// One vendor/model line of the joined analytical summary.
    #[derive(Debug, PartialEq, Deserialize)]
    struct ModelSummary {
        /// The joined vendor.
        vendor: String,
        /// The grouped model.
        model: String,
        /// Calls with status `ok`.
        successes: i64,
        /// Every call.
        attempts: i64,
    }

    /// A join with a filtering aggregate over two caller-registered tables.
    ///
    /// One representative analytical query rather than a tour of DataFusion:
    /// it crosses two written tables, filters inside an aggregate, and groups,
    /// asserting values so a silently wrong plan fails. Counts are cast to a
    /// stable SQL type so the assertion pins values, not accumulator width.
    ///
    /// # Panics
    ///
    /// Panics when the summary differs from the written rows.
    #[tokio::test]
    async fn analytical_sql_over_written_tables() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server start");
        let client = service_client(&srv, "sdk-analytical-journey", &["admin"]).await;

        let facts = owned_fqn("inference");
        let dims = owned_fqn("model_info");
        let inferences = [
            InferenceRow::new(1, "opus", 100, 120.5, "ok"),
            InferenceRow::new(2, "opus", 300, 240.0, "ok"),
            InferenceRow::new(3, "opus", 200, 180.25, "error"),
            InferenceRow::new(4, "haiku", 50, 30.0, "ok"),
            InferenceRow::new(5, "haiku", 150, 60.75, "ok"),
        ];
        let model_info = [
            ModelInfoRow {
                model: "opus",
                vendor: "anthropic",
            },
            ModelInfoRow {
                model: "haiku",
                vendor: "anthropic",
            },
        ];
        publish_rows(&srv, &client, &facts, inference_schema(), &inferences).await;
        let dims_schema = Arc::new(Schema::new(vec![
            Field::new("model", DataType::Utf8, false),
            Field::new("vendor", DataType::Utf8, false),
        ]));
        publish_rows(&srv, &client, &dims, dims_schema, &model_info).await;

        let summary: Vec<ModelSummary> = Bifrost::connect(&client)
            .await
            .expect("reader connects")
            .sql_as(
                &format!(
                    "SELECT d.vendor, f.model, \
                        CAST(COUNT(*) FILTER (WHERE f.status = 'ok') AS BIGINT) AS successes, \
                        CAST(COUNT(*) AS BIGINT) AS attempts \
                     FROM {facts} AS f INNER JOIN {dims} AS d ON f.model = d.model \
                     GROUP BY d.vendor, f.model ORDER BY f.model"
                ),
                &[],
            )
            .await
            .expect("joined aggregate");
        assert_eq!(
            summary,
            vec![
                ModelSummary {
                    vendor: "anthropic".to_owned(),
                    model: "haiku".to_owned(),
                    successes: 2,
                    attempts: 2,
                },
                ModelSummary {
                    vendor: "anthropic".to_owned(),
                    model: "opus".to_owned(),
                    successes: 2,
                    attempts: 3,
                },
            ]
        );

        srv.shutdown().await.expect("server shutdown");
    }

    /// A server holding the canonical signal fixture under one fresh scope.
    struct CanonicalSignals {
        /// The running server.
        server: WyrdTestServer,
        /// The admin writer, also used to read.
        bifrost: Bifrost,
        /// The instrumentation scope every fixture row carries.
        scope: String,
    }

    impl CanonicalSignals {
        /// Describes, builds, writes, and publishes one span, log, and metric
        /// batch through the public Arrow door.
        ///
        /// The batches are built from the schemas `describe` publishes, never
        /// from a second hard-coded ledger, so this is exactly a caller's path.
        ///
        /// # Panics
        ///
        /// Panics when a describe, write, or publish fails.
        async fn written() -> Self {
            let server = WyrdTestServer::start_bound()
                .await
                .expect("test server start");
            let client = service_client(&server, "sdk-canonical-writer", &["admin"]).await;
            let bifrost = Bifrost::connect(&client).await.expect("writer connects");
            let scope = format!("wyrd.sdk.canonical.{}", uuid::Uuid::now_v7().simple());
            let anchor = 1_760_000_000_000_000_000_i64;
            for (fqn, build) in [
                (
                    "vala.traces.spans",
                    fixture::spans as fn(&SchemaRef, &str, i64) -> RecordBatch,
                ),
                ("vala.logs.records", fixture::logs),
                ("vala.metrics.points", fixture::points),
            ] {
                let described = TableConfig::describe(&client, fqn)
                    .await
                    .expect("describe the canonical table");
                bifrost
                    .write_batch(fqn, &build(described.user_schema(), &scope, anchor))
                    .await
                    .expect("the canonical batch is accepted");
            }
            server
                .flush_bifrost()
                .await
                .expect("publish the server-owned Scribe");
            Self {
                server,
                bifrost,
                scope,
            }
        }

        /// Collects `sql` through the writer and returns its batches.
        ///
        /// # Panics
        ///
        /// Panics when the query fails or its terminal is not a success.
        async fn read(&self, sql: &str) -> Vec<RecordBatch> {
            let result = self.bifrost.sql(sql, &[]).await.expect("canonical read");
            assert_eq!(result.terminal().outcome, QueryTerminalOutcome::Success);
            result.batches().to_vec()
        }
    }

    /// A GenAI trace reads back as one root chat span and its failed tool
    /// span, and its token usage aggregates by model.
    ///
    /// # Panics
    ///
    /// Panics when the hierarchy, status, or token totals differ from the
    /// fixture.
    #[tokio::test]
    async fn canonical_spans_read_back_as_a_genai_trace() {
        let signals = CanonicalSignals::written().await;
        let scope = &signals.scope;
        let hierarchy = signals
            .read(&format!(
                "SELECT gen_ai_operation_name, status_code, \
                    CAST(CASE WHEN parent_span_id IS NULL THEN 1 ELSE 0 END AS BIGINT) AS is_root \
                 FROM vala.traces.spans WHERE scope_name = '{scope}' \
                 ORDER BY start_time_unix_nano"
            ))
            .await;
        assert_eq!(
            string_col(&hierarchy, "gen_ai_operation_name"),
            vec!["chat", "execute_tool"]
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(&hierarchy, "is_root"),
            vec![Some(1), Some(0)],
            "exactly the parent is a root; the tool span carries its parent id"
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Int32Type>(&hierarchy, "status_code"),
            vec![Some(1), Some(2)],
            "the tool span records the error status the fixture wrote"
        );

        let tokens = signals
            .read(&format!(
                "SELECT CAST(SUM(gen_ai_usage_input_tokens) AS BIGINT) AS input_tokens, \
                    CAST(SUM(gen_ai_usage_output_tokens) AS BIGINT) AS output_tokens \
                 FROM vala.traces.spans \
                 WHERE scope_name = '{scope}' AND gen_ai_request_model = '{model}'",
                model = fixture::MODEL
            ))
            .await;
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(&tokens, "input_tokens"),
            vec![Some(fixture::INPUT_TOKENS + 64)]
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(&tokens, "output_tokens"),
            vec![Some(fixture::OUTPUT_TOKENS + 16)]
        );
        signals.server.shutdown().await.expect("server shutdown");
    }

    /// The error log joins to the tool span that failed, not to the root.
    ///
    /// # Panics
    ///
    /// Panics when the joined log line differs from the fixture.
    #[tokio::test]
    async fn canonical_error_log_correlates_to_its_span() {
        let signals = CanonicalSignals::written().await;
        let correlated = signals
            .read(&format!(
                "SELECT l.severity_text, l.event_name, s.name AS span_name \
                 FROM vala.logs.records l JOIN vala.traces.spans s \
                   ON l.trace_id = s.trace_id AND l.span_id = s.span_id \
                 WHERE l.scope_name = '{}'",
                signals.scope
            ))
            .await;
        assert_eq!(string_col(&correlated, "severity_text"), vec!["ERROR"]);
        assert_eq!(
            string_col(&correlated, "event_name"),
            vec!["tool.retry.exhausted"]
        );
        assert_eq!(
            string_col(&correlated, "span_name"),
            vec!["execute_tool search"]
        );
        signals.server.shutdown().await.expect("server shutdown");
    }

    /// Gauge, histogram, and sum points aggregate by kind into their own value
    /// columns.
    ///
    /// # Panics
    ///
    /// Panics when a kind's aggregate differs from the fixture.
    #[tokio::test]
    async fn canonical_metric_points_aggregate_by_kind() {
        let signals = CanonicalSignals::written().await;
        let metrics = signals
            .read(&format!(
                "SELECT metric_type, \
                    CAST(SUM(COALESCE(int_value, 0)) AS BIGINT) AS ints, \
                    CAST(SUM(COALESCE(double_value, 0.0)) AS DOUBLE) AS doubles, \
                    CAST(SUM(COALESCE(histogram_count, 0)) AS BIGINT) AS observations \
                 FROM vala.metrics.points WHERE scope_name = '{}' \
                 GROUP BY metric_type ORDER BY metric_type",
                signals.scope
            ))
            .await;
        assert_eq!(
            string_col(&metrics, "metric_type"),
            vec!["gauge", "histogram", "sum"]
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(&metrics, "ints"),
            vec![Some(0), Some(0), Some(fixture::COUNTER_VALUE)]
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Float64Type>(&metrics, "doubles"),
            vec![Some(fixture::GAUGE_VALUE), Some(0.0), Some(0.0)]
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(&metrics, "observations"),
            vec![Some(0), Some(fixture::HISTOGRAM_COUNT), Some(0)]
        );
        signals.server.shutdown().await.expect("server shutdown");
    }

    /// Table query permission alone reads a span's complete row: its nested
    /// event and link, and its structured GenAI messages verbatim.
    ///
    /// # Panics
    ///
    /// Panics when the query-only reader is refused or a nested or payload
    /// value differs from the fixture.
    #[tokio::test]
    async fn canonical_span_payload_reads_back_for_a_query_reader() {
        let signals = CanonicalSignals::written().await;
        let server = &signals.server;
        let permission: wyrd_runtime::Permission =
            "bifrost_query:read".parse().expect("a declared permission");
        server
            .seed_role("sdk_canonical_reader", &[permission])
            .await
            .expect("seed the query-only role");
        let reader = Bifrost::query_only(
            &service_client(server, "sdk_canonical_reader", &["sdk_canonical_reader"]).await,
        );
        let scope = &signals.scope;
        let nested = reader
            .sql(
                &format!(
                    "SELECT CAST(array_length(events) AS BIGINT) AS events, \
                     CAST(array_length(links) AS BIGINT) AS links, \
                     events[1]['name'] AS event_name, links[1]['trace_state'] AS link_state \
                     FROM vala.traces.spans \
                     WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
                ),
                &[],
            )
            .await
            .expect("a query-only caller reads the nested event and link");
        let nested = nested.batches();
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(nested, "events"),
            vec![Some(1)]
        );
        assert_eq!(
            primitive_col::<arrow::datatypes::Int64Type>(nested, "links"),
            vec![Some(1)]
        );
        assert_eq!(
            string_col(nested, "event_name"),
            vec![fixture::EVENT_NAME.to_owned()]
        );
        assert_eq!(
            string_col(nested, "link_state"),
            vec![fixture::LINK_TRACE_STATE.to_owned()]
        );

        let messages = reader
            .sql(
                &format!(
                    "SELECT attributes FROM vala.traces.spans \
                     WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
                ),
                &[],
            )
            .await
            .expect("a query-only caller reads the structured GenAI messages");
        let batches = messages.batches();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let stored = arrow::compute::cast(
            batches[0]
                .column_by_name("attributes")
                .expect("column `attributes`"),
            &DataType::Binary,
        )
        .expect("the canonical attribute payload reads back as bytes");
        let payload = stored
            .as_any()
            .downcast_ref::<arrow::array::BinaryArray>()
            .expect("the cast payload is Binary")
            .value(0)
            .to_vec();
        let payload = String::from_utf8_lossy(&payload);
        assert!(payload.contains(fixture::INPUT_MESSAGES));
        assert!(payload.contains(fixture::OUTPUT_MESSAGES));

        signals.server.shutdown().await.expect("server shutdown");
    }
}
