//! Provider credential write and redacted view contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use super::GatewayContractError;
use crate::auth::SecretBearer;
use crate::ids::{CredentialBindingName, ProviderCredentialName, ProviderId, SecretBackendName};

/// Vault KV v2 reference of the form `<path>#<key>`.
///
/// `<path>` is one or more `/`-separated segments inside the operator's KV v2
/// mount; `<key>` names a member of the secret's `data` object. Segments are
/// non-empty, never `.` or `..`, and contain no `?`, `#`, `%`, or `\`. The
/// whole reference is at most 1024 bytes without control characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct ExternalSecretReference(String);

impl ExternalSecretReference {
    /// Builds a validated `<path>#<key>` reference, splitting at the first `#`.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `source.reference` when the
    /// value is longer than 1024 bytes, contains a control character, lacks a
    /// `#`, has an empty key, or has a path failing [`Self::is_valid_path`].
    pub fn new(value: &str) -> Result<Self, GatewayContractError> {
        let valid = value.len() <= 1024
            && !value.chars().any(char::is_control)
            && value
                .split_once('#')
                .is_some_and(|(path, key)| Self::is_valid_path(path) && !key.is_empty());
        if !valid {
            return Err(GatewayContractError::new(
                "source.reference",
                "must be `<path>#<key>` within 1024 bytes: non-empty path segments other than `.` or `..` without `?`, `#`, `%`, or `\\`, a non-empty key, and no control characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Whether `path` satisfies the Vault path grammar: one or more non-empty
    /// `/`-separated segments other than `.` or `..`, without `?`, `#`, `%`,
    /// or `\`. Operator mounts and assigned prefixes use the same grammar.
    #[must_use]
    pub fn is_valid_path(path: &str) -> bool {
        !path.contains(['?', '#', '%', '\\'])
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
    }

    /// Borrows the full reference.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrows the secret path before the first `#`.
    #[must_use]
    pub fn path(&self) -> &str {
        self.0.split_once('#').map_or(&self.0, |(path, _)| path)
    }

    /// Borrows the `data` key after the first `#`.
    #[must_use]
    pub fn key(&self) -> &str {
        self.0.split_once('#').map_or("", |(_, key)| key)
    }
}

impl<'de> Deserialize<'de> for ExternalSecretReference {
    /// Decodes a string through [`ExternalSecretReference::new`], so a decoded
    /// document can never carry an unvalidated reference.
    ///
    /// # Errors
    ///
    /// Returns the deserializer's custom error when the value is not a string or
    /// fails reference validation.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

/// Where a provider credential's value comes from, as written by an admin.
///
/// `Environment` and `ExternalSecret` name operator-owned authority the server
/// resolves at runtime. `ManagedSecret` is the one source carrying a value:
/// it is write-only, crosses the bounded administration ingress once, and is
/// protected under the tenant's keyring before persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderCredentialWriteSource {
    /// Operator-configured environment variable or mounted-file binding.
    Environment {
        /// Binding name the operator configured on the server.
        binding: CredentialBindingName,
    },
    /// Reference resolved from an operator-configured secret backend.
    ExternalSecret {
        /// Configured backend identity.
        backend: SecretBackendName,
        /// Opaque backend reference.
        reference: ExternalSecretReference,
    },
    /// Provider key the tenant administrator submits for Wyrd to protect.
    ///
    /// Write-only: the redacted view is the unit
    /// [`ProviderCredentialSourceView::ManagedSecret`], so no read, listing,
    /// error, log, or audit record can disclose the value.
    ManagedSecret {
        /// Submitted provider key, redacted in `Debug` and write-only in every
        /// generated schema.
        secret: SecretBearer,
    },
}

/// `PUT /v1/admin/gateway/provider-credentials/{name}` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ProviderCredentialWrite {
    /// Tenant-unique credential name; must match the path name.
    pub name: ProviderCredentialName,
    /// Provider the credential authenticates to.
    pub provider: ProviderId,
    /// Credential source.
    pub source: ProviderCredentialWriteSource,
}

/// Lifecycle state of a provider credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialState {
    /// Usable by newly admitted calls.
    Active,
    /// Terminally revoked; cannot be replaced or reactivated.
    Revoked,
}

/// Redacted credential source.
///
/// Discloses no plaintext, ciphertext, digest, fingerprint, prefix, or suffix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderCredentialSourceView {
    /// Operator-configured binding.
    Environment {
        /// Binding name.
        binding: CredentialBindingName,
    },
    /// External secret backend reference.
    ExternalSecret {
        /// Configured backend identity.
        backend: SecretBackendName,
        /// Opaque backend reference.
        reference: ExternalSecretReference,
    },
    /// Wyrd-protected tenant secret; the view carries no member at all.
    ManagedSecret,
}

impl From<&ProviderCredentialWriteSource> for ProviderCredentialSourceView {
    /// Projects a write source to its redacted view.
    fn from(source: &ProviderCredentialWriteSource) -> Self {
        match source {
            ProviderCredentialWriteSource::Environment { binding } => Self::Environment {
                binding: binding.clone(),
            },
            ProviderCredentialWriteSource::ExternalSecret { backend, reference } => {
                Self::ExternalSecret {
                    backend: backend.clone(),
                    reference: reference.clone(),
                }
            }
            ProviderCredentialWriteSource::ManagedSecret { .. } => Self::ManagedSecret,
        }
    }
}

/// Redacted read view of one provider credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ProviderCredentialView {
    /// Tenant-unique credential name.
    pub name: ProviderCredentialName,
    /// Provider the credential authenticates to.
    pub provider: ProviderId,
    /// Redacted source.
    pub source: ProviderCredentialSourceView,
    /// Lifecycle state.
    pub state: ProviderCredentialState,
    /// First creation time.
    pub created_at: DateTime<Utc>,
    /// Last mutation time.
    pub updated_at: DateTime<Utc>,
    /// Last active replacement time, if ever rotated.
    pub rotated_at: Option<DateTime<Utc>>,
    /// Revocation time, if revoked.
    pub revoked_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::{
        ExternalSecretReference, ProviderCredentialSourceView, ProviderCredentialWrite,
        ProviderCredentialWriteSource,
    };
    use serde_json::json;

    /// Proves each operator-owned source decodes and projects to an identical
    /// redacted view.
    #[test]
    fn sources_decode_and_project_to_views() {
        for source in [
            json!({"environment": {"binding": "openai-prod"}}),
            json!({"external_secret": {"backend": "vault-main", "reference": "kv/data/openai#key"}}),
        ] {
            let write: ProviderCredentialWrite = serde_json::from_value(json!({
                "name": "openai-key",
                "provider": "openai",
                "source": source,
            }))
            .expect("credential write decodes");
            let view = ProviderCredentialSourceView::from(&write.source);
            assert_eq!(serde_json::to_value(&view).expect("view encodes"), source);
        }
    }

    /// Proves a submitted managed secret decodes, keeps its value only behind
    /// the write-only wrapper, projects to the unit view, and never renders in
    /// `Debug`.
    ///
    /// # Panics
    ///
    /// Panics when the write does not decode, when the source decodes as
    /// another variant, when the wrapper does not expose the submitted value,
    /// or when the `Debug` rendering or the projected view discloses it.
    #[test]
    fn managed_secret_is_write_only_and_projects_to_a_unit_view() {
        let write: ProviderCredentialWrite = serde_json::from_value(json!({
            "name": "openai-key",
            "provider": "openai",
            "source": {"managed_secret": {"secret": "sk-live-SECRET"}},
        }))
        .expect("managed credential write decodes");
        let ProviderCredentialWriteSource::ManagedSecret { secret } = &write.source else {
            panic!("managed secret source decoded as another variant");
        };
        assert_eq!(secret.expose(), "sk-live-SECRET");
        assert!(
            !format!("{write:?}").contains("sk-live-SECRET"),
            "debug output discloses the submitted secret"
        );
        let view = ProviderCredentialSourceView::from(&write.source);
        let encoded = serde_json::to_value(&view).expect("view encodes");
        assert_eq!(encoded, json!("managed_secret"));
        assert!(
            !encoded.to_string().contains("sk-live-SECRET"),
            "redacted view discloses the submitted secret"
        );
    }

    /// Proves closed sources reject unknown variants, raw paths, and extra fields.
    ///
    /// # Panics
    ///
    /// Panics when any listed invalid source decodes.
    #[test]
    fn invalid_sources_are_rejected() {
        for source in [
            json!({"kms": {"key": "x"}}),
            json!({"managed_secret": {"secret": "sk", "key_version": "v1"}}),
            json!({"environment": {"binding": "/etc/secret"}}),
            json!({"environment": {"binding": "openai", "path": "/etc/secret"}}),
            json!({"external_secret": {"backend": "vault", "reference": ""}}),
        ] {
            assert!(
                serde_json::from_value::<ProviderCredentialWrite>(json!({
                    "name": "openai-key",
                    "provider": "openai",
                    "source": source,
                }))
                .is_err(),
                "{source}"
            );
        }
    }

    /// Proves the Vault `<path>#<key>` grammar splits at the first `#` and
    /// rejects every malformed path, key, and length.
    #[test]
    fn vault_references_follow_path_key_grammar() {
        let reference = ExternalSecretReference::new("tenants/acme/openai#api#key").expect("valid");
        assert_eq!(reference.path(), "tenants/acme/openai");
        assert_eq!(reference.key(), "api#key");
        let long = format!("{}#k", "a".repeat(1023));
        for invalid in [
            "",
            "no-key",
            "path#",
            "#key",
            "a//b#k",
            "/a#k",
            "a/#k",
            "a/./b#k",
            "a/../b#k",
            "a?b#k",
            "a%2fb#k",
            "a\\b#k",
            "a\nb#k",
            long.as_str(),
        ] {
            let error = ExternalSecretReference::new(invalid).expect_err(invalid);
            assert_eq!(error.field, "source.reference");
        }
    }
}
