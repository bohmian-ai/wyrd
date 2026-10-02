//! Top-level client configuration.
//!
//! [`ClientConfig`] is the single entry point for configuring `wyrd-client`.
//! Build from the global profile with [`ClientConfig::from_global`] or from
//! environment variables with [`ClientConfig::from_env`], optionally set
//! [`ClientConfig::credential`] for an explicit credential that outranks env and
//! file credentials, then call [`ClientConfig::resolve_credential`] to obtain
//! the effective [`ResolvedCredential`].

use std::path::PathBuf;

use secrecy::SecretString;

use crate::auth::TokenExchange;
use crate::error::WyrdClientError;
use crate::global_config::{GlobalConfig, TokenCacheKind};
use crate::saved_login::{SavedLogins, canonical_origin};
use crate::transport::{
    config::{GrpcConfig, HTTP_DEFAULT_BASE_URL, HttpConfig, grpc_endpoint_for},
    credential::{CredentialChain, CredentialSource, ResolvedCredential},
};

/// Token cache strategy.
///
/// Declared here (commit 04); behavior is implemented in commit 05.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TokenCacheMode {
    /// Cache resolved tokens in memory only (default).
    #[default]
    InMemory,
    /// Persist resolved tokens to disk for reuse across process restarts.
    Disk,
}

/// Top-level Wyrd client configuration.
///
/// Fields are public to allow fine-grained programmatic construction.
/// Call [`ClientConfig::from_env`] to build from environment variables with
/// defaults, then adjust fields as needed before calling
/// [`ClientConfig::resolve_credential`].
#[derive(Default)]
pub struct ClientConfig {
    /// gRPC transport configuration.
    pub grpc: GrpcConfig,
    /// HTTP transport configuration.
    pub http: HttpConfig,
    /// Explicit credential.
    ///
    /// When set, this value is prepended to the credential chain as tier 0 and
    /// always wins over `WYRD_API_KEY`, `WYRD_ACCESS_TOKEN`, and the
    /// `credentials.toml` floor.
    pub credential: Option<SecretString>,
    /// Optional tenant selector: the slug paired with workload identity
    /// credentials, and the tenant slug or id that picks one saved user login
    /// for this server.
    pub tenant: Option<String>,
    /// Token cache mode.
    pub token_cache: TokenCacheMode,
    /// Path used by the disk token cache.
    pub token_cache_path: Option<PathBuf>,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("grpc", &self.grpc)
            .field("http", &self.http)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "[REDACTED]"),
            )
            .field("tenant", &self.tenant)
            .field("token_cache", &self.token_cache)
            .field("token_cache_path", &self.token_cache_path)
            .finish()
    }
}

impl ClientConfig {
    /// Build from environment variables and built-in defaults.
    ///
    /// - `WYRD_SERVER_URL` overrides the HTTP base URL (default:
    ///   `http://localhost:8080`).
    /// - `WYRD_GRPC_URL` overrides the gRPC endpoint (default: the server
    ///   URL's scheme and host on the public gRPC port `50051`).
    ///
    /// The [`ClientConfig::credential`] field is left `None`; set it
    /// explicitly after construction to make it the highest-priority
    /// credential source.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_global_with_env(&GlobalConfig::default())
    }

    /// Build from the global config file, environment variables, and defaults.
    ///
    /// # Errors
    /// Returns an error when the global config file exists but cannot be read
    /// or parsed.
    pub fn from_global() -> Result<Self, WyrdClientError> {
        Ok(Self::from_global_with_env(&GlobalConfig::load()?))
    }

    /// Overlay global config values with environment values and defaults.
    #[must_use]
    pub fn from_global_with_env(global: &GlobalConfig) -> Self {
        Self::from_global_with_overrides(global, None, None)
    }

    /// Overlay explicit endpoints over global config, environment, and defaults.
    ///
    /// Each endpoint resolves explicit value, then global config, then its
    /// environment variable. The HTTP base URL then falls back to its default;
    /// the gRPC endpoint falls back to [`grpc_endpoint_for`] of the *effective*
    /// HTTP base URL, so re-pointing only `server_url` re-points gRPC with it.
    #[must_use]
    pub fn from_global_with_overrides(
        global: &GlobalConfig,
        server_url: Option<&str>,
        grpc_url: Option<&str>,
    ) -> Self {
        let http_base_url = server_url
            .map(|url| url.trim_end_matches('/').to_owned())
            .or_else(|| global.client.http_url.clone())
            .or_else(|| std::env::var("WYRD_SERVER_URL").ok())
            .unwrap_or_else(|| HTTP_DEFAULT_BASE_URL.to_string());
        let grpc_endpoint = grpc_url
            .map(str::to_owned)
            .or_else(|| global.client.grpc_url.clone())
            .or_else(|| std::env::var("WYRD_GRPC_URL").ok())
            .unwrap_or_else(|| grpc_endpoint_for(&http_base_url));
        let tenant = global
            .client
            .tenant
            .clone()
            .or_else(|| std::env::var("WYRD_TENANT").ok());
        let configured_cache = global.client.token_cache.as_ref();
        let token_cache = configured_cache
            .and_then(|cache| cache.kind)
            .or_else(|| match std::env::var("WYRD_TOKEN_CACHE").ok().as_deref() {
                Some("disk") => Some(TokenCacheKind::Disk),
                Some("in_memory") => Some(TokenCacheKind::InMemory),
                _ => None,
            })
            .map_or(TokenCacheMode::InMemory, |kind| match kind {
                TokenCacheKind::InMemory => TokenCacheMode::InMemory,
                TokenCacheKind::Disk => TokenCacheMode::Disk,
            });
        let token_cache_path = configured_cache
            .and_then(|cache| cache.path.clone())
            .or_else(|| {
                std::env::var("WYRD_TOKEN_CACHE_PATH")
                    .ok()
                    .map(PathBuf::from)
            })
            .or_else(|| wyrd_utils::config_dir::wyrd_config_dir().map(|path| path.join("tokens")));

        Self {
            grpc: GrpcConfig {
                endpoint: grpc_endpoint,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: http_base_url,
                ..HttpConfig::default()
            },
            credential: None,
            tenant,
            token_cache,
            token_cache_path,
        }
    }

    /// Resolve the effective credential.
    ///
    /// Precedence (lowest index wins):
    /// 1. `self.credential` — explicit credential set by caller
    /// 2. `WYRD_ACCESS_TOKEN` — tier 1 env
    /// 3. `WYRD_WORKLOAD_TOKEN` + `WYRD_TENANT` — tier 2 env
    /// 4. `WYRD_API_KEY` — tier 3 env
    /// 5. the saved user login `wyrd auth login` wrote for this server origin
    ///    and `self.tenant` ([`SavedLogins::select`]), renewed in place
    /// 6. `~/.config/wyrd/credentials.toml` `[default].api_key` — file floor
    ///
    /// A `credentials.toml` that is unsafe or corrupt, or saved logins that
    /// are ambiguous or have no record for the selected tenant, fail here
    /// instead of falling through to the floor, so a person's selection is
    /// never silently replaced by another identity.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] for the saved-login refusals
    /// above, [`WyrdClientError::Config`] for an unparsable server URL, and
    /// [`WyrdClientError::NoCredentials`] when no tier yields a credential.
    pub fn resolve_credential(&self) -> Result<ResolvedCredential, WyrdClientError> {
        let mut chain = CredentialChain::default();
        if let Some(credential) = &self.credential {
            chain.push(CredentialSource::explicit(credential.clone()));
        }
        chain.extend(CredentialChain::env_only(self.tenant.as_deref()));
        if !chain.is_empty() {
            return chain.resolve();
        }
        if let Some(store) = SavedLogins::locate() {
            let origin = canonical_origin(&self.http.base_url)?;
            if let Some(login) = store.select(&origin, self.tenant.as_deref())? {
                let exchange = TokenExchange::new(&self.http.base_url, self.http.timeout_ms)?;
                return Ok(ResolvedCredential::Renewable(
                    store.source(&login, exchange),
                ));
            }
        }
        CredentialChain::credentials_file().resolve()
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    /// Credential fixture used by the precedence tests.
    ///
    /// The precedence tests assert that an explicit credential wins, so the
    /// fixture must be something the server-side parser would accept; a
    /// prose placeholder would classify as a bearer token instead.
    const API_KEY_FIXTURE: &str =
        "wyrd_sk_4d5e1c3a9b7f4e2d8a6c0b1e2f3a4b5c_1a2b3c4d_9f8e7d6c5b4a39281706f5e4d3c2b1a0";

    use std::str::FromStr;

    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::TenantSlug;

    use crate::global_config::{ClientSection, GlobalConfig};
    use crate::saved_login::{
        SAVED_LOGIN_FORMAT_VERSION, SavedLogin, SavedLoginState, SavedLogins,
    };

    use super::{ClientConfig, TokenCacheMode};
    use crate::transport::{
        config::{GRPC_DEFAULT_ENDPOINT, HTTP_DEFAULT_BASE_URL},
        credential::ResolvedCredential,
    };

    #[test]
    fn from_env_uses_defaults_when_no_env_vars() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
            std::env::remove_var("WYRD_SERVER_URL");
        }

        let cfg = ClientConfig::from_env();

        assert_eq!(cfg.grpc.endpoint, GRPC_DEFAULT_ENDPOINT);
        assert_eq!(cfg.http.base_url, HTTP_DEFAULT_BASE_URL);
        assert_eq!(cfg.token_cache, TokenCacheMode::InMemory);
        assert!(cfg.credential.is_none());
    }

    #[test]
    fn file_values_beat_environment_values() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX serializes environment mutation in this test binary.
        unsafe {
            std::env::set_var("WYRD_GRPC_URL", "environment-grpc");
            std::env::set_var("WYRD_SERVER_URL", "environment-http");
            std::env::set_var("WYRD_TENANT", "environment-tenant");
        }

        let config = GlobalConfig {
            client: ClientSection {
                grpc_url: Some("file-grpc".to_owned()),
                http_url: Some("file-http".to_owned()),
                tenant: Some("file-tenant".to_owned()),
                token_cache: None,
            },
        };
        let resolved = ClientConfig::from_global_with_env(&config);

        // SAFETY: ENV_MUTEX serializes environment mutation in this test binary.
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
            std::env::remove_var("WYRD_SERVER_URL");
            std::env::remove_var("WYRD_TENANT");
        }

        assert_eq!(resolved.grpc.endpoint, "file-grpc");
        assert_eq!(resolved.http.base_url, "file-http");
        assert_eq!(resolved.tenant.as_deref(), Some("file-tenant"));
    }

    #[test]
    fn missing_file_values_fall_back_to_environment() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX serializes environment mutation in this test binary.
        unsafe {
            std::env::set_var("WYRD_GRPC_URL", "environment-grpc");
            std::env::remove_var("WYRD_SERVER_URL");
            std::env::set_var("WYRD_TENANT", "environment-tenant");
        }

        let resolved = ClientConfig::from_global_with_env(&GlobalConfig::default());

        // SAFETY: ENV_MUTEX serializes environment mutation in this test binary.
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
            std::env::remove_var("WYRD_TENANT");
        }

        assert_eq!(resolved.grpc.endpoint, "environment-grpc");
        assert_eq!(resolved.http.base_url, HTTP_DEFAULT_BASE_URL);
        assert_eq!(resolved.tenant.as_deref(), Some("environment-tenant"));
    }

    #[test]
    fn wyrd_grpc_url_overrides_default_endpoint() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_GRPC_URL", "https://grpc.example.com:443");
        }
        let cfg = ClientConfig::from_env();
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
        }

        assert_eq!(cfg.grpc.endpoint, "https://grpc.example.com:443");
    }

    /// A client given only `server_url` dials gRPC on the same scheme and host
    /// at the public port; an explicit gRPC URL still wins.
    #[test]
    fn grpc_endpoint_derives_from_server_url_unless_overridden() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
            std::env::remove_var("WYRD_SERVER_URL");
        }
        let global = GlobalConfig::default();

        let derived = ClientConfig::from_global_with_overrides(
            &global,
            Some("https://wyrd.example.com/"),
            None,
        );
        let overridden = ClientConfig::from_global_with_overrides(
            &global,
            Some("https://wyrd.example.com"),
            Some("https://grpc.example.com:443"),
        );

        assert_eq!(derived.http.base_url, "https://wyrd.example.com");
        assert_eq!(derived.grpc.endpoint, "https://wyrd.example.com:50051");
        assert_eq!(overridden.grpc.endpoint, "https://grpc.example.com:443");
        assert_eq!(
            ClientConfig::from_env().grpc.endpoint,
            GRPC_DEFAULT_ENDPOINT
        );
    }

    #[test]
    fn wyrd_server_url_overrides_default_base_url() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_SERVER_URL", "https://api.example.com");
        }
        let cfg = ClientConfig::from_env();
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_SERVER_URL");
        }

        assert_eq!(cfg.http.base_url, "https://api.example.com");
    }

    #[test]
    fn no_credentials_when_chain_empty() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_TENANT");
            std::env::remove_var("WYRD_API_KEY");
            // Redirect HOME so no credentials.toml is found.
            std::env::set_var("HOME", std::env::temp_dir());
        }

        let cfg = ClientConfig::from_env();
        let result = cfg.resolve_credential();

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("HOME");
        }

        assert!(result.is_err(), "empty chain must return NoCredentials");
    }

    #[test]
    fn env_api_key_resolves_when_no_explicit() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_API_KEY", "env_api_key_value");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
        }

        let cfg = ClientConfig::from_env();
        let cred = cfg.resolve_credential().expect("resolves from env");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_API_KEY");
        }

        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(k.expose_secret(), "env_api_key_value");
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn explicit_credential_beats_env_wyrd_api_key() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_API_KEY", "env_key_should_lose");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
        }

        let mut cfg = ClientConfig::from_env();
        cfg.credential = Some(API_KEY_FIXTURE.to_owned().into());

        let cred = cfg.resolve_credential().expect("explicit key resolves");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_API_KEY");
        }

        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(
                    k.expose_secret(),
                    API_KEY_FIXTURE,
                    "an explicit credential must beat ambient WYRD_API_KEY"
                );
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn workload_token_beats_env_api_key() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_WORKLOAD_TOKEN", "workload-jwt");
            std::env::set_var("WYRD_TENANT", "acme");
            std::env::set_var("WYRD_API_KEY", "api-key-should-lose");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
        }

        let cfg = ClientConfig::from_env();
        let cred = cfg.resolve_credential().expect("resolves from env");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_TENANT");
            std::env::remove_var("WYRD_API_KEY");
        }

        match cred {
            ResolvedCredential::WorkloadJwt { tenant, .. } => {
                assert_eq!(
                    tenant, "acme",
                    "WYRD_WORKLOAD_TOKEN must outrank WYRD_API_KEY"
                );
            }
            other => panic!("expected WorkloadJwt, got {other:?}"),
        }
    }

    #[test]
    fn explicit_access_token_beats_workload_token() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_ACCESS_TOKEN", "real-access-token");
            std::env::set_var("WYRD_WORKLOAD_TOKEN", "workload-should-lose");
            std::env::set_var("WYRD_TENANT", "acme");
            std::env::remove_var("WYRD_API_KEY");
        }

        let cfg = ClientConfig::from_env();
        let cred = cfg.resolve_credential().expect("resolves from env");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_TENANT");
        }

        match cred {
            ResolvedCredential::BearerToken(token) => {
                assert_eq!(
                    token.expose_secret(),
                    "real-access-token",
                    "WYRD_ACCESS_TOKEN must outrank WYRD_WORKLOAD_TOKEN"
                );
            }
            other => panic!("expected BearerToken, got {other:?}"),
        }
    }

    #[test]
    fn explicit_credential_beats_workload_token() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_WORKLOAD_TOKEN", "workload-should-lose");
            std::env::set_var("WYRD_TENANT", "acme");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_API_KEY");
        }

        let mut cfg = ClientConfig::from_env();
        cfg.credential = Some(API_KEY_FIXTURE.to_owned().into());
        let cred = cfg.resolve_credential().expect("explicit key resolves");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_TENANT");
        }

        match cred {
            ResolvedCredential::ApiKey(key) => {
                assert_eq!(
                    key.expose_secret(),
                    API_KEY_FIXTURE,
                    "an explicit credential must outrank WYRD_WORKLOAD_TOKEN"
                );
            }
            other => panic!("expected ApiKey, got {other:?}"),
        }
    }

    #[test]
    fn workload_token_without_tenant_yields_no_source() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_WORKLOAD_TOKEN", "workload-jwt");
            std::env::remove_var("WYRD_TENANT");
            std::env::set_var("WYRD_API_KEY", "api-key-fallback");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
        }

        let cfg = ClientConfig::from_env();
        let cred = cfg.resolve_credential().expect("falls through to api key");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_API_KEY");
        }

        match cred {
            ResolvedCredential::ApiKey(key) => {
                assert_eq!(
                    key.expose_secret(),
                    "api-key-fallback",
                    "WYRD_WORKLOAD_TOKEN without WYRD_TENANT must yield no source"
                );
            }
            other => panic!("expected ApiKey fallback, got {other:?}"),
        }
    }

    #[test]
    fn debug_does_not_leak_credential() {
        let cfg = ClientConfig {
            credential: Some("top-secret".to_owned().into()),
            ..Default::default()
        };

        let debug = format!("{cfg:?}");

        assert!(
            !debug.contains("top-secret"),
            "Debug must not expose the raw credential"
        );
        assert!(debug.contains("REDACTED"));
    }

    /// A saved user login ranks below every environment tier and above the
    /// `credentials.toml` floor, a `credentials.toml` other users can read
    /// fails closed, and a tenant selector naming no saved login for the
    /// server fails instead of falling through to the floor.
    #[cfg(unix)]
    #[test]
    fn saved_login_ranks_between_env_and_credentials_file() {
        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let home = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            home.path().join("credentials.toml"),
            format!("[default]\napi_key = \"{API_KEY_FIXTURE}\"\n"),
        )
        .expect("writes floor");
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_CONFIG_HOME", home.path());
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_API_KEY");
        }
        let mut cfg = ClientConfig::from_env();
        cfg.http.base_url = "https://wyrd.example.com/".to_owned();
        cfg.tenant = None;
        let chmod = |mode: u32| {
            std::fs::set_permissions(
                home.path().join("credentials.toml"),
                <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(mode),
            )
            .expect("chmod");
        };
        chmod(0o644);
        let exposed = cfg.resolve_credential().map(|_| ());
        chmod(0o600);
        let floor = cfg.resolve_credential().expect("floor resolves");

        let store = SavedLogins::locate().expect("config home");
        let tenant_key = TenantSlug::from_str("acme").expect("slug");
        store
            .save(SavedLogin {
                format_version: SAVED_LOGIN_FORMAT_VERSION,
                origin: "https://wyrd.example.com".to_owned(),
                tenant_id: DataTenantId::new_v7(),
                tenant_key,
                principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                generation: 1,
                state: SavedLoginState::LoggedOut,
            })
            .expect("saves");
        let saved = cfg.resolve_credential().expect("saved login resolves");
        cfg.tenant = Some("globex".to_owned());
        let mismatch = cfg.resolve_credential().map(|_| ());
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("WYRD_API_KEY", "env_api_key_value");
        }
        let env = cfg.resolve_credential().expect("env resolves");
        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("WYRD_API_KEY");
            std::env::remove_var("WYRD_CONFIG_HOME");
        }

        let error = exposed.expect_err("a credential file others can read fails closed");
        assert!(error.to_string().contains("unsafe_store"), "{error}");
        assert!(matches!(floor, ResolvedCredential::ApiKey(_)), "{floor:?}");
        match saved {
            ResolvedCredential::Renewable(source) => {
                assert!(
                    source
                        .identity()
                        .starts_with("saved-login:https://wyrd.example.com:")
                );
            }
            other => panic!("expected the saved login, got {other:?}"),
        }
        let error = mismatch.expect_err("tenant mismatch fails closed");
        assert!(error.to_string().contains("tenant_mismatch"), "{error}");
        assert!(
            matches!(&env, ResolvedCredential::ApiKey(key) if key.expose_secret() == "env_api_key_value"),
            "{env:?}"
        );
    }
}
