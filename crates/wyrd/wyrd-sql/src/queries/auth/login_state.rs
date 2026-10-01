//! Tenant-scoped login state queries for the OIDC authorization-code flow.
//!
//! A row is keyed by the SHA-256 of its random state value and moves through
//! three steps, each one statement on the caller's RLS [`TenantConn`]:
//! [`insert_login_state`] when a login begins, [`consume_login_state`] when the
//! callback arrives (before any provider IO), and [`complete_login_state`] when
//! the callback has issued a session. [`redeem_login_completion`] then deletes
//! the completed row once, by the initiation binding the login recorded.
//! Forced RLS is the only tenant selection; `data_tenant_id` is written as the
//! row's owner and never repeated as a predicate. `PostgreSQL` owns every
//! expiry: callers bind lifetimes, never instants.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_spec::auth::{LoginInitiation, Sha256Hex};

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

/// Drop this tenant's rows that can no longer be consumed or redeemed.
const PURGE_EXPIRED_LOGIN_STATE_SQL: &str = r#"
    DELETE FROM wyrd.auth_login_state
     WHERE expires_at <= statement_timestamp()
"#;

/// Begin a login: record one unconsumed row owned by the RLS tenant.
///
/// Forced RLS's `WITH CHECK` refuses a `data_tenant_id` other than the
/// connection's tenant. `ON CONFLICT DO NOTHING` never overwrites: a reused
/// state hash, browser flow hash, or CLI handoff id inserts nothing, so one
/// binding names at most one login. `PostgreSQL` derives `expires_at` from the
/// bound lifetime in seconds.
const INSERT_LOGIN_STATE_SQL: &str = r#"
    INSERT INTO wyrd.auth_login_state (
        state_hash, data_tenant_id, connection_id, connection_revision, issuer,
        client_id, redirect_uri, code_verifier, nonce, browser_flow_hash,
        cli_handoff_id, expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
              statement_timestamp() + ($12 * interval '1 second'))
    ON CONFLICT DO NOTHING
"#;

/// Consume a login's state exactly once at the callback, before provider IO.
///
/// Only an unconsumed, unexpired row visible under forced RLS matches, and
/// setting `consumed_at` makes every replay of the same state match nothing.
/// Returns the pinned connection, PKCE verifier, nonce, and initiation binding
/// the callback needs to finish the exchange.
const CONSUME_LOGIN_STATE_SQL: &str = r#"
    UPDATE wyrd.auth_login_state
       SET consumed_at = statement_timestamp()
     WHERE state_hash = $1
       AND consumed_at IS NULL
       AND expires_at > statement_timestamp()
    RETURNING connection_id, connection_revision, issuer, client_id, redirect_uri,
              code_verifier, nonce, browser_flow_hash, cli_handoff_id
"#;

/// Attach the sealed session to a consumed login, once.
///
/// Matches only a consumed row of the RLS tenant with no completion yet, so a
/// completion is written at most once and never before consumption. Resets
/// `expires_at` to a fresh, short `PostgreSQL`-derived redemption window.
const COMPLETE_LOGIN_STATE_SQL: &str = r#"
    UPDATE wyrd.auth_login_state
       SET completion_sealed = $2,
           expires_at = statement_timestamp() + ($3 * interval '1 second')
     WHERE state_hash = $1
       AND consumed_at IS NOT NULL
       AND completion_sealed IS NULL
"#;

/// Redeem a completed login once by its initiation binding.
///
/// Deletes the RLS tenant's completed, unexpired row bound to either the
/// browser flow hash or the CLI handoff id and returns its sealed session.
/// Deletion is the one-use guarantee: a second redemption matches nothing.
const REDEEM_LOGIN_COMPLETION_SQL: &str = r#"
    DELETE FROM wyrd.auth_login_state
     WHERE (browser_flow_hash = $1 OR cli_handoff_id = $2)
       AND completion_sealed IS NOT NULL
       AND expires_at > statement_timestamp()
    RETURNING completion_sealed
"#;

/// The `(browser_flow_hash, cli_handoff_id)` column pair of an initiation;
/// exactly one is set, which is how the stored row records its kind.
fn initiation_columns(initiation: &LoginInitiation) -> (Option<&[u8]>, Option<Uuid>) {
    match initiation {
        LoginInitiation::Browser(hash) => (Some(hash.as_bytes().as_slice()), None),
        LoginInitiation::Cli(handoff_id) => (None, Some(*handoff_id)),
    }
}

/// Rebuild the initiation from its stored columns.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] when the columns do not describe exactly
/// one binding or the flow hash is not 32 bytes.
fn initiation_from_columns(
    flow_hash: Option<Vec<u8>>,
    handoff_id: Option<Uuid>,
) -> Result<LoginInitiation, sqlx::Error> {
    let corrupt = || sqlx::Error::Decode("login state binding is corrupt".into());
    match (flow_hash, handoff_id) {
        (Some(hash), None) => {
            let bytes: [u8; 32] = hash.try_into().map_err(|_| corrupt())?;
            Ok(LoginInitiation::Browser(Sha256Hex::from(bytes)))
        }
        (None, Some(handoff_id)) => Ok(LoginInitiation::Cli(handoff_id)),
        _ => Err(corrupt()),
    }
}

/// One login-state row: written when a login begins and returned when the
/// callback consumes it.
///
/// `Debug` is safe to print: the PKCE verifier is a [`SecretString`] and is
/// exposed only when bound into SQL or sent in the provider token request.
#[derive(Debug, Clone)]
pub struct LoginState {
    /// The exact connection revision the login began through.
    pub connection: HumanConnectionBinding,
    /// Exact issuer of that connection revision.
    pub issuer: String,
    /// Exact OAuth client id of that connection revision.
    pub client_id: String,
    /// The deployment-controlled callback the provider redirects to.
    pub redirect_uri: String,
    /// Server-generated PKCE verifier.
    pub code_verifier: SecretString,
    /// Server-generated nonce the ID token must echo.
    pub nonce: String,
    /// How the login was initiated and what redeems its completion.
    pub initiation: LoginInitiation,
}

/// Raw consumed row as returned by [`CONSUME_LOGIN_STATE_SQL`].
#[derive(sqlx::FromRow)]
struct ConsumedRow {
    /// Bound connection id and revision.
    #[sqlx(flatten)]
    connection: HumanConnectionBinding,
    /// Exact issuer.
    issuer: String,
    /// Exact client id.
    client_id: String,
    /// Callback URI.
    redirect_uri: String,
    /// PKCE verifier; wrapped in a [`SecretString`] as soon as it is decoded.
    code_verifier: String,
    /// Nonce.
    nonce: String,
    /// Browser flow hash, for a browser login.
    browser_flow_hash: Option<Vec<u8>>,
    /// CLI handoff id, for a CLI login.
    cli_handoff_id: Option<Uuid>,
}

/// Insert a login-state row keyed by `state_hash` whose expiry `PostgreSQL`
/// derives from `ttl`.
///
/// First purges this tenant's rows that are past their expiry, so abandoned
/// logins and unredeemed completions do not accumulate; RLS confines the purge
/// to the connection's tenant. The insert refuses to overwrite: when the state
/// hash or the initiation binding is already recorded the row is not written
/// and `false` is returned, so a binding is never shared by two logins.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or the insert.
pub async fn insert_login_state(
    conn: &mut TenantConn<'_>,
    state_hash: &Sha256Hex,
    row: &LoginState,
    ttl: Duration,
) -> Result<bool, sqlx::Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    sqlx::query(PURGE_EXPIRED_LOGIN_STATE_SQL)
        .execute(&mut **conn.transaction())
        .await?;
    let (flow_hash, handoff_id) = initiation_columns(&row.initiation);
    let inserted = sqlx::query(INSERT_LOGIN_STATE_SQL)
        .bind(state_hash.as_bytes().as_slice())
        .bind(tenant)
        .bind(row.connection.connection_id)
        .bind(row.connection.connection_revision)
        .bind(&row.issuer)
        .bind(&row.client_id)
        .bind(&row.redirect_uri)
        .bind(row.code_verifier.expose_secret())
        .bind(&row.nonce)
        .bind(flow_hash)
        .bind(handoff_id)
        .bind(ttl.as_secs_f64())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(inserted.rows_affected() == 1)
}

/// Consume an unconsumed, unexpired login-state row exactly once.
///
/// Marks the row consumed rather than deleting it, so the callback can later
/// attach the sealed completion to the same row. A missing, expired, already
/// consumed, or other tenant's row returns `None`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update or the stored binding
/// is corrupt.
pub async fn consume_login_state(
    conn: &mut TenantConn<'_>,
    state_hash: &Sha256Hex,
) -> Result<Option<LoginState>, sqlx::Error> {
    let row = sqlx::query_as::<_, ConsumedRow>(CONSUME_LOGIN_STATE_SQL)
        .bind(state_hash.as_bytes().as_slice())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(|row| {
        Ok(LoginState {
            initiation: initiation_from_columns(row.browser_flow_hash, row.cli_handoff_id)?,
            connection: row.connection,
            issuer: row.issuer,
            client_id: row.client_id,
            redirect_uri: row.redirect_uri,
            code_verifier: SecretString::from(row.code_verifier),
            nonce: row.nonce,
        })
    })
    .transpose()
}

/// Attach a sealed completion to a consumed row and restart its expiry at
/// `ttl`, the redemption window.
///
/// Returns `false` when this tenant has no consumed, uncompleted row with
/// this hash.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn complete_login_state(
    conn: &mut TenantConn<'_>,
    state_hash: &Sha256Hex,
    sealed: &[u8],
    ttl: Duration,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(COMPLETE_LOGIN_STATE_SQL)
        .bind(state_hash.as_bytes().as_slice())
        .bind(sealed)
        .bind(ttl.as_secs_f64())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Delete this tenant's completed, unexpired row recorded for `initiation`
/// and return its sealed completion; `None` when there is none.
///
/// The delete is the single use: a second redemption finds nothing.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn redeem_login_completion(
    conn: &mut TenantConn<'_>,
    initiation: &LoginInitiation,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let (flow_hash, handoff_id) = initiation_columns(initiation);
    sqlx::query_scalar::<_, Vec<u8>>(REDEEM_LOGIN_COMPLETION_SQL)
        .bind(flow_hash)
        .bind(handoff_id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use uuid::Uuid;
    use wyrd_spec::auth::{LoginInitiation, Sha256Hex};

    use super::{LoginState, initiation_columns, initiation_from_columns};
    use crate::row_types::auth::HumanConnectionBinding;

    /// An initiation round-trips through its stored columns, and a stored row
    /// naming both or neither binding, or a short hash, is refused as corrupt.
    #[test]
    fn initiation_columns_round_trip_and_refuse_ambiguity() {
        let browser = LoginInitiation::Browser(Sha256Hex::digest(b"flow"));
        let (hash, handoff) = initiation_columns(&browser);
        let rebuilt = initiation_from_columns(hash.map(<[u8]>::to_vec), handoff)
            .expect("browser binding rebuilds");
        assert_eq!(rebuilt, browser);

        let cli = LoginInitiation::Cli(Uuid::now_v7());
        let (hash, handoff) = initiation_columns(&cli);
        let rebuilt = initiation_from_columns(hash.map(<[u8]>::to_vec), handoff)
            .expect("cli binding rebuilds");
        assert_eq!(rebuilt, cli);

        assert!(initiation_from_columns(Some(vec![1; 32]), Some(Uuid::now_v7())).is_err());
        assert!(initiation_from_columns(None, None).is_err());
        assert!(initiation_from_columns(Some(vec![1; 3]), None).is_err());
    }

    /// Formatting a login state never prints its PKCE verifier.
    #[test]
    fn login_state_debug_redacts_the_pkce_verifier() {
        let sentinel = "pkce-verifier-sentinel-7f3a";
        let state = LoginState {
            connection: HumanConnectionBinding {
                connection_id: Uuid::now_v7(),
                connection_revision: 1,
            },
            issuer: "https://idp.example.com".to_owned(),
            client_id: "wyrd".to_owned(),
            redirect_uri: "https://wyrd.example.com/auth/callback".to_owned(),
            code_verifier: SecretString::from(sentinel),
            nonce: "nonce".to_owned(),
            initiation: LoginInitiation::Browser(Sha256Hex::digest(b"flow")),
        };

        let printed = format!("{state:?} {state:#?}");

        assert!(!printed.contains(sentinel), "{printed}");
    }
}
