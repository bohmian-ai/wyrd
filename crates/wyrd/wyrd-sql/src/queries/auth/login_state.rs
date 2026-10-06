//! Tenant-scoped login state queries for the OIDC authorization-code flow.
//!
//! A row is keyed by the SHA-256 of its random state value and moves through
//! up to three steps, each one statement on the caller's RLS [`TenantConn`]:
//! [`insert_login_state`] when a login begins, [`consume_login_state`] when the
//! provider callback arrives (before any provider IO), and, for a login that
//! answers an OAuth authorization request, [`issue_authorization_code`] once
//! the callback has verified the sign-in. [`redeem_authorization_code`] then
//! deletes that row once at the token endpoint (RFC 6749 §4.1.2). A device
//! login records its approval on the device authorization instead, and a
//! candidate connection test is bound to the principal that began it; neither
//! ever carries a code. Forced RLS is the only tenant selection;
//! `data_tenant_id` is written as the row's owner and never repeated as a
//! predicate. `PostgreSQL` owns every expiry: callers bind lifetimes, never
//! instants.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_spec::auth::{
    ClientAuthorization, ConnectionTester, LoginInitiation, OAuthClientId, PrincipalId,
    PrincipalKindTag, Sha256Hex,
};

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

/// Drop this tenant's rows that can no longer be consumed or redeemed.
const PURGE_EXPIRED_LOGIN_STATE_SQL: &str = r"
    DELETE FROM wyrd.auth_login_state
     WHERE expires_at <= statement_timestamp()
";

/// Begin a login: record one unconsumed row owned by the RLS tenant.
///
/// Forced RLS's `WITH CHECK` refuses a `data_tenant_id` other than the
/// connection's tenant. `ON CONFLICT DO NOTHING` never overwrites: a reused
/// state hash or device id inserts nothing, so one binding names at most one
/// login. `PostgreSQL` derives `expires_at` from the bound lifetime in seconds.
const INSERT_LOGIN_STATE_SQL: &str = r"
    INSERT INTO wyrd.auth_login_state (
        state_hash, data_tenant_id, connection_id, connection_revision, issuer,
        client_id, redirect_uri, code_verifier, nonce, oauth_client_id,
        client_redirect_uri, code_challenge, client_state, device_id,
        tester_principal_id, tester_principal_kind, expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
              statement_timestamp() + ($17 * interval '1 second'))
    ON CONFLICT DO NOTHING
";

/// Consume a login's state exactly once at the callback, before provider IO.
///
/// Only an unconsumed, unexpired row visible under forced RLS matches, and
/// setting `consumed_at` makes every replay of the same state match nothing.
/// Returns the pinned connection, PKCE verifier, nonce, and initiation binding
/// the callback needs to finish the exchange.
const CONSUME_LOGIN_STATE_SQL: &str = r"
    UPDATE wyrd.auth_login_state
       SET consumed_at = statement_timestamp()
     WHERE state_hash = $1
       AND consumed_at IS NULL
       AND expires_at > statement_timestamp()
    RETURNING connection_id, connection_revision, issuer, client_id, redirect_uri,
              code_verifier, nonce, oauth_client_id, client_redirect_uri,
              code_challenge, client_state, device_id, tester_principal_id,
              tester_principal_kind
";

/// Attach an authorization code and its principal to a consumed
/// authorization-request login, once.
///
/// Matches only a consumed row of the RLS tenant that answers an OAuth client
/// and has no code yet, so a code is issued at most once and never before
/// consumption. Resets `expires_at` to the code's short `PostgreSQL`-derived
/// lifetime.
const ISSUE_AUTHORIZATION_CODE_SQL: &str = r"
    UPDATE wyrd.auth_login_state
       SET code_hash = $2,
           principal_id = $3,
           expires_at = statement_timestamp() + ($4 * interval '1 second')
     WHERE state_hash = $1
       AND consumed_at IS NOT NULL
       AND oauth_client_id IS NOT NULL
       AND code_hash IS NULL
";

/// Redeem an authorization code once.
///
/// Deletes the RLS tenant's row whose code hashes to `$1` and returns its
/// client binding, principal, and connection, with whether it was still
/// unexpired. Deletion is the one-use guarantee: a second redemption matches
/// nothing, and an expired code is removed as it is refused.
const REDEEM_AUTHORIZATION_CODE_SQL: &str = r"
    DELETE FROM wyrd.auth_login_state
     WHERE code_hash = $1
    RETURNING oauth_client_id, client_redirect_uri, code_challenge, principal_id,
              connection_id, connection_revision,
              expires_at > statement_timestamp() AS live
";

/// The stored binding columns of one initiation; exactly one binding is set,
/// which is how the stored row records its kind.
#[derive(Debug, Default, PartialEq, Eq)]
struct InitiationColumns<'a> {
    /// The OAuth client, for an authorization-request login.
    oauth_client_id: Option<&'static str>,
    /// That client's redirect URI.
    client_redirect_uri: Option<&'a str>,
    /// That client's PKCE S256 challenge.
    code_challenge: Option<&'a str>,
    /// That client's opaque `state`, when it sent one.
    client_state: Option<&'a str>,
    /// Device authorization id, for a device-code login.
    device_id: Option<Uuid>,
    /// Principal that began a candidate connection test.
    tester_principal_id: Option<Uuid>,
    /// That principal's kind label.
    tester_principal_kind: Option<&'static str>,
}

/// Project an initiation onto its stored binding columns.
fn initiation_columns(initiation: &LoginInitiation) -> InitiationColumns<'_> {
    match initiation {
        LoginInitiation::Authorize(authorization) => InitiationColumns {
            oauth_client_id: Some(authorization.client.as_str()),
            client_redirect_uri: Some(&authorization.redirect_uri),
            code_challenge: Some(&authorization.code_challenge),
            client_state: authorization.state.as_deref(),
            ..InitiationColumns::default()
        },
        LoginInitiation::Device(device_id) => InitiationColumns {
            device_id: Some(*device_id),
            ..InitiationColumns::default()
        },
        LoginInitiation::ConnectionTest(tester) => InitiationColumns {
            tester_principal_id: Some(tester.principal_id.as_uuid()),
            tester_principal_kind: Some(tester.principal_kind.as_str()),
            ..InitiationColumns::default()
        },
    }
}

/// The stored client column decoded, refusing a value no client is named by.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] for an unknown client id.
fn stored_client(client_id: &str) -> Result<OAuthClientId, sqlx::Error> {
    OAuthClientId::parse(client_id)
        .ok_or_else(|| sqlx::Error::Decode("login state client is corrupt".into()))
}

/// Rebuild the initiation from its stored binding columns.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] when the columns do not describe exactly
/// one complete binding, the client is unknown, or the tester kind is not a
/// principal kind label.
fn initiation_from_columns(row: &ConsumedRow) -> Result<LoginInitiation, sqlx::Error> {
    let corrupt = || sqlx::Error::Decode("login state binding is corrupt".into());
    match (
        &row.oauth_client_id,
        row.device_id,
        row.tester_principal_id,
        &row.tester_principal_kind,
    ) {
        (Some(client), None, None, None) => {
            let (Some(redirect_uri), Some(code_challenge)) =
                (&row.client_redirect_uri, &row.code_challenge)
            else {
                return Err(corrupt());
            };
            Ok(LoginInitiation::Authorize(ClientAuthorization {
                client: stored_client(client)?,
                redirect_uri: redirect_uri.clone(),
                code_challenge: code_challenge.clone(),
                state: row.client_state.clone(),
            }))
        }
        (None, Some(device_id), None, None) => Ok(LoginInitiation::Device(device_id)),
        (None, None, Some(principal_id), Some(kind)) => {
            let principal_kind = serde_json::from_value::<PrincipalKindTag>(kind.clone().into())
                .map_err(|_| corrupt())?;
            Ok(LoginInitiation::ConnectionTest(ConnectionTester {
                principal_id: PrincipalId::new(principal_id),
                principal_kind,
            }))
        }
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
    /// Exact provider OAuth client id of that connection revision.
    pub client_id: String,
    /// The deployment-controlled callback the provider redirects to.
    pub redirect_uri: String,
    /// Server-generated PKCE verifier for the provider.
    pub code_verifier: SecretString,
    /// Server-generated nonce the ID token must echo.
    pub nonce: String,
    /// How the login was initiated and what its sign-in grants.
    pub initiation: LoginInitiation,
}

/// An authorization code removed by [`redeem_authorization_code`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedeemedCode {
    /// The client the code was issued to.
    pub client: OAuthClientId,
    /// The redirect URI the authorization request named.
    pub redirect_uri: String,
    /// The PKCE S256 challenge the authorization request carried.
    pub code_challenge: String,
    /// The signed-in principal.
    pub principal_id: Uuid,
    /// The connection revision the sign-in went through.
    pub connection: HumanConnectionBinding,
    /// Whether the code was still unexpired.
    pub live: bool,
}

/// Raw consumed row as returned by [`CONSUME_LOGIN_STATE_SQL`].
#[derive(sqlx::FromRow)]
struct ConsumedRow {
    /// Bound connection id and revision.
    #[sqlx(flatten)]
    connection: HumanConnectionBinding,
    /// Exact issuer.
    issuer: String,
    /// Exact provider client id.
    client_id: String,
    /// Callback URI.
    redirect_uri: String,
    /// PKCE verifier; wrapped in a [`SecretString`] as soon as it is decoded.
    code_verifier: String,
    /// Nonce.
    nonce: String,
    /// OAuth client, for an authorization-request login.
    oauth_client_id: Option<String>,
    /// That client's redirect URI.
    client_redirect_uri: Option<String>,
    /// That client's PKCE challenge.
    code_challenge: Option<String>,
    /// That client's `state`.
    client_state: Option<String>,
    /// Device authorization id, for a device-code login.
    device_id: Option<Uuid>,
    /// Test principal id, for a candidate connection test.
    tester_principal_id: Option<Uuid>,
    /// Test principal kind label, for a candidate connection test.
    tester_principal_kind: Option<String>,
}

/// Raw redeemed row as returned by [`REDEEM_AUTHORIZATION_CODE_SQL`].
#[derive(sqlx::FromRow)]
struct RedeemedRow {
    /// The client the code was issued to.
    oauth_client_id: Option<String>,
    /// The client's redirect URI.
    client_redirect_uri: Option<String>,
    /// The client's PKCE challenge.
    code_challenge: Option<String>,
    /// The signed-in principal.
    principal_id: Option<Uuid>,
    /// Bound connection id and revision.
    #[sqlx(flatten)]
    connection: HumanConnectionBinding,
    /// Whether the code was unexpired.
    live: bool,
}

/// Insert a login-state row keyed by `state_hash` whose expiry `PostgreSQL`
/// derives from `ttl`.
///
/// First purges this tenant's rows that are past their expiry, so abandoned
/// logins and unredeemed codes do not accumulate; RLS confines the purge to
/// the connection's tenant. The insert refuses to overwrite: when the state
/// hash or the device id is already recorded the row is not written and
/// `false` is returned, so a device is never bound to two logins.
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
    let columns = initiation_columns(&row.initiation);
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
        .bind(columns.oauth_client_id)
        .bind(columns.client_redirect_uri)
        .bind(columns.code_challenge)
        .bind(columns.client_state)
        .bind(columns.device_id)
        .bind(columns.tester_principal_id)
        .bind(columns.tester_principal_kind)
        .bind(ttl.as_secs_f64())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(inserted.rows_affected() == 1)
}

/// Consume an unconsumed, unexpired login-state row exactly once.
///
/// Marks the row consumed rather than deleting it, so the callback can later
/// attach an authorization code to the same row. A missing, expired, already
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
            initiation: initiation_from_columns(&row)?,
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

/// Attach the authorization code hashing to `code_hash` and the signed-in
/// `principal_id` to a consumed authorization-request login, and restart its
/// expiry at `ttl`, the code lifetime.
///
/// Returns `false` when this tenant has no consumed, code-less
/// authorization-request row with this state hash.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn issue_authorization_code(
    conn: &mut TenantConn<'_>,
    state_hash: &Sha256Hex,
    code_hash: &Sha256Hex,
    principal_id: Uuid,
    ttl: Duration,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(ISSUE_AUTHORIZATION_CODE_SQL)
        .bind(state_hash.as_bytes().as_slice())
        .bind(code_hash.as_bytes().as_slice())
        .bind(principal_id)
        .bind(ttl.as_secs_f64())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Delete this tenant's row holding the authorization code that hashes to
/// `code_hash` and return what the code was bound to; `None` when no row
/// holds it.
///
/// The delete is the single use: a second redemption finds nothing, and an
/// expired code is returned with `live == false` and removed all the same.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete or the stored
/// binding is corrupt.
pub async fn redeem_authorization_code(
    conn: &mut TenantConn<'_>,
    code_hash: &Sha256Hex,
) -> Result<Option<RedeemedCode>, sqlx::Error> {
    let row = sqlx::query_as::<_, RedeemedRow>(REDEEM_AUTHORIZATION_CODE_SQL)
        .bind(code_hash.as_bytes().as_slice())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(|row| {
        let (Some(client), Some(redirect_uri), Some(code_challenge), Some(principal_id)) = (
            row.oauth_client_id,
            row.client_redirect_uri,
            row.code_challenge,
            row.principal_id,
        ) else {
            return Err(sqlx::Error::Decode(
                "authorization code binding is corrupt".into(),
            ));
        };
        Ok(RedeemedCode {
            client: stored_client(&client)?,
            redirect_uri,
            code_challenge,
            principal_id,
            connection: row.connection,
            live: row.live,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use uuid::Uuid;
    use wyrd_spec::auth::{
        ClientAuthorization, ConnectionTester, LoginInitiation, OAuthClientId, PrincipalId,
        PrincipalKindTag,
    };

    use super::{ConsumedRow, LoginState, initiation_columns, initiation_from_columns};
    use crate::row_types::auth::HumanConnectionBinding;

    /// A consumed row carrying exactly the binding columns `initiation`
    /// projects to.
    fn row(initiation: &LoginInitiation) -> ConsumedRow {
        let columns = initiation_columns(initiation);
        ConsumedRow {
            connection: HumanConnectionBinding {
                connection_id: Uuid::now_v7(),
                connection_revision: 1,
            },
            issuer: String::new(),
            client_id: String::new(),
            redirect_uri: String::new(),
            code_verifier: String::new(),
            nonce: String::new(),
            oauth_client_id: columns.oauth_client_id.map(str::to_owned),
            client_redirect_uri: columns.client_redirect_uri.map(str::to_owned),
            code_challenge: columns.code_challenge.map(str::to_owned),
            client_state: columns.client_state.map(str::to_owned),
            device_id: columns.device_id,
            tester_principal_id: columns.tester_principal_id,
            tester_principal_kind: columns.tester_principal_kind.map(str::to_owned),
        }
    }

    /// An authorization request from `wyrd-ui`.
    fn authorize() -> LoginInitiation {
        LoginInitiation::Authorize(ClientAuthorization {
            client: OAuthClientId::WyrdUi,
            redirect_uri: "https://wyrd.example.com/login/callback".to_owned(),
            code_challenge: "challenge".to_owned(),
            state: Some("client-state".to_owned()),
        })
    }

    /// Every initiation round-trips through its stored columns, and a stored
    /// row naming several or no bindings, an incomplete client binding, an
    /// unknown client, or an unknown tester kind is refused as corrupt.
    ///
    /// # Panics
    /// Panics when a binding round-trips differently.
    #[test]
    fn initiation_columns_round_trip_and_refuse_ambiguity() {
        let tester = LoginInitiation::ConnectionTest(ConnectionTester {
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::TenantAdmin,
        });
        for initiation in [authorize(), LoginInitiation::Device(Uuid::now_v7()), tester] {
            assert_eq!(
                initiation_from_columns(&row(&initiation)).expect("binding rebuilds"),
                initiation
            );
        }

        let mut both = row(&authorize());
        both.device_id = Some(Uuid::now_v7());
        let mut none = row(&LoginInitiation::Device(Uuid::now_v7()));
        none.device_id = None;
        let mut incomplete = row(&authorize());
        incomplete.code_challenge = None;
        let mut unknown_client = row(&authorize());
        unknown_client.oauth_client_id = Some("other".to_owned());
        let mut unknown_kind = row(&LoginInitiation::ConnectionTest(ConnectionTester {
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
        }));
        unknown_kind.tester_principal_kind = Some("root".to_owned());
        for corrupt in [both, none, incomplete, unknown_client, unknown_kind] {
            assert!(initiation_from_columns(&corrupt).is_err());
        }
    }

    /// Formatting a login state never prints its PKCE verifier.
    ///
    /// # Panics
    /// Panics when the verifier is printed.
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
            initiation: authorize(),
        };

        let printed = format!("{state:?} {state:#?}");

        assert!(!printed.contains(sentinel), "{printed}");
    }
}
