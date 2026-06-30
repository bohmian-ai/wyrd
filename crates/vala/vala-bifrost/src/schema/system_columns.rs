use arrow::datatypes::{DataType, Field, TimeUnit};

use crate::types::TableScope;
use wyrd_spec::vala::{DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT};

pub fn with_system_columns(mut user_fields: Vec<Field>, scope: TableScope) -> Vec<Field> {
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
