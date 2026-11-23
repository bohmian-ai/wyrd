//! Public workspace signup records: one visitor's social proof, the creation
//! it may complete, and the shared admission windows creation is counted in.
//!
//! Nothing is stored before proof: the round-trip rides a sealed browser
//! cookie. A verified proof is recorded once per state or email identifier
//! digest as `proven`, held by the SHA-256 of a completion secret only the
//! proving browser holds. Every timestamp is PostgreSQL's.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use uuid::Uuid;

use crate::{OperatorPool, SqlError};

/// A verified signup proof as its completion reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProvenSignup {
    /// Attempt id.
    pub id: Uuid,
    /// `email` or `social`.
    pub proof_kind: String,
    /// Provider kind of a social proof.
    pub provider: Option<String>,
    /// The provider revision a social proof was made through.
    pub provider_revision: Option<i64>,
    /// A social proof's issuer namespace.
    pub issuer: Option<String>,
    /// A social proof's subject.
    pub subject: Option<String>,
    /// The verified address of an email proof, or a provider's email.
    pub email: Option<String>,
    /// `proven`, `creating`, `failed` or `completed`.
    pub status: String,
    /// The tenant this attempt claimed, when it claimed one.
    pub tenant_id: Option<Uuid>,
    /// That tenant's slug.
    pub tenant_slug: Option<String>,
}

/// Which admission window an attempt counts in.
#[derive(Debug, Clone, Copy)]
pub enum SignupAdmissionScope {
    /// The trusted source of a signup start.
    Start,
    /// The address an email signup start mails.
    Address,
    /// The trusted source of a workspace creation.
    Source,
    /// The proven identity.
    Identity,
    /// The trusted source of an unauthenticated sign-in start.
    SignIn,
    /// The trusted source of an emailed discovery, contact or reset proof.
    EmailSource,
    /// The address such a proof is mailed to.
    EmailAddress,
    /// The signed-in account asking for such a proof.
    EmailAccount,
    /// The trusted source of a password-reset request.
    Reset,
    /// The tenant account a password-reset request names.
    ResetAccount,
}

impl SignupAdmissionScope {
    /// The stored scope name.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Address => "address",
            Self::Source => "source",
            Self::Identity => "identity",
            Self::SignIn => "sign_in",
            Self::EmailSource => "email_source",
            Self::EmailAddress => "email_address",
            Self::EmailAccount => "email_account",
            Self::Reset => "reset",
            Self::ResetAccount => "reset_account",
        }
    }
}

/// A verified proof to record.
#[derive(Debug, Clone)]
pub struct SignupProof<'a> {
    /// Attempt id.
    pub id: Uuid,
    /// `email` or `social`.
    pub kind: &'a str,
    /// SHA-256 of the provider state or email identifier, unique per proof.
    pub proof_hash: &'a [u8],
    /// A social proof's provider and exact revision.
    pub provider: Option<(&'a str, i64)>,
    /// SHA-256 of the proving browser's binding.
    pub browser_binding_hash: &'a [u8],
    /// SHA-256 of the completion secret.
    pub completion_hash: &'a [u8],
    /// A social proof's issuer and subject.
    pub identity: Option<(&'a str, &'a str)>,
    /// The verified address, or a provider's email.
    pub email: Option<&'a str>,
}

/// Record `proof`, completable for `ttl`; `false` when a proof with its
/// digest already exists, live or spent, so a replay records nothing.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails.
pub async fn insert_proven_signup(
    pool: &OperatorPool,
    proof: &SignupProof<'_>,
    ttl: Duration,
) -> Result<bool, SqlError> {
    sqlx::query(
        "INSERT INTO platform.signup_attempts
             (id, proof_kind, proof_hash, provider, provider_revision, browser_binding_hash,
              completion_hash, issuer, subject, email, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                 statement_timestamp() + ($11 * interval '1 second'))
         ON CONFLICT (proof_hash) DO NOTHING",
    )
    .bind(proof.id)
    .bind(proof.kind)
    .bind(proof.proof_hash)
    .bind(proof.provider.map(|(provider, _)| provider))
    .bind(proof.provider.map(|(_, revision)| revision))
    .bind(proof.browser_binding_hash)
    .bind(proof.completion_hash)
    .bind(proof.identity.map(|(issuer, _)| issuer))
    .bind(proof.identity.map(|(_, subject)| subject))
    .bind(proof.email)
    .bind(ttl.as_secs_f64())
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// Record `address`, proven by email, as the verified contact of the live,
/// still-`proven` provider attempt `attempt` whose provider verified none,
/// and hold it under a new `completion_hash`; the same address again, as
/// after a lost response, only rotates the completion. `false` otherwise.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn attach_signup_contact(
    pool: &OperatorPool,
    attempt: Uuid,
    address: &str,
    completion_hash: &[u8],
) -> Result<bool, SqlError> {
    sqlx::query(
        "UPDATE platform.signup_attempts
            SET email = $2, completion_hash = $3, updated_at = statement_timestamp()
          WHERE id = $1 AND proof_kind = 'social' AND status = 'proven'
            AND (email IS NULL OR email = $2)
            AND expires_at > statement_timestamp()",
    )
    .bind(attempt)
    .bind(address)
    .bind(completion_hash)
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// Hold the live, still-`proven` proof with `proof_hash` under a new
/// `completion_hash`, when the same browser (`browser_binding_hash`) proved
/// it: the recovery of a lost verification response. `false` otherwise.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn renew_signup_completion(
    pool: &OperatorPool,
    proof_hash: &[u8],
    browser_binding_hash: &[u8],
    completion_hash: &[u8],
) -> Result<bool, SqlError> {
    sqlx::query(
        "UPDATE platform.signup_attempts
            SET completion_hash = $3, updated_at = statement_timestamp()
          WHERE proof_hash = $1 AND browser_binding_hash = $2 AND status = 'proven'
            AND expires_at > statement_timestamp()",
    )
    .bind(proof_hash)
    .bind(browser_binding_hash)
    .bind(completion_hash)
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// The database's current statement time, the clock pre-proof envelopes are
/// stamped and checked against.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn signup_clock(pool: &OperatorPool) -> Result<chrono::DateTime<chrono::Utc>, SqlError> {
    sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(pool.pool())
        .await
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
        "SELECT a.id, a.proof_kind, a.provider, a.provider_revision, a.issuer, a.subject,
                a.email, a.status, t.data_tenant_id AS tenant_id, t.slug AS tenant_slug
           FROM platform.signup_attempts a
           LEFT JOIN platform.tenants t ON t.signup_attempt_id = a.id
          WHERE a.completion_hash = $1 AND a.expires_at > statement_timestamp()",
    )
    .bind(completion_hash)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Move `attempt` from `proven` or `failed` to `creating`, or take over its
/// own `creating` run once that run has made no progress for the two-minute
/// signup staleness bound, inside the proof's completion window; `false`
/// when another
/// completion holds it, it already completed, or its proof expired.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn start_signup_creation(pool: &OperatorPool, attempt: Uuid) -> Result<bool, SqlError> {
    sqlx::query(
        "UPDATE platform.signup_attempts
            SET status = 'creating', updated_at = statement_timestamp()
          WHERE id = $1
            AND (status IN ('proven', 'failed')
                 OR (status = 'creating'
                     AND updated_at < statement_timestamp() - interval '2 minutes'))
            AND expires_at > statement_timestamp()",
    )
    .bind(attempt)
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// End `attempt`'s creation as `completed`, clearing the address it was
/// proven with, or, so its owner may retry, `failed`.
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
                email = CASE WHEN $2 THEN NULL ELSE email END,
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

/// Count one creation attempt for the pseudonym `key` in `scope`'s window of length
/// `window`, and return how long until the window admits another when this
/// one exceeds `limit`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the statement fails.
pub async fn admit_signup(
    pool: &OperatorPool,
    scope: SignupAdmissionScope,
    key: &[u8; 32],
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
    .bind(key.as_slice())
    .bind(window.as_secs_f64())
    .fetch_one(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok((attempts > limit).then(|| Duration::from_secs_f64(wait.max(1.0).ceil())))
}

/// Delete at most `batch` admission windows that ended more than `window`
/// ago and at most `batch` signup attempts whose completion expired more
/// than `retain` ago, plus at most `batch` spent discovery proofs past their
/// expiry, and return how many rows went.
///
/// Spent email and state digests stay for `retain` past their completion
/// window, so a proof replayed while its envelope lives is still recognized;
/// a completed attempt has already lost its email. A tenant an attempt
/// claimed keeps its creator key when the attempt goes. Each statement is
/// bounded, so a backlog drains over repeated calls rather than in one long
/// delete.
///
/// # Errors
/// Returns [`SqlError::Query`] when a statement fails.
pub async fn purge_expired_signup_state(
    pool: &OperatorPool,
    window: Duration,
    retain: Duration,
    batch: i64,
) -> Result<u64, SqlError> {
    let windows = sqlx::query(
        "DELETE FROM platform.signup_admissions
          WHERE ctid = ANY(ARRAY(
                SELECT ctid FROM platform.signup_admissions
                 WHERE window_start <= statement_timestamp() - ($1 * interval '1 second')
                 LIMIT $2))",
    )
    .bind(window.as_secs_f64())
    .bind(batch)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    let attempts = sqlx::query(
        "DELETE FROM platform.signup_attempts
          WHERE ctid = ANY(ARRAY(
                SELECT ctid FROM platform.signup_attempts
                 WHERE expires_at <= statement_timestamp() - ($2 * interval '1 second')
                 LIMIT $1))",
    )
    .bind(batch)
    .bind(retain.as_secs_f64())
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    let discoveries = sqlx::query(
        "DELETE FROM platform.discovery_proofs
          WHERE ctid = ANY(ARRAY(
                SELECT ctid FROM platform.discovery_proofs
                 WHERE expires_at <= statement_timestamp()
                 LIMIT $1))",
    )
    .bind(batch)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(windows.rows_affected() + attempts.rows_affected() + discoveries.rows_affected())
}
