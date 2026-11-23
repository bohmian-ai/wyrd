//! `[kms]`: the one typed stored-secret wrapping configuration.
//!
//! TOML deserializes into [`KmsConfig`]; [`KmsConfig::apply_env`] applies the
//! canonical `WYRD_KMS_*` projections over it. [`crate::WyrdKms::new`]
//! validates the result. Diagnostics name the setting, never its value.

use std::fmt;
use std::path::PathBuf;

use secrecy::SecretString;
use serde::Deserialize;
use url::Url;

use crate::KmsError;

/// The wrapping backend a deployment selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KmsBackendKind {
    /// Owner-only base64 key files under `key_dir`, held in process memory.
    File,
    /// An explicit, ephemeral process-memory key for disposable development
    /// and tests; losing the process loses every envelope it wrote.
    Memory,
    /// A GCP Cloud KMS symmetric CryptoKey.
    GcpKms,
    /// An AWS KMS symmetric key.
    AwsKms,
    /// A HashiCorp Vault Transit key.
    VaultTransit,
}

impl KmsBackendKind {
    /// The canonical configuration spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Memory => "memory",
            Self::GcpKms => "gcp_kms",
            Self::AwsKms => "aws_kms",
            Self::VaultTransit => "vault_transit",
        }
    }

    /// Whether the backend's keys live in process memory, so wrap and unwrap
    /// wait on no external service.
    #[must_use]
    pub const fn is_local(self) -> bool {
        matches!(self, Self::File | Self::Memory)
    }

    /// Parse the canonical spelling.
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "file" => Self::File,
            "memory" => Self::Memory,
            "gcp_kms" => Self::GcpKms,
            "aws_kms" => Self::AwsKms,
            "vault_transit" => Self::VaultTransit,
            _ => return None,
        })
    }
}

impl fmt::Display for KmsBackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stored-secret wrapping configuration (`[kms]`, `WYRD_KMS_*`).
///
/// Every field but `vault_token` has a TOML key of the same name;
/// `vault_token` is environment-only.
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KmsConfig {
    /// Selected backend (`WYRD_KMS_BACKEND`); required.
    #[serde(default)]
    pub backend: Option<KmsBackendKind>,
    /// Reference new envelopes are wrapped under (`WYRD_KMS_ACTIVE_KEY`);
    /// required.
    #[serde(default)]
    pub active_key: Option<String>,
    /// References still allowed to open existing envelopes
    /// (`WYRD_KMS_RETAINED_KEYS`, comma-separated).
    #[serde(default)]
    pub retained_keys: Vec<String>,
    /// `file` only: directory of key files named by reference
    /// (`WYRD_KMS_KEY_DIR`).
    #[serde(default)]
    pub key_dir: Option<PathBuf>,
    /// Remote API base URL (`WYRD_KMS_ENDPOINT`): required for
    /// `vault_transit`, an optional override for `gcp_kms` and `aws_kms`.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// `vault_transit` only: Transit mount (`WYRD_KMS_VAULT_MOUNT`),
    /// default `transit`.
    #[serde(default)]
    pub vault_mount: Option<String>,
    /// `vault_transit` only: Enterprise namespace
    /// (`WYRD_KMS_VAULT_NAMESPACE`).
    #[serde(default)]
    pub vault_namespace: Option<String>,
    /// `vault_transit` only: owner-only token file, re-read per request so an
    /// agent may renew it in place (`WYRD_KMS_VAULT_TOKEN_FILE`).
    #[serde(default)]
    pub vault_token_file: Option<PathBuf>,
    /// `vault_transit` only: inline token from `WYRD_KMS_VAULT_TOKEN`; never
    /// read from TOML.
    #[serde(skip)]
    pub vault_token: Option<SecretString>,
}

impl fmt::Debug for KmsConfig {
    /// Redacting debug: the backend, retained-key count, and which optional
    /// settings are present; never a reference, path, endpoint, or token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KmsConfig")
            .field("backend", &self.backend)
            .field("active_key", &self.active_key.is_some())
            .field("retained_keys", &self.retained_keys.len())
            .field("key_dir", &self.key_dir.is_some())
            .field("endpoint", &self.endpoint.is_some())
            .field("vault_token", &self.vault_token.is_some())
            .finish_non_exhaustive()
    }
}

impl KmsConfig {
    /// The explicit development `memory` selection with `active_key`: an
    /// ephemeral random key per facade, refused in production.
    #[must_use]
    pub fn memory(active_key: &str) -> Self {
        Self {
            backend: Some(KmsBackendKind::Memory),
            active_key: Some(active_key.to_owned()),
            ..Self::default()
        }
    }

    /// Whether no setting is present, which leaves a deployment without
    /// recoverable stored secrets rather than with a default backend.
    #[must_use]
    pub fn is_unset(&self) -> bool {
        self.backend.is_none()
            && self.active_key.is_none()
            && self.retained_keys.is_empty()
            && self.key_dir.is_none()
            && self.endpoint.is_none()
            && self.vault_mount.is_none()
            && self.vault_namespace.is_none()
            && self.vault_token_file.is_none()
            && self.vault_token.is_none()
    }

    /// Apply the `WYRD_KMS_*` environment projections over this
    /// configuration; a set variable replaces its setting.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] for an empty or non-UTF-8 variable or an
    /// unknown backend spelling.
    pub fn apply_env(&mut self) -> Result<(), KmsError> {
        if let Some(value) = env("WYRD_KMS_BACKEND", "backend")? {
            self.backend = Some(KmsBackendKind::parse(&value).ok_or(KmsError::Config {
                setting: "backend",
                reason: "must be file, memory, gcp_kms, aws_kms, or vault_transit",
            })?);
        }
        if let Some(value) = env("WYRD_KMS_ACTIVE_KEY", "active_key")? {
            self.active_key = Some(value);
        }
        if let Some(value) = env("WYRD_KMS_RETAINED_KEYS", "retained_keys")? {
            self.retained_keys = value
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(str::to_owned)
                .collect();
        }
        if let Some(value) = env("WYRD_KMS_KEY_DIR", "key_dir")? {
            self.key_dir = Some(PathBuf::from(value));
        }
        if let Some(value) = env("WYRD_KMS_ENDPOINT", "endpoint")? {
            self.endpoint = Some(value);
        }
        if let Some(value) = env("WYRD_KMS_VAULT_MOUNT", "vault_mount")? {
            self.vault_mount = Some(value);
        }
        if let Some(value) = env("WYRD_KMS_VAULT_NAMESPACE", "vault_namespace")? {
            self.vault_namespace = Some(value);
        }
        if let Some(value) = env("WYRD_KMS_VAULT_TOKEN_FILE", "vault_token_file")? {
            self.vault_token_file = Some(PathBuf::from(value));
        }
        if let Some(value) = env("WYRD_KMS_VAULT_TOKEN", "vault_token")? {
            self.vault_token = Some(SecretString::from(value));
        }
        Ok(())
    }
}

/// A validated configuration.
pub(crate) struct Settings {
    /// Selected backend.
    pub(crate) kind: KmsBackendKind,
    /// Active reference.
    pub(crate) active: String,
    /// Retained references.
    pub(crate) retained: Vec<String>,
    /// `file` key directory.
    pub(crate) key_dir: Option<PathBuf>,
    /// Remote endpoint override, or the Vault address.
    pub(crate) endpoint: Option<Url>,
    /// `vault_transit` settings.
    pub(crate) vault: Option<VaultSettings>,
}

/// Validated `vault_transit` settings.
pub(crate) struct VaultSettings {
    /// Transit mount.
    pub(crate) mount: String,
    /// Enterprise namespace.
    pub(crate) namespace: Option<String>,
    /// Where the token comes from.
    pub(crate) token: VaultToken,
}

/// The one configured Vault token source.
pub(crate) enum VaultToken {
    /// Owner-only file re-read per request.
    File(PathBuf),
    /// Inline environment token.
    Inline(SecretString),
}

/// A refusal of `setting` for `reason`.
const fn invalid(setting: &'static str, reason: &'static str) -> KmsError {
    KmsError::Config { setting, reason }
}

impl KmsConfig {
    /// Validate this configuration for a `production` or development server.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] naming the first offending setting: a
    /// missing backend or active key, a reference malformed for the backend,
    /// an active key repeated as retained or a duplicate retained key, a
    /// setting that does not apply to the backend, a missing `key_dir` or
    /// Vault endpoint, anything but exactly one Vault token source, a
    /// non-`http(s)` endpoint, and in `production` the memory backend or a
    /// plaintext endpoint.
    pub(crate) fn resolve(&self, production: bool) -> Result<Settings, KmsError> {
        let kind = self.backend.ok_or(invalid("backend", "is required"))?;
        let active = self
            .active_key
            .clone()
            .ok_or(invalid("active_key", "is required"))?;
        if !valid_reference(kind, &active) {
            return Err(invalid(
                "active_key",
                "is not a valid key reference for the backend",
            ));
        }
        for (index, retained) in self.retained_keys.iter().enumerate() {
            if !valid_reference(kind, retained) {
                return Err(invalid(
                    "retained_keys",
                    "holds an invalid key reference for the backend",
                ));
            }
            if *retained == active || self.retained_keys[..index].contains(retained) {
                return Err(invalid(
                    "retained_keys",
                    "repeats the active key or another retained key",
                ));
            }
        }
        if kind == KmsBackendKind::Memory {
            if production {
                return Err(invalid("backend", "memory is refused in production"));
            }
            if !self.retained_keys.is_empty() {
                return Err(invalid(
                    "retained_keys",
                    "do not apply to the ephemeral memory backend",
                ));
            }
        }
        let vault = kind == KmsBackendKind::VaultTransit;
        for (present, applies, setting) in [
            (
                self.key_dir.is_some(),
                kind == KmsBackendKind::File,
                "key_dir",
            ),
            (
                self.endpoint.is_some(),
                matches!(
                    kind,
                    KmsBackendKind::GcpKms | KmsBackendKind::AwsKms | KmsBackendKind::VaultTransit
                ),
                "endpoint",
            ),
            (self.vault_mount.is_some(), vault, "vault_mount"),
            (self.vault_namespace.is_some(), vault, "vault_namespace"),
            (self.vault_token_file.is_some(), vault, "vault_token_file"),
            (self.vault_token.is_some(), vault, "vault_token"),
        ] {
            if present && !applies {
                return Err(invalid(setting, "does not apply to the selected backend"));
            }
        }
        if kind == KmsBackendKind::File && self.key_dir.is_none() {
            return Err(invalid("key_dir", "is required for the file backend"));
        }
        let endpoint = self
            .endpoint
            .as_deref()
            .map(|endpoint| parse_endpoint(endpoint, production))
            .transpose()?;
        let vault = if vault {
            if endpoint.is_none() {
                return Err(invalid(
                    "endpoint",
                    "is required for the vault_transit backend",
                ));
            }
            let token = match (&self.vault_token_file, &self.vault_token) {
                (Some(path), None) => VaultToken::File(path.clone()),
                (None, Some(token)) => VaultToken::Inline(token.clone()),
                _ => {
                    return Err(invalid(
                        "vault_token",
                        "requires exactly one of vault_token_file and WYRD_KMS_VAULT_TOKEN",
                    ));
                }
            };
            Some(VaultSettings {
                mount: self
                    .vault_mount
                    .clone()
                    .unwrap_or_else(|| "transit".to_owned()),
                namespace: self.vault_namespace.clone(),
                token,
            })
        } else {
            None
        };
        Ok(Settings {
            kind,
            active,
            retained: self.retained_keys.clone(),
            key_dir: self.key_dir.clone(),
            endpoint,
            vault,
        })
    }
}

/// Parse an `http(s)` endpoint; `production` requires `https`.
///
/// # Errors
/// Returns [`KmsError::Config`] on `endpoint`.
fn parse_endpoint(endpoint: &str, production: bool) -> Result<Url, KmsError> {
    let url = Url::parse(endpoint).map_err(|_| invalid("endpoint", "is not a URL"))?;
    match url.scheme() {
        "https" => Ok(url),
        "http" if !production => Ok(url),
        "http" => Err(invalid("endpoint", "must use https in production")),
        _ => Err(invalid("endpoint", "must be an http(s) URL")),
    }
}

/// Whether `reference` is well formed for `kind`.
///
/// Local and Transit references are key names of at most 128 ASCII letters,
/// digits, `_`, `-`, or `.`, not starting with `.`, so a file reference can
/// never leave `key_dir`. GCP references are CryptoKey resource names without
/// a version; AWS references are key ARNs, never aliases.
fn valid_reference(kind: KmsBackendKind, reference: &str) -> bool {
    match kind {
        KmsBackendKind::File | KmsBackendKind::Memory | KmsBackendKind::VaultTransit => {
            key_name(reference)
        }
        KmsBackendKind::GcpKms => {
            let parts: Vec<&str> = reference.split('/').collect();
            parts.len() == 8
                && ["projects", "locations", "keyRings", "cryptoKeys"]
                    .iter()
                    .zip(parts.chunks(2))
                    .all(|(label, pair)| pair[0] == *label && key_name(pair[1]))
        }
        KmsBackendKind::AwsKms => aws_region(reference).is_some(),
    }
}

/// Whether `name` is a safe key name.
fn key_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// The partition and region of an AWS KMS key ARN
/// `arn:<partition>:kms:<region>:<12-digit account>:key/<id>`, or `None`.
pub(crate) fn aws_region(arn: &str) -> Option<(&str, &str)> {
    let mut parts = arn.splitn(6, ':');
    let (Some("arn"), Some(partition), Some("kms"), Some(region), Some(account), Some(resource)) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return None;
    };
    let id = resource.strip_prefix("key/")?;
    let ok = matches!(partition, "aws" | "aws-cn" | "aws-us-gov")
        && !region.is_empty()
        && region
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && account.len() == 12
        && account.bytes().all(|byte| byte.is_ascii_digit())
        && !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    ok.then_some((partition, region))
}

/// Read `key`, treating unset as absent and an empty or non-UTF-8 value as a
/// configuration error on `setting`.
///
/// # Errors
/// Returns [`KmsError::Config`] for an empty or non-UTF-8 value.
fn env(key: &str, setting: &'static str) -> Result<Option<String>, KmsError> {
    match std::env::var(key) {
        Ok(value) if value.trim().is_empty() => Err(KmsError::Config {
            setting,
            reason: "is set but empty",
        }),
        Ok(value) => Ok(Some(value.trim().to_owned())),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(KmsError::Config {
            setting,
            reason: "is not valid UTF-8",
        }),
    }
}

#[cfg(test)]
mod tests {
    //! Validation, redaction, and environment projection of `[kms]`.

    use super::*;

    /// A configuration selecting `backend` with active key `active`.
    fn config(backend: KmsBackendKind, active: &str) -> KmsConfig {
        KmsConfig {
            backend: Some(backend),
            active_key: Some(active.to_owned()),
            ..KmsConfig::default()
        }
    }

    /// The setting a refused configuration names.
    fn refused(config: &KmsConfig, production: bool) -> &'static str {
        match config.resolve(production) {
            Err(KmsError::Config { setting, .. }) => setting,
            Err(other) => panic!("unexpected error {other:?}"),
            Ok(_) => panic!("configuration was accepted"),
        }
    }

    /// Valid selections of every backend resolve.
    #[test]
    fn valid_selections_resolve() {
        let file = KmsConfig {
            key_dir: Some(PathBuf::from("/keys")),
            retained_keys: vec!["k0".to_owned()],
            ..config(KmsBackendKind::File, "k1")
        };
        let gcp = config(
            KmsBackendKind::GcpKms,
            "projects/p/locations/global/keyRings/r/cryptoKeys/wyrd-secrets",
        );
        let aws = config(
            KmsBackendKind::AwsKms,
            "arn:aws:kms:us-east-1:111122223333:key/1234abcd-12ab-34cd-56ef-1234567890ab",
        );
        let vault = KmsConfig {
            endpoint: Some("https://vault.internal:8200".to_owned()),
            vault_token_file: Some(PathBuf::from("/run/vault-token")),
            ..config(KmsBackendKind::VaultTransit, "wyrd-secrets")
        };
        for (valid, production) in [
            (&file, true),
            (&config(KmsBackendKind::Memory, "dev"), false),
            (&gcp, true),
            (&aws, true),
            (&vault, true),
        ] {
            assert!(valid.resolve(production).is_ok(), "{valid:?}");
        }
    }

    /// Missing, malformed, incoherent, and production-unsafe settings refuse
    /// and name the setting.
    #[test]
    fn malformed_missing_or_incoherent_settings_refuse() {
        let memory = config(KmsBackendKind::Memory, "dev");
        let file = KmsConfig {
            key_dir: Some(PathBuf::from("/keys")),
            ..config(KmsBackendKind::File, "k1")
        };
        let vault = KmsConfig {
            endpoint: Some("https://vault.internal:8200".to_owned()),
            vault_token: Some(SecretString::from("hvs.x")),
            ..config(KmsBackendKind::VaultTransit, "wyrd-secrets")
        };
        let cases: Vec<(KmsConfig, bool, &str)> = vec![
            (KmsConfig::default(), false, "backend"),
            (
                KmsConfig {
                    active_key: None,
                    ..memory.clone()
                },
                false,
                "active_key",
            ),
            (memory.clone(), true, "backend"),
            (
                KmsConfig {
                    retained_keys: vec!["old".to_owned()],
                    ..memory.clone()
                },
                false,
                "retained_keys",
            ),
            (
                KmsConfig {
                    key_dir: Some(PathBuf::from("/keys")),
                    ..memory.clone()
                },
                false,
                "key_dir",
            ),
            (
                KmsConfig {
                    key_dir: None,
                    ..file.clone()
                },
                false,
                "key_dir",
            ),
            (config(KmsBackendKind::File, "../k1"), false, "active_key"),
            (
                KmsConfig {
                    retained_keys: vec!["k1".to_owned()],
                    ..file.clone()
                },
                false,
                "retained_keys",
            ),
            (
                KmsConfig {
                    retained_keys: vec!["k0".to_owned(), "k0".to_owned()],
                    ..file.clone()
                },
                false,
                "retained_keys",
            ),
            (
                KmsConfig {
                    endpoint: Some("https://kms".to_owned()),
                    ..file.clone()
                },
                false,
                "endpoint",
            ),
            (
                KmsConfig {
                    vault_mount: Some("transit".to_owned()),
                    ..file.clone()
                },
                false,
                "vault_mount",
            ),
            (
                config(
                    KmsBackendKind::GcpKms,
                    "projects/p/locations/global/keyRings/r/cryptoKeys/k/cryptoKeyVersions/1",
                ),
                false,
                "active_key",
            ),
            (
                config(
                    KmsBackendKind::AwsKms,
                    "arn:aws:kms:us-east-1:111122223333:alias/wyrd",
                ),
                false,
                "active_key",
            ),
            (
                KmsConfig {
                    endpoint: Some("http://localhost:4566".to_owned()),
                    ..config(
                        KmsBackendKind::AwsKms,
                        "arn:aws:kms:us-east-1:111122223333:key/abc",
                    )
                },
                true,
                "endpoint",
            ),
            (
                KmsConfig {
                    endpoint: None,
                    ..vault.clone()
                },
                false,
                "endpoint",
            ),
            (
                KmsConfig {
                    endpoint: Some("not a url".to_owned()),
                    ..vault.clone()
                },
                false,
                "endpoint",
            ),
            (
                KmsConfig {
                    endpoint: Some("http://vault.internal:8200".to_owned()),
                    ..vault.clone()
                },
                true,
                "endpoint",
            ),
            (
                KmsConfig {
                    vault_token: None,
                    ..vault.clone()
                },
                false,
                "vault_token",
            ),
            (
                KmsConfig {
                    vault_token_file: Some(PathBuf::from("/run/token")),
                    ..vault.clone()
                },
                false,
                "vault_token",
            ),
        ];
        for (case, production, setting) in &cases {
            assert_eq!(refused(case, *production), *setting, "{case:?}");
        }
    }

    /// Debug output and every diagnostic omit references, paths,
    /// endpoints, and tokens.
    #[test]
    fn debug_and_diagnostics_are_redacted() {
        let vault = KmsConfig {
            endpoint: Some("http://vault.secret-host:8200".to_owned()),
            vault_token: Some(SecretString::from("hvs.canary-token")),
            vault_token_file: Some(PathBuf::from("/run/canary-path")),
            retained_keys: vec!["canary-retained".to_owned()],
            ..config(KmsBackendKind::VaultTransit, "canary-active")
        };
        let debug = format!("{vault:?}");
        let error = vault.resolve(true).err().expect("refused").to_string();
        for text in [debug, error] {
            for canary in [
                "secret-host",
                "canary-token",
                "canary-path",
                "canary-retained",
                "canary-active",
            ] {
                assert!(!text.contains(canary), "{text}");
            }
        }
    }

    /// `WYRD_KMS_*` variables project onto their settings; an unknown
    /// backend or an empty variable refuses.
    #[test]
    fn environment_projects_onto_settings() {
        temp_env::with_vars(
            [
                ("WYRD_KMS_BACKEND", Some("vault_transit")),
                ("WYRD_KMS_ACTIVE_KEY", Some("wyrd-secrets")),
                ("WYRD_KMS_RETAINED_KEYS", Some("old-a, old-b")),
                ("WYRD_KMS_ENDPOINT", Some("https://vault.internal:8200")),
                ("WYRD_KMS_VAULT_MOUNT", Some("transit/wyrd")),
                ("WYRD_KMS_VAULT_NAMESPACE", Some("ns1")),
                ("WYRD_KMS_VAULT_TOKEN", Some("hvs.x")),
            ],
            || {
                let mut projected = KmsConfig::default();
                projected.apply_env().expect("projects");
                assert_eq!(projected.backend, Some(KmsBackendKind::VaultTransit));
                assert_eq!(projected.active_key.as_deref(), Some("wyrd-secrets"));
                assert_eq!(projected.retained_keys, ["old-a", "old-b"]);
                assert_eq!(projected.vault_mount.as_deref(), Some("transit/wyrd"));
                assert!(projected.vault_token.is_some());
                assert!(projected.resolve(true).is_ok());
            },
        );
        for (key, value, setting) in [
            ("WYRD_KMS_BACKEND", "sealing", "backend"),
            ("WYRD_KMS_ACTIVE_KEY", " ", "active_key"),
        ] {
            temp_env::with_var(key, Some(value), || {
                assert!(
                    matches!(
                        KmsConfig::default().apply_env(),
                        Err(KmsError::Config { setting: named, .. }) if named == setting
                    ),
                    "{key}"
                );
            });
        }
    }
}
