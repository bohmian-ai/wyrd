//! Pure `schema_to_fieldspec` mapping core (PyO3-free).
//!
//! The mapping half of `schema_to_fieldspec` per the locked
//! `06-serialization-spec.md` table: JSON-Schema `Value` → `Vec<FieldSpec>` and
//! `arrow::Schema` → `Vec<FieldSpec>`, both returning the Arrow-free C2 wire type
//! [`wyrd_spec::vala::api::FieldSpec`]. The PyO3 acquisition (Pydantic
//! `model_json_schema()` / `pyarrow.Schema`) lives in `wyrd_client::bifrost` and calls these.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow_schema::extension::{EXTENSION_TYPE_METADATA_KEY, EXTENSION_TYPE_NAME_KEY};
use arrow_schema::{DataType, Field, Fields, Schema, TimeUnit as ArrowTimeUnit};
use parquet_variant_compute::VariantType;
use serde_json::{Map, Value};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{BifrostTableDescription, DataTypeSpec, FieldSpec, TimeUnit};

use crate::error::WyrdQueueError;
use crate::variant::{is_variant, variant_storage_type};

/// Walk a JSON-Schema object (a Pydantic `model_json_schema()`, Zod
/// `z.toJSONSchema()`, or `schemars` output) into the wire `Vec<FieldSpec>`.
///
/// User fields only; field order follows the input `Value`'s property order.
/// Every node maps through one decision table:
///
/// - an empty or always-true schema, an object without `properties`, a string
///   with `contentMediaType: application/json`, an array without `items`, and
///   a union (`anyOf`, `oneOf`, or a `type` list) with more than one non-null
///   branch are Variant;
/// - a union of exactly one type and null is that type, nullable, and an
///   `allOf` with one member is that member;
/// - an object with `properties` is a Struct and a typed array a List.
///
/// `$ref` resolves through `$defs` or `definitions`.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] for a root without `properties`, an
/// object allowing undeclared keys beside declared properties, an unsupported
/// type, or an unresolvable `$ref`.
pub fn json_schema_to_fieldspec(schema: &Value) -> Result<Vec<FieldSpec>, WyrdQueueError> {
    let defs = schema
        .get("$defs")
        .or_else(|| schema.get("definitions"))
        .and_then(Value::as_object);
    if schema.get("properties").is_none() {
        return Err(WyrdQueueError::SchemaParse(
            "a table schema must be an object with `properties`".to_owned(),
        ));
    }
    build_fields(schema, defs)
}

/// Walk a Rust `arrow::Schema` into the wire `Vec<FieldSpec>`.
///
/// The precision path: a caller who needs `Int32`, a non-UTC `tz`, `Decimal128`,
/// or `FixedSizeBinary` supplies an explicit Arrow schema. Field order and
/// nullability are taken verbatim from the Arrow fields.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] naming the first field whose Arrow
/// type is outside the register-accepted wire set, and
/// [`WyrdQueueError::Contract`] for a type Bifrost cannot store (see
/// [`check_supported`]).
pub fn arrow_schema_to_fieldspec(schema: &Schema) -> Result<Vec<FieldSpec>, WyrdQueueError> {
    let fields = schema
        .fields()
        .iter()
        .map(|f| field_to_spec(f))
        .collect::<Result<Vec<_>, _>>()?;
    check_supported(&fields)?;
    Ok(fields)
}

/// Map a `Vec<FieldSpec>` into an Arrow `Schema`, the client-side forward
/// direction that mirrors `arrow_schema_to_fieldspec`.
///
/// Every `DataTypeSpec` variant in the register-accepted set maps to exactly the
/// `arrow::DataType` the server's register path produces through the same
/// [`spec_to_field`]. A list
/// element and a struct child are full declarations, so their names,
/// nullability are reproduced rather than synthesized. Field metadata is
/// dropped at every depth: see [`spec_to_field`].
///
/// # Errors
/// Returns `Ok` for all supported `DataTypeSpec` variants. The function
/// signature returns `Result` for symmetry with `json_schema_to_arrow`.
pub fn fieldspec_to_arrow(fields: &[FieldSpec]) -> Result<Schema, WyrdQueueError> {
    Ok(Schema::new(
        fields
            .iter()
            .map(|spec| spec_to_field(spec, false))
            .collect::<Vec<_>>(),
    ))
}

/// Build the Arrow schema a writer sends for one described table.
///
/// The layout is the table's own `user_fields`, then its declared correlation
/// inputs in description order, then — only when `include_event_time` — the
/// managed candidates the writer chooses to supply itself. Everything else on
/// the physical table is server-stamped and must not appear on the wire.
///
/// Use this for a direct Arrow writer; [`crate::RowPreflight::from_description`]
/// is the JSON-row path and appends correlation itself.
///
/// # Errors
///
/// Returns [`WyrdQueueError::SchemaParse`] when the description declares a
/// column name twice across its three field classes, which would make the
/// batch ambiguous.
pub fn writable_schema(
    description: &BifrostTableDescription,
    include_event_time: bool,
) -> Result<Schema, WyrdQueueError> {
    let declared = description
        .user_fields
        .iter()
        .chain(&description.correlation_fields)
        .chain(if include_event_time {
            description.managed_candidates.as_slice()
        } else {
            &[]
        });
    let declared = declared.collect::<Vec<_>>();
    if let Some(name) = first_duplicate(declared.iter().map(|spec| spec.name.as_str())) {
        return Err(WyrdQueueError::SchemaParse(format!(
            "described column `{name}` is declared more than once"
        )));
    }
    Ok(Schema::new(
        declared
            .into_iter()
            .map(|spec| spec_to_field(spec, false))
            .collect::<Vec<_>>(),
    ))
}

/// The first column name that occurs more than once in `names`.
///
/// Arrow allows duplicate field names, but a writer's columns are matched by
/// name, so a duplicate would make one of the supplied columns unreachable.
/// The described writable schema and every caller batch refuse it through
/// this one rule.
pub(crate) fn first_duplicate<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    names.into_iter().find(|name| !seen.insert(*name))
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

/// Map the `properties` of one object node into fields.
///
/// A field is nullable when its schema admits null or, when the node names a
/// `required` set, when it is not in that set.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] when the node allows undeclared
/// keys beside its properties, or a property fails [`map_type`].
fn build_fields(
    obj_schema: &Value,
    defs: Option<&Map<String, Value>>,
) -> Result<Vec<FieldSpec>, WyrdQueueError> {
    let props = obj_schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| WyrdQueueError::SchemaParse("`properties` is not an object".to_owned()))?;
    if obj_schema
        .get("additionalProperties")
        .is_some_and(|extra| extra != &Value::Bool(false))
    {
        return Err(WyrdQueueError::SchemaParse(
            "an object allows undeclared keys beside its declared properties; declare a Variant field for open data".to_owned(),
        ));
    }
    let required = obj_schema.get("required").and_then(Value::as_array);
    let has_required = required.is_some();
    let required: std::collections::HashSet<&str> = required
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut out = Vec::with_capacity(props.len());
    for (name, prop) in props {
        let (data_type, admits_null) = map_type(prop, defs)?;
        out.push(FieldSpec {
            name: name.clone(),
            data_type,
            nullable: admits_null || !has_required || !required.contains(name.as_str()),
            metadata: BTreeMap::new(),
        });
    }
    Ok(out)
}

/// Map one schema node to its column type and whether it admits null.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] for a node no column type holds:
/// an always-false schema, a null-only union, a multi-member `allOf`, an
/// unknown `type`, or an unresolvable `$ref`.
fn map_type(
    prop: &Value,
    defs: Option<&Map<String, Value>>,
) -> Result<(DataTypeSpec, bool), WyrdQueueError> {
    let node = match prop {
        Value::Bool(true) => return Ok((DataTypeSpec::Variant, true)),
        Value::Object(node) => node,
        other => {
            return Err(WyrdQueueError::SchemaParse(format!(
                "schema node {other} admits no value"
            )));
        }
    };
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        return map_type(resolve_ref(reference, defs)?, defs);
    }
    if let Some(all) = node.get("allOf").and_then(Value::as_array) {
        return match all.as_slice() {
            [member] => map_type(member, defs),
            _ => Err(WyrdQueueError::SchemaParse(
                "allOf with more than one member is unsupported".to_owned(),
            )),
        };
    }
    if let Some(branches) = node
        .get("anyOf")
        .or_else(|| node.get("oneOf"))
        .and_then(Value::as_array)
    {
        let (nulls, others): (Vec<_>, Vec<_>) = branches.iter().partition(|b| is_null(b));
        return match others.as_slice() {
            [] => Err(WyrdQueueError::SchemaParse(
                "union without a non-null branch".to_owned(),
            )),
            [only] => map_type(only, defs).map(|(dt, admits)| (dt, admits || !nulls.is_empty())),
            _ => Ok((DataTypeSpec::Variant, !nulls.is_empty())),
        };
    }
    if let Some(types) = node.get("type").and_then(Value::as_array) {
        let others: Vec<&str> = types
            .iter()
            .filter_map(Value::as_str)
            .filter(|t| *t != "null")
            .collect();
        let admits_null = others.len() < types.len();
        return match others.as_slice() {
            [] => Err(WyrdQueueError::SchemaParse(
                "union without a non-null branch".to_owned(),
            )),
            [only] => {
                let mut single = node.clone();
                single.insert("type".to_owned(), Value::from(*only));
                map_type(&Value::Object(single), defs)
                    .map(|(dt, admits)| (dt, admits || admits_null))
            }
            _ => Ok((DataTypeSpec::Variant, admits_null)),
        };
    }
    // A bare `{"enum": [...]}` (no explicit type) is a string enumeration → Utf8;
    // dictionary-encoding is a builder optimization, not a wire type.
    if node.get("type").is_none() && node.get("enum").is_some() {
        return Ok((DataTypeSpec::Utf8, false));
    }
    let data_type = match node.get("type").and_then(Value::as_str) {
        // An untyped node constrains nothing: `Any`, `JsonValue`, `unknown`.
        None => return Ok((DataTypeSpec::Variant, true)),
        Some("integer") => DataTypeSpec::Int64,
        Some("number") => DataTypeSpec::Float64,
        Some("boolean") => DataTypeSpec::Bool,
        Some("string") => {
            if node.get("contentMediaType").and_then(Value::as_str) == Some("application/json") {
                DataTypeSpec::Variant
            } else {
                match node.get("format").and_then(Value::as_str) {
                    Some("date-time") => DataTypeSpec::Timestamp {
                        unit: TimeUnit::Microsecond,
                        tz: Some("UTC".to_owned()),
                    },
                    Some("date") => DataTypeSpec::Date32,
                    _ => DataTypeSpec::Utf8,
                }
            }
        }
        Some("array") => match node.get("items") {
            None => DataTypeSpec::Variant,
            Some(items) => {
                let (data_type, nullable) = map_type(items, defs)?;
                DataTypeSpec::List(Box::new(FieldSpec {
                    name: "item".to_owned(),
                    data_type,
                    nullable,
                    metadata: BTreeMap::new(),
                }))
            }
        },
        Some("object") if node.contains_key("properties") => {
            DataTypeSpec::Struct(build_fields(prop, defs)?)
        }
        Some("object") => DataTypeSpec::Variant,
        Some(other) => {
            return Err(WyrdQueueError::SchemaParse(format!(
                "unsupported JSON-Schema type: {other}"
            )));
        }
    };
    Ok((data_type, false))
}

/// Report whether a union branch is the null schema.
fn is_null(branch: &Value) -> bool {
    branch.get("type").and_then(Value::as_str) == Some("null")
}

/// Resolve a local `$ref` through `$defs` or `definitions`.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] for a non-local reference or a
/// missing definition.
fn resolve_ref<'a>(
    reference: &str,
    defs: Option<&'a Map<String, Value>>,
) -> Result<&'a Value, WyrdQueueError> {
    let name = reference
        .strip_prefix("#/$defs/")
        .or_else(|| reference.strip_prefix("#/definitions/"))
        .ok_or_else(|| {
            WyrdQueueError::SchemaParse(format!("unsupported $ref form: {reference}"))
        })?;
    defs.and_then(|d| d.get(name))
        .ok_or_else(|| WyrdQueueError::SchemaParse(format!("unresolvable $ref: {reference}")))
}

/// Refuse every declaration Bifrost cannot store, at any depth.
///
/// This is the one supported-type decision: the SDK schema doors call it
/// before any request and the server register path repeats it. Iceberg
/// stores neither UInt64, Date64, Time32, nanosecond Time64, second or
/// millisecond timestamps, nor a timestamp zone other than UTC.
///
/// # Errors
/// Returns [`WyrdQueueError::Contract`] carrying
/// [`BifrostError::UnsupportedType`] naming the first refused field by its
/// dotted path and its Arrow type.
pub fn check_supported(fields: &[FieldSpec]) -> Result<(), WyrdQueueError> {
    fields
        .iter()
        .try_for_each(|field| check_supported_type(&field.name, &field.data_type))
}

/// Refuse one declaration, recursing through List and Struct children.
///
/// # Errors
/// As [`check_supported`].
fn check_supported_type(path: &str, data_type: &DataTypeSpec) -> Result<(), WyrdQueueError> {
    let supported = match data_type {
        DataTypeSpec::UInt64 | DataTypeSpec::Date64 | DataTypeSpec::Time32 { .. } => false,
        DataTypeSpec::Time64 { unit } => *unit == TimeUnit::Microsecond,
        DataTypeSpec::Timestamp { unit, tz } => {
            matches!(unit, TimeUnit::Microsecond | TimeUnit::Nanosecond)
                && tz
                    .as_deref()
                    .is_none_or(|zone| zone == "UTC" || zone == "+00:00")
        }
        DataTypeSpec::List(element) => {
            return check_supported_type(&format!("{path}.{}", element.name), &element.data_type);
        }
        DataTypeSpec::Struct(children) => {
            return children.iter().try_for_each(|child| {
                check_supported_type(&format!("{path}.{}", child.name), &child.data_type)
            });
        }
        _ => true,
    };
    if supported {
        return Ok(());
    }
    Err(WyrdQueueError::Contract(BifrostError::UnsupportedType {
        field: path.to_owned(),
        data_type: data_type_to_arrow(data_type, false).to_string(),
    }))
}

/// Project one Arrow field onto its wire declaration, metadata included.
///
/// This is the one Arrow-to-wire field mapping: the client schema door, the
/// server's register path, and the catalog's describe projection all call it.
/// Metadata is carried verbatim so a stable `PARQUET:field_id` survives at
/// every nesting depth rather than only on top-level columns. A Variant is
/// recognized by its extension marker and reported as
/// [`DataTypeSpec::Variant`], so the marker's own keys are dropped as type
/// rather than identity metadata.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] when the field, or any nested
/// child, has an Arrow type outside the register-accepted wire set.
pub fn field_to_spec(field: &Field) -> Result<FieldSpec, WyrdQueueError> {
    let variant = is_variant(field);
    Ok(FieldSpec {
        name: field.name().clone(),
        data_type: if variant {
            DataTypeSpec::Variant
        } else {
            dtspec_from_arrow(field.data_type())?
        },
        nullable: field.is_nullable(),
        metadata: field
            .metadata()
            .iter()
            .filter(|(key, _)| !variant || !is_extension_key(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    })
}

/// Project one wire declaration onto its Arrow field.
///
/// This is the one wire-to-Arrow field mapping. Name, nullability, and the
/// exact type — everything that shapes an Arrow buffer — are reproduced at
/// every depth, and a Variant declaration always carries the
/// `arrow.parquet.variant` extension marker, which is its type.
///
/// `carry_metadata` selects whether declared field metadata is kept at every
/// depth. The server's register path keeps it, so a stable `PARQUET:field_id`
/// round-trips through [`field_to_spec`]. A client writer drops it: a
/// `PARQUET:field_id` is the server's own physical identity, which it assigns
/// at registration, re-derives on every stamp, and ignores on an incoming
/// batch, so repeating it would put a value on the wire that looks
/// load-bearing and is not.
#[must_use]
pub fn spec_to_field(spec: &FieldSpec, carry_metadata: bool) -> Field {
    let field = Field::new(
        spec.name.as_str(),
        data_type_to_arrow(&spec.data_type, carry_metadata),
        spec.nullable,
    );
    let field = if carry_metadata {
        field.with_metadata(spec.metadata.clone().into_iter().collect())
    } else {
        field
    };
    match spec.data_type {
        DataTypeSpec::Variant => field.with_extension_type(VariantType),
        _ => field,
    }
}

/// Report whether a metadata key is one of Arrow's extension-type keys.
///
/// Schema identities and wire descriptions express a Variant through its type,
/// so they drop these keys rather than committing the extension spelling.
#[must_use]
pub fn is_extension_key(key: &str) -> bool {
    key == EXTENSION_TYPE_NAME_KEY || key == EXTENSION_TYPE_METADATA_KEY
}

/// Map one Arrow type onto its wire form, recursing through nested fields.
///
/// # Errors
/// Returns [`WyrdQueueError::SchemaParse`] for a type outside the
/// register-accepted wire set.
fn dtspec_from_arrow(dt: &DataType) -> Result<DataTypeSpec, WyrdQueueError> {
    Ok(match dt {
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
        DataType::List(element) => DataTypeSpec::List(Box::new(field_to_spec(element)?)),
        DataType::Struct(fields) => DataTypeSpec::Struct(
            fields
                .iter()
                .map(|f| field_to_spec(f))
                .collect::<Result<_, _>>()?,
        ),
        other => {
            return Err(WyrdQueueError::SchemaParse(format!(
                "Arrow type {other} is not representable on the wire"
            )));
        }
    })
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

/// Map one wire type onto its Arrow form, recursing through nested fields.
///
/// A Variant maps to its unshredded storage struct; the extension marker is
/// stamped on the enclosing field by [`spec_to_field`], which also receives
/// `carry_metadata` for every nested child.
fn data_type_to_arrow(spec: &DataTypeSpec, carry_metadata: bool) -> DataType {
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
        DataTypeSpec::List(element) => {
            DataType::List(Arc::new(spec_to_field(element, carry_metadata)))
        }
        DataTypeSpec::Struct(fields) => DataType::Struct(Fields::from(
            fields
                .iter()
                .map(|field| spec_to_field(field, carry_metadata))
                .collect::<Vec<_>>(),
        )),
        // The unshredded `arrow.parquet.variant` storage; the extension marker
        // is stamped on the enclosing field by `spec_to_field`.
        DataTypeSpec::Variant => variant_storage_type(),
    }
}

#[cfg(test)]
mod schema_tests {
    //! `schema_to_fieldspec` mapping-table proof: JSON-Schema and Arrow → C2 `FieldSpec`.

    use crate::schema::{
        arrow_schema_to_fieldspec, check_supported, field_to_spec, fieldspec_to_arrow,
        json_schema_to_arrow, json_schema_to_fieldspec,
    };
    use crate::{RowPreflight, WyrdQueueError};
    use arrow_schema::{DataType, Field, Fields, Schema, TimeUnit as ArrowTimeUnit};
    use serde_json::json;
    use wyrd_spec::vala::BifrostError;
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

    /// A typed array maps to a List whose items keep the declared
    /// nullability, and row preparation refuses a null item only where the
    /// items are non-null.
    ///
    /// # Panics
    ///
    /// Panics when an item's nullability differs from its declaration or a
    /// null item is admitted into a non-null List.
    #[test]
    fn array_becomes_list() {
        let schema = json!({
            "properties": {
                "tags": {"type": "array", "items": {"type": "string"}},
                "maybe_tags": {"type": "array", "items": {"anyOf": [{"type": "string"}, {"type": "null"}]}},
            }
        });
        let specs = json_schema_to_fieldspec(&schema).expect("maps");

        assert_eq!(
            field(&specs, "tags").data_type,
            DataTypeSpec::List(Box::new(make_field("item", DataTypeSpec::Utf8, false)))
        );
        assert_eq!(
            field(&specs, "maybe_tags").data_type,
            DataTypeSpec::List(Box::new(make_field("item", DataTypeSpec::Utf8, true)))
        );
        let preflight = RowPreflight::new(&fieldspec_to_arrow(&specs).expect("arrow"));
        preflight
            .prepare(&[r#"{"maybe_tags": ["a", null]}"#], None, None)
            .expect("nullable items admit null");
        let refused = preflight
            .prepare(&[r#"{"tags": ["a", null]}"#], None, None)
            .expect_err("non-null items refuse null");
        assert_eq!(refused.code(), "WYRD_VALA_400_SCHEMA_PARSE");
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

    /// Every supported open and nested declaration form maps to its exact column, and every
    /// unsupported form is refused with its exact catalog code.
    ///
    /// # Panics
    ///
    /// Panics when a form maps to another type or nullability, or a refusal
    /// carries another code or detail.
    #[test]
    fn open_nested_and_unsupported_schemas_map_exactly() {
        let variant = DataTypeSpec::Variant;
        let schema = json!({
            "definitions": {"Inner": {"type": "object", "properties": {"k": {"type": "string"}}, "required": ["k"]}},
            "properties": {
                "any": {},
                "always": true,
                "dict_any": {"type": "object", "additionalProperties": true},
                "dict_int": {"type": "object", "additionalProperties": {"type": "integer"}},
                "record": {"type": "object", "propertyNames": {"type": "string"}, "additionalProperties": {}},
                "json_text": {"type": "string", "contentMediaType": "application/json"},
                "union": {"anyOf": [{"type": "integer"}, {"type": "string"}]},
                "one_of": {"oneOf": [{"type": "integer"}, {"type": "boolean"}, {"type": "null"}]},
                "type_list": {"type": ["integer", "string"]},
                "optional_int": {"type": ["integer", "null"]},
                "optional_ref": {"anyOf": [{"$ref": "#/definitions/Inner"}, {"type": "null"}]},
                "wrapped": {"allOf": [{"$ref": "#/definitions/Inner"}]},
                "untyped_list": {"type": "array"},
                "typed_list": {"type": "array", "items": {"type": "integer"}},
                "nullable_items": {"type": "array", "items": {"type": ["integer", "null"]}},
                "closed": {"type": "object", "properties": {"x": {"type": "number"}}, "additionalProperties": false, "required": ["x"]},
            },
            "required": ["any", "always", "dict_any", "union", "type_list", "wrapped", "typed_list", "closed"]
        });
        let specs =
            json_schema_to_fieldspec(&schema).expect("every supported open and nested form maps");
        let inner = DataTypeSpec::Struct(vec![make_field("k", DataTypeSpec::Utf8, false)]);
        let expected = vec![
            ("any", variant.clone(), true),
            ("always", variant.clone(), true),
            ("dict_any", variant.clone(), false),
            ("dict_int", variant.clone(), true),
            ("record", variant.clone(), true),
            ("json_text", variant.clone(), true),
            ("union", variant.clone(), false),
            ("one_of", variant.clone(), true),
            ("type_list", variant.clone(), false),
            ("optional_int", DataTypeSpec::Int64, true),
            ("optional_ref", inner.clone(), true),
            ("wrapped", inner, false),
            ("untyped_list", variant, true),
            (
                "typed_list",
                DataTypeSpec::List(Box::new(make_field("item", DataTypeSpec::Int64, false))),
                false,
            ),
            (
                "nullable_items",
                DataTypeSpec::List(Box::new(make_field("item", DataTypeSpec::Int64, true))),
                true,
            ),
            (
                "closed",
                DataTypeSpec::Struct(vec![make_field("x", DataTypeSpec::Float64, false)]),
                false,
            ),
        ];
        assert_eq!(
            specs
                .iter()
                .map(|f| (f.name.as_str(), f.data_type.clone(), f.nullable))
                .collect::<Vec<_>>(),
            expected
        );

        for open in [
            json!({"properties": {"id": {"type": "integer"}}, "additionalProperties": true}),
            json!({"properties": {"m": {"type": "object", "properties": {"a": {"type": "string"}}, "additionalProperties": {}}}}),
        ] {
            let error = json_schema_to_fieldspec(&open).expect_err("open extras refuse");
            assert_eq!(error.code(), "WYRD_VALA_400_SCHEMA_PARSE");
            assert!(
                error
                    .to_string()
                    .contains("declare a Variant field for open data")
            );
        }
        assert_eq!(
            json_schema_to_fieldspec(&json!({"type": "object"}))
                .expect_err("a free-form root declares no columns")
                .code(),
            "WYRD_VALA_400_SCHEMA_PARSE"
        );

        let ts = |unit, tz: Option<&str>| DataTypeSpec::Timestamp {
            unit,
            tz: tz.map(str::to_owned),
        };
        for data_type in [
            DataTypeSpec::UInt64,
            DataTypeSpec::Date64,
            DataTypeSpec::Time32 {
                unit: TimeUnit::Second,
            },
            DataTypeSpec::Time64 {
                unit: TimeUnit::Nanosecond,
            },
            ts(TimeUnit::Second, None),
            ts(TimeUnit::Millisecond, Some("UTC")),
            ts(TimeUnit::Microsecond, Some("America/New_York")),
        ] {
            let rendered = fieldspec_to_arrow(&[make_field("x", data_type.clone(), true)])
                .expect("projects")
                .field(0)
                .data_type()
                .to_string();
            let nested = vec![make_field(
                "outer",
                DataTypeSpec::Struct(vec![make_field("bad", data_type, true)]),
                true,
            )];
            let Err(WyrdQueueError::Contract(error)) = check_supported(&nested) else {
                panic!("{rendered} must be refused");
            };
            assert_eq!(
                error,
                BifrostError::UnsupportedType {
                    field: "outer.bad".to_owned(),
                    data_type: rendered,
                }
            );
            let arrow = fieldspec_to_arrow(&nested).expect("projects");
            assert_eq!(
                arrow_schema_to_fieldspec(&arrow)
                    .expect_err("the SDK door refuses")
                    .code(),
                "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE"
            );
        }
        check_supported(&[
            make_field("ok_utc", ts(TimeUnit::Microsecond, Some("UTC")), true),
            make_field("ok_ns", ts(TimeUnit::Nanosecond, None), true),
            make_field(
                "ok_time",
                DataTypeSpec::Time64 {
                    unit: TimeUnit::Microsecond,
                },
                true,
            ),
            make_field("ok_variant", DataTypeSpec::Variant, true),
        ])
        .expect("supported types pass");
    }

    #[test]
    fn arrow_schema_maps_precision_types_verbatim() {
        let schema = Schema::new(vec![
            Field::new("small", DataType::Int32, false),
            Field::new("money", DataType::Decimal128(10, 2), true),
            Field::new(
                "local",
                DataType::Timestamp(ArrowTimeUnit::Nanosecond, Some("+00:00".into())),
                true,
            ),
        ]);
        let specs =
            arrow_schema_to_fieldspec(&schema).expect("every precision type is representable");

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
                tz: Some("+00:00".to_owned())
            }
        );
    }

    /// An Arrow type outside the wire set is refused, never coerced, even
    /// when it is nested.
    ///
    /// # Panics
    ///
    /// Panics when an unrepresentable type maps to a declaration.
    #[test]
    fn arrow_schema_refuses_unrepresentable_types() {
        let nested = DataType::Struct(Fields::from(vec![Field::new(
            "half",
            DataType::Float16,
            true,
        )]));
        let schema = Schema::new(vec![Field::new("outer", nested, true)]);
        assert!(matches!(
            arrow_schema_to_fieldspec(&schema),
            Err(WyrdQueueError::SchemaParse(_))
        ));
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

    /// Projects `specs` to Arrow and inverts each field through the pure wire
    /// mapping, so every wire type (stored or not) proves its round trip.
    ///
    /// # Panics
    ///
    /// Panics when a spec does not project or a projected field does not map back.
    fn round_trip(specs: Vec<FieldSpec>) -> Vec<FieldSpec> {
        let schema = fieldspec_to_arrow(&specs).expect("fieldspec_to_arrow");
        schema
            .fields()
            .iter()
            .map(|f| field_to_spec(f).expect("a projected field maps back"))
            .collect()
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
                DataTypeSpec::List(Box::new(make_field("item", DataTypeSpec::Utf8, true))),
                true,
            ),
            make_field(
                "counts",
                DataTypeSpec::List(Box::new(make_field("element", DataTypeSpec::Int64, false))),
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

    /// A recursive declaration keeps every child's shape and drops its ids.
    ///
    /// The list element is the declaration the description carried — its own
    /// name and nullability — not a synthesized nullable `item`, and a struct
    /// child keeps the same, so a client rebuilds the exact buffer layout the
    /// server stores. It does not rebuild the server's `PARQUET:field_id`: that
    /// is physical identity the server assigns and ignores on an incoming
    /// batch, so no depth of this projection puts it on the wire.
    ///
    /// # Panics
    ///
    /// Panics when Arrow loses a nested name or nullability, or when any field
    /// at any depth carries metadata onto the wire.
    #[test]
    fn fieldspec_to_arrow_keeps_recursive_shape_without_field_ids() {
        use std::collections::BTreeMap;
        use wyrd_spec::vala::api::PARQUET_FIELD_ID_KEY;

        let with_id = |mut spec: FieldSpec, id: i32| {
            spec.metadata = BTreeMap::from([(PARQUET_FIELD_ID_KEY.to_owned(), id.to_string())]);
            spec
        };
        let specs = vec![with_id(
            make_field(
                "events",
                DataTypeSpec::List(Box::new(with_id(
                    make_field(
                        "event",
                        DataTypeSpec::Struct(vec![with_id(
                            make_field("name", DataTypeSpec::Utf8, false),
                            19,
                        )]),
                        false,
                    ),
                    17,
                ))),
                false,
            ),
            16,
        )];

        let arrow = fieldspec_to_arrow(&specs).expect("the declaration maps to Arrow");
        let column = arrow.field(0);
        assert!(column.metadata().is_empty(), "a column sends no field id");
        let DataType::List(element) = column.data_type() else {
            panic!("a list declaration must project an Arrow list");
        };
        assert_eq!(element.name(), "event");
        assert!(!element.is_nullable());
        assert!(
            element.metadata().is_empty(),
            "a list element sends no field id"
        );
        let DataType::Struct(children) = element.data_type() else {
            panic!("the list element must project its declared struct");
        };
        assert_eq!(children[0].name(), "name");
        assert!(
            children[0].metadata().is_empty(),
            "a struct child sends no field id"
        );

        let round_tripped =
            arrow_schema_to_fieldspec(&arrow).expect("a projected schema maps back");
        assert_eq!(
            round_tripped,
            specs.iter().map(strip_ids).collect::<Vec<_>>(),
            "reading the projection back yields the same shape with no ids"
        );
    }

    /// Clears one declaration's metadata at every depth for comparison.
    fn strip_ids(spec: &FieldSpec) -> FieldSpec {
        use std::collections::BTreeMap;

        FieldSpec {
            name: spec.name.clone(),
            data_type: match &spec.data_type {
                DataTypeSpec::List(element) => DataTypeSpec::List(Box::new(strip_ids(element))),
                DataTypeSpec::Struct(children) => {
                    DataTypeSpec::Struct(children.iter().map(strip_ids).collect())
                }
                other => other.clone(),
            },
            nullable: spec.nullable,
            metadata: BTreeMap::new(),
        }
    }
}
