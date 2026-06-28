//! ADC-style credential resolution chain for `wyrd-client`.
//!
//! [`CredentialChain`] selects the highest-priority available credential from
//! an ordered list of [`CredentialSource`]s.  The chain is built once at
//! client startup (from env vars, explicit config, or workload metadata) and
//! resolved before each outbound request.

use secrecy::SecretString;

use crate::error::WyrdClientError;

/// A resolved credential ready to attach to an outbound request.
///
/// Secrets are held as [`SecretString`]; call `.expose_secret()` on the inner
/// value to read them.  `Debug` is redacted — no raw key or token leaks.
#[derive(Clone)]
pub enum ResolvedCredential {
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
    /// A Wyrd API key for the `wyrd_api_key` grant.
    ApiKey(SecretString),
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BearerToken(_) => f.debug_tuple("BearerToken").field(&"[REDACTED]").finish(),
            Self::WorkloadJwt { tenant, .. } => f
                .debug_struct("WorkloadJwt")
                .field("jwt", &"[REDACTED]")
                .field("tenant", tenant)
                .finish(),
            Self::ApiKey(_) => f.debug_tuple("ApiKey").field(&"[REDACTED]").finish(),
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
    /// `wyrd_api_key` grant at call time.  Tier 3.
    ApiKey {
        /// Raw API key, redacted in `Debug`.
        key: SecretString,
    },
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
        let mut chain = Self::default();
        for source in [
            explicit_token_from_env(),
            workload_token_from_env(),
            api_key_from_env(),
            api_key_from_credentials_file(),
        ]
        .into_iter()
        .flatten()
        {
            chain.push(source);
        }
        chain
    }

    /// Append a credential source to the chain.
    pub fn push(&mut self, source: CredentialSource) {
        self.sources.push(source);
    }

    /// Append all sources from `other` to the end of this chain.
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
            .map(|source| match source {
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
            .unwrap_or(Err(WyrdClientError::NoCredentials))
    }

    /// Returns `true` when the chain has no sources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

/// Environment-based credential sources for the ADC chain.
/// WYRD_ACCESS_TOKEN is an actual WYRD JWT access token, which is the highest-priority source.
fn explicit_token_from_env() -> Option<CredentialSource> {
    let token = std::env::var("WYRD_ACCESS_TOKEN")
        .ok()
        .filter(|v| !v.is_empty())?;
    Some(CredentialSource::ExplicitToken {
        token: SecretString::from(token),
    })
}

/// Environment-based credential sources for the ADC chain.
/// WYRD_WORKLOAD_TOKEN + WYRD_TENANT is a workload identity token, which is the second-highest-priority source.
/// Often used in cloud-native environments where the workload identity provider issues a JWT that can be exchanged for a Wyrd access token.
fn workload_token_from_env() -> Option<CredentialSource> {
    let jwt = std::env::var("WYRD_WORKLOAD_TOKEN")
        .ok()
        .filter(|v| !v.is_empty())?;
    let tenant = std::env::var("WYRD_TENANT")
        .ok()
        .filter(|v| !v.is_empty())?;
    Some(CredentialSource::WorkloadToken {
        jwt: SecretString::from(jwt),
        tenant,
    })
}

/// Environment-based credential sources for the ADC chain.
/// WYRD_API_KEY is a Wyrd API key, which is the third-highest-priority source.  It is exchanged for a Wyrd access token at call time.
/// Often used in CI/CD pipelines or other environments where a long-lived API key is available.
/// An api key is granted for all Service and Agent principals upon registration
fn api_key_from_env() -> Option<CredentialSource> {
    let key = std::env::var("WYRD_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())?;
    Some(CredentialSource::ApiKey {
        key: SecretString::from(key),
    })
}

/// Environment-based credential sources for the ADC chain.
/// ~/.config/wyrd/credentials.toml is a file-based credential source, which is
/// the lowest-priority source.  It is exchanged for a Wyrd access token at call time.
/// This is often used in local development environments where a user has a credentials file with their API key.
fn api_key_from_credentials_file() -> Option<CredentialSource> {
    let key = read_credentials_toml_api_key()?;
    Some(CredentialSource::ApiKey {
        key: SecretString::from(key),
    })
}

/// Parse `~/.config/wyrd/credentials.toml` and return `[default].api_key`.
///
/// Returns `None` when the file is absent, unreadable, or has no key.
fn read_credentials_toml_api_key() -> Option<String> {
    let expanded = shellexpand::tilde("~/.config/wyrd/credentials.toml");
    let content = std::fs::read_to_string(expanded.as_ref()).ok()?;

    #[derive(serde::Deserialize)]
    struct CredentialsFile {
        default: Option<DefaultProfile>,
    }

    #[derive(serde::Deserialize)]
    struct DefaultProfile {
        api_key: Option<String>,
    }

    let parsed: CredentialsFile = toml::from_str(&content).ok()?;
    parsed.default?.api_key.filter(|k| !k.is_empty())
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::{CredentialChain, CredentialSource, ResolvedCredential};

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

    #[test]
    fn credentials_toml_floor_resolves_api_key() {
        use std::fs;

        let _env = crate::ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());

        let tmp = std::env::temp_dir().join(format!(
            "wyrd_cred_test_{}_{}",
            std::process::id(),
            uuid::Uuid::now_v7()
        ));
        fs::create_dir_all(tmp.join(".config/wyrd")).unwrap();
        fs::write(
            tmp.join(".config/wyrd/credentials.toml"),
            "[default]\napi_key = \"file_floor_key\"\n",
        )
        .unwrap();

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::set_var("HOME", &tmp);
            std::env::remove_var("WYRD_API_KEY");
            std::env::remove_var("WYRD_ACCESS_TOKEN");
            std::env::remove_var("WYRD_WORKLOAD_TOKEN");
            std::env::remove_var("WYRD_TENANT");
        }

        let chain = CredentialChain::from_env();
        let cred = chain.resolve().expect("file floor resolves");

        // SAFETY: ENV_MUTEX (held for this test) serializes env mutation in this binary.
        unsafe {
            std::env::remove_var("HOME");
        }
        fs::remove_dir_all(&tmp).ok();

        match cred {
            ResolvedCredential::ApiKey(k) => {
                assert_eq!(k.expose_secret(), "file_floor_key");
            }
            _ => panic!("expected ApiKey from credentials.toml floor"),
        }
    }
}
