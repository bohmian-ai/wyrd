//! JSON schema helpers.

/// Generate a root schema for a type.
#[must_use]
pub fn schema_for<T: schemars::JsonSchema>() -> schemars::Schema {
    schemars::schema_for!(T)
}

/// Schema extension naming the value an omitted-when-`None` field carries
/// whenever serde serializes it.
pub const SERIALIZED_EXTENSION: &str = "x-wyrd-serialized";

/// Schema for an `Option<T>` field serde skips when `None`.
///
/// Use it as `#[schemars(schema_with = "crate::schema::skipped_option::<T>")]`
/// beside `#[serde(default, skip_serializing_if = "Option::is_none")]`. The
/// schema itself stays the ordinary `Option<T>` schema, so deserialization still
/// accepts an omitted or null value. [`SERIALIZED_EXTENSION`] additionally
/// carries `T`'s schema: the field is absent from serialized output when `None`
/// and holds a `T` otherwise, never null unless `T` itself admits null. Output
/// projections such as the TypeScript wire-type generator read the extension
/// instead of the input schema.
pub fn skipped_option<T: schemars::JsonSchema>(
    generator: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    let serialized = generator.subschema_for::<T>().to_value();
    let mut schema = generator.subschema_for::<Option<T>>();
    schema
        .ensure_object()
        .insert(SERIALIZED_EXTENSION.to_owned(), serialized);
    schema
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    /// Fixture whose fields serde omits when `None`, one scalar and one opaque.
    #[derive(Serialize, Deserialize, schemars::JsonSchema)]
    struct Skipped {
        /// Omitted when `None`; a string otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(schema_with = "super::skipped_option::<String>")]
        scalar: Option<String>,
        /// Omitted when `None`; any JSON value, null included, otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(schema_with = "super::skipped_option::<serde_json::Value>")]
        opaque: Option<serde_json::Value>,
    }

    /// The input schema keeps accepting omitted and null values while the
    /// extension carries the serialized value, which stays any-JSON for an
    /// opaque field.
    #[test]
    fn skipped_option_keeps_input_schema_and_records_serialized_value() {
        let schema = serde_json::to_value(super::schema_for::<Skipped>()).expect("schema JSON");
        assert_eq!(schema.get("required"), None);
        let scalar = &schema["properties"]["scalar"];
        assert_eq!(scalar["type"], json!(["string", "null"]));
        assert_eq!(
            scalar[super::SERIALIZED_EXTENSION],
            json!({"type": "string"})
        );
        assert_eq!(
            schema["properties"]["opaque"][super::SERIALIZED_EXTENSION],
            json!(true)
        );
    }
}
