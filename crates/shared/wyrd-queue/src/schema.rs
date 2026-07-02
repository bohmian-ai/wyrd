//! Pure `schema_to_fieldspec` mapping core (PyO3-free).
//!
//! The mapping half of `schema_to_fieldspec` per the locked
//! `06-serialization-spec.md` table: JSON-Schema `Value` → `Vec<FieldSpec>` and
//! `arrow::Schema` → `Vec<FieldSpec>`, both returning the Arrow-free C2 wire type
//! [`wyrd_spec::vala::api::FieldSpec`]. The PyO3 acquisition (Pydantic
//! `model_json_schema()` / `pyarrow.Schema`) lives in `vala-sdk` and calls these.

use std::collections::BTreeMap;

use arrow_schema::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
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

fn build_fields(obj_schema: &Value, defs: Option<&Map<String, Value>>) -> Result<Vec<FieldSpec>, WyrdQueueError> {
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

fn map_type(prop: &Value, defs: Option<&Map<String, Value>>) -> Result<DataTypeSpec, WyrdQueueError> {
    if let Some(reference) = prop.get("$ref").and_then(Value::as_str) {
        let resolved = resolve_ref(reference, defs)?;
        return map_type(resolved, defs);
    }
    if let Some(any) = prop.get("anyOf").and_then(Value::as_array) {
        let branch = any
            .iter()
            .find(|b| b.get("type").and_then(Value::as_str) != Some("null"))
            .ok_or_else(|| WyrdQueueError::SchemaParse("anyOf without a non-null branch".to_owned()))?;
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
