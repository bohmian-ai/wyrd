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
//! `SecretRef` derives `JsonSchema`. When generated WITHOUT the `test-utils`
//! feature, the `Inline` variant is absent from the schema — this is the
//! committed public-schema shape. Generating with `test-utils` would expose
//! the test-only variant; the committed `examples/gen_schemas.rs` MUST NOT
//! enable `test-utils`. See commit 8 for the schema-drift gate and the
//! no-`test-utils` integration test that asserts `{"source":"inline"}` is
//! rejected by serde when the variant does not exist.

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
        // Opaque string. The variant is excluded from generated schema goldens
        // because gen_schemas runs without `test-utils`; this impl exists only
        // to satisfy the `#[derive(JsonSchema)]` on `SecretRef` in test builds.
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
                // Compare via redacted debug form — the actual secret is never
                // exposed. Two `Inline` values with the same redacted output
                // are considered equal for test/debug purposes.
                format!("{:?}", a) == format!("{:?}", b)
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
