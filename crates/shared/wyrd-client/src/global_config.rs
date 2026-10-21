//! User-scoped client configuration loaded from `config.toml`.

use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

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
    /// Non-secret settings `wyrd init` resolved and reuses on restart.
    #[serde(default)]
    pub dev: DevSection,
}

/// Local development selection saved by `wyrd init`.
///
/// Every value is non-secret; database URLs live in the protected
/// `credentials.toml` `[dev]` table instead. Paths are absolute so a restart
/// from another working directory selects the same data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevSection {
    /// Tenant slug provisioned for local development.
    pub tenant: Option<String>,
    /// HTTP loopback listener.
    pub bind: Option<String>,
    /// Public gRPC loopback listener.
    pub grpc_bind: Option<String>,
    /// Effective `WYRD_STORAGE_URL`.
    pub storage_url: Option<String>,
    /// Effective absolute `WYRD_BIFROST_DATA_DIR`.
    pub bifrost_data_dir: Option<String>,
}

/// Database URLs `wyrd init` keeps in the protected credentials file.
#[derive(Default, Deserialize)]
struct DevDatabaseFile {
    /// `[dev]`; absent means nothing saved.
    #[serde(default)]
    dev: DevDatabase,
}

/// The protected `[dev]` table.
#[derive(Default, Deserialize)]
struct DevDatabase {
    /// Ordinary migration-owner database URL.
    database_url: Option<String>,
    /// Optional separate platform database URL.
    platform_database_url: Option<String>,
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

/// Resolve the Wyrd configuration directory local setup writes.
///
/// # Errors
/// Refuses when no configuration home variable resolves.
fn config_dir(environment: &Environment) -> Result<std::path::PathBuf, WyrdClientError> {
    environment
        .config_dir()
        .ok_or_else(|| WyrdClientError::Config {
            field: "config_home".to_owned(),
            reason: "set WYRD_CONFIG_HOME, XDG_CONFIG_HOME, or HOME".to_owned(),
        })
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
        let dir = config_dir(environment)?;
        let store = CredentialsFile::at(dir.clone());
        let _lock = store.lock()?;
        CredentialsFile::config_at(dir).document()?;
        let document = store.document()?;
        CredentialsFile::api_key(&document)
    }

    /// Read the database URLs saved by local development, as
    /// `(database, platform)`, from the protected credentials file.
    ///
    /// # Errors
    /// Refuses unsafe or corrupt credential storage and an unresolved directory.
    pub fn local_databases(
        environment: &Environment,
    ) -> Result<(Option<SecretString>, Option<SecretString>), WyrdClientError> {
        let store = CredentialsFile::at(config_dir(environment)?);
        let _lock = store.lock()?;
        let saved: DevDatabaseFile =
            toml::from_str(&store.document()?.to_string()).map_err(|_| {
                WyrdClientError::Config {
                    field: "credentials".to_owned(),
                    reason: "credentials.toml has an invalid [dev] table".to_owned(),
                }
            })?;
        let secret = |value: Option<String>| {
            value
                .filter(|value| !value.is_empty())
                .map(SecretString::from)
        };
        Ok((
            secret(saved.dev.database_url),
            secret(saved.dev.platform_database_url),
        ))
    }

    /// Save local development's resolved settings: `dev` in `config.toml`
    /// and the database URLs in the protected `credentials.toml`, under the
    /// store lock with atomic replacement and every other setting, key, login,
    /// and comment preserved. A `None` platform URL removes a saved one.
    ///
    /// # Errors
    /// Refuses unsafe or corrupt files and IO failures; after a configuration
    /// failure the protected database URLs may already be saved.
    pub fn save_local_dev(
        environment: &Environment,
        dev: &DevSection,
        database: &SecretString,
        platform: Option<&SecretString>,
    ) -> Result<(), WyrdClientError> {
        let dir = config_dir(environment)?;
        let credentials = CredentialsFile::at(dir.clone());
        let _lock = credentials.lock()?;
        let config = CredentialsFile::config_at(dir);
        let mut settings = config.document()?;
        let mut keys = credentials.document()?;
        toml::from_str::<Self>(&settings.to_string()).map_err(|_| WyrdClientError::Config {
            field: "global_config".to_owned(),
            reason: "config.toml contains invalid settings".to_owned(),
        })?;
        keys["dev"]["database_url"] = toml_edit::value(database.expose_secret());
        match platform {
            Some(platform) => {
                keys["dev"]["platform_database_url"] = toml_edit::value(platform.expose_secret());
            }
            None => {
                if let Some(table) = keys["dev"].as_table_like_mut() {
                    table.remove("platform_database_url");
                }
            }
        }
        credentials.replace(&keys)?;
        for (field, value) in [
            ("tenant", &dev.tenant),
            ("bind", &dev.bind),
            ("grpc_bind", &dev.grpc_bind),
            ("storage_url", &dev.storage_url),
            ("bifrost_data_dir", &dev.bifrost_data_dir),
        ] {
            if let Some(value) = value {
                settings["dev"][field] = toml_edit::value(value.as_str());
            }
        }
        config.replace(&settings)
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
        let dir = config_dir(environment)?;
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
    use secrecy::{ExposeSecret, SecretString};
    use std::fs;

    use tempfile::tempdir;

    use super::{DevSection, GlobalConfig, TokenCacheKind};

    /// Local development settings round-trip: non-secret values land in
    /// `config.toml`, database URLs only in `credentials.toml`, and the saved
    /// key survives.
    ///
    /// # Panics
    /// Panics when a database URL leaks into general configuration or a saved
    /// value is lost.
    #[test]
    fn local_dev_settings_keep_database_urls_protected() {
        let dir = private_tempdir();
        let environment =
            Environment::from([("WYRD_CONFIG_HOME", dir.path().to_str().expect("UTF-8 path"))]);
        let key = SecretString::from("dev-key".to_owned());
        GlobalConfig::save_local(&environment, "http://a", "http://b", Some(&key)).expect("key");
        let dev = DevSection {
            tenant: Some("local".to_owned()),
            storage_url: Some("file:///srv/storage".to_owned()),
            ..DevSection::default()
        };
        let database = SecretString::from("postgres://owner:pw@db/wyrd".to_owned());
        let platform = SecretString::from("postgres://platform:pw@db/wyrd".to_owned());
        GlobalConfig::save_local_dev(&environment, &dev, &database, Some(&platform))
            .expect("first save");
        GlobalConfig::save_local_dev(&environment, &dev, &database, None).expect("second save");
        let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
        assert!(!config.contains("pw@"));
        assert_eq!(GlobalConfig::load_in(&environment).expect("loads").dev, dev);
        let (saved, saved_platform) = GlobalConfig::local_databases(&environment).expect("reads");
        assert_eq!(
            saved.expect("database").expose_secret(),
            database.expose_secret()
        );
        assert!(saved_platform.is_none());
        assert!(
            GlobalConfig::local_api_key(&environment)
                .expect("key")
                .is_some()
        );
    }

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
        // An existing config is owner-only, as the store requires; `fs::write`
        // follows the umask and may leave it group-writable.
        #[cfg(unix)]
        fs::set_permissions(
            dir.path().join("config.toml"),
            <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
        )
        .expect("owner-only config fixture");
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
