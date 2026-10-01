//! Uniform system-column appending for Bifrost physical schemas.

use arrow::datatypes::{DataType, Field, TimeUnit};

use wyrd_spec::vala::managed_columns::{
    CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
    WYRD_REQUEST_ID,
};

/// Extend the user fields with the physical Bifrost columns for a dynamically-created
/// table. Every table uses the same server-owned column order. Tenant ownership
/// is not a column: every Parquet file proves it in footer metadata.
///
/// The correlation columns are server-stamped/resolved but must exist in the stored
/// Iceberg schema so the columnar write has a landing target. Optional card/run
/// context stays nullable, while the authenticated principal and request identity
/// are required. These columns do **not** perturb the user-fields-only
/// `SchemaFingerprint`, which is computed over the user fields alone in
/// `create_table`.
pub fn with_managed_columns(mut user_fields: Vec<Field>) -> Vec<Field> {
    user_fields.push(Field::new(RUN_ID, DataType::Utf8, true));
    user_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
    user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, false));
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
    user_fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Dynamic schemas include the non-null request identity and no tenant column.
    ///
    /// # Panics
    ///
    /// Panics if the field names differ from the user field followed by the
    /// managed envelope in canonical order (a tenant column would add a name),
    /// or if `principal_id` or `wyrd_request_id` is nullable.
    fn with_managed_columns_appends_the_envelope_without_a_tenant_column() {
        let fields = with_managed_columns(vec![Field::new("value", DataType::UInt64, false)]);

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
            ]
        );
        assert!(
            fields
                .iter()
                .find(|field| field.name() == PRINCIPAL_ID)
                .is_some_and(|field| !field.is_nullable())
        );
        assert!(
            fields
                .iter()
                .find(|field| field.name() == WYRD_REQUEST_ID)
                .is_some_and(|field| !field.is_nullable())
        );
    }
}
