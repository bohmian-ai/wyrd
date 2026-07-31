//! Uniform system-column appending for Bifrost physical schemas.

use arrow::array::{Array, Int32Array, RecordBatch};
use arrow::datatypes::{DataType, Field, TimeUnit};

use wyrd_spec::vala::managed_columns::{
    CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID, WYRD_ROW_ORDINAL,
};

/// Invalid persisted representation of the immutable batch-local row identity.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum RowOrdinalInvariant {
    /// The required managed column is absent.
    #[error("persisted batch lacks wyrd_row_ordinal")]
    Missing,
    /// The physical field is nullable despite the required contract.
    #[error("persisted wyrd_row_ordinal field is nullable")]
    Nullable,
    /// The physical array is not Arrow `Int32`.
    #[error("persisted wyrd_row_ordinal is not Int32")]
    WrongType,
    /// A persisted row has no identity value.
    #[error("persisted wyrd_row_ordinal is null at row {row}")]
    Null {
        /// Zero-based physical row containing the invalid null.
        row: usize,
    },
    /// A persisted signed value cannot form the non-negative Rust cursor type.
    #[error("persisted wyrd_row_ordinal is negative at row {row}: {value}")]
    Negative {
        /// Zero-based physical row containing the invalid value.
        row: usize,
        /// Invalid signed Arrow value.
        value: i32,
    },
}

/// Validates and borrows the required non-negative persisted row identities.
///
/// # Errors
///
/// Returns [`RowOrdinalInvariant`] when the managed field is missing,
/// nullable, not `Int32`, contains nulls, or contains a negative value.
pub(crate) fn row_ordinals(batch: &RecordBatch) -> Result<&Int32Array, RowOrdinalInvariant> {
    let index = batch
        .schema()
        .index_of(WYRD_ROW_ORDINAL)
        .map_err(|_| RowOrdinalInvariant::Missing)?;
    if batch.schema().field(index).is_nullable() {
        return Err(RowOrdinalInvariant::Nullable);
    }
    let values = batch
        .column(index)
        .as_any()
        .downcast_ref::<Int32Array>()
        .ok_or(RowOrdinalInvariant::WrongType)?;
    for row in 0..values.len() {
        if values.is_null(row) {
            return Err(RowOrdinalInvariant::Null { row });
        }
        let value = values.value(row);
        if value < 0 {
            return Err(RowOrdinalInvariant::Negative { row, value });
        }
    }
    Ok(values)
}

/// Extend the user fields with the physical Bifrost columns for a dynamically-created
/// table. Every table uses the same server-owned column order and includes a
/// non-null `data_tenant_id`.
///
/// The correlation columns are server-stamped/resolved but must exist in the stored
/// Iceberg schema so the columnar write has a landing target. Optional card/run
/// context stays nullable, while the authenticated principal and request identity
/// are required. These columns do **not** perturb the user-fields-only
/// [`SchemaFingerprint`], which is computed over the user fields alone in
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
    user_fields.push(Field::new(WYRD_ROW_ORDINAL, DataType::Int32, false));
    user_fields.push(Field::new(DATA_TENANT_ID, DataType::Utf8, false));
    user_fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Dynamic schemas include the complete non-null tenant and request identity.
    fn with_managed_columns_always_includes_tenant() {
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
                WYRD_ROW_ORDINAL,
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
        assert_eq!(
            fields.last().map(|field| field.name().as_str()),
            Some(DATA_TENANT_ID)
        );
    }
}
