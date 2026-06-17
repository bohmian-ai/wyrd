//! Secret-bearing wire-field wrapper.

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::Schema;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Opaque secret-bearing API field.
///
/// The JSON wire shape is a string, while schema metadata marks the field as a
/// write-only password and `Debug` redacts the payload.
#[derive(Clone)]
pub struct SecretBearer(SecretString);

impl SecretBearer {
    /// Wrap a raw secret string for API transport.
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(SecretString::from(raw))
    }

    /// Expose the raw secret at an audited call site.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Convert to the internal secret type.
    #[must_use]
    pub fn into_secret_string(self) -> SecretString {
        self.0
    }
}

impl std::fmt::Debug for SecretBearer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SecretBearer").field(&"[REDACTED]").finish()
    }
}

impl PartialEq for SecretBearer {
    fn eq(&self, other: &Self) -> bool {
        self.expose() == other.expose()
    }
}

impl Eq for SecretBearer {}

impl Serialize for SecretBearer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.expose_secret())
    }
}

impl<'de> Deserialize<'de> for SecretBearer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl JsonSchema for SecretBearer {
    fn schema_name() -> String {
        "SecretBearer".to_owned()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        serde_json::from_value(serde_json::json!({
            "type": "string",
            "format": "password",
            "writeOnly": true,
            "description": "Opaque secret. Redacted in logs and audit payloads."
        }))
        .expect("SecretBearer schema is valid JSON Schema")
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for SecretBearer {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, Schema, SchemaFormat, Type};

        utoipa::openapi::RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .format(Some(SchemaFormat::Custom("password".to_owned())))
                .description(Some("Opaque secret. Redacted in logs and audit payloads."))
                .write_only(Some(true))
                .build(),
        ))
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for SecretBearer {}

#[cfg(test)]
mod tests {
    use super::SecretBearer;

    #[test]
    fn redacted_debug_does_not_leak() {
        let bearer = SecretBearer::new("hunter2".to_owned());

        let debug = format!("{bearer:?}");

        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("hunter2"));
    }

    #[test]
    fn serialize_emits_raw_string() {
        let json = serde_json::to_string(&SecretBearer::new("x".to_owned())).expect("serializes");

        assert_eq!(json, "\"x\"");
    }

    #[test]
    fn deserialize_round_trips() {
        let bearer: SecretBearer = serde_json::from_str("\"secret\"").expect("deserializes");

        assert_eq!(bearer.expose(), "secret");
    }

    #[test]
    fn json_schema_is_opaque_string() {
        let schema = schemars::schema_for!(SecretBearer);
        let value = serde_json::to_value(schema.schema).expect("schema serializes");

        assert_eq!(value["type"], "string");
        assert_eq!(value["format"], "password");
        assert_eq!(value["writeOnly"], true);
    }

    #[cfg(feature = "server")]
    #[test]
    fn to_schema_is_opaque_string() {
        fn assert_to_schema<T: utoipa::ToSchema>() {}

        assert_to_schema::<SecretBearer>();
        let schema = <SecretBearer as utoipa::PartialSchema>::schema();
        let value = serde_json::to_value(schema).expect("schema serializes");

        assert!(
            value.to_string().contains("password"),
            "schema must remain password formatted: {value}"
        );
    }
}
