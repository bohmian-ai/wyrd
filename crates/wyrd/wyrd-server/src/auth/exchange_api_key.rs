//! API-key exchange and RFC 8693 delegation services.

use std::sync::Arc;

use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_auth_issue::{IssueError, IssuingKey};
use wyrd_auth_verify::{
    AccessTokenClaims, ActClaim, AuthError, PrincipalKindWire, PrincipalRef, TokenVerifier,
};
use wyrd_runtime::{Permission, PermissionCheck, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::auth::{RequestedSubject, SecretBearer, TokenResponse, TokenType};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    api_key_by_prefix, insert_audit_token_exchange, insert_refresh_token, service_account_by_id,
    service_account_roles, touch_api_key_last_used,
};

use crate::auth::issue_api_key::{WyrdApiKey, principal_kind_for_card};
use crate::auth::permission_resolver::SqlPermissionResolver;

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
    pub verifier: Arc<TokenVerifier<SqlPermissionResolver>>,
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
    /// Refresh token.
    pub refresh_token: SecretString,
    /// Token type.
    pub token_type: TokenType,
    /// Access token expiry.
    pub expires_at: chrono::DateTime<Utc>,
    /// Principal id.
    pub principal_id: PrincipalId,
    /// Principal kind.
    pub principal_kind: PrincipalKind,
    /// Bound card ref.
    pub card_ref: CardRef,
}

impl ExchangedToken {
    /// Convert to public token response.
    #[must_use]
    pub fn into_response(self) -> TokenResponse {
        TokenResponse {
            access_token: SecretBearer::new(self.access_token.expose_secret().to_owned()),
            refresh_token: SecretBearer::new(self.refresh_token.expose_secret().to_owned()),
            token_type: self.token_type,
            expires_at: self.expires_at,
        }
    }
}

/// API-key exchange failure.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    /// Invalid public API-key failure.
    #[error("api key invalid")]
    InvalidApiKey,
    /// JWT issue failure.
    #[error("token issue failed")]
    Issue(#[from] IssueError),
    /// Blocking task failed.
    #[error("api key verify task failed")]
    Join(#[from] tokio::task::JoinError),
    /// Role name from SQL was invalid.
    #[error("role name is invalid")]
    InvalidRole,
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
    /// Delegation chain too deep.
    #[error("delegation chain would exceed max depth")]
    DelegationDepthExceeded,
    /// JWT issue failure.
    #[error("token issue failed")]
    Issue(#[from] IssueError),
    /// Role name from SQL was invalid.
    #[error("role name is invalid")]
    InvalidRole,
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

impl ExchangeApiKey {
    /// Exchange a Wyrd API key for access and refresh tokens.
    ///
    /// # Errors
    /// Returns a single observable invalid-key error for parse, lookup, status,
    /// expiry, revocation, or hash mismatch failures.
    #[tracing::instrument(level = "debug", skip(self, conn, api_key), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        api_key: SecretString,
    ) -> Result<ExchangedToken, ExchangeError> {
        let parsed =
            WyrdApiKey::parse(api_key.expose_secret()).map_err(|_| ExchangeError::InvalidApiKey)?;
        if parsed.tenant_id != conn.data_tenant_id() {
            return Err(ExchangeError::InvalidApiKey);
        }
        let Some(row) = api_key_by_prefix(conn, &parsed.prefix).await? else {
            return Err(ExchangeError::InvalidApiKey);
        };
        if row.status != "active" {
            return Err(ExchangeError::InvalidApiKey);
        }

        let raw = api_key.clone();
        let hash = row.key_hash.clone();
        let ok = tokio::task::spawn_blocking(move || wyrd_auth_issue::verify_api_key(&raw, &hash))
            .await?;
        if !ok {
            return Err(ExchangeError::InvalidApiKey);
        }

        let roles = role_refs(service_account_roles(conn, row.principal_id).await?)
            .map_err(|_| ExchangeError::InvalidRole)?;
        touch_api_key_last_used(conn, row.api_key_id).await?;
        issue_for_subject(
            conn,
            &self.issuing_key,
            &self.settings,
            row.principal_id,
            &row.principal_kind,
            row.card_ref.0,
            roles,
            None,
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
        let roles = role_refs(service_account_roles(conn, row.id).await?)
            .map_err(|_| DelegateError::InvalidRole)?;
        let caller_claims = claims_from_verified(&verified.principal, &verified.delegation_chain);
        let requested_ref = principal_ref(
            row.id,
            &row.principal_kind,
            conn.data_tenant_id(),
            row.card_ref.0.clone(),
        )
        .ok_or(DelegateError::SubjectNotFound)?;
        let access_token = self.issuing_key.issue_delegated_access_token(
            &caller_claims,
            requested_ref,
            roles.clone(),
            self.settings.access_ttl,
        )?;
        let refresh_token = self.issuing_key.issue_refresh_token(
            principal_kind_wire(&row.principal_kind).ok_or(DelegateError::SubjectNotFound)?,
            PrincipalId::new(row.id),
            conn.data_tenant_id(),
            self.settings.refresh_ttl,
        )?;
        let expires_at = Utc::now() + self.settings.access_ttl;
        let refresh_expires_at = Utc::now() + self.settings.refresh_ttl;
        insert_refresh_token(
            conn,
            Uuid::new_v4(),
            &row.principal_kind,
            row.id,
            &token_hash(&refresh_token),
            refresh_expires_at,
        )
        .await?;
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
            refresh_token: SecretString::from(refresh_token),
            token_type: TokenType::Bearer,
            expires_at,
            principal_id: PrincipalId::new(row.id),
            principal_kind: runtime_principal_kind(&row.principal_kind, row.card_ref.0.clone())
                .ok_or(DelegateError::SubjectNotFound)?,
            card_ref: row.card_ref.0,
        })
    }
}

async fn issue_for_subject(
    conn: &mut TenantConn<'_>,
    issuing_key: &IssuingKey,
    settings: &TokenExchangeSettings,
    principal_id: Uuid,
    principal_kind: &str,
    card_ref: CardRef,
    roles: Vec<RoleRef>,
    act: Option<Box<ActClaim>>,
) -> Result<ExchangedToken, IssueOrSqlError> {
    let id = PrincipalId::new(principal_id);
    let access_token = match (principal_kind, act) {
        ("service", None) => issuing_key.issue_service_access_token(
            id,
            conn.data_tenant_id(),
            card_ref.clone(),
            roles.clone(),
            settings.access_ttl,
        )?,
        ("agent", None) => issuing_key.issue_agent_access_token(
            id,
            conn.data_tenant_id(),
            card_ref.clone(),
            roles.clone(),
            settings.access_ttl,
        )?,
        _ => return Err(IssueOrSqlError::Issue(IssueError::InvalidPrincipalKind)),
    };
    let refresh_token = issuing_key.issue_refresh_token(
        principal_kind_wire(principal_kind).ok_or(IssueError::InvalidPrincipalKind)?,
        id,
        conn.data_tenant_id(),
        settings.refresh_ttl,
    )?;
    let expires_at = Utc::now() + settings.access_ttl;
    insert_refresh_token(
        conn,
        Uuid::new_v4(),
        principal_kind,
        principal_id,
        &token_hash(&refresh_token),
        Utc::now() + settings.refresh_ttl,
    )
    .await?;

    Ok(ExchangedToken {
        access_token: SecretString::from(access_token),
        refresh_token: SecretString::from(refresh_token),
        token_type: TokenType::Bearer,
        expires_at,
        principal_id: id,
        principal_kind: runtime_principal_kind(principal_kind, card_ref.clone())
            .ok_or(IssueError::InvalidPrincipalKind)?,
        card_ref,
    })
}

#[derive(Debug, thiserror::Error)]
enum IssueOrSqlError {
    #[error("issue")]
    Issue(#[from] IssueError),
    #[error("db")]
    Database(#[from] sqlx::Error),
}

impl From<IssueOrSqlError> for ExchangeError {
    fn from(error: IssueOrSqlError) -> Self {
        match error {
            IssueOrSqlError::Issue(error) => Self::Issue(error),
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

fn role_refs(names: Vec<String>) -> Result<Vec<RoleRef>, wyrd_runtime::InvalidRoleName> {
    names.into_iter().map(|name| RoleRef::new(&name)).collect()
}

fn principal_kind_wire(value: &str) -> Option<PrincipalKindWire> {
    match value {
        "service" => Some(PrincipalKindWire::Service),
        "agent" => Some(PrincipalKindWire::Agent),
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
) -> Option<PrincipalRef> {
    Some(PrincipalRef {
        id: PrincipalId::new(id),
        kind: principal_kind_wire(kind)?,
        tenant_id,
        card_ref: Some(card_ref),
    })
}

fn claims_from_verified(
    principal: &wyrd_runtime::Principal,
    chain: &[wyrd_runtime::DelegationStep],
) -> AccessTokenClaims {
    let principal_ref = PrincipalRef::from(principal);
    AccessTokenClaims {
        sub: chain
            .first()
            .map(|step| step.principal.id.to_string())
            .unwrap_or_else(|| principal.id.to_string()),
        principal: principal_ref,
        roles: principal.roles.clone(),
        act: act_from_chain(chain, principal.tenant_id),
        exp: (Utc::now() + Duration::minutes(5)).timestamp() as usize,
        iat: Utc::now().timestamp() as usize,
        iss: "wyrd".to_owned(),
        jti: "delegation-source".to_owned(),
    }
}

fn act_from_chain(
    chain: &[wyrd_runtime::DelegationStep],
    tenant_id: wyrd_spec::DataTenantId,
) -> Option<Box<ActClaim>> {
    chain.iter().rev().fold(None, |act, step| {
        Some(Box::new(ActClaim {
            sub: step.principal.id.to_string(),
            principal: PrincipalRef {
                id: step.principal.id,
                kind: match &step.principal.kind {
                    PrincipalKind::User => PrincipalKindWire::User,
                    PrincipalKind::Service { .. } => PrincipalKindWire::Service,
                    PrincipalKind::Agent { .. } => PrincipalKindWire::Agent,
                },
                tenant_id,
                card_ref: step.principal.card_ref.clone(),
            },
            act,
        }))
    })
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

impl From<ExchangeError> for WyrdError {
    fn from(error: ExchangeError) -> Self {
        match error {
            ExchangeError::InvalidApiKey => WyrdError::ApiKeyInvalid {
                message: "API key not found, revoked, expired, or hash mismatch".to_owned(),
                details: json!({}),
            },
            ExchangeError::Issue(_) | ExchangeError::Join(_) | ExchangeError::InvalidRole => {
                WyrdError::Internal {
                    message: "failed to exchange API key".to_owned(),
                    details: json!({}),
                }
            }
            ExchangeError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
        }
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
            DelegateError::DelegationDepthExceeded => WyrdError::DelegationDepthExceededIssue {
                message: "delegation chain would exceed max depth".to_owned(),
                details: json!({ "max": wyrd_auth_issue::MAX_DELEGATION_DEPTH }),
            },
            DelegateError::Issue(_) | DelegateError::InvalidRole => WyrdError::Internal {
                message: "failed to issue delegated token".to_owned(),
                details: json!({}),
            },
            DelegateError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
        }
    }
}
