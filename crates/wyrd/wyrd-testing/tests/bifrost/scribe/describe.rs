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
    assert_eq!(
        default_layout.time_column.as_deref(),
        Some("wyrd_ingested_at")
    );
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
        vec![PartitionFieldWire::new(
            "event_time",
            PartitionTransformWire::Hour
        )],
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
        assert_eq!(
            committed.physical_layout.sort_keys,
            before.physical_layout.sort_keys
        );
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
    assert_eq!(
        combined.table_version,
        current.table_version.next().expect("next")
    );
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
        .alter_table(
            "vala.datasets",
            &table,
            &bloom_edit(combined.table_version, &[]),
        )
        .await
        .expect("an explicit empty Bloom list commits");
    assert!(cleared.physical_layout.bloom_columns.is_empty());
    assert_eq!(
        cleared.physical_layout.partition_by,
        combined.physical_layout.partition_by
    );

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
    assert_eq!(
        winner.table_version,
        cleared.table_version.next().expect("next")
    );
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
        assert!(
            refused.to_string().contains(expected),
            "names {expected}: {refused}"
        );
        assert_eq!(
            describe("vala.datasets", table.clone()).await,
            settled,
            "a refused edit changes nothing"
        );
    }
    server.shutdown().await.expect("server shutdown");
}

/// Each described user field's name and stable field id, in order.
///
/// # Panics
///
/// Panics when a described field carries no Parquet field id.
fn field_ids(description: &wyrd_spec::vala::api::BifrostTableDescription) -> Vec<(String, String)> {
    description
        .user_fields
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                field.metadata[wyrd_spec::vala::api::PARQUET_FIELD_ID_KEY].clone(),
            )
        })
        .collect()
}

/// An edit at `version` that only adds and renames columns.
fn schema_edit(
    version: TableVersion,
    add: Vec<wyrd_spec::vala::api::FieldSpec>,
    rename: &[(&str, &str)],
) -> wyrd_spec::vala::api::AlterTableRequest {
    wyrd_spec::vala::api::AlterTableRequest {
        expected_table_version: version,
        physical_layout: None,
        add_columns: (!add.is_empty()).then_some(add),
        rename_columns: (!rename.is_empty()).then(|| {
            rename
                .iter()
                .map(|&(from, to)| wyrd_spec::vala::api::ColumnRename {
                    from: from.to_owned(),
                    to: to.to_owned(),
                })
                .collect()
        }),
    }
}

/// One nullable or required string column declaration.
fn string_column(name: &str, nullable: bool) -> wyrd_spec::vala::api::FieldSpec {
    wyrd_spec::vala::api::FieldSpec {
        name: name.to_owned(),
        data_type: wyrd_spec::vala::api::DataTypeSpec::Utf8,
        nullable,
        metadata: std::collections::BTreeMap::new(),
    }
}

/// A nullable column addition and a rename of a layout-referenced column keep
/// every stored value and field id, and new writes use the committed schema.
///
/// Rows written before the edit read back under the renamed column with their
/// values and read the added column as null; the partition and Bloom entries
/// over the renamed column follow it; describe reports the new names under
/// the same field ids and the added column under a fresh one; and a batch in
/// the edited shape is accepted. Every edit outside the safe-edit contract —
/// unknown, protected, reserved, colliding, chained, no-op, or required — is
/// refused with its violation, as is a request mixing a valid addition with an
/// invalid rename and any schema edit of a built-in table, and each refusal
/// leaves the description and rows unchanged.
///
/// # Panics
///
/// Panics when an edit loses a value or field id, a refusal carries another
/// code or violation, or a refused edit changes the table.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn custom_schema_edits_preserve_field_ids_and_values() {
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::vala::api::{PartitionFieldWire, PartitionTransformWire, PhysicalLayoutWire};

    let server = start_scribe_server().await;
    let client = tenant_client(&server, server.data_tenant_id()).await;
    let bifrost = Bifrost::query_only(&client);
    let table = unique_table("evolved");
    let fqn = format!("vala.datasets.{table}");
    Bifrost::connect_with(
        &client,
        BifrostOptions {
            table: Some(
                wyrd_client::bifrost::TableConfig::from_arrow(
                    &fqn,
                    std::sync::Arc::new(Schema::new(vec![
                        Field::new("value", DataType::Int64, false),
                        Field::new("customer", DataType::Utf8, true),
                    ])),
                )
                .expect("declared table")
                .with_layout(PhysicalLayoutWire {
                    time_column: None,
                    partition_by: vec![
                        PartitionFieldWire::new("wyrd_ingested_at", PartitionTransformWire::Hour),
                        PartitionFieldWire::new("customer", PartitionTransformWire::Identity),
                    ],
                    sort_keys: Vec::new(),
                    bloom_columns: vec!["customer".to_owned()],
                }),
            ),
            ..BifrostOptions::default()
        },
    )
    .await
    .expect("the writer connects")
    .register()
    .await
    .expect("register the dynamic table");
    let write = |schema: Vec<Field>, columns: Vec<arrow::array::ArrayRef>| {
        let client = &client;
        let fqn = &fqn;
        async move {
            let batch = arrow::record_batch::RecordBatch::try_new(
                std::sync::Arc::new(Schema::new(schema)),
                columns,
            )
            .expect("batch");
            super::support::append_batch(client, fqn, uuid::Uuid::now_v7(), &batch).await
        }
    };
    write(
        vec![
            Field::new("value", DataType::Int64, false),
            Field::new("customer", DataType::Utf8, true),
        ],
        vec![
            std::sync::Arc::new(Int64Array::from(vec![1, 2])),
            std::sync::Arc::new(StringArray::from(vec![Some("a"), None])),
        ],
    )
    .await
    .expect("rows under the registered schema are accepted");
    let before = bifrost
        .describe_table("vala.datasets", &table)
        .await
        .expect("describe before the edit");

    let edited = bifrost
        .alter_table(
            "vala.datasets",
            &table,
            &schema_edit(
                before.table_version,
                vec![string_column("note", true)],
                &[("customer", "client")],
            ),
        )
        .await
        .expect("a nullable addition and a rename commit together");
    assert_eq!(
        edited.table_version,
        before.table_version.next().expect("next")
    );
    let before_ids = field_ids(&before);
    let edited_ids = field_ids(&edited);
    assert_eq!(edited_ids[0], before_ids[0], "value keeps its name and id");
    assert_eq!(
        edited_ids[1],
        ("client".to_owned(), before_ids[1].1.clone()),
        "the renamed column keeps its field id"
    );
    assert_eq!(edited_ids[2].0, "note");
    assert!(
        before_ids.iter().all(|(_, id)| *id != edited_ids[2].1),
        "the added column gets a fresh field id"
    );
    assert_eq!(
        edited.physical_layout.partition_by[1],
        PartitionFieldWire::new("client", PartitionTransformWire::Identity),
        "the partition entry follows the rename"
    );
    assert_eq!(edited.physical_layout.bloom_columns, ["client"]);

    write(
        vec![
            Field::new("value", DataType::Int64, false),
            Field::new("client", DataType::Utf8, true),
            Field::new("note", DataType::Utf8, true),
        ],
        vec![
            std::sync::Arc::new(Int64Array::from(vec![3])),
            std::sync::Arc::new(StringArray::from(vec![Some("c")])),
            std::sync::Arc::new(StringArray::from(vec![Some("n")])),
        ],
    )
    .await
    .expect("a row in the edited shape is accepted");
    let rows = || async {
        bifrost
            .sql_as::<serde_json::Value>(
                &format!("SELECT value, client, note FROM {fqn} ORDER BY value"),
                &[],
            )
            .await
            .expect("read the edited table")
    };
    // A row projects a null column by omitting it.
    let expected = vec![
        serde_json::json!({"value": 1, "client": "a"}),
        serde_json::json!({"value": 2}),
        serde_json::json!({"value": 3, "client": "c", "note": "n"}),
    ];
    assert_eq!(rows().await, expected, "stored values survive the edit");

    let version = edited.table_version;
    let invalid = "WYRD_VALA_400_BIFROST_INVALID_TABLE_EDIT";
    let refusals = [
        (
            schema_edit(version, Vec::new(), &[("absent", "x")]),
            invalid,
            "UnknownColumn",
        ),
        (
            schema_edit(version, Vec::new(), &[("client", "value")]),
            invalid,
            "NameCollision",
        ),
        (
            schema_edit(
                version,
                Vec::new(),
                &[("value", "client"), ("client", "c2")],
            ),
            invalid,
            "AmbiguousRename",
        ),
        (
            schema_edit(version, Vec::new(), &[("value", "value")]),
            invalid,
            "NoEffectiveChange",
        ),
        (
            schema_edit(version, Vec::new(), &[("wyrd_ingested_at", "t")]),
            invalid,
            "ProtectedColumn",
        ),
        (
            schema_edit(version, vec![string_column("value", true)], &[]),
            invalid,
            "NameCollision",
        ),
        (
            schema_edit(version, vec![string_column("tag", false)], &[]),
            invalid,
            "RequiredColumn",
        ),
        (
            schema_edit(version, vec![string_column("card_uid", true)], &[]),
            "WYRD_VALA_400_BIFROST_RESERVED_COLUMN",
            "card_uid",
        ),
        (
            schema_edit(
                version,
                vec![string_column("tag", true)],
                &[("absent", "x")],
            ),
            invalid,
            "UnknownColumn",
        ),
    ];
    for (edit, expected_code, expected) in refusals {
        let refused = bifrost
            .alter_table("vala.datasets", &table, &edit)
            .await
            .expect_err("the edit is refused");
        assert_eq!(code(&refused), expected_code, "{refused}");
        assert!(
            refused.to_string().contains(expected),
            "names {expected}: {refused}"
        );
        assert_eq!(
            bifrost
                .describe_table("vala.datasets", &table)
                .await
                .expect("describe after a refusal"),
            edited,
            "a refused edit changes nothing"
        );
    }
    assert_eq!(rows().await, expected, "refusals change no rows");

    let spans = bifrost
        .describe_table("vala.traces", "spans")
        .await
        .expect("describe the span table");
    let refused = bifrost
        .alter_table(
            "vala.traces",
            "spans",
            &schema_edit(spans.table_version, vec![string_column("tag", true)], &[]),
        )
        .await
        .expect_err("a built-in schema is canonical");
    assert_eq!(code(&refused), invalid);
    assert!(refused.to_string().contains("ProtectedColumn"), "{refused}");
    server.shutdown().await.expect("server shutdown");
}

/// A committed layout override reaches every pod, survives restarting every
/// pod, and leaves accepted batches, old data, and other tenants untouched.
///
/// Two pods share one catalog. Pod B admits a batch at table version 1 and
/// publishes it; a refused edit through pod A changes nothing either pod
/// serves. The committed edit through pod A is then described by pod B at
/// once, and pod B's admission follows it: new batches at the superseded
/// version are refused once its cache hears the commit, and a batch at the
/// committed version is accepted. A built-in table's Bloom override commits
/// the same way, and another tenant's same-named table keeps its own
/// definition. After every pod restarts, both overrides are still described,
/// the version-1 batch still retries as acknowledged without duplicating its
/// rows, a new version-1 batch is refused, and a filter over the new Bloom
/// column reads rows from files written before and after the edit.
///
/// # Panics
///
/// Panics when a pod serves another definition than the committed one, an
/// override is lost on restart, a stale batch is admitted after the refresh,
/// an accepted retry is refused or duplicated, or rows read back otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres and object storage"]
async fn tenant_layout_overrides_survive_restart_and_refresh() {
    use wyrd_spec::vala::api::{NullOrderWire, PhysicalLayoutEdit, SortDirectionWire, SortKeyWire};
    use wyrd_testing::bifrost::cluster::{BifrostClusterSpec, WyrdTestCluster};
    use wyrd_testing::bifrost::revisions::{composite_batch, register_composite_table, shifted};

    /// The cluster's pod at `index`.
    ///
    /// # Panics
    ///
    /// Panics when the cluster has no such pod.
    fn pod(cluster: &WyrdTestCluster, index: usize) -> &wyrd_testing::WyrdTestServer {
        cluster
            .server(index)
            .unwrap_or_else(|| panic!("the cluster owns pod {index}"))
    }
    /// Publishes every batch both pods hold.
    ///
    /// # Panics
    ///
    /// Panics when either pod cannot publish.
    async fn flush(cluster: &WyrdTestCluster) {
        for index in [0, 1] {
            pod(cluster, index)
                .flush_bifrost()
                .await
                .expect("the pod publishes");
        }
    }

    /// The stable code of a stale table version.
    const CONFLICT: &str = "WYRD_VALA_409_BIFROST_TABLE_VERSION_CONFLICT";
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("the two-pod cluster starts");
    let tenant = cluster.data_tenant_id();
    let neighbour = cluster
        .add_tenant("layout-override-neighbour")
        .await
        .expect("the neighbour tenant is seeded");
    let name = unique_table("override");
    let table = register_composite_table(pod(&cluster, 0), tenant, &name).await;
    register_composite_table(pod(&cluster, 0), neighbour, &name).await;
    let key = |tenant| {
        let cluster = &cluster;
        async move {
            pod(cluster, 0)
                .bootstrap_service_in_tenant(tenant, &unique_table("override_admin"), &["admin"])
                .await
                .expect("tenant service bootstrap")
                .api_key()
                .expect("service API key")
                .clone()
        }
    };
    let (api_key, neighbour_key) = (key(tenant).await, key(neighbour).await);
    let clients = |cluster: &WyrdTestCluster| {
        [0, 1].map(|index| super::support::endpoint_client(pod(cluster, index), &api_key))
    };

    let [a, b] = clients(&cluster);
    let base = composite_batch();
    let accepted = uuid::Uuid::now_v7();
    super::support::append_batch_at(&b, &table, TableVersion::INITIAL, accepted, &base)
        .await
        .expect("pod B admits a batch at version 1");
    flush(&cluster).await;
    let describe = |client: &wyrd_client::WyrdClient, name: &str| {
        let bifrost = Bifrost::query_only(client);
        let name = name.to_owned();
        async move {
            bifrost
                .describe_table("vala.datasets", &name)
                .await
                .expect("describe the table")
        }
    };
    let original = describe(&b, &name).await;

    let refused = Bifrost::query_only(&a)
        .alter_table(
            "vala.datasets",
            &name,
            &bloom_edit(original.table_version, &["absent"]),
        )
        .await
        .expect_err("an invalid edit is refused");
    assert_eq!(
        code(&refused),
        "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT"
    );
    assert_eq!(describe(&a, &name).await, original);
    assert_eq!(describe(&b, &name).await, original);

    let committed = Bifrost::query_only(&a)
        .alter_table(
            "vala.datasets",
            &name,
            &layout_edit(
                original.table_version,
                PhysicalLayoutEdit {
                    sort_keys: Some(vec![SortKeyWire {
                        column: "value".to_owned(),
                        direction: SortDirectionWire::Desc,
                        null_order: NullOrderWire::Last,
                    }]),
                    bloom_columns: Some(vec!["customer".to_owned()]),
                    partition_by: None,
                },
            ),
        )
        .await
        .expect("pod A commits the override");
    assert_eq!(
        describe(&b, &name).await,
        committed,
        "pod B describes the commit"
    );

    // Pod B's admission cache follows the commit when its listener hears it;
    // until then a version-1 batch is still a valid version-1 batch.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut probes = Vec::new();
    loop {
        let probe = shifted(
            &base,
            100 * (i64::try_from(probes.len()).expect("small") + 1),
        );
        match super::support::append_batch_at(
            &b,
            &table,
            original.table_version,
            uuid::Uuid::now_v7(),
            &probe,
        )
        .await
        {
            Ok(()) => probes.push(probe),
            Err(error) if error.code() == CONFLICT => break,
            Err(error) => panic!("a stale batch is refused only as a conflict: {error}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pod B never heard the committed edit"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let after = shifted(&base, 10_000);
    super::support::append_batch(&b, &table, uuid::Uuid::now_v7(), &after)
        .await
        .expect("pod B admits a batch at the committed version");

    let spans = Bifrost::query_only(&a)
        .describe_table("vala.traces", "spans")
        .await
        .expect("describe spans");
    let blooms: &[&str] = if spans.physical_layout.bloom_columns.is_empty() {
        &["span_id"]
    } else {
        &[]
    };
    let spans = Bifrost::query_only(&a)
        .alter_table(
            "vala.traces",
            "spans",
            &bloom_edit(spans.table_version, blooms),
        )
        .await
        .expect("a built-in layout override commits");
    let neighbour_original = describe(
        &super::support::endpoint_client(pod(&cluster, 0), &neighbour_key),
        &name,
    )
    .await;
    assert_eq!(neighbour_original.table_version, TableVersion::INITIAL);
    assert!(neighbour_original.physical_layout.bloom_columns.is_empty());
    flush(&cluster).await;

    for node in cluster.configured_node_ids() {
        cluster.stop_node(node).await.expect("the pod stops");
    }
    for node in cluster.configured_node_ids() {
        cluster.restart_node(node).await.expect("the pod restarts");
    }
    let [a, b] = clients(&cluster);
    for client in [&a, &b] {
        assert_eq!(
            describe(client, &name).await,
            committed,
            "restart keeps the override"
        );
        assert_eq!(
            Bifrost::query_only(client)
                .describe_table("vala.traces", "spans")
                .await
                .expect("describe spans after restart"),
            spans,
            "startup validates the built-in schema without reverting its layout"
        );
    }
    assert_eq!(
        describe(
            &super::support::endpoint_client(pod(&cluster, 0), &neighbour_key),
            &name
        )
        .await,
        neighbour_original,
        "the neighbour's same-named table keeps its definition"
    );
    super::support::append_batch_at(&b, &table, original.table_version, accepted, &base)
        .await
        .expect("the accepted batch retries at its original version");
    let stale = super::support::append_batch_at(
        &a,
        &table,
        original.table_version,
        uuid::Uuid::now_v7(),
        &shifted(&base, 50_000),
    )
    .await
    .expect_err("a new batch at the superseded version is refused");
    assert_eq!(stale.code(), CONFLICT, "{stale}");

    let values = |batch: &arrow::record_batch::RecordBatch, customer: Option<&str>| {
        let customers = batch
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .expect("customer is a string");
        let values = batch
            .column(3)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("value is Int64");
        (0..batch.num_rows())
            .filter(|&row| {
                customer.is_none()
                    || customers.is_valid(row) && Some(customers.value(row)) == customer
            })
            .map(|row| values.value(row))
            .collect::<Vec<_>>()
    };
    let written: Vec<_> = std::iter::once(&base)
        .chain(&probes)
        .chain([&after])
        .collect();
    for (filter, customer) in [("", None), (" WHERE customer = 'a'", Some("a"))] {
        let mut expected: Vec<i64> = written
            .iter()
            .flat_map(|batch| values(batch, customer))
            .collect();
        expected.sort_unstable();
        let mut read =
            super::support::read_sql(&a, &format!("SELECT value FROM {table}{filter}")).await;
        read.sort_unstable();
        assert_eq!(read, expected, "`{filter}` reads every file exactly once");
    }
    cluster.shutdown().await.expect("cluster shutdown");
}

/// An edit returns the definition it committed even when the next edit is
/// already waiting.
///
/// Edit A is paused right after its native publication is staged. Edit B, built
/// from A's new version, cannot publish while A is paused, because A still
/// holds the table's registration lock and projects its response under it.
/// Released, A returns its own version and Bloom list, B then returns the
/// next version, and describe reads B's.
///
/// # Panics
///
/// Panics when A returns another version or layout, B commits while A is
/// paused, or describe reads anything but B's definition.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn an_edit_returns_its_own_committed_definition() {
    use std::sync::Arc;
    use tokio::sync::Notify;
    use vala_bifrost_redux::catalog::pause_next_alter_after_publication_for_test;

    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("own_result");
    super::support::register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;
    let bifrost = Bifrost::query_only(&client);
    let before = bifrost
        .describe_table("vala.datasets", &name)
        .await
        .expect("describe the registered table");
    let next = before.table_version.next().expect("next version");

    let (published, release) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    pause_next_alter_after_publication_for_test(Arc::clone(&published), Arc::clone(&release));
    let first_edit = bloom_edit(before.table_version, &["value"]);
    let second_edit = bloom_edit(next, &[]);
    let first = bifrost.alter_table("vala.datasets", &name, &first_edit);
    let second = async {
        published.notified().await;
        let mut second = std::pin::pin!(bifrost.alter_table("vala.datasets", &name, &second_edit));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(500), &mut second)
                .await
                .is_err(),
            "the next edit waits while the first has not projected its result"
        );
        release.notify_one();
        second.await
    };
    let (first, second) = tokio::join!(first, second);
    let first = first.expect("the first edit commits");
    let second = second.expect("the next edit commits after it");

    assert_eq!(first.table_version, next);
    assert_eq!(first.physical_layout.bloom_columns, ["value"]);
    assert_eq!(second.table_version, next.next().expect("next version"));
    assert!(second.physical_layout.bloom_columns.is_empty());
    assert_eq!(
        bifrost
            .describe_table("vala.datasets", &name)
            .await
            .expect("describe the table"),
        second,
        "describe reads the current head"
    );
}

/// The real `PATCH` route decodes strictly, authorizes DDL, compares the
/// version, and refuses every negative class with one stable code.
///
/// Raw HTTP pins the wire: a canonical decimal-string version and a supported
/// effective edit commit and return the description with its version as a
/// decimal string. Unknown, explicit-null, nested-null, nested unknown members
/// and nulls inside otherwise effective edits, numeric, zero,
/// overflowing, and noncanonical versions, a malformed body, a time-axis
/// edit, and an unsupported operation are 400s; a superseded version is a
/// 409; a caller holding only record and query grants is a 403; and another
/// tenant's caller gets a 404 that reveals nothing. Every refusal leaves the
/// description unchanged, and each decoded request records one audited
/// permission decision.
///
/// # Panics
///
/// Panics when a request answers with another status or code, a refusal
/// changes the table, or the audit history misses a decision.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn table_patch_http_contract_refuses_invalid_requests() {
    use wyrd_runtime::Permission;

    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("http_patch");
    super::support::register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    server
        .seed_role(
            "patch_writer_reader",
            &[
                Permission::bifrost_record_write(),
                Permission::bifrost_query_read(),
                Permission::bifrost_table_read(),
            ],
        )
        .await
        .expect("the record and query role is seeded");
    let neighbour = server
        .seed_tenant("http-patch-neighbour")
        .await
        .expect("the neighbour tenant is seeded");
    let token = |tenant, roles: &'static [&'static str]| {
        let server = &server;
        async move {
            let bootstrap = server
                .bootstrap_service_in_tenant(tenant, &unique_table("http_patch"), roles)
                .await
                .expect("service bootstrap");
            server
                .exchange_api_key(bootstrap.api_key().expect("service API key"))
                .await
                .expect("the key exchanges")
        }
    };
    let admin = token(tenant, &["admin"]).await;
    let writer = token(tenant, &["patch_writer_reader"]).await;
    let foreign = token(neighbour, &["admin"]).await;
    let url = format!(
        "{}/v1/bifrost/tables/vala.datasets/{name}",
        server.base_url().expect("HTTP URL")
    );
    let http = reqwest::Client::new();
    let send = |token: &str, body: String| {
        http.patch(&url)
            .header("x-wyrd-access-token", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(body)
            .send()
    };
    let describe = || async {
        let response = http
            .get(&url)
            .header("x-wyrd-access-token", format!("Bearer {admin}"))
            .send()
            .await
            .expect("describe responds");
        assert_eq!(response.status(), 200);
        response
            .json::<serde_json::Value>()
            .await
            .expect("describe is JSON")
    };
    let original = describe().await;
    assert_eq!(
        original["table_version"], "1",
        "the version travels as a decimal string"
    );

    let committed = send(
        &admin,
        r#"{"expected_table_version":"1","physical_layout":{"bloom_columns":["value"]}}"#
            .to_owned(),
    )
    .await
    .expect("the edit responds");
    assert_eq!(committed.status(), 200);
    let committed: serde_json::Value = committed.json().await.expect("the edit returns JSON");
    assert_eq!(committed["table_version"], "2");
    assert_eq!(
        committed["physical_layout"]["bloom_columns"],
        serde_json::json!(["value"])
    );
    assert_eq!(
        describe().await,
        committed,
        "the response is the committed description"
    );

    let validation = "WYRD_SPEC_400_VALIDATION";
    let cases = [
        (
            &admin,
            r#"{"expected_table_version":"2","drop_columns":["value"]}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":null}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"2","physical_layout":null}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"2","physical_layout":{"bloom_columns":null}}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":2,"physical_layout":{"bloom_columns":[]}}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"0","physical_layout":{"bloom_columns":[]}}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"9223372036854775808","physical_layout":{"bloom_columns":[]}}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"02","physical_layout":{"bloom_columns":[]}}"#,
            400,
            validation,
        ),
        (&admin, r#"{"expected_table_version":"2""#, 400, validation),
        (
            &admin,
            r#"{"expected_table_version":"2","physical_layout":{"time_column":"value"}}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"2","add_columns":[{"name":"note","data_type":"Utf8","nullabel":true}]}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"2","physical_layout":{"partition_by":[{"column":"wyrd_ingested_at","transform":"day","buckets":null}]}}"#,
            400,
            validation,
        ),
        (
            &admin,
            r#"{"expected_table_version":"1","physical_layout":{"bloom_columns":[]}}"#,
            409,
            "WYRD_VALA_409_BIFROST_TABLE_VERSION_CONFLICT",
        ),
        (
            &writer,
            r#"{"expected_table_version":"2","physical_layout":{"bloom_columns":[]}}"#,
            403,
            "WYRD_PERMISSION_403_DENIED_RBAC",
        ),
        (
            &foreign,
            r#"{"expected_table_version":"2","physical_layout":{"bloom_columns":[]}}"#,
            404,
            "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
        ),
    ];
    for (caller, body, status, expected_code) in cases {
        let response = send(caller, body.to_owned())
            .await
            .expect("the edit responds");
        assert_eq!(response.status(), status, "{body}");
        let problem: serde_json::Value = response.json().await.expect("a refusal is problem JSON");
        assert_eq!(problem["code"], expected_code, "{body}: {problem}");
        assert!(
            !problem.to_string().contains(&tenant.to_string()),
            "a refusal reveals no tenant: {problem}"
        );
        assert_eq!(describe().await, committed, "{body} changed nothing");
    }
    // The committed edit, the conflict, and the denial reached authorization;
    // decode failures are refused before it.
    server.await_audit_retained().await.expect("audit settles");
    assert_eq!(
        server
            .retained_audit_operation_count(tenant, "vala.bifrost.alter")
            .await
            .expect("count audited decisions"),
        3,
        "every tenant decision on the edit route is audited"
    );
    server.shutdown().await.expect("server shutdown");
}
