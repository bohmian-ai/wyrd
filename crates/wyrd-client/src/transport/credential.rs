//! ADC-style credential resolution chain for `wyrd-client`.
//!
//! [`CredentialChain`] selects the highest-priority available credential from
//! an ordered list of [`CredentialSource`]s.  The chain is built once at
//! client startup (from env vars, explicit config, or workload metadata) and
//! resolved before each outbound request.

use crate::error::WyrdClientError;

/// A resolved credential ready to attach to an outbound request.
#[derive(Debug, Clone)]
pub enum ResolvedCredential {
    /// A Wyrd access token (`Bearer <token>`).
    BearerToken(String),
    /// A Wyrd API key for the `wyrd_api_key` grant.
    ApiKey(String),
}

/// Source of credentials in the ADC-style resolution chain.
///
/// Sources are checked in priority order (lowest index = highest priority).
/// The first source that produces a credential wins.
#[derive(Debug, Clone)]
pub enum CredentialSource {
    /// Explicitly-configured Wyrd access token.  Tier 1.
    ExplicitToken {
        /// Raw access token string.
        token: String,
    },
    /// API key floor.  Exchanged for a Wyrd access token via the
    /// `wyrd_api_key` grant at call time.  Tier 3.
    ApiKey {
        /// Raw API key string.
        key: String,
    },
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
    /// - `WYRD_API_KEY` — tier 3 floor
    ///
    /// Returns an empty chain when no env vars are set.
    #[must_use]
    pub fn from_env() -> Self {
        let mut chain = Self::default();
        if let Ok(token) = std::env::var("WYRD_ACCESS_TOKEN") {
            if !token.is_empty() {
                chain.push(CredentialSource::ExplicitToken { token });
            }
        }
        if let Ok(key) = std::env::var("WYRD_API_KEY") {
            if !key.is_empty() {
                chain.push(CredentialSource::ApiKey { key });
            }
        }
        chain
    }

    /// Append a credential source to the chain.
    pub fn push(&mut self, source: CredentialSource) {
        self.sources.push(source);
    }

    /// Resolve the highest-priority credential in the chain.
    ///
    /// Returns the first [`ResolvedCredential`] the chain can produce, or
    /// `Err(WyrdClientError::NoCredentials)` when all sources are absent.
    ///
    /// # Errors
    /// Returns an error when the chain is empty or all sources produce nothing.
    pub fn resolve(&self) -> Result<ResolvedCredential, WyrdClientError> {
        for source in &self.sources {
            match source {
                CredentialSource::ExplicitToken { token } => {
                    return Ok(ResolvedCredential::BearerToken(token.clone()));
                }
                CredentialSource::ApiKey { key } => {
                    return Ok(ResolvedCredential::ApiKey(key.clone()));
                }
            }
        }
        Err(WyrdClientError::NoCredentials)
    }

    /// Returns `true` when the chain has at least one source.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

#[cfg(test)]
mod tests {
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
            token: "access-tok".to_owned(),
        });
        chain.push(CredentialSource::ApiKey {
            key: "api-key".to_owned(),
        });
        let cred = chain.resolve().expect("chain resolves");
        assert!(
            matches!(cred, ResolvedCredential::BearerToken(t) if t == "access-tok"),
            "explicit token wins over api key"
        );
    }

    #[test]
    fn api_key_resolves_when_no_token() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ApiKey {
            key: "k_abc".to_owned(),
        });
        let cred = chain.resolve().expect("chain resolves");
        assert!(
            matches!(cred, ResolvedCredential::ApiKey(k) if k == "k_abc"),
            "api key resolves"
        );
    }

    #[test]
    fn first_source_wins_when_multiple_tokens() {
        let mut chain = CredentialChain::default();
        chain.push(CredentialSource::ExplicitToken {
            token: "first".to_owned(),
        });
        chain.push(CredentialSource::ExplicitToken {
            token: "second".to_owned(),
        });
        let cred = chain.resolve().expect("resolves");
        assert!(matches!(cred, ResolvedCredential::BearerToken(t) if t == "first"));
    }

    #[test]
    fn is_empty_reflects_chain_state() {
        let mut chain = CredentialChain::default();
        assert!(chain.is_empty());
        chain.push(CredentialSource::ApiKey {
            key: "k".to_owned(),
        });
        assert!(!chain.is_empty());
    }
}
