//! Namespace resolution for the Bifrost register path.
//!
//! Wire-to-Arrow field conversion is owned by `wyrd_types::spec_to_field`, and
//! the schema fingerprint by the Redux catalog that stores it.

use vala_bifrost_redux::namespaces::BifrostNamespace;
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use arrow::datatypes::DataType;
    use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec, PARQUET_FIELD_ID_KEY};
    use wyrd_types::{field_to_spec, spec_to_field};

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
