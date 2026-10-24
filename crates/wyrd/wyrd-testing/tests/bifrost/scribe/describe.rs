//! Describe reaches the exact stored physical schema for both table kinds.

use arrow::array::Array;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::Bifrost;
use wyrd_client::bifrost::BifrostOptions;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::ids::TableVersion;

use super::support::{start_scribe_server, tenant_client, unique_table};

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

/// The stable catalog code a refused client call carries.
fn code(error: &wyrd_client::bifrost::BifrostClientError) -> String {
    wyrd_spec::error::WyrdError::from(error).code().to_owned()
}

/// An edit at `version` that replaces only the listed layout lists.
fn layout_edit(
    version: TableVersion,
    layout: wyrd_spec::vala::api::PhysicalLayoutEdit,
) -> wyrd_spec::vala::api::AlterTableRequest {
    wyrd_spec::vala::api::AlterTableRequest {
        expected_table_version: version,
        physical_layout: Some(layout),
        add_columns: None,
        rename_columns: None,
    }
}

/// An edit at `version` that replaces only the Bloom list.
fn bloom_edit(version: TableVersion, columns: &[&str]) -> wyrd_spec::vala::api::AlterTableRequest {
    layout_edit(
        version,
        wyrd_spec::vala::api::PhysicalLayoutEdit {
            bloom_columns: Some(columns.iter().map(|&column| column.to_owned()).collect()),
            ..wyrd_spec::vala::api::PhysicalLayoutEdit::default()
        },
    )
}

/// An administrator's explicit layout edit replaces exactly the supplied
/// lists at the next table version, and the version compares every edit.
///
/// For a built-in and a dynamic table alike, a Bloom-only edit at the
/// described version commits once and returns the description describe then
/// reads, and the same edit at the superseded version is refused as a version
/// conflict. On the dynamic table a combined partition-and-sort edit keeps the
/// omitted Bloom list; an explicit empty Bloom list clears it; two edits built
/// from one version race to exactly one commit, so neither is lost silently;
/// and an invalid post-edit layout, an edit that changes nothing, and an edit
/// that supplies nothing are refused. Every refusal leaves the description,
/// version included, unchanged.
///
/// # Panics
///
/// Panics when an edit commits other than once, a refusal carries another
/// code, or a refused edit changes the described table.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn table_patch_preserves_omitted_lists_and_compares_versions() {
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::vala::api::{
        NullOrderWire, PartitionFieldWire, PartitionTransformWire, PhysicalLayoutEdit,
        SortDirectionWire, SortKeyWire,
    };

    let server = start_scribe_server().await;
    let client = tenant_client(&server, server.data_tenant_id()).await;
    let bifrost = Bifrost::query_only(&client);
    let table = unique_table("patched");
    Bifrost::connect_with(
        &client,
        BifrostOptions {
            table: Some(
                wyrd_client::bifrost::TableConfig::from_arrow(
                    &format!("vala.datasets.{table}"),
                    std::sync::Arc::new(Schema::new(vec![
                        Field::new("customer", DataType::Utf8, true),
                        Field::new("name", DataType::Utf8, false),
                    ])),
                )
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
    let describe = |namespace: &'static str, name: String| {
        let bifrost = &bifrost;
        async move {
            bifrost
                .describe_table(namespace, &name)
                .await
                .expect("describe the table")
        }
    };

    for (namespace, name, candidate) in [
        ("vala.traces", "spans".to_owned(), "span_id"),
        ("vala.datasets", table.clone(), "name"),
    ] {
        let before = describe(namespace, name.clone()).await;
        let blooms: &[&str] = if before.physical_layout.bloom_columns.is_empty() {
            &[candidate]
        } else {
            &[]
        };
        let committed = bifrost
            .alter_table(namespace, &name, &bloom_edit(before.table_version, blooms))
            .await
            .unwrap_or_else(|error| panic!("{namespace}.{name} accepts a Bloom edit: {error}"));
        assert_eq!(
            Some(committed.table_version),
            before.table_version.next(),
            "{namespace}.{name}: a Bloom-only edit advances the version once"
        );
        assert_eq!(committed.physical_layout.bloom_columns, blooms);
        assert_eq!(
            committed.physical_layout.partition_by, before.physical_layout.partition_by,
            "{namespace}.{name}: an omitted list is kept"
        );
        assert_eq!(committed.physical_layout.sort_keys, before.physical_layout.sort_keys);
        assert_eq!(
            describe(namespace, name.clone()).await,
            committed,
            "{namespace}.{name}: the edit returns the description describe reads"
        );
        let stale = bifrost
            .alter_table(namespace, &name, &bloom_edit(before.table_version, &[]))
            .await
            .expect_err("an edit at a superseded version is refused");
        assert_eq!(code(&stale), "WYRD_VALA_409_BIFROST_TABLE_VERSION_CONFLICT");
        assert_eq!(describe(namespace, name).await, committed);
    }

    let current = describe("vala.datasets", table.clone()).await;
    let combined = bifrost
        .alter_table(
            "vala.datasets",
            &table,
            &layout_edit(
                current.table_version,
                PhysicalLayoutEdit {
                    partition_by: Some(vec![
                        PartitionFieldWire::new("wyrd_ingested_at", PartitionTransformWire::Day),
                        PartitionFieldWire::new("customer", PartitionTransformWire::Identity),
                    ]),
                    sort_keys: Some(vec![SortKeyWire {
                        column: "customer".to_owned(),
                        direction: SortDirectionWire::Asc,
                        null_order: NullOrderWire::First,
                    }]),
                    bloom_columns: None,
                },
            ),
        )
        .await
        .expect("a combined partition and sort edit commits");
    assert_eq!(combined.table_version, current.table_version.next().expect("next"));
    assert_eq!(
        combined.physical_layout.partition_by,
        vec![
            PartitionFieldWire::new("wyrd_ingested_at", PartitionTransformWire::Day),
            PartitionFieldWire::new("customer", PartitionTransformWire::Identity),
        ]
    );
    assert_eq!(combined.physical_layout.sort_keys[0].column, "customer");
    assert_eq!(
        combined.physical_layout.bloom_columns, current.physical_layout.bloom_columns,
        "the omitted Bloom list is kept"
    );
    assert_eq!(combined.physical_layout.bloom_columns, ["name"]);
    let cleared = bifrost
        .alter_table("vala.datasets", &table, &bloom_edit(combined.table_version, &[]))
        .await
        .expect("an explicit empty Bloom list commits");
    assert!(cleared.physical_layout.bloom_columns.is_empty());
    assert_eq!(cleared.physical_layout.partition_by, combined.physical_layout.partition_by);

    let (name_edit, customer_edit) = (
        bloom_edit(cleared.table_version, &["name"]),
        bloom_edit(cleared.table_version, &["customer"]),
    );
    let (first, second) = tokio::join!(
        bifrost.alter_table("vala.datasets", &table, &name_edit),
        bifrost.alter_table("vala.datasets", &table, &customer_edit),
    );
    let (winner, loser) = match (first, second) {
        (Ok(winner), Err(loser)) | (Err(loser), Ok(winner)) => (winner, loser),
        other => panic!("exactly one edit from one version commits: {other:?}"),
    };
    assert_eq!(code(&loser), "WYRD_VALA_409_BIFROST_TABLE_VERSION_CONFLICT");
    assert_eq!(winner.table_version, cleared.table_version.next().expect("next"));
    let settled = describe("vala.datasets", table.clone()).await;
    assert_eq!(settled, winner, "the losing edit changed nothing");

    let refusals = [
        (
            bloom_edit(settled.table_version, &["absent"]),
            "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT",
            "BloomColumn/UnknownColumn",
        ),
        (
            layout_edit(
                settled.table_version,
                PhysicalLayoutEdit {
                    partition_by: Some(Vec::new()),
                    ..PhysicalLayoutEdit::default()
                },
            ),
            "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT",
            "Partition/Missing",
        ),
        (
            bloom_edit(
                settled.table_version,
                &settled
                    .physical_layout
                    .bloom_columns
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
            "WYRD_VALA_400_BIFROST_INVALID_TABLE_EDIT",
            "NoEffectiveChange",
        ),
        (
            layout_edit(settled.table_version, PhysicalLayoutEdit::default()),
            "WYRD_VALA_400_BIFROST_INVALID_TABLE_EDIT",
            "NoEffectiveChange",
        ),
    ];
    for (edit, expected_code, expected) in refusals {
        let refused = bifrost
            .alter_table("vala.datasets", &table, &edit)
            .await
            .expect_err("the edit is refused");
        assert_eq!(code(&refused), expected_code, "{refused}");
        assert!(refused.to_string().contains(expected), "names {expected}: {refused}");
        assert_eq!(
            describe("vala.datasets", table.clone()).await,
            settled,
            "a refused edit changes nothing"
        );
    }
    server.shutdown().await.expect("server shutdown");
}
