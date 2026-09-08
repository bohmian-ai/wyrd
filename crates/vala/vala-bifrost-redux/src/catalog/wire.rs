//! Redux catalog row and stored-schema projection onto public wire types.

use arrow::datatypes::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
use vala_sql::row_types::olap_catalog::BifrostTableRow;
use wyrd_spec::vala::api::{
    BifrostTableEntry, DataTypeSpec, FieldSpec, INPUT_CLASS_GATE_CORRELATION, INPUT_CLASS_KEY,
    PhysicalLayoutWire, TableStatus, TimeUnit,
};
use wyrd_spec::vala::{
    CARD_REF, RUN_ID, WYRD_EVENT_TIME, is_reserved_correlation_column, is_reserved_managed_column,
};

use crate::catalog::BifrostCatalogError;

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

/// The three describe field classes projected from one stored physical schema.
///
/// Splitting them is what lets a writer tell what it declares (`user_fields`)
/// from the correlation inputs Gate resolves (`correlation_fields`) and the
/// managed columns it may supply instead of letting the server stamp them
/// (`managed_candidates`).
#[derive(Debug, Clone, PartialEq)]
pub struct DescribedFields {
    /// The table's own declared columns, in stored order.
    pub user_fields: Vec<FieldSpec>,
    /// `card_ref` then `run_id`, the correlation inputs a writer supplies.
    pub correlation_fields: Vec<FieldSpec>,
    /// `wyrd_event_time`, the one managed column a writer may supply itself.
    pub managed_candidates: Vec<FieldSpec>,
}

/// Project a stored physical schema onto the three describe field classes.
///
/// `card_ref` is a Gate input rather than a stored column: it resolves to
/// `card_uid` on write, so it is synthesized here with the gate-correlation
/// input class and carries no field id. Every other declaration is taken from
/// the stored schema, so its type, nullability, and stable field id are the
/// table's actual ones rather than a restatement.
///
/// # Errors
///
/// Returns [`BifrostCatalogError::MetadataMismatch`] when a stored column type
/// is not representable on the wire.
pub fn described_fields_from_stored_schema(
    schema: &Schema,
) -> Result<DescribedFields, BifrostCatalogError> {
    let mut described = DescribedFields {
        user_fields: Vec::new(),
        correlation_fields: vec![FieldSpec {
            name: CARD_REF.to_owned(),
            data_type: DataTypeSpec::Utf8,
            nullable: false,
            metadata: std::collections::BTreeMap::from([(
                INPUT_CLASS_KEY.to_owned(),
                INPUT_CLASS_GATE_CORRELATION.to_owned(),
            )]),
        }],
        managed_candidates: Vec::new(),
    };
    let mut run_id = None;
    for field in schema.fields() {
        let name = field.name().as_str();
        if name == RUN_ID {
            run_id = Some(field_to_spec(field)?);
        } else if name == WYRD_EVENT_TIME {
            described.managed_candidates.push(field_to_spec(field)?);
        } else if !is_reserved_managed_column(name) && !is_reserved_correlation_column(name) {
            described.user_fields.push(field_to_spec(field)?);
        }
    }
    // A table whose correlation policy appends no `run_id` column still accepts
    // one on the wire; it simply has no stored field id to report.
    described
        .correlation_fields
        .push(run_id.unwrap_or_else(|| FieldSpec {
            name: RUN_ID.to_owned(),
            data_type: DataTypeSpec::Utf8,
            nullable: true,
            metadata: std::collections::BTreeMap::new(),
        }));
    Ok(described)
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

/// Project one stored Arrow field onto its wire declaration.
///
/// Arrow metadata is carried verbatim, which is what puts the stored
/// `PARQUET:field_id` (and every nested child's) onto the wire.
///
/// # Errors
///
/// Returns [`BifrostCatalogError::MetadataMismatch`] when the stored type is
/// not representable on the wire.
fn field_to_spec(field: &Field) -> Result<FieldSpec, BifrostCatalogError> {
    Ok(FieldSpec {
        name: field.name().clone(),
        data_type: data_type_from_arrow(field.data_type())?,
        nullable: field.is_nullable(),
        metadata: field
            .metadata()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
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
        DataType::List(element) => DataTypeSpec::List(Box::new(field_to_spec(element)?)),
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
    use wyrd_spec::vala::{PRINCIPAL_ID, RESERVED_CORRELATION_COLUMNS, RESERVED_MANAGED_COLUMNS};

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

    /// Projection splits every stored column into its describe class.
    ///
    /// The split is what tells a client which columns it declares, which it
    /// supplies as correlation inputs, and which the server stamps. Leaking a
    /// server-stamped column such as `principal_id` into `user_fields` would
    /// present it as an ordinary declarable column.
    ///
    /// # Panics
    ///
    /// Panics when a class gains or loses a column.
    #[test]
    fn stored_schema_projection_classifies_every_correlation_column() {
        let mut fields: Vec<Field> = RESERVED_MANAGED_COLUMNS
            .iter()
            .chain(RESERVED_CORRELATION_COLUMNS.iter())
            .map(|name| {
                if *name == WYRD_EVENT_TIME {
                    Field::new(
                        *name,
                        DataType::Timestamp(ArrowTimeUnit::Microsecond, Some("UTC".into())),
                        false,
                    )
                } else {
                    Field::new(*name, DataType::Utf8, true)
                }
            })
            .collect();
        fields.push(Field::new("value", DataType::Int64, true));
        let schema = Schema::new(fields);

        let described = described_fields_from_stored_schema(&schema)
            .expect("the stored schema projects onto the wire");

        assert_eq!(
            described
                .user_fields
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            vec!["value"],
            "only declarable columns are user fields: {:?}",
            described.user_fields
        );
        assert_eq!(
            described
                .correlation_fields
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            vec![CARD_REF, RUN_ID],
            "correlation is exactly the card reference input and the run id"
        );
        assert_eq!(
            described.correlation_fields[0]
                .metadata
                .get(INPUT_CLASS_KEY),
            Some(&INPUT_CLASS_GATE_CORRELATION.to_owned()),
            "card_ref is a Gate input, not a stored column"
        );
        assert_eq!(
            described
                .managed_candidates
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            vec![WYRD_EVENT_TIME],
            "event time is the one managed column a writer may supply"
        );
    }
}
