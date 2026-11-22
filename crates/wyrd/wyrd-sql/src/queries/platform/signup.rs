//! Public workspace signup records: one visitor's social proof, the creation
//! it may complete, and the shared admission windows creation is counted in.
//!
//! The provider round-trip row is deleted when its callback consumes it; a
//! verified proof is then written back under the same id as `proven`, held by
//! the SHA-256 of a completion secret only the proving browser holds. Every
//! timestamp is PostgreSQL's.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use uuid::Uuid;

use crate::{OperatorPool, SqlError};

/// A signup's provider round-trip as begun.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingSignup {
    /// Attempt id, kept when the proof is recorded.
    pub id: Uuid,
    /// Provider kind.
    pub provider: String,
    /// The exact provider revision the redirect was issued for.
    pub provider_revision: i64,
    /// The provider's social callback.
    pub redirect_uri: String,
    /// Server-held PKCE verifier.
    pub code_verifier: String,
    /// Server-held nonce, for an OpenID provider.
    pub nonce: Option<String>,
    /// SHA-256 of the beginning browser's login-transaction secret.
    pub browser_binding_hash: Vec<u8>,
}

/// A verified signup proof as its completion reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProvenSignup {
    /// Attempt id.
    pub id: Uuid,
    /// Provider kind.
    pub provider: String,
    /// The provider revision the proof was made through.
    pub provider_revision: i64,
    /// The proven identity's issuer namespace.
    pub issuer: String,
    /// The proven identity's subject.
    pub subject: String,
    /// The provider's email, when it gave one.
    pub email: Option<String>,
    /// `proven`, `creating`, `failed` or `completed`.
    pub status: String,
    /// The tenant this attempt claimed, when it claimed one.
    pub tenant_id: Option<Uuid>,
    /// That tenant's slug.
    pub tenant_slug: Option<String>,
}

/// Which admission window a creation attempt counts in.
#[derive(Debug, Clone, Copy)]
pub enum SignupAdmissionScope {
    /// The trusted source of a signup start.
    Start,
    /// The trusted source of a workspace creation.
    Source,
    /// The proven provider identity.
    Identity,
}

impl SignupAdmissionScope {
    /// The stored scope name.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Source => "source",
            Self::Identity => "identity",
        }
    }
}

/// Record a begun signup round-trip keyed by `state_hash`, expiring after
/// `ttl`, and drop expired attempts no tenant names.
///
/// # Errors
/// Returns [`SqlError::Query`] when a statement fails.
pub async fn insert_signup_attempt(
    pool: &OperatorPool,
    state_hash: &[u8],
    attempt: &PendingSignup,
    ttl: Duration,
) -> Result<(), SqlError> {
    sqlx::query("DELETE FROM platform.signup_attempts WHERE expires_at <= statement_timestamp()")
        .execute(pool.pool())
        .await
        .map_err(SqlError::from)?;
    sqlx::query(
        "INSERT INTO platform.signup_attempts
             (id, state_hash, provider, provider_revision, redirect_uri, code_verifier,
              nonce, browser_binding_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                 statement_timestamp() + ($9 * interval '1 second'))",
    )
    .bind(attempt.id)
    .bind(state_hash)
    .bind(&attempt.provider)
    .bind(attempt.provider_revision)
    .bind(&attempt.redirect_uri)
    .bind(&attempt.code_verifier)
    .bind(&attempt.nonce)
    .bind(&attempt.browser_binding_hash)
    .bind(ttl.as_secs_f64())
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Consume the live pending round-trip of `provider` keyed by `state_hash`,
/// whatever happens next; `None` when there is none.
///
/// # Errors
/// Returns [`SqlError::Query`] when the statement fails.
pub async fn take_signup_attempt(
    pool: &OperatorPool,
    provider: &str,
    state_hash: &[u8],
) -> Result<Option<PendingSignup>, SqlError> {
    sqlx::query_as(
        "DELETE FROM platform.signup_attempts
          WHERE state_hash = $1 AND provider = $2 AND status = 'pending'
            AND expires_at > statement_timestamp()
          RETURNING id, provider, provider_revision, redirect_uri, code_verifier,
                    nonce, browser_binding_hash",
    )
    .bind(state_hash)
    .bind(provider)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// The proven identity a consumed round-trip established.
#[derive(Debug, Clone)]
pub struct SignupProof<'a> {
    /// Issuer namespace.
    pub issuer: &'a str,
    /// Subject within it.
    pub subject: &'a str,
    /// Email, when the provider gave one.
    pub email: Option<&'a str>,
}

/// Record `attempt`'s verified `proof`, held by `completion_hash` and
/// completable for `ttl`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails.
pub async fn insert_proven_signup(
    pool: &OperatorPool,
    attempt: &PendingSignup,
    state_hash: &[u8],
    completion_hash: &[u8],
    proof: &SignupProof<'_>,
    ttl: Duration,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.signup_attempts
             (id, state_hash, provider, provider_revision, redirect_uri, nonce,
              browser_binding_hash, completion_hash, issuer, subject, email, status,
              expires_at)
         VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $8, $9, $10, 'proven',
                 statement_timestamp() + ($11 * interval '1 second'))",
    )
    .bind(attempt.id)
    .bind(state_hash)
    .bind(&attempt.provider)
    .bind(attempt.provider_revision)
    .bind(&attempt.redirect_uri)
    .bind(&attempt.browser_binding_hash)
    .bind(completion_hash)
    .bind(proof.issuer)
    .bind(proof.subject)
    .bind(proof.email)
    .bind(ttl.as_secs_f64())
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// The live proven attempt held by `completion_hash`, with the tenant it
/// claimed.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn proven_signup(
    pool: &OperatorPool,
    completion_hash: &[u8],
) -> Result<Option<ProvenSignup>, SqlError> {
    sqlx::query_as(
        "SELECT a.id, a.provider, a.provider_revision, a.issuer, a.subject, a.email,
                a.status, t.data_tenant_id AS tenant_id, t.slug AS tenant_slug
           FROM platform.signup_attempts a
           LEFT JOIN platform.tenants t ON t.signup_attempt_id = a.id
          WHERE a.completion_hash = $1 AND a.expires_at > statement_timestamp()",
    )
    .bind(completion_hash)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Move `attempt` from `proven` or `failed` to `creating`; `false` when
/// another completion holds it or it already completed.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn start_signup_creation(pool: &OperatorPool, attempt: Uuid) -> Result<bool, SqlError> {
    sqlx::query(
        "UPDATE platform.signup_attempts
            SET status = 'creating', updated_at = statement_timestamp()
          WHERE id = $1 AND status IN ('proven', 'failed')
            AND expires_at > statement_timestamp()",
    )
    .bind(attempt)
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// End `attempt`'s creation as `completed` or, so its owner may retry,
/// `failed`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn finish_signup_creation(
    pool: &OperatorPool,
    attempt: Uuid,
    completed: bool,
) -> Result<(), SqlError> {
    sqlx::query(
        "UPDATE platform.signup_attempts
            SET status = CASE WHEN $2 THEN 'completed' ELSE 'failed' END,
                updated_at = statement_timestamp()
          WHERE id = $1 AND status = 'creating'",
    )
    .bind(attempt)
    .bind(completed)
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Count one creation attempt for `key` in `scope`'s window of length
/// `window`, and return how long until the window admits another when this
/// one exceeds `limit`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the statement fails.
pub async fn admit_signup(
    pool: &OperatorPool,
    scope: SignupAdmissionScope,
    key: &str,
    limit: i32,
    window: Duration,
) -> Result<Option<Duration>, SqlError> {
    let (attempts, wait): (i32, f64) = sqlx::query_as(
        "INSERT INTO platform.signup_admissions AS w (scope, key, window_start, attempts)
         VALUES ($1, $2, statement_timestamp(), 1)
         ON CONFLICT (scope, key) DO UPDATE
            SET window_start = CASE
                    WHEN w.window_start <= statement_timestamp() - ($3 * interval '1 second')
                    THEN statement_timestamp() ELSE w.window_start END,
                attempts = CASE
                    WHEN w.window_start <= statement_timestamp() - ($3 * interval '1 second')
                    THEN 1 ELSE w.attempts + 1 END
         RETURNING attempts,
                   EXTRACT(EPOCH FROM window_start + ($3 * interval '1 second')
                                      - statement_timestamp())::float8",
    )
    .bind(scope.as_str())
    .bind(key)
    .bind(window.as_secs_f64())
    .fetch_one(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok((attempts > limit).then(|| Duration::from_secs_f64(wait.max(1.0).ceil())))
}
