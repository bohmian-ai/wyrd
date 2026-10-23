//! Wyrd API keys presented directly to public gateway ingress.
//!
//! A stock provider SDK configured only with a base URL and a static key
//! sends that key where it would send its provider key. Ingress exchanges such
//! a key through the same API-key grant `POST /auth/token` runs, so the
//! exchange commits its use record and stages its authorization decision, and
//! then verifies the minted token like any presented access token.
//!
//! [`GatewayApiKeys`] reuses the minted token for the identical key for at
//! most [`TOKEN_REUSE`] after the exchange began, so retiring a key takes
//! effect within that window however late an in-flight exchange completes. Entries are keyed by the SHA-256 of the complete key and
//! its optional workload root, never the key itself, so no key's authority,
//! and no release's context, is ever reused for another. A
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
use wyrd_spec::ids::CardUid;

use crate::components::auth::AuthenticatedPrincipal;
use crate::components::auth::routes::TokenGrants;
use crate::components::auth::token_extract::verify_access_token;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Longest a token exchanged for a presented key is reused, measured from the
/// start of the exchange; it bounds how long a retired key keeps working.
const TOKEN_REUSE: Duration = Duration::from_secs(60);

/// Most exchanged tokens one process keeps; the least recently used entry is
/// evicted beyond it.
const CAPACITY: NonZeroUsize = NonZeroUsize::new(4096).expect("capacity is non-zero");

/// One exchanged token and the instant it stops being reused.
struct CachedToken {
    /// The minted Wyrd access token.
    token: SecretString,
    /// The earlier of the exchange's start plus [`TOKEN_REUSE`] and the
    /// token's own expiry measured from that start; never moved by a read.
    deadline: Instant,
}

/// The process's short-lived reuse of tokens exchanged for API keys presented
/// to gateway ingress.
pub(crate) struct GatewayApiKeys {
    /// Tokens by [`Self::entry`], behind one short synchronous lock; no
    /// await happens under it.
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
    /// principal, under the workload root `workload_uid` selects, if any.
    ///
    /// A reusable token for the identical key and root is verified and used; otherwise,
    /// or when that token no longer verifies, the key is exchanged through
    /// [`TokenGrants`] under `request_id`, the minted token verified for the
    /// Wyrd audience, and only then cached by [`Self::reuse_or_exchange`].
    /// Either way gateway authorization still decides, and audits, every call.
    ///
    /// # Cancellation and retries
    /// Dropping the future mid-exchange can leave the grant's work partly
    /// done: its audit decision may already be staged and its use record
    /// committed, or the commit's outcome unknown, while no token is returned
    /// or cached. Nothing is rolled back. A later request with the same key
    /// finds no cache entry and exchanges, and is audited, again; exchanges
    /// are not deduplicated, so concurrent misses for one key each exchange.
    ///
    /// # Errors
    /// Returns the exchange's refusal, [`WyrdError::ApiKeyInvalid`] for every
    /// unusable key, its store or issuance error, and the verifier's refusal
    /// of the minted token.
    pub(crate) async fn authenticate(
        &self,
        state: &AppState,
        api_key: &SecretString,
        workload_uid: Option<CardUid>,
        request_id: &str,
    ) -> Result<AuthenticatedPrincipal, WyrdError> {
        let entry = Self::entry(api_key, workload_uid.as_ref());
        let exchange = async {
            let exchanged = TokenGrants { state, request_id }
                .api_key(
                    &SecretBearer::new(api_key.expose_secret().to_owned()),
                    workload_uid,
                )
                .await?;
            Ok((
                exchanged.access_token.into_secret_string(),
                Duration::from_secs(exchanged.expires_in),
            ))
        };
        let verify = |token| {
            verify_access_token(
                state.auth.token_verifier.as_deref(),
                token,
                wyrd_auth_verify::TokenAudience::Wyrd,
            )
            .map_err(|WyrdErrorResponse(error)| error)
        };
        self.reuse_or_exchange(entry, Instant::now, verify, exchange)
            .await
    }

    /// Reuses the token cached under `entry` when it still verifies, and
    /// otherwise awaits `exchange` and caches its verified token.
    ///
    /// `clock` is read on every lookup and once before `exchange` is polled.
    /// That pre-exchange instant is the reuse origin, so a slow exchange,
    /// or one that commits just before the key is retired, never pushes the
    /// entry past [`TOKEN_REUSE`] after the exchange began. A result whose
    /// window already closed is still returned for this request but is
    /// never reusable. `exchange` yields the minted token and its lifetime.
    ///
    /// # Errors
    /// Returns the error of `exchange`, or of `verify` for the minted token.
    async fn reuse_or_exchange<T>(
        &self,
        entry: [u8; 32],
        clock: impl Fn() -> Instant,
        verify: impl Fn(SecretString) -> Result<T, WyrdError>,
        exchange: impl Future<Output = Result<(SecretString, Duration), WyrdError>>,
    ) -> Result<T, WyrdError> {
        if let Some(token) = self.get(entry, clock())
            && let Ok(principal) = verify(token)
        {
            return Ok(principal);
        }
        let origin = clock();
        let (token, lifetime) = exchange.await?;
        let principal = verify(token.clone())?;
        if clock() < origin + TOKEN_REUSE.min(lifetime) {
            self.insert(entry, token, lifetime, origin);
        }
        Ok(principal)
    }

    /// The SHA-256 of the complete `api_key` and its optional workload root,
    /// the only form a key is kept in. A rootless entry and each root's entry
    /// of one key are distinct.
    fn entry(api_key: &SecretString, workload_uid: Option<&CardUid>) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(api_key.expose_secret().as_bytes());
        if let Some(uid) = workload_uid {
            digest.update([0]);
            digest.update(uid.as_str().as_bytes());
        }
        digest.finalize().into()
    }

    /// The token cached under `entry` when it is still reusable at `now`.
    ///
    /// A hit does not move its deadline; an entry past it is removed.
    ///
    /// # Panics
    /// Panics when the cache lock is poisoned, which no holder can cause
    /// because nothing under it panics.
    fn get(&self, entry: [u8; 32], now: Instant) -> Option<SecretString> {
        let mut tokens = self.tokens.lock().expect("gateway api key cache lock");
        match tokens.get(&entry) {
            Some(cached) if now < cached.deadline => Some(cached.token.clone()),
            Some(_) => {
                tokens.pop(&entry);
                None
            }
            None => None,
        }
    }

    /// Caches `token` under `entry`, whose exchange began at `origin` and
    /// which is valid for `lifetime`, until the earlier of `origin` plus
    /// [`TOKEN_REUSE`] and `origin` plus that lifetime.
    ///
    /// Replaces any entry of the same key and evicts the least recently used
    /// entry beyond capacity.
    ///
    /// # Panics
    /// Panics when the cache lock is poisoned.
    fn insert(&self, entry: [u8; 32], token: SecretString, lifetime: Duration, origin: Instant) {
        let deadline = origin + TOKEN_REUSE.min(lifetime);
        self.tokens
            .lock()
            .expect("gateway api key cache lock")
            .put(entry, CachedToken { token, deadline });
    }
}

#[cfg(test)]
mod tests {
    //! Reuse, fixed expiry, per-key isolation, and bounds of exchanged tokens.

    use std::cell::Cell;
    use std::num::NonZeroUsize;
    use std::time::{Duration, Instant};

    use secrecy::{ExposeSecret, SecretString};
    use wyrd_spec::ids::CardUid;

    use super::{GatewayApiKeys, TOKEN_REUSE};

    /// A secret holding `value`.
    fn secret(value: &str) -> SecretString {
        SecretString::from(value.to_owned())
    }

    /// The rootless cache entry of `key`.
    fn entry(key: &str) -> [u8; 32] {
        GatewayApiKeys::entry(&secret(key), None)
    }

    /// The token cached for `key` at `now`, exposed for comparison.
    fn cached(cache: &GatewayApiKeys, key: &str, now: Instant) -> Option<String> {
        cache
            .get(entry(key), now)
            .map(|token| token.expose_secret().to_owned())
    }

    /// A miss reuses its token only until [`TOKEN_REUSE`] after its exchange
    /// began, however late the exchange completes.
    ///
    /// # Panics
    /// Panics when a delayed exchange's token outlives that window.
    #[tokio::test]
    async fn delayed_exchange_keeps_its_original_reuse_deadline() {
        let cache = GatewayApiKeys::default();
        let start = Instant::now();
        let now = Cell::new(start);
        let hour = Duration::from_secs(3600);
        let exchange = |token: &'static str, completes: Duration| {
            let now = &now;
            async move {
                now.set(start + completes);
                Ok((secret(token), hour))
            }
        };
        let verify = |token: SecretString| Ok(token.expose_secret().to_owned());

        let first = cache
            .reuse_or_exchange(
                entry("wyrd_sk_slow"),
                || now.get(),
                verify,
                exchange("jwt-slow", Duration::from_secs(2)),
            )
            .await
            .expect("delayed exchange authenticates");
        assert_eq!(first, "jwt-slow");
        let before_deadline = start + Duration::from_millis(59_999);
        assert_eq!(
            cached(&cache, "wyrd_sk_slow", before_deadline).as_deref(),
            Some("jwt-slow")
        );
        assert_eq!(
            cached(&cache, "wyrd_sk_slow", start + TOKEN_REUSE),
            None,
            "reuse ends sixty seconds after the exchange began"
        );

        now.set(start);
        let late = cache
            .reuse_or_exchange(
                entry("wyrd_sk_late"),
                || now.get(),
                verify,
                exchange("jwt-late", Duration::from_secs(61)),
            )
            .await
            .expect("the late exchange still serves its own request");
        assert_eq!(late, "jwt-late");
        assert_eq!(
            cache.tokens.lock().expect("cache lock").len(),
            0,
            "the late completion took no cache slot"
        );
        assert_eq!(
            cached(&cache, "wyrd_sk_late", start + Duration::from_secs(61)),
            None,
            "a completion after its window admits no reusable token"
        );
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
        cache.insert(entry("wyrd_sk_a"), secret("jwt-a"), hour, start);
        assert_eq!(cached(&cache, "wyrd_sk_a", start).as_deref(), Some("jwt-a"));
        let root = CardUid::new("01890a5d-ac96-774b-bcce-b302099a8057").expect("static uid");
        assert_eq!(
            cache.get(
                GatewayApiKeys::entry(&secret("wyrd_sk_a"), Some(&root)),
                start
            ),
            None,
            "a root's entry is not the rootless entry of the same key"
        );
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
            entry("wyrd_sk_short"),
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
            cache.insert(entry(key), secret(key), hour, start);
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
