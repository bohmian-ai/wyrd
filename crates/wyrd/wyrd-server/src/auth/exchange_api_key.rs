//! API-key exchange and RFC 8693 delegation services.

use std::sync::Arc;

use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_auth_issue::{DelegationCaller, IssueError, IssuingKey};
use wyrd_auth_verify::{ActClaim, AuthError, PrincipalKindTag, TokenPrincipalRef, TokenVerifier};
use wyrd_runtime::{Permission, PermissionCheck, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::auth::{RequestedSubject, SecretBearer, TokenResponse, TokenType};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    ApiKeyStatus, api_key_by_prefix, api_key_status_by_prefix, insert_audit_token_exchange,
    insert_refresh_token, list_service_account_roles, service_account_by_id,
    touch_api_key_last_used,
};

use crate::auth::card_scope::resolve_card_scope;
use crate::auth::issue_api_key::{WyrdApiKey, principal_kind_for_card};
use crate::auth::permission_resolver::SqlPermissionResolver;
use crate::auth::pg_resolvers::PgIssuerResolver;

/// Token exchange settings.
#[derive(Debug, Clone)]
pub struct TokenExchangeSettings {
    /// Access token lifetime.
    pub access_ttl: Duration,
    /// Refresh token lifetime.
    pub refresh_ttl: Duration,
}

impl Default for TokenExchangeSettings {
    fn default() -> Self {
        Self {
            access_ttl: Duration::minutes(15),
            refresh_ttl: Duration::days(30),
        }
    }
}

/// API-key exchange service.
#[derive(Clone)]
pub struct ExchangeApiKey {
    /// JWT issuing key.
    pub issuing_key: Arc<IssuingKey>,
    /// Settings.
    pub settings: TokenExchangeSettings,
}

impl std::fmt::Debug for ExchangeApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExchangeApiKey")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

/// Delegated token service.
#[derive(Clone)]
pub struct DelegateToken {
    /// JWT issuing key.
    pub issuing_key: Arc<IssuingKey>,
    /// JWT verifier.
    pub verifier: Arc<TokenVerifier<SqlPermissionResolver, PgIssuerResolver>>,
    /// Permission checker.
    pub permission_check: Arc<dyn PermissionCheck>,
    /// Settings.
    pub settings: TokenExchangeSettings,
}

impl std::fmt::Debug for DelegateToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateToken")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

/// Internal exchanged-token shape.
#[derive(Debug, Clone)]
pub struct ExchangedToken {
    /// Access token.
    pub access_token: SecretString,
    /// Refresh token. `None` for grants that do not issue one (workload
    /// `jwt-bearer` and `token-exchange` delegation), whose callers hold a
    /// durable credential they can re-present for a fresh access token.
    pub refresh_token: Option<SecretString>,
    /// Token type.
    pub token_type: TokenType,
    /// Access token expiry.
    pub expires_at: chrono::DateTime<Utc>,
}

impl ExchangedToken {
    /// Convert to public token response.
    #[must_use]
    pub fn into_response(self) -> TokenResponse {
        TokenResponse {
            access_token: SecretBearer::new(self.access_token.expose_secret().to_owned()),
            refresh_token: self
                .refresh_token
                .map(|token| SecretBearer::new(token.expose_secret().to_owned())),
            token_type: self.token_type,
            expires_at: self.expires_at,
        }
    }
}

/// Whether a token-issuing path mints a refresh token alongside the access
/// token.
///
/// Refresh tokens exist to spare a credential holder from re-proving identity.
/// A human OIDC session and an API key benefit from that. A workload with a
/// platform-attested assertion, or a short-lived delegated principal, do not:
/// they can re-present their durable credential, so issuing a long-lived
/// refresh secret only widens the leak surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RefreshPolicy {
    /// Issue and persist a refresh token (API-key exchange, human login).
    Mint,
    /// Access token only; no refresh token is issued or stored.
    Skip,
}

/// The service/agent principal a token is being issued for.
pub(super) struct IssueSubject {
    /// Stable principal id.
    pub principal_id: Uuid,
    /// Principal kind: `"service"` or `"agent"`.
    pub principal_kind: String,
    /// Principal card reference embedded in the access token.
    pub card_ref: CardRef,
    /// Effective roles embedded in the access token.
    pub roles: Vec<RoleRef>,
}

/// API-key exchange failure.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    /// Key format is invalid or the key's embedded tenant does not match the connection tenant.
    #[error("api key tenant mismatch")]
    CrossTenant,
    /// No unexpired, unrevoked key matching this prefix exists.
    #[error("api key not found")]
    NotFound,
    /// Key exists but the associated service account is not active.
    #[error("service account is not active")]
    AccountDisabled,
    /// Key exists and account is active but Argon2 hash verification failed.
    #[error("api key hash mismatch")]
    HashMismatch,
    /// JWT issue failure.
    #[error("token issue failed")]
    Issue(#[from] IssueError),
    /// Blocking task failed.
    #[error("api key verify task failed")]
    Join(#[from] tokio::task::JoinError),
    /// Role name from SQL was invalid.
    #[error("role name is invalid")]
    InvalidRole,
    /// Card-scope resolution failed; fail closed rather than mint an unknown scope.
    #[error("card scope resolution failed")]
    CardScope(WyrdError),
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

/// Delegated token failure.
#[derive(Debug, thiserror::Error)]
pub enum DelegateError {
    /// Subject token failed verification.
    #[error("subject token invalid")]
    InvalidSubjectToken(#[from] AuthError),
    /// Requested subject not found.
    #[error("requested subject not found")]
    SubjectNotFound,
    /// Permission denied.
    #[error("caller lacks delegation issue permission")]
    PermissionDenied,
    /// JWT issue failure.
    #[error("token issue failed")]
    Issue(#[from] IssueError),
    /// Role name from SQL was invalid.
    #[error("role name is invalid")]
    InvalidRole,
    /// Card-scope resolution failed; fail closed rather than mint an unknown scope.
    #[error("card scope resolution failed")]
    CardScope(WyrdError),
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

impl ExchangeApiKey {
    /// Exchange a Wyrd API key for access and refresh tokens.
    ///
    /// # Errors
    /// All authentication failures map to `WyrdError::ApiKeyInvalid` at the HTTP
    /// boundary. Internal variants carry distinct failure paths for diagnostics.
    #[tracing::instrument(level = "debug", skip(self, conn, api_key), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        api_key: SecretString,
    ) -> Result<ExchangedToken, ExchangeError> {
        let parsed =
            WyrdApiKey::parse(api_key.expose_secret()).map_err(|_| ExchangeError::NotFound)?;
        if parsed.tenant_id != conn.data_tenant_id() {
            return Err(ExchangeError::CrossTenant);
        }
        let Some(row) = api_key_by_prefix(conn, &parsed.prefix).await? else {
            return Err(ExchangeError::NotFound);
        };
        if row.status != "active" {
            return Err(ExchangeError::AccountDisabled);
        }

        let raw = api_key.clone();
        let hash = row.key_hash.clone();
        let ok = tokio::task::spawn_blocking(move || wyrd_auth_issue::verify_api_key(&raw, &hash))
            .await?;
        if !ok {
            return Err(ExchangeError::HashMismatch);
        }

        let roles = role_refs(list_service_account_roles(conn, row.principal_id).await?)
            .map_err(|_| ExchangeError::InvalidRole)?;
        touch_api_key_last_used(conn, row.api_key_id).await?;
        issue_for_subject(
            conn,
            &self.issuing_key,
            &self.settings,
            IssueSubject {
                principal_id: row.principal_id,
                principal_kind: row.principal_kind,
                card_ref: row.card_ref.0,
                roles,
            },
            RefreshPolicy::Mint,
        )
        .await
        .map_err(ExchangeError::from)
    }
}

impl DelegateToken {
    /// Exchange an access token for a delegated Service/Agent token.
    ///
    /// # Errors
    /// Returns a typed error when verification, permission, subject resolution,
    /// issue, or database work fails.
    #[tracing::instrument(level = "debug", skip(self, conn, subject_token), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        subject_token: SecretString,
        requested_subject: RequestedSubject,
        request_id: &str,
    ) -> Result<ExchangedToken, DelegateError> {
        let verified = self
            .verifier
            .verify(&subject_token, &conn.data_tenant_id())
            .await?;
        self.permission_check
            .check(&verified.principal, &Permission::delegation_issue())
            .into_result()
            .map_err(|_| DelegateError::PermissionDenied)?;

        let row = resolve_requested_subject(conn, requested_subject).await?;
        let _ = runtime_principal_kind(&row.principal_kind, row.card_ref.0.clone())
            .ok_or(DelegateError::SubjectNotFound)?;
        let roles = role_refs(list_service_account_roles(conn, row.id).await?)
            .map_err(|_| DelegateError::InvalidRole)?;
        let caller = DelegationCaller {
            sub: verified
                .delegation_chain
                .first()
                .map(|step| step.principal.id.to_string())
                .unwrap_or_else(|| verified.principal.id.to_string()),
            principal: TokenPrincipalRef::from(&verified.principal),
            act: act_from_chain(&verified.delegation_chain, conn.data_tenant_id()),
        };
        let requested_ref = principal_ref(
            row.id,
            &row.principal_kind,
            conn.data_tenant_id(),
            row.card_ref.0.clone(),
        )
        .ok_or(DelegateError::SubjectNotFound)?;
        // Delegated tokens tag only cards BOTH the delegating caller and the
        // acting callee are authorized for (the fail-safe intersection). The
        // caller's own scope already rides their verified token.
        let callee_scope = resolve_card_scope(conn, &row.principal_kind, &row.card_ref.0)
            .await
            .map_err(DelegateError::CardScope)?;
        let card_scope = verified.principal.card_scope().intersection(&callee_scope);
        let access_token = self.issuing_key.issue_delegated_access_token(
            &caller,
            requested_ref,
            roles.clone(),
            card_scope,
            self.settings.access_ttl,
        )?;
        // Delegated tokens are short-lived and non-refreshable by design
        // (RFC 8693). The caller re-delegates when the access token expires, so
        // no refresh token is issued or persisted for the delegated principal.
        let expires_at = Utc::now() + self.settings.access_ttl;
        insert_audit_token_exchange(
            conn,
            Uuid::new_v4(),
            row.id,
            verified.principal.id.as_uuid(),
            json!(verified.delegation_chain),
            request_id,
            expires_at,
        )
        .await?;

        Ok(ExchangedToken {
            access_token: SecretString::from(access_token),
            refresh_token: None,
            token_type: TokenType::Bearer,
            expires_at,
        })
    }
}

pub(super) async fn issue_for_subject(
    conn: &mut TenantConn<'_>,
    issuing_key: &IssuingKey,
    settings: &TokenExchangeSettings,
    subject: IssueSubject,
    refresh: RefreshPolicy,
) -> Result<ExchangedToken, IssueOrSqlError> {
    let IssueSubject {
        principal_id,
        principal_kind,
        card_ref,
        roles,
    } = subject;
    let id = PrincipalId::new(principal_id);
    let card_scope = resolve_card_scope(conn, &principal_kind, &card_ref)
        .await
        .map_err(IssueOrSqlError::CardScope)?;
    let access_token = match principal_kind.as_str() {
        "service" => issuing_key.issue_service_access_token(
            id,
            conn.data_tenant_id(),
            card_ref.clone(),
            roles.clone(),
            card_scope,
            settings.access_ttl,
        )?,
        "agent" => issuing_key.issue_agent_access_token(
            id,
            conn.data_tenant_id(),
            card_ref.clone(),
            roles.clone(),
            card_scope,
            settings.access_ttl,
        )?,
        _ => return Err(IssueOrSqlError::Issue(IssueError::InvalidPrincipalKind)),
    };
    let refresh_token = match refresh {
        RefreshPolicy::Mint => {
            let token = issuing_key.issue_refresh_token(
                principal_kind_tag(&principal_kind).ok_or(IssueError::InvalidPrincipalKind)?,
                id,
                conn.data_tenant_id(),
                settings.refresh_ttl,
            )?;
            insert_refresh_token(
                conn,
                Uuid::new_v4(),
                &principal_kind,
                principal_id,
                &token_hash(&token),
                Utc::now() + settings.refresh_ttl,
            )
            .await?;
            Some(SecretString::from(token))
        }
        RefreshPolicy::Skip => None,
    };
    let expires_at = Utc::now() + settings.access_ttl;

    Ok(ExchangedToken {
        access_token: SecretString::from(access_token),
        refresh_token,
        token_type: TokenType::Bearer,
        expires_at,
    })
}

#[derive(Debug, thiserror::Error)]
pub(super) enum IssueOrSqlError {
    #[error("issue")]
    Issue(#[from] IssueError),
    #[error("card scope")]
    CardScope(WyrdError),
    #[error("db")]
    Database(#[from] sqlx::Error),
}

impl From<IssueOrSqlError> for ExchangeError {
    fn from(error: IssueOrSqlError) -> Self {
        match error {
            IssueOrSqlError::Issue(error) => Self::Issue(error),
            IssueOrSqlError::CardScope(error) => Self::CardScope(error),
            IssueOrSqlError::Database(error) => Self::Database(error),
        }
    }
}

async fn resolve_requested_subject(
    conn: &mut TenantConn<'_>,
    requested_subject: RequestedSubject,
) -> Result<wyrd_sql::queries::auth::ServiceAccountPrincipalRow, DelegateError> {
    match requested_subject {
        RequestedSubject::PrincipalId { id } => service_account_by_id(conn, id.as_uuid())
            .await?
            .ok_or(DelegateError::SubjectNotFound),
        RequestedSubject::CardRef { card_ref } => {
            let principal_kind =
                principal_kind_for_card(&card_ref).map_err(|_| DelegateError::SubjectNotFound)?;
            wyrd_sql::queries::auth::service_account_by_card_ref(conn, principal_kind, &card_ref)
                .await?
                .ok_or(DelegateError::SubjectNotFound)
        }
    }
}

pub(crate) fn role_refs(names: Vec<String>) -> Result<Vec<RoleRef>, wyrd_runtime::InvalidRoleName> {
    names.into_iter().map(|name| RoleRef::new(&name)).collect()
}

pub(super) fn principal_kind_tag(value: &str) -> Option<PrincipalKindTag> {
    match value {
        "service" => Some(PrincipalKindTag::Service),
        "agent" => Some(PrincipalKindTag::Agent),
        _ => None,
    }
}

fn runtime_principal_kind(value: &str, card_ref: CardRef) -> Option<PrincipalKind> {
    match value {
        "service" if card_ref.kind == CardKind::Service => {
            Some(PrincipalKind::Service { card_ref })
        }
        "agent" if card_ref.kind == CardKind::Agent => Some(PrincipalKind::Agent { card_ref }),
        _ => None,
    }
}

fn principal_ref(
    id: Uuid,
    kind: &str,
    tenant_id: wyrd_spec::DataTenantId,
    card_ref: CardRef,
) -> Option<TokenPrincipalRef> {
    Some(TokenPrincipalRef {
        id: PrincipalId::new(id),
        kind: runtime_principal_kind(kind, card_ref)?,
        tenant_id,
    })
}

fn act_from_chain(
    chain: &[wyrd_runtime::DelegationStep],
    tenant_id: wyrd_spec::DataTenantId,
) -> Option<Box<ActClaim>> {
    chain.iter().fold(None, |act, step| {
        Some(Box::new(ActClaim {
            sub: step.principal.id.to_string(),
            principal: TokenPrincipalRef::from((&step.principal, tenant_id)),
            act,
        }))
    })
}

pub(crate) fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

/// Disambiguate `ExchangeError::NotFound` by checking API-key lifecycle status.
pub(crate) async fn resolve_not_found_reason(
    conn: &mut TenantConn<'_>,
    prefix: &str,
) -> &'static str {
    reason_from_api_key_status(api_key_status_by_prefix(conn, prefix).await, prefix)
}

fn reason_from_api_key_status(
    result: Result<ApiKeyStatus, sqlx::Error>,
    prefix: &str,
) -> &'static str {
    match result {
        Ok(ApiKeyStatus::Revoked) => "revoked",
        Ok(ApiKeyStatus::Expired) => "expired",
        Ok(ApiKeyStatus::Missing) => "not_found",
        Ok(ApiKeyStatus::Active) => {
            tracing::warn!(
                prefix = %prefix,
                "api_key_status_by_prefix returned Active after ExchangeError::NotFound; race or cache inconsistency",
            );
            "not_found"
        }
        Err(error) => {
            tracing::error!(
                prefix = %prefix,
                error = %error,
                "api_key_status_by_prefix failed on error path",
            );
            "backend_unavailable"
        }
    }
}

/// Map API-key exchange errors to public Wyrd errors.
///
/// Credential failures keep the single public
/// `WYRD_AUTH_401_API_KEY_INVALID` code and discriminate through
/// `details.reason`.
pub async fn map_exchange_error_to_wyrd(
    conn: &mut TenantConn<'_>,
    prefix: &str,
    error: ExchangeError,
) -> WyrdError {
    match error {
        ExchangeError::CrossTenant => WyrdError::ApiKeyInvalid {
            message: "API key tenant does not match connection tenant".to_owned(),
            details: json!({ "reason": "cross_tenant" }),
        },
        ExchangeError::NotFound => {
            let reason = resolve_not_found_reason(conn, prefix).await;
            WyrdError::ApiKeyInvalid {
                message: format!("API key {reason}"),
                details: json!({ "reason": reason }),
            }
        }
        ExchangeError::AccountDisabled => WyrdError::ApiKeyInvalid {
            message: "service account is not active".to_owned(),
            details: json!({ "reason": "account_disabled" }),
        },
        ExchangeError::HashMismatch => WyrdError::ApiKeyInvalid {
            message: "API key hash verification failed".to_owned(),
            details: json!({ "reason": "hash_mismatch" }),
        },
        ExchangeError::Issue(_) | ExchangeError::Join(_) | ExchangeError::InvalidRole => {
            WyrdError::Internal {
                message: "failed to exchange API key".to_owned(),
                details: json!({}),
            }
        }
        ExchangeError::CardScope(error) => error,
        ExchangeError::Database(_) => WyrdError::AuthVerifyUnavailable {
            message: "auth backend unavailable".to_owned(),
            details: json!({ "retry_after_seconds": 1 }),
        },
    }
}

impl From<DelegateError> for WyrdError {
    fn from(error: DelegateError) -> Self {
        match error {
            DelegateError::InvalidSubjectToken(error) => crate::error::auth_error_to_wyrd(error),
            DelegateError::SubjectNotFound => WyrdError::PrincipalNotFound {
                message: "requested principal not found in tenant".to_owned(),
                details: json!({}),
            },
            DelegateError::PermissionDenied => WyrdError::PermissionDeniedRbac {
                message: "caller lacks delegation issue permission".to_owned(),
                details: json!({ "required": Permission::delegation_issue() }),
            },
            DelegateError::Issue(IssueError::DelegationDepthExceeded { max }) => {
                WyrdError::DelegationDepthExceededIssue {
                    message: format!("delegation chain would exceed max depth of {max}"),
                    details: json!({ "max": max }),
                }
            }
            DelegateError::Issue(_) | DelegateError::InvalidRole => WyrdError::Internal {
                message: "failed to issue delegated token".to_owned(),
                details: json!({}),
            },
            DelegateError::CardScope(error) => error,
            DelegateError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use secrecy::{ExposeSecret, SecretString};
    use sqlx::types::Json;
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        AccessTokenClaims, Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
        verify_eddsa,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{
        CardScope, Permission, PermissionCheck, PermissionVerdict, Principal, PrincipalId,
        RbacCheck, RoleRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::RequestedSubject;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::ApiKeyStatus;

    use super::{
        DelegateError, DelegateToken, ExchangeApiKey, ExchangeError, TokenExchangeSettings,
    };
    use crate::auth::issue_api_key::WyrdApiKey;
    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::auth::seed::seed_builtin_roles_for_tenant;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    fn test_service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("test-service").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn test_issuing_key() -> Arc<IssuingKey> {
        Arc::new(
            IssuingKey::from_ed_pem(
                SecretString::from(PRIVATE_KEY_PEM),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("test private key loads"),
        )
    }

    fn exchange_service() -> ExchangeApiKey {
        ExchangeApiKey {
            issuing_key: test_issuing_key(),
            settings: TokenExchangeSettings::default(),
        }
    }

    async fn insert_test_user(conn: &mut TenantConn<'_>, tenant_id: DataTenantId) -> Uuid {
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(user_id)
        .bind(tenant_id.as_uuid())
        .bind(format!("test-{}@example.com", user_id))
        .execute(&mut **conn.transaction())
        .await
        .expect("test user inserts");
        user_id
    }

    async fn insert_test_service_account(
        conn: &mut TenantConn<'_>,
        tenant_id: DataTenantId,
        created_by: Uuid,
        card_ref: &CardRef,
    ) -> Uuid {
        let sa_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref, space, name, version, status, created_by)
             VALUES ($1, $2, 'service', 'Service', $3, $4, $5, $6, $7, 'active', $8)",
        )
        .bind(sa_id)
        .bind(tenant_id.as_uuid())
        .bind(Uuid::new_v4())
        .bind(Json(card_ref.clone()))
        .bind(card_ref.space.as_str())
        .bind(format!("svc-{}", sa_id))
        .bind(card_ref.version.as_str())
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("service account inserts");
        sa_id
    }

    async fn insert_lifecycle_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        sa_id: Uuid,
        created_by: Uuid,
        prefix: &str,
        expires_at: chrono::DateTime<Utc>,
        revoked: bool,
    ) {
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, sa_id, prefix, key_hash, created_by, expires_at, revoked_at)
             VALUES ($1, $2, $3, $4, 'placeholder-hash', $5, $6, CASE WHEN $7 THEN now() ELSE NULL END)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(prefix)
        .bind(created_by)
        .bind(expires_at)
        .bind(revoked)
        .execute(&mut **conn.transaction())
        .await
        .expect("api key lifecycle row inserts");
    }

    fn api_key_invalid_reason(error: &WyrdError) -> &str {
        match error {
            WyrdError::ApiKeyInvalid { details, .. } => details["reason"]
                .as_str()
                .expect("api key invalid details.reason is a string"),
            other => panic!("expected ApiKeyInvalid, got {other:?}"),
        }
    }

    #[test]
    fn argon2_runs_on_blocking_pool() {
        let source = include_str!("exchange_api_key.rs");

        assert!(source.contains("tokio::task::spawn_blocking"));
        assert!(source.contains("wyrd_auth_issue::verify_api_key"));
    }

    #[test]
    fn act_from_chain_roundtrip() {
        use wyrd_runtime::{
            DelegationStep, PrincipalId, PrincipalKind, PrincipalRef as RuntimePrincipalRef,
        };
        use wyrd_semver::VersionBlock;
        use wyrd_spec::DataTenantId;
        use wyrd_spec::envelope::CardKind;
        use wyrd_spec::ids::{CardName, SpaceName};
        use wyrd_spec::reference::CardRef;

        let tenant_id: DataTenantId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id");
        let make_card = |name: &str| CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static name"),
            version: VersionBlock::parse("1.0.0").expect("static version"),
            space: SpaceName::new("prod").expect("static space"),
            uid: None,
        };
        let id_a: PrincipalId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"
            .parse()
            .expect("static id");
        let id_b: PrincipalId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b03"
            .parse()
            .expect("static id");
        let a = DelegationStep {
            principal: RuntimePrincipalRef {
                id: id_a,
                kind: PrincipalKind::Service {
                    card_ref: make_card("initiator"),
                },
            },
        };
        let b = DelegationStep {
            principal: RuntimePrincipalRef {
                id: id_b,
                kind: PrincipalKind::Service {
                    card_ref: make_card("immediate"),
                },
            },
        };

        let result = super::act_from_chain(&[a.clone(), b.clone()], tenant_id)
            .expect("non-empty chain builds act");

        assert_eq!(result.sub, b.principal.id.to_string());
        assert_eq!(
            result.act.as_ref().expect("inner act present").sub,
            a.principal.id.to_string()
        );

        // Round-trip: walk outermost-first, reverse → initiator-first order
        let mut flat = Vec::new();
        let mut cur = Some(result.as_ref());
        while let Some(layer) = cur {
            flat.push(layer.sub.clone());
            cur = layer.act.as_deref();
        }
        flat.reverse();
        assert_eq!(flat[0], a.principal.id.to_string());
        assert_eq!(flat[1], b.principal.id.to_string());
    }

    #[tokio::test]
    async fn api_key_invalid_reason_distinct_for_every_variant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &card_ref).await;

        insert_lifecycle_key(
            &mut conn,
            tenant,
            sa_id,
            user_id,
            "revoked-prefix",
            Utc::now() + Duration::days(1),
            true,
        )
        .await;
        insert_lifecycle_key(
            &mut conn,
            tenant,
            sa_id,
            user_id,
            "expired-prefix",
            Utc::now() - Duration::hours(1),
            false,
        )
        .await;

        let mut reasons = BTreeSet::new();
        for (prefix, error) in [
            ("direct", ExchangeError::CrossTenant),
            ("missing-prefix", ExchangeError::NotFound),
            ("revoked-prefix", ExchangeError::NotFound),
            ("expired-prefix", ExchangeError::NotFound),
            ("direct", ExchangeError::AccountDisabled),
            ("direct", ExchangeError::HashMismatch),
        ] {
            let mapped = super::map_exchange_error_to_wyrd(&mut conn, prefix, error).await;
            let reason = api_key_invalid_reason(&mapped);
            assert!(!reason.is_empty());
            assert!(
                reasons.insert(reason.to_owned()),
                "duplicate reason {reason}"
            );
        }

        let backend_unavailable =
            super::reason_from_api_key_status(Err(sqlx::Error::RowNotFound), "prefix");
        assert_eq!(backend_unavailable, "backend_unavailable");
        assert!(reasons.insert(backend_unavailable.to_owned()));

        assert_eq!(
            reasons,
            BTreeSet::from([
                "account_disabled".to_owned(),
                "backend_unavailable".to_owned(),
                "cross_tenant".to_owned(),
                "expired".to_owned(),
                "hash_mismatch".to_owned(),
                "not_found".to_owned(),
                "revoked".to_owned(),
            ])
        );
    }

    #[test]
    fn notfound_status_active_logs_race_and_returns_not_found() {
        let reason = super::reason_from_api_key_status(Ok(ApiKeyStatus::Active), "race-prefix");

        assert_eq!(reason, "not_found");
    }

    #[test]
    fn notfound_status_err_returns_backend_unavailable() {
        let reason =
            super::reason_from_api_key_status(Err(sqlx::Error::RowNotFound), "error-prefix");

        assert_eq!(reason, "backend_unavailable");
    }

    #[tokio::test]
    async fn cross_tenant_key_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();

        // Key has tenant_a embedded in its prefix.
        let key = WyrdApiKey::generate(tenant_a);

        // Present it through a tenant_b connection. The tenant mismatch fires
        // before any database query so tenant_b does not need to exist in
        // platform.tenants.
        let mut conn = fixture
            .tenant_conn_for(tenant_b)
            .await
            .expect("tenant B conn opens");
        let result = exchange_service().execute(&mut conn, key.secret).await;

        assert!(matches!(result, Err(ExchangeError::CrossTenant)));
    }

    #[tokio::test]
    async fn revoked_key_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();
        let key = WyrdApiKey::generate(tenant);

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &card_ref).await;

        // Insert the key row with revoked_at = now(). The prefix-lookup query
        // filters on `revoked_at IS NULL`, so this row is invisible and the
        // service returns NotFound.
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, sa_id, prefix, key_hash, created_by, expires_at, revoked_at)
             VALUES ($1, $2, $3, $4, 'placeholder-hash', $5, now() + interval '1 year', now())",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&key.prefix)
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("revoked api key inserts");

        let result = exchange_service().execute(&mut conn, key.secret).await;

        assert!(matches!(result, Err(ExchangeError::NotFound)));
    }

    #[tokio::test]
    async fn hash_mismatch_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();
        let key = WyrdApiKey::generate(tenant);

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &card_ref).await;

        // Store a hash that is not a valid Argon2 PHC string for this key.
        // verify_api_key() returns false → HashMismatch.
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, sa_id, prefix, key_hash, created_by, expires_at)
             VALUES ($1, $2, $3, $4, 'not-a-valid-phc-hash', $5, now() + interval '1 year')",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&key.prefix)
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("api key with wrong hash inserts");

        let result = exchange_service().execute(&mut conn, key.secret).await;

        assert!(matches!(result, Err(ExchangeError::HashMismatch)));
    }

    #[tokio::test]
    async fn delegation_permission_denied() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let issuing_key = test_issuing_key();

        // Issue a service token with no roles → effective permissions are empty
        // → delegation_issue check fails before any database lookup.
        let subject_token = issuing_key
            .issue_service_access_token(
                PrincipalId::new(Uuid::new_v4()),
                tenant,
                test_service_card_ref(),
                vec![],
                wyrd_runtime::CardScope::default(),
                Duration::minutes(15),
            )
            .expect("subject token issues");

        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(Kid::new("k1").expect("kid is valid"), Arc::new(public_key));
        let verifier = Arc::new(TokenVerifier::new(
            decoding_keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(
                fixture.app_pool().clone(),
            ))),
            WyrdAuthVerifySettings::default(),
        ));

        let delegate = DelegateToken {
            issuing_key,
            verifier,
            permission_check: Arc::new(RbacCheck),
            settings: TokenExchangeSettings::default(),
        };

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let result = delegate
            .execute(
                &mut conn,
                SecretString::from(subject_token),
                RequestedSubject::PrincipalId {
                    id: wyrd_spec::auth::PrincipalId::new(Uuid::new_v4()),
                },
                "test-request-id",
            )
            .await;

        assert!(matches!(result, Err(DelegateError::PermissionDenied)));
    }

    fn component_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Model,
            name: CardName::new("shared-model").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn callee_service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("callee-service").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    async fn insert_service_card(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        card_ref: &CardRef,
        components: &[CardRef],
    ) {
        use wyrd_spec::card::service::{ServiceComponent, ServiceSpec};
        let spec = ServiceSpec {
            components: components
                .iter()
                .enumerate()
                .map(|(index, component)| ServiceComponent {
                    alias: format!("component-{index}"),
                    card_ref: component.clone(),
                    source: None,
                    config: Default::default(),
                    credential_refs: Vec::new(),
                })
                .collect(),
            ..Default::default()
        };
        sqlx::query(
            "INSERT INTO wyrd.cards
                 (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(card_ref.kind.wire_name())
        .bind(card_ref.space.as_str())
        .bind(card_ref.name.as_str())
        .bind(card_ref.version.as_str())
        .bind(Json(spec))
        .bind("test-service-spec-hash")
        .execute(&mut **conn.transaction())
        .await
        .expect("service card inserts");
    }

    async fn insert_valid_api_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        sa_id: Uuid,
        created_by: Uuid,
        prefix: &str,
        key_hash: &str,
    ) {
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, sa_id, prefix, key_hash, created_by, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, now() + interval '365 days')",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(prefix)
        .bind(key_hash)
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("api key inserts");
    }

    fn decode_access(token: &SecretString) -> AccessTokenClaims {
        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        verify_eddsa::<AccessTokenClaims>(token.expose_secret(), &public_key, Some("wyrd"))
            .expect("access token verifies")
    }

    /// Permission check stub that authorizes every request, isolating the
    /// card-scope intersection from RBAC role seeding.
    #[derive(Debug)]
    struct AllowAllCheck;

    impl PermissionCheck for AllowAllCheck {
        fn check(&self, _: &Principal, _: &Permission) -> PermissionVerdict {
            PermissionVerdict::Allow
        }
    }

    fn delegate_service(fixture: &PgFixture) -> DelegateToken {
        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(Kid::new("k1").expect("kid is valid"), Arc::new(public_key));
        let verifier = Arc::new(TokenVerifier::new(
            decoding_keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(
                fixture.app_pool().clone(),
            ))),
            WyrdAuthVerifySettings::default(),
        ));
        DelegateToken {
            issuing_key: test_issuing_key(),
            verifier,
            permission_check: Arc::new(AllowAllCheck),
            settings: TokenExchangeSettings::default(),
        }
    }

    #[tokio::test]
    async fn exchange_card_scope_carries_components_union_own() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let own = test_service_card_ref();
        let component = component_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        insert_service_card(&mut conn, tenant, &own, std::slice::from_ref(&component)).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &own).await;

        let api_key = WyrdApiKey::generate(tenant);
        let key_hash = wyrd_auth_issue::hash_api_key(&api_key.secret).expect("api key hashes");
        insert_valid_api_key(
            &mut conn,
            tenant,
            sa_id,
            user_id,
            &api_key.prefix,
            &key_hash,
        )
        .await;

        let exchanged = exchange_service()
            .execute(&mut conn, api_key.secret)
            .await
            .expect("exchange succeeds");
        let claims = decode_access(&exchanged.access_token);

        assert!(claims.card_scope.contains(&own), "own card is in scope");
        assert!(
            claims.card_scope.contains(&component),
            "component card is in scope"
        );
        assert_eq!(claims.card_scope.len(), 2);
    }

    #[tokio::test]
    async fn exchange_card_scope_fails_closed_when_service_card_missing() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let own = test_service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        // Deliberately do NOT register the Service card → scope cannot resolve.
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &own).await;

        let api_key = WyrdApiKey::generate(tenant);
        let key_hash = wyrd_auth_issue::hash_api_key(&api_key.secret).expect("api key hashes");
        insert_valid_api_key(
            &mut conn,
            tenant,
            sa_id,
            user_id,
            &api_key.prefix,
            &key_hash,
        )
        .await;

        let result = exchange_service().execute(&mut conn, api_key.secret).await;

        assert!(
            matches!(result, Err(ExchangeError::CardScope(_))),
            "mint fails closed on missing service card: {result:?}"
        );
    }

    #[tokio::test]
    async fn delegated_card_scope_intersection_of_caller_and_callee() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let shared = component_card_ref();
        let caller_only = CardRef {
            kind: CardKind::Model,
            name: CardName::new("caller-only-model").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        };
        let callee_card = callee_service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        // Callee: a Service card whose scope is {shared} ∪ {callee own}.
        insert_service_card(
            &mut conn,
            tenant,
            &callee_card,
            std::slice::from_ref(&shared),
        )
        .await;
        insert_test_service_account(&mut conn, tenant, user_id, &callee_card).await;

        // Caller token carries a scope of {shared, caller_only}. Intersection
        // with the callee scope {shared, callee_own} must be {shared}.
        let caller_scope = CardScope::new([shared.clone(), caller_only.clone()]);
        let subject_token = test_issuing_key()
            .issue_service_access_token(
                PrincipalId::new(Uuid::new_v4()),
                tenant,
                test_service_card_ref(),
                vec![],
                caller_scope,
                Duration::minutes(15),
            )
            .expect("subject token issues");

        let exchanged = delegate_service(&fixture)
            .execute(
                &mut conn,
                SecretString::from(subject_token),
                RequestedSubject::CardRef {
                    card_ref: callee_card.clone(),
                },
                "test-delegated-scope",
            )
            .await
            .expect("delegation succeeds");
        let claims = decode_access(&exchanged.access_token);

        assert!(claims.card_scope.contains(&shared), "shared card survives");
        assert!(
            !claims.card_scope.contains(&caller_only),
            "caller-only card is excluded by callee scope"
        );
        assert!(
            !claims.card_scope.contains(&callee_card),
            "callee-own card is excluded by caller scope"
        );
        assert_eq!(claims.card_scope.len(), 1);
    }

    #[tokio::test]
    async fn delegation_issues_no_refresh_token() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        // Seed the per-tenant builtin roles so the resolver maps the delegator's
        // `runtime_admin` role claim to `delegation_issue`, and insert the target
        // Service principal (plus its Service card so the callee scope resolves)
        // the exchange resolves by card_ref.
        let target_card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("delegation-target").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        };
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        let creator = insert_test_user(&mut conn, tenant).await;
        insert_test_service_account(&mut conn, tenant, creator, &target_card_ref).await;
        insert_service_card(&mut conn, tenant, &target_card_ref, &[]).await;
        conn.commit().await.expect("seed commits");

        let issuing_key = test_issuing_key();
        let subject_token = issuing_key
            .issue_service_access_token(
                PrincipalId::new(Uuid::new_v4()),
                tenant,
                test_service_card_ref(),
                vec![RoleRef::new("runtime_admin").expect("role name is valid")],
                CardScope::new([test_service_card_ref()]),
                Duration::minutes(15),
            )
            .expect("subject token issues");

        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(Kid::new("k1").expect("kid is valid"), Arc::new(public_key));
        let verifier = Arc::new(TokenVerifier::new(
            decoding_keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(
                fixture.app_pool().clone(),
            ))),
            WyrdAuthVerifySettings::default(),
        ));

        let delegate = DelegateToken {
            issuing_key,
            verifier,
            permission_check: Arc::new(RbacCheck),
            settings: TokenExchangeSettings::default(),
        };

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let exchanged = delegate
            .execute(
                &mut conn,
                SecretString::from(subject_token),
                RequestedSubject::CardRef {
                    card_ref: target_card_ref,
                },
                "req-delegation-no-refresh",
            )
            .await
            .expect("delegation succeeds");

        assert!(
            exchanged.refresh_token.is_none(),
            "delegated token-exchange must not issue a refresh token"
        );
        assert_eq!(exchanged.token_type, wyrd_spec::auth::TokenType::Bearer);
    }
}
