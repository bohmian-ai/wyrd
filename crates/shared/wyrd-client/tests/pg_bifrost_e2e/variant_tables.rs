//! Tables with free-form, union, and nested fields.
//!
//! A model field typed `serde_json::Value` or as an untagged enum is stored as
//! a Variant column, a nested struct as a Struct column, and a `Vec` as a List
//! column. Rows and Arrow batches go in; native Rust values come back out.

mod pg_tests {
    use std::sync::Arc;

    use std::collections::BTreeMap;

    use arrow::array::{
        ArrayRef, AsArray, BinaryArray, Int64Array, RecordBatch, StringArray, StructArray,
    };
    use arrow_schema::{DataType, Field, Schema, TimeUnit};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use wyrd_client::WyrdClient;
    use wyrd_client::bifrost::{Bifrost, BifrostClientError, Correlation, TableConfig};
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_queue::variant::{EncodedVariant, is_variant};
    use wyrd_queue::{QueueConfig, RowPreflight};
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
                    // The server's frame ceiling, so a direct writer can send
                    // a Variant cell past the 8 MiB value limit.
                    max_message_bytes: 16 * 1024 * 1024,
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
        let DataType::List(item) = schema.field_with_name("tags").expect("tags").data_type() else {
            panic!("tags is a List");
        };
        assert!(!item.is_nullable(), "Vec<String> items are never null");
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

    /// A model that allows extra keys beside its declared fields.
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct Loose {
        /// A declared field.
        id: i64,
        /// Every other key, which no column could hold.
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    }

    /// A model allowing extra keys is refused with the remediation to
    /// declare a Variant field instead.
    #[tokio::test]
    async fn model_allowing_extra_keys_is_refused() {
        let refused = TableConfig::from_model::<Loose>("vala.datasets.loose")
            .expect_err("extra keys could be lost");

        assert_eq!(code(&refused), "WYRD_VALA_400_SCHEMA_PARSE");
        assert!(
            refused
                .to_string()
                .contains("declare a Variant field for open data"),
            "{refused}"
        );
    }

    /// Every type Iceberg cannot store is refused when the table is
    /// declared, naming the field and its type.
    #[tokio::test]
    async fn each_unsupported_type_is_refused_when_declared() {
        let new_york = Some("America/New_York".into());
        for data_type in [
            DataType::UInt64,
            DataType::Date64,
            DataType::Time32(TimeUnit::Second),
            DataType::Timestamp(TimeUnit::Second, None),
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            DataType::Timestamp(TimeUnit::Microsecond, new_york),
        ] {
            let schema = Arc::new(Schema::new(vec![Field::new(
                "moment",
                data_type.clone(),
                true,
            )]));

            let refused =
                TableConfig::from_arrow("vala.datasets.unsupported", schema).expect_err("refused");

            assert_eq!(code(&refused), "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE");
            let message = refused.to_string();
            assert!(
                message.contains("moment") && message.contains(&data_type.to_string()),
                "{message}"
            );
        }
    }

    /// One Variant cell whose bytes are written directly, bypassing JSON
    /// encoding.
    ///
    /// # Panics
    ///
    /// Panics when Arrow refuses the storage arrays.
    fn variant_cell(metadata: &[u8], value: &[u8]) -> ArrayRef {
        let DataType::Struct(storage) = wyrd_queue::variant::variant_storage_type() else {
            panic!("Variant storage is a struct");
        };
        Arc::new(StructArray::new(
            storage,
            vec![
                Arc::new(BinaryArray::from(vec![metadata])) as ArrayRef,
                Arc::new(BinaryArray::from(vec![value])),
            ],
            None,
        ))
    }

    /// Wrap one encoded Variant value in a one-element array, adding a level
    /// of nesting without the encoder's depth check.
    fn wrap_in_array(child: &[u8]) -> Vec<u8> {
        // Array header: basic type 3, four-byte offsets, one-byte count.
        let mut value = vec![0x0F, 1];
        value.extend_from_slice(&0_u32.to_le_bytes());
        value.extend_from_slice(&u32::try_from(child.len()).expect("fits").to_le_bytes());
        value.extend_from_slice(child);
        value
    }

    /// Return `batch` with the column named like `field` replaced by
    /// `field` and `column`, keeping every other column unchanged.
    ///
    /// # Panics
    ///
    /// Panics when the batch has no such column or Arrow refuses the result.
    fn with_column(batch: &RecordBatch, field: Field, column: ArrayRef) -> RecordBatch {
        let index = batch
            .schema()
            .index_of(field.name())
            .expect("column exists");
        let mut fields: Vec<Field> = batch
            .schema()
            .fields()
            .iter()
            .map(|field| field.as_ref().clone())
            .collect();
        let mut columns = batch.columns().to_vec();
        fields[index] = field;
        columns[index] = column;
        RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).expect("batch builds")
    }

    /// Variant data sent straight to the server, skipping the SDK's checks,
    /// is refused before anything is stored. Variant identity must match the
    /// registered table: a `payload` Variant sent without its extension or
    /// with a foreign one, and the Variant extension on the ordinary `point`
    /// Struct or its nested `label`, are unsupported types — even ahead of
    /// malformed bytes in an earlier column. Malformed, too deep, and too
    /// large Variant bytes get their own errors. A valid batch on the same
    /// path is stored.
    #[tokio::test]
    async fn server_refuses_unstorable_variant_bytes_sent_directly() {
        let events = Events::start().await;
        // `enqueue_batch` sends a batch unchanged; messages may carry a cell
        // past the 8 MiB Variant limit.
        let direct = Bifrost::connect_with_config(
            &events.client,
            None,
            QueueConfig {
                max_message_bytes: 12 * 1024 * 1024,
                ..QueueConfig::default()
            },
        )
        .await
        .expect("direct writer connects");
        let described = events
            .bifrost
            .describe(&events.table)
            .await
            .expect("table describes");
        let valid = RowPreflight::from_description(&described)
            .expect("preflight")
            .prepare(&[br#"{"id": 1, "payload": {"ok": true}}"#], None, None)
            .expect("row prepares")
            .batch()
            .clone();
        let payload = valid.schema().index_of("payload").expect("payload column");

        let mut deep = json!(1);
        for _ in 0..64 {
            deep = json!([deep]);
        }
        let deep = EncodedVariant::from_json(&deep).expect("64 levels encode");
        let small = EncodedVariant::from_json(&json!(1)).expect("encodes");
        let schema = valid.schema();
        let payload_field = schema.field(payload).clone();
        let payload_bytes = |cell: ArrayRef| with_column(&valid, payload_field.clone(), cell);
        let labelled = |field: Field, extension: &str| {
            field.with_metadata(std::collections::HashMap::from([(
                "ARROW:extension:name".to_owned(),
                extension.to_owned(),
            )]))
        };
        let point_field = schema
            .field_with_name("point")
            .expect("point column")
            .clone();
        let point = valid
            .column_by_name("point")
            .expect("point column")
            .as_struct()
            .clone();
        let (children, point_columns, point_nulls) = point.into_parts();
        let children: arrow_schema::Fields = children
            .iter()
            .map(|child| match child.name().as_str() {
                "label" => labelled(child.as_ref().clone(), "arrow.parquet.variant"),
                _ => child.as_ref().clone(),
            })
            .collect();
        let nested_label = Arc::new(
            StructArray::try_new(children.clone(), point_columns, point_nulls)
                .expect("point rebuilds"),
        ) as ArrayRef;
        let unsupported = "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE";
        for (batch, expected) in [
            (
                with_column(
                    &valid,
                    payload_field.clone().with_metadata(Default::default()),
                    Arc::clone(valid.column(payload)),
                ),
                unsupported,
            ),
            (
                with_column(
                    &valid,
                    labelled(payload_field.clone(), "acme.variant"),
                    Arc::clone(valid.column(payload)),
                ),
                unsupported,
            ),
            (
                with_column(
                    &payload_bytes(variant_cell(small.metadata(), &[0xFF])),
                    labelled(point_field.clone(), "arrow.parquet.variant"),
                    Arc::clone(valid.column_by_name("point").expect("point column")),
                ),
                unsupported,
            ),
            (
                with_column(
                    &valid,
                    point_field
                        .clone()
                        .with_data_type(DataType::Struct(children)),
                    nested_label,
                ),
                unsupported,
            ),
            (
                payload_bytes(variant_cell(small.metadata(), &[0xFF])),
                "WYRD_VALA_400_VARIANT_INVALID_JSON",
            ),
            (
                payload_bytes(variant_cell(deep.metadata(), &wrap_in_array(deep.value()))),
                "WYRD_VALA_400_VARIANT_TOO_DEEP",
            ),
            (
                payload_bytes(variant_cell(
                    small.metadata(),
                    &vec![0; 8 * 1024 * 1024 + 1],
                )),
                "WYRD_VALA_413_VARIANT_TOO_LARGE",
            ),
        ] {
            direct
                .enqueue_batch(&events.table, batch, None)
                .expect("the SDK sends the batch unchanged");
            let refused = direct.flush().await.expect_err("the server refuses");
            events.server.flush_bifrost().await.expect("publish");

            assert_eq!(code(&refused), expected, "{refused}");
            assert_eq!(events.read(&events.table).await, Vec::new());
        }

        direct
            .enqueue_batch(&events.table, valid, None)
            .expect("valid batch sends");
        direct.flush().await.expect("valid batch is stored");
        events.server.flush_bifrost().await.expect("publish");
        let stored = Event {
            id: 1,
            payload: Some(json!({"ok": true})),
            ..Event::default()
        };
        assert_eq!(events.read(&events.table).await, vec![stored]);
        events.stop().await;
    }
}
