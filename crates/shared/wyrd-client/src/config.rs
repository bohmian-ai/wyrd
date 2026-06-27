//! Top-level client configuration.
//!
//! [`ClientConfig`] is the single entry point for configuring `wyrd-client`.
//! Build from environment variables with [`ClientConfig::from_env`], optionally
//! set [`ClientConfig::api_key`] for an explicit key that outranks ambient env
//! and file credentials, then call [`ClientConfig::resolve_credential`] to
//! obtain the effective [`ResolvedCredential`].

use secrecy::SecretString;

use crate::error::WyrdClientError;
use crate::transport::{
    config::{GrpcConfig, HttpConfig, GRPC_DEFAULT_ENDPOINT, HTTP_DEFAULT_BASE_URL},
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
pub struct ClientConfig {
    /// gRPC transport configuration.
    pub grpc: GrpcConfig,
    /// HTTP transport configuration.
    pub http: HttpConfig,
    /// Explicit API key.
    ///
    /// When set, this key is prepended to the credential chain as tier 0 and
    /// always wins over `WYRD_API_KEY`, `WYRD_ACCESS_TOKEN`, and the
    /// `credentials.toml` floor.
    pub api_key: Option<SecretString>,
    /// Token cache mode.
    pub token_cache: TokenCacheMode,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("grpc", &self.grpc)
            .field("http", &self.http)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("token_cache", &self.token_cache)
            .finish()
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            grpc: GrpcConfig::default(),
            http: HttpConfig::default(),
            api_key: None,
            token_cache: TokenCacheMode::default(),
        }
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
    /// The [`ClientConfig::api_key`] field is left `None`; set it explicitly
    /// after construction to make it the highest-priority credential source.
    #[must_use]
    pub fn from_env() -> Self {
        let grpc_endpoint = std::env::var("WYRD_GRPC_URL")
            .unwrap_or_else(|_| GRPC_DEFAULT_ENDPOINT.to_string());
        let http_base_url = std::env::var("WYRD_SERVER_URL")
            .unwrap_or_else(|_| HTTP_DEFAULT_BASE_URL.to_string());

        Self {
            grpc: GrpcConfig {
                endpoint: grpc_endpoint,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: http_base_url,
                ..HttpConfig::default()
            },
            api_key: None,
            token_cache: TokenCacheMode::default(),
        }
    }

    /// Resolve the effective credential.
    ///
    /// Precedence (lowest index wins):
    /// 1. `self.api_key` — explicit key set by caller
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
        if let Some(key) = &self.api_key {
            chain.push(CredentialSource::ApiKey { key: key.clone() });
        }
        chain.extend(CredentialChain::from_env());
        chain.resolve()
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::{ClientConfig, TokenCacheMode};
    use crate::transport::{
        config::{GRPC_DEFAULT_ENDPOINT, HTTP_DEFAULT_BASE_URL},
        credential::ResolvedCredential,
    };

    #[test]
    fn from_env_uses_defaults_when_no_env_vars() {
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
            std::env::remove_var("WYRD_SERVER_URL");
        }

        let cfg = ClientConfig::from_env();

        assert_eq!(cfg.grpc.endpoint, GRPC_DEFAULT_ENDPOINT);
        assert_eq!(cfg.http.base_url, HTTP_DEFAULT_BASE_URL);
        assert_eq!(cfg.token_cache, TokenCacheMode::InMemory);
        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn wyrd_grpc_url_overrides_default_endpoint() {
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::set_var("WYRD_GRPC_URL", "https://grpc.example.com:443");
        }
        let cfg = ClientConfig::from_env();
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::remove_var("WYRD_GRPC_URL");
        }

        assert_eq!(cfg.grpc.endpoint, "https://grpc.example.com:443");
    }

    #[test]
    fn wyrd_server_url_overrides_default_base_url() {
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::set_var("WYRD_SERVER_URL", "https://api.example.com");
        }
        let cfg = ClientConfig::from_env();
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::remove_var("WYRD_SERVER_URL");
        }

        assert_eq!(cfg.http.base_url, "https://api.example.com");
    }

    #[test]
    fn no_credentials_when_chain_empty() {
        // SAFETY: single-threaded test runner (--test-threads=1).
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

        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe { std::env::remove_var("HOME"); }

        assert!(result.is_err(), "empty chain must return NoCredentials");
    }

    #[test]
    fn env_api_key_resolves_when_no_explicit() {
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::set_var("WYRD_API_KEY", "env_api_key_value");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
        }

        let cfg = ClientConfig::from_env();
        let cred = cfg.resolve_credential().expect("resolves from env");

        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe { std::env::remove_var("WYRD_API_KEY"); }

        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(k.expose_secret(), "env_api_key_value");
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn explicit_api_key_beats_env_wyrd_api_key() {
        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe {
            std::env::set_var("WYRD_API_KEY", "env_key_should_lose");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
        }

        let mut cfg = ClientConfig::from_env();
        cfg.api_key = Some("explicit_key_wins".to_owned().into());

        let cred = cfg.resolve_credential().expect("explicit key resolves");

        // SAFETY: single-threaded test runner (--test-threads=1).
        unsafe { std::env::remove_var("WYRD_API_KEY"); }

        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(
                    k.expose_secret(),
                    "explicit_key_wins",
                    "explicit api_key must beat ambient WYRD_API_KEY"
                );
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn debug_does_not_leak_api_key() {
        let mut cfg = ClientConfig::default();
        cfg.api_key = Some("top-secret".to_owned().into());

        let debug = format!("{cfg:?}");

        assert!(!debug.contains("top-secret"), "Debug must not expose the raw api_key");
        assert!(debug.contains("REDACTED"));
    }
}
