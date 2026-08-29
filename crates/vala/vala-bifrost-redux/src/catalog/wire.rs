//! Redux catalog row and stored-schema projection onto public wire types.

use arrow::datatypes::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
use vala_sql::row_types::olap_catalog::BifrostTableRow;
use wyrd_spec::vala::api::{
    BifrostTableEntry, DataTypeSpec, FieldSpec, PhysicalLayoutWire, TableStatus, TimeUnit,
};
use wyrd_spec::vala::{is_reserved_correlation_column, is_reserved_managed_column};

use crate::catalog::BifrostCatalogError;

const COLUMN_CLASS_KEY: &str = "wyrd:column_class";
const COLUMN_CLASS_CORRELATION: &str = "correlation";

/// Reject user fields that collide with server-owned physical columns.
pub fn reject_reserved_field_names(user_fields: &[Field]) -> Result<(), BifrostCatalogError> {
    for field in user_fields {
        let name = field.name();
        if is_reserved_managed_column(name) || is_reserved_correlation_column(name) {
            return Err(BifrostCatalogError::ReservedColumn(name.clone()));
        }
    }
    Ok(())
}

/// Map one tenant-scoped catalog row to its public list entry.
pub fn entry_from_row(row: &BifrostTableRow) -> Result<BifrostTableEntry, BifrostCatalogError> {
    let (namespace, name) = row.fqn.rsplit_once('.').ok_or_else(|| {
        BifrostCatalogError::MetadataMismatch(format!(
            "fqn missing namespace separator: {}",
            row.fqn
        ))
    })?;
    Ok(BifrostTableEntry {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        table_uid: to_hex(&row.table_uid),
        status: status_from_db(&row.status)?,
        fingerprint: to_hex(&row.fingerprint),
        registered_at: row.registered_at,
        updated_at: row.updated_at,
    })
}

/// Decode the persisted canonical physical layout from one catalog row.
///
/// The `physical_layout` column is written only by the catalog from
/// [`crate::catalog::PhysicalLayout::to_wire`], so anything that fails to
/// deserialize is control-plane corruption, not caller input.
///
/// # Errors
/// Returns [`BifrostCatalogError::MetadataMismatch`] when the stored JSON is
/// not a physical-layout declaration.
pub fn layout_wire_from_row(
    row: &BifrostTableRow,
) -> Result<PhysicalLayoutWire, BifrostCatalogError> {
    serde_json::from_value(row.physical_layout.clone()).map_err(|error| {
        BifrostCatalogError::MetadataMismatch(format!(
            "stored physical_layout for {} is not decodable: {error}",
            row.fqn
        ))
    })
}

/// Project a stored physical schema onto user and correlation fields.
pub fn fields_from_stored_schema(schema: &Schema) -> Result<Vec<FieldSpec>, BifrostCatalogError> {
    let mut fields = Vec::new();
    for field in schema.fields() {
        let name = field.name();
        if is_reserved_managed_column(name) {
            continue;
        }
        let mut spec = field_to_spec(field)?;
        if is_reserved_correlation_column(name) {
            spec.metadata.insert(
                COLUMN_CLASS_KEY.to_owned(),
                COLUMN_CLASS_CORRELATION.to_owned(),
            );
        }
        fields.push(spec);
    }
    Ok(fields)
}

fn status_from_db(status: &str) -> Result<TableStatus, BifrostCatalogError> {
    match status {
        "active" => Ok(TableStatus::Active),
        "deprecated" => Ok(TableStatus::Deprecated),
        "quarantined" => Ok(TableStatus::Quarantined),
        other => Err(BifrostCatalogError::MetadataMismatch(format!(
            "unknown table status: {other}"
        ))),
    }
}

fn field_to_spec(field: &Field) -> Result<FieldSpec, BifrostCatalogError> {
    Ok(FieldSpec {
        name: field.name().clone(),
        data_type: data_type_from_arrow(field.data_type())?,
        nullable: field.is_nullable(),
        metadata: std::collections::BTreeMap::new(),
    })
}

fn data_type_from_arrow(data_type: &DataType) -> Result<DataTypeSpec, BifrostCatalogError> {
    let spec = match data_type {
        DataType::Boolean => DataTypeSpec::Bool,
        DataType::Int8 => DataTypeSpec::Int8,
        DataType::Int16 => DataTypeSpec::Int16,
        DataType::Int32 => DataTypeSpec::Int32,
        DataType::Int64 => DataTypeSpec::Int64,
        DataType::UInt8 => DataTypeSpec::UInt8,
        DataType::UInt16 => DataTypeSpec::UInt16,
        DataType::UInt32 => DataTypeSpec::UInt32,
        DataType::UInt64 => DataTypeSpec::UInt64,
        DataType::Float32 => DataTypeSpec::Float32,
        DataType::Float64 => DataTypeSpec::Float64,
        DataType::Utf8 => DataTypeSpec::Utf8,
        DataType::LargeUtf8 => DataTypeSpec::LargeUtf8,
        DataType::Binary => DataTypeSpec::Binary,
        DataType::LargeBinary => DataTypeSpec::LargeBinary,
        DataType::FixedSizeBinary(len) => DataTypeSpec::FixedSizeBinary { len: *len },
        DataType::Date32 => DataTypeSpec::Date32,
        DataType::Date64 => DataTypeSpec::Date64,
        DataType::Timestamp(unit, timezone) => DataTypeSpec::Timestamp {
            unit: time_unit_from_arrow(*unit),
            tz: timezone.as_ref().map(ToString::to_string),
        },
        DataType::Time32(unit) => DataTypeSpec::Time32 {
            unit: time_unit_from_arrow(*unit),
        },
        DataType::Time64(unit) => DataTypeSpec::Time64 {
            unit: time_unit_from_arrow(*unit),
        },
        DataType::Decimal128(precision, scale) => DataTypeSpec::Decimal128 {
            precision: *precision,
            scale: *scale,
        },
        DataType::List(field) => {
            DataTypeSpec::List(Box::new(data_type_from_arrow(field.data_type())?))
        }
        DataType::Struct(fields) => {
            let fields = fields
                .iter()
                .map(|field| field_to_spec(field))
                .collect::<Result<Vec<_>, _>>()?;
            DataTypeSpec::Struct(fields)
        }
        other => {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "stored column type is not representable on the wire: {other:?}"
            )));
        }
    };
    Ok(spec)
}

const fn time_unit_from_arrow(unit: ArrowTimeUnit) -> TimeUnit {
    match unit {
        ArrowTimeUnit::Second => TimeUnit::Second,
        ArrowTimeUnit::Millisecond => TimeUnit::Millisecond,
        ArrowTimeUnit::Microsecond => TimeUnit::Microsecond,
        ArrowTimeUnit::Nanosecond => TimeUnit::Nanosecond,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use wyrd_spec::vala::{
        PRINCIPAL_ID, RESERVED_CORRELATION_COLUMNS, RESERVED_MANAGED_COLUMNS,
    };

    use super::*;

    /// Every canonical reserved name, managed and correlation alike, is refused.
    ///
    /// The sets are iterated from the spec constants rather than spelled out
    /// here. A hand-written list silently stops covering a name the moment one
    /// is added to the contract, which is how `principal_id` went unproven
    /// while it was already reserved.
    #[test]
    fn every_canonical_reserved_field_name_is_rejected() {
        let canonical = RESERVED_MANAGED_COLUMNS
            .iter()
            .chain(RESERVED_CORRELATION_COLUMNS.iter());
        for name in canonical {
            let error = reject_reserved_field_names(&[Field::new(*name, DataType::Int64, true)])
                .expect_err("every reserved field must fail");
            assert!(
                matches!(error, BifrostCatalogError::ReservedColumn(ref column) if column == name),
                "reserved column {name} was refused as {error:?}"
            );
        }
        assert!(
            RESERVED_CORRELATION_COLUMNS.contains(&PRINCIPAL_ID),
            "the server-stamped principal column must stay reserved"
        );
    }

    /// Projection classifies exactly the correlation columns, hides the managed
    /// ones, and leaves user fields unannotated.
    ///
    /// The correlation class is what tells a client which columns it may not
    /// declare but will still read back. Missing one member — again, the case
    /// `principal_id` was in — would present a server-stamped column as an
    /// ordinary user field.
    #[test]
    fn stored_schema_projection_classifies_every_correlation_column() {
        let mut fields: Vec<Field> = RESERVED_MANAGED_COLUMNS
            .iter()
            .chain(RESERVED_CORRELATION_COLUMNS.iter())
            .map(|name| Field::new(*name, DataType::Utf8, true))
            .collect();
        fields.push(Field::new("value", DataType::Int64, true));
        let schema = Schema::new(fields);

        let projected =
            fields_from_stored_schema(&schema).expect("the stored schema projects onto the wire");

        let classified: Vec<&str> = projected
            .iter()
            .filter(|spec| {
                spec.metadata.get(COLUMN_CLASS_KEY).map(String::as_str)
                    == Some(COLUMN_CLASS_CORRELATION)
            })
            .map(|spec| spec.name.as_str())
            .collect();
        assert_eq!(
            classified, RESERVED_CORRELATION_COLUMNS,
            "exactly the canonical correlation columns carry the correlation class"
        );
        assert_eq!(
            projected.len(),
            RESERVED_CORRELATION_COLUMNS.len() + 1,
            "managed columns must not reach the wire: {projected:?}"
        );
    }
}
