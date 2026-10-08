//! Top-level client configuration.
//!
//! [`ClientConfig`] is the single entry point for configuring `wyrd-client`.
//! Build from the global profile with [`ClientConfig::from_global`] or from
//! environment variables with [`ClientConfig::from_env`], optionally set
//! [`ClientConfig::credential`] for an explicit credential that outranks env and
//! file credentials, then call [`ClientConfig::resolve_credential`] to obtain
//! the effective [`ResolvedCredential`].

use secrecy::SecretString;

use crate::auth::TokenExchange;
use crate::environment::Environment;
use crate::error::WyrdClientError;
use crate::global_config::{GlobalConfig, TokenCacheKind};
use crate::saved_login::{SavedLogins, canonical_origin};
use crate::transport::{
    config::{GrpcConfig, HTTP_DEFAULT_BASE_URL, HttpConfig, grpc_endpoint_for},
    credential::{CredentialChain, CredentialSource, ResolvedCredential},
};

/// Where an API key's exchanged access token is cached.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TokenCacheMode {
    /// Cache resolved tokens in memory only (default).
    #[default]
    InMemory,
    /// Also cache an API key's access token in `credentials.toml`, beside
    /// that `[default].api_key`, so every process using the key reuses it.
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
    /// Optional tenant selector: a tenant route key, the one `wyrd auth login`
    /// takes. It picks one saved user login for this server and is the tenant
    /// a workload token's exchange asks for, ahead of an ambient
    /// `WYRD_TENANT`. Without it, the newest saved login for this server is
    /// used; with it, a server whose saved logins all belong to other tenants
    /// is refused with `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE`. A bearer or API
    /// key already names its tenant, so resolving one beside a selector is
    /// refused.
    pub tenant: Option<String>,
    /// Token cache mode.
    pub token_cache: TokenCacheMode,
    /// Where [`ClientConfig::resolve_credential`] reads the environment
    /// credential tiers and the configuration directory holding saved logins
    /// and `credentials.toml`. Defaults to the process environment.
    pub environment: Environment,
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
            .field("environment", &self.environment)
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
    ///
    /// # Arguments
    /// * `global` - Loaded global config whose values take precedence over environment
    ///   variables.
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
    ///
    /// # Arguments
    /// * `global` - Loaded global config consulted after the explicit endpoints.
    /// * `server_url` - Explicit HTTP base URL; `None` falls back to global config,
    ///   `WYRD_SERVER_URL`, then the default.
    /// * `grpc_url` - Explicit gRPC endpoint; `None` falls back to global config,
    ///   `WYRD_GRPC_URL`, then one derived from the HTTP base URL.
    #[must_use]
    pub fn from_global_with_overrides(
        global: &GlobalConfig,
        server_url: Option<&str>,
        grpc_url: Option<&str>,
    ) -> Self {
        Self::from_environment(Environment::Process, global, server_url, grpc_url)
    }

    /// Overlay explicit endpoints over global config, `environment`, and
    /// defaults, and keep `environment` for credential resolution.
    ///
    /// Resolution follows [`ClientConfig::from_global_with_overrides`] with
    /// every ambient value read from `environment` instead of the process.
    ///
    /// # Arguments
    /// * `environment` - Source of every ambient variable, retained for credential resolution.
    /// * `global` - Loaded global config consulted after the explicit endpoints.
    /// * `server_url` - Explicit HTTP base URL; `None` falls back to global config,
    ///   `WYRD_SERVER_URL`, then the default.
    /// * `grpc_url` - Explicit gRPC endpoint; `None` falls back to global config,
    ///   `WYRD_GRPC_URL`, then one derived from the HTTP base URL.
    #[must_use]
    pub fn from_environment(
        environment: Environment,
        global: &GlobalConfig,
        server_url: Option<&str>,
        grpc_url: Option<&str>,
    ) -> Self {
        let http_base_url = server_url
            .map(|url| url.trim_end_matches('/').to_owned())
            .or_else(|| global.client.http_url.clone())
            .or_else(|| environment.var("WYRD_SERVER_URL"))
            .unwrap_or_else(|| HTTP_DEFAULT_BASE_URL.to_string());
        let grpc_endpoint = grpc_url
            .map(str::to_owned)
            .or_else(|| global.client.grpc_url.clone())
            .or_else(|| environment.var("WYRD_GRPC_URL"))
            .unwrap_or_else(|| grpc_endpoint_for(&http_base_url));
        let tenant = global
            .client
            .tenant
            .clone()
            .or_else(|| environment.var("WYRD_TENANT"));
        let configured_cache = global.client.token_cache.as_ref();
        let token_cache = configured_cache
            .and_then(|cache| cache.kind)
            .or_else(|| match environment.var("WYRD_TOKEN_CACHE").as_deref() {
                Some("disk") => Some(TokenCacheKind::Disk),
                Some("in_memory") => Some(TokenCacheKind::InMemory),
                _ => None,
            })
            .map_or(TokenCacheMode::InMemory, |kind| match kind {
                TokenCacheKind::InMemory => TokenCacheMode::InMemory,
                TokenCacheKind::Disk => TokenCacheMode::Disk,
            });

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
            environment,
        }
    }

    /// Resolve the effective credential.
    ///
    /// Precedence (lowest index wins):
    /// 1. `self.credential` — explicit credential set by caller
    /// 2. `WYRD_ACCESS_TOKEN` — tier 1 env
    /// 3. `WYRD_WORKLOAD_TOKEN` + `self.tenant`, else `WYRD_TENANT` — tier 2 env
    /// 4. `WYRD_API_KEY` — tier 3 env
    /// 5. the saved user login `wyrd auth login` wrote for this server origin
    ///    ([`SavedLogins::select`]): the one for `self.tenant` when it is set,
    ///    otherwise the newest login for the server; renewed in place
    /// 6. `~/.config/wyrd/credentials.toml` `[default].api_key` — file floor
    ///
    /// A `credentials.toml` that is unsafe or corrupt, or a selected tenant
    /// with no saved login when the server has others, fails here instead of
    /// falling through to the floor, so a person's selection is never
    /// silently replaced by another identity. A tenant selector beside
    /// a bearer or API key (tiers 1, 2, 4, and 6) is refused rather than
    /// ignored, because that credential already names its tenant.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::SavedLogin`] for the saved-login refusals
    /// above, [`WyrdClientError::Config`] for an unparsable server URL or a
    /// tenant selector beside a bearer or API key, and
    /// [`WyrdClientError::NoCredentials`] when no tier yields a credential.
    pub fn resolve_credential(&self) -> Result<ResolvedCredential, WyrdClientError> {
        let mut chain = CredentialChain::default();
        if let Some(credential) = &self.credential {
            chain.push(CredentialSource::explicit(credential.clone()));
        }
        chain.extend(CredentialChain::env_only(
            &self.environment,
            self.tenant.as_deref(),
        ));
        if !chain.is_empty() {
            return self.refuse_selector(chain.resolve()?);
        }
        if let Some(store) = self.environment.config_dir().map(SavedLogins::at) {
            let origin = canonical_origin(&self.http.base_url)?;
            if let Some(login) = store.select(&origin, self.tenant.as_deref())? {
                let exchange = TokenExchange::new(&self.http.base_url, self.http.timeout_ms)?;
                return Ok(ResolvedCredential::Renewable(
                    store.renewing_source(&login, exchange),
                ));
            }
        }
        self.refuse_selector(CredentialChain::credentials_file(&self.environment).resolve()?)
    }

    /// Pass `credential` through unless it is a bearer or API key resolved
    /// beside a non-empty tenant selector.
    ///
    /// Such a credential is bound to its tenant by the server, so the
    /// selector could only be ignored or contradicted; refusing keeps a
    /// person's selection from silently meaning nothing.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] on field `tenant` for a bearer or
    /// API key with a selector set.
    fn refuse_selector(
        &self,
        credential: ResolvedCredential,
    ) -> Result<ResolvedCredential, WyrdClientError> {
        let selector = self.tenant.as_deref().filter(|tenant| !tenant.is_empty());
        match (&credential, selector) {
            (ResolvedCredential::BearerToken(_) | ResolvedCredential::ApiKey(_), Some(tenant)) => {
                Err(WyrdClientError::Config {
                    field: "tenant".to_owned(),
                    reason: format!(
                        "this credential already names its tenant; remove the tenant selector \
                         {tenant} (client tenant option, client.tenant, or WYRD_TENANT)"
                    ),
                })
            }
            _ => Ok(credential),
        }
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

    use wyrd_spec::auth::SecretBearer;
    use wyrd_spec::ids::TenantSlug;

    use crate::environment::Environment;
    use crate::global_config::{ClientSection, GlobalConfig, LocalWorkflowConfig};
    use crate::saved_login::{SavedLogin, SavedLogins};

    use super::{ClientConfig, TokenCacheMode};
    use crate::transport::{
        config::{GRPC_DEFAULT_ENDPOINT, HTTP_DEFAULT_BASE_URL},
        credential::ResolvedCredential,
    };

    /// A config built from defaults and exactly `environment`.
    fn config_in(environment: Environment) -> ClientConfig {
        ClientConfig::from_environment(environment, &GlobalConfig::default(), None, None)
    }

    /// With no variables set, every endpoint and mode takes its default.
    #[test]
    fn from_env_uses_defaults_when_no_env_vars() {
        let cfg = config_in(Environment::from([]));

        assert_eq!(cfg.grpc.endpoint, GRPC_DEFAULT_ENDPOINT);
        assert_eq!(cfg.http.base_url, HTTP_DEFAULT_BASE_URL);
        assert_eq!(cfg.token_cache, TokenCacheMode::InMemory);
        assert!(cfg.credential.is_none());
    }

    /// Global config file values outrank environment values.
    #[test]
    fn file_values_beat_environment_values() {
        let environment = Environment::from([
            ("WYRD_GRPC_URL", "environment-grpc"),
            ("WYRD_SERVER_URL", "environment-http"),
            ("WYRD_TENANT", "environment-tenant"),
        ]);
        let config = GlobalConfig {
            client: ClientSection {
                grpc_url: Some("file-grpc".to_owned()),
                http_url: Some("file-http".to_owned()),
                tenant: Some("file-tenant".to_owned()),
                token_cache: None,
            },
            workflow: LocalWorkflowConfig::default(),
        };

        let resolved = ClientConfig::from_environment(environment, &config, None, None);

        assert_eq!(resolved.grpc.endpoint, "file-grpc");
        assert_eq!(resolved.http.base_url, "file-http");
        assert_eq!(resolved.tenant.as_deref(), Some("file-tenant"));
    }

    /// Values the global config omits fall back to the environment.
    #[test]
    fn missing_file_values_fall_back_to_environment() {
        let resolved = config_in(Environment::from([
            ("WYRD_GRPC_URL", "environment-grpc"),
            ("WYRD_TENANT", "environment-tenant"),
        ]));

        assert_eq!(resolved.grpc.endpoint, "environment-grpc");
        assert_eq!(resolved.http.base_url, HTTP_DEFAULT_BASE_URL);
        assert_eq!(resolved.tenant.as_deref(), Some("environment-tenant"));
    }

    /// `WYRD_GRPC_URL` replaces the default gRPC endpoint.
    #[test]
    fn wyrd_grpc_url_overrides_default_endpoint() {
        let cfg = config_in(Environment::from([(
            "WYRD_GRPC_URL",
            "https://grpc.example.com:443",
        )]));

        assert_eq!(cfg.grpc.endpoint, "https://grpc.example.com:443");
    }

    /// A client given only `server_url` dials gRPC on the same scheme and host
    /// at the public port; an explicit gRPC URL still wins.
    #[test]
    fn grpc_endpoint_derives_from_server_url_unless_overridden() {
        let global = GlobalConfig::default();

        let derived = ClientConfig::from_environment(
            Environment::from([]),
            &global,
            Some("https://wyrd.example.com/"),
            None,
        );
        let overridden = ClientConfig::from_environment(
            Environment::from([]),
            &global,
            Some("https://wyrd.example.com"),
            Some("https://grpc.example.com:443"),
        );

        assert_eq!(derived.http.base_url, "https://wyrd.example.com");
        assert_eq!(derived.grpc.endpoint, "https://wyrd.example.com:50051");
        assert_eq!(overridden.grpc.endpoint, "https://grpc.example.com:443");
        assert_eq!(
            config_in(Environment::from([])).grpc.endpoint,
            GRPC_DEFAULT_ENDPOINT
        );
    }

    /// `WYRD_SERVER_URL` replaces the default HTTP base URL.
    #[test]
    fn wyrd_server_url_overrides_default_base_url() {
        let cfg = config_in(Environment::from([(
            "WYRD_SERVER_URL",
            "https://api.example.com",
        )]));

        assert_eq!(cfg.http.base_url, "https://api.example.com");
    }

    /// No environment tier, saved login, or `credentials.toml` resolves to
    /// `NoCredentials`.
    #[test]
    fn no_credentials_when_chain_empty() {
        let home = crate::credentials_file::private_tempdir();
        let cfg = config_in(Environment::from([(
            "WYRD_CONFIG_HOME",
            home.path().to_str().expect("utf-8 path"),
        )]));

        let result = cfg.resolve_credential();

        assert!(result.is_err(), "empty chain must return NoCredentials");
    }

    /// `WYRD_API_KEY` resolves when no explicit credential is set.
    #[test]
    fn env_api_key_resolves_when_no_explicit() {
        let cfg = config_in(Environment::from([("WYRD_API_KEY", "env_api_key_value")]));

        let cred = cfg.resolve_credential().expect("resolves from env");

        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(k.expose_secret(), "env_api_key_value");
            }
            _ => panic!("expected ApiKey"),
        }
    }

    /// An explicit credential outranks `WYRD_API_KEY`.
    #[test]
    fn explicit_credential_beats_env_wyrd_api_key() {
        let mut cfg = config_in(Environment::from([("WYRD_API_KEY", "env_key_should_lose")]));
        cfg.credential = Some(API_KEY_FIXTURE.to_owned().into());

        let cred = cfg.resolve_credential().expect("explicit key resolves");

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

    /// `WYRD_WORKLOAD_TOKEN` with its tenant outranks `WYRD_API_KEY`.
    #[test]
    fn workload_token_beats_env_api_key() {
        let mut cfg = config_in(Environment::from([
            ("WYRD_WORKLOAD_TOKEN", "workload-jwt"),
            ("WYRD_TENANT", "acme"),
            ("WYRD_API_KEY", "api-key-should-lose"),
        ]));
        // Workload routing reads the ambient tenant; no selector is set.
        cfg.tenant = None;

        let cred = cfg.resolve_credential().expect("resolves from env");

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

    /// `WYRD_ACCESS_TOKEN` outranks `WYRD_WORKLOAD_TOKEN`.
    #[test]
    fn explicit_access_token_beats_workload_token() {
        let mut cfg = config_in(Environment::from([
            ("WYRD_ACCESS_TOKEN", "real-access-token"),
            ("WYRD_WORKLOAD_TOKEN", "workload-should-lose"),
            ("WYRD_TENANT", "acme"),
        ]));
        // Workload routing reads the ambient tenant; no selector is set.
        cfg.tenant = None;

        let cred = cfg.resolve_credential().expect("resolves from env");

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

    /// An explicit credential outranks `WYRD_WORKLOAD_TOKEN`.
    #[test]
    fn explicit_credential_beats_workload_token() {
        let mut cfg = config_in(Environment::from([
            ("WYRD_WORKLOAD_TOKEN", "workload-should-lose"),
            ("WYRD_TENANT", "acme"),
        ]));
        // Workload routing reads the ambient tenant; no selector is set.
        cfg.tenant = None;
        cfg.credential = Some(API_KEY_FIXTURE.to_owned().into());

        let cred = cfg.resolve_credential().expect("explicit key resolves");

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

    /// A workload token with no tenant is no source, so the chain falls
    /// through to `WYRD_API_KEY`.
    #[test]
    fn workload_token_without_tenant_yields_no_source() {
        let cfg = config_in(Environment::from([
            ("WYRD_WORKLOAD_TOKEN", "workload-jwt"),
            ("WYRD_API_KEY", "api-key-fallback"),
        ]));

        let cred = cfg.resolve_credential().expect("falls through to api key");

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
        let home = crate::credentials_file::private_tempdir();
        let config_home = home.path().to_str().expect("utf-8 path");
        std::fs::write(
            home.path().join("credentials.toml"),
            format!("[default]\napi_key = \"{API_KEY_FIXTURE}\"\n"),
        )
        .expect("writes floor");
        let mut cfg = config_in(Environment::from([("WYRD_CONFIG_HOME", config_home)]));
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

        let store = SavedLogins::at(home.path().to_path_buf());
        let tenant_key = TenantSlug::from_str("acme").expect("slug");
        store
            .save(SavedLogin {
                origin: "https://wyrd.example.com".to_owned(),
                tenant_key,
                access_token: SecretBearer::new("access".to_owned()),
                access_expires_at: chrono::Utc::now(),
                refresh_token: SecretBearer::new("refresh".to_owned()),
            })
            .expect("saves");
        let saved = cfg.resolve_credential().expect("saved login resolves");
        cfg.tenant = Some("globex".to_owned());
        let mismatch = cfg.resolve_credential().map(|_| ());
        cfg.tenant = None;
        cfg.environment = Environment::from([
            ("WYRD_CONFIG_HOME", config_home),
            ("WYRD_API_KEY", "env_api_key_value"),
        ]);
        let env = cfg.resolve_credential().expect("env resolves");

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

    /// A tenant selector beside a credential that already names its tenant —
    /// an explicit bearer or API key, `WYRD_ACCESS_TOKEN`, `WYRD_API_KEY`, or
    /// the `credentials.toml` floor — is refused during resolution, before any
    /// request, and never falls to another tier; the selector routes a
    /// workload token ahead of an ambient `WYRD_TENANT`.
    ///
    /// # Panics
    /// Panics when a tier accepts a selector or the workload is misrouted.
    #[cfg(unix)]
    #[test]
    fn tenant_selector_is_refused_beside_a_self_naming_credential() {
        let home = crate::credentials_file::private_tempdir();
        let config_home = home.path().to_str().expect("utf-8 path");
        let floor = home.path().join("credentials.toml");
        std::fs::write(
            &floor,
            format!("[default]\napi_key = \"{API_KEY_FIXTURE}\"\n"),
        )
        .expect("writes floor");
        std::fs::set_permissions(
            &floor,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
        )
        .expect("chmod");
        let tiers = [
            ("explicit bearer", Some("explicit-access-token"), None),
            ("explicit API key", Some(API_KEY_FIXTURE), None),
            (
                "WYRD_ACCESS_TOKEN",
                None,
                Some(("WYRD_ACCESS_TOKEN", "env-access-token")),
            ),
            (
                "WYRD_API_KEY",
                None,
                Some(("WYRD_API_KEY", API_KEY_FIXTURE)),
            ),
            ("credentials.toml floor", None, None),
        ];
        let mut outcomes = Vec::new();
        for (tier, credential, env) in tiers {
            let environment = match env {
                Some((name, value)) => {
                    Environment::from([("WYRD_CONFIG_HOME", config_home), (name, value)])
                }
                None => Environment::from([("WYRD_CONFIG_HOME", config_home)]),
            };
            let mut cfg = config_in(environment);
            cfg.http.base_url = "https://wyrd.example.com".to_owned();
            cfg.credential = credential.map(|value| value.to_owned().into());
            let unselected = cfg.resolve_credential().map(|_| ());
            cfg.tenant = Some("acme".to_owned());
            outcomes.push((tier, unselected, cfg.resolve_credential().map(|_| ())));
        }
        let mut cfg = config_in(Environment::from([
            ("WYRD_CONFIG_HOME", config_home),
            ("WYRD_WORKLOAD_TOKEN", "workload-assertion"),
            ("WYRD_TENANT", "globex"),
        ]));
        cfg.tenant = Some("acme".to_owned());
        let routed = cfg.resolve_credential();

        for (tier, unselected, selected) in outcomes {
            unselected.unwrap_or_else(|error| panic!("{tier} resolves alone: {error}"));
            let error = selected.expect_err("a selector beside a self-naming credential");
            assert_eq!(error.code(), "WYRD_CLIENT_400_CONFIG_INVALID", "{tier}");
            assert!(
                error.to_string().contains("already names its tenant"),
                "{tier}: {error}"
            );
        }
        match routed.expect("workload resolves") {
            ResolvedCredential::WorkloadJwt { tenant, .. } => assert_eq!(tenant, "acme"),
            other => panic!("expected WorkloadJwt, got {other:?}"),
        }
    }
}
