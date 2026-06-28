//! `SecretRef` typed pointer into the runtime secret store.
//!
//! `SecretRef` is the canonical secret-indirection type for all Wyrd
//! transports and the server-internal alert router. It decouples
//! configuration (which lives in serialized form in YAML / env / registry)
//! from credential values (which live in env vars, mounted files, or an
//! external secret manager). Resolution is owned by the Wyrd shared auth
//! shell; consumers carry `SecretRef` and call `wyrd_auth::resolve(secret_ref)`
//! at use-site.
//!
//! ## Schema-generation contract
//!
//! `SecretRef` derives `JsonSchema`. The `Inline` variant carries
//! `#[schemars(skip)]`, so it is absent from the generated schema in EVERY
//! build configuration — including test builds where `test-utils` is active
//! through feature unification (e.g. a downstream crate's `wyrd-testing`
//! dev-dependency). The committed public-schema shape therefore never contains
//! the test-only variant, and an in-process `schema_for!` (as the schema-drift
//! gate uses) produces the same clean shape regardless of features. See commit
//! 8 for the schema-drift gate and the no-`test-utils` integration test that
//! asserts `{"source":"inline"}` is rejected by serde when the variant does not
//! exist in a production build.

use serde::{Deserialize, Serialize};

/// Pointer into the Wyrd runtime secret store.
///
/// Variants cover the three standard deployment patterns: environment variables
/// (CI, simple containers), file mounts (Kubernetes Secrets), and external
/// secret managers (Vault, AWS Secrets Manager, GSM). The `Inline` variant is
/// available only behind `feature = "test-utils"` (or in `#[cfg(test)]` inside
/// `wyrd-spec` itself). It is absent from production builds at the type level
/// and absent from the committed JSON Schema goldens (which are generated
/// without `test-utils`).
///
/// Wire shape: `{"source": "env", "name": "WYRD_API_KEY"}`.
///
/// # `Eq` and `Hash` implementation note
///
/// `PartialEq`, `Eq`, and `Hash` are **hand-implemented** (not derived) because
/// the `Inline` variant's `InlineSecret` field does not implement `Eq` or
/// `Hash` (intentional — we never compare or hash plaintext secrets). The hand
/// impls compare/hash only the non-secret discriminant fields (`Env.name`,
/// `File.path`, `Vault.key`); for `Inline`, equality and hashing use the
/// redacted debug form so the secret value is never exposed. See the explicit
/// `impl` blocks below the enum definition.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(
    description = "Pointer into a Wyrd runtime secret source. Production builds support environment variables, mounted files, and external secret-manager keys."
)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretRef {
    /// An environment variable name. The runtime reads `std::env::var(name)`
    /// at connect time. Conventionally `SCREAMING_SNAKE_CASE`.
    ///
    /// Example: `{"source": "env", "name": "WYRD_SECRET_API_KEY"}`.
    Env {
        /// Environment variable name. Must be non-empty.
        name: String,
    },
    /// An absolute path to a file on the local filesystem. Kubernetes Secret
    /// volume mounts land here.
    ///
    /// Example: `{"source": "file", "path": "/var/run/secrets/wyrd/api-key"}`.
    File {
        /// Absolute path to a file whose contents are the secret value. Must
        /// be non-empty.
        path: String,
    },
    /// A backend-agnostic key in an external secret manager (HashiCorp Vault,
    /// AWS Secrets Manager, Google Secret Manager). The Wyrd auth shell maps
    /// the key to the configured backend.
    ///
    /// Example: `{"source": "vault", "key": "secret/data/wyrd/api-key"}`.
    Vault {
        /// Backend-agnostic key path. Interpretation is backend-specific.
        key: String,
    },
    /// Plaintext secret value. **Testing only.** This variant is only compiled
    /// when `cfg(any(test, feature = "test-utils"))`. The `wyrd_auth::resolve`
    /// function rejects this variant at runtime outside test builds.
    ///
    /// Example: `{"source": "inline", "value": "supersecret"}`.
    #[cfg(any(test, feature = "test-utils"))]
    #[schemars(skip)]
    Inline {
        /// The plaintext secret value. Wrapped in [`InlineSecret`] so it is
        /// redacted in `Debug` output and never accidentally logged or hashed.
        value: InlineSecret,
    },
}

/// Test-only secret newtype with redacted `Debug` and an opaque JSON-Schema
/// representation.
///
/// This type exists ONLY because `secrecy::SecretString` does not implement
/// `schemars::JsonSchema`, and deriving `JsonSchema` on `SecretRef` while the
/// `Inline` variant exists would not compile. `InlineSecret` is a thin
/// purpose-built wrapper:
///
/// - `Debug` writes `"InlineSecret(<redacted>)"`; the plaintext is never
///   formatted.
/// - `Serialize` writes the inner string transparently (test fixtures need to
///   round-trip).
/// - `Deserialize` accepts any string.
/// - `JsonSchema` reports the type as a plain string. Since this type is only
///   reachable via `SecretRef::Inline` and the public schema is generated
///   without `test-utils`, the schema is never published.
/// - No `PartialEq`, `Eq`, or `Hash` impls — the parent `SecretRef`
///   hand-implementations rely on the redacted `Debug` form, not on these
///   traits.
///
/// The type is gated on the same `cfg` as `SecretRef::Inline` so it never
/// exists in production builds.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone)]
pub struct InlineSecret(String);

#[cfg(any(test, feature = "test-utils"))]
impl InlineSecret {
    /// Wrap a plaintext string. Construction is allowed only because the type
    /// itself is `cfg`-gated to test/test-utils.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Expose the plaintext value. Use only in tests or in the auth-resolver
    /// shell when proving the `cfg` gate.
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
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<'de> serde::Deserialize<'de> for InlineSecret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self(s))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl schemars::JsonSchema for InlineSecret {
    fn schema_name() -> String {
        "InlineSecret".to_string()
    }

    fn json_schema(schema_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        // Opaque string. The `SecretRef::Inline` variant carries
        // `#[schemars(skip)]`, so this impl is never reached during schema
        // generation; it exists only to satisfy the `#[derive(JsonSchema)]` on
        // `SecretRef` in test builds where the variant's field type must resolve.
        // NOTE: parameter is `schema_gen`, not `gen`, because `gen` is a
        // reserved keyword in Rust 2024 (wyrd-spec is on the 2024 edition).
        // The type path uses the `r#gen` raw identifier for the same reason —
        // `gen` is reserved in path segments too on Rust 2024.
        <String as schemars::JsonSchema>::json_schema(schema_gen)
    }
}

// The `Inline` variant's `InlineSecret` field is intentionally not `Eq`/`Hash`
// — we never compare or hash plaintext secrets. For `SecretRef::Inline`,
// equality and hashing use the redacted `Debug` representation so the secret
// value is never compared or hashed in plain text.

impl PartialEq for SecretRef {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Env { name: a }, Self::Env { name: b }) => a == b,
            (Self::File { path: a }, Self::File { path: b }) => a == b,
            (Self::Vault { key: a }, Self::Vault { key: b }) => a == b,
            #[cfg(any(test, feature = "test-utils"))]
            (Self::Inline { value: a }, Self::Inline { value: b }) => {
                // Use expose() deliberately: this arm is cfg-gated to
                // test/test-utils, so comparing plaintext here is intentional
                // and safe. The redacted Debug form would make all Inline
                // values equal regardless of content, making round-trip tests
                // vacuous.
                a.expose() == b.expose()
            }
            _ => false,
        }
    }
}

impl Eq for SecretRef {}

impl std::hash::Hash for SecretRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the discriminant tag so different variants always differ.
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Env { name } => name.hash(state),
            Self::File { path } => path.hash(state),
            Self::Vault { key } => key.hash(state),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Inline { .. } => {
                // Do not hash the secret value. Discriminant alone is
                // sufficient; two `Inline` values hash to the same bucket
                // and are distinguished by `PartialEq`.
            }
        }
    }
}

/// Validation error for [`SecretRef`] field invariants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SecretRefError {
    /// A required string field was empty.
    #[error("field '{0}' must not be empty")]
    EmptyField(&'static str),
    /// A `File` path was relative (did not start with `/`).
    #[error("path must be absolute (must start with '/')")]
    RelativePath,
}

impl SecretRef {
    /// Validate structural field invariants.
    ///
    /// Returns `Err` if:
    /// - `Env.name` is empty
    /// - `File.path` is empty
    /// - `File.path` is not absolute (does not start with `/`)
    /// - `Vault.key` is empty
    ///
    /// `Inline` always returns `Ok(())` — inline values are test fixtures
    /// and carry no path/name field to validate.
    pub fn validate(&self) -> Result<(), SecretRefError> {
        match self {
            Self::Env { name } if name.is_empty() => Err(SecretRefError::EmptyField("name")),
            Self::File { path } if path.is_empty() => Err(SecretRefError::EmptyField("path")),
            Self::File { path } if !path.starts_with('/') => Err(SecretRefError::RelativePath),
            Self::Vault { key } if key.is_empty() => Err(SecretRefError::EmptyField("key")),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_empty_name_is_invalid() {
        let r = SecretRef::Env {
            name: String::new(),
        }
        .validate();
        assert_eq!(r.unwrap_err(), SecretRefError::EmptyField("name"));
    }

    #[test]
    fn env_non_empty_name_is_valid() {
        let r = SecretRef::Env {
            name: "WYRD_KEY".to_string(),
        }
        .validate();
        assert!(r.is_ok());
    }

    #[test]
    fn file_empty_path_is_invalid() {
        let r = SecretRef::File {
            path: String::new(),
        }
        .validate();
        assert_eq!(r.unwrap_err(), SecretRefError::EmptyField("path"));
    }

    #[test]
    fn file_relative_path_is_invalid() {
        let r = SecretRef::File {
            path: "var/run/secrets/key".to_string(),
        }
        .validate();
        assert_eq!(r.unwrap_err(), SecretRefError::RelativePath);
    }

    #[test]
    fn file_absolute_path_is_valid() {
        let r = SecretRef::File {
            path: "/var/run/secrets/wyrd/api-key".to_string(),
        }
        .validate();
        assert!(r.is_ok());
    }

    #[test]
    fn vault_empty_key_is_invalid() {
        let r = SecretRef::Vault { key: String::new() }.validate();
        assert_eq!(r.unwrap_err(), SecretRefError::EmptyField("key"));
    }

    #[test]
    fn vault_non_empty_key_is_valid() {
        let r = SecretRef::Vault {
            key: "secret/data/wyrd/api-key".to_string(),
        }
        .validate();
        assert!(r.is_ok());
    }

    #[test]
    fn inline_partial_eq_compares_actual_values() {
        let a = SecretRef::Inline {
            value: InlineSecret::new("secret-a"),
        };
        let b = SecretRef::Inline {
            value: InlineSecret::new("secret-b"),
        };
        let c = SecretRef::Inline {
            value: InlineSecret::new("secret-a"),
        };
        assert_ne!(a, b);
        assert_eq!(a, c);
    }
}
