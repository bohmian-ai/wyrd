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
use crate::exchange_api_key::{
    ExchangeApiKey, api_key_invalid, map_exchange_error_to_wyrd, token_hash,
};
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
    /// rotated credential as fresh, so one renewal serves both.
    ///
    /// A renewal that does not issue keeps its owner's transaction meaning
    /// ([`Renewal`]), decided against the stored token's expiry under
    /// `PostgreSQL`'s clock:
    ///
    /// - An ordinary refusal — an unknown or revoked refresh token, a
    ///   deactivated or replaced connection, a revoked API key, a suspended
    ///   principal — while the token is still valid inside the renewal margin
    ///   rolls the attempt back (no tentative refresh, key-use, or revocation
    ///   write commits).
    /// - A replayed refresh token while the token is still valid commits the
    ///   attempt, so the family revocation and containment audit
    ///   [`RefreshTokens`] staged survive; the session row is left live.
    /// - In both cases the row is then locked again and the already-issued
    ///   token is served unchanged until its stored expiry; the relock happens
    ///   at most once per call. Once the token has expired, either outcome
    ///   revokes the session in the same transaction and commits; no other
    ///   credential is tried.
    /// - An internal failure returns the error before or after expiry; the
    ///   dropped transaction rolls back every tentative write, the old token
    ///   is not served, and the session stays renewable on retry.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for an unknown, revoked, expired, or
    /// no-longer-renewable session, [`WyrdError::Validation`] without a
    /// keyring, [`WyrdError::AuthVerifyUnavailable`] when the store fails, and
    /// the internal renewal failure (for example [`WyrdError::RoleCorrupt`] or
    /// [`WyrdError::Internal`]) otherwise.
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
        let mut renewal_refused = false;
        loop {
            let mut conn = self.begin(tenant).await?;
            let row = lock_browser_session(&mut conn, &id_hash, RENEW_MARGIN)
                .await
                .map_err(store_error)?
                .ok_or_else(session_ended)?;
            let keyring = self.require_keyring()?;
            if row.access_fresh || (renewal_refused && !row.access_expired) {
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
                Err(outcome @ (Renewal::Refused | Renewal::Contained))
                    if !row.access_expired && !renewal_refused =>
                {
                    if matches!(outcome, Renewal::Contained) {
                        conn.commit().await.map_err(store_error)?;
                    } else {
                        conn.rollback().await.map_err(store_error)?;
                    }
                    renewal_refused = true;
                    tracing::info!(
                        tenant_id = %tenant,
                        "browser session early renewal refused; serving the issued token until expiry"
                    );
                    continue;
                }
                Err(Renewal::Refused | Renewal::Contained) => {
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
            return Ok((
                tenant,
                CurrentSession {
                    conn,
                    row,
                    access_token: renewed.access_token,
                    access_expires_at: renewed.expires_at,
                },
            ));
        }
    }

    /// Mint a successor access token for a locked session through the
    /// ordinary issuance path for its mode.
    ///
    /// The stored credential is opened under the keyring and handed to the
    /// mode's owner — [`RefreshTokens`] or [`ExchangeApiKey`] — whose error
    /// classification ([`RefreshError::is_refusal`],
    /// [`crate::exchange_api_key::ExchangeError::is_refusal`]) decides the
    /// outcome. Nothing is committed here.
    ///
    /// # Errors
    /// Returns [`Renewal::Refused`] when the stored credential no longer
    /// issues, [`Renewal::Contained`] when the stored refresh token was
    /// replayed and its family revocation and audit are staged in `conn`,
    /// and [`Renewal::Failed`] when the stored credential is missing or
    /// cannot be opened, or for a store, audit, signing, corrupt-role, or
    /// verification-task failure.
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
                let refresh = open_credential(keyring, row.refresh_token_sealed.as_deref())?;
                RefreshTokens { issuer }
                    .execute(conn, refresh, request_id)
                    .await
                    .map_err(|error| match error {
                        RefreshError::Reused => Renewal::Contained,
                        error if error.is_refusal() => Renewal::Refused,
                        error => Renewal::Failed(WyrdError::from(error)),
                    })
            }
            BrowserSessionMode::ApiKeyExchange => {
                let api_key = open_credential(keyring, row.api_key_sealed.as_deref())?;
                let exchanged = ExchangeApiKey { issuer }
                    .execute(conn, api_key, request_id)
                    .await;
                match exchanged {
                    Ok(exchanged) => Ok(exchanged),
                    Err(error) if error.is_refusal() => Err(Renewal::Refused),
                    // A failure never reaches the prefix-keyed refusal
                    // logging, so no prefix is needed to render it.
                    Err(error) => Err(Renewal::Failed(
                        map_exchange_error_to_wyrd(conn, "", error).await,
                    )),
                }
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

/// Why a renewal did not produce a token, kept distinct because each
/// outcome has its own transaction meaning in [`BrowserSessions::current`].
enum Renewal {
    /// The stored credential no longer issues; the attempt rolls back and the
    /// session ends at its access token's expiry.
    Refused,
    /// The stored refresh token was replayed: its family revocation and
    /// containment audit are staged in the attempt's transaction, which must
    /// commit; the session ends at its access token's expiry.
    Contained,
    /// An internal failure; the attempt rolls back and the session is left
    /// renewable.
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
/// Only opens; the session lifecycle consequence of a failure belongs to the
/// caller. A stored access token that cannot be opened is reported as an
/// ended session, while renewal maps the same failure to a retryable internal
/// failure through `open_credential`.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] when no held key opens the envelope or
/// its plaintext is not UTF-8.
fn open_text(keyring: &SealingKeyring, sealed: &[u8]) -> Result<SecretString, WyrdError> {
    let opened = keyring.open(sealed).map_err(|_| session_ended())?;
    String::from_utf8(opened)
        .map(SecretString::from)
        .map_err(|_| session_ended())
}

/// Open the sealed credential a session renews through.
///
/// # Errors
/// Returns [`Renewal::Failed`] with [`WyrdError::Internal`] when the session
/// stores no credential for its mode or no held key opens it: both are
/// corrupt state the renewal must not turn into the end of the session.
fn open_credential(
    keyring: &SealingKeyring,
    sealed: Option<&[u8]>,
) -> Result<SecretString, Renewal> {
    sealed
        .and_then(|sealed| open_text(keyring, sealed).ok())
        .ok_or_else(|| Renewal::Failed(unusable("browser session credential could not be opened")))
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

    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_spec::error::WyrdError;

    use super::{Renewal, lower_hex_256, open_credential, random_hex_256, seal, session_hash};
    use crate::exchange_api_key::ExchangeError;
    use crate::issuance::IssuanceError;
    use crate::refresh::RefreshError;

    /// A renewal credential the session cannot produce is a retryable
    /// internal failure, never a refusal that ends the session.
    ///
    /// Both a mode with no stored envelope and an envelope sealed under a key
    /// the active keyring does not hold must yield
    /// `Renewal::Failed(WyrdError::Internal)`, so a repaired keyring or
    /// envelope lets the same session renew.
    #[test]
    fn missing_or_unopenable_renewal_credential_is_retryable_failure() {
        let held = SealingKeyring::new(SecretKey::from_bytes([1_u8; 32]));
        let unheld = SealingKeyring::new(SecretKey::from_bytes([2_u8; 32]));
        let foreign = seal(&unheld, "credential").expect("test credential seals");
        assert!(open_credential(&unheld, Some(&foreign)).is_ok());
        for (case, sealed) in [("missing", None), ("unopenable", Some(foreign.as_slice()))] {
            assert!(
                matches!(
                    open_credential(&held, sealed),
                    Err(Renewal::Failed(WyrdError::Internal { .. }))
                ),
                "a {case} renewal credential must be a retryable internal failure"
            );
        }
    }

    /// Renewal ends a session only on an ordinary lifecycle refusal.
    ///
    /// Every producer's classification must agree: an inactive tenant,
    /// principal, or connection and a credential the producer rejects
    /// (unknown, cross-tenant, or wrong-secret) are refusals wherever they
    /// surface; a replay is not a refusal (its containment commits); store,
    /// corrupt-role, and signing failures are internal. A stored envelope that
    /// cannot be opened never reaches a producer and is covered by
    /// `missing_or_unopenable_renewal_credential_is_retryable_failure`.
    #[test]
    fn only_lifecycle_refusals_end_a_renewing_session() {
        for refusal in [
            IssuanceError::TenantNotAdmitting,
            IssuanceError::PrincipalInactive,
            IssuanceError::ConnectionInactive,
        ] {
            assert!(refusal.is_refusal(), "{refusal:?}");
        }
        assert!(RefreshError::NotFound.is_refusal());
        assert!(RefreshError::Issuance(IssuanceError::ConnectionInactive).is_refusal());
        assert!(ExchangeError::from(IssuanceError::PrincipalInactive).is_refusal());
        assert!(ExchangeError::from(IssuanceError::ConnectionInactive).is_refusal());
        for refusal in [
            ExchangeError::CrossTenant,
            ExchangeError::NotFound,
            ExchangeError::HashMismatch,
        ] {
            assert!(refusal.is_refusal(), "{refusal:?}");
        }

        assert!(!RefreshError::Reused.is_refusal());
        assert!(!RefreshError::Database(sqlx::Error::PoolTimedOut).is_refusal());
        assert!(
            !RefreshError::Issuance(IssuanceError::RoleCorrupt {
                role: "r".to_owned()
            })
            .is_refusal()
        );
        assert!(!ExchangeError::Database(sqlx::Error::PoolTimedOut).is_refusal());
        assert!(
            !ExchangeError::from(IssuanceError::RoleCorrupt {
                role: "r".to_owned()
            })
            .is_refusal()
        );
    }

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

/// Postgres-backed browser-session lifecycle behavior.
#[cfg(test)]
mod pg_tests {
    use chrono::{DateTime, Utc};
    use secrecy::{ExposeSecret, SecretString};
    use serde_json::json;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_spec::auth::Sha256Hex;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::TenantSlug;
    use wyrd_sql::queries::auth::{
        BrowserSessionMode, BrowserSessionWrite, grant_role_to_service_account,
        insert_browser_session, insert_role, replace_user_roles, revoke_api_key,
    };

    use super::{
        BrowserSessions, SSO_SESSION_LIFETIME, random_hex_256, refresh_expiry, seal, token_hash,
    };
    use crate::audit::REFRESH_FAMILY_REVOKE_OPERATION;
    use crate::exchange_api_key::pg_tests::{
        browser_sessions, insert_live_api_key, insert_test_service_account, insert_test_user,
        test_service_card_ref,
    };
    use crate::refresh::{RefreshError, RefreshTokens};

    /// The stored lifecycle columns a renewal that commits nothing must leave
    /// as they were: sealed access token, its expiry, the session's
    /// revocation, and the backing credential's tentative write.
    type StoredState = (
        Option<Vec<u8>>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
    );

    /// The credential a session renews through, naming the column its
    /// renewal tentatively writes.
    enum Backing<'a> {
        /// An API-key session: renewal stamps the key's `last_used_at`.
        ApiKey(Uuid),
        /// An OIDC session: renewal retires the refresh row with this token
        /// hash by setting its `revoked_at`.
        Refresh(&'a str),
    }

    /// Read the one stored session's lifecycle columns and its backing
    /// credential's tentative-write column as the superuser, outside tenant
    /// RLS.
    ///
    /// # Panics
    /// Panics when the superuser pool cannot open or the read fails.
    async fn stored_state(fixture: &PgFixture, backing: &Backing<'_>) -> StoredState {
        let pool = fixture
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        let query = match backing {
            Backing::ApiKey(api_key_id) => sqlx::query_as(
                "SELECT s.access_token_sealed, s.access_expires_at, s.revoked_at, k.last_used_at
                   FROM wyrd.auth_browser_sessions s, wyrd.auth_api_keys k
                  WHERE k.id = $1",
            )
            .bind(*api_key_id),
            Backing::Refresh(hash) => sqlx::query_as(
                "SELECT s.access_token_sealed, s.access_expires_at, s.revoked_at, r.revoked_at
                   FROM wyrd.auth_browser_sessions s, wyrd.auth_refresh_tokens r
                  WHERE r.token_hash = $1",
            )
            .bind(*hash),
        };
        query
            .fetch_one(&pool)
            .await
            .expect("stored session state reads")
    }

    /// A seeded OIDC browser session and the values its tests compare.
    struct OidcSession {
        /// The raw session id the BFF cookie carries.
        session_id: SecretString,
        /// The access token the session stores.
        access_token: SecretString,
        /// The refresh token the session stores.
        refresh_token: SecretString,
        /// The signed-in user.
        user_id: Uuid,
        /// The one role the user holds, so a test can corrupt it.
        role_id: Uuid,
    }

    /// Sign a user in the way a completed SSO login does and store the
    /// browser session `complete` would.
    ///
    /// Seeds an active user holding one empty role and the tenant's Active
    /// human connection, mints the first access/refresh pair through the
    /// session owner's issuer bound to that connection, and stores both
    /// sealed under the owner's keyring in one committed transaction.
    ///
    /// # Panics
    /// Panics when a seed, issuance, seal, or the commit fails.
    async fn oidc_session(fixture: &PgFixture, sessions: &BrowserSessions) -> OidcSession {
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let role_id = Uuid::new_v4();
        insert_role(&mut conn, role_id, "runtime_admin", &json!([]), false)
            .await
            .expect("role seeds");
        replace_user_roles(&mut conn, user_id, &["runtime_admin"])
            .await
            .expect("role assigns");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        let issued = sessions
            .issuer
            .issue_human_session(&mut conn, user_id, None, binding, "req-login")
            .await
            .expect("login issues a pair");
        let refresh_token = issued
            .refresh_token
            .expect("a login issues a refresh token");
        let keyring = sessions.require_keyring().expect("test keyring");
        let csrf = random_hex_256();
        let session_id = random_hex_256();
        let write = BrowserSessionWrite {
            principal_id: user_id,
            connection_id: Some(binding.connection_id),
            mode: BrowserSessionMode::OidcRefresh,
            access_token_sealed: seal(keyring, issued.access_token.expose_secret())
                .expect("access token seals"),
            access_expires_at: issued.expires_at,
            refresh_token_sealed: Some(
                seal(keyring, refresh_token.expose_secret()).expect("refresh token seals"),
            ),
            refresh_expires_at: Some(
                refresh_expiry(refresh_token.expose_secret()).expect("refresh expiry reads"),
            ),
            api_key_sealed: None,
            csrf_hash: Sha256Hex::digest(csrf.as_bytes()),
            csrf_token_sealed: seal(keyring, &csrf).expect("csrf seals"),
            lifetime: SSO_SESSION_LIFETIME,
        };
        insert_browser_session(&mut conn, &Sha256Hex::digest(session_id.as_bytes()), &write)
            .await
            .expect("session stores")
            .expect("session id is fresh");
        conn.commit().await.expect("sign-in commits");
        OidcSession {
            session_id: SecretString::from(session_id),
            access_token: issued.access_token,
            refresh_token,
            user_id,
            role_id,
        }
    }

    /// Replace role `role_id`'s stored permission document as the superuser,
    /// outside tenant RLS; a document the permission schema rejects makes
    /// every issuance for its holders fail as corrupt state.
    ///
    /// # Panics
    /// Panics when the superuser pool cannot open or the update fails.
    async fn set_role_permissions(
        fixture: &PgFixture,
        role_id: Uuid,
        permissions: serde_json::Value,
    ) {
        let updated = sqlx::query("UPDATE wyrd.auth_roles SET permissions = $2 WHERE id = $1")
            .bind(role_id)
            .bind(permissions)
            .execute(
                &fixture
                    .superuser_pool()
                    .await
                    .expect("superuser pool opens"),
            )
            .await
            .expect("role permissions update");
        assert_eq!(updated.rows_affected(), 1);
    }

    /// Drive one session through an injected internal renewal failure and
    /// prove it stays retryable.
    ///
    /// Corrupts role `role_id`, then uses the session with its token inside
    /// the renewal margin and again after its expiry: both uses must fail
    /// with the corrupt-role error, not serve the stored token and not end
    /// the session, and must leave the session row and the backing
    /// credential's tentative-write column exactly as stored. Once the role
    /// is repaired the next use renews to a new token.
    ///
    /// # Panics
    /// Panics when a seed or update fails or any assertion fails.
    async fn assert_internal_failure_is_retryable(
        fixture: &PgFixture,
        sessions: &BrowserSessions,
        session_id: &SecretString,
        role_id: Uuid,
        backing: &Backing<'_>,
        issued: &SecretString,
    ) {
        set_role_permissions(fixture, role_id, json!([{ "resource": "not-a-resource" }])).await;
        for (offset, phase) in [("30 seconds", "early"), ("-1 second", "expired")] {
            set_access_expiry(fixture, offset).await;
            let before = stored_state(fixture, backing).await;
            let failed = sessions
                .authority(session_id, &format!("req-{phase}"))
                .await
                .expect_err("an internal renewal failure fails the request");
            assert!(
                matches!(failed, WyrdError::RoleCorrupt { .. }),
                "{phase}: the failure is reported, not turned into a refusal: {failed:?}"
            );
            assert_eq!(
                stored_state(fixture, backing).await,
                before,
                "{phase}: no tentative credential or session write commits"
            );
        }
        set_role_permissions(fixture, role_id, json!([])).await;
        let renewed = sessions
            .authority(session_id, "req-repaired")
            .await
            .expect("the session renews once the failure is removed");
        assert_ne!(
            renewed.access_token.expose_secret(),
            issued.expose_secret(),
            "the repaired renewal issues a successor"
        );
    }

    /// Move the one stored session's access-token expiry to `offset` from
    /// `PostgreSQL`'s clock.
    ///
    /// # Panics
    /// Panics when the superuser pool cannot open or the update fails.
    async fn set_access_expiry(fixture: &PgFixture, offset: &str) {
        let updated = sqlx::query(
            "UPDATE wyrd.auth_browser_sessions \
             SET access_expires_at = statement_timestamp() + $1::interval",
        )
        .bind(offset)
        .execute(
            &fixture
                .superuser_pool()
                .await
                .expect("superuser pool opens"),
        )
        .await
        .expect("access expiry updates");
        assert_eq!(updated.rows_affected(), 1);
    }

    /// A refused proactive renewal keeps the issued token until its stored
    /// expiry and ends the session only on the first use after it.
    ///
    /// An API-key session is signed in, then its API key is revoked so
    /// renewal is disallowed. With the stored token inside the renewal margin
    /// but unexpired, `authority` must return that same token and leave the
    /// sealed token, its expiry, the session's revocation, and the key's last
    /// use exactly as stored. Once the token's expiry has passed, the next use must refuse
    /// with the session-ended error and revoke the row.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, a seed or update fails, or any
    /// assertion fails.
    #[tokio::test]
    async fn proactive_renewal_refusal_preserves_authority_until_expiry() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let route = TenantSlug::new(fixture.tenant_slug().to_owned()).expect("fixture slug");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id =
            insert_test_service_account(&mut conn, tenant, user_id, &test_service_card_ref()).await;
        let (api_key_id, api_key) = insert_live_api_key(&mut conn, tenant, sa_id, user_id).await;
        conn.commit().await.expect("seed commits");

        let sessions = browser_sessions(&fixture);
        let created = sessions
            .exchange_api_key(
                &route,
                &api_key,
                &SecretString::from("ab".repeat(32)),
                "req-sign-in",
            )
            .await
            .expect("the live key signs in");
        let issued = sessions
            .authority(&created.session_id, "req-fresh")
            .await
            .expect("a fresh session yields its token");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        assert!(
            revoke_api_key(&mut conn, api_key_id)
                .await
                .expect("api key revokes")
        );
        conn.commit().await.expect("key revocation commits");
        set_access_expiry(&fixture, "30 seconds").await;
        let backing = Backing::ApiKey(api_key_id);
        let before = stored_state(&fixture, &backing).await;

        let early = sessions
            .authority(&created.session_id, "req-early")
            .await
            .expect("a refused early renewal still serves the unexpired token");
        assert_eq!(
            early.access_token.expose_secret(),
            issued.access_token.expose_secret(),
            "the already-issued token is served, not a successor"
        );
        assert_eq!(early.access_expires_at, before.1);
        sessions
            .read(&created.session_id, "req-early-read")
            .await
            .expect("the session still reads before its token expires");
        assert_eq!(
            stored_state(&fixture, &backing).await,
            before,
            "no renewal, key-use, or revocation write commits before expiry"
        );

        set_access_expiry(&fixture, "-1 second").await;
        let ended = sessions
            .authority(&created.session_id, "req-expired")
            .await
            .expect_err("the first use after expiry is refused");
        assert!(matches!(ended, WyrdError::InvalidToken { .. }), "{ended:?}");
        let (sealed, _, revoked_at, _) = stored_state(&fixture, &backing).await;
        assert!(
            sealed.is_none() && revoked_at.is_some(),
            "the refused post-expiry renewal revokes the session"
        );
    }
    /// A proactive renewal that presents an already-rotated refresh token
    /// commits the family containment and keeps the issued token until its
    /// stored expiry.
    ///
    /// An OIDC session is signed in, then an attacker holding its refresh
    /// token rotates it first and commits. With the stored access token inside
    /// the renewal margin, `authority` must still serve that same token while
    /// the replay's family revocation and canonical containment audit commit:
    /// the attacker's successor is revoked as reused in committed state and
    /// cannot itself rotate. The session row stays live until the token's
    /// expiry; the first use after it is refused and revokes the row.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, a seed or update fails, or any
    /// assertion fails.
    #[tokio::test]
    async fn proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let sessions = browser_sessions(&fixture);
        let session = oidc_session(&fixture, &sessions).await;
        let refresh = RefreshTokens {
            issuer: sessions.issuer.clone(),
        };

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let stolen = refresh
            .execute(&mut conn, session.refresh_token.clone(), "req-attacker")
            .await
            .expect("the attacker's rotation succeeds")
            .refresh_token
            .expect("rotation issues a successor");
        conn.commit()
            .await
            .expect("the attacker's rotation commits");
        let stolen_hash = token_hash(stolen.expose_secret());
        let backing = Backing::Refresh(&stolen_hash);

        set_access_expiry(&fixture, "30 seconds").await;
        let early = sessions
            .authority(&session.session_id, "req-early")
            .await
            .expect("a contained replay still serves the unexpired token");
        assert_eq!(
            early.access_token.expose_secret(),
            session.access_token.expose_secret(),
            "the already-issued token is served, not a successor"
        );
        let (sealed, _, session_revoked, successor_revoked) =
            stored_state(&fixture, &backing).await;
        assert!(
            sealed.is_some() && session_revoked.is_none(),
            "the browser session stays live until its token expires"
        );
        assert!(
            successor_revoked.is_some(),
            "the attacker's successor is revoked in committed state"
        );
        let contained: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND operation = $2 AND principal_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(REFRESH_FAMILY_REVOKE_OPERATION)
        .bind(session.user_id)
        .fetch_one(
            &fixture
                .superuser_pool()
                .await
                .expect("superuser pool opens"),
        )
        .await
        .expect("audit query runs");
        assert!(contained >= 1, "the containment audit commits");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let replayed = refresh.execute(&mut conn, stolen, "req-successor").await;
        assert!(
            matches!(replayed, Err(RefreshError::Reused)),
            "the attacker's successor cannot rotate: {replayed:?}"
        );
        drop(conn);

        set_access_expiry(&fixture, "-1 second").await;
        let ended = sessions
            .authority(&session.session_id, "req-expired")
            .await
            .expect_err("the first use after expiry is refused");
        assert!(matches!(ended, WyrdError::InvalidToken { .. }), "{ended:?}");
        let (sealed, _, session_revoked, _) = stored_state(&fixture, &backing).await;
        assert!(
            sealed.is_none() && session_revoked.is_some(),
            "the contained post-expiry renewal revokes the session"
        );
    }

    /// An internal failure while renewing an OIDC session fails the request,
    /// commits nothing, and leaves the session renewable.
    ///
    /// The user's role is corrupted so refresh issuance fails after the
    /// stored refresh row was tentatively consumed; both before and after the
    /// access token's expiry the use must fail without serving the token,
    /// retiring the refresh row, or ending the session, and the repaired role
    /// renews it.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, a seed or update fails, or any
    /// assertion fails.
    #[tokio::test]
    async fn refresh_renewal_internal_failure_rolls_back_and_remains_retryable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let sessions = browser_sessions(&fixture);
        let session = oidc_session(&fixture, &sessions).await;
        let refresh_hash = token_hash(session.refresh_token.expose_secret());
        assert_internal_failure_is_retryable(
            &fixture,
            &sessions,
            &session.session_id,
            session.role_id,
            &Backing::Refresh(&refresh_hash),
            &session.access_token,
        )
        .await;
    }

    /// An internal failure while renewing an API-key session fails the
    /// request, commits nothing, and leaves the session renewable.
    ///
    /// The service account's role is corrupted so issuance fails after the
    /// key's use was tentatively stamped; both before and after the access
    /// token's expiry the use must fail without serving the token, stamping
    /// the key, or ending the session, and the repaired role renews it.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, a seed or update fails, or any
    /// assertion fails.
    #[tokio::test]
    async fn api_key_renewal_internal_failure_rolls_back_and_remains_retryable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let route = TenantSlug::new(fixture.tenant_slug().to_owned()).expect("fixture slug");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id =
            insert_test_service_account(&mut conn, tenant, user_id, &test_service_card_ref()).await;
        let role_id = Uuid::new_v4();
        insert_role(&mut conn, role_id, "runtime_reader", &json!([]), false)
            .await
            .expect("role seeds");
        assert!(
            grant_role_to_service_account(&mut conn, sa_id, role_id)
                .await
                .expect("role grants")
        );
        let (api_key_id, api_key) = insert_live_api_key(&mut conn, tenant, sa_id, user_id).await;
        conn.commit().await.expect("seed commits");

        let sessions = browser_sessions(&fixture);
        let created = sessions
            .exchange_api_key(
                &route,
                &api_key,
                &SecretString::from("ab".repeat(32)),
                "req-sign-in",
            )
            .await
            .expect("the live key signs in");
        let issued = sessions
            .authority(&created.session_id, "req-fresh")
            .await
            .expect("a fresh session yields its token");
        assert_internal_failure_is_retryable(
            &fixture,
            &sessions,
            &created.session_id,
            role_id,
            &Backing::ApiKey(api_key_id),
            &issued.access_token,
        )
        .await;
    }
}
