//! Uniform system-column appending for Bifrost physical schemas.

use arrow::datatypes::{DataType, Field, TimeUnit};

use wyrd_spec::vala::system_columns::{
    CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};

/// Extend the user fields with the physical Bifrost columns for a dynamically-created
/// table. Every table uses the same server-owned column order and includes a
/// non-null `data_tenant_id`.
///
/// The correlation columns are server-stamped/resolved but must exist in the stored
/// Iceberg schema so the columnar write has a landing target; they are nullable so
/// the internal `BifrostWriteContext::system()` path can write them as NULL. They do
/// **not** perturb the user-fields-only [`SchemaFingerprint`], which is computed over
/// the user fields alone in `create_table`.
pub fn with_system_columns(mut user_fields: Vec<Field>) -> Vec<Field> {
    user_fields.push(Field::new(RUN_ID, DataType::Utf8, true));
    user_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
    user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, true));
    user_fields.push(Field::new(WYRD_REQUEST_ID, DataType::Utf8, false));
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
    user_fields.push(Field::new(DATA_TENANT_ID, DataType::Utf8, false));
    user_fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_system_columns_always_includes_tenant() {
        let fields = with_system_columns(vec![Field::new("value", DataType::UInt64, false)]);

        assert_eq!(
            fields
                .iter()
                .map(|field| field.name().as_str())
                .collect::<Vec<_>>(),
            vec![
                "value",
                RUN_ID,
                CARD_UID,
                PRINCIPAL_ID,
                WYRD_REQUEST_ID,
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
                WYRD_BATCH_ID,
                DATA_TENANT_ID,
            ]
        );

        let tenant_fields: Vec<_> = fields
            .iter()
            .filter(|field| field.name() == DATA_TENANT_ID)
            .collect();
        assert_eq!(tenant_fields.len(), 1);
        assert_eq!(tenant_fields[0].data_type(), &DataType::Utf8);
        assert!(!tenant_fields[0].is_nullable());
        assert_eq!(
            fields.last().map(|field| field.name().as_str()),
            Some(DATA_TENANT_ID)
        );
    }
}
