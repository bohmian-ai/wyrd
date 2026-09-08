//! `DataTypeSpec ↔ arrow::DataType` conversion and schema fingerprinting for the
//! Bifrost register path.
//!
//! The Arrow-free wire types live in `wyrd-spec`; the Arrow bridge lives here (in
//! the server), never in `wyrd-spec`. The register handler turns a wire
//! [`FieldSpec`] list into Arrow [`Field`]s for Redux catalog registration
//! and derives the server-authoritative schema fingerprint the same way the
//! catalog does (user-fields-only, over the Arrow schema).

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::SchemaFingerprint;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec, TimeUnit};

/// Resolve a wire namespace string (e.g. `"vala.bifrost"`) to its engine enum.
///
/// Unknown namespaces are a client error rather than a silent default.
pub fn namespace_from_wire(namespace: &str) -> Result<BifrostNamespace, WyrdError> {
    BifrostNamespace::from_wire(namespace).ok_or_else(|| WyrdError::Validation {
        message: format!("unknown Bifrost namespace: {namespace}"),
        details: serde_json::json!({ "namespace": namespace }),
    })
}

/// Map a wire time precision to its Arrow form.
fn time_unit_to_arrow(unit: TimeUnit) -> ArrowTimeUnit {
    match unit {
        TimeUnit::Second => ArrowTimeUnit::Second,
        TimeUnit::Millisecond => ArrowTimeUnit::Millisecond,
        TimeUnit::Microsecond => ArrowTimeUnit::Microsecond,
        TimeUnit::Nanosecond => ArrowTimeUnit::Nanosecond,
    }
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

/// Convert a wire [`DataTypeSpec`] into an Arrow [`DataType`].
pub fn data_type_to_arrow(spec: &DataTypeSpec) -> DataType {
    match spec {
        DataTypeSpec::Bool => DataType::Boolean,
        DataTypeSpec::Int8 => DataType::Int8,
        DataTypeSpec::Int16 => DataType::Int16,
        DataTypeSpec::Int32 => DataType::Int32,
        DataTypeSpec::Int64 => DataType::Int64,
        DataTypeSpec::UInt8 => DataType::UInt8,
        DataTypeSpec::UInt16 => DataType::UInt16,
        DataTypeSpec::UInt32 => DataType::UInt32,
        DataTypeSpec::UInt64 => DataType::UInt64,
        DataTypeSpec::Float32 => DataType::Float32,
        DataTypeSpec::Float64 => DataType::Float64,
        DataTypeSpec::Utf8 => DataType::Utf8,
        DataTypeSpec::LargeUtf8 => DataType::LargeUtf8,
        DataTypeSpec::Binary => DataType::Binary,
        DataTypeSpec::LargeBinary => DataType::LargeBinary,
        DataTypeSpec::FixedSizeBinary { len } => DataType::FixedSizeBinary(*len),
        DataTypeSpec::Date32 => DataType::Date32,
        DataTypeSpec::Date64 => DataType::Date64,
        DataTypeSpec::Timestamp { unit, tz } => {
            DataType::Timestamp(time_unit_to_arrow(*unit), tz.clone().map(Into::into))
        }
        DataTypeSpec::Time32 { unit } => DataType::Time32(time_unit_to_arrow(*unit)),
        DataTypeSpec::Time64 { unit } => DataType::Time64(time_unit_to_arrow(*unit)),
        DataTypeSpec::Decimal128 { precision, scale } => DataType::Decimal128(*precision, *scale),
        DataTypeSpec::List(element) => DataType::List(Arc::new(field_to_arrow(element))),
        DataTypeSpec::Struct(fields) => {
            DataType::Struct(fields.iter().map(field_to_arrow).collect())
        }
    }
}

/// Convert an Arrow [`DataType`] back into a wire [`DataTypeSpec`].
///
/// A stored type outside the register-accepted set is a `500` (the catalog
/// should never hold one), never a silent coercion.
pub fn data_type_from_arrow(dt: &DataType) -> Result<DataTypeSpec, WyrdError> {
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
        DataType::List(element) => DataTypeSpec::List(Box::new(field_from_arrow(element)?)),
        DataType::Struct(fields) => {
            let mut specs = Vec::with_capacity(fields.len());
            for field in fields {
                specs.push(field_from_arrow(field)?);
            }
            DataTypeSpec::Struct(specs)
        }
        other => {
            return Err(WyrdError::Internal {
                message: "stored Bifrost column type is not representable on the wire".to_owned(),
                details: serde_json::json!({ "data_type": format!("{other:?}") }),
            });
        }
    };
    Ok(spec)
}

/// Convert a wire [`FieldSpec`] into an Arrow [`Field`].
///
/// Metadata travels with the field, so a stable `PARQUET:field_id` supplied by
/// a description survives the round trip into Arrow at every nesting depth.
pub fn field_to_arrow(field: &FieldSpec) -> Field {
    Field::new(
        field.name.clone(),
        data_type_to_arrow(&field.data_type),
        field.nullable,
    )
    .with_metadata(field.metadata.clone().into_iter().collect())
}

/// Convert an Arrow [`Field`] back into a wire [`FieldSpec`].
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored type is outside the
/// register-accepted set, propagated from [`data_type_from_arrow`].
pub fn field_from_arrow(field: &Field) -> Result<FieldSpec, WyrdError> {
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

/// Lower-case hex of a byte slice — matches the engine's wire encoding.
pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Server-authoritative, user-fields-only schema fingerprint as lower-case hex.
///
/// Reproduces the Redux catalog fingerprint: the SHA-256 over the
/// user Arrow fields, before system/correlation columns are appended.
pub fn fingerprint_hex(user_fields: &[Field]) -> String {
    let schema = Schema::new(user_fields.to_vec());
    to_hex(&SchemaFingerprint::from_arrow_schema(&schema).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use wyrd_spec::vala::api::PARQUET_FIELD_ID_KEY;

    fn sample_field_specs() -> Vec<FieldSpec> {
        let plain = |name: &str, data_type: DataTypeSpec| FieldSpec {
            name: name.to_owned(),
            data_type,
            nullable: true,
            metadata: BTreeMap::new(),
        };
        vec![
            plain("b", DataTypeSpec::Bool),
            plain("i64", DataTypeSpec::Int64),
            plain("u32", DataTypeSpec::UInt32),
            plain("f64", DataTypeSpec::Float64),
            plain("s", DataTypeSpec::Utf8),
            plain("blob", DataTypeSpec::Binary),
            plain("fixed", DataTypeSpec::FixedSizeBinary { len: 16 }),
            plain(
                "ts",
                DataTypeSpec::Timestamp {
                    unit: TimeUnit::Microsecond,
                    tz: Some("UTC".to_owned()),
                },
            ),
            plain(
                "dec",
                DataTypeSpec::Decimal128 {
                    precision: 38,
                    scale: 9,
                },
            ),
            plain(
                "tags",
                DataTypeSpec::List(Box::new(FieldSpec {
                    name: "item".to_owned(),
                    data_type: DataTypeSpec::Utf8,
                    nullable: true,
                    metadata: BTreeMap::new(),
                })),
            ),
            plain(
                "nested",
                DataTypeSpec::Struct(vec![plain("inner", DataTypeSpec::Int32)]),
            ),
        ]
    }

    #[test]
    fn bifrost_tables_datatype_arrow_fingerprint_round_trips() {
        let specs = sample_field_specs();
        let arrow_fields: Vec<Field> = specs.iter().map(field_to_arrow).collect();
        let forward_fp = fingerprint_hex(&arrow_fields);

        // arrow → spec → arrow must reproduce the exact same fingerprint.
        let round_tripped: Vec<Field> = arrow_fields
            .iter()
            .map(|f| field_from_arrow(f).expect("arrow field maps back to spec"))
            .map(|s| field_to_arrow(&s))
            .collect();
        let round_fp = fingerprint_hex(&round_tripped);

        assert_eq!(forward_fp, round_fp);

        // And the fingerprint reproduces the catalog's own computation.
        let schema = Schema::new(arrow_fields.clone());
        let expected = to_hex(&SchemaFingerprint::from_arrow_schema(&schema).0);
        assert_eq!(forward_fp, expected);
    }

    #[test]
    fn bifrost_tables_namespace_parse_rejects_unknown() {
        assert_eq!(
            namespace_from_wire("vala.bifrost").expect("known"),
            BifrostNamespace::Bifrost
        );
        let err = namespace_from_wire("public").expect_err("unknown namespace rejected");
        assert_eq!(err.status(), 400);
    }

    /// A nested description keeps every child's identity through Arrow.
    ///
    /// The list child is a full declaration, not a synthesized nullable
    /// `item`, and both it and a struct child carry their stable
    /// `PARQUET:field_id` in each direction. A client that rebuilds an Arrow
    /// schema from a description therefore reproduces the stored physical
    /// schema rather than an approximation of it.
    #[test]
    fn nested_field_description_preserves_identity_and_metadata() {
        let id = |value: i32| BTreeMap::from([(PARQUET_FIELD_ID_KEY.to_owned(), value.to_string())]);
        let spec = FieldSpec {
            name: "events".to_owned(),
            data_type: DataTypeSpec::List(Box::new(FieldSpec {
                name: "event".to_owned(),
                data_type: DataTypeSpec::Struct(vec![FieldSpec {
                    name: "attributes".to_owned(),
                    data_type: DataTypeSpec::Binary,
                    nullable: false,
                    metadata: id(20),
                }]),
                nullable: false,
                metadata: id(17),
            })),
            nullable: false,
            metadata: id(16),
        };

        let arrow = field_to_arrow(&spec);
        assert_eq!(arrow.metadata().get(PARQUET_FIELD_ID_KEY), Some(&"16".to_owned()));
        let DataType::List(element) = arrow.data_type() else {
            panic!("a list declaration must project an Arrow list");
        };
        assert_eq!(element.name(), "event");
        assert!(!element.is_nullable());
        assert_eq!(
            element.metadata().get(PARQUET_FIELD_ID_KEY),
            Some(&"17".to_owned())
        );
        let DataType::Struct(children) = element.data_type() else {
            panic!("the list element must project its declared struct");
        };
        assert_eq!(
            children[0].metadata().get(PARQUET_FIELD_ID_KEY),
            Some(&"20".to_owned())
        );

        assert_eq!(
            field_from_arrow(&arrow).expect("the Arrow field maps back to its declaration"),
            spec,
            "every nested name, nullability, and field id survives both directions"
        );
    }
}
