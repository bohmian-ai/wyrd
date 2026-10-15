//! Wyrd API keys presented directly to public gateway ingress.
//!
//! A stock provider SDK configured only with a base URL and a static key
//! sends that key where it would send its provider key. Ingress exchanges such
//! a key through the same API-key grant `POST /auth/token` runs, so the
//! exchange commits its use record and stages its authorization decision, and
//! then verifies the minted token like any presented access token.
//!
//! [`GatewayApiKeys`] reuses the minted token for the identical key for at
//! most [`TOKEN_REUSE`] after the exchange, so revoking a key takes effect
//! within that window. Entries are keyed by the SHA-256 of the complete key,
//! never the key itself, so no key's authority is ever reused for another. A
//! read never renews an entry, a failed exchange is never cached, and the
//! process keeps at most [`CAPACITY`] entries.

use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lru::LruCache;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use wyrd_spec::auth::SecretBearer;
use wyrd_spec::error::WyrdError;

use crate::components::auth::AuthenticatedPrincipal;
use crate::components::auth::routes::TokenGrants;
use crate::components::auth::token_extract::verify_access_token;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Longest a token exchanged for a presented key is reused, measured from the
/// exchange; it bounds how long a revoked key keeps working.
const TOKEN_REUSE: Duration = Duration::from_secs(60);

/// Most exchanged tokens one process keeps; the least recently used entry is
/// evicted beyond it.
const CAPACITY: NonZeroUsize = NonZeroUsize::new(4096).expect("capacity is non-zero");

/// One exchanged token and the instant it stops being reused.
struct CachedToken {
    /// The minted Wyrd access token.
    token: SecretString,
    /// The earlier of the exchange plus [`TOKEN_REUSE`] and the token's own
    /// expiry; fixed at admission.
    deadline: Instant,
}

/// The process's short-lived reuse of tokens exchanged for API keys presented
/// to gateway ingress.
pub(crate) struct GatewayApiKeys {
    /// Tokens by SHA-256 of the complete key, behind one short synchronous
    /// lock; no await happens under it.
    tokens: Mutex<LruCache<[u8; 32], CachedToken>>,
}

impl Default for GatewayApiKeys {
    /// An empty cache bounded at [`CAPACITY`] entries.
    fn default() -> Self {
        Self::with_capacity(CAPACITY)
    }
}

impl GatewayApiKeys {
    /// An empty cache holding at most `capacity` entries.
    fn with_capacity(capacity: NonZeroUsize) -> Self {
        Self {
            tokens: Mutex::new(LruCache::new(capacity)),
        }
    }

    /// Authenticates a request that presented `api_key` as the key's own
    /// principal.
    ///
    /// A reusable token for the identical key is verified and used; otherwise,
    /// or when that token no longer verifies, the key is exchanged through
    /// [`TokenGrants`] under `request_id`, the minted token verified for the
    /// Wyrd audience, and only then cached. Either way gateway authorization
    /// still decides, and audits, every call.
    ///
    /// # Errors
    /// Returns the exchange's refusal, [`WyrdError::ApiKeyInvalid`] for every
    /// unusable key, its store or issuance error, and the verifier's refusal
    /// of the minted token.
    pub(crate) async fn authenticate(
        &self,
        state: &AppState,
        api_key: &SecretString,
        request_id: &str,
    ) -> Result<AuthenticatedPrincipal, WyrdError> {
        let verify = |token| {
            verify_access_token(
                state.auth.token_verifier.as_deref(),
                token,
                wyrd_auth_verify::TokenAudience::Wyrd,
            )
            .map_err(|WyrdErrorResponse(error)| error)
        };
        if let Some(token) = self.get(api_key, Instant::now())
            && let Ok(principal) = verify(token)
        {
            return Ok(principal);
        }
        let exchanged = TokenGrants { state, request_id }
            .api_key(&SecretBearer::new(api_key.expose_secret().to_owned()))
            .await?;
        let token = exchanged.access_token.into_secret_string();
        let principal = verify(token.clone())?;
        self.insert(
            api_key,
            token,
            Duration::from_secs(exchanged.expires_in),
            Instant::now(),
        );
        Ok(principal)
    }

    /// The SHA-256 of the complete `api_key`, the only form a key is kept in.
    fn hash(api_key: &SecretString) -> [u8; 32] {
        Sha256::digest(api_key.expose_secret().as_bytes()).into()
    }

    /// The token exchanged for `api_key` when it is still reusable at `now`.
    ///
    /// A hit does not move its deadline; an entry past it is removed.
    ///
    /// # Panics
    /// Panics when the cache lock is poisoned, which no holder can cause
    /// because nothing under it panics.
    fn get(&self, api_key: &SecretString, now: Instant) -> Option<SecretString> {
        let hash = Self::hash(api_key);
        let mut tokens = self.tokens.lock().expect("gateway api key cache lock");
        match tokens.get(&hash) {
            Some(cached) if now < cached.deadline => Some(cached.token.clone()),
            Some(_) => {
                tokens.pop(&hash);
                None
            }
            None => None,
        }
    }

    /// Caches `token`, exchanged for `api_key` at `now` and valid for
    /// `lifetime`, until the earlier of [`TOKEN_REUSE`] and that lifetime.
    ///
    /// Replaces any entry of the same key and evicts the least recently used
    /// entry beyond capacity.
    ///
    /// # Panics
    /// Panics when the cache lock is poisoned.
    fn insert(
        &self,
        api_key: &SecretString,
        token: SecretString,
        lifetime: Duration,
        now: Instant,
    ) {
        let deadline = now + TOKEN_REUSE.min(lifetime);
        self.tokens
            .lock()
            .expect("gateway api key cache lock")
            .put(Self::hash(api_key), CachedToken { token, deadline });
    }
}

#[cfg(test)]
mod tests {
    //! Reuse, fixed expiry, per-key isolation, and bounds of exchanged tokens.

    use std::num::NonZeroUsize;
    use std::time::{Duration, Instant};

    use secrecy::{ExposeSecret, SecretString};

    use super::{GatewayApiKeys, TOKEN_REUSE};

    /// A secret holding `value`.
    fn secret(value: &str) -> SecretString {
        SecretString::from(value.to_owned())
    }

    /// The token cached for `key` at `now`, exposed for comparison.
    fn cached(cache: &GatewayApiKeys, key: &str, now: Instant) -> Option<String> {
        cache
            .get(&secret(key), now)
            .map(|token| token.expose_secret().to_owned())
    }

    /// A token is reused only for its identical key, until a deadline fixed at
    /// admission that repeated hits never extend; a token expiring sooner
    /// stops being reused at its own expiry; and the least recently used
    /// entry is evicted beyond capacity.
    ///
    /// # Panics
    /// Panics when the cache reuses a token differently than documented.
    #[test]
    fn gateway_api_key_cache_is_bounded_isolated_and_expires() {
        let cache = GatewayApiKeys::with_capacity(NonZeroUsize::new(2).expect("non-zero"));
        let start = Instant::now();
        let hour = Duration::from_secs(3600);
        cache.insert(&secret("wyrd_sk_a"), secret("jwt-a"), hour, start);
        assert_eq!(cached(&cache, "wyrd_sk_a", start).as_deref(), Some("jwt-a"));
        assert_eq!(cached(&cache, "wyrd_sk_b", start), None);
        assert_eq!(cached(&cache, "wyrd_sk_a_", start), None);
        for elapsed in [1, 30, 59] {
            let now = start + Duration::from_secs(elapsed);
            assert_eq!(cached(&cache, "wyrd_sk_a", now).as_deref(), Some("jwt-a"));
        }
        let just_before = start + Duration::from_millis(59_999);
        assert_eq!(
            cached(&cache, "wyrd_sk_a", just_before).as_deref(),
            Some("jwt-a")
        );
        assert_eq!(cached(&cache, "wyrd_sk_a", start + TOKEN_REUSE), None);
        assert_eq!(
            cached(&cache, "wyrd_sk_a", start),
            None,
            "expired entry is removed"
        );

        cache.insert(
            &secret("wyrd_sk_short"),
            secret("jwt-short"),
            Duration::from_secs(10),
            start,
        );
        let ten = start + Duration::from_secs(10);
        assert_eq!(
            cached(
                &cache,
                "wyrd_sk_short",
                start + Duration::from_millis(9_999)
            )
            .as_deref(),
            Some("jwt-short")
        );
        assert_eq!(cached(&cache, "wyrd_sk_short", ten), None);

        for key in ["wyrd_sk_1", "wyrd_sk_2", "wyrd_sk_3"] {
            cache.insert(&secret(key), secret(key), hour, start);
        }
        assert_eq!(cached(&cache, "wyrd_sk_1", start), None, "evicted");
        assert_eq!(
            cached(&cache, "wyrd_sk_2", start).as_deref(),
            Some("wyrd_sk_2")
        );
        assert_eq!(
            cached(&cache, "wyrd_sk_3", start).as_deref(),
            Some("wyrd_sk_3")
        );
    }
}
