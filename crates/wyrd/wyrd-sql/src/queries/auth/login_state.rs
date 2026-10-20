//! Tenant-scoped login state queries for the OIDC authorization-code flow.
//!
//! A row is keyed by the SHA-256 of its random state value and moves through
//! two steps, each one statement on the caller's RLS [`TenantConn`]:
//! [`insert_login_state`] when a login begins and [`consume_login_state`] when
//! the provider callback arrives (before any provider IO). A browser login is
//! bound to the digest of its beginning browser's login-transaction secret, a
//! device login to its device authorization, and a candidate connection test
//! to the principal that began it. Rows retained from the retired OAuth
//! authorization-code profile are never consumed and only expire. Forced RLS
//! is the only tenant selection; `data_tenant_id` is written as the row's
//! owner and never repeated as a predicate. `PostgreSQL` owns every expiry:
//! callers bind lifetimes, never instants.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_spec::auth::{
    ConnectionTester, LoginInitiation, PrincipalId, PrincipalKindTag, Sha256Hex,
};

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

/// Drop this tenant's rows that can no longer be consumed.
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
        client_id, redirect_uri, code_verifier, nonce, browser_binding_hash,
        device_id, tester_principal_id, tester_principal_kind, expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
              statement_timestamp() + ($14 * interval '1 second'))
    ON CONFLICT DO NOTHING
";

/// Consume a login's state exactly once at the callback, before provider IO.
///
/// Only an unconsumed, unexpired row visible under forced RLS matches, and
/// setting `consumed_at` makes every replay of the same state match nothing.
/// A row retained from the retired OAuth client profile never matches.
/// Returns the pinned connection, PKCE verifier, nonce, and initiation binding
/// the callback needs to finish the exchange.
const CONSUME_LOGIN_STATE_SQL: &str = r"
    UPDATE wyrd.auth_login_state
       SET consumed_at = statement_timestamp()
     WHERE state_hash = $1
       AND consumed_at IS NULL
       AND oauth_client_id IS NULL
       AND expires_at > statement_timestamp()
    RETURNING connection_id, connection_revision, issuer, client_id, redirect_uri,
              code_verifier, nonce, browser_binding_hash, device_id,
              tester_principal_id, tester_principal_kind
";

/// The stored binding columns of one initiation; exactly one binding is set,
/// which is how the stored row records its kind.
#[derive(Debug, Default, PartialEq, Eq)]
struct InitiationColumns<'a> {
    /// Digest of the beginning browser's login-transaction secret, for a
    /// browser login.
    browser_binding_hash: Option<&'a [u8]>,
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
        LoginInitiation::Browser(binding) => InitiationColumns {
            browser_binding_hash: Some(binding.as_bytes().as_slice()),
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

/// Rebuild the initiation from its stored binding columns.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] when the columns do not describe exactly
/// one complete binding, the browser digest is not 32 bytes, or the tester
/// kind is not a principal kind label.
fn initiation_from_columns(row: &ConsumedRow) -> Result<LoginInitiation, sqlx::Error> {
    let corrupt = || sqlx::Error::Decode("login state binding is corrupt".into());
    match (
        &row.browser_binding_hash,
        row.device_id,
        row.tester_principal_id,
        &row.tester_principal_kind,
    ) {
        (Some(binding), None, None, None) => {
            let digest: [u8; 32] = binding.as_slice().try_into().map_err(|_| corrupt())?;
            Ok(LoginInitiation::Browser(Sha256Hex::from(digest)))
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
    /// Browser login-transaction digest, for a browser login.
    browser_binding_hash: Option<Vec<u8>>,
    /// Device authorization id, for a device-code login.
    device_id: Option<Uuid>,
    /// Test principal id, for a candidate connection test.
    tester_principal_id: Option<Uuid>,
    /// Test principal kind label, for a candidate connection test.
    tester_principal_kind: Option<String>,
}

/// Insert a login-state row keyed by `state_hash` whose expiry `PostgreSQL`
/// derives from `ttl`.
///
/// First purges this tenant's rows that are past their expiry, so abandoned
/// logins do not accumulate; RLS confines the purge to the connection's
/// tenant. The insert refuses to overwrite: when the state hash or the device
/// id is already recorded the row is not written and `false` is returned, so
/// a device is never bound to two logins.
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
        .bind(columns.browser_binding_hash)
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
/// Marks the row consumed rather than deleting it; expiry purge removes it. A
/// missing, expired, already consumed, retired-profile, or other tenant's row
/// returns `None`.
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

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use uuid::Uuid;
    use wyrd_spec::auth::{
        ConnectionTester, LoginInitiation, PrincipalId, PrincipalKindTag, Sha256Hex,
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
            browser_binding_hash: columns.browser_binding_hash.map(<[u8]>::to_vec),
            device_id: columns.device_id,
            tester_principal_id: columns.tester_principal_id,
            tester_principal_kind: columns.tester_principal_kind.map(str::to_owned),
        }
    }

    /// A browser login bound to a fixed login-transaction digest.
    fn browser() -> LoginInitiation {
        LoginInitiation::Browser(Sha256Hex::digest(b"login-transaction"))
    }

    /// Every initiation round-trips through its stored columns, and a stored
    /// row naming several or no bindings, a short browser digest, or an
    /// unknown tester kind is refused as corrupt.
    ///
    /// # Panics
    /// Panics when a binding round-trips differently.
    #[test]
    fn initiation_columns_round_trip_and_refuse_ambiguity() {
        let tester = LoginInitiation::ConnectionTest(ConnectionTester {
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::TenantAdmin,
        });
        for initiation in [browser(), LoginInitiation::Device(Uuid::now_v7()), tester] {
            assert_eq!(
                initiation_from_columns(&row(&initiation)).expect("binding rebuilds"),
                initiation
            );
        }

        let mut both = row(&browser());
        both.device_id = Some(Uuid::now_v7());
        let mut none = row(&LoginInitiation::Device(Uuid::now_v7()));
        none.device_id = None;
        let mut short = row(&browser());
        short.browser_binding_hash = Some(vec![0; 16]);
        let mut unknown_kind = row(&LoginInitiation::ConnectionTest(ConnectionTester {
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
        }));
        unknown_kind.tester_principal_kind = Some("root".to_owned());
        for corrupt in [both, none, short, unknown_kind] {
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
            initiation: browser(),
        };

        let printed = format!("{state:?} {state:#?}");

        assert!(!printed.contains(sentinel), "{printed}");
    }
}
