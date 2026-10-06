//! Namespace resolution and schema fingerprinting for the Bifrost register
//! path.
//!
//! Wire-to-Arrow field conversion is owned by `wyrd_queue::spec_to_field`; the
//! register handler uses it to build Redux catalog fields and derives the
//! server-authoritative schema fingerprint here the same way the catalog does
//! (user-fields-only, over the Arrow schema).

use arrow::datatypes::{Field, Schema};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::SchemaFingerprint;
use wyrd_spec::error::WyrdError;

/// Resolve a wire namespace string (e.g. `"vala.bifrost"`) to its engine enum.
///
/// Unknown namespaces are a client error rather than a silent default.
pub fn namespace_from_wire(namespace: &str) -> Result<BifrostNamespace, WyrdError> {
    BifrostNamespace::from_wire(namespace).ok_or_else(|| WyrdError::Validation {
        message: format!("unknown Bifrost namespace: {namespace}"),
        details: serde_json::json!({ "namespace": namespace }),
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

    use arrow::datatypes::DataType;
    use wyrd_queue::{field_to_spec, spec_to_field};
    use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec, PARQUET_FIELD_ID_KEY, TimeUnit};

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
        let arrow_fields: Vec<Field> = specs.iter().map(|spec| spec_to_field(spec, true)).collect();
        let forward_fp = fingerprint_hex(&arrow_fields);

        // arrow → spec → arrow must reproduce the exact same fingerprint.
        let round_tripped: Vec<Field> = arrow_fields
            .iter()
            .map(|f| field_to_spec(f).expect("arrow field maps back to spec"))
            .map(|s| spec_to_field(&s, true))
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
        let id =
            |value: i32| BTreeMap::from([(PARQUET_FIELD_ID_KEY.to_owned(), value.to_string())]);
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

        let arrow = spec_to_field(&spec, true);
        assert_eq!(
            arrow.metadata().get(PARQUET_FIELD_ID_KEY),
            Some(&"16".to_owned())
        );
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
            field_to_spec(&arrow).expect("the Arrow field maps back to its declaration"),
            spec,
            "every nested name, nullability, and field id survives both directions"
        );
    }
}
