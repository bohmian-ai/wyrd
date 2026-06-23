use std::sync::Arc;

use arrow::array::{Array, FixedSizeBinaryBuilder, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use arrow::error::ArrowError;
use arrow::record_batch::RecordBatch;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::{
    DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
};

/// Append the server-stamped system columns to a caller batch (user fields only).
///
/// Adds `wyrd_event_time` and `wyrd_ingested_at` (both `ingested_at_us`, UTC
/// microseconds), `wyrd_batch_id` (the 2PC `batch_id`), and — when `tenant` is
/// `Some` (i.e. a `SystemShared` table) — `data_tenant_id`. `TenantOwned` tables
/// pass `None`: the tenant lives in the table's physical path, not a column.
///
/// # Errors
/// Returns [`ArrowError`] when building the batch-id column or assembling the
/// stamped [`RecordBatch`] fails.
pub fn stamp_system_columns(
    batch: RecordBatch,
    ingested_at_us: i64,
    batch_id: [u8; 16],
    tenant: Option<DataTenantId>,
) -> Result<RecordBatch, ArrowError> {
    let nrows = batch.num_rows();

    let event_time = Arc::new(
        TimestampMicrosecondArray::from(vec![ingested_at_us; nrows])
            .with_timezone("UTC".to_string()),
    ) as Arc<dyn Array>;

    let ingested_at = Arc::new(
        TimestampMicrosecondArray::from(vec![ingested_at_us; nrows])
            .with_timezone("UTC".to_string()),
    ) as Arc<dyn Array>;

    let mut fsb_builder = FixedSizeBinaryBuilder::with_capacity(nrows, 16);
    for _ in 0..nrows {
        fsb_builder.append_value(batch_id)?;
    }
    let batch_id_col = Arc::new(fsb_builder.finish()) as Arc<dyn Array>;

    let mut new_schema_fields: Vec<Field> = batch.schema().fields().iter().map(|f| f.as_ref().clone()).collect();
    let mut new_columns: Vec<Arc<dyn Array>> = batch.columns().to_vec();

    let ts_field = Field::new(
        WYRD_EVENT_TIME,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    );
    let ia_field = Field::new(
        WYRD_INGESTED_AT,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    );
    let bid_field = Field::new(WYRD_BATCH_ID, DataType::FixedSizeBinary(16), false);

    new_schema_fields.push(ts_field);
    new_schema_fields.push(ia_field);
    new_schema_fields.push(bid_field);
    new_columns.push(event_time);
    new_columns.push(ingested_at);
    new_columns.push(batch_id_col);

    if let Some(tenant_id) = tenant {
        let tid_col = Arc::new(StringArray::from(vec![tenant_id.to_string(); nrows]))
            as Arc<dyn Array>;
        new_schema_fields.push(Field::new(DATA_TENANT_ID, DataType::Utf8, false));
        new_columns.push(tid_col);
    }

    let new_schema = Arc::new(Schema::new(new_schema_fields)) as SchemaRef;
    RecordBatch::try_new(new_schema, new_columns)
}
