//! System column appending — copied from vala-bifrost.

use arrow::datatypes::{DataType, Field, TimeUnit};

use wyrd_spec::vala::system_columns::{
    CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT,
};

/// Table scope determines whether `data_tenant_id` is appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableScope {
    TenantOwned,
    SystemShared,
}

/// Extend the user fields with the physical Bifrost columns for a dynamically-created
/// (non-domain) table: the three universal correlation columns (`run_id`, `card_uid`,
/// `principal_id`, all nullable) followed by the server-owned system columns.
///
/// For pre-declared domain tables use `ensure_system_cols` in `tables/system_columns.rs`,
/// which appends only the columns the table's `CorrelationPolicy` permits.
///
/// The correlation columns are server-stamped/resolved but must exist in the stored
/// Iceberg schema so the columnar write has a landing target; they are nullable so
/// the internal `BifrostWriteContext::system()` path can write them as NULL. They do
/// **not** perturb the user-fields-only [`SchemaFingerprint`], which is computed over
/// the user fields alone in `create_table`.
pub fn with_system_columns(mut user_fields: Vec<Field>, scope: TableScope) -> Vec<Field> {
    user_fields.push(Field::new(RUN_ID, DataType::Utf8, true));
    user_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
    user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, true));
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
