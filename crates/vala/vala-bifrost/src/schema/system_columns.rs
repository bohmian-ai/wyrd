use arrow::datatypes::{DataType, Field, TimeUnit};

use crate::types::TableScope;
use wyrd_spec::vala::{
    CARD_REF, DATA_TENANT_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
};

/// Extend the user fields with the physical Bifrost columns: the two universal
/// **correlation** columns (`run_id`, `card_ref`, both nullable) followed by the
/// server-owned **system** columns. The correlation columns are client-supplied
/// cell values (never server-stamped) but must exist in the stored Iceberg
/// schema so the columnar write has a landing target; they are nullable so the
/// internal `BifrostWriteContext::system()` path can write them as NULL. They do
/// **not** perturb the user-fields-only [`SchemaFingerprint`], which is computed
/// over the user fields alone in `create_table`.
pub fn with_system_columns(mut user_fields: Vec<Field>, scope: TableScope) -> Vec<Field> {
    user_fields.push(Field::new(RUN_ID, DataType::Utf8, true));
    user_fields.push(Field::new(CARD_REF, DataType::Utf8, true));
    user_fields.push(Field::new(
        WYRD_EVENT_TIME,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    user_fields.push(Field::new(
        WYRD_INGESTED_AT,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    user_fields.push(Field::new(
        WYRD_BATCH_ID,
        DataType::FixedSizeBinary(16),
        false,
    ));
    if scope == TableScope::SystemShared {
        user_fields.push(Field::new(DATA_TENANT_ID, DataType::Utf8, false));
    }
    user_fields
}
