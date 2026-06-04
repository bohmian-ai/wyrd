//! Typed projection of the OpenTelemetry `AnyValue` type.
//!
//! The storage shape on trace records remains `serde_json::Map<String,
//! serde_json::Value>`. Producers can construct attributes with this enum to
//! preserve OTel int, double, string, bytes, array, and key-value-list intent
//! before converting to the loose JSON storage boundary.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::WyrdError;

/// Typed attribute value mirroring OTel `AnyValue`.
///
/// The JSON wire form is untagged and matches the corresponding
/// [`serde_json::Value`] shape. `Bytes` serializes as a base64 string, but a
/// JSON string deserializes as [`AttributeValue::String`] because the loose
/// storage form has no native byte type.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// OTel `AnyValue.string_value`.
    String(String),
    /// OTel `AnyValue.bool_value`.
    Bool(bool),
    /// OTel `AnyValue.int_value`.
    Int(i64),
    /// OTel `AnyValue.double_value`.
    Double(f64),
    /// OTel `AnyValue.bytes_value`, encoded as base64 in JSON.
    Bytes(Vec<u8>),
    /// OTel `AnyValue.array_value`.
    Array(Vec<AttributeValue>),
    /// OTel `AnyValue.kvlist_value`.
    KvList(BTreeMap<String, AttributeValue>),
}

impl Serialize for AttributeValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AttributeValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.is_null() {
            return Err(serde::de::Error::custom(
                "attribute_value cannot be null; omit the key instead",
            ));
        }
        Ok(Self::from_json(&value))
    }
}

impl schemars::JsonSchema for AttributeValue {
    fn schema_name() -> String {
        "AttributeValue".to_string()
    }

    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        <serde_json::Value as schemars::JsonSchema>::json_schema(generator)
    }
}

impl AttributeValue {
    /// Convert to a loose JSON value for storage on trace record attribute
    /// maps.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::String(value) => serde_json::Value::String(value.clone()),
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Int(value) => serde_json::Value::Number((*value).into()),
            Self::Double(value) => serde_json::Number::from_f64(*value)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Self::Bytes(value) => serde_json::Value::String(base64_encode(value)),
            Self::Array(values) => {
                serde_json::Value::Array(values.iter().map(Self::to_json).collect())
            }
            Self::KvList(values) => serde_json::Value::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_json()))
                    .collect(),
            ),
        }
    }

    /// Build from a loose JSON value.
    ///
    /// This is not an inverse for [`AttributeValue::Bytes`]: a JSON string
    /// always becomes [`AttributeValue::String`].
    pub fn from_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::String(String::new()),
            serde_json::Value::Bool(value) => Self::Bool(*value),
            serde_json::Value::Number(value) => {
                if let Some(int_value) = value.as_i64() {
                    Self::Int(int_value)
                } else if let Some(double_value) = value.as_f64() {
                    Self::Double(double_value)
                } else {
                    Self::String(value.to_string())
                }
            }
            serde_json::Value::String(value) => Self::String(value.clone()),
            serde_json::Value::Array(values) => {
                Self::Array(values.iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(values) => {
                let mut map = BTreeMap::new();
                for (key, value) in values {
                    map.insert(key.clone(), Self::from_json(value));
                }
                Self::KvList(map)
            }
        }
    }

    /// Validate recursive attribute values.
    ///
    /// Returns [`WyrdError::Validation`] when any double value is NaN or
    /// infinite.
    pub fn validate(&self) -> Result<(), WyrdError> {
        match self {
            Self::Double(value) if !value.is_finite() => Err(WyrdError::Validation {
                message: format!("attribute_value double must be finite, got {value}"),
                details: serde_json::Value::Null,
            }),
            Self::Array(values) => {
                for value in values {
                    value.validate()?;
                }
                Ok(())
            }
            Self::KvList(values) => {
                for value in values.values() {
                    value.validate()?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
