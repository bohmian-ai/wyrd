//! Row → public wire mapping for the engine's list/describe surfaces.
//!
//! `vala-sql` stays a pure row layer; this module is where a stored
//! [`BifrostTableRow`] and Iceberg schema become the Arrow-free
//! `wyrd_spec::vala::api` wire types the HTTP/Python/MCP surfaces read. Every
//! mapping is fail-loud: an unparseable scope/status/fqn or an unrepresentable
//! stored type is a `WYRD_VALA_500_*`, never a silent drop.

use arrow::datatypes::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
use vala_sql::row_types::olap_catalog::BifrostTableRow;
use wyrd_spec::vala::api::{
    BifrostTableEntry, DataTypeSpec, FieldSpec, TableScopeWire, TableStatus, TimeUnit,
};
use wyrd_spec::vala::{is_reserved_correlation_column, is_reserved_system_column};

use crate::error::BifrostError;
use crate::types::TableScope;

/// Metadata key flagging a describe field as a client-supplied correlation
/// column, distinct from user fields and from server-stamped system columns.
const COLUMN_CLASS_KEY: &str = "wyrd:column_class";
/// Metadata value for the correlation-column class.
const COLUMN_CLASS_CORRELATION: &str = "correlation";

/// Lower-case hex of a byte slice.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Decode the persisted table scope string into the public wire enum.
fn scope_wire_from_db(scope: &str) -> Result<TableScopeWire, BifrostError> {
    match TableScope::from_db_str(scope)? {
        TableScope::TenantOwned => Ok(TableScopeWire::TenantOwned),
        TableScope::SystemShared => Ok(TableScopeWire::SystemShared),
    }
}

/// Decode the persisted lifecycle status string, rejecting unknown values.
fn status_from_db(status: &str) -> Result<TableStatus, BifrostError> {
    match status {
        "active" => Ok(TableStatus::Active),
        "deprecated" => Ok(TableStatus::Deprecated),
        "quarantined" => Ok(TableStatus::Quarantined),
        other => Err(BifrostError::MetadataMismatch(format!(
            "unknown table status: {other}"
        ))),
    }
}

/// Map a stored catalog row to the public list entry.
///
/// Unparseable `fqn`/`scope`/`status` are `WYRD_VALA_500_*` failures, never
/// silently dropped, so a corrupt control-plane row surfaces loudly instead of
/// producing a plausible-but-wrong listing.
pub fn entry_from_row(row: &BifrostTableRow) -> Result<BifrostTableEntry, BifrostError> {
    let (namespace, name) = row.fqn.rsplit_once('.').ok_or_else(|| {
        BifrostError::MetadataMismatch(format!("fqn missing namespace separator: {}", row.fqn))
    })?;

    Ok(BifrostTableEntry {
        namespace: namespace.to_string(),
        name: name.to_string(),
        table_uid: to_hex(&row.table_uid),
        scope: scope_wire_from_db(&row.scope)?,
        status: status_from_db(&row.status)?,
        fingerprint: to_hex(&row.fingerprint),
        partition_columns: row.partition_columns.clone(),
        registered_at: row.registered_at,
        updated_at: row.updated_at,
    })
}

/// Map an Arrow logical type to its Arrow-free wire form.
///
/// The stored schema only holds types the register path accepted, so every
/// column is representable; an unrepresentable type is a `WYRD_VALA_500_*`
/// (corrupt/mismatched metadata) rather than a silent coercion.
fn data_type_spec_from_arrow(dt: &DataType) -> Result<DataTypeSpec, BifrostError> {
    let spec = match dt {
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
        DataType::Timestamp(unit, tz) => DataTypeSpec::Timestamp {
            unit: time_unit_from_arrow(*unit),
            tz: tz.as_ref().map(ToString::to_string),
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
            DataTypeSpec::List(Box::new(data_type_spec_from_arrow(field.data_type())?))
        }
        DataType::Struct(fields) => {
            let mut specs = Vec::with_capacity(fields.len());
            for field in fields {
                specs.push(field_to_field_spec(field)?);
            }
            DataTypeSpec::Struct(specs)
        }
        other => {
            return Err(BifrostError::MetadataMismatch(format!(
                "stored column type is not representable on the wire: {other:?}"
            )));
        }
    };
    Ok(spec)
}

/// Map an Arrow time precision to its wire form.
fn time_unit_from_arrow(unit: ArrowTimeUnit) -> TimeUnit {
    match unit {
        ArrowTimeUnit::Second => TimeUnit::Second,
        ArrowTimeUnit::Millisecond => TimeUnit::Millisecond,
        ArrowTimeUnit::Microsecond => TimeUnit::Microsecond,
        ArrowTimeUnit::Nanosecond => TimeUnit::Nanosecond,
    }
}

/// Map a single Arrow field to a wire [`FieldSpec`] (no column-class flag).
fn field_to_field_spec(field: &Field) -> Result<FieldSpec, BifrostError> {
    Ok(FieldSpec {
        name: field.name().clone(),
        data_type: data_type_spec_from_arrow(field.data_type())?,
        nullable: field.is_nullable(),
        metadata: std::collections::BTreeMap::new(),
    })
}

/// Project a stored physical schema onto the describe field list.
///
/// User fields are surfaced unflagged; the universal correlation columns
/// (`run_id`, `card_uid`, `principal_id`) are surfaced with a
/// `wyrd:column_class = correlation` flag; the server-stamped
/// `wyrd_*`/`data_tenant_id` system columns are excluded entirely.
pub fn fields_from_stored_schema(schema: &Schema) -> Result<Vec<FieldSpec>, BifrostError> {
    let mut fields = Vec::new();
    for field in schema.fields() {
        let name = field.name();
        if is_reserved_system_column(name) {
            continue;
        }
        let mut spec = field_to_field_spec(field)?;
        if is_reserved_correlation_column(name) {
            spec.metadata.insert(
                COLUMN_CLASS_KEY.to_string(),
                COLUMN_CLASS_CORRELATION.to_string(),
            );
        }
        fields.push(spec);
    }
    Ok(fields)
}

/// M6 reserved-name guard for the `create_table` pre-build path.
///
/// A *user* field may not take a reserved system column name (`wyrd_*`,
/// `data_tenant_id`) or a reserved universal correlation name (`run_id`,
/// `card_uid`, `principal_id`). Rejected with `WYRD_VALA_400_BIFROST_RESERVED_COLUMN`.
/// For domain tables use `reject_reserved_domain_fields` (policy-aware).
pub fn reject_reserved_field_names(user_fields: &[Field]) -> Result<(), BifrostError> {
    for field in user_fields {
        let name = field.name();
        if is_reserved_system_column(name) || is_reserved_correlation_column(name) {
            return Err(BifrostError::ReservedColumn(name.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field};
    use sqlx::types::Uuid;
    use sqlx::types::chrono::Utc;

    fn sample_row() -> BifrostTableRow {
        BifrostTableRow {
            data_tenant_id: Uuid::now_v7(),
            table_uid: vec![0xabu8; 16],
            fqn: "vala.bifrost.events".to_string(),
            fingerprint: vec![0x01u8; 32],
            scope: "tenant_owned".to_string(),
            status: "active".to_string(),
            partition_columns: vec!["day".to_string()],
            registered_at: Utc::now(),
            updated_at: Utc::now(),
            origin: None,
            actor: None,
        }
    }

    #[test]
    fn list_tables_entry_from_row_maps_all_fields() {
        let entry = entry_from_row(&sample_row()).expect("maps");
        assert_eq!(entry.namespace, "vala.bifrost");
        assert_eq!(entry.name, "events");
        assert_eq!(entry.table_uid, "ab".repeat(16));
        assert_eq!(entry.fingerprint, "01".repeat(32));
        assert_eq!(entry.scope, TableScopeWire::TenantOwned);
        assert_eq!(entry.status, TableStatus::Active);
        assert_eq!(entry.partition_columns, vec!["day".to_string()]);
    }

    #[test]
    fn list_tables_entry_from_row_rejects_bad_scope_and_status() {
        let mut row = sample_row();
        row.scope = "bogus".to_string();
        assert!(entry_from_row(&row).is_err());

        let mut row = sample_row();
        row.status = "bogus".to_string();
        assert!(entry_from_row(&row).is_err());

        let mut row = sample_row();
        row.fqn = "nodot".to_string();
        assert!(entry_from_row(&row).is_err());
    }

    #[test]
    fn describe_table_fields_surface_correlation_flagged() {
        // A physical schema: one user field + reconciled correlation columns + system columns.
        let schema = Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new("wyrd_event_time", DataType::Utf8, false),
            Field::new("wyrd_ingested_at", DataType::Utf8, false),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ]);

        let fields = fields_from_stored_schema(&schema).expect("maps");
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();

        // User + correlation surfaced; system columns excluded.
        assert_eq!(names, vec!["value", "run_id", "card_uid", "principal_id"]);

        let card_uid = fields.iter().find(|f| f.name == "card_uid").unwrap();
        assert_eq!(
            card_uid
                .metadata
                .get("wyrd:column_class")
                .map(String::as_str),
            Some("correlation")
        );

        // User field is distinct: no column-class flag.
        let value = fields.iter().find(|f| f.name == "value").unwrap();
        assert!(value.metadata.is_empty());
        assert_eq!(value.data_type, DataTypeSpec::Int64);
    }

    #[test]
    fn describe_table_maps_nested_and_timestamp_types() {
        let schema = Schema::new(vec![
            Field::new(
                "ts",
                DataType::Timestamp(ArrowTimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "tags",
                DataType::List(std::sync::Arc::new(Field::new(
                    "item",
                    DataType::Utf8,
                    true,
                ))),
                true,
            ),
        ]);

        let fields = fields_from_stored_schema(&schema).expect("maps");
        assert_eq!(
            fields[0].data_type,
            DataTypeSpec::Timestamp {
                unit: TimeUnit::Microsecond,
                tz: Some("UTC".to_string()),
            }
        );
        assert_eq!(
            fields[1].data_type,
            DataTypeSpec::List(Box::new(DataTypeSpec::Utf8))
        );
    }

    #[test]
    fn create_table_reserved_name_guard_rejects_correlation_and_system() {
        for reserved in [
            "run_id",
            "card_uid",
            "principal_id",
            "wyrd_event_time",
            "data_tenant_id",
        ] {
            let fields = vec![Field::new(reserved, DataType::Int64, true)];
            let err = reject_reserved_field_names(&fields).unwrap_err();
            assert!(
                matches!(err, BifrostError::ReservedColumn(ref c) if c == reserved),
                "expected ReservedColumn({reserved}), got {err:?}"
            );
        }
    }

    #[test]
    fn create_table_reserved_name_guard_allows_normal_fields() {
        let fields = vec![
            Field::new("id", DataType::Int64, false),
            Field::new("label", DataType::Utf8, true),
        ];
        assert!(reject_reserved_field_names(&fields).is_ok());
    }
}
