//! `[kms]`: the one typed stored-secret wrapping configuration.
//!
//! TOML deserializes into [`KmsConfig`]; [`KmsConfig::apply_env`] applies the
//! canonical `WYRD_KMS_*` projections over it. [`crate::WyrdKms::new`]
//! validates the result. Diagnostics name the setting, never its value.

use std::fmt;
use std::path::PathBuf;

use secrecy::SecretString;
use serde::Deserialize;

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
}

impl KmsConfig {
    /// Validate this configuration for a `production` or development server.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] naming the first offending setting.
    pub(crate) fn resolve(&self, production: bool) -> Result<Settings, KmsError> {
        let _ = production;
        let kind = self.backend.ok_or(KmsError::Config {
            setting: "backend",
            reason: "is required",
        })?;
        let active = self.active_key.clone().ok_or(KmsError::Config {
            setting: "active_key",
            reason: "is required",
        })?;
        Ok(Settings {
            kind,
            active,
            retained: self.retained_keys.clone(),
            key_dir: self.key_dir.clone(),
        })
    }
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
