//! API-key exchange and RFC 8693 delegation services.

use std::sync::Arc;

use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_auth_issue::{DelegationCaller, IssueError, IssuingKey};
use wyrd_auth_verify::{ActClaim, AuthError, PrincipalKindWire, TokenPrincipalRef, TokenVerifier};
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
        let access_token = self.issuing_key.issue_delegated_access_token(
            &caller,
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
) -> Result<ExchangedToken, IssueOrSqlError> {
    let id = PrincipalId::new(principal_id);
    let access_token = match principal_kind {
        "service" => issuing_key.issue_service_access_token(
            id,
            conn.data_tenant_id(),
            card_ref.clone(),
            roles.clone(),
            settings.access_ttl,
        )?,
        "agent" => issuing_key.issue_agent_access_token(
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
) -> Option<TokenPrincipalRef> {
    Some(TokenPrincipalRef {
        id: PrincipalId::new(id),
        kind: principal_kind_wire(kind)?,
        tenant_id,
        card_ref: Some(card_ref),
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
            DelegateError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::Duration;
    use secrecy::SecretString;
    use sqlx::types::Json;
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{PrincipalId, RbacCheck};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::RequestedSubject;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::version::VersionBlock;
    use wyrd_sql::TenantConn;

    use crate::auth::issue_api_key::WyrdApiKey;
    use crate::auth::permission_resolver::SqlPermissionResolver;
    use super::{DelegateError, DelegateToken, ExchangeApiKey, ExchangeError, TokenExchangeSettings};

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
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref, name, status, created_by)
             VALUES ($1, $2, 'service', 'Service', $3, $4, $5, 'active', $6)",
        )
        .bind(sa_id)
        .bind(tenant_id.as_uuid())
        .bind(Uuid::new_v4())
        .bind(Json(card_ref.clone()))
        .bind(format!("svc-{}", sa_id))
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("service account inserts");
        sa_id
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
        use wyrd_spec::DataTenantId;
        use wyrd_spec::envelope::CardKind;
        use wyrd_spec::ids::{CardName, SpaceName};
        use wyrd_spec::reference::CardRef;
        use wyrd_spec::version::VersionBlock;

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

        assert!(matches!(result, Err(ExchangeError::InvalidApiKey)));
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
        // service returns InvalidApiKey.
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

        assert!(matches!(result, Err(ExchangeError::InvalidApiKey)));
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
        // verify_api_key() returns false → InvalidApiKey.
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

        assert!(matches!(result, Err(ExchangeError::InvalidApiKey)));
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
                Duration::minutes(15),
            )
            .expect("subject token issues");

        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key),
        );
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
}
