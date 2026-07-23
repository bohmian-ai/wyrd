//! User-scoped client configuration loaded from `config.toml`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::WyrdClientError;

/// Global configuration loaded from the user's Wyrd config directory.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    /// Client connection and credential settings.
    #[serde(default)]
    pub client: ClientSection,
}

/// User-scoped client settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientSection {
    /// Optional gRPC endpoint override.
    pub grpc_url: Option<String>,
    /// Optional HTTP endpoint override.
    pub http_url: Option<String>,
    /// Optional workload-identity tenant slug.
    pub tenant: Option<String>,
    /// Optional access-token cache settings.
    pub token_cache: Option<TokenCacheSection>,
}

/// Access-token cache settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenCacheSection {
    /// Cache persistence strategy.
    pub kind: Option<TokenCacheKind>,
    /// Path used when the cache is persisted to disk.
    pub path: Option<PathBuf>,
}

/// Supported access-token cache persistence strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCacheKind {
    /// Keep access tokens in memory only.
    InMemory,
    /// Persist access tokens on disk.
    Disk,
}

impl GlobalConfig {
    /// Load `config.toml` from the resolved Wyrd config directory.
    ///
    /// A missing directory or file is equivalent to an empty configuration.
    /// Invalid TOML or an unknown field is returned to the caller.
    pub fn load() -> Result<Self, WyrdClientError> {
        let Some(directory) = wyrd_utils::config_dir::wyrd_config_dir() else {
            return Ok(Self::default());
        };
        Self::load_from(&directory.join("config.toml"))
    }

    /// Load a global config from an explicit path.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when the file cannot be read or
    /// deserialized, except for a missing file which returns an empty config.
    pub fn load_from(path: &Path) -> Result<Self, WyrdClientError> {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(WyrdClientError::Config {
                    field: "global_config".to_owned(),
                    reason: format!("failed to read {}: {error}", path.display()),
                });
            }
        };
        toml::from_str(&content).map_err(|error| WyrdClientError::Config {
            field: "global_config".to_owned(),
            reason: format!("failed to parse {}: {error}", path.display()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{GlobalConfig, TokenCacheKind};

    #[test]
    fn missing_file_returns_default() {
        let directory = tempdir().unwrap();
        let config = GlobalConfig::load_from(&directory.path().join("missing.toml")).unwrap();
        assert!(config.client.grpc_url.is_none());
    }

    #[test]
    fn full_file_populates_client_settings() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "[client]\ngrpc_url = \"grpc\"\nhttp_url = \"http\"\ntenant = \"acme\"\n\n[client.token_cache]\nkind = \"disk\"\npath = \"tokens\"\n",
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(config.client.grpc_url.as_deref(), Some("grpc"));
        assert_eq!(config.client.http_url.as_deref(), Some("http"));
        assert_eq!(config.client.tenant.as_deref(), Some("acme"));
        let cache = config.client.token_cache.as_ref().unwrap();
        assert_eq!(cache.kind, Some(TokenCacheKind::Disk));
        assert_eq!(cache.path.as_deref(), Some(std::path::Path::new("tokens")));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[unknown]\nvalue = true\n").unwrap();
        assert!(GlobalConfig::load_from(&path).is_err());
    }
}
