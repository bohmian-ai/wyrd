//! Platform-scope credential issuance and verification.
//!
//! Credentials authenticate a platform principal; they are never the identity
//! and never carry authority. This module owns the two halves that keep that
//! true: minting a secret whose plaintext leaves the server exactly once, and
//! verifying a presented secret back to the principal that owns it.
//!
//! The tenant plane's equivalent is [`crate::issue_api_key`], which runs under
//! row-level security. Platform credentials sit outside that boundary by
//! construction and therefore run on the BYPASSRLS [`OperatorPool`].

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth_issue::{IssueError, hash_api_key, verify_api_key};
use wyrd_spec::auth::PrincipalId;
use wyrd_sql::queries::platform::credentials::{
    insert_platform_credential_tx, platform_credential_by_prefix, touch_platform_credential,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn};

/// Prefix identifying a platform-scope credential on sight.
///
/// Distinct from the tenant key prefix so an operator can tell at a glance which
/// control plane a leaked or pasted secret belongs to, and so credential lookup
/// routes to the right store before any verification work.
const PLATFORM_KEY_PREFIX: &str = "wyrd_global";

/// A freshly generated platform credential, plaintext included.
///
/// Exists only between generation and the single response that returns it. The
/// server persists `prefix` and a verifier derived from `secret`, never
/// `secret` itself.
#[derive(Debug)]
pub struct PlatformCredential {
    /// Non-secret lookup prefix, stored and used to find the credential row.
    pub prefix: String,
    /// Full plaintext secret, returned to the operator exactly once.
    pub secret: SecretString,
}

impl PlatformCredential {
    /// Generate a platform credential from cryptographically secure randomness.
    ///
    /// The prefix is a visible, non-secret fragment; the remainder is the
    /// entropy an attacker would have to guess. Both halves together are the
    /// plaintext the operator stores in their own secret manager.
    #[must_use]
    pub fn generate() -> Self {
        let visible = Uuid::new_v4().simple().to_string();
        let prefix = format!("{PLATFORM_KEY_PREFIX}_{}", &visible[..12]);
        let secret = SecretString::from(format!("{prefix}_{}", Uuid::new_v4().simple()));
        Self { prefix, secret }
    }

    /// Recover the lookup prefix from a presented secret.
    ///
    /// Returns `None` when the value is not shaped like a platform credential,
    /// which the caller must render indistinguishable from a wrong secret.
    #[must_use]
    pub fn prefix_of(presented: &SecretString) -> Option<String> {
        let raw = presented.expose_secret();
        let mut parts = raw.split('_');
        if parts.next() != Some("wyrd") || parts.next() != Some("global") {
            return None;
        }
        let visible = parts.next()?;
        parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        Some(format!("{PLATFORM_KEY_PREFIX}_{visible}"))
    }
}

/// The verifier every rejected credential is checked against.
///
/// Derived once per process from throwaway randomness, so no presented secret
/// can match it and the cost of failing is the cost of succeeding. Computing it
/// lazily rather than per request matters: Argon2 is deliberately expensive,
/// and paying for the dummy on every rejection would be a denial-of-service
/// amplifier rather than a timing defence.
static DUMMY_VERIFIER: LazyLock<String> = LazyLock::new(|| {
    hash_api_key(&SecretString::from(Uuid::new_v4().to_string()))
        .expect("Argon2 hashing generated randomness cannot fail")
});

/// A credential that authenticated, with the identity of both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticatedPlatformCredential {
    /// Principal the credential authenticates.
    pub principal_id: PrincipalId,
    /// The credential itself, so a session can be bound to it.
    pub credential_id: Uuid,
}

/// A credential that was just issued, with its durable id.
#[derive(Debug)]
pub struct IssuedPlatformCredential {
    /// Durable credential row id, for later listing or revocation.
    pub id: Uuid,
    /// The plaintext, returned once.
    pub credential: PlatformCredential,
}

/// Platform credential issuance and verification failure.
///
/// Authentication failures collapse into a single [`Self::InvalidCredential`]
/// variant on purpose: an unknown prefix, a wrong secret, an expired or revoked
/// credential, and a suspended principal must be indistinguishable to a caller,
/// or the error itself becomes an oracle for enumerating principals.
#[derive(Debug, thiserror::Error)]
pub enum PlatformCredentialError {
    /// The presented credential is unusable, for any reason.
    #[error("invalid platform credential")]
    InvalidCredential,
    /// Argon2 hashing failed.
    #[error("platform credential hash failed")]
    Hash(#[from] IssueError),
    /// The hashing task could not be joined.
    #[error("platform credential hash task failed")]
    Join(#[from] tokio::task::JoinError),
    /// The platform store rejected the operation.
    #[error("platform credential store failed: {0}")]
    Store(#[from] SqlError),
}

/// Issues and verifies platform-scope credentials.
///
/// Owns the operator boundary it writes through, so callers discover credential
/// work as `credentials.issue(...)` and
/// `credentials.authenticate_for_session(...)` rather
/// than threading a pool through every call.
#[derive(Clone)]
pub struct PlatformCredentials {
    /// Cross-tenant boundary the platform credential store lives behind.
    pool: OperatorPool,
}

impl std::fmt::Debug for PlatformCredentials {
    /// Prints the handle without its pool: a connection source has no
    /// inspectable state and printing it would only add noise to a trace.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformCredentials")
            .finish_non_exhaustive()
    }
}

impl PlatformCredentials {
    /// Bind credential issuance and verification to one operator boundary.
    #[must_use]
    pub const fn new(pool: OperatorPool) -> Self {
        Self { pool }
    }

    /// Mint a credential for an existing platform principal.
    ///
    /// Generates the secret, hashes it off the async runtime because Argon2 is
    /// deliberately expensive, persists only the verifier, and hands the
    /// plaintext back for its single exposure. `expires_at` is optional: an
    /// administrative credential established at initialization has no natural
    /// lifetime.
    ///
    /// The insert runs on the caller's transaction — the one already carrying
    /// the authorization allowance — and the caller commits. A credential that
    /// committed on its own would be a usable secret the deployment never
    /// recorded permitting.
    ///
    /// # Errors
    /// Returns [`PlatformCredentialError::Hash`] or
    /// [`PlatformCredentialError::Join`] when hashing fails, and
    /// [`PlatformCredentialError::Store`] when the insert is rejected —
    /// including when `principal_id` names no platform principal.
    #[tracing::instrument(level = "debug", skip(self, conn), fields(principal_id = %principal_id), err)]
    pub async fn issue(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<IssuedPlatformCredential, PlatformCredentialError> {
        let credential = PlatformCredential::generate();
        let raw = credential.secret.clone();
        let secret_hash = tokio::task::spawn_blocking(move || hash_api_key(&raw)).await??;

        let id = Uuid::new_v4();
        insert_platform_credential_tx(
            conn,
            id,
            principal_id,
            &credential.prefix,
            &secret_hash,
            expires_at,
        )
        .await?;

        Ok(IssuedPlatformCredential { id, credential })
    }

    /// Authenticate a credential and report which credential it was.
    ///
    /// The exchange path needs the credential's identity as well as its
    /// principal, so a minted session can be tied to — and revoked with — the
    /// credential that produced it.
    ///
    /// # Errors
    /// Returns [`PlatformCredentialError::InvalidCredential`] for every
    /// rejection and [`PlatformCredentialError::Store`] when the lookup fails.
    pub async fn authenticate_for_session(
        &self,
        presented: &SecretString,
    ) -> Result<AuthenticatedPlatformCredential, PlatformCredentialError> {
        let row = match PlatformCredential::prefix_of(presented) {
            Some(prefix) => platform_credential_by_prefix(&self.pool, &prefix)
                .await?
                .filter(|row| row.is_usable(Utc::now())),
            None => None,
        };

        // Exactly one verification, whatever was wrong with the input. A
        // malformed shape, an unknown prefix, and a revoked, expired or
        // suspended credential all used to answer before Argon2 ran, so a live
        // prefix with a wrong tail took visibly longer than any of them and the
        // endpoint enumerated live prefixes by clock. Verifying the dummy costs
        // what verifying a real row costs, so there is nothing left to measure.
        let verifier = row
            .as_ref()
            .map_or_else(|| DUMMY_VERIFIER.clone(), |row| row.secret_hash.clone());
        let candidate = presented.clone();
        let matched =
            tokio::task::spawn_blocking(move || verify_api_key(&candidate, &verifier)).await?;

        let Some(row) = row.filter(|_| matched) else {
            return Err(PlatformCredentialError::InvalidCredential);
        };
        touch_platform_credential(&self.pool, row.id).await?;
        Ok(AuthenticatedPlatformCredential {
            principal_id: PrincipalId::new(row.principal_id),
            credential_id: row.id,
        })
    }
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};

    use super::{PLATFORM_KEY_PREFIX, PlatformCredential};

    /// A generated credential carries its prefix and enough trailing entropy
    /// that the visible half never reveals the secret half.
    #[test]
    fn generated_credential_starts_with_its_prefix_and_extends_it() {
        let credential = PlatformCredential::generate();

        assert!(credential.prefix.starts_with(PLATFORM_KEY_PREFIX));
        let secret = credential.secret.expose_secret();
        assert!(secret.starts_with(&credential.prefix));
        assert!(
            secret.len() > credential.prefix.len() + 16,
            "the secret extends well beyond its visible prefix"
        );
    }

    /// Two generations never collide, so a prefix identifies one credential.
    #[test]
    fn generation_is_unique() {
        let first = PlatformCredential::generate();
        let second = PlatformCredential::generate();

        assert_ne!(first.prefix, second.prefix);
        assert_ne!(first.secret.expose_secret(), second.secret.expose_secret());
    }

    /// The lookup prefix round-trips out of a presented secret.
    #[test]
    fn prefix_round_trips_from_a_presented_secret() {
        let credential = PlatformCredential::generate();

        assert_eq!(
            PlatformCredential::prefix_of(&credential.secret),
            Some(credential.prefix)
        );
    }

    /// Anything not shaped like a platform credential yields no prefix, so it
    /// cannot reach the store and cannot be distinguished from a wrong secret.
    #[test]
    fn malformed_values_yield_no_prefix() {
        for raw in [
            "",
            "wyrd_global",
            "wyrd_global_only",
            "wyrd_sk_tenant_visible_secret",
            "wyrd_global_visible_secret_extra",
            "not-a-credential",
        ] {
            assert_eq!(
                PlatformCredential::prefix_of(&SecretString::from(raw.to_owned())),
                None,
                "{raw} must not resolve to a lookup prefix"
            );
        }
    }
}

#[cfg(test)]
mod pg_tests {
    //! Issuance and verification against real Postgres.
    //!
    //! These prove the properties the durable layer cannot: that the plaintext
    //! is generated here and never stored, and that every rejection is one
    //! indistinguishable outcome.

    use chrono::{Duration, Utc};
    use secrecy::{ExposeSecret, SecretString};
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_sql::queries::platform::credentials::revoke_platform_credential;
    use wyrd_sql::queries::platform::principals::insert_platform_principal;

    use super::{PlatformCredentialError, PlatformCredentials};

    /// Seed a platform principal and return its id.
    async fn seed_principal(fixture: &PgFixture, name: &str) -> Uuid {
        let id = Uuid::now_v7();
        insert_platform_principal(
            fixture.operator_pool(),
            id,
            PrincipalKindTag::GlobalAdmin,
            name,
        )
        .await
        .expect("platform principal inserts");
        id
    }

    /// Issue one credential and commit it, as the route does.
    ///
    /// Issuance now writes on the caller's transaction so the credential and
    /// the allowance permitting it commit together; these tests want the row
    /// standing, so they commit immediately.
    async fn issue_committed(
        fixture: &PgFixture,
        principal: Uuid,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> super::IssuedPlatformCredential {
        let pool = fixture.operator_pool().clone();
        let mut conn = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        let issued = PlatformCredentials::new(pool.clone())
            .issue(&mut conn, principal, expires_at)
            .await
            .expect("credential issues");
        conn.commit().await.expect("credential commits");
        issued
    }

    /// The issued plaintext authenticates back to its principal, and the value
    /// stored is a verifier that is not the plaintext and cannot reproduce it.
    #[tokio::test]
    async fn issued_plaintext_authenticates_and_is_never_stored() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = seed_principal(&fixture, "issuing").await;

        let issued = issue_committed(&fixture, principal, None).await;
        let plaintext = issued.credential.secret.expose_secret().to_owned();

        let resolved = PlatformCredentials::new(pool.clone())
            .authenticate_for_session(&issued.credential.secret)
            .await
            .expect("the issued credential authenticates");
        assert_eq!(resolved.principal_id.as_uuid(), principal);

        let stored: String =
            sqlx::query_scalar("SELECT secret_hash FROM platform.credentials WHERE id = $1")
                .bind(issued.id)
                .fetch_one(pool.pool())
                .await
                .expect("verifier reads back");
        assert_ne!(stored, plaintext, "the plaintext is never persisted");
        assert!(
            !stored.contains(plaintext.rsplit('_').next().expect("secret has a tail")),
            "the stored verifier does not contain the secret material"
        );
        assert!(
            stored.starts_with("$argon2"),
            "an Argon2 verifier is stored"
        );
    }

    /// Every rejection — unknown, malformed, wrong secret, revoked, expired,
    /// suspended principal — is the same error, so it cannot be used as an
    /// oracle.
    #[tokio::test]
    async fn every_rejection_is_the_same_error() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();

        let live_owner = seed_principal(&fixture, "live").await;
        let live = issue_committed(&fixture, live_owner, None).await;

        let revoked_owner = seed_principal(&fixture, "revoked").await;
        let revoked = issue_committed(&fixture, revoked_owner, None).await;
        let mut revocation = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        revoke_platform_credential(&mut revocation, revoked.id)
            .await
            .expect("revocation succeeds");
        revocation.commit().await.expect("revocation commits");

        let expired_owner = seed_principal(&fixture, "expired").await;
        let expired = issue_committed(
            &fixture,
            expired_owner,
            Some(Utc::now() - Duration::hours(1)),
        )
        .await;

        let suspended_owner = seed_principal(&fixture, "suspended").await;
        let suspended = issue_committed(&fixture, suspended_owner, None).await;
        sqlx::query("UPDATE platform.principals SET status = 'suspended' WHERE id = $1")
            .bind(suspended_owner)
            .execute(pool.pool())
            .await
            .expect("suspension succeeds");

        // A well-formed secret whose tail is wrong: same prefix, different body.
        let wrong_secret = SecretString::from(format!(
            "{}_{}",
            live.credential.prefix,
            Uuid::now_v7().simple()
        ));

        let rejections = [
            (
                "malformed",
                SecretString::from("not-a-credential".to_owned()),
            ),
            (
                "unknown prefix",
                SecretString::from(format!("wyrd_global_{}_{}", "a".repeat(12), "b".repeat(32))),
            ),
            ("wrong secret", wrong_secret),
            ("revoked", revoked.credential.secret),
            ("expired", expired.credential.secret),
            ("suspended principal", suspended.credential.secret),
        ];

        // Warm the dummy verifier so its one-off derivation is not mistaken for
        // the per-request cost this measures.
        let _ = PlatformCredentials::new(pool.clone())
            .authenticate_for_session(&SecretString::from("warm".to_owned()))
            .await;

        let mut elapsed = Vec::new();
        for (label, presented) in rejections {
            let started = std::time::Instant::now();
            let error = PlatformCredentials::new(pool.clone())
                .authenticate_for_session(&presented)
                .await
                .expect_err("rejection");
            elapsed.push((label, started.elapsed()));
            assert!(
                matches!(error, PlatformCredentialError::InvalidCredential),
                "{label} must be indistinguishable, got {error:?}"
            );
            assert_eq!(
                error.to_string(),
                "invalid platform credential",
                "{label} must render identically"
            );
        }

        // Identical error bodies are not enough: a rejection that skips Argon2
        // answers orders of magnitude sooner and says so. Argon2 dominates every
        // one of these requests, so a shape that verified nothing would land
        // far below the one that verified a real row against a wrong secret.
        let wrong_secret_cost = elapsed
            .iter()
            .find(|(label, _)| *label == "wrong secret")
            .expect("the known-prefix rejection was measured")
            .1;
        for (label, cost) in &elapsed {
            assert!(
                *cost * 3 >= wrong_secret_cost,
                "{label} rejected in {cost:?} against {wrong_secret_cost:?} for a wrong secret, \
                 so it skipped the verification that hides which prefixes are live"
            );
        }

        PlatformCredentials::new(pool.clone())
            .authenticate_for_session(&live.credential.secret)
            .await
            .expect("the live credential still authenticates");
    }

    /// Revocation takes effect on the very next request.
    ///
    /// The platform plane caches nothing. Its session token is verified for
    /// signature and scope, and then every request re-reads the credential
    /// record, its principal, and the principal's grant from the store, so a
    /// revoked credential that worked a moment ago stops working immediately.
    /// That is why this plane needs no revocation epoch: the tenant plane
    /// advances one because it caches verified tokens and a cached verification
    /// would otherwise outlive the revocation, and there is no such cache here
    /// to invalidate.
    #[tokio::test]
    async fn revocation_takes_effect_on_the_next_request() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = seed_principal(&fixture, "revoked-mid-life").await;
        let issued = issue_committed(&fixture, principal, None).await;

        PlatformCredentials::new(pool.clone())
            .authenticate_for_session(&issued.credential.secret)
            .await
            .expect("the credential works before revocation");

        let mut revocation = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        revoke_platform_credential(&mut revocation, issued.id)
            .await
            .expect("revocation succeeds");
        revocation.commit().await.expect("revocation commits");

        let error = PlatformCredentials::new(pool.clone())
            .authenticate_for_session(&issued.credential.secret)
            .await
            .expect_err("the same credential stops working immediately");
        assert!(matches!(error, PlatformCredentialError::InvalidCredential));
    }

    /// Successful use is recorded as operator-facing metadata only.
    #[tokio::test]
    async fn successful_use_records_last_used() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.operator_pool();
        let principal = seed_principal(&fixture, "touched").await;
        let issued = issue_committed(&fixture, principal, None).await;

        PlatformCredentials::new(pool.clone())
            .authenticate_for_session(&issued.credential.secret)
            .await
            .expect("authenticates");

        let last_used: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT last_used_at FROM platform.credentials WHERE id = $1")
                .bind(issued.id)
                .fetch_one(pool.pool())
                .await
                .expect("metadata reads back");
        assert!(last_used.is_some(), "successful use is recorded");
    }
}
