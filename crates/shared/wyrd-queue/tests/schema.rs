//! `schema_to_fieldspec` mapping-table proof: JSON-Schema and Arrow → C2 `FieldSpec`.

use arrow_schema::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
use serde_json::json;
use wyrd_queue::schema::{arrow_schema_to_fieldspec, json_schema_to_fieldspec};
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
