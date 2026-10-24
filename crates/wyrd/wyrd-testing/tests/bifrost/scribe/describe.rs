//! Describe reaches the exact stored physical schema for both table kinds.

use arrow::array::Array;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::Bifrost;
use wyrd_client::bifrost::BifrostOptions;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::ids::TableVersion;

use super::support::{start_scribe_server, unique_table};

/// A writer never keeps its own copy of the physical contract.
///
/// For a canonical signal table the description's projected Arrow schema is
/// the registry's physical schema — every name, type shape, and nullability —
/// including the `card_uid` correlation input a writer supplies, and the
/// exact canonical physical fingerprint. Types compare exactly, because the
/// description projects the registry's own physical schema. For a dynamic
/// table, batches prepared straight from the description are accepted by the
/// real ingest wire and land with optional Card correlation: the correlated
/// row keeps its `card_uid`, the uncorrelated row stores none and still
/// carries its authenticated principal.
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
    Bifrost::connect_with(
        &client,
        BifrostOptions {
            table: Some(
                wyrd_client::bifrost::TableConfig::from_arrow(&fqn, schema)
                    .expect("declared table"),
            ),
            ..BifrostOptions::default()
        },
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
    let described_schema = wyrd_types::writable_schema(&canonical)
        .expect("the canonical description projects an Arrow schema");
    for stored in (definition.schema)().fields() {
        let name = stored.name().as_str();
        // Server-resolved columns are not a writer's to supply; `run_id` is the
        // one managed column a writer may still send.
        if vala_bifrost_redux::tables::managed_columns::is_managed_column(name) && name != "run_id"
        {
            continue;
        }
        let described = described_schema
            .field_with_name(name)
            .unwrap_or_else(|_| panic!("described schema keeps `{name}`"));
        assert!(
            stored.data_type().equals_datatype(described.data_type()),
            "`{name}` keeps its stored type"
        );
        assert_eq!(
            described.is_nullable(),
            stored.is_nullable(),
            "`{name}` keeps its stored nullability"
        );
        assert!(
            described
                .metadata()
                .keys()
                .all(|key| wyrd_types::is_extension_key(key)),
            "`{name}` sends no field identity beyond its extension type; ingress compares by shape"
        );
    }
    let card_uid = canonical
        .correlation_fields
        .iter()
        .find(|field| field.name == "card_uid")
        .expect("the Card correlation input is described");
    assert!(
        card_uid.nullable && card_uid.data_type == wyrd_spec::vala::api::DataTypeSpec::Utf8,
        "card_uid is the optional text column the writer supplies"
    );
    for name in ["card_uid", "run_id"] {
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
        wyrd_types::writable_schema(&dynamic)
            .expect("the dynamic description projects an Arrow schema")
            .field_with_name("card_uid")
            .expect("the Card correlation input is described")
            .is_nullable(),
        "Card correlation is optional, so describe must not demand it"
    );
    let preflight = wyrd_queue::RowPreflight::from_description(&dynamic)
        .expect("the dynamic description builds a row preflight");
    let correlated = preflight
        .prepare(
            &[r#"{"id": 1, "value": "described"}"#],
            writer_card.uid.as_ref(),
            None,
        )
        .expect("a described row is accepted");
    let uncorrelated = preflight
        .prepare(&[r#"{"id": 2, "value": "uncorrelated"}"#], None, None)
        .expect("a described row without Card correlation is accepted");
    let ingest = wyrd_testing::bifrost::write::RawIngest::connect(&client)
        .await
        .expect("connect the public ingest wire");
    for prepared in [correlated, uncorrelated] {
        ingest
            .insert(
                &fqn,
                uuid::Uuid::now_v7(),
                prepared.to_ipc().expect("seal the described batch"),
            )
            .await
            .expect("the described batch is accepted by the real wire");
    }
    server.flush_bifrost().await.expect("publish the append");
    let durable_rows: u64 = server
        .published_hot_files_for_test(
            server.data_tenant_id(),
            BifrostNamespace::Datasets.as_str(),
            &table_name,
        )
        .await
        .expect("published hot files are inspectable")
        .iter()
        .map(|file| file.row_count)
        .sum();
    assert_eq!(
        durable_rows, 2,
        "the description-built batch reached the durable read boundary"
    );

    let mut stream = query
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: format!("SELECT id, card_uid, sub_principal_id FROM {fqn} ORDER BY id"),
            deadline_ms: None,
        })
        .await
        .expect("Oracle reads the stored correlation columns");
    let mut correlation = Vec::new();
    while let Some(batch) = stream.next_batch().await.expect("valid terminal stream") {
        let card_uid = batch.column_by_name("card_uid").expect("card_uid column");
        let principal_id = batch
            .column_by_name("sub_principal_id")
            .expect("sub_principal_id column");
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

/// A composite layout registers at table version 1, describes canonically,
/// and re-registers idempotently without a version change; a changed
/// declaration and every out-of-bounds declaration are refused, and a refusal
/// leaves no registration behind.
///
/// The accepted table partitions hourly on a zoned `event_time` plus an
/// identity dimension over a nullable string and a bucket dimension over a
/// nullable integer, sorts on the string, and Blooms a third column; describe
/// returns exactly that declaration, with the complete Bloom set and no hidden
/// floor. A table registered without a layout resolves to
/// `hour(wyrd_ingested_at)`, that column descending, and no Bloom columns; a
/// declared time column alone drives the same defaults over itself. Each
/// refused declaration
/// — including a timezone-naive or nullable time axis — returns the
/// invalid-layout code naming its field and violation, and the
/// same name then registers as `Created` under a valid layout.
///
/// # Panics
///
/// Panics when an accepted layout is refused or describes differently, a
/// refusal carries another code, field, or violation, or a refused name is
/// already registered.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn registered_layouts_are_bounded_and_resolved() {
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::vala::api::{
        NullOrderWire, PartitionFieldWire, PartitionTransformWire, PhysicalLayoutWire,
        RegisterOutcome, SortDirectionWire, SortKeyWire,
    };

    let server = start_scribe_server().await;
    let bootstrap = server
        .bootstrap_service(&unique_table("layout_writer"), &["admin"])
        .await
        .expect("bootstrap the registering writer");
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
    let schema = std::sync::Arc::new(Schema::new(vec![
        Field::new(
            "event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            "local_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new(
            "maybe_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            true,
        ),
        Field::new("customer", DataType::Utf8, true),
        Field::new("shard", DataType::Int64, true),
        Field::new("score", DataType::Float64, true),
        Field::new("name", DataType::Utf8, false),
    ]));
    let register = |table: String, layout: Option<PhysicalLayoutWire>| {
        let client = &client;
        let schema = std::sync::Arc::clone(&schema);
        async move {
            let mut config = wyrd_client::bifrost::TableConfig::from_arrow(
                &format!("vala.datasets.{table}"),
                schema,
            )
            .expect("declared table");
            if let Some(layout) = layout {
                config = config.with_layout(layout);
            }
            Bifrost::connect_with(
                client,
                BifrostOptions {
                    table: Some(config),
                    ..BifrostOptions::default()
                },
            )
            .await
            .expect("the writer connects")
            .register()
            .await
        }
    };
    let query = Bifrost::query_only(&client);
    let part = |column: &str, transform, buckets, width| PartitionFieldWire {
        buckets,
        width,
        ..PartitionFieldWire::new(column, transform)
    };
    let composite = PhysicalLayoutWire {
        time_column: Some("event_time".to_owned()),
        partition_by: vec![
            part("event_time", PartitionTransformWire::Hour, None, None),
            part("customer", PartitionTransformWire::Identity, None, None),
            part("shard", PartitionTransformWire::Bucket, Some(16), None),
        ],
        sort_keys: vec![SortKeyWire {
            column: "customer".to_owned(),
            direction: SortDirectionWire::Asc,
            null_order: NullOrderWire::First,
        }],
        bloom_columns: vec!["name".to_owned()],
    };

    let table = unique_table("composite");
    assert_eq!(
        register(table.clone(), Some(composite.clone()))
            .await
            .expect("a composite layout registers"),
        RegisterOutcome::Created
    );
    let described = query
        .describe_table("vala.datasets", &table)
        .await
        .expect("describe the composite table");
    assert_eq!(
        described.physical_layout, composite,
        "describe returns exactly the declared composite layout"
    );
    assert_eq!(described.table_version, TableVersion::INITIAL);
    assert_eq!(
        register(table.clone(), Some(composite.clone()))
            .await
            .expect("the same declaration re-registers"),
        RegisterOutcome::AlreadyExists
    );
    assert_eq!(
        query
            .describe_table("vala.datasets", &table)
            .await
            .expect("describe after re-registration")
            .table_version,
        TableVersion::INITIAL,
        "an equivalent re-registration never advances the version"
    );
    let mut changed = composite.clone();
    changed.partition_by[2] = part("shard", PartitionTransformWire::Truncate, None, Some(10));
    let mismatch = register(table.clone(), Some(changed))
        .await
        .expect_err("a changed declaration never alters the table");
    assert_eq!(
        wyrd_spec::error::WyrdError::from(&mismatch).code(),
        "WYRD_VALA_409_BIFROST_LAYOUT_MISMATCH"
    );
    assert_eq!(
        query
            .describe_table("vala.datasets", &table)
            .await
            .expect("describe after the refused change")
            .physical_layout,
        composite,
        "the refused change left the registered layout unchanged"
    );

    let defaults = unique_table("default_layout");
    register(defaults.clone(), None)
        .await
        .expect("an omitted layout registers");
    let default_layout = query
        .describe_table("vala.datasets", &defaults)
        .await
        .expect("describe the default table")
        .physical_layout;
    assert_eq!(
        default_layout.partition_by,
        vec![PartitionFieldWire::new(
            "wyrd_ingested_at",
            PartitionTransformWire::Hour
        )]
    );
    assert!(
        default_layout.bloom_columns.is_empty(),
        "the default layout carries no hidden Bloom floor"
    );
    assert_eq!(default_layout.time_column.as_deref(), Some("wyrd_ingested_at"));
    assert_eq!(
        default_layout.sort_keys,
        vec![SortKeyWire {
            column: "wyrd_ingested_at".to_owned(),
            direction: SortDirectionWire::Desc,
            null_order: NullOrderWire::Last,
        }],
        "the default sorts newest ingest first"
    );

    let timed = unique_table("timed_default");
    register(
        timed.clone(),
        Some(PhysicalLayoutWire {
            time_column: Some("event_time".to_owned()),
            partition_by: Vec::new(),
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        }),
    )
    .await
    .expect("a declared time column alone registers");
    let timed_layout = query
        .describe_table("vala.datasets", &timed)
        .await
        .expect("describe the timed table")
        .physical_layout;
    assert_eq!(
        timed_layout.partition_by,
        vec![PartitionFieldWire::new("event_time", PartitionTransformWire::Hour)],
        "a declared time column drives the default partition"
    );
    assert_eq!(
        timed_layout.sort_keys,
        vec![SortKeyWire {
            column: "event_time".to_owned(),
            direction: SortDirectionWire::Desc,
            null_order: NullOrderWire::Last,
        }],
        "a declared time column drives the default sort"
    );

    let with = |edit: &dyn Fn(&mut PhysicalLayoutWire)| {
        let mut layout = composite.clone();
        edit(&mut layout);
        layout
    };
    let refusals = [
        (
            with(&|layout| layout.time_column = Some("local_time".to_owned())),
            "TimeColumn/UnsupportedColumn",
        ),
        (
            with(&|layout| {
                layout.partition_by.push(part(
                    "name",
                    PartitionTransformWire::Identity,
                    None,
                    None,
                ));
            }),
            "Partition/TooManyKeys",
        ),
        (
            with(&|layout| layout.partition_by[2].buckets = Some(1025)),
            "PartitionTransform/OutOfRange",
        ),
        (
            with(&|layout| {
                layout.partition_by[1] =
                    part("score", PartitionTransformWire::Identity, None, None);
            }),
            "Partition/UnsupportedColumn",
        ),
        (
            with(&|layout| layout.time_column = Some("maybe_time".to_owned())),
            "TimeColumn/UnsupportedColumn",
        ),
        (
            with(&|layout| layout.bloom_columns = vec!["event_time".to_owned()]),
            "BloomColumn/UnsupportedColumn",
        ),
        (
            with(&|layout| layout.bloom_columns = vec!["absent".to_owned()]),
            "BloomColumn/UnknownColumn",
        ),
        (
            with(&|layout| {
                let key = layout.sort_keys[0].clone();
                layout.sort_keys.push(key);
            }),
            "SortKey/Duplicate",
        ),
        (
            with(&|layout| {
                layout.partition_by[2] =
                    part("customer", PartitionTransformWire::Identity, None, None);
            }),
            "Partition/Duplicate",
        ),
        (
            with(&|layout| layout.partition_by[2].buckets = Some(1)),
            "PartitionTransform/OutOfRange",
        ),
    ];
    for (layout, expected) in refusals {
        let refused_table = unique_table("refused_layout");
        let refused = register(refused_table.clone(), Some(layout))
            .await
            .expect_err("an out-of-bounds declaration is refused");
        assert_eq!(
            wyrd_spec::error::WyrdError::from(&refused).code(),
            "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT"
        );
        assert!(
            refused.to_string().contains(expected),
            "the refusal names {expected}: {refused}"
        );
        assert_eq!(
            register(refused_table, Some(composite.clone()))
                .await
                .expect("the refused name registers under a valid layout"),
            RegisterOutcome::Created,
            "a refusal leaves no registration behind"
        );
    }
    server.shutdown().await.expect("server shutdown");
}
