//! `schema_to_fieldspec` mapping-table proof: JSON-Schema and Arrow → C2 `FieldSpec`.

use arrow_schema::{DataType, Field, Schema, TimeUnit as ArrowTimeUnit};
use serde_json::json;
use wyrd_queue::schema::{
    arrow_schema_to_fieldspec, fieldspec_to_arrow, json_schema_to_arrow, json_schema_to_fieldspec,
};
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
            make_field(
                "ts_naive",
                DataTypeSpec::Timestamp { unit, tz: None },
                true,
            ),
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
    let specs = vec![make_field(
        "point",
        DataTypeSpec::Struct(inner),
        true,
    )];
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
    let via_compose =
        fieldspec_to_arrow(&json_schema_to_fieldspec(&schema).expect("parse")).expect("compose");
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
