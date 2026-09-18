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
use wyrd_spec::auth::PrincipalId;
use wyrd_sql::queries::platform::credentials::platform_credential_by_id;
use wyrd_sql::{OperatorPool, SqlError};

use crate::platform_credentials::{PlatformCredentialError, PlatformCredentials};

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
    /// Credential that minted the presented token, recorded in audit so an
    /// operation is traceable to the credential as well as the identity.
    pub credential_id: Uuid,
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

impl std::fmt::Debug for PlatformSessions {
    /// Prints the handle without its key or pool, neither of which may reach a
    /// log or trace.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    ) -> Result<PlatformSession, PlatformSessionError> {
        let credentials = PlatformCredentials::new(self.pool.clone());
        let authenticated = credentials.authenticate_for_session(presented).await?;
        let token = self
            .issuing_key
            .issue_platform_access_token(
                authenticated.principal_id,
                authenticated.credential_id,
                self.ttl,
            )
            .map_err(|error| PlatformSessionError::Key(error.to_string()))?;

        Ok(PlatformSession {
            token: SecretString::from(token),
            principal_id: authenticated.principal_id,
            credential_id: authenticated.credential_id,
        })
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
        let (Ok(principal_id), Ok(credential_id)) =
            (claims.sub.parse::<Uuid>(), claims.cid.parse::<Uuid>())
        else {
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
            credential_id,
        })
    }
}
