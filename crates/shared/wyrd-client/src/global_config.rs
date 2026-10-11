//! User-scoped client configuration loaded from `config.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use wyrd_spec::card::workflow::ExternalGatewayBindingConfig;
use wyrd_spec::ids::CredentialBindingName;

use crate::credentials_file::CredentialsFile;
use crate::environment::Environment;
use crate::error::WyrdClientError;

/// Global configuration loaded from the user's Wyrd config directory.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    /// Client connection and credential settings.
    #[serde(default)]
    pub client: ClientSection,
    /// Local Workflow execution settings.
    #[serde(default)]
    pub workflow: LocalWorkflowConfig,
}

/// Settings for Workflows executed in this process.
///
/// Holds secret references only; a run resolves the references of the
/// bindings its routes select, and loading resolves none.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalWorkflowConfig {
    /// External gateway bindings an `ext_gateway` route selects by name,
    /// configured as `[workflow.external_gateway_bindings.<name>]`.
    #[serde(default)]
    pub external_gateway_bindings: BTreeMap<CredentialBindingName, ExternalGatewayBindingConfig>,
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
}

/// Supported access-token cache persistence strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCacheKind {
    /// Keep access tokens in memory only.
    InMemory,
    /// Also cache an API key's access token in `credentials.toml`.
    Disk,
}

impl GlobalConfig {
    /// Read the protected file-floor key for local setup without consulting
    /// environment credentials or saved human logins.
    ///
    /// # Errors
    /// Refuses unsafe or corrupt credential storage and an unresolved directory.
    pub fn local_api_key(
        environment: &Environment,
    ) -> Result<Option<SecretString>, WyrdClientError> {
        let dir = environment
            .config_dir()
            .ok_or_else(|| WyrdClientError::Config {
                field: "config_home".to_owned(),
                reason: "set WYRD_CONFIG_HOME, XDG_CONFIG_HOME, or HOME".to_owned(),
            })?;
        let store = CredentialsFile::at(dir.clone());
        let _lock = store.lock()?;
        CredentialsFile::config_at(dir).document()?;
        let document = store.document()?;
        CredentialsFile::api_key(&document)
    }

    /// Save local setup's HTTP and gRPC endpoints and expected tenant key in
    /// the existing protected files, preserving comments and other settings.
    /// Both endpoints are replaced together so a fresh client never pairs the
    /// local key with a stale endpoint from another deployment.
    ///
    /// An existing different default key is never overwritten. The credential
    /// is committed first: a later endpoint failure leaves the key recoverable
    /// by rerunning with the same configuration directory.
    ///
    /// # Errors
    /// Refuses unsafe or corrupt files, a conflicting key, and IO failures.
    /// After a configuration-write failure the credential may already be saved.
    pub fn save_local(
        environment: &Environment,
        server: &str,
        grpc: &str,
        credential: Option<&SecretString>,
    ) -> Result<(), WyrdClientError> {
        let dir = environment
            .config_dir()
            .ok_or_else(|| WyrdClientError::Config {
                field: "config_home".to_owned(),
                reason: "set WYRD_CONFIG_HOME, XDG_CONFIG_HOME, or HOME".to_owned(),
            })?;
        let credentials = CredentialsFile::at(dir.clone());
        let _lock = credentials.lock()?;
        let config = CredentialsFile::config_at(dir);
        let mut settings = config.document()?;
        let mut keys = credentials.document()?;
        toml::from_str::<Self>(&settings.to_string()).map_err(|_| WyrdClientError::Config {
            field: "global_config".to_owned(),
            reason: "config.toml contains invalid settings".to_owned(),
        })?;
        let existing = CredentialsFile::api_key(&keys)?;
        if existing.as_ref().is_some_and(|key| {
            credential.is_none_or(|expected| key.expose_secret() != expected.expose_secret())
        }) {
            return Err(WyrdClientError::Config {
                field: "credentials".to_owned(),
                reason: "an existing default key is preserved; use a separate WYRD_CONFIG_HOME for this deployment".to_owned(),
            });
        }
        if let Some(credential) = credential.filter(|_| existing.is_none()) {
            keys["default"]["api_key"] = toml_edit::value(credential.expose_secret());
            if let Some(profile) = keys["default"].as_table_like_mut() {
                profile.remove("access_token");
                profile.remove("access_expires_at");
            }
            credentials.replace(&keys)?;
        }
        settings["client"]["http_url"] = toml_edit::value(server);
        settings["client"]["grpc_url"] = toml_edit::value(grpc);
        config.replace(&settings)
    }

    /// Load `config.toml` from the resolved Wyrd config directory.
    ///
    /// A missing directory or file is equivalent to an empty configuration.
    /// Invalid TOML or an unknown field is returned to the caller.
    ///
    /// # Errors
    /// Returns the errors of [`GlobalConfig::load_from`].
    pub fn load() -> Result<Self, WyrdClientError> {
        Self::load_in(&Environment::Process)
    }

    /// Load `config.toml` from the Wyrd config directory `environment`
    /// resolves.
    ///
    /// A missing directory or file is equivalent to an empty configuration.
    ///
    /// # Arguments
    /// * `environment` - Variables that resolve the Wyrd config directory.
    ///
    /// # Errors
    /// Returns the errors of [`GlobalConfig::load_from`].
    pub fn load_in(environment: &Environment) -> Result<Self, WyrdClientError> {
        let Some(directory) = environment.config_dir() else {
            return Ok(Self::default());
        };
        Self::load_from(&directory.join("config.toml"))
    }

    /// Load a global config from an explicit path.
    ///
    /// # Arguments
    /// * `path` - Path of the TOML config file to read.
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
    use crate::credentials_file::private_tempdir;
    use crate::environment::Environment;
    use secrecy::SecretString;
    use std::fs;

    use tempfile::tempdir;

    use super::{GlobalConfig, TokenCacheKind};

    /// Local bootstrap preserves unrelated settings and refuses to replace a
    /// different default key; a rerun with the saved key is harmless.
    ///
    /// # Panics
    /// Panics when persistence changes unrelated content or accepts a conflict.
    #[test]
    fn local_setup_preserves_content_and_existing_key() {
        let dir = private_tempdir();
        let environment =
            Environment::from([("WYRD_CONFIG_HOME", dir.path().to_str().expect("UTF-8 path"))]);
        fs::write(
            dir.path().join("config.toml"),
            "# retain this comment\n[client]\ngrpc_url = 'https://stale.example.com:50051'\n[client.token_cache]\nkind = 'disk'\n",
        )
        .expect("config fixture");
        let key = SecretString::from("first-key".to_owned());
        GlobalConfig::save_local(
            &environment,
            "http://127.0.0.1:8080",
            "http://127.0.0.1:50051",
            Some(&key),
        )
        .expect("first save");
        let credentials_path = dir.path().join("credentials.toml");
        let mut retained = fs::read_to_string(&credentials_path).expect("credentials saved");
        retained.push_str("# retained cache\naccess_token = 'cached-token'\naccess_expires_at = '2099-01-01T00:00:00Z'\n");
        fs::write(&credentials_path, retained).expect("cache fixture");
        let credentials = fs::read(&credentials_path).expect("cached credentials");
        GlobalConfig::save_local(
            &environment,
            "http://127.0.0.1:8080",
            "http://127.0.0.1:50051",
            Some(&key),
        )
        .expect("retry");
        assert!(
            GlobalConfig::save_local(
                &environment,
                "http://127.0.0.1:9090",
                "http://127.0.0.1:50052",
                None
            )
            .is_err()
        );
        assert_eq!(
            fs::read(dir.path().join("credentials.toml")).expect("credentials retained"),
            credentials
        );
        assert!(
            fs::read_to_string(dir.path().join("config.toml"))
                .expect("config saved")
                .contains("# retain this comment")
        );
        assert_eq!(
            GlobalConfig::load_in(&environment)
                .expect("loads")
                .client
                .token_cache
                .expect("cache retained")
                .kind,
            Some(TokenCacheKind::Disk)
        );
        assert!(
            GlobalConfig::save_local(
                &environment,
                "http://127.0.0.1:9090",
                "http://127.0.0.1:50052",
                Some(&SecretString::from("other-key".to_owned()))
            )
            .is_err()
        );
        assert_eq!(
            fs::read(dir.path().join("credentials.toml")).expect("original key retained"),
            credentials
        );
        let saved = GlobalConfig::load_in(&environment).expect("loads").client;
        assert_eq!(saved.http_url.as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(saved.grpc_url.as_deref(), Some("http://127.0.0.1:50051"));
    }

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
            "[client]\ngrpc_url = \"grpc\"\nhttp_url = \"http\"\ntenant = \"acme\"\n\n[client.token_cache]\nkind = \"disk\"\n",
        )
        .unwrap();

        let config = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(config.client.grpc_url.as_deref(), Some("grpc"));
        assert_eq!(config.client.http_url.as_deref(), Some("http"));
        assert_eq!(config.client.tenant.as_deref(), Some("acme"));
        let cache = config.client.token_cache.as_ref().unwrap();
        assert_eq!(cache.kind, Some(TokenCacheKind::Disk));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[unknown]\nvalue = true\n").unwrap();
        assert!(GlobalConfig::load_from(&path).is_err());
    }
}
