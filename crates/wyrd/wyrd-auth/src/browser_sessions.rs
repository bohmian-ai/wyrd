//! Rust-owned opaque browser sessions.
//!
//! A browser session is a random identifier carried only in a host-only
//! HttpOnly cookie as `{tenant}.{secret}`. The tenant prefix only routes the
//! lookup; the SHA-256 of the whole value under tenant RLS is the authority,
//! so a cookie cannot name another tenant's row. [`BrowserSessions`] creates
//! sessions from a recovery API key ([`BrowserSessions::recover`]) or a
//! completed provider sign-in (the common callback), resolves one on every
//! request ([`BrowserSessions::resolve`]) by re-checking the exact human
//! connection revision or the originating API key and re-resolving the
//! principal's current authority, and revokes one at logout
//! ([`BrowserSessions::logout`]). Sessions end twelve absolute hours after
//! creation and never slide. No API key, provider token, access token or
//! refresh token is ever stored or returned.

use std::time::Duration;

use base64::Engine as _;
use chrono::{DateTime, Utc};
use rand::RngCore as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use wyrd_auth_verify::VerifiedToken;
use wyrd_runtime::{Action, PermissionSet, Resource};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, BrowserCredential, BrowserSessionView, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::vala::api::AuditOutcome;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    BrowserSessionAuthority, LiveBrowserSession, human_connection_is_active,
    insert_browser_session, live_browser_session, revoke_browser_session,
};

use crate::audit::{LOGIN_OPERATION, principal_event};
use crate::callback::audit_authorization_code_failure;
use crate::connections::HumanConnections;
use crate::error::store_error;
use crate::exchange_api_key::{ExchangeApiKey, api_key_invalid, map_exchange_error_to_wyrd};
use crate::issuance::{IssuanceError, TenantGrant, TenantTokenIssuer};
use crate::issue_api_key::WyrdApiKey;

/// Absolute lifetime of a browser session; it never slides.
pub const BROWSER_SESSION_TTL: Duration = Duration::from_hours(12);

/// A browser session resolved for one request.
#[derive(Debug, Clone)]
pub struct BrowserSession {
    /// The principal's current authority, as a bearer token for the same
    /// principal and credential would verify.
    pub verified: VerifiedToken,
    /// How the session was created.
    pub credential: BrowserCredential,
    /// The session's absolute expiry.
    pub expires_at: DateTime<Utc>,
}

/// A session just created, with the cookie value returned exactly once.
pub struct CreatedBrowserSession {
    /// The cookie value `{tenant}.{secret}`; only its SHA-256 is stored.
    pub cookie: SecretString,
    /// The new session, resolved.
    pub session: BrowserSession,
}

impl std::fmt::Debug for CreatedBrowserSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedBrowserSession")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

/// The browser-session owner over the tenant issuer and the human-connection
/// owner's store.
#[derive(Clone)]
pub struct BrowserSessions {
    /// Resolves the principal's current authority on every request.
    issuer: TenantTokenIssuer,
    /// Begins provider logins and supplies the runtime store.
    connections: HumanConnections,
}

impl std::fmt::Debug for BrowserSessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserSessions").finish_non_exhaustive()
    }
}

impl BrowserSessions {
    /// Build the owner over the tenant issuer and human-connection owner.
    #[must_use]
    pub fn new(issuer: TenantTokenIssuer, connections: HumanConnections) -> Self {
        Self {
            issuer,
            connections,
        }
    }

    /// Create a session from a recovery API key presented for
    /// `tenant_route_key`.
    ///
    /// The key establishes its own tenant; a route key naming any other (or
    /// no) tenant is refused exactly as an invalid key is. The key is then
    /// verified and its use recorded through the API-key exchange's own
    /// authentication, which also resolves the principal's current authority
    /// and refuses an inactive tenant or principal. One tenant transaction
    /// stores only the new session's digest, bound to the key's credential
    /// id, and commits; one allowed `auth.login` event is then staged. A
    /// refusal for a known tenant stages its denied exchange event.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for every unusable key or
    /// mismatching route key, and [`WyrdError::AuthVerifyUnavailable`] when
    /// the store fails.
    pub async fn recover(
        &self,
        tenant_route_key: &TenantSlug,
        api_key: &SecretString,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let parsed = WyrdApiKey::parse(api_key.expose_secret()).map_err(|_| api_key_invalid())?;
        let result = self
            .recover_in(tenant_route_key, &parsed, api_key, request_id)
            .await;
        if let Err(error) = &result {
            audit_authorization_code_failure(
                self.issuer.audit(),
                parsed.tenant_id,
                request_id,
                error,
            );
        }
        result
    }

    /// The routed part of [`Self::recover`] once the key names its tenant.
    ///
    /// # Errors
    /// As [`Self::recover`].
    async fn recover_in(
        &self,
        tenant_route_key: &TenantSlug,
        parsed: &WyrdApiKey,
        api_key: &SecretString,
        request_id: &str,
    ) -> Result<CreatedBrowserSession, WyrdError> {
        let postgres = self.connections.postgres();
        let routed = postgres
            .resolve_tenant_slug(tenant_route_key)
            .await
            .map_err(store_error)?;
        if routed != Some(parsed.tenant_id) {
            tracing::info!("recovery key does not belong to the route's tenant");
            return Err(api_key_invalid());
        }
        let mut conn = postgres
            .tenant_conn(parsed.tenant_id)
            .await
            .map_err(store_error)?;
        let exchange = ExchangeApiKey {
            issuer: self.issuer.clone(),
        };
        let verified = match exchange.authenticate(&mut conn, api_key).await {
            Ok(verified) => verified,
            Err(error) => {
                return Err(map_exchange_error_to_wyrd(&mut conn, &parsed.prefix, error).await);
            }
        };
        let credential_id =
            verified
                .principal
                .credential_id
                .ok_or_else(|| WyrdError::Internal {
                    message: "an API-key principal carries no credential id".to_owned(),
                    details: json!({}),
                })?;
        let created = create_session(
            &mut conn,
            verified,
            BrowserSessionAuthority::ApiKey(credential_id),
        )
        .await?;
        conn.commit().await.map_err(store_error)?;
        self.stage_login(&created.session, request_id);
        Ok(created)
    }

    /// Resolve the session a cookie value names for one request.
    ///
    /// The value's tenant prefix opens that tenant's RLS transaction, where
    /// the value's SHA-256 must name an unexpired, unrevoked session. A human
    /// session's exact connection revision must still be the tenant's Active
    /// connection; a recovery session's API key must still be unrevoked,
    /// unexpired and its principal's. The principal's current authority is
    /// then resolved through the tenant issuer — with exactly the grant the
    /// session was created through, so a recovery session never widens into
    /// other authority — refusing an inactive tenant or principal. Nothing is
    /// written: resolution neither extends the session nor records activity.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for a malformed, unknown, expired,
    /// revoked, or other tenant's value and for a session whose connection or
    /// key is no longer valid; [`WyrdError::CredentialRevoked`] for an
    /// inactive principal or tenant; and [`WyrdError::AuthVerifyUnavailable`]
    /// when the store fails.
    pub async fn resolve(&self, cookie: &str) -> Result<BrowserSession, WyrdError> {
        let (tenant, hash) = cookie_key(cookie)?;
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let live = live_browser_session(&mut conn, &hash)
            .await
            .map_err(store_error)?
            .ok_or_else(session_invalid)?;
        let session = self.authorize(&mut conn, live).await?;
        conn.commit().await.map_err(store_error)?;
        Ok(session)
    }

    /// Revoke the session a cookie value names, immediately and only that
    /// session; the API key or connection it was created through is untouched.
    ///
    /// A malformed, unknown, or already revoked value revokes nothing and is
    /// not an error, so logout always clears the browser's cookie.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn logout(&self, cookie: &str) -> Result<(), WyrdError> {
        let Ok((tenant, hash)) = cookie_key(cookie) else {
            return Ok(());
        };
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        revoke_browser_session(&mut conn, &hash)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)
    }

    /// The safe metadata of a resolved session, with the tenant's route key
    /// and display name from the tenant directory.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the tenant is no longer in the
    /// directory, and [`WyrdError::AuthVerifyUnavailable`] when the read fails.
    pub async fn view(&self, session: &BrowserSession) -> Result<BrowserSessionView, WyrdError> {
        let principal = &session.verified.principal;
        let (tenant, tenant_name) = self
            .connections
            .postgres()
            .tenant_directory_entry(principal.tenant_id)
            .await
            .map_err(store_error)?
            .ok_or_else(session_invalid)?;
        Ok(BrowserSessionView {
            tenant,
            tenant_id: principal.tenant_id,
            tenant_name,
            principal_id: principal.id,
            principal_kind: principal.kind.tag(),
            credential: session.credential,
            permissions: permission_names(&principal.effective_permissions),
            expires_at: session.expires_at,
        })
    }

    /// Re-check a live session's authority and resolve its principal.
    ///
    /// # Errors
    /// As [`Self::resolve`] after the lookup.
    async fn authorize(
        &self,
        conn: &mut TenantConn<'_>,
        live: LiveBrowserSession,
    ) -> Result<BrowserSession, WyrdError> {
        let (grant, credential) = match live.authority {
            BrowserSessionAuthority::Connection(binding) => {
                if !human_connection_is_active(
                    conn,
                    binding.connection_id,
                    binding.connection_revision,
                )
                .await
                .map_err(store_error)?
                {
                    return Err(IssuanceError::ConnectionInactive.into());
                }
                (TenantGrant::OidcLogin, BrowserCredential::Sso)
            }
            BrowserSessionAuthority::ApiKey(credential_id) => (
                TenantGrant::BrowserRecovery { credential_id },
                BrowserCredential::ApiKey,
            ),
        };
        let verified = self.issuer.verify(conn, live.principal_id, grant).await?;
        Ok(BrowserSession {
            verified,
            credential,
            expires_at: live.expires_at,
        })
    }

    /// Stage the allowed `auth.login` event for a session just created.
    fn stage_login(&self, session: &BrowserSession, request_id: &str) {
        let principal = &session.verified.principal;
        self.issuer.audit().stage(
            principal.tenant_id,
            principal_event(
                request_id,
                LOGIN_OPERATION,
                Attribution::direct(principal.id),
                principal.kind.tag(),
                None,
                AuditOutcome::Allowed,
            ),
        );
    }
}

/// Store a new session for `verified`'s principal created through
/// `authority` on `conn`, and return it with its cookie value.
///
/// The caller commits `conn`; nothing is visible until it does.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when the insert fails.
pub(crate) async fn create_session(
    conn: &mut TenantConn<'_>,
    verified: VerifiedToken,
    authority: BrowserSessionAuthority,
) -> Result<CreatedBrowserSession, WyrdError> {
    let tenant = conn.data_tenant_id();
    let cookie = SecretString::from(format!("{tenant}.{}", random_secret()));
    let expires_at = insert_browser_session(
        conn,
        &Sha256Hex::digest(cookie.expose_secret().as_bytes()),
        verified.principal.id.as_uuid(),
        authority,
        BROWSER_SESSION_TTL,
    )
    .await
    .map_err(store_error)?;
    let credential = match authority {
        BrowserSessionAuthority::Connection(_) => BrowserCredential::Sso,
        BrowserSessionAuthority::ApiKey(_) => BrowserCredential::ApiKey,
    };
    Ok(CreatedBrowserSession {
        cookie,
        session: BrowserSession {
            verified,
            credential,
            expires_at,
        },
    })
}

/// The tenant a cookie value routes to and the digest that names its row.
///
/// # Errors
/// Returns the invalid-session refusal for a value with no tenant prefix.
fn cookie_key(cookie: &str) -> Result<(DataTenantId, Sha256Hex), WyrdError> {
    let tenant = cookie
        .split_once('.')
        .and_then(|(tenant, _)| tenant.parse::<DataTenantId>().ok())
        .ok_or_else(session_invalid)?;
    Ok((tenant, Sha256Hex::digest(cookie.as_bytes())))
}

/// The `resource:action` names of a permission set, with composite resources
/// and actions expanded to their members.
fn permission_names(permissions: &PermissionSet) -> Vec<String> {
    /// The single resources a resource names.
    fn resources(resource: &Resource) -> Vec<&'static str> {
        match resource {
            Resource::AnyOf(members) => members.iter().flat_map(resources).collect(),
            single => single.as_str().into_iter().collect(),
        }
    }
    /// The single actions an action names.
    fn actions(action: &Action) -> Vec<&'static str> {
        match action {
            Action::AnyOf(members) => members.iter().flat_map(actions).collect(),
            single => single.as_str().into_iter().collect(),
        }
    }
    let mut names = permissions
        .iter()
        .flat_map(|permission| {
            let actions = actions(&permission.action);
            resources(&permission.resource)
                .into_iter()
                .flat_map(move |resource| {
                    actions
                        .clone()
                        .into_iter()
                        .map(move |action| format!("{resource}:{action}"))
                })
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    names
}

/// A fresh unguessable secret: 32 random bytes, base64url without padding.
fn random_secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The one refusal for a session that does not resolve, so a malformed,
/// unknown, expired, revoked, and other tenant's value are indistinguishable.
fn session_invalid() -> WyrdError {
    WyrdError::InvalidToken {
        message: "the browser session is missing, expired, or revoked; sign in again".to_owned(),
        details: json!({ "reason": "browser_session_invalid" }),
    }
}

#[cfg(test)]
mod tests {
    use wyrd_runtime::{Permission, PermissionSet};

    use super::{cookie_key, permission_names};

    /// Composite resources and actions expand to sorted, deduplicated
    /// `resource:action` names.
    ///
    /// # Panics
    /// Panics when the projection differs.
    #[test]
    fn permission_names_expand_composites() {
        let mut set = PermissionSet::new();
        for permission in ["cards:read", "cards:write", "users:read"] {
            set.insert(permission.parse::<Permission>().expect("permission parses"));
        }
        assert_eq!(
            permission_names(&set),
            ["cards:read", "cards:write", "users:read"]
        );
    }

    /// A value without a tenant prefix names no row.
    ///
    /// # Panics
    /// Panics when a malformed value is accepted.
    #[test]
    fn cookie_without_a_tenant_prefix_is_refused() {
        for value in ["", "no-dot", "not-a-uuid.secret"] {
            let error = cookie_key(value).expect_err("malformed value is refused");
            assert_eq!(error.code(), "WYRD_AUTH_401_INVALID_TOKEN");
        }
    }
}
