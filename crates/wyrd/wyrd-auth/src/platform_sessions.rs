//! Platform-scope session tokens.
//!
//! The platform plane follows the same two-layer shape as the tenant plane: a
//! credential is a bootstrap credential exchanged once for a short-lived token,
//! and every request thereafter presents the token. Request handling therefore
//! never reads credential material, a lookup prefix, or a credential record.
//!
//! What differs is what the token can say. A platform token carries no tenant,
//! no roles, and no delegation chain, so it cannot be mistaken for — or
//! replayed as — a tenant token, and authority is resolved from the principal's
//! grant at verification time rather than frozen at mint time.

use std::sync::Arc;

use chrono::{Duration, Utc};
use secrecy::SecretString;
use uuid::Uuid;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::{PLATFORM_TOKEN_SCOPE, PlatformAccessTokenClaims};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::platform::credentials::platform_credential_by_id;
use wyrd_sql::queries::platform::identity::{
    pin_platform_identity, platform_identity_by_subject_tx,
};
use wyrd_sql::queries::platform::principals::{
    platform_principal_by_id, platform_principal_by_id_tx,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn};

use crate::audit::{TOKEN_EXCHANGE_OPERATION, auth_event, principal_kind_tag};
use crate::platform_credentials::{PlatformCredentialError, authenticate_for_session};
use std::fmt::{Debug, Formatter, Result as FmtResult};

/// Default platform session lifetime.
///
/// Short for the same reason tenant access tokens are: a bearer token is
/// replayable until it expires, and the platform plane is the most privileged
/// surface in the deployment.
pub const DEFAULT_PLATFORM_TOKEN_TTL_MINUTES: i64 = 15;

/// A platform session that was just minted.
#[derive(Debug)]
pub struct PlatformSession {
    /// The bearer token, presented on subsequent requests.
    pub token: SecretString,
    /// Principal the session acts as.
    pub principal_id: PrincipalId,
    /// Credential that minted it; revoking that credential ends this session.
    pub credential_id: Uuid,
}

/// A verified platform session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedPlatformSession {
    /// Principal the request acts as.
    pub principal_id: PrincipalId,
    /// The kind the directory stores for that principal, read here rather than
    /// carried in the token so a re-registered or corrected kind takes effect
    /// on the next request.
    pub principal_kind: PrincipalKindTag,
    /// Credential that minted the presented token, recorded in audit so an
    /// operation is traceable to the credential as well as the identity.
    ///
    /// `None` for a session established by federated login, where the identity
    /// is the whole story and no credential was presented.
    pub credential_id: Option<Uuid>,
}

/// Platform session failure.
#[derive(Debug, thiserror::Error)]
pub enum PlatformSessionError {
    /// The presented credential or token is unusable, for any reason.
    ///
    /// One variant on purpose: an unknown credential, a wrong secret, a
    /// malformed or expired token, a revoked credential, and a suspended
    /// principal must be indistinguishable, or the error becomes an oracle.
    #[error("invalid platform session")]
    Invalid,
    /// Signing or verification material failed.
    #[error("platform session key operation failed: {0}")]
    Key(String),
    /// The platform store could not be reached.
    #[error("platform session store failed: {0}")]
    Store(#[from] SqlError),
}

impl From<PlatformCredentialError> for PlatformSessionError {
    /// Narrow a credential failure to what a session caller may learn.
    ///
    /// Only two outcomes carry through: the credential was not usable, or the
    /// store failed. Every other cause collapses into an opaque key error, so
    /// the exchange route cannot leak which condition rejected a presented
    /// credential.
    fn from(error: PlatformCredentialError) -> Self {
        match error {
            PlatformCredentialError::InvalidCredential => Self::Invalid,
            PlatformCredentialError::Store(error) => Self::Store(error),
            other => Self::Key(other.to_string()),
        }
    }
}

/// Mints and verifies platform-scope session tokens.
///
/// Owns the operator boundary, the signing key, and the session lifetime,
/// because a session is only meaningful as the composition of all three.
pub struct PlatformSessions {
    /// Cross-tenant boundary the credential and grant stores live behind.
    pool: OperatorPool,
    /// Key this deployment signs platform sessions with.
    issuing_key: Arc<IssuingKey>,
    /// Lifetime minted sessions carry.
    ttl: Duration,
}

impl Debug for PlatformSessions {
    /// Prints the handle without its key or pool, neither of which may reach a
    /// log or trace.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("PlatformSessions")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl PlatformSessions {
    /// Bind session minting and verification to one boundary and key.
    #[must_use]
    pub fn new(pool: OperatorPool, issuing_key: Arc<IssuingKey>) -> Self {
        Self {
            pool,
            issuing_key,
            ttl: Duration::minutes(DEFAULT_PLATFORM_TOKEN_TTL_MINUTES),
        }
    }

    /// Override the minted session lifetime.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Exchange a platform credential for a short-lived session token.
    ///
    /// This is the one place credential material is read. Everything after it
    /// presents the returned token instead.
    ///
    /// # Errors
    /// Returns [`PlatformSessionError::Invalid`] for every credential
    /// rejection, [`PlatformSessionError::Key`] when signing fails, and
    /// [`PlatformSessionError::Store`] when the store cannot be reached.
    #[tracing::instrument(level = "debug", skip(self, presented), err)]
    pub async fn exchange(
        &self,
        presented: &SecretString,
        request_id: &str,
    ) -> Result<PlatformSession, PlatformSessionError> {
        let mut conn = self.pool.begin_platform_audited().await?;
        let authenticated = authenticate_for_session(&mut conn, presented).await?;
        let token = self
            .issuing_key
            .issue_platform_access_token(
                authenticated.principal_id,
                Some(authenticated.credential_id),
                self.ttl,
            )
            .map_err(|error| PlatformSessionError::Key(error.to_string()))?;
        self.record_grant(
            &mut conn,
            authenticated.principal_id,
            authenticated.principal_kind,
            Some(authenticated.credential_id),
            request_id,
        )
        .await?;
        conn.commit().await?;

        Ok(PlatformSession {
            token: SecretString::from(token),
            principal_id: authenticated.principal_id,
            credential_id: authenticated.credential_id,
        })
    }

    /// Mint a platform session for an identity federated login just verified.
    ///
    /// Owns the whole grant boundary: resolving the pinned subject, pinning a
    /// pre-registration on its first login, re-reading the principal, minting
    /// the token, and appending the canonical record all run on one audited
    /// transaction. Pinning is permanent and one-way, so committing it ahead of
    /// the grant would leave durable identity state that nothing audited and
    /// nobody granted whenever the append or the signing then failed. The
    /// caller passes only what it verified — the issuer it checked the token
    /// against, the subject the token asserted, and the verified claim to match
    /// a first login on — and gets a token or nothing.
    ///
    /// No credential is involved: the provider token established who this is.
    /// The principal is re-read here so a suspended administrator cannot obtain
    /// a session even with a valid provider token, which keeps "authority is
    /// read from the store, not the token" true on this path too.
    ///
    /// `match_claim` is `None` when the provider asserted no claim this
    /// deployment will pin on, which can only refuse a first login.
    ///
    /// # Errors
    /// Returns [`PlatformSessionError::Invalid`] when no registration matches
    /// the subject or claim, when another login pinned it first, or when the
    /// resolved principal is not active, [`PlatformSessionError::Key`] when
    /// signing fails, and [`PlatformSessionError::Store`] when the store fails.
    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn issue_federated(
        &self,
        issuer: &str,
        subject: &str,
        match_claim: Option<&str>,
        request_id: &str,
    ) -> Result<SecretString, PlatformSessionError> {
        let mut conn = self.pool.begin_platform_audited().await?;
        let pinned = platform_identity_by_subject_tx(&mut conn, issuer, subject).await?;
        let principal_id = if let Some(identity) = pinned {
            identity.principal_id
        } else {
            // First login: match the pre-registered claim and pin the subject.
            // The store's `subject IS NULL` predicate makes this a one-time
            // transition, so a concurrent second login pins nothing and is
            // refused rather than racing.
            let Some(claim) = match_claim else {
                return Err(PlatformSessionError::Invalid);
            };
            pin_platform_identity(&mut conn, issuer, claim, subject)
                .await?
                .ok_or(PlatformSessionError::Invalid)?
        };

        let Some(principal) = platform_principal_by_id_tx(&mut conn, principal_id).await? else {
            return Err(PlatformSessionError::Invalid);
        };
        if !principal.is_active() {
            return Err(PlatformSessionError::Invalid);
        }

        let token = self
            .issuing_key
            .issue_platform_access_token(PrincipalId::new(principal_id), None, self.ttl)
            .map_err(|error| PlatformSessionError::Key(error.to_string()))?;
        // A federated grant names the principal the provider resolved to, the
        // kind the directory stores for it, and no credential, because none was
        // presented.
        self.record_grant(
            &mut conn,
            PrincipalId::new(principal_id),
            principal_kind_tag(&principal.principal_kind),
            None,
            request_id,
        )
        .await?;
        conn.commit().await?;
        Ok(SecretString::from(token))
    }

    /// Append the one canonical grant record for a platform session.
    ///
    /// Both platform grants — credential exchange and federated issuance —
    /// converge here, so the record cannot be present on one path and missing
    /// on the other. It is appended on the caller's audited operator
    /// transaction, which is what makes the token and its record inseparable:
    /// an append failure drops the transaction, so no grant-side effect
    /// survives and no token is returned.
    ///
    /// `principal_kind` is the kind the directory stores for this principal,
    /// read on the grant's own transaction rather than assumed: a federated
    /// human is registered as a `user`, and recording every platform grant as
    /// the deployment root would make retained history attribute an operator's
    /// sign-in to the machine identity.
    ///
    /// `credential_id` names the credential spent, and is absent for a
    /// federated session where the identity is the whole story.
    ///
    /// # Errors
    /// Returns [`PlatformSessionError::Store`] when the append is rejected.
    async fn record_grant(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: PrincipalId,
        principal_kind: PrincipalKindTag,
        credential_id: Option<Uuid>,
        request_id: &str,
    ) -> Result<(), PlatformSessionError> {
        let expires_at = Utc::now() + self.ttl;
        let mut event = auth_event(
            request_id,
            TOKEN_EXCHANGE_OPERATION,
            principal_id,
            principal_kind,
            None,
            AuditOutcome::Allowed,
            AuditDetail::TokenExchange {
                subject_principal_id: principal_id,
                actor_principal_id: principal_id,
                delegation_chain: Vec::new(),
                expires_at,
            },
        );
        event.credential_id = credential_id;
        vala_sql::queries::audit_staging::append_audit(conn, &event)
            .await
            .map_err(PlatformSessionError::Store)?;
        Ok(())
    }

    /// Confirm that verified platform claims still name a live session.
    ///
    /// Signature, issuer, and scope are settled by the token verifier; this is
    /// the half that cannot be settled from the token alone. Re-reading the
    /// credential that minted it is what makes revoking a credential end its
    /// sessions immediately, without a separate authorization epoch.
    ///
    /// # Errors
    /// Returns [`PlatformSessionError::Invalid`] when the claims are malformed,
    /// the credential is unknown, revoked or expired, the principal is not
    /// active, or the token names a credential belonging to another principal.
    /// Returns [`PlatformSessionError::Store`] when the credential state cannot
    /// be read — an unavailable store is not an invalid session.
    #[tracing::instrument(level = "debug", skip(self, claims), err)]
    pub async fn confirm(
        &self,
        claims: &PlatformAccessTokenClaims,
    ) -> Result<VerifiedPlatformSession, PlatformSessionError> {
        if claims.scope != PLATFORM_TOKEN_SCOPE {
            return Err(PlatformSessionError::Invalid);
        }
        let Ok(principal_id) = claims.sub.parse::<Uuid>() else {
            return Err(PlatformSessionError::Invalid);
        };

        // Either anchor answers the same question — is the authority this token
        // was minted under still live — and both re-read it from the store
        // rather than trusting the token.
        match claims.cid.as_deref() {
            Some(cid) => self.confirm_credential_session(principal_id, cid).await,
            None => self.confirm_federated_session(principal_id).await,
        }
    }

    /// Confirm a session minted from a credential.
    ///
    /// Re-reading the credential is what makes revoking it end its sessions at
    /// once. The row carries the principal's status, so a suspended principal
    /// is rejected by the same read.
    ///
    /// # Errors
    /// Returns [`PlatformSessionError::Invalid`] when the credential id is
    /// malformed, unknown, revoked, expired, belongs to another principal, or
    /// its principal is not active, and [`PlatformSessionError::Store`] when
    /// the credential cannot be read.
    async fn confirm_credential_session(
        &self,
        principal_id: Uuid,
        cid: &str,
    ) -> Result<VerifiedPlatformSession, PlatformSessionError> {
        let Ok(credential_id) = cid.parse::<Uuid>() else {
            return Err(PlatformSessionError::Invalid);
        };
        let Some(row) = platform_credential_by_id(&self.pool, credential_id).await? else {
            return Err(PlatformSessionError::Invalid);
        };
        if row.principal_id != principal_id || !row.is_usable(Utc::now()) {
            return Err(PlatformSessionError::Invalid);
        }

        Ok(VerifiedPlatformSession {
            principal_id: PrincipalId::new(principal_id),
            principal_kind: principal_kind_tag(&row.principal_kind),
            credential_id: Some(credential_id),
        })
    }

    /// Confirm a session established by federated login.
    ///
    /// There is no credential to re-read, so the principal itself is the
    /// anchor: suspending it ends every session it holds on the next request,
    /// which is the human equivalent of revoking a credential.
    ///
    /// # Errors
    /// Returns [`PlatformSessionError::Invalid`] when the principal is unknown
    /// or not active, and [`PlatformSessionError::Store`] when it cannot be
    /// read.
    async fn confirm_federated_session(
        &self,
        principal_id: Uuid,
    ) -> Result<VerifiedPlatformSession, PlatformSessionError> {
        let Some(principal) = platform_principal_by_id(&self.pool, principal_id).await? else {
            return Err(PlatformSessionError::Invalid);
        };
        if !principal.is_active() {
            return Err(PlatformSessionError::Invalid);
        }

        Ok(VerifiedPlatformSession {
            principal_id: PrincipalId::new(principal_id),
            principal_kind: principal_kind_tag(&principal.principal_kind),
            credential_id: None,
        })
    }
}

#[cfg(test)]
mod pg_tests {
    //! Durable proof that every platform grant is recorded before it is served.
    //!
    //! A platform session is the most privileged bearer the deployment issues.
    //! Both ways of obtaining one — spending a credential and federated login —
    //! must leave exactly one canonical grant row, and an append the store
    //! refuses must leave neither a token nor any grant-side effect.

    use std::sync::Arc;

    use secrecy::{ExposeSecret, SecretString};
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::Kid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_sql::queries::platform::identity::insert_platform_identity_tx;
    use wyrd_sql::queries::platform::principals::insert_platform_principal;

    use super::{PlatformSessionError, PlatformSessions};
    use crate::audit::TOKEN_EXCHANGE_OPERATION;
    use crate::platform_credentials::issue_platform_credential;

    /// The issuer these tests register federated identities against.
    const TEST_ISSUER: &str = "https://issuer.test";

    /// A throwaway Ed25519 key these tests mint and verify platform sessions
    /// with, so no test depends on deployment key material.
    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    /// The signing key platform sessions are minted with in these tests.
    fn issuing_key() -> Arc<IssuingKey> {
        Arc::new(
            IssuingKey::from_ed_pem(
                SecretString::from(PRIVATE_KEY_PEM),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("test private key loads"),
        )
    }

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

    /// Issue one credential for `principal` and commit it, as the route does.
    async fn issue_credential(fixture: &PgFixture, principal: Uuid) -> SecretString {
        let pool = fixture.operator_pool().clone();
        let mut conn = pool
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        let issued = issue_platform_credential(&mut conn, principal, None)
            .await
            .expect("credential issues");
        conn.commit().await.expect("credential commits");
        issued.credential.secret
    }

    /// Seed an inactive-free platform principal of `kind` plus an unpinned
    /// federated pre-registration, as the administrative registration route
    /// does, and return its id.
    ///
    /// A federated administrator is registered as a `user`: the deployment root
    /// kind belongs to the machine identity that bootstrapped the platform, not
    /// to a human who signs in through a provider.
    async fn seed_registered_human(fixture: &PgFixture, name: &str, claim: &str) -> Uuid {
        let id = Uuid::now_v7();
        insert_platform_principal(fixture.operator_pool(), id, PrincipalKindTag::User, name)
            .await
            .expect("platform principal inserts");
        let mut conn = fixture
            .operator_pool()
            .begin_platform_audited()
            .await
            .expect("transaction opens");
        insert_platform_identity_tx(&mut conn, id, TEST_ISSUER, claim)
            .await
            .expect("pre-registration inserts");
        conn.commit().await.expect("pre-registration commits");
        id
    }

    /// Read the subject pinned onto `principal`'s pre-registration, if any.
    async fn pinned_subject(fixture: &PgFixture, principal: Uuid) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT subject FROM platform.principal_identities WHERE principal_id = $1",
        )
        .bind(principal)
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("pre-registration reads")
    }

    /// Read the grant records staged for `principal` and the kind and
    /// credential each one names.
    ///
    /// Staging is written by the platform transaction and read back here as the
    /// superuser, because `wyrd_platform_admin` is granted append-only access to
    /// `vala.audit_staging` and cannot select from it.
    async fn staged_grant_attribution(
        fixture: &PgFixture,
        principal: Uuid,
    ) -> Vec<(String, Option<Uuid>)> {
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query_as::<_, (String, Option<Uuid>)>(
            "SELECT principal_kind, credential_id FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND operation = $2 AND principal_id = $3",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(TOKEN_EXCHANGE_OPERATION)
        .bind(principal)
        .fetch_all(&admin)
        .await
        .expect("grant record query runs")
    }

    /// Spending a credential commits one grant record naming that credential
    /// and the kind the directory stores for its owner.
    ///
    /// # Panics
    /// Panics when the exchange fails or the record is missing, duplicated, or
    /// misattributed.
    #[tokio::test]
    async fn a_credential_exchange_commits_one_attributed_grant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let principal = seed_principal(&fixture, "exchange-audited").await;
        let secret = issue_credential(&fixture, principal).await;

        let session = PlatformSessions::new(fixture.operator_pool().clone(), issuing_key())
            .exchange(&secret, "req-platform-exchange")
            .await
            .expect("the credential exchanges");

        let grants = staged_grant_attribution(&fixture, principal).await;
        assert_eq!(grants.len(), 1, "exactly one grant record is committed");
        assert_eq!(
            grants[0],
            ("global_admin".to_owned(), Some(session.credential_id)),
            "the grant names the stored kind and the credential that was spent"
        );
    }

    /// A first federated login pins, grants, and audits as one boundary, or
    /// leaves nothing behind.
    ///
    /// Pinning a subject is permanent and one-way, so a pin that outlived a
    /// failed grant would be durable identity state nothing audited and nobody
    /// was granted — and the human it silently claimed could never be
    /// re-registered. The grant also has to record the kind the directory
    /// stores: a federated human is a `user`, and attributing their sign-in to
    /// the deployment root would make retained history name the wrong identity.
    ///
    /// # Panics
    /// Panics when the failed attempt leaves a pin or a token, when the retry
    /// does not produce exactly one pin and one grant, or when that grant is
    /// misattributed.
    #[tokio::test]
    async fn a_first_federated_login_pins_grants_and_audits_atomically() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let principal =
            seed_registered_human(&fixture, "federated-human", "admin@example.test").await;
        let sessions = PlatformSessions::new(fixture.operator_pool().clone(), issuing_key());

        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("REVOKE INSERT ON vala.audit_staging FROM wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("append privilege revoked");

        let refused = sessions
            .issue_federated(
                TEST_ISSUER,
                "subject-1",
                Some("admin@example.test"),
                "req-platform-unauditable",
            )
            .await;

        sqlx::query("GRANT INSERT ON vala.audit_staging TO wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("append privilege restored");

        assert!(
            matches!(refused, Err(PlatformSessionError::Store(_))),
            "an unauditable grant is a store failure, not a rejected identity: {refused:?}"
        );
        assert_eq!(
            pinned_subject(&fixture, principal).await,
            None,
            "a failed grant leaves the pre-registration unpinned"
        );

        let token = sessions
            .issue_federated(
                TEST_ISSUER,
                "subject-1",
                Some("admin@example.test"),
                "req-platform-federated",
            )
            .await
            .expect("the retry is granted a session");
        assert!(!token.expose_secret().is_empty());

        assert_eq!(
            pinned_subject(&fixture, principal).await.as_deref(),
            Some("subject-1"),
            "the retry pins the subject exactly once"
        );
        let grants = staged_grant_attribution(&fixture, principal).await;
        assert_eq!(grants.len(), 1, "exactly one grant record is committed");
        assert_eq!(
            grants[0],
            ("user".to_owned(), None),
            "the grant names the stored kind and no credential, because none was presented"
        );
    }

    /// An audit store that refuses the append returns no token and no effect.
    ///
    /// The grant, its record, and the credential's last-used touch share one
    /// transaction, so a refused append rolls all of it back. A token handed
    /// out here would be a platform session the deployment cannot account for.
    ///
    /// # Panics
    /// Panics when the exchange succeeds, when the failure is reported as an
    /// invalid credential rather than a store failure, or when the rolled-back
    /// touch survived.
    #[tokio::test]
    async fn a_refused_audit_append_returns_no_token_and_no_effect() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let principal = seed_principal(&fixture, "audit-refused").await;
        let secret = issue_credential(&fixture, principal).await;

        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("REVOKE INSERT ON vala.audit_staging FROM wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("append privilege revoked");

        let result = PlatformSessions::new(fixture.operator_pool().clone(), issuing_key())
            .exchange(&secret, "req-platform-unauditable")
            .await;

        sqlx::query("GRANT INSERT ON vala.audit_staging TO wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("append privilege restored");

        assert!(
            matches!(result, Err(PlatformSessionError::Store(_))),
            "an unrecordable grant is refused as a store failure, got: {result:?}"
        );
        assert!(
            staged_grant_attribution(&fixture, principal)
                .await
                .is_empty(),
            "no grant record survives the refusal"
        );
        let touched: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar(
            "SELECT last_used_at FROM platform.credentials WHERE principal_id = $1",
        )
        .bind(principal)
        .fetch_optional(&admin)
        .await
        .expect("credential metadata reads back");
        assert_eq!(
            touched,
            Some(None),
            "the rolled-back grant leaves no record of a use that produced no token"
        );
    }
}
