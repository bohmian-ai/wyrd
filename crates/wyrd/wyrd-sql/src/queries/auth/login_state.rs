//! Tenant-scoped login state queries for the OIDC authorization-code flow.
//!
//! A row is keyed by the SHA-256 of its random state value and moves through
//! three steps, each one statement on the caller's RLS [`TenantConn`]:
//! [`insert_login_state`] when a login begins, [`consume_login_state`] when the
//! callback arrives (before any provider IO), and [`complete_login_state`] when
//! the callback has issued a session. [`redeem_login_completion`] then deletes
//! the completed row once, by the initiation binding the login recorded.
//! `PostgreSQL` owns every expiry: callers bind lifetimes, never instants.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use uuid::Uuid;

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

/// Drop this tenant's rows that can no longer be consumed or redeemed.
const PURGE_EXPIRED_LOGIN_STATE_SQL: &str = r#"
    DELETE FROM wyrd.auth_login_state
     WHERE data_tenant_id = $1
       AND expires_at <= statement_timestamp()
"#;

const INSERT_LOGIN_STATE_SQL: &str = r#"
    INSERT INTO wyrd.auth_login_state (
        state_hash, data_tenant_id, connection_id, connection_revision, issuer,
        client_id, redirect_uri, code_verifier, nonce, initiation_kind,
        browser_flow_hash, cli_handoff_id, expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
              statement_timestamp() + ($13 * interval '1 second'))
    ON CONFLICT DO NOTHING
"#;

const CONSUME_LOGIN_STATE_SQL: &str = r#"
    UPDATE wyrd.auth_login_state
       SET consumed_at = statement_timestamp()
     WHERE data_tenant_id = $1
       AND state_hash = $2
       AND consumed_at IS NULL
       AND expires_at > statement_timestamp()
    RETURNING connection_id, connection_revision, issuer, client_id, redirect_uri,
              code_verifier, nonce, initiation_kind, browser_flow_hash, cli_handoff_id
"#;

const COMPLETE_LOGIN_STATE_SQL: &str = r#"
    UPDATE wyrd.auth_login_state
       SET completion_sealed = $3,
           expires_at = statement_timestamp() + ($4 * interval '1 second')
     WHERE data_tenant_id = $1
       AND state_hash = $2
       AND consumed_at IS NOT NULL
       AND completion_sealed IS NULL
"#;

const REDEEM_LOGIN_COMPLETION_SQL: &str = r#"
    DELETE FROM wyrd.auth_login_state
     WHERE data_tenant_id = $1
       AND initiation_kind = $2
       AND (browser_flow_hash = $3 OR cli_handoff_id = $4)
       AND completion_sealed IS NOT NULL
       AND expires_at > statement_timestamp()
    RETURNING completion_sealed
"#;

/// How a login was initiated, and the one binding its completion is redeemed
/// by.
///
/// Stored as `initiation_kind` plus exactly one of `browser_flow_hash` or
/// `cli_handoff_id`; the table's checks refuse any other combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStateBinding {
    /// A browser login bound to the SHA-256 of the BFF's random flow id.
    Browser {
        /// SHA-256 of the flow id the BFF holds in its HttpOnly cookie.
        flow_hash: [u8; 32],
    },
    /// A CLI login bound to a server-issued handoff id.
    Cli {
        /// The handoff the CLI polls to redeem the completion.
        handoff_id: Uuid,
    },
}

impl LoginStateBinding {
    /// The stored `initiation_kind` discriminator.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Browser { .. } => "browser",
            Self::Cli { .. } => "cli",
        }
    }

    /// The `(browser_flow_hash, cli_handoff_id)` column pair; exactly one is set.
    fn columns(&self) -> (Option<&[u8]>, Option<Uuid>) {
        match self {
            Self::Browser { flow_hash } => (Some(flow_hash.as_slice()), None),
            Self::Cli { handoff_id } => (None, Some(*handoff_id)),
        }
    }

    /// Rebuild the binding from its stored columns.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Decode`] when the stored discriminator and
    /// columns do not describe exactly one binding.
    fn from_columns(
        kind: &str,
        flow_hash: Option<Vec<u8>>,
        handoff_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        let corrupt = || sqlx::Error::Decode("login state binding is corrupt".into());
        match (kind, flow_hash, handoff_id) {
            ("browser", Some(hash), None) => Ok(Self::Browser {
                flow_hash: hash.try_into().map_err(|_| corrupt())?,
            }),
            ("cli", None, Some(handoff_id)) => Ok(Self::Cli { handoff_id }),
            _ => Err(corrupt()),
        }
    }
}

/// One login-state row as written when a login begins.
#[derive(Debug, Clone)]
pub struct NewLoginState {
    /// SHA-256 of the random state value carried in the authorization URL.
    pub state_hash: [u8; 32],
    /// The exact connection revision the login began through.
    pub connection: HumanConnectionBinding,
    /// Exact issuer of that connection revision.
    pub issuer: String,
    /// Exact OAuth client id of that connection revision.
    pub client_id: String,
    /// The deployment-controlled callback the provider redirects to.
    pub redirect_uri: String,
    /// Server-generated PKCE verifier.
    pub code_verifier: String,
    /// Server-generated nonce the ID token must echo.
    pub nonce: String,
    /// How the login was initiated and what redeems its completion.
    pub binding: LoginStateBinding,
}

/// The consumed login state the callback continues from.
#[derive(Debug, Clone)]
pub struct ConsumedLoginState {
    /// The exact connection revision the login began through.
    pub connection: HumanConnectionBinding,
    /// Exact issuer of that connection revision.
    pub issuer: String,
    /// Exact OAuth client id of that connection revision.
    pub client_id: String,
    /// The callback URI the code was issued for.
    pub redirect_uri: String,
    /// Server-generated PKCE verifier.
    pub code_verifier: String,
    /// Server-generated nonce the ID token must echo.
    pub nonce: String,
    /// How the login was initiated.
    pub binding: LoginStateBinding,
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
    /// PKCE verifier.
    code_verifier: String,
    /// Nonce.
    nonce: String,
    /// `browser` or `cli`.
    initiation_kind: String,
    /// Browser flow hash, for a browser login.
    browser_flow_hash: Option<Vec<u8>>,
    /// CLI handoff id, for a CLI login.
    cli_handoff_id: Option<Uuid>,
}

/// Insert a login-state row whose expiry `PostgreSQL` derives from `ttl`.
///
/// First purges this tenant's rows that are past their expiry, so abandoned
/// logins and unredeemed completions do not accumulate. The insert refuses to
/// overwrite: when the state hash or the initiation binding is already
/// recorded the row is not written and `false` is returned, so a binding is
/// never shared by two logins.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or the insert.
pub async fn insert_login_state(
    conn: &mut TenantConn<'_>,
    row: &NewLoginState,
    ttl: Duration,
) -> Result<bool, sqlx::Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    sqlx::query(PURGE_EXPIRED_LOGIN_STATE_SQL)
        .bind(tenant)
        .execute(&mut **conn.transaction())
        .await?;
    let (flow_hash, handoff_id) = row.binding.columns();
    let inserted = sqlx::query(INSERT_LOGIN_STATE_SQL)
        .bind(row.state_hash.as_slice())
        .bind(tenant)
        .bind(row.connection.connection_id)
        .bind(row.connection.connection_revision)
        .bind(&row.issuer)
        .bind(&row.client_id)
        .bind(&row.redirect_uri)
        .bind(&row.code_verifier)
        .bind(&row.nonce)
        .bind(row.binding.kind())
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
/// attach the sealed completion to the same row. A missing, expired, or
/// already consumed row returns `None`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update or the stored binding
/// is corrupt.
pub async fn consume_login_state(
    conn: &mut TenantConn<'_>,
    state_hash: &[u8; 32],
) -> Result<Option<ConsumedLoginState>, sqlx::Error> {
    let row = sqlx::query_as::<_, ConsumedRow>(CONSUME_LOGIN_STATE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(state_hash.as_slice())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(|row| {
        Ok(ConsumedLoginState {
            binding: LoginStateBinding::from_columns(
                &row.initiation_kind,
                row.browser_flow_hash,
                row.cli_handoff_id,
            )?,
            connection: row.connection,
            issuer: row.issuer,
            client_id: row.client_id,
            redirect_uri: row.redirect_uri,
            code_verifier: row.code_verifier,
            nonce: row.nonce,
        })
    })
    .transpose()
}

/// Attach a sealed completion to a consumed row and restart its expiry at
/// `ttl`, the redemption window.
///
/// Returns `false` when no consumed, uncompleted row has this hash.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn complete_login_state(
    conn: &mut TenantConn<'_>,
    state_hash: &[u8; 32],
    sealed: &[u8],
    ttl: Duration,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(COMPLETE_LOGIN_STATE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(state_hash.as_slice())
        .bind(sealed)
        .bind(ttl.as_secs_f64())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Delete the completed, unexpired row recorded for `binding` and return its
/// sealed completion; `None` when there is none.
///
/// The delete is the single use: a second redemption finds nothing.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn redeem_login_completion(
    conn: &mut TenantConn<'_>,
    binding: &LoginStateBinding,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let (flow_hash, handoff_id) = binding.columns();
    sqlx::query_scalar::<_, Vec<u8>>(REDEEM_LOGIN_COMPLETION_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(binding.kind())
        .bind(flow_hash)
        .bind(handoff_id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETE_LOGIN_STATE_SQL, CONSUME_LOGIN_STATE_SQL, INSERT_LOGIN_STATE_SQL,
        LoginStateBinding, REDEEM_LOGIN_COMPLETION_SQL,
    };
    use uuid::Uuid;

    /// Every statement stays tenant-bound and lets `PostgreSQL` own expiry.
    #[test]
    fn login_state_queries_are_tenant_scoped() {
        assert!(INSERT_LOGIN_STATE_SQL.contains("statement_timestamp()"));
        for sql in [
            CONSUME_LOGIN_STATE_SQL,
            COMPLETE_LOGIN_STATE_SQL,
            REDEEM_LOGIN_COMPLETION_SQL,
        ] {
            assert!(sql.contains("data_tenant_id = $1"), "{sql}");
        }
        assert!(CONSUME_LOGIN_STATE_SQL.contains("consumed_at IS NULL"));
        assert!(CONSUME_LOGIN_STATE_SQL.contains("expires_at > statement_timestamp()"));
    }

    /// A binding round-trips through its stored columns, and a stored row
    /// naming both or neither binding is refused as corrupt.
    #[test]
    fn binding_columns_round_trip_and_refuse_ambiguity() {
        let browser = LoginStateBinding::Browser { flow_hash: [7; 32] };
        let (hash, handoff) = browser.columns();
        let rebuilt = LoginStateBinding::from_columns("browser", hash.map(<[u8]>::to_vec), handoff)
            .expect("browser binding rebuilds");
        assert_eq!(rebuilt, browser);

        let cli = LoginStateBinding::Cli {
            handoff_id: Uuid::now_v7(),
        };
        let (hash, handoff) = cli.columns();
        let rebuilt = LoginStateBinding::from_columns("cli", hash.map(<[u8]>::to_vec), handoff)
            .expect("cli binding rebuilds");
        assert_eq!(rebuilt, cli);

        assert!(
            LoginStateBinding::from_columns("browser", Some(vec![1; 32]), Some(Uuid::now_v7()))
                .is_err()
        );
        assert!(LoginStateBinding::from_columns("cli", None, None).is_err());
        assert!(LoginStateBinding::from_columns("browser", Some(vec![1; 3]), None).is_err());
    }
}
