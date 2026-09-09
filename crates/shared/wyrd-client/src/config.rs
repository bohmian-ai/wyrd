//! Top-level client configuration.
//!
//! [`ClientConfig`] is the single entry point for configuring `wyrd-client`.
//! Build from environment variables with [`ClientConfig::from_env`], optionally
//! set [`ClientConfig::credential`] for an explicit credential that outranks ambient
//! env and file credentials, then call [`ClientConfig::resolve_credential`] to
//! obtain the effective [`ResolvedCredential`].

use secrecy::SecretString;

use crate::error::WyrdClientError;
use crate::transport::{
    config::{GRPC_DEFAULT_ENDPOINT, GrpcConfig, HTTP_DEFAULT_BASE_URL, HttpConfig},
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
    /// `credentials.toml` floor. It may be either a Wyrd API key or an
    /// already-issued access token; [`CredentialSource::explicit`] decides
    /// which grant it belongs to from the key's own prefix.
    pub credential: Option<SecretString>,
    /// Token cache mode.
    pub token_cache: TokenCacheMode,
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
            .field("token_cache", &self.token_cache)
            .finish()
    }
}

impl ClientConfig {
    /// Build from environment variables.
    ///
    /// - `WYRD_GRPC_URL` overrides the gRPC endpoint (default:
    ///   `http://localhost:50051`).
    /// - `WYRD_SERVER_URL` overrides the HTTP base URL (default:
    ///   `http://localhost:50050`).
    ///
    /// The [`ClientConfig::credential`] field is left `None`; set it
    /// explicitly after construction to make it the highest-priority
    /// credential source.
    #[must_use]
    pub fn from_env() -> Self {
        let grpc_endpoint =
            std::env::var("WYRD_GRPC_URL").unwrap_or_else(|_| GRPC_DEFAULT_ENDPOINT.to_string());
        let http_base_url =
            std::env::var("WYRD_SERVER_URL").unwrap_or_else(|_| HTTP_DEFAULT_BASE_URL.to_string());

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
            token_cache: TokenCacheMode::default(),
        }
    }

    /// Resolve the effective credential.
    ///
    /// Precedence (lowest index wins):
    /// 1. `self.credential` — explicit credential set by caller
    /// 2. `WYRD_ACCESS_TOKEN` — tier 1 env
    /// 3. `WYRD_WORKLOAD_TOKEN` + `WYRD_TENANT` — tier 2 env
    /// 4. `WYRD_API_KEY` — tier 3 env
    /// 5. `~/.config/wyrd/credentials.toml` `[default].api_key` — file floor
    ///
    /// # Errors
    /// Returns [`WyrdClientError::NoCredentials`] when the chain (including the
    /// file floor) yields nothing.
    pub fn resolve_credential(&self) -> Result<ResolvedCredential, WyrdClientError> {
        let mut chain = CredentialChain::default();
        if let Some(credential) = &self.credential {
            chain.push(CredentialSource::explicit(credential.clone()));
        }
        chain.extend(CredentialChain::from_env());
        chain.resolve()
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    /// A plaintext API key shaped exactly as [`WyrdApiKey::generate`] mints
    /// one: `wyrd_sk_<simple-uuid tenant>_<8-char visible>_<32-char secret>`.
    ///
    /// The precedence tests assert that an explicit credential wins, so the
    /// fixture must be something the server-side parser would accept; a
    /// prose placeholder would classify as a bearer token instead.
    const API_KEY_FIXTURE: &str =
        "wyrd_sk_4d5e1c3a9b7f4e2d8a6c0b1e2f3a4b5c_1a2b3c4d_9f8e7d6c5b4a39281706f5e4d3c2b1a0";

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
}
