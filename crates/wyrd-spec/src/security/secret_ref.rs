//! `SecretRef` typed pointer into the runtime secret store.
//!
//! `SecretRef` decouples serialized configuration from credential values.
//! Resolution belongs to the Wyrd shared auth shell at use-site.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Pointer into the Wyrd runtime secret store.
///
/// Production sources point at environment variables, mounted files, or an
/// external secret manager. Test-only helpers are excluded from production
/// schemas.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretRef {
    /// Environment variable name.
    Env {
        /// Environment variable name. Must be non-empty when resolved.
        name: String,
    },
    /// Local filesystem path to a file containing the secret value.
    File {
        /// File path. Must be non-empty when resolved.
        path: String,
    },
    /// Backend-agnostic key in an external secret manager.
    Vault {
        /// Secret-manager key path. Interpretation is backend-specific.
        key: String,
    },
    /// Plaintext secret value. Testing only.
    #[cfg(any(test, feature = "test-utils"))]
    Inline {
        /// Plaintext secret value wrapped in a redacting newtype.
        value: InlineSecret,
    },
}

/// Test-only secret newtype with redacted `Debug` and opaque JSON Schema.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone)]
pub struct InlineSecret(String);

#[cfg(any(test, feature = "test-utils"))]
impl InlineSecret {
    /// Wrap a plaintext string.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Expose the plaintext value for test and auth-resolver gates.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl std::fmt::Debug for InlineSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InlineSecret(<redacted>)")
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl serde::Serialize for InlineSecret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<'de> serde::Deserialize<'de> for InlineSecret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self(value))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl schemars::JsonSchema for InlineSecret {
    fn schema_name() -> String {
        "InlineSecret".to_string()
    }

    fn json_schema(schema_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(schema_gen)
    }
}

impl PartialEq for SecretRef {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Env { name: a }, Self::Env { name: b }) => a == b,
            (Self::File { path: a }, Self::File { path: b }) => a == b,
            (Self::Vault { key: a }, Self::Vault { key: b }) => a == b,
            #[cfg(any(test, feature = "test-utils"))]
            (Self::Inline { value: a }, Self::Inline { value: b }) => {
                format!("{a:?}") == format!("{b:?}")
            }
            _ => false,
        }
    }
}

impl Eq for SecretRef {}

impl Hash for SecretRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Env { name } => name.hash(state),
            Self::File { path } => path.hash(state),
            Self::Vault { key } => key.hash(state),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Inline { .. } => {}
        }
    }
}
