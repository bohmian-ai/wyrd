//! One composite-partition table and the catalog revisions journeys apply to it.
//!
//! The table is partitioned by event hour, nullable customer identity, and a
//! four-way shard bucket. The revisions commit straight through the table's
//! Iceberg catalog under the same current-id guards a public alteration uses,
//! so a journey can write rows under one layout revision and read, promote, or
//! compact them under the next.

use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use iceberg::TableRequirement;
use iceberg::spec::TableMetadata;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{PartitionFieldWire, PartitionTransformWire, PhysicalLayoutWire};

use crate::WyrdTestServer;

/// One hour in microseconds.
pub const COMPOSITE_HOUR_MICROS: i64 = 3_600_000_000;
/// Rows in one composite batch.
pub const COMPOSITE_ROWS: i64 = 12;

/// Start of yesterday, UTC, in microseconds: the first composite hour.
///
/// Relative so rows stay inside Scribe's event-time acceptance window, and a
/// closed day so Forge treats its partitions as complete.
///
/// # Panics
///
/// Never in practice: midnight is representable on every day.
#[must_use]
pub fn composite_first_hour_micros() -> i64 {
    super::forge_harness::default_fixture_day()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always representable")
        .and_utc()
        .timestamp_micros()
}

/// Registers the composite table `vala.datasets.<name>` through the real
/// catalog and returns its qualified name.
///
/// # Panics
///
/// Panics when the catalog refuses the registration.
pub async fn register_composite_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> String {
    server
        .create_bifrost_table_for_test(vala_bifrost_redux::catalog::CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Datasets, name),
            user_fields: composite_schema()
                .fields()
                .iter()
                .map(|field| field.as_ref().clone())
                .collect(),
            tenant,
            attribution: wyrd_spec::auth::Attribution::unresolved(),
            physical_layout: Some(PhysicalLayoutWire {
                time_column: Some("event_time".to_owned()),
                partition_by: vec![
                    PartitionFieldWire::new("event_time", PartitionTransformWire::Hour),
                    PartitionFieldWire::new("customer", PartitionTransformWire::Identity),
                    PartitionFieldWire {
                        buckets: Some(4),
                        ..PartitionFieldWire::new("shard", PartitionTransformWire::Bucket)
                    },
                ],
                sort_keys: Vec::new(),
                bloom_columns: Vec::new(),
            }),
        })
        .await
        .expect("the catalog registers the composite table");
    format!("vala.datasets.{name}")
}

/// The composite table's user schema.
#[must_use]
pub fn composite_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new(
            "event_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("customer", DataType::Utf8, true),
        Field::new("shard", DataType::Int64, false),
        Field::new("value", DataType::Int64, false),
    ]))
}

/// Row `value` spans two hours, customers `a`, `b`, and null, and five shards.
///
/// # Panics
///
/// Panics when the fixed columns do not assemble a batch.
#[must_use]
pub fn composite_batch() -> RecordBatch {
    let first = composite_first_hour_micros();
    let rows = 0..COMPOSITE_ROWS;
    RecordBatch::try_new(
        composite_schema(),
        vec![
            Arc::new(
                arrow::array::TimestampMicrosecondArray::from_iter_values(
                    rows.clone()
                        .map(|row| first + (row % 2) * COMPOSITE_HOUR_MICROS + row),
                )
                .with_timezone("UTC"),
            ),
            Arc::new(arrow::array::StringArray::from_iter(rows.clone().map(
                |row| match row % 3 {
                    0 => Some("a"),
                    1 => Some("b"),
                    _ => None,
                },
            ))),
            Arc::new(Int64Array::from_iter_values(
                rows.clone().map(|row| row % 5),
            )),
            Arc::new(Int64Array::from_iter_values(rows)),
        ],
    )
    .expect("composite batch")
}

/// Returns `batch` with every `value` shifted by `by`, so two composite batches
/// stay distinguishable on read.
///
/// # Panics
///
/// Panics when `batch` is not a composite batch.
#[must_use]
pub fn shifted(batch: &RecordBatch, by: i64) -> RecordBatch {
    let values = batch
        .column(3)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("value is Int64");
    let mut columns = batch.columns().to_vec();
    columns[3] = Arc::new(Int64Array::from_iter_values(
        values.values().iter().map(|value| value + by),
    ));
    RecordBatch::try_new(batch.schema(), columns).expect("shifted batch")
}

impl WyrdTestServer {
    /// Commits the updates `revise` derives from a table's current metadata
    /// straight through its Iceberg catalog, guarded by the current schema,
    /// spec, and sort-order ids, then evicts this node's cached revision so
    /// the next admission reads it.
    ///
    /// # Panics
    ///
    /// Panics when the table cannot be loaded or the revision does not commit.
    pub async fn commit_table_revision(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        revise: impl FnOnce(&TableMetadata) -> Vec<iceberg::TableUpdate>,
    ) {
        let catalog = self.bifrost_catalog();
        let binding =
            TenantTableBinding::resolve((tenant, table.clone())).expect("binding resolves");
        let physical = catalog
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("physical table loads");
        let metadata = physical.metadata();
        let transaction = Transaction::new(&physical);
        let action = revise(metadata).into_iter().fold(
            transaction
                .apply_updates()
                .requirement(TableRequirement::CurrentSchemaIdMatch {
                    current_schema_id: metadata.current_schema_id(),
                })
                .requirement(TableRequirement::DefaultSpecIdMatch {
                    default_spec_id: metadata.default_partition_spec_id(),
                })
                .requirement(TableRequirement::DefaultSortOrderIdMatch {
                    default_sort_order_id: metadata.default_sort_order_id(),
                }),
            iceberg::transaction::ApplyUpdatesAction::update,
        );
        ApplyTransactionAction::apply(action, transaction)
            .expect("revision applies")
            .commit(catalog.iceberg_catalog().as_ref())
            .await
            .expect("revision commits");
        catalog.evict_committed_revision(table, tenant);
    }

    /// Commits a new default sort order — the current keys plus `value`
    /// descending — and returns its id.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_sort_revision(&self, tenant: DataTenantId, table: &TableRef) -> i32 {
        use iceberg::spec::{NullOrder, SortDirection, SortField, SortOrder, Transform};
        let mut order_id = 0;
        self.commit_table_revision(tenant, table, |metadata| {
            order_id = metadata
                .sort_orders_iter()
                .map(|order| order.order_id)
                .max()
                .expect("a registered table has a sort order")
                + 1;
            let value_id = metadata
                .current_schema()
                .field_by_name("value")
                .expect("value is a column")
                .id;
            let fields = metadata
                .default_sort_order()
                .fields
                .iter()
                .cloned()
                .chain([SortField {
                    source_id: value_id,
                    transform: Transform::Identity,
                    direction: SortDirection::Descending,
                    null_order: NullOrder::Last,
                }])
                .collect();
            vec![
                iceberg::TableUpdate::AddSortOrder {
                    sort_order: SortOrder::builder()
                        .with_order_id(order_id)
                        .with_fields(fields)
                        .build_unbound()
                        .expect("sort order builds"),
                },
                iceberg::TableUpdate::SetDefaultSortOrder { sort_order_id: -1 },
            ]
        })
        .await;
        i32::try_from(order_id).expect("sort order id fits i32")
    }

    /// Commits a partition spec that keeps the current fields except the
    /// `shard` bucket.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_spec_revision(&self, tenant: DataTenantId, table: &TableRef) {
        self.commit_table_revision(tenant, table, |metadata| {
            let shard = metadata
                .current_schema()
                .field_by_name("shard")
                .expect("shard is a column")
                .id;
            let spec = metadata
                .default_partition_spec()
                .fields()
                .iter()
                .filter(|field| field.source_id != shard)
                .try_fold(
                    iceberg::spec::UnboundPartitionSpec::builder(),
                    |builder, field| {
                        builder.add_partition_field(
                            field.source_id,
                            field.name.clone(),
                            field.transform,
                        )
                    },
                )
                .expect("spec builds")
                .build();
            vec![
                iceberg::TableUpdate::AddSpec { spec },
                iceberg::TableUpdate::SetDefaultSpec { spec_id: -1 },
            ]
        })
        .await;
    }

    /// Commits a schema that renames `customer` to `client` under its field
    /// id and adds a nullable `note` string.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_schema_revision(&self, tenant: DataTenantId, table: &TableRef) {
        use iceberg::spec::{NestedField, PrimitiveType, Schema as IcebergSchema, Type};
        self.commit_table_revision(tenant, table, |metadata| {
            let current = metadata.current_schema();
            let fields = current
                .as_struct()
                .fields()
                .iter()
                .map(|field| {
                    if field.name == "customer" {
                        let mut renamed = field.as_ref().clone();
                        "client".clone_into(&mut renamed.name);
                        Arc::new(renamed)
                    } else {
                        Arc::clone(field)
                    }
                })
                .chain([Arc::new(NestedField::optional(
                    metadata.last_column_id() + 1,
                    "note",
                    Type::Primitive(PrimitiveType::String),
                ))])
                .collect::<Vec<_>>();
            let schema = IcebergSchema::builder()
                .with_schema_id(current.schema_id() + 1)
                .with_identifier_field_ids(current.identifier_field_ids())
                .with_fields(fields)
                .build()
                .expect("schema builds");
            vec![
                iceberg::TableUpdate::AddSchema { schema },
                iceberg::TableUpdate::SetCurrentSchema { schema_id: -1 },
            ]
        })
        .await;
    }
}
