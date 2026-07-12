//! Pure `schema_to_fieldspec` mapping core (PyO3-free).
//!
//! The mapping half of `schema_to_fieldspec` per the locked
//! `06-serialization-spec.md` table: JSON-Schema `Value` → `Vec<FieldSpec>` and
//! `arrow::Schema` → `Vec<FieldSpec>`, both returning the Arrow-free C2 wire type
//! [`wyrd_spec::vala::api::FieldSpec`]. The PyO3 acquisition (Pydantic
//! `model_json_schema()` / `pyarrow.Schema`) lives in `vala-sdk` and calls these.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow_schema::{DataType, Field, Fields, Schema, TimeUnit as ArrowTimeUnit};
use serde_json::{Map, Value};
use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec, TimeUnit};

use crate::error::WyrdQueueError;

/// Walk a JSON-Schema object (a Pydantic `model_json_schema()` output) into the
/// wire `Vec<FieldSpec>`, per the locked mapping table.
///
/// User fields only; field order follows the input `Value`'s property order.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] on a free-form `object` (no
/// `properties`), an `array` with no `items`, an unsupported type, or an
/// unresolvable `$ref`.
pub fn json_schema_to_fieldspec(schema: &Value) -> Result<Vec<FieldSpec>, WyrdQueueError> {
    let defs = schema.get("$defs").and_then(Value::as_object);
    build_fields(schema, defs)
}

/// Walk a Rust `arrow::Schema` into the wire `Vec<FieldSpec>`.
///
/// The precision path: a caller who needs `Int32`, a non-UTC `tz`, `Decimal128`,
/// or `FixedSizeBinary` supplies an explicit Arrow schema. Field order and
/// nullability are taken verbatim from the Arrow fields.
#[must_use]
pub fn arrow_schema_to_fieldspec(schema: &Schema) -> Vec<FieldSpec> {
    schema.fields().iter().map(|f| field_to_spec(f)).collect()
}

/// Map a `Vec<FieldSpec>` into an Arrow `Schema`, the client-side forward
/// direction that mirrors `arrow_schema_to_fieldspec`.
///
/// Every `DataTypeSpec` variant in the register-accepted set maps to exactly the
/// `arrow::DataType` the server twin `data_type_to_arrow` would produce. List
/// item fields are named `"item"` and are nullable; Struct fields recurse.
///
/// # Errors
/// Returns `Ok` for all supported `DataTypeSpec` variants. The function
/// signature returns `Result` for symmetry with `json_schema_to_arrow`.
pub fn fieldspec_to_arrow(fields: &[FieldSpec]) -> Result<Schema, WyrdQueueError> {
    let arrow_fields: Vec<Field> = fields
        .iter()
        .map(|f| {
            Field::new(
                f.name.as_str(),
                data_type_to_arrow(&f.data_type),
                f.nullable,
            )
        })
        .collect();
    Ok(Schema::new(arrow_fields))
}

/// Walk a JSON-Schema object into an Arrow `Schema` in one step.
///
/// Composes `json_schema_to_fieldspec` then `fieldspec_to_arrow`. All
/// `SchemaParse` failure modes are owned by the parse step.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] on an unsupported or malformed
/// JSON-Schema node, forwarded from `json_schema_to_fieldspec`.
pub fn json_schema_to_arrow(schema: &Value) -> Result<Schema, WyrdQueueError> {
    fieldspec_to_arrow(&json_schema_to_fieldspec(schema)?)
}

fn build_fields(
    obj_schema: &Value,
    defs: Option<&Map<String, Value>>,
) -> Result<Vec<FieldSpec>, WyrdQueueError> {
    let props = obj_schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(free_form_dict)?;
    let required = obj_schema.get("required").and_then(Value::as_array);
    let has_required = required.is_some();
    let required: std::collections::HashSet<&str> = required
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut out = Vec::with_capacity(props.len());
    for (name, prop) in props {
        let optional = is_optional(prop);
        // Nullability: an `anyOf[..,null]` (Optional[T]) or a field absent from
        // `required` is nullable; a field in `required` is not. Defaults to true
        // when the schema names no `required` set at all.
        let nullable = if optional {
            true
        } else if has_required {
            !required.contains(name.as_str())
        } else {
            true
        };
        out.push(FieldSpec {
            name: name.clone(),
            data_type: map_type(prop, defs)?,
            nullable,
            metadata: BTreeMap::new(),
        });
    }
    Ok(out)
}

fn map_type(
    prop: &Value,
    defs: Option<&Map<String, Value>>,
) -> Result<DataTypeSpec, WyrdQueueError> {
    if let Some(reference) = prop.get("$ref").and_then(Value::as_str) {
        let resolved = resolve_ref(reference, defs)?;
        return map_type(resolved, defs);
    }
    if let Some(any) = prop.get("anyOf").and_then(Value::as_array) {
        let branch = any
            .iter()
            .find(|b| b.get("type").and_then(Value::as_str) != Some("null"))
            .ok_or_else(|| {
                WyrdQueueError::SchemaParse("anyOf without a non-null branch".to_owned())
            })?;
        return map_type(branch, defs);
    }
    // A bare `{"enum": [...]}` (no explicit type) is a string enumeration → Utf8;
    // dictionary-encoding is a builder optimization, not a wire type.
    if prop.get("type").is_none() && prop.get("enum").is_some() {
        return Ok(DataTypeSpec::Utf8);
    }
    match prop.get("type").and_then(Value::as_str) {
        Some("integer") => Ok(DataTypeSpec::Int64),
        Some("number") => Ok(DataTypeSpec::Float64),
        Some("boolean") => Ok(DataTypeSpec::Bool),
        Some("string") => Ok(match prop.get("format").and_then(Value::as_str) {
            Some("date-time") => DataTypeSpec::Timestamp {
                unit: TimeUnit::Microsecond,
                tz: Some("UTC".to_owned()),
            },
            Some("date") => DataTypeSpec::Date32,
            _ => DataTypeSpec::Utf8,
        }),
        Some("array") => {
            let items = prop.get("items").ok_or_else(|| {
                WyrdQueueError::SchemaParse("array schema missing `items`".to_owned())
            })?;
            Ok(DataTypeSpec::List(Box::new(map_type(items, defs)?)))
        }
        Some("object") => {
            if prop.get("properties").is_some() {
                Ok(DataTypeSpec::Struct(build_fields(prop, defs)?))
            } else {
                Err(free_form_dict())
            }
        }
        Some(other) => Err(WyrdQueueError::SchemaParse(format!(
            "unsupported JSON-Schema type: {other}"
        ))),
        None => Err(WyrdQueueError::SchemaParse(
            "schema node has no `type`, `$ref`, `anyOf`, or `enum`".to_owned(),
        )),
    }
}

fn resolve_ref<'a>(
    reference: &str,
    defs: Option<&'a Map<String, Value>>,
) -> Result<&'a Value, WyrdQueueError> {
    let name = reference.strip_prefix("#/$defs/").ok_or_else(|| {
        WyrdQueueError::SchemaParse(format!("unsupported $ref form: {reference}"))
    })?;
    defs.and_then(|d| d.get(name))
        .ok_or_else(|| WyrdQueueError::SchemaParse(format!("unresolvable $ref: {reference}")))
}

fn is_optional(prop: &Value) -> bool {
    prop.get("anyOf")
        .and_then(Value::as_array)
        .is_some_and(|any| {
            any.iter()
                .any(|b| b.get("type").and_then(Value::as_str) == Some("null"))
        })
}

fn free_form_dict() -> WyrdQueueError {
    WyrdQueueError::SchemaParse(
        "free-form dict unsupported — declare a model or a `pyarrow.Schema`".to_owned(),
    )
}

fn field_to_spec(field: &Field) -> FieldSpec {
    FieldSpec {
        name: field.name().clone(),
        data_type: dtspec_from_arrow(field.data_type()),
        nullable: field.is_nullable(),
        metadata: BTreeMap::new(),
    }
}

fn dtspec_from_arrow(dt: &DataType) -> DataTypeSpec {
    match dt {
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
        DataType::List(field) => DataTypeSpec::List(Box::new(dtspec_from_arrow(field.data_type()))),
        DataType::Struct(fields) => {
            DataTypeSpec::Struct(fields.iter().map(|f| field_to_spec(f)).collect())
        }
        // A valid, register-accepted Arrow schema only carries the representable
        // set above; an out-of-set type is coerced to Utf8 rather than dropped.
        _ => DataTypeSpec::Utf8,
    }
}

fn time_unit_from_arrow(unit: ArrowTimeUnit) -> TimeUnit {
    match unit {
        ArrowTimeUnit::Second => TimeUnit::Second,
        ArrowTimeUnit::Millisecond => TimeUnit::Millisecond,
        ArrowTimeUnit::Microsecond => TimeUnit::Microsecond,
        ArrowTimeUnit::Nanosecond => TimeUnit::Nanosecond,
    }
}

fn time_unit_to_arrow(unit: TimeUnit) -> ArrowTimeUnit {
    match unit {
        TimeUnit::Second => ArrowTimeUnit::Second,
        TimeUnit::Millisecond => ArrowTimeUnit::Millisecond,
        TimeUnit::Microsecond => ArrowTimeUnit::Microsecond,
        TimeUnit::Nanosecond => ArrowTimeUnit::Nanosecond,
    }
}

fn data_type_to_arrow(spec: &DataTypeSpec) -> DataType {
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
            DataType::Timestamp(time_unit_to_arrow(*unit), tz.as_deref().map(Into::into))
        }
        DataTypeSpec::Time32 { unit } => DataType::Time32(time_unit_to_arrow(*unit)),
        DataTypeSpec::Time64 { unit } => DataType::Time64(time_unit_to_arrow(*unit)),
        DataTypeSpec::Decimal128 { precision, scale } => DataType::Decimal128(*precision, *scale),
        DataTypeSpec::List(inner) => DataType::List(Arc::new(Field::new(
            "item",
            data_type_to_arrow(inner),
            true,
        ))),
        DataTypeSpec::Struct(fields) => {
            let arrow_fields: Vec<Field> = fields
                .iter()
                .map(|f| {
                    Field::new(
                        f.name.as_str(),
                        data_type_to_arrow(&f.data_type),
                        f.nullable,
                    )
                })
                .collect();
            DataType::Struct(Fields::from(arrow_fields))
        }
    }
}

#[cfg(test)]
mod schema_tests {
    //! `schema_to_fieldspec` mapping-table proof: JSON-Schema and Arrow → C2 `FieldSpec`.

    use crate::schema::{
        arrow_schema_to_fieldspec, fieldspec_to_arrow, json_schema_to_arrow,
        json_schema_to_fieldspec,
    };
    use arrow_schema::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
    use serde_json::json;
    use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec, TimeUnit};

    fn field<'a>(specs: &'a [FieldSpec], name: &str) -> &'a FieldSpec {
        specs
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("field `{name}` present"))
    }

    #[test]
    fn json_scalar_types_map_per_table() {
        let schema = json!({
            "properties": {
                "count": {"type": "integer"},
                "ratio": {"type": "number"},
                "active": {"type": "boolean"},
                "label": {"type": "string"},
                "when": {"type": "string", "format": "date-time"},
                "day": {"type": "string", "format": "date"},
            },
            "required": ["count"]
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        assert_eq!(field(&specs, "count").data_type, DataTypeSpec::Int64);
        assert_eq!(field(&specs, "ratio").data_type, DataTypeSpec::Float64);
        assert_eq!(field(&specs, "active").data_type, DataTypeSpec::Bool);
        assert_eq!(field(&specs, "label").data_type, DataTypeSpec::Utf8);
        assert_eq!(
            field(&specs, "when").data_type,
            DataTypeSpec::Timestamp {
                unit: TimeUnit::Microsecond,
                tz: Some("UTC".to_owned())
            }
        );
        assert_eq!(field(&specs, "day").data_type, DataTypeSpec::Date32);
    }

    #[test]
    fn required_drives_nullability() {
        let schema = json!({
            "properties": {
                "id": {"type": "integer"},
                "note": {"type": "string"},
            },
            "required": ["id"]
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        assert!(
            !field(&specs, "id").nullable,
            "required field is non-nullable"
        );
        assert!(field(&specs, "note").nullable, "unlisted field is nullable");
    }

    #[test]
    fn optional_anyof_null_is_nullable() {
        let schema = json!({
            "properties": {
                "maybe": {"anyOf": [{"type": "integer"}, {"type": "null"}]},
            },
            "required": ["maybe"]
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        let f = field(&specs, "maybe");
        assert_eq!(f.data_type, DataTypeSpec::Int64);
        assert!(f.nullable, "Optional[int] is nullable even when required");
    }

    #[test]
    fn nested_object_becomes_struct() {
        let schema = json!({
            "properties": {
                "addr": {
                    "type": "object",
                    "properties": {"city": {"type": "string"}, "zip": {"type": "integer"}},
                    "required": ["city"]
                }
            }
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        let DataTypeSpec::Struct(inner) = &field(&specs, "addr").data_type else {
            panic!("addr maps to a struct");
        };
        assert_eq!(field(inner, "city").data_type, DataTypeSpec::Utf8);
        assert!(!field(inner, "city").nullable);
        assert_eq!(field(inner, "zip").data_type, DataTypeSpec::Int64);
    }

    #[test]
    fn array_becomes_list() {
        let schema = json!({
            "properties": {"tags": {"type": "array", "items": {"type": "string"}}}
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        assert_eq!(
            field(&specs, "tags").data_type,
            DataTypeSpec::List(Box::new(DataTypeSpec::Utf8))
        );
    }

    #[test]
    fn ref_resolves_through_defs() {
        let schema = json!({
            "$defs": {
                "Point": {"type": "object", "properties": {"x": {"type": "number"}}}
            },
            "properties": {"p": {"$ref": "#/$defs/Point"}}
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        let DataTypeSpec::Struct(inner) = &field(&specs, "p").data_type else {
            panic!("$ref resolves to the struct");
        };
        assert_eq!(field(inner, "x").data_type, DataTypeSpec::Float64);
    }

    #[test]
    fn free_form_dict_is_schema_parse() {
        let err = json_schema_to_fieldspec(&json!({"type": "object"})).unwrap_err();
        assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE");

        let nested = json!({"properties": {"blob": {"type": "object"}}});
        let err = json_schema_to_fieldspec(&nested).unwrap_err();
        assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE");
    }

    #[test]
    fn arrow_schema_maps_precision_types_verbatim() {
        let schema = Schema::new(vec![
            Field::new("small", DataType::Int32, false),
            Field::new("money", DataType::Decimal128(10, 2), true),
            Field::new(
                "local",
                DataType::Timestamp(ArrowTimeUnit::Nanosecond, Some("America/New_York".into())),
                true,
            ),
        ]);
        let specs = arrow_schema_to_fieldspec(&schema);

        assert_eq!(field(&specs, "small").data_type, DataTypeSpec::Int32);
        assert!(!field(&specs, "small").nullable);
        assert_eq!(
            field(&specs, "money").data_type,
            DataTypeSpec::Decimal128 {
                precision: 10,
                scale: 2
            }
        );
        assert_eq!(
            field(&specs, "local").data_type,
            DataTypeSpec::Timestamp {
                unit: TimeUnit::Nanosecond,
                tz: Some("America/New_York".to_owned())
            }
        );
    }

    // ── fieldspec_to_arrow round-trip tests ───────────────────────────────────────

    fn make_field(name: &str, dt: DataTypeSpec, nullable: bool) -> FieldSpec {
        use std::collections::BTreeMap;
        FieldSpec {
            name: name.to_owned(),
            data_type: dt,
            nullable,
            metadata: BTreeMap::new(),
        }
    }

    fn round_trip(specs: Vec<FieldSpec>) -> Vec<FieldSpec> {
        let schema = fieldspec_to_arrow(&specs).expect("fieldspec_to_arrow");
        arrow_schema_to_fieldspec(&schema)
    }

    #[test]
    fn fieldspec_to_arrow_scalar_variants_round_trip() {
        let specs = vec![
            make_field("f_bool", DataTypeSpec::Bool, false),
            make_field("f_i8", DataTypeSpec::Int8, true),
            make_field("f_i16", DataTypeSpec::Int16, false),
            make_field("f_i32", DataTypeSpec::Int32, true),
            make_field("f_i64", DataTypeSpec::Int64, false),
            make_field("f_u8", DataTypeSpec::UInt8, true),
            make_field("f_u16", DataTypeSpec::UInt16, false),
            make_field("f_u32", DataTypeSpec::UInt32, true),
            make_field("f_u64", DataTypeSpec::UInt64, false),
            make_field("f_f32", DataTypeSpec::Float32, true),
            make_field("f_f64", DataTypeSpec::Float64, false),
            make_field("f_utf8", DataTypeSpec::Utf8, true),
            make_field("f_large_utf8", DataTypeSpec::LargeUtf8, false),
            make_field("f_binary", DataTypeSpec::Binary, true),
            make_field("f_large_binary", DataTypeSpec::LargeBinary, false),
            make_field("f_fsb", DataTypeSpec::FixedSizeBinary { len: 16 }, true),
            make_field("f_date32", DataTypeSpec::Date32, false),
            make_field("f_date64", DataTypeSpec::Date64, true),
            make_field(
                "f_dec",
                DataTypeSpec::Decimal128 {
                    precision: 12,
                    scale: 4,
                },
                false,
            ),
        ];
        assert_eq!(round_trip(specs.clone()), specs);
    }

    #[test]
    fn fieldspec_to_arrow_timestamp_all_time_units_round_trip() {
        for unit in [
            TimeUnit::Second,
            TimeUnit::Millisecond,
            TimeUnit::Microsecond,
            TimeUnit::Nanosecond,
        ] {
            let specs = vec![
                make_field(
                    "ts_utc",
                    DataTypeSpec::Timestamp {
                        unit,
                        tz: Some("UTC".to_owned()),
                    },
                    false,
                ),
                make_field("ts_naive", DataTypeSpec::Timestamp { unit, tz: None }, true),
            ];
            assert_eq!(round_trip(specs.clone()), specs, "unit={unit:?}");
        }
    }

    #[test]
    fn fieldspec_to_arrow_time32_time64_all_units_round_trip() {
        let specs = vec![
            make_field(
                "t32s",
                DataTypeSpec::Time32 {
                    unit: TimeUnit::Second,
                },
                false,
            ),
            make_field(
                "t32ms",
                DataTypeSpec::Time32 {
                    unit: TimeUnit::Millisecond,
                },
                true,
            ),
            make_field(
                "t64us",
                DataTypeSpec::Time64 {
                    unit: TimeUnit::Microsecond,
                },
                false,
            ),
            make_field(
                "t64ns",
                DataTypeSpec::Time64 {
                    unit: TimeUnit::Nanosecond,
                },
                true,
            ),
        ];
        assert_eq!(round_trip(specs.clone()), specs);
    }

    #[test]
    fn fieldspec_to_arrow_list_round_trip() {
        let specs = vec![
            make_field(
                "tags",
                DataTypeSpec::List(Box::new(DataTypeSpec::Utf8)),
                true,
            ),
            make_field(
                "counts",
                DataTypeSpec::List(Box::new(DataTypeSpec::Int64)),
                false,
            ),
        ];
        assert_eq!(round_trip(specs.clone()), specs);
    }

    #[test]
    fn fieldspec_to_arrow_struct_round_trip() {
        let inner = vec![
            make_field("x", DataTypeSpec::Float64, false),
            make_field("y", DataTypeSpec::Float64, true),
        ];
        let specs = vec![make_field("point", DataTypeSpec::Struct(inner), true)];
        assert_eq!(round_trip(specs.clone()), specs);
    }

    #[test]
    fn fieldspec_to_arrow_nullability_preserved() {
        let specs = vec![
            make_field("non_null", DataTypeSpec::Int64, false),
            make_field("nullable", DataTypeSpec::Int64, true),
        ];
        let out = round_trip(specs.clone());
        assert!(!out[0].nullable);
        assert!(out[1].nullable);
    }

    // ── json_schema_to_arrow tests ────────────────────────────────────────────────

    #[test]
    fn json_schema_to_arrow_equals_compose_path() {
        let schema = json!({
            "properties": {
                "id": {"type": "integer"},
                "name": {"type": "string"},
                "score": {"type": "number"},
                "active": {"type": "boolean"},
            },
            "required": ["id"]
        });
        let direct = json_schema_to_arrow(&schema).expect("direct");
        let via_compose = fieldspec_to_arrow(&json_schema_to_fieldspec(&schema).expect("parse"))
            .expect("compose");
        assert_eq!(direct, via_compose);
    }

    #[test]
    fn json_schema_to_arrow_array_missing_items_is_schema_parse() {
        let schema = json!({"properties": {"bad": {"type": "array"}}});
        let err = json_schema_to_arrow(&schema).unwrap_err();
        assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE");
    }

    #[test]
    fn json_schema_to_arrow_free_form_dict_is_schema_parse() {
        let schema = json!({"type": "object"});
        let err = json_schema_to_arrow(&schema).unwrap_err();
        assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE");
    }
}
