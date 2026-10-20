//! ADC-style credential resolution chain for `wyrd-client`.
//!
//! [`CredentialChain`] selects the highest-priority available credential from
//! an ordered list of [`CredentialSource`]s.  The chain is built once at
//! client startup (from env vars, explicit config, or workload metadata) and
//! resolved before each outbound request.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use wyrd_spec::auth::{SecretBearer, TokenAudience};

use crate::auth::AuthMiddleware;
use crate::environment::Environment;
use crate::error::WyrdClientError;

/// One access token minted in-process, with the instant the server stops
/// accepting it.
///
/// `Debug` is redacted through [`SecretBearer`].
#[derive(Debug)]
pub struct MintedAccessToken {
    /// Minted bearer presented on every authenticated request.
    pub access_token: SecretBearer,
    /// Expiry the middleware refreshes ahead of by its fixed skew.
    pub expires_at: DateTime<Utc>,
}

/// In-process minter of short-lived access tokens for one fixed identity.
///
/// An embedding server that signs its own tokens has no durable secret to
/// exchange, so it supplies this instead. [`AuthMiddleware`] caches,
/// proactively refreshes, single-flights, and force-refreshes minted tokens
/// through the same gate as exchanged ones; the source only mints.
///
/// [`AuthMiddleware`]: crate::auth::AuthMiddleware
pub trait AccessTokenSource: Send + Sync {
    /// Stable, non-secret name of the identity every minted token binds.
    ///
    /// Client-side pooling keys on it, so sources minting for different
    /// principals or tenants never share a producer.
    fn identity(&self) -> &str;

    /// Mints one fresh access token.
    ///
    /// # Errors
    ///
    /// Returns a [`WyrdClientError`] when the token cannot be minted.
    fn mint(&self) -> Result<MintedAccessToken, WyrdClientError>;
}

/// One tenant browser session a server-side web host forwards to Wyrd.
///
/// Rust owns browser sessions: the server issued this opaque cookie to a
/// browser at `origin`, and a web host acting for that browser (the Wyrd UI's
/// server, never browser code) presents it on the browser's behalf. Requests
/// carry the one `wyrd_session_<tenant>=<value>` cookie, `Origin`, and the
/// forwarded host and scheme of `origin`, so the server resolves the session
/// against the public origin it was issued for. The cookie is not a bearer:
/// it authenticates the tenant `/v1` HTTP surface only, so gRPC, MCP and
/// token exchange refuse a client built on it. `Debug` is redacted.
#[derive(Clone)]
pub struct BrowserSession {
    /// The single `wyrd_session_<tenant>=<value>` cookie pair.
    cookie: SecretString,
    /// The browser-facing origin the session was issued at, `scheme://host[:port]`.
    origin: reqwest::Url,
}

impl BrowserSession {
    /// Name prefix of every Rust-issued tenant browser-session cookie.
    const PREFIX: &'static str = "wyrd_session_";

    /// Validate one browser-session cookie pair and its public origin.
    ///
    /// `cookie` must be exactly one `wyrd_session_<tenant>=<value>` pair, the
    /// form the server issues, so a host cannot forward several tenants'
    /// sessions or unrelated cookies through it. `origin` must be an `http`
    /// or `https` URL with a host and no userinfo, path, query or fragment.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] naming `browser_session.cookie` or
    /// `browser_session.origin` when either is malformed.
    pub fn new(cookie: SecretString, origin: &str) -> Result<Self, WyrdClientError> {
        let invalid = |field: &str, reason: &str| WyrdClientError::Config {
            field: format!("browser_session.{field}"),
            reason: reason.to_owned(),
        };
        let pair = cookie.expose_secret();
        let well_formed = pair.split_once('=').is_some_and(|(name, value)| {
            name.strip_prefix(Self::PREFIX)
                .is_some_and(|label| !label.is_empty())
                && !value.is_empty()
        }) && pair
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b';' && byte != b',');
        if !well_formed {
            return Err(invalid(
                "cookie",
                "must be exactly one wyrd_session_<tenant>=<value> cookie pair",
            ));
        }
        let parsed = reqwest::Url::parse(origin)
            .map_err(|_| invalid("origin", "must be an absolute http or https URL"))?;
        let bare = matches!(parsed.scheme(), "http" | "https")
            && parsed.host().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.path() == "/"
            && parsed.query().is_none()
            && parsed.fragment().is_none();
        if !bare {
            return Err(invalid(
                "origin",
                "must be scheme://host[:port] with no userinfo, path, query or fragment",
            ));
        }
        Ok(Self {
            cookie,
            origin: parsed,
        })
    }

    /// The request headers that present this session: `Cookie`, `Origin`,
    /// `X-Forwarded-Host` and `X-Forwarded-Proto`.
    ///
    /// # Panics
    /// Never: [`Self::new`] admits only header-safe ASCII values.
    pub(crate) fn headers(&self) -> [(&'static str, reqwest::header::HeaderValue); 4] {
        let value = |text: &str| {
            reqwest::header::HeaderValue::from_str(text)
                .expect("invariant: a validated browser session is header-safe ASCII")
        };
        let mut cookie = value(self.cookie.expose_secret());
        cookie.set_sensitive(true);
        let host = match self.origin.port() {
            Some(port) => format!("{}:{port}", self.origin.host_str().unwrap_or_default()),
            None => self.origin.host_str().unwrap_or_default().to_owned(),
        };
        [
            ("cookie", cookie),
            ("origin", value(&self.origin.origin().ascii_serialization())),
            ("x-forwarded-host", value(&host)),
            ("x-forwarded-proto", value(self.origin.scheme())),
        ]
    }

    /// The secret cookie pair, for credential fingerprinting.
    pub(crate) fn cookie(&self) -> &SecretString {
        &self.cookie
    }
}

impl std::fmt::Debug for BrowserSession {
    /// Shows the public origin only; the cookie is redacted.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserSession")
            .field("cookie", &"[REDACTED]")
            .field("origin", &self.origin.as_str())
            .finish()
    }
}

/// A resolved credential ready to attach to an outbound request.
///
/// Secrets are held as [`SecretString`]; call `.expose_secret()` on the inner
/// value to read them.  `Debug` is redacted — no raw key or token leaks.
#[derive(Clone)]
pub enum ResolvedCredential {
    /// An in-process source minting short-lived access tokens, cached and
    /// refreshed by the middleware and never persisted.
    Renewable(Arc<dyn AccessTokenSource>),
    /// A Wyrd access token (`Bearer <token>`).
    BearerToken(SecretString),
    /// A platform workload JWT to exchange via the `jwt_bearer` grant.
    /// Carries the raw JWT and the tenant slug for routing.
    WorkloadJwt {
        /// Raw JWT from the workload identity provider.
        jwt: SecretString,
        /// Tenant slug for `jwt_bearer` grant routing.
        tenant: String,
    },
    /// A Wyrd API key for the RFC 8693 API-key token exchange.
    ApiKey(SecretString),
    /// An RFC 8693 delegation: `actor` acts for the holder of `subject_token`.
    ///
    /// Built only by [`AuthMiddleware::on_behalf_of`]. The middleware holding
    /// it exchanges the subject token plus the actor's current bearer at
    /// `/auth/token` for a short-lived token bound to `audience`, caching that
    /// result in memory only.
    ///
    /// [`AuthMiddleware::on_behalf_of`]: crate::auth::AuthMiddleware::on_behalf_of
    Delegated {
        /// The inbound access token of the subject being acted for.
        subject_token: SecretString,
        /// The Wyrd surface the delegated token is bound to.
        audience: TokenAudience,
        /// The acting client's own auth path, which supplies the actor token.
        actor: Arc<AuthMiddleware>,
    },
    /// A Rust-owned tenant browser session forwarded by a server-side web
    /// host. It has no bearer: only the HTTP transport presents it.
    BrowserSession(BrowserSession),
}

impl std::fmt::Debug for ResolvedCredential {
    /// Format the credential with every secret replaced by `[REDACTED]`;
    /// only non-secret routing fields (tenant slug, audience) are shown.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Renewable(source) => f
                .debug_struct("Renewable")
                .field("identity", &source.identity())
                .finish(),
            Self::BearerToken(_) => f.debug_tuple("BearerToken").field(&"[REDACTED]").finish(),
            Self::WorkloadJwt { tenant, .. } => f
                .debug_struct("WorkloadJwt")
                .field("jwt", &"[REDACTED]")
                .field("tenant", tenant)
                .finish(),
            Self::ApiKey(_) => f.debug_tuple("ApiKey").field(&"[REDACTED]").finish(),
            Self::Delegated { audience, .. } => f
                .debug_struct("Delegated")
                .field("subject_token", &"[REDACTED]")
                .field("audience", audience)
                .finish_non_exhaustive(),
            Self::BrowserSession(session) => {
                f.debug_tuple("BrowserSession").field(session).finish()
            }
        }
    }
}

/// Source of credentials in the ADC-style resolution chain.
///
/// Sources are checked in priority order (lowest index = highest priority).
/// The first source that produces a credential wins.
///
/// Secrets are held as [`SecretString`].  `Debug` is redacted.
#[derive(Clone)]
pub enum CredentialSource {
    /// Explicitly-configured Wyrd access token.  Tier 1.
    ExplicitToken {
        /// Access token, redacted in `Debug`.
        token: SecretString,
    },
    /// Platform workload identity token exchanged via the `jwt_bearer` grant
    /// at call time.  Tier 2.
    WorkloadToken {
        /// Raw JWT from the workload identity provider, redacted in `Debug`.
        jwt: SecretString,
        /// Tenant slug for `jwt_bearer` exchange routing.
        tenant: String,
    },
    /// API key floor.  Exchanged for a Wyrd access token via the
    /// RFC 8693 API-key token exchange at call time.  Tier 3.
    ApiKey {
        /// Raw API key, redacted in `Debug`.
        key: SecretString,
    },
}

/// The self-identifying prefix every Wyrd API key carries.
///
/// A key is minted as `wyrd_sk_<tenant>_<visible>_<secret>`, so the prefix is
/// the one thing about a credential string a client may read without asking
/// the server. Anything without it is an already-issued access token.
pub const API_KEY_PREFIX: &str = "wyrd_sk_";

impl CredentialSource {
    /// Classify one explicitly supplied credential string.
    ///
    /// SDK surfaces take a single `credential` value rather than asking the
    /// caller which grant it belongs to. This is the one place that
    /// distinction is made: a value carrying [`API_KEY_PREFIX`] is exchanged
    /// through the RFC 8693 API-key token exchange, anything else is presented verbatim
    /// as a bearer access token. Routing it in one place is what keeps the
    /// Rust, Python, and TypeScript clients from disagreeing about what a
    /// credential is.
    ///
    /// # Arguments
    /// * `credential` - Credential string; an [`API_KEY_PREFIX`] value is an API key, anything
    ///   else an access token.
    #[must_use]
    pub fn explicit(credential: SecretString) -> Self {
        if credential.expose_secret().starts_with(API_KEY_PREFIX) {
            Self::ApiKey { key: credential }
        } else {
            Self::ExplicitToken { token: credential }
        }
    }
}

impl std::fmt::Debug for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExplicitToken { .. } => f
                .debug_struct("ExplicitToken")
                .field("token", &"[REDACTED]")
                .finish(),
            Self::WorkloadToken { tenant, .. } => f
                .debug_struct("WorkloadToken")
                .field("jwt", &"[REDACTED]")
                .field("tenant", tenant)
                .finish(),
            Self::ApiKey { .. } => f
                .debug_struct("ApiKey")
                .field("key", &"[REDACTED]")
                .finish(),
        }
    }
}

/// ADC-style credential resolution chain.
///
/// Add sources via [`CredentialChain::push`]; call [`CredentialChain::resolve`]
/// to get the highest-priority credential available.  The chain is checked in
/// insertion order — push tier-1 sources first.
#[derive(Debug, Default, Clone)]
pub struct CredentialChain {
    sources: Vec<CredentialSource>,
}

impl CredentialChain {
    /// Build a chain from the environment using standard Wyrd env vars:
    ///
    /// - `WYRD_ACCESS_TOKEN` — tier 1
    /// - `WYRD_WORKLOAD_TOKEN` + `WYRD_TENANT` — tier 2 (workload jwt-bearer)
    /// - `WYRD_API_KEY` — tier 3
    /// - `~/.config/wyrd/credentials.toml` `[default].api_key` — file floor
    ///
    /// Returns an empty chain when no env vars are set and the file is absent.
    /// A missing or unreadable `credentials.toml` is silently ignored.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_env_with_tenant(None)
    }

    /// Build a credential chain using an optional file-configured tenant:
    /// [`Self::env_only`] followed by [`Self::credentials_file`].
    ///
    /// # Arguments
    /// * `tenant_override` - Tenant for the workload-token tier; `None` or empty falls back to
    ///   `WYRD_TENANT`.
    #[must_use]
    pub fn from_env_with_tenant(tenant_override: Option<&str>) -> Self {
        let mut chain = Self::env_only(&Environment::Process, tenant_override);
        chain.extend(Self::credentials_file(&Environment::Process));
        chain
    }

    /// The environment tiers alone — `WYRD_ACCESS_TOKEN`, then
    /// `WYRD_WORKLOAD_TOKEN` with its tenant, then `WYRD_API_KEY` — read from
    /// `environment`, without the `credentials.toml` floor, so a caller can
    /// rank a saved user login between them.
    ///
    /// # Arguments
    /// * `environment` - Variables the tiers are read from.
    /// * `tenant_override` - Tenant for the workload-token tier; `None` or empty falls back to
    ///   `WYRD_TENANT`.
    #[must_use]
    pub fn env_only(environment: &Environment, tenant_override: Option<&str>) -> Self {
        Self {
            sources: [
                explicit_token_from_env(environment),
                workload_token_from_env(environment, tenant_override),
                api_key_from_env(environment),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }
    }

    /// The `credentials.toml` `[default].api_key` floor alone, in the
    /// configuration directory `environment` resolves; empty when the file is
    /// absent, unreadable, or has no key.
    ///
    /// # Arguments
    /// * `environment` - Variables that resolve the configuration directory.
    #[must_use]
    pub fn credentials_file(environment: &Environment) -> Self {
        Self {
            sources: api_key_from_credentials_file(environment)
                .into_iter()
                .collect(),
        }
    }

    /// Append a credential source to the chain.
    ///
    /// # Arguments
    /// * `source` - Source appended at the lowest priority.
    pub fn push(&mut self, source: CredentialSource) {
        self.sources.push(source);
    }

    /// Append all sources from `other` to the end of this chain.
    ///
    /// # Arguments
    /// * `other` - Chain whose sources are appended in order, below this chain's.
    pub fn extend(&mut self, other: CredentialChain) {
        self.sources.extend(other.sources);
    }

    /// Resolve the highest-priority credential in the chain.
    ///
    /// Returns the first [`ResolvedCredential`] the chain can produce, or
    /// `Err(WyrdClientError::NoCredentials)` when all sources are absent.
    ///
    /// # Errors
    /// Returns an error when the chain is empty or all sources produce nothing.
    pub fn resolve(&self) -> Result<ResolvedCredential, WyrdClientError> {
        self.sources
            .first()
            .map_or(Err(WyrdClientError::NoCredentials), |source| match source {
                CredentialSource::ExplicitToken { token } => {
                    Ok(ResolvedCredential::BearerToken(token.clone()))
                }
                CredentialSource::WorkloadToken { jwt, tenant } => {
                    Ok(ResolvedCredential::WorkloadJwt {
                        jwt: jwt.clone(),
                        tenant: tenant.clone(),
                    })
                }
                CredentialSource::ApiKey { key } => Ok(ResolvedCredential::ApiKey(key.clone())),
            })
    }

    /// Returns `true` when the chain has no sources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

/// Environment-based credential sources for the ADC chain.
/// WYRD_ACCESS_TOKEN is an actual WYRD JWT access token, which is the highest-priority source.
fn explicit_token_from_env(environment: &Environment) -> Option<CredentialSource> {
    let token = environment.var("WYRD_ACCESS_TOKEN")?;
    Some(CredentialSource::ExplicitToken {
        token: SecretString::from(token),
    })
}

/// Environment-based credential sources for the ADC chain.
/// WYRD_WORKLOAD_TOKEN + WYRD_TENANT is a workload identity token, which is the second-highest-priority source.
/// Often used in cloud-native environments where the workload identity provider issues a JWT that can be exchanged for a Wyrd access token.
///
/// A configured tenant selector (`tenant_override`) routes the exchange ahead
/// of an ambient `WYRD_TENANT`, so the tenant the caller selected is the one
/// the server binds.
fn workload_token_from_env(
    environment: &Environment,
    tenant_override: Option<&str>,
) -> Option<CredentialSource> {
    let jwt = environment.var("WYRD_WORKLOAD_TOKEN")?;
    let tenant = tenant_override
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| environment.var("WYRD_TENANT"))?;
    Some(CredentialSource::WorkloadToken {
        jwt: SecretString::from(jwt),
        tenant,
    })
}

/// Environment-based credential sources for the ADC chain.
/// WYRD_API_KEY is a Wyrd API key, which is the third-highest-priority source.  It is exchanged for a Wyrd access token at call time.
/// Often used in CI/CD pipelines or other environments where a long-lived API key is available.
/// An api key is granted for all Service and Agent principals upon registration
fn api_key_from_env(environment: &Environment) -> Option<CredentialSource> {
    let key = environment.var("WYRD_API_KEY")?;
    Some(CredentialSource::ApiKey {
        key: SecretString::from(key),
    })
}

/// Environment-based credential sources for the ADC chain.
/// ~/.config/wyrd/credentials.toml is a file-based credential source, which is
/// the lowest-priority source.  It is exchanged for a Wyrd access token at call time.
/// This is often used in local development environments where a user has a credentials file with their API key.
fn api_key_from_credentials_file(environment: &Environment) -> Option<CredentialSource> {
    let key = read_credentials_toml_api_key(environment)?;
    Some(CredentialSource::ApiKey {
        key: SecretString::from(key),
    })
}

/// Parse the resolved Wyrd `credentials.toml` and return `[default].api_key`.
///
/// Returns `None` when the file is absent, unreadable, or has no key.
fn read_credentials_toml_api_key(environment: &Environment) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct CredentialsFile {
        default: Option<DefaultProfile>,
    }

    #[derive(serde::Deserialize)]
    struct DefaultProfile {
        api_key: Option<String>,
    }

    let path = environment.config_dir()?.join("credentials.toml");
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: CredentialsFile = toml::from_str(&content).ok()?;
    parsed.default?.api_key.filter(|k| !k.is_empty())
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::{CredentialChain, CredentialSource, ResolvedCredential};
    use crate::environment::Environment;

    /// One `credential` string routes to the grant its own prefix names.
    ///
    /// This is the property every SDK's single-`credential` constructor rests
    /// on: an API key is exchanged, an access token is presented verbatim, and
    /// neither caller has to say which they hold.
    #[test]
    fn explicit_credential_routes_by_its_own_prefix() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::explicit(
            "wyrd_sk_tenant_visible_secret".to_owned().into(),
        ));
        match chain.resolve().expect("an API key resolves") {
            ResolvedCredential::ApiKey(key) => {
                assert_eq!(key.expose_secret(), "wyrd_sk_tenant_visible_secret");
            }
            other => panic!("a wyrd_sk_ credential is an API key, got {other:?}"),
        }

        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::explicit(
            "eyJhbGciOi.token".to_owned().into(),
        ));
        match chain.resolve().expect("an access token resolves") {
            ResolvedCredential::BearerToken(token) => {
                assert_eq!(token.expose_secret(), "eyJhbGciOi.token");
            }
            other => panic!("an unprefixed credential is a bearer token, got {other:?}"),
        }
    }

    #[test]
    fn empty_chain_returns_no_credentials() {
        let chain = CredentialChain::default();
        assert!(chain.resolve().is_err());
    }

    #[test]
    fn explicit_token_tier_resolves_first() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ExplicitToken {
            token: "access-tok".to_owned().into(),
        });
        chain.push(CredentialSource::ApiKey {
            key: "api-key".to_owned().into(),
        });
        let cred = chain.resolve().expect("chain resolves");
        match cred {
            ResolvedCredential::BearerToken(t) => {
                assert_eq!(
                    t.expose_secret(),
                    "access-tok",
                    "explicit token wins over api key"
                );
            }
            _ => panic!("expected BearerToken"),
        }
    }

    #[test]
    fn api_key_resolves_when_no_token() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ApiKey {
            key: "k_abc".to_owned().into(),
        });
        let cred = chain.resolve().expect("chain resolves");
        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(k.expose_secret(), "k_abc", "api key resolves");
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn first_source_wins_when_multiple_tokens() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ExplicitToken {
            token: "first".to_owned().into(),
        });
        chain.push(CredentialSource::ExplicitToken {
            token: "second".to_owned().into(),
        });
        let cred = chain.resolve().expect("resolves");
        match cred {
            ResolvedCredential::BearerToken(t) => {
                assert_eq!(t.expose_secret(), "first");
            }
            _ => panic!("expected BearerToken"),
        }
    }

    #[test]
    fn is_empty_reflects_chain_state() {
        let mut chain = CredentialChain::default();
        assert!(chain.is_empty());
        chain.push(CredentialSource::ApiKey {
            key: "k".to_owned().into(),
        });
        assert!(!chain.is_empty());
    }

    #[test]
    fn workload_token_tier_resolves_before_api_key() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::WorkloadToken {
            jwt: "workload.jwt.token".to_owned().into(),
            tenant: "acme".to_owned(),
        });
        chain.push(CredentialSource::ApiKey {
            key: "api-key-fallback".to_owned().into(),
        });
        let cred = chain.resolve().expect("chain resolves");
        match cred {
            ResolvedCredential::WorkloadJwt { jwt, tenant } => {
                assert_eq!(
                    jwt.expose_secret(),
                    "workload.jwt.token",
                    "workload token wins over api key"
                );
                assert_eq!(tenant, "acme");
            }
            _ => panic!("expected WorkloadJwt"),
        }
    }

    #[test]
    fn explicit_token_wins_over_workload_and_api_key() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ExplicitToken {
            token: "explicit".to_owned().into(),
        });
        chain.push(CredentialSource::WorkloadToken {
            jwt: "workload.jwt".to_owned().into(),
            tenant: "t".to_owned(),
        });
        chain.push(CredentialSource::ApiKey {
            key: "api-key".to_owned().into(),
        });
        let cred = chain.resolve().expect("chain resolves");
        match cred {
            ResolvedCredential::BearerToken(t) => {
                assert_eq!(
                    t.expose_secret(),
                    "explicit",
                    "explicit token wins over workload and api key"
                );
            }
            _ => panic!("expected BearerToken"),
        }
    }

    #[test]
    fn redacted_debug_does_not_leak_secrets() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ExplicitToken {
            token: "super-secret-token".to_owned().into(),
        });
        let cred = chain.resolve().expect("resolves");

        let src_debug = format!("{chain:?}");
        assert!(!src_debug.contains("super-secret-token"));
        assert!(src_debug.contains("REDACTED"));

        let cred_debug = format!("{cred:?}");
        assert!(!cred_debug.contains("super-secret-token"));
        assert!(cred_debug.contains("REDACTED"));
    }

    /// The `credentials.toml` floor under `$WYRD_CONFIG_HOME` resolves its
    /// `[default].api_key` when no environment tier is set.
    #[test]
    fn credentials_toml_floor_resolves_api_key() {
        let home = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            home.path().join("credentials.toml"),
            "[default]\napi_key = \"file_floor_key\"\n",
        )
        .expect("writes credentials.toml");
        let environment = Environment::from([(
            "WYRD_CONFIG_HOME",
            home.path().to_str().expect("utf-8 path"),
        )]);

        let cred = CredentialChain::credentials_file(&environment)
            .resolve()
            .expect("file floor resolves");

        match cred {
            ResolvedCredential::ApiKey(k) => assert_eq!(k.expose_secret(), "file_floor_key"),
            _ => panic!("expected ApiKey from credentials.toml floor"),
        }
    }

    /// `$XDG_CONFIG_HOME/wyrd/credentials.toml` is the floor when
    /// `$WYRD_CONFIG_HOME` is unset.
    #[test]
    fn xdg_config_home_resolves_credentials_file() {
        let xdg = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(xdg.path().join("wyrd")).expect("creates wyrd dir");
        std::fs::write(
            xdg.path().join("wyrd/credentials.toml"),
            "[default]\napi_key = \"xdg_key\"\n",
        )
        .expect("writes credentials.toml");
        let environment =
            Environment::from([("XDG_CONFIG_HOME", xdg.path().to_str().expect("utf-8 path"))]);

        let credential = CredentialChain::credentials_file(&environment)
            .resolve()
            .expect("XDG credentials file resolves");

        match credential {
            ResolvedCredential::ApiKey(key) => assert_eq!(key.expose_secret(), "xdg_key"),
            other => panic!("expected ApiKey from XDG credentials.toml, got {other:?}"),
        }
    }
}
