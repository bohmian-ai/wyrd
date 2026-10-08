//! Describe reaches the exact stored physical schema for both table kinds.

use arrow::array::Array;
use wyrd_client::Bifrost;
use wyrd_spec::vala::api::BifrostQueryRequest;

use super::support::{start_scribe_server, unique_table};

/// A writer never keeps its own copy of the physical contract.
///
/// For a canonical signal table the description's projected Arrow schema is
/// the registry's physical schema — every name, type shape, and nullability —
/// plus the one Gate input (`card_ref`) the write path resolves rather than
/// stores, and the exact canonical physical fingerprint. Types compare through
/// the storage layer's own round-trip equivalence, which is what "the stored
/// schema" means once Iceberg has widened a byte or string column. For a
/// dynamic table, a batch built straight from the description is accepted by
/// the real ingest wire and lands with optional Card correlation: the
/// correlated row keeps its `card_uid`, the uncorrelated row stores none and
/// still carries its authenticated principal.
///
/// # Panics
///
/// Panics when a described schema diverges from the stored physical schema,
/// the canonical fingerprint differs from the registry's, or a
/// description-built batch is refused or reads back with other correlation.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn described_canonical_and_dynamic_schemas_reach_exact_physical_schema() {
    let server = start_scribe_server().await;
    let table_name = unique_table("described");
    let fqn = format!("vala.datasets.{table_name}");
    let bootstrap = server
        .bootstrap_service(&unique_table("describe_writer"), &["admin"])
        .await
        .expect("bootstrap the describing writer");
    let writer_card = bootstrap.card_ref().expect("machine card ref").clone();
    let client = wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
        grpc: wyrd_client::transport::GrpcConfig {
            endpoint: server.grpc_url().expect("gRPC URL"),
            connect_retries: 0,
            ..wyrd_client::transport::GrpcConfig::default()
        },
        http: wyrd_client::transport::HttpConfig {
            base_url: server.base_url().expect("HTTP URL").to_owned(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        credential: Some(bootstrap.api_key().expect("machine API key").clone()),
        ..wyrd_client::config::ClientConfig::default()
    })
    .expect("writer client");
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Utf8, false),
    ]));
    Bifrost::connect_with_table(
        &client,
        wyrd_client::bifrost::TableConfig::from_arrow(&fqn, schema).expect("declared table"),
    )
    .await
    .expect("the writer connects")
    .register()
    .await
    .expect("register the dynamic table");
    let query = Bifrost::query_only(&client);

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
    for stored in (definition.schema)().fields() {
        let name = stored.name().as_str();
        // Server-resolved columns are not a writer's to supply; `wyrd_event_time`
        // and `run_id` are the two managed columns a writer may still send.
        if vala_bifrost_redux::tables::managed_columns::is_managed_column(name)
            && name != "wyrd_event_time"
            && name != "run_id"
        {
            continue;
        }
        let described = described_schema
            .field_with_name(name)
            .unwrap_or_else(|_| panic!("described schema keeps `{name}`"));
        assert!(
            vala_bifrost_redux::tables::arrow_type_shape_matches(
                stored.data_type(),
                described.data_type()
            ),
            "`{name}` keeps its stored type shape"
        );
        assert_eq!(
            described.is_nullable(),
            stored.is_nullable(),
            "`{name}` keeps its stored nullability"
        );
        assert!(
            described.metadata().is_empty(),
            "`{name}` sends no field identity; ingress compares by shape"
        );
    }
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

    let (namespace, name) = fqn.rsplit_once('.').expect("the fqn names a namespace");
    let dynamic = query
        .describe_table(namespace, name)
        .await
        .expect("describe the dynamic table");
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
            None,
        )
        .expect("a described row is accepted");
    builder
        .append_json_row(r#"{"id": 2, "value": "uncorrelated"}"#, None, None, None)
        .expect("a described row without Card correlation is accepted");
    wyrd_testing::bifrost::write::RawIngest::connect(&client)
        .await
        .expect("connect the public ingest wire")
        .insert(
            &fqn,
            uuid::Uuid::now_v7(),
            builder.finish_ipc().expect("seal the described batch"),
        )
        .await
        .expect("the described batch is accepted by the real wire");
    server.flush_bifrost().await.expect("publish the append");

    let mut stream = query
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: format!("SELECT id, card_uid, principal_id FROM {fqn} ORDER BY id"),
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
        "both described rows land; only the correlated one carries a card_uid"
    );
    server.shutdown().await.expect("server shutdown");
}
