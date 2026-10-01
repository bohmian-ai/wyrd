//! Server-owned production UI browser sessions.
//!
//! The `SvelteKit` BFF holds only an opaque random session id in an `HttpOnly`
//! cookie; [`BrowserSessions`] owns the Postgres record behind it and the Wyrd
//! credential it stands for. It is storage for the BFF's session, not an
//! identity issuer: every access token it hands out was minted by the ordinary
//! tenant issuance path ([`RefreshTokens`] for an SSO session,
//! [`ExchangeApiKey`] for an OIDC-off operator session), so a suspended user,
//! a withdrawn grant, or a deactivated login connection governs a browser
//! session exactly as it governs any other client.
//!
//! The tenant is never caller-supplied: a session is located by the hash of
//! its random id, a login completion by the hash of the BFF's flow id, and an
//! API-key session by the key's own embedded tenant, which must equal the
//! route key the person signed in at.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_verify::TokenVerifier;
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{LoginInitiation, Sha256Hex, TokenResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{
    BrowserSessionMode, BrowserSessionWrite, LockedBrowserSession, insert_browser_session,
    lock_browser_session, redeem_login_completion, refresh_by_hash, revoke_browser_session,
    revoke_refresh, rotate_browser_session,
};
use wyrd_sql::{TenantConn, WyrdPostgres};

use crate::connections::HumanConnections;
use crate::credential_verify::verify_presented;
use crate::error::{auth_error_to_wyrd, store_error};
use crate::exchange_api_key::{ExchangeApiKey, api_key_invalid, token_hash};
use crate::issuance::{ExchangedToken, TenantTokenIssuer};
use crate::refresh::{RefreshError, RefreshTokens, claims_from_refresh_jwt};

/// Absolute lifetime of a tenant SSO browser session; refresh rotation keeps
/// its access token current until then.
const SSO_SESSION_LIFETIME: Duration = Duration::from_hours(12);

/// Absolute lifetime of an OIDC-off API-key browser session.
const API_KEY_SESSION_LIFETIME: Duration = Duration::from_hours(8);

/// A stored access token this close to expiry is renewed before it is used,
/// so the BFF never forwards a token that expires mid-request.
const RENEW_MARGIN: Duration = Duration::from_mins(1);

/// A session just created by [`BrowserSessions::complete`] or
/// [`BrowserSessions::exchange_api_key`].
#[derive(Debug)]
pub struct CreatedBrowserSession {
    /// The raw 256-bit session id, lowercase hex. Returned once; only its
    /// SHA-256 is stored.
    pub session_id: SecretString,
    /// Route key of the tenant the session belongs to.
    pub tenant_key: TenantSlug,
    /// When the session ends regardless of renewal.
    pub expires_at: DateTime<Utc>,
}

/// The safe projection of a live session the BFF renders pages from.
#[derive(Debug)]
pub struct BrowserSessionView {
    /// The session's tenant, as the session row itself names it.
    pub tenant_id: DataTenantId,
    /// Route key of the session's tenant.
    pub tenant_key: TenantSlug,
    /// Display name of the session's tenant.
    pub tenant_name: String,
    /// The principal the session acts as.
    pub principal_id: Uuid,
    /// Role names carried by the current access token.
    pub roles: Vec<String>,
    /// Permissions carried by the current access token.
    pub permissions: Vec<String>,
    /// When the session ends regardless of renewal.
    pub expires_at: DateTime<Utc>,
    /// The session CSRF token the BFF renders into forms.
    pub csrf_token: SecretString,
}

/// A current access token for one BFF-side Wyrd API call.
#[derive(Debug)]
pub struct BrowserSessionAuthority {
    /// The access token; never leaves the BFF server.
    pub access_token: SecretString,
    /// When it expires.
    pub access_expires_at: DateTime<Utc>,
}

/// A live session locked for the rest of its transaction, with an access
/// token that is fresh enough to use.
struct CurrentSession<'c> {
    /// The open tenant transaction holding the row lock.
    conn: TenantConn<'c>,
    /// The locked row as last stored.
    row: LockedBrowserSession,
    /// The usable access token, renewed if the stored one was stale.
    access_token: SecretString,
    /// Its expiry.
    access_expires_at: DateTime<Utc>,
}

/// Owner of production UI browser sessions.
///
/// Built once per server when the deployment enables the BFF channel. Holds
/// the runtime store, the deployment sealing keyring every stored credential
/// is sealed under, the tenant issuance owner renewals mint through, the token
/// verifier that reads the safe role summary from the server's own tokens, and
/// the human-connection owner whose Active connection decides whether a
/// tenant offers SSO.
#[derive(Clone)]
pub struct BrowserSessions {
    /// Runtime Postgres handle; every row statement runs under tenant RLS.
    postgres: WyrdPostgres,
    /// Deployment sealing keyring; `None` refuses creation and renewal.
    keyring: Option<Arc<SealingKeyring>>,
    /// The shared tenant issuance owner.
    issuer: TenantTokenIssuer,
    /// Verifier for the server's own access tokens.
    verifier: Arc<TokenVerifier>,
    /// Tenant human-connection owner.
    connections: HumanConnections,
}

impl std::fmt::Debug for BrowserSessions {
    /// Render only the type name: the owner holds the sealing keyring and the
    /// issuing key, so no field is ever printed into logs or traces.
    ///
    /// # Errors
    /// Returns the formatter's error when writing fails.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserSessions").finish_non_exhaustive()
    }
}

impl BrowserSessions {
    /// Build the owner over its store, keyring, issuer, verifier, and
    /// connection owner.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        keyring: Option<Arc<SealingKeyring>>,
        issuer: TenantTokenIssuer,
        verifier: Arc<TokenVerifier>,
        connections: HumanConnections,
    ) -> Self {
        Self {
            postgres,
            keyring,
            issuer,
            verifier,
            connections,
        }
    }

    /// Whether `tenant_key` currently offers SSO sign-in.
    ///
    /// True only for an active tenant with an Active human connection; an
    /// unknown tenant answers `false` exactly like a tenant without SSO, so the
    /// answer enumerates nothing.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails or the
    /// Active connection cannot be opened.
    pub async fn sso_available(&self, tenant_key: &TenantSlug) -> Result<bool, WyrdError> {
        let Some(tenant) = self
            .postgres
            .resolve_tenant_slug(tenant_key)
            .await
            .map_err(store_error)?
        else {
            return Ok(false);
        };
        Ok(self.connections.active_connection(tenant).await?.is_some())
    }

    /// Turn the completed SSO login bound to `flow_id` into a browser session.
    ///
    /// `flow_id` is the raw value of the BFF's `HttpOnly` flow cookie; its
    /// SHA-256 is the binding login began with, and the only thing that names
    /// the tenant. One tenant transaction redeems the sealed completion (the
    /// delete is its single use), stores the session with its token pair and
    /// the CSRF token sealed under the deployment keyring, and commits. A
    /// completion that cannot be opened is still consumed.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] (`sealing_key_missing`) without a
    /// keyring or for a malformed flow id or CSRF token,
    /// [`WyrdError::InvalidState`] when no completed, unexpired login waits
    /// for the flow (never completed, already redeemed, expired),
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails, and
    /// [`WyrdError::Internal`] when the completion or its tokens cannot be
    /// read.
    pub async fn complete(
        &self,
        flow_id: &SecretString,
        csrf_token: &SecretString,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let keyring = self.require_keyring()?;
        let flow_hash = Sha256Hex::digest(lower_hex_256(flow_id, "flow_id")?.as_bytes());
        let csrf = lower_hex_256(csrf_token, "csrf_token")?;
        let tenant = self
            .postgres
            .login_completion_tenant(&flow_hash)
            .await
            .map_err(store_error)?
            .ok_or_else(completion_missing)?;
        let tenant_key = self.tenant_entry(tenant).await?.0;
        let mut conn = self.begin(tenant).await?;
        let redeemed = redeem_login_completion(&mut conn, &LoginInitiation::Browser(flow_hash))
            .await
            .map_err(store_error)?
            .ok_or_else(completion_missing)?;
        let token = match open_completion(keyring, &redeemed.sealed) {
            Ok(token) => token,
            Err(error) => {
                // The completion is single-use even when it is unusable.
                conn.commit().await.map_err(store_error)?;
                return Err(error);
            }
        };
        let refresh = token
            .refresh_token
            .as_ref()
            .ok_or_else(|| unusable("login completion carries no refresh token"))?;
        let refresh_expires_at = refresh_expiry(refresh.expose())?;
        let principal_id = self.principal_of(tenant, token.access_token.expose())?;
        let write = BrowserSessionWrite {
            principal_id,
            connection_id: Some(redeemed.connection_id),
            mode: BrowserSessionMode::OidcRefresh,
            access_token_sealed: seal(keyring, token.access_token.expose())?,
            access_expires_at: token.expires_at,
            refresh_token_sealed: Some(seal(keyring, refresh.expose())?),
            refresh_expires_at: Some(refresh_expires_at),
            api_key_sealed: None,
            csrf_hash: Sha256Hex::digest(csrf.as_bytes()),
            csrf_token_sealed: seal(keyring, &csrf)?,
            lifetime: SSO_SESSION_LIFETIME,
        };
        self.insert(conn, tenant_key, &write).await
    }

    /// Exchange an existing tenant API key for an OIDC-off browser session.
    ///
    /// The route tenant `tenant_key` names is resolved first and the presented
    /// key is handed, unparsed, to the ordinary audited API-key exchange on
    /// that tenant's connection; the shared verifier there decides malformed,
    /// other-tenant, unknown, revoked, and wrong-secret keys at the cost of
    /// exactly one verification. An unknown route tenant has no connection, so
    /// it pays that one verification against the fixed dummy instead. Every
    /// refusal is the one indistinguishable API-key refusal, and only a
    /// successful exchange stores a session: the key is sealed so each later
    /// renewal re-exchanges it under the session lock. No refresh token
    /// exists; the session ends after eight hours or when the key stops
    /// exchanging.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] without a keyring or for a malformed
    /// CSRF token, [`WyrdError::ApiKeyInvalid`] for an unknown route tenant, an
    /// unusable key, or one of another tenant, [`WyrdError::Internal`] when
    /// the dummy verification task fails, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn exchange_api_key(
        &self,
        tenant_key: &TenantSlug,
        api_key: &SecretString,
        csrf_token: &SecretString,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let keyring = self.require_keyring()?;
        let csrf = lower_hex_256(csrf_token, "csrf_token")?;
        let Some(tenant) = self
            .postgres
            .resolve_tenant_slug(tenant_key)
            .await
            .map_err(store_error)?
        else {
            verify_presented(api_key, None)
                .await
                .map_err(|_| unusable("api key verification failed"))?;
            return Err(api_key_invalid());
        };
        let mut conn = self.begin(tenant).await?;
        let exchanged = ExchangeApiKey {
            issuer: self.issuer.clone(),
        }
        .execute(&mut conn, api_key.clone(), request_id)
        .await
        .map_err(|error| {
            tracing::info!(error = %error, "browser api key sign-in refused");
            api_key_invalid()
        })?;
        let principal_id = self.principal_of(tenant, exchanged.access_token.expose_secret())?;
        let write = BrowserSessionWrite {
            principal_id,
            connection_id: None,
            mode: BrowserSessionMode::ApiKeyExchange,
            access_token_sealed: seal(keyring, exchanged.access_token.expose_secret())?,
            access_expires_at: exchanged.expires_at,
            refresh_token_sealed: None,
            refresh_expires_at: None,
            api_key_sealed: Some(seal(keyring, api_key.expose_secret())?),
            csrf_hash: Sha256Hex::digest(csrf.as_bytes()),
            csrf_token_sealed: seal(keyring, &csrf)?,
            lifetime: API_KEY_SESSION_LIFETIME,
        };
        self.insert(conn, tenant_key.clone(), &write).await
    }

    /// Read the safe projection of the live session `session_id` names,
    /// renewing its access token first when it is stale.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for an unknown, revoked, expired, or
    /// no-longer-renewable session, and the store and keyring errors of
    /// renewal.
    pub async fn read(
        &self,
        session_id: &SecretString,
        request_id: &str,
    ) -> Result<BrowserSessionView, WyrdError> {
        let (tenant, current) = self.current(session_id, request_id).await?;
        let keyring = self.require_keyring()?;
        let csrf_token = open_text(keyring, &current.row.csrf_token_sealed)?;
        let verified = self
            .verifier
            .verify(&current.access_token, &tenant)
            .map_err(auth_error_to_wyrd)?;
        let expires_at = current.row.absolute_expires_at;
        current.conn.commit().await.map_err(store_error)?;
        let (tenant_key, tenant_name) = self.tenant_entry(tenant).await?;
        Ok(BrowserSessionView {
            tenant_id: tenant,
            tenant_key,
            tenant_name,
            principal_id: verified.principal.id.as_uuid(),
            roles: verified
                .principal
                .roles
                .iter()
                .map(|role| role.as_str().to_owned())
                .collect(),
            // `resource:action` labels; a composite `AnyOf` grant has no
            // label (its `Display` errors) and is left out of this projection.
            permissions: verified
                .principal
                .effective_permissions
                .iter()
                .filter_map(|permission| {
                    Some(format!(
                        "{}:{}",
                        permission.resource.as_str()?,
                        permission.action.as_str()?
                    ))
                })
                .collect(),
            expires_at,
            csrf_token,
        })
    }

    /// Return a current access token for the session `session_id` names,
    /// renewing it first when it is stale.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for an unknown, revoked, expired, or
    /// no-longer-renewable session, and the store and keyring errors of
    /// renewal.
    pub async fn authority(
        &self,
        session_id: &SecretString,
        request_id: &str,
    ) -> Result<BrowserSessionAuthority, WyrdError> {
        let (_, current) = self.current(session_id, request_id).await?;
        let authority = BrowserSessionAuthority {
            access_token: current.access_token,
            access_expires_at: current.access_expires_at,
        };
        current.conn.commit().await.map_err(store_error)?;
        Ok(authority)
    }

    /// End the session `session_id` names. Idempotent: an unknown, expired,
    /// or already revoked session is a no-op.
    ///
    /// An SSO session's current refresh token is revoked with it, so its
    /// rotation chain cannot continue; an API-key session's stored key is
    /// wiped, while the operator's API key itself stays valid.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn logout(&self, session_id: &SecretString) -> Result<(), WyrdError> {
        let id_hash = session_hash(session_id)?;
        let Some(tenant) = self
            .postgres
            .browser_session_tenant(&id_hash)
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        let mut conn = self.begin(tenant).await?;
        let Some(row) = lock_browser_session(&mut conn, &id_hash, Duration::ZERO)
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        if let (Some(sealed), Some(keyring)) = (&row.refresh_token_sealed, &self.keyring)
            && let Ok(refresh) = open_text(keyring, sealed)
            && let Some(stored) = refresh_by_hash(&mut conn, &token_hash(refresh.expose_secret()))
                .await
                .map_err(store_error)?
        {
            revoke_refresh(&mut conn, stored.id, "browser_logout")
                .await
                .map_err(store_error)?;
        }
        revoke_browser_session(&mut conn, &id_hash)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)
    }

    /// Lock the live session `session_id` names and make its access token
    /// usable, renewing it under the row lock when it is stale.
    ///
    /// A concurrent replica blocks on the row lock and then sees the winner's
    /// rotated credential as fresh, so one renewal serves both. A renewal the
    /// issuance path refuses — a revoked or reused refresh token, a
    /// deactivated or replaced connection, a revoked API key, a suspended
    /// principal — revokes the session in the same transaction and commits;
    /// no other credential is tried.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for an unknown, revoked, expired, or
    /// no-longer-renewable session, [`WyrdError::Validation`] without a
    /// keyring, and [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    async fn current(
        &self,
        session_id: &SecretString,
        request_id: &str,
    ) -> Result<(DataTenantId, CurrentSession<'_>), WyrdError> {
        let id_hash = session_hash(session_id)?;
        let tenant = self
            .postgres
            .browser_session_tenant(&id_hash)
            .await
            .map_err(store_error)?
            .ok_or_else(session_ended)?;
        let mut conn = self.begin(tenant).await?;
        let row = lock_browser_session(&mut conn, &id_hash, RENEW_MARGIN)
            .await
            .map_err(store_error)?
            .ok_or_else(session_ended)?;
        let keyring = self.require_keyring()?;
        if row.access_fresh {
            let access_token = open_text(keyring, &row.access_token_sealed)?;
            let access_expires_at = row.access_expires_at;
            return Ok((
                tenant,
                CurrentSession {
                    conn,
                    row,
                    access_token,
                    access_expires_at,
                },
            ));
        }
        let renewed = match self.renew(&mut conn, keyring, &row, request_id).await {
            Ok(renewed) => renewed,
            Err(Renewal::Refused) => {
                revoke_browser_session(&mut conn, &id_hash)
                    .await
                    .map_err(store_error)?;
                conn.commit().await.map_err(store_error)?;
                tracing::info!(tenant_id = %tenant, "browser session renewal refused; session ended");
                return Err(session_ended());
            }
            Err(Renewal::Failed(error)) => return Err(error),
        };
        let refresh = match &renewed.refresh_token {
            Some(token) => Some((
                seal(keyring, token.expose_secret())?,
                refresh_expiry(token.expose_secret())?,
            )),
            None => None,
        };
        rotate_browser_session(
            &mut conn,
            &id_hash,
            &seal(keyring, renewed.access_token.expose_secret())?,
            renewed.expires_at,
            refresh
                .as_ref()
                .map(|(sealed, expires_at)| (sealed.as_slice(), *expires_at)),
        )
        .await
        .map_err(store_error)?;
        Ok((
            tenant,
            CurrentSession {
                conn,
                row,
                access_token: renewed.access_token,
                access_expires_at: renewed.expires_at,
            },
        ))
    }

    /// Mint a successor access token for a locked session through the
    /// ordinary issuance path for its mode.
    ///
    /// # Errors
    /// Returns [`Renewal::Refused`] when the stored credential no longer
    /// issues (the session must end) and [`Renewal::Failed`] for store, audit,
    /// or keyring failures that leave the session as it was.
    async fn renew(
        &self,
        conn: &mut TenantConn<'_>,
        keyring: &SealingKeyring,
        row: &LockedBrowserSession,
        request_id: &str,
    ) -> Result<ExchangedToken, Renewal> {
        let issuer = self.issuer.clone();
        match row.mode {
            BrowserSessionMode::OidcRefresh => {
                let sealed = row
                    .refresh_token_sealed
                    .as_deref()
                    .ok_or(Renewal::Refused)?;
                let refresh = open_text(keyring, sealed).map_err(|_| Renewal::Refused)?;
                RefreshTokens { issuer }
                    .execute(conn, refresh, request_id)
                    .await
                    .map_err(|error| match error {
                        RefreshError::Database(_)
                        | RefreshError::Issuance(crate::issuance::IssuanceError::Database(_)) => {
                            Renewal::Failed(WyrdError::from(error))
                        }
                        _ => Renewal::Refused,
                    })
            }
            BrowserSessionMode::ApiKeyExchange => {
                let sealed = row.api_key_sealed.as_deref().ok_or(Renewal::Refused)?;
                let api_key = open_text(keyring, sealed).map_err(|_| Renewal::Refused)?;
                ExchangeApiKey { issuer }
                    .execute(conn, api_key, request_id)
                    .await
                    .map_err(|error| match error {
                        crate::exchange_api_key::ExchangeError::Database(error) => {
                            Renewal::Failed(store_error(error))
                        }
                        _ => Renewal::Refused,
                    })
            }
        }
    }

    /// Generate a session id, store `write` under its hash in `conn`'s
    /// tenant, and commit.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails and
    /// [`WyrdError::Internal`] in the astronomically unlikely event the random
    /// id collides.
    async fn insert(
        &self,
        mut conn: TenantConn<'_>,
        tenant_key: TenantSlug,
        write: &BrowserSessionWrite,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let session_id = random_hex_256();
        let id_hash = Sha256Hex::digest(session_id.as_bytes());
        let expires_at = insert_browser_session(&mut conn, &id_hash, write)
            .await
            .map_err(store_error)?
            .ok_or_else(|| unusable("browser session id collided"))?;
        conn.commit().await.map_err(store_error)?;
        Ok(CreatedBrowserSession {
            session_id: SecretString::from(session_id),
            tenant_key,
            expires_at,
        })
    }

    /// The principal a freshly issued access token names.
    ///
    /// # Errors
    /// Returns the verifier's refusal mapped to the public catalog.
    fn principal_of(&self, tenant: DataTenantId, access_token: &str) -> Result<Uuid, WyrdError> {
        let verified = self
            .verifier
            .verify(&SecretString::from(access_token.to_owned()), &tenant)
            .map_err(auth_error_to_wyrd)?;
        Ok(verified.principal.id.as_uuid())
    }

    /// The tenant's route key and display name.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for a deleted tenant and
    /// [`WyrdError::AuthVerifyUnavailable`] when the directory read fails.
    async fn tenant_entry(&self, tenant: DataTenantId) -> Result<(TenantSlug, String), WyrdError> {
        self.postgres
            .tenant_directory_entry(tenant)
            .await
            .map_err(store_error)?
            .ok_or_else(session_ended)
    }

    /// The deployment keyring, required for every sealed credential.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with reason `sealing_key_missing`.
    fn require_keyring(&self) -> Result<&SealingKeyring, WyrdError> {
        self.keyring
            .as_deref()
            .ok_or_else(|| WyrdError::Validation {
                message: "a deployment sealing key (WYRD_SEALING_KEY_FILE) is required to store \
                          browser sessions"
                    .to_owned(),
                details: json!({ "reason": "sealing_key_missing" }),
            })
    }

    /// Open one RLS tenant transaction.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when none can be acquired.
    async fn begin(&self, tenant: DataTenantId) -> Result<TenantConn<'_>, WyrdError> {
        self.postgres.tenant_conn(tenant).await.map_err(store_error)
    }
}

/// Why a renewal did not produce a token.
enum Renewal {
    /// The stored credential no longer issues; the session must end.
    Refused,
    /// An infrastructure failure; the session is left as it was.
    Failed(WyrdError),
}

/// The SHA-256 a session is stored under.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] for a value that is not 64 lowercase
/// hex characters, so a malformed cookie is just an unknown session.
fn session_hash(session_id: &SecretString) -> Result<Sha256Hex, WyrdError> {
    lower_hex_256(session_id, "session_id")
        .map(|id| Sha256Hex::digest(id.as_bytes()))
        .map_err(|_| session_ended())
}

/// Require `value` to be 64 lowercase hex characters (256 random bits).
///
/// # Errors
/// Returns [`WyrdError::Validation`] naming `field` otherwise.
fn lower_hex_256(value: &SecretString, field: &str) -> Result<String, WyrdError> {
    let value = value.expose_secret();
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value.to_owned())
    } else {
        Err(WyrdError::Validation {
            message: format!("{field} must be 64 lowercase hex characters"),
            details: json!({ "field": field }),
        })
    }
}

/// 32 random bytes as lowercase hex.
fn random_hex_256() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    Sha256Hex::from(bytes).to_string()
}

/// Seal `plaintext` under the deployment write key.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when sealing fails.
fn seal(keyring: &SealingKeyring, plaintext: &str) -> Result<Vec<u8>, WyrdError> {
    keyring
        .seal(plaintext.as_bytes())
        .map_err(|_| unusable("browser session credential could not be sealed"))
}

/// Open a sealed UTF-8 credential.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] when no held key opens it or it is not
/// UTF-8: a credential sealed under a retired key ends its session.
fn open_text(keyring: &SealingKeyring, sealed: &[u8]) -> Result<SecretString, WyrdError> {
    let opened = keyring.open(sealed).map_err(|_| session_ended())?;
    String::from_utf8(opened)
        .map(SecretString::from)
        .map_err(|_| session_ended())
}

/// Open and decode a sealed login completion.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when it cannot be opened or decoded.
fn open_completion(keyring: &SealingKeyring, sealed: &[u8]) -> Result<TokenResponse, WyrdError> {
    let opened = keyring
        .open(sealed)
        .map_err(|_| unusable("the login completion could not be processed; start a new login"))?;
    serde_json::from_slice(&opened)
        .map_err(|_| unusable("the login completion could not be processed; start a new login"))
}

/// The expiry a refresh token was signed with.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the token's claims cannot be read.
fn refresh_expiry(refresh_token: &str) -> Result<DateTime<Utc>, WyrdError> {
    let claims = claims_from_refresh_jwt(refresh_token)
        .map_err(|_| unusable("refresh token claims are unreadable"))?;
    i64::try_from(claims.exp)
        .ok()
        .and_then(|exp| DateTime::from_timestamp(exp, 0))
        .ok_or_else(|| unusable("refresh token expiry is out of range"))
}

/// The refusal for a session that is unknown, revoked, expired, or no longer
/// renewable; the BFF clears its cookie on it.
fn session_ended() -> WyrdError {
    WyrdError::InvalidToken {
        message: "the browser session is not valid; sign in again".to_owned(),
        details: json!({}),
    }
}

/// The refusal for a flow with no completed, unexpired login waiting.
fn completion_missing() -> WyrdError {
    WyrdError::InvalidState {
        message: "no completed login is waiting for this browser; start a new login".to_owned(),
        details: json!({ "reason": "login_completion_missing" }),
    }
}

/// An internal failure with a fixed message.
fn unusable(message: &str) -> WyrdError {
    WyrdError::Internal {
        message: message.to_owned(),
        details: json!({}),
    }
}

/// Pure validation of the identifiers the BFF presents.
#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::{lower_hex_256, random_hex_256, session_hash};

    /// Generated ids are exactly the shape the channel accepts, and anything
    /// else is refused: uppercase, short, or non-hex values never reach a
    /// store lookup.
    #[test]
    fn identifiers_must_be_256_bit_lowercase_hex() {
        let id = random_hex_256();
        assert!(lower_hex_256(&SecretString::from(id.clone()), "id").is_ok());
        for bad in [
            id.to_uppercase(),
            id[..63].to_owned(),
            format!("{}g", &id[..63]),
        ] {
            assert!(lower_hex_256(&SecretString::from(bad.clone()), "id").is_err());
            assert!(session_hash(&SecretString::from(bad)).is_err());
        }
        assert_ne!(random_hex_256(), id);
    }
}
