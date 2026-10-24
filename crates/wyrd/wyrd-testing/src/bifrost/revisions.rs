//! One composite-partition table and the catalog revisions journeys apply to it.
//!
//! The table is partitioned by event hour, nullable customer identity, and a
//! four-way shard bucket. The revisions commit through the catalog's public
//! alteration owner from the described version, so a journey can write rows
//! under one layout revision and read, promote, or compact them under the
//! next.

use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::{TableAlteration, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostTableDescription, ColumnRename, PartitionFieldWire, PartitionTransformWire,
    PhysicalLayoutEdit, PhysicalLayoutWire,
};

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
    /// Commits the edit `revise` derives from the table's current definition
    /// through the catalog's public alteration owner, built from the
    /// described version and current layout, and returns the committed
    /// description.
    ///
    /// # Repetition, cancellation, and retry
    ///
    /// Every revision helper commits through this one. Each call describes
    /// the table anew and edits from that version, so it is not idempotent:
    /// repeating an effective call commits a further version, never a retry
    /// of the first. The edit commits or rolls back as one, so dropping the
    /// future before it returns either changes nothing or leaves the whole
    /// edit committed; it is not retried, and a refused edit panics.
    ///
    /// # Panics
    ///
    /// Panics when the table cannot be described or the edit does not commit.
    async fn alter_table_from_current(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        revise: impl FnOnce(&BifrostTableDescription) -> TableAlteration,
    ) -> BifrostTableDescription {
        let catalog = self.bifrost_catalog();
        let current = catalog
            .describe_table(table, tenant)
            .await
            .expect("the table describes");
        catalog
            .alter_table(tenant, table, revise(&current))
            .await
            .expect("the edit commits")
    }

    /// Commits `columns` as the table's complete ordered Bloom list: a
    /// Bloom-only definition edit that leaves every schema, spec, and sort
    /// order id unchanged and still advances the table version by one.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_bloom_revision(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        columns: &[&str],
    ) {
        self.commit_layout_revision(tenant, table, |layout| PhysicalLayoutEdit {
            bloom_columns: Some(columns.iter().map(|&column| column.to_owned()).collect()),
            ..layout_edit(layout)
        })
        .await;
    }

    /// Commits a new default sort order — the current keys plus `value`
    /// descending — and returns the id the committed order carries.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit or the table cannot be
    /// loaded afterwards.
    pub async fn commit_sort_revision(&self, tenant: DataTenantId, table: &TableRef) -> i32 {
        use wyrd_spec::vala::api::{NullOrderWire, SortDirectionWire, SortKeyWire};
        self.commit_layout_revision(tenant, table, |layout| PhysicalLayoutEdit {
            sort_keys: Some(
                layout
                    .sort_keys
                    .iter()
                    .cloned()
                    .chain([SortKeyWire {
                        column: "value".to_owned(),
                        direction: SortDirectionWire::Desc,
                        null_order: NullOrderWire::Last,
                    }])
                    .collect(),
            ),
            ..layout_edit(layout)
        })
        .await;
        let binding =
            TenantTableBinding::resolve((tenant, table.clone())).expect("binding resolves");
        let order_id = self
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("physical table loads")
            .metadata()
            .default_sort_order_id();
        i32::try_from(order_id).expect("sort order id fits i32")
    }

    /// Commits a partition spec that keeps the current fields except the
    /// `shard` bucket.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_spec_revision(&self, tenant: DataTenantId, table: &TableRef) {
        self.commit_partition_revision(tenant, table, |fields| {
            fields.retain(|field| field.column != "shard");
        })
        .await;
    }

    /// Commits a partition spec that keeps the current fields and adds an
    /// identity partition on `column`, so rows the old spec grouped together
    /// can fan out into one tuple per distinct `column` value.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_identity_partition_revision(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        column: &str,
    ) {
        self.commit_partition_revision(tenant, table, |fields| {
            fields.push(PartitionFieldWire::new(
                column,
                PartitionTransformWire::Identity,
            ));
        })
        .await;
    }

    /// Commits a partition spec that keeps the current fields but buckets
    /// `shard` into `buckets`, so the new spec's tuples have the same shape
    /// and can hold the same values as the old spec's.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_bucket_revision(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        buckets: u32,
    ) {
        self.commit_partition_revision(tenant, table, |fields| {
            for field in fields.iter_mut().filter(|field| field.column == "shard") {
                field.buckets = Some(buckets);
            }
        })
        .await;
    }

    /// Commits a schema that renames `customer` to `client` under its field
    /// id and adds a nullable `note` string.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    pub async fn commit_schema_revision(&self, tenant: DataTenantId, table: &TableRef) {
        self.alter_table_from_current(tenant, table, |current| TableAlteration {
            expected_table_version: current.table_version,
            physical_layout: PhysicalLayoutEdit::default(),
            add_columns: vec![Field::new("note", DataType::Utf8, true)],
            rename_columns: vec![ColumnRename {
                from: "customer".to_owned(),
                to: "client".to_owned(),
            }],
        })
        .await;
    }

    /// Commits the complete layout lists `edit` derives from the current
    /// layout, with no schema change.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    async fn commit_layout_revision(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        edit: impl FnOnce(&PhysicalLayoutWire) -> PhysicalLayoutEdit,
    ) {
        self.alter_table_from_current(tenant, table, |current| TableAlteration {
            expected_table_version: current.table_version,
            physical_layout: edit(&current.physical_layout),
            add_columns: Vec::new(),
            rename_columns: Vec::new(),
        })
        .await;
    }

    /// Commits the current partition fields as `revise` changes them.
    ///
    /// Repetition, cancellation, and retry follow `alter_table_from_current`.
    ///
    /// # Panics
    ///
    /// Panics when the revision does not commit.
    async fn commit_partition_revision(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        revise: impl FnOnce(&mut Vec<PartitionFieldWire>),
    ) {
        self.commit_layout_revision(tenant, table, |layout| {
            let mut fields = layout.partition_by.clone();
            revise(&mut fields);
            PhysicalLayoutEdit {
                partition_by: Some(fields),
                ..layout_edit(layout)
            }
        })
        .await;
    }
}

/// The complete current lists of `layout` as an edit that changes nothing.
fn layout_edit(layout: &PhysicalLayoutWire) -> PhysicalLayoutEdit {
    PhysicalLayoutEdit {
        partition_by: Some(layout.partition_by.clone()),
        sort_keys: Some(layout.sort_keys.clone()),
        bloom_columns: Some(layout.bloom_columns.clone()),
    }
}
