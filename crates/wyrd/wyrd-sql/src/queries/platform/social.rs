//! Query slots for shared social sign-in providers and deployment login
//! settings.
//!
//! `platform.social_providers` holds one row per closed provider kind.
//! Replacing a client advances its revision and clears its test and enablement
//! in the same statement, so an earlier revision can never be mistaken for the
//! current one. A provider test is a one-use row in
//! `platform.social_login_state`, consumed before any provider IO. The single
//! `platform.login_settings` row carries the deployment's password and signup
//! switches and, once an operator configures one, its SMTP relay.
//!
//! Every statement runs on the [`OperatorPool`] or an audited operator
//! [`TenantConn`], because `platform.*` admits only operator sessions.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError, TenantConn};

/// One stored shared provider.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SocialProviderRow {
    /// Provider kind label (`google` or `github`).
    pub provider: String,
    /// Current configuration revision.
    pub revision: i64,
    /// OAuth client id.
    pub client_id: String,
    /// Sealed client secret envelope.
    pub client_secret_enc: Vec<u8>,
    /// Whether the current revision passed a provider sign-in.
    pub tested: bool,
    /// Whether the provider is offered for sign-in.
    pub enabled: bool,
}

/// Read every configured provider.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn social_providers(pool: &OperatorPool) -> Result<Vec<SocialProviderRow>, SqlError> {
    sqlx::query_as::<_, SocialProviderRow>(
        "SELECT provider, revision, client_id, client_secret_enc, tested_at IS NOT NULL AS tested, enabled FROM platform.social_providers ORDER BY provider",
    )
    .fetch_all(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Read one configured provider.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn social_provider(
    pool: &OperatorPool,
    provider: &str,
) -> Result<Option<SocialProviderRow>, SqlError> {
    sqlx::query_as::<_, SocialProviderRow>(
        "SELECT provider, revision, client_id, client_secret_enc, tested_at IS NOT NULL AS tested, enabled FROM platform.social_providers WHERE provider = $1",
    )
    .bind(provider)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Store `provider`'s client as a new, untested, disabled revision and return
/// that row.
///
/// An upsert on the provider key: two operators replacing the client at once
/// converge on one row, each advancing the revision.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails, including an empty
/// client id.
pub async fn put_social_provider(
    conn: &mut TenantConn<'_>,
    provider: &str,
    client_id: &str,
    client_secret_enc: &[u8],
) -> Result<SocialProviderRow, SqlError> {
    sqlx::query_as::<_, SocialProviderRow>(
        "INSERT INTO platform.social_providers (provider, revision, client_id, client_secret_enc)
         VALUES ($1, 1, $2, $3)
         ON CONFLICT (provider) DO UPDATE SET
             revision          = platform.social_providers.revision + 1,
             client_id         = EXCLUDED.client_id,
             client_secret_enc = EXCLUDED.client_secret_enc,
             tested_at         = NULL,
             enabled           = FALSE,
             updated_at        = now()
         RETURNING provider, revision, client_id, client_secret_enc, tested_at IS NOT NULL AS tested, enabled",
    )
    .bind(provider)
    .bind(client_id)
    .bind(client_secret_enc)
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Mark exactly `revision` of `provider` tested.
///
/// Returns `false` when the provider moved to another revision meanwhile, so a
/// test of a replaced client never certifies its replacement.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn stamp_social_provider_tested(
    pool: &OperatorPool,
    provider: &str,
    revision: i64,
) -> Result<bool, SqlError> {
    sqlx::query(
        "UPDATE platform.social_providers
            SET tested_at = statement_timestamp(), updated_at = now()
          WHERE provider = $1 AND revision = $2",
    )
    .bind(provider)
    .bind(revision)
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// Outcome of [`set_social_provider_enabled`].
#[derive(Debug, Clone)]
pub enum ProviderEnablement {
    /// The provider now holds the requested state.
    Changed(SocialProviderRow),
    /// The provider is not configured.
    Unconfigured,
    /// The provider's current revision is not the one the caller named.
    StaleRevision,
    /// Enabling was asked of a revision no sign-in has proven.
    Untested,
}

/// Enable or disable exactly `revision` of `provider` on `conn`.
///
/// The row is locked, so a concurrent replacement either commits first and
/// this sees a stale revision, or waits for this decision.
///
/// # Errors
/// Returns [`SqlError::Query`] when a statement fails.
pub async fn set_social_provider_enabled(
    conn: &mut TenantConn<'_>,
    provider: &str,
    revision: i64,
    enabled: bool,
) -> Result<ProviderEnablement, SqlError> {
    let current = sqlx::query_as::<_, SocialProviderRow>(
        "SELECT provider, revision, client_id, client_secret_enc, tested_at IS NOT NULL AS tested, enabled FROM platform.social_providers
          WHERE provider = $1 FOR UPDATE",
    )
    .bind(provider)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    let Some(current) = current else {
        return Ok(ProviderEnablement::Unconfigured);
    };
    if current.revision != revision {
        return Ok(ProviderEnablement::StaleRevision);
    }
    if enabled && !current.tested {
        return Ok(ProviderEnablement::Untested);
    }
    sqlx::query_as::<_, SocialProviderRow>(
        "UPDATE platform.social_providers SET enabled = $2, updated_at = now()
          WHERE provider = $1 RETURNING provider, revision, client_id, client_secret_enc, tested_at IS NOT NULL AS tested, enabled",
    )
    .bind(provider)
    .bind(enabled)
    .fetch_one(&mut **conn.transaction())
    .await
    .map(ProviderEnablement::Changed)
    .map_err(SqlError::from)
}

/// A provider-test transaction as stored and as consumed.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SocialTestState {
    /// Provider kind label.
    pub provider: String,
    /// The exact revision under test.
    pub provider_revision: i64,
    /// Callback the provider redirects to.
    pub redirect_uri: String,
    /// Server-held PKCE verifier.
    pub code_verifier: String,
    /// OIDC nonce, for an OIDC provider.
    pub nonce: Option<String>,
    /// Platform principal that began the test.
    pub tester_principal_id: Uuid,
}

/// Record a provider-test transaction keyed by `state_hash`, expiring after
/// `ttl` by `PostgreSQL`'s clock; expired rows are purged first.
///
/// # Errors
/// Returns [`SqlError::Query`] when a statement fails, including a repeated
/// state hash.
pub async fn insert_social_test_state(
    pool: &OperatorPool,
    state_hash: &[u8],
    state: &SocialTestState,
    ttl: Duration,
) -> Result<(), SqlError> {
    sqlx::query(
        "DELETE FROM platform.social_login_state WHERE expires_at <= statement_timestamp()",
    )
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    sqlx::query(
        "INSERT INTO platform.social_login_state
             (state_hash, provider, provider_revision, redirect_uri, code_verifier,
              nonce, tester_principal_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7,
                 statement_timestamp() + ($8 * interval '1 second'))",
    )
    .bind(state_hash)
    .bind(&state.provider)
    .bind(state.provider_revision)
    .bind(&state.redirect_uri)
    .bind(&state.code_verifier)
    .bind(&state.nonce)
    .bind(state.tester_principal_id)
    .bind(ttl.as_secs_f64())
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Consume `provider`'s unexpired, unconsumed test transaction exactly once.
///
/// A replay, an expired row, another provider's state and a forged key all
/// return `None`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the statement fails.
pub async fn take_social_test_state(
    pool: &OperatorPool,
    provider: &str,
    state_hash: &[u8],
) -> Result<Option<SocialTestState>, SqlError> {
    sqlx::query_as::<_, SocialTestState>(
        "UPDATE platform.social_login_state SET consumed_at = statement_timestamp()
          WHERE state_hash = $1 AND provider = $2
            AND consumed_at IS NULL AND expires_at > statement_timestamp()
         RETURNING provider, provider_revision, redirect_uri, code_verifier,
                   nonce, tester_principal_id",
    )
    .bind(state_hash)
    .bind(provider)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// The deployment login settings row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LoginSettingsRow {
    /// Password sign-in is available.
    pub password_enabled: bool,
    /// Public workspace signup is available.
    pub signup_enabled: bool,
    /// The owned-workspace cap per proven identity, at least 1.
    pub signup_workspace_cap: i32,
    /// The Turnstile site key, when a signup challenge is configured.
    pub signup_challenge_site_key: Option<String>,
}

/// The configured signup challenge's site key and sealed Siteverify secret;
/// `None` when no challenge is configured.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn signup_challenge(pool: &OperatorPool) -> Result<Option<(String, Vec<u8>)>, SqlError> {
    sqlx::query_as::<_, (Option<String>, Option<Vec<u8>>)>(
        "SELECT signup_challenge_site_key, signup_challenge_secret FROM platform.login_settings",
    )
    .fetch_one(pool.pool())
    .await
    .map(|(key, secret)| key.zip(secret))
    .map_err(SqlError::from)
}

/// Read the deployment login settings.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails, including a missing row.
pub async fn login_settings(pool: &OperatorPool) -> Result<LoginSettingsRow, SqlError> {
    sqlx::query_as::<_, LoginSettingsRow>(
        "SELECT password_enabled, signup_enabled, signup_workspace_cap,
                signup_challenge_site_key
           FROM platform.login_settings",
    )
    .fetch_one(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Replace the deployment login settings on `conn`.
///
/// A signup challenge without `sealed_secret` keeps the stored secret; no
/// site key clears both.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails, including a check
/// violation for a site key that would have no secret.
pub async fn put_login_settings(
    conn: &mut TenantConn<'_>,
    settings: &LoginSettingsRow,
    sealed_secret: Option<&[u8]>,
) -> Result<(), SqlError> {
    sqlx::query(
        "UPDATE platform.login_settings
            SET password_enabled = $1, signup_enabled = $2,
                signup_workspace_cap = $3,
                signup_challenge_site_key = $4,
                signup_challenge_secret = CASE
                    WHEN $4 IS NULL THEN NULL
                    ELSE coalesce($5, signup_challenge_secret) END,
                updated_at = now()",
    )
    .bind(settings.password_enabled)
    .bind(settings.signup_enabled)
    .bind(settings.signup_workspace_cap)
    .bind(settings.signup_challenge_site_key.as_deref())
    .bind(sealed_secret)
    .execute(&mut **conn.transaction())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// The deployment's SMTP relay, as stored on the login settings row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MailRelayRow {
    /// Relay host name or address.
    #[sqlx(rename = "smtp_host")]
    pub host: String,
    /// Relay TCP port.
    #[sqlx(rename = "smtp_port")]
    pub port: i32,
    /// TLS mode label: `none`, `starttls` or `tls`.
    #[sqlx(rename = "smtp_tls")]
    pub tls: String,
    /// Relay username, when the relay authenticates.
    #[sqlx(rename = "smtp_username")]
    pub username: Option<String>,
    /// Sealing-keyring envelope of the relay password.
    #[sqlx(rename = "smtp_password_enc")]
    pub password_enc: Option<Vec<u8>>,
    /// Envelope and header sender address.
    #[sqlx(rename = "smtp_sender")]
    pub sender: String,
}

/// Read the configured SMTP relay; `None` when no operator configured one.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails, including a missing row.
pub async fn mail_relay(pool: &OperatorPool) -> Result<Option<MailRelayRow>, SqlError> {
    sqlx::query_as::<_, MailRelayRow>(
        "SELECT smtp_host, smtp_port, smtp_tls, smtp_username, smtp_password_enc, smtp_sender
           FROM platform.login_settings WHERE smtp_host IS NOT NULL",
    )
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Replace the SMTP relay on the audited operator `conn`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails, and a `409` constraint
/// error for an incomplete relay.
pub async fn put_mail_relay(
    conn: &mut TenantConn<'_>,
    relay: &MailRelayRow,
) -> Result<(), SqlError> {
    sqlx::query(
        "UPDATE platform.login_settings
            SET smtp_host = $1, smtp_port = $2, smtp_tls = $3, smtp_username = $4,
                smtp_password_enc = $5, smtp_sender = $6, updated_at = now()",
    )
    .bind(&relay.host)
    .bind(relay.port)
    .bind(&relay.tls)
    .bind(relay.username.as_deref())
    .bind(relay.password_enc.as_deref())
    .bind(&relay.sender)
    .execute(&mut **conn.transaction())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Read the platform derivation key's envelope, if one was ever stored.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn derivation_key(pool: &OperatorPool) -> Result<Option<Vec<u8>>, SqlError> {
    sqlx::query_scalar::<_, Vec<u8>>("SELECT envelope FROM platform.derivation_key")
        .fetch_optional(pool.pool())
        .await
        .map_err(SqlError::from)
}

/// Store `envelope` as the platform derivation key unless one is already
/// stored, then return the stored envelope.
///
/// Concurrent first starts each offer their own envelope; exactly one wins
/// and every caller gets the winner, so all replicas derive from one key.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write or read fails.
pub async fn offer_derivation_key(
    pool: &OperatorPool,
    envelope: &[u8],
) -> Result<Vec<u8>, SqlError> {
    sqlx::query(
        "INSERT INTO platform.derivation_key (envelope) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(envelope)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    sqlx::query_scalar::<_, Vec<u8>>("SELECT envelope FROM platform.derivation_key")
        .fetch_one(pool.pool())
        .await
        .map_err(SqlError::from)
}
