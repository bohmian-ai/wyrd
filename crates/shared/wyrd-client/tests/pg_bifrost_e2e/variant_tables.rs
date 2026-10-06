//! Tables with free-form, union, and nested fields.
//!
//! A model field typed `serde_json::Value` or as an untagged enum is stored as
//! a Variant column, a nested struct as a Struct column, and a `Vec` as a List
//! column. Rows and Arrow batches go in; native Rust values come back out.

mod pg_tests {
    use std::sync::Arc;

    use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
    use arrow_schema::{DataType, Field, Schema};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use wyrd_client::WyrdClient;
    use wyrd_client::bifrost::{Bifrost, BifrostClientError, Correlation, TableConfig};
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_spec::vala::api::{
        DataTypeSpec, FieldSpec, RegisterTableRequest, RegisterTableResponse,
    };
    use wyrd_testing::server::WyrdTestServer;

    /// A fixed nested model, stored as a Struct column.
    #[derive(Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
    struct Point {
        /// A required field inside the struct.
        x: i64,
        /// An optional field inside the struct.
        label: Option<String>,
    }

    /// Either shape of the `mixed` field; a union is stored as Variant.
    #[derive(Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(untagged)]
    enum Mixed {
        /// A number.
        Number(i64),
        /// A string.
        Text(String),
    }

    /// One row using every column shape a model can declare.
    #[derive(Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    struct Event {
        /// Row id.
        id: i64,
        /// Free-form JSON, stored as Variant.
        payload: Option<Value>,
        /// A union, stored as Variant.
        mixed: Option<Mixed>,
        /// A nested model, stored as Struct.
        point: Option<Point>,
        /// A typed list, stored as List.
        tags: Option<Vec<String>>,
    }

    /// The query that reads every `Event` column back.
    fn select_events(table: &str) -> String {
        format!("SELECT id, payload, mixed, point, tags FROM {table} ORDER BY id")
    }

    /// A running server plus a client writing to a freshly registered
    /// `Event` table.
    struct Events {
        /// The test server; shut down by [`Events::stop`].
        server: WyrdTestServer,
        /// An SDK client authenticated with the server's write key.
        client: WyrdClient,
        /// The Bifrost client bound to [`Events::table`].
        bifrost: Bifrost,
        /// The registered table's `namespace.name`.
        table: String,
    }

    impl Events {
        /// Start a server and register a fresh `Event` table on it.
        ///
        /// # Panics
        ///
        /// Panics when the server, client, or registration fails.
        async fn start() -> Self {
            let server = WyrdTestServer::start_bound().await.expect("server starts");
            let writer = server
                .bootstrap_service("variant-writer", &["admin"])
                .await
                .expect("writer bootstraps");
            let client = WyrdClient::with_config(ClientConfig {
                grpc: GrpcConfig {
                    endpoint: server.grpc_url().expect("gRPC URL"),
                    ..GrpcConfig::default()
                },
                http: HttpConfig {
                    base_url: server.base_url().expect("HTTP URL").to_owned(),
                    ..HttpConfig::default()
                },
                credential: Some(writer.api_key().expect("machine API key").clone()),
                ..ClientConfig::default()
            })
            .expect("client connects");
            let table = format!("vala.datasets.events_{}", uuid::Uuid::now_v7().simple());
            let bifrost = Bifrost::connect_with_table(
                &client,
                TableConfig::from_model::<Event>(&table).expect("Event declares a table"),
            )
            .await
            .expect("bifrost connects");
            bifrost.register().await.expect("table registers");
            Self {
                server,
                client,
                bifrost,
                table,
            }
        }

        /// Insert rows, flush them, and make them queryable.
        ///
        /// # Panics
        ///
        /// Panics when an insert, flush, or publish fails.
        async fn insert(&self, events: &[Event]) {
            for event in events {
                let row = serde_json::to_vec(event).expect("event serializes");
                self.bifrost
                    .insert(row, Correlation::default())
                    .expect("row is accepted");
            }
            self.bifrost.flush().await.expect("rows flush");
            self.server.flush_bifrost().await.expect("rows publish");
        }

        /// Read every `Event` row back, ordered by id.
        ///
        /// # Panics
        ///
        /// Panics when the query fails.
        async fn read(&self, table: &str) -> Vec<Event> {
            self.bifrost
                .sql_as(&select_events(table))
                .await
                .expect("rows read")
        }

        /// Shut the server down.
        ///
        /// # Panics
        ///
        /// Panics when shutdown fails.
        async fn stop(self) {
            self.server.shutdown().await.expect("server stops");
        }
    }

    /// A one-row batch the way a user writes it: an id plus JSON text for the
    /// `payload` and `mixed` Variant columns. Omitted nullable columns are
    /// filled with nulls by `write_batch`.
    ///
    /// # Panics
    ///
    /// Panics when Arrow refuses the batch.
    fn json_text_batch(payload: &str, mixed: &str) -> RecordBatch {
        RecordBatch::try_from_iter([
            ("id", Arc::new(Int64Array::from(vec![1])) as ArrayRef),
            ("payload", Arc::new(StringArray::from(vec![payload]))),
            ("mixed", Arc::new(StringArray::from(vec![mixed]))),
        ])
        .expect("batch builds")
    }

    /// Whether a field carries the Arrow Variant extension.
    fn is_variant(field: &Field) -> bool {
        field.extension_type_name() == Some("arrow.parquet.variant")
    }

    /// The stable code an SDK error carries.
    fn code(error: &BifrostClientError) -> &'static str {
        wyrd_spec::error::WyrdError::from(error).code()
    }

    /// Model fields map to Variant, Struct, and List columns.
    #[tokio::test]
    async fn model_fields_become_variant_struct_and_list_columns() {
        let events = Events::start().await;

        let described = TableConfig::describe(&events.client, &events.table)
            .await
            .expect("table describes");
        let schema = described.user_schema();

        assert!(is_variant(
            schema.field_with_name("payload").expect("payload")
        ));
        assert!(is_variant(schema.field_with_name("mixed").expect("mixed")));
        assert!(matches!(
            schema.field_with_name("point").expect("point").data_type(),
            DataType::Struct(_)
        ));
        assert!(matches!(
            schema.field_with_name("tags").expect("tags").data_type(),
            DataType::List(_)
        ));
        events.stop().await;
    }

    /// Inserted rows read back as the same native values, including a
    /// `u64::MAX` inside free-form JSON and a JSON-looking string.
    #[tokio::test]
    async fn rows_come_back_as_native_values() {
        let events = Events::start().await;
        let written = vec![
            Event {
                id: 1,
                payload: Some(json!({"scores": [1, {"z": null}], "big": u64::MAX})),
                mixed: Some(Mixed::Number(5)),
                point: Some(Point {
                    x: 1,
                    label: Some("a".to_owned()),
                }),
                tags: Some(vec!["a".to_owned(), "b".to_owned()]),
            },
            Event {
                id: 2,
                payload: Some(Value::String(r#"{"stays": "a string"}"#.to_owned())),
                mixed: Some(Mixed::Text("five".to_owned())),
                tags: Some(Vec::new()),
                ..Event::default()
            },
        ];

        events.insert(&written).await;

        assert_eq!(events.read(&events.table).await, written);
        events.stop().await;
    }

    /// A Utf8 column of JSON text is stored as Variant.
    #[tokio::test]
    async fn arrow_json_text_is_stored_as_variant() {
        let events = Events::start().await;
        let batch = json_text_batch(r#"{"n": 9007199254740993}"#, r#""seven""#);

        events
            .bifrost
            .write_batch(&events.table, &batch)
            .await
            .expect("batch is accepted");
        events.server.flush_bifrost().await.expect("rows publish");

        let expected = Event {
            id: 1,
            payload: Some(json!({"n": 9_007_199_254_740_993_i64})),
            mixed: Some(Mixed::Text("seven".to_owned())),
            ..Event::default()
        };
        assert_eq!(events.read(&events.table).await, vec![expected]);
        events.stop().await;
    }

    /// Query results keep the Variant extension and copy into another table.
    #[tokio::test]
    async fn query_results_copy_into_another_table() {
        let events = Events::start().await;
        let archive = format!("vala.datasets.archive_{}", uuid::Uuid::now_v7().simple());
        Bifrost::connect_with_table(
            &events.client,
            TableConfig::from_model::<Event>(&archive).expect("table"),
        )
        .await
        .expect("bifrost connects")
        .register()
        .await
        .expect("archive registers");
        let event = Event {
            id: 1,
            payload: Some(json!({"source": "row"})),
            mixed: Some(Mixed::Number(8)),
            point: Some(Point { x: 1, label: None }),
            tags: Some(vec!["a".to_owned()]),
        };
        events.insert(std::slice::from_ref(&event)).await;

        let copied = events
            .bifrost
            .sql(&select_events(&events.table))
            .await
            .expect("query runs");
        assert!(is_variant(
            copied.schema().field_with_name("payload").expect("payload")
        ));
        for batch in copied.batches() {
            events
                .bifrost
                .write_batch(&archive, batch)
                .await
                .expect("copy is accepted");
        }
        events.server.flush_bifrost().await.expect("rows publish");

        assert_eq!(events.read(&archive).await, vec![event]);
        events.stop().await;
    }

    /// Struct field access returns the typed column; Variant path access
    /// returns Variant.
    #[tokio::test]
    async fn struct_fields_stay_typed_and_variant_paths_stay_variant() {
        let events = Events::start().await;
        events
            .insert(&[Event {
                id: 1,
                payload: Some(json!({"scores": [1, 2]})),
                point: Some(Point { x: 7, label: None }),
                ..Event::default()
            }])
            .await;

        let result = events
            .bifrost
            .sql(&format!(
                "SELECT point['x'] AS x, payload -> 'scores' AS scores FROM {}",
                events.table
            ))
            .await
            .expect("query runs");

        assert_eq!(
            result.schema().field_with_name("x").expect("x").data_type(),
            &DataType::Int64
        );
        assert!(is_variant(
            result.schema().field_with_name("scores").expect("scores")
        ));
        events.stop().await;
    }

    /// One bad row refuses the whole multi-row insert, so nothing is written.
    #[tokio::test]
    async fn undeclared_field_refuses_the_whole_insert() {
        let events = Events::start().await;
        let writer = events
            .bifrost
            .writer_table(&events.table)
            .await
            .expect("writer table");

        let refused = events
            .bifrost
            .insert_rows_into(
                &writer,
                vec![
                    br#"{"id": 1}"#.to_vec(),
                    br#"{"id": 2, "undeclared": true}"#.to_vec(),
                ],
                Correlation::default(),
            )
            .expect_err("the second row has an undeclared field");
        events.bifrost.flush().await.expect("nothing to flush");
        events.server.flush_bifrost().await.expect("publish");

        assert_eq!(code(&refused), "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD");
        assert_eq!(events.read(&events.table).await, Vec::new());
        events.stop().await;
    }

    /// Invalid JSON text is refused before the batch is sent.
    #[tokio::test]
    async fn invalid_json_text_is_refused() {
        let events = Events::start().await;
        let batch = json_text_batch("{not json", "1");

        let refused = events
            .bifrost
            .write_batch(&events.table, &batch)
            .await
            .expect_err("the payload is not JSON");
        events.server.flush_bifrost().await.expect("publish");

        assert_eq!(code(&refused), "WYRD_VALA_400_VARIANT_INVALID_JSON");
        assert_eq!(events.read(&events.table).await, Vec::new());
        events.stop().await;
    }

    /// An unsigned column is refused by the SDK, and by the server when sent
    /// directly, and no table is created.
    #[tokio::test]
    async fn unsupported_type_is_refused_by_sdk_and_server() {
        let events = Events::start().await;
        let table = format!("vala.datasets.unsigned_{}", uuid::Uuid::now_v7().simple());
        let schema = Arc::new(Schema::new(vec![Field::new(
            "count",
            DataType::UInt64,
            true,
        )]));

        let sdk = TableConfig::from_arrow(&table, schema).expect_err("SDK refuses UInt64");
        assert_eq!(code(&sdk), "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE");

        let (namespace, name) = table.rsplit_once('.').expect("namespace.name");
        let server = events
            .client
            .request_json::<_, RegisterTableResponse>(
                reqwest::Method::POST,
                "/v1/bifrost/tables",
                Some(&RegisterTableRequest {
                    namespace: namespace.to_owned(),
                    name: name.to_owned(),
                    fields: vec![FieldSpec {
                        name: "count".to_owned(),
                        data_type: DataTypeSpec::UInt64,
                        nullable: true,
                        metadata: Default::default(),
                    }],
                    physical_layout: None,
                    compaction_target_file_size_bytes: None,
                    compaction_type: None,
                }),
            )
            .await
            .expect_err("server refuses UInt64");
        assert_eq!(server.code(), "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE");

        let missing = TableConfig::describe(&events.client, &table)
            .await
            .expect_err("no table");
        assert_eq!(code(&missing), "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");
        events.stop().await;
    }
}
