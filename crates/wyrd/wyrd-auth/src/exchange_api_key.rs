//! API-key exchange and RFC 8693 delegation services.

use std::sync::Arc;

use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_auth_issue::{DelegationCaller, IssueError, IssuingKey};
use wyrd_auth_verify::{ActClaim, AuthError, TokenPrincipalRef, TokenVerifier};
use wyrd_runtime::{Permission, PermissionCheck, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::{RequestedSubject, SecretBearer, TokenResponse, TokenType};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_spec::vala::audit_detail::CardScopeMintKind;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    ApiKeyStatus, api_key_by_prefix, api_key_status_by_prefix, list_service_account_roles,
    service_account_by_id, touch_api_key_last_used,
};
use wyrd_sql::queries::platform::tenant_resolver::tenant_admits_credentials;

use crate::audit::{TOKEN_EXCHANGE_OPERATION, append_auth_audit, auth_event};
use crate::credential_verify::verify_presented;

use crate::card_scope::{
    IssueErrorOrWyrd, MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_DELEGATION, issue_scope_error,
    resolve_card_ref_scope, write_scope_mint_success_audit,
};
use crate::error::auth_error_to_wyrd;
use crate::issue_api_key::{WyrdApiKey, principal_kind_for_card};
use crate::permission_resolver::SqlPermissionResolver;
use crate::pg_resolvers::PgIssuerResolver;

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
    /// Refresh token. Present only for a human OIDC session, which has no
    /// durable credential to re-present. Every machine grant — API-key
    /// exchange, workload `jwt-bearer`, and `token-exchange` delegation —
    /// leaves this `None` and re-exchanges its credential instead.
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

/// The service/agent principal a token is being issued for.
pub(crate) struct IssueSubject {
    /// Stable principal id.
    pub principal_id: Uuid,
    /// Principal kind label, as stored on the durable principal row.
    pub principal_kind: String,
    /// Bound Card reference embedded in the access token, absent for a
    /// principal that binds no Card.
    pub card_ref: Option<CardRef>,
    /// Effective roles embedded in the access token.
    pub roles: Vec<RoleRef>,
    /// Non-secret id of the credential presented to earn this token, when one
    /// was. It travels in the access token's claims so audit names the key a
    /// decision was made with, not merely its holder.
    pub credential_id: Option<Uuid>,
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
    /// The key's tenant is not in a state that admits credentials.
    ///
    /// Distinct from [`ExchangeError::AccountDisabled`] internally so a
    /// suspended tenant is diagnosable, and rendered identically at the
    /// boundary so a caller cannot tell a suspended tenant from a disabled
    /// principal or an unknown key.
    #[error("tenant does not admit credentials")]
    TenantNotAdmitting,
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
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    /// Wyrd contract error.
    #[error("wyrd error")]
    Wyrd(#[from] WyrdError),
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
    /// Wyrd contract error.
    #[error("wyrd error")]
    Wyrd(#[from] WyrdError),
}

impl ExchangeApiKey {
    /// Exchange a Wyrd API key for access and refresh tokens.
    ///
    /// # Errors
    /// All authentication failures map to `WyrdError::ApiKeyInvalid` at the HTTP
    /// boundary. Internal variants carry distinct failure paths for diagnostics.
    ///
    /// # Panics
    /// Panics only if the refusal bookkeeping below is ever changed so that an
    /// absent credential row leaves no refusal — the invariant the `expect`
    /// names.
    #[tracing::instrument(level = "debug", skip(self, conn, api_key), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        api_key: SecretString,
        request_id: &str,
    ) -> Result<ExchangedToken, ExchangeError> {
        // Every refusal is decided first and answered last, because Argon2 is
        // what a refusal costs. A malformed key, another tenant's key, a
        // suspended tenant, an unknown prefix, and a disabled account all used
        // to return before verification ran, so a live prefix with a wrong tail
        // took measurably longer than any of them — enough to enumerate live
        // prefixes by clock without ever guessing a secret.
        let (row, refusal) = match WyrdApiKey::parse(api_key.expose_secret()) {
            Err(_) => (None, Some(ExchangeError::NotFound)),
            Ok(parsed) if parsed.tenant_id != conn.data_tenant_id() => {
                (None, Some(ExchangeError::CrossTenant))
            }
            // The tenant's own lifecycle decides before the principal's does.
            // Suspending a tenant has to stop its credentials working, or the
            // status is a label rather than a control — and this is the one place
            // every credential-bearing entry to a tenant converges, so checking
            // here cannot be forgotten by a route added later.
            Ok(parsed) if !tenant_admits_credentials(conn, parsed.tenant_id).await? => {
                (None, Some(ExchangeError::TenantNotAdmitting))
            }
            Ok(parsed) => match api_key_by_prefix(conn, &parsed.prefix).await? {
                None => (None, Some(ExchangeError::NotFound)),
                Some(row) if row.status != "active" => (None, Some(ExchangeError::AccountDisabled)),
                Some(row) => (Some(row), None),
            },
        };

        let matched = verify_presented(&api_key, row.as_ref().map(|row| row.key_hash.as_str()))
            .await
            .map_err(ExchangeError::Join)?;
        if let Some(refusal) = refusal {
            return Err(refusal);
        }
        let row = row.expect("invariant: a refusal was recorded for every absent row");
        if !matched {
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
                card_ref: row.card_ref.map(|card_ref| card_ref.0),
                roles,
                credential_id: Some(row.api_key_id),
            },
            request_id,
            MINT_KIND_API_KEY_EXCHANGE,
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
        let requested_card_ref = row
            .card_ref
            .clone()
            .map(|card_ref| card_ref.0)
            .ok_or(DelegateError::SubjectNotFound)?;
        let card_ref_scope = resolve_card_ref_scope(conn, &requested_card_ref).await?;
        let _ = runtime_principal_kind(&row.principal_kind, Some(requested_card_ref.clone()))
            .ok_or(DelegateError::SubjectNotFound)?;
        let roles = role_refs(list_service_account_roles(conn, row.id).await?)
            .map_err(|_| DelegateError::InvalidRole)?;
        let caller = DelegationCaller {
            sub: verified.delegation_chain.first().map_or_else(
                || verified.principal.id.to_string(),
                |step| step.principal.id.to_string(),
            ),
            principal: TokenPrincipalRef::from(&verified.principal),
            act: act_from_chain(&verified.delegation_chain, conn.data_tenant_id()),
        };
        let requested_ref = principal_ref(
            row.id,
            &row.principal_kind,
            conn.data_tenant_id(),
            requested_card_ref.clone(),
            card_ref_scope.clone(),
        )
        .ok_or(DelegateError::SubjectNotFound)?;
        let access_token = self
            .issuing_key
            .issue_delegated_access_token(
                &caller,
                requested_ref,
                roles.clone(),
                self.settings.access_ttl,
            )
            .map_err(|error| delegate_issue_error(error, &requested_card_ref))?;
        // Delegated tokens are short-lived and non-refreshable by design
        // (RFC 8693). The caller re-delegates when the access token expires, so
        // no refresh token is issued or persisted for the delegated principal.
        let expires_at = Utc::now() + self.settings.access_ttl;
        let mut event = auth_event(
            request_id,
            TOKEN_EXCHANGE_OPERATION,
            verified.principal.id,
            verified.principal.kind.tag(),
            verified.principal.card_ref().cloned(),
            AuditOutcome::Allowed,
            AuditDetail::TokenExchange {
                subject_principal_id: PrincipalId::new(row.id),
                actor_principal_id: verified.principal.id,
                delegation_chain: verified
                    .delegation_chain
                    .iter()
                    .filter_map(|step| step.principal.card_ref().cloned())
                    .collect(),
                expires_at,
            },
        );
        event.permission = Permission::delegation_issue().to_string();
        append_auth_audit(conn, &event).await?;
        write_scope_mint_success_audit(
            conn,
            row.id,
            &requested_card_ref,
            &card_ref_scope,
            request_id,
            MINT_KIND_DELEGATION,
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

/// Issue tokens for a principal that binds no Card.
///
/// Separate from the Card-bound path because there is no card scope to resolve
/// and no scope-mint audit to write: a Card-free principal has no emit
/// authority to attribute. Its roles and tenant still bound what it may do.
///
/// # Errors
/// Returns an issuance error when the kind is not Card-free-eligible or
/// signing fails, and a SQL error when the refresh token cannot be stored.
async fn issue_cardless_subject(
    conn: &mut TenantConn<'_>,
    issuing_key: &IssuingKey,
    settings: &TokenExchangeSettings,
    subject: IssueSubject,
) -> Result<ExchangedToken, IssueOrSqlError> {
    let IssueSubject {
        principal_id,
        principal_kind,
        card_ref: _,
        roles,
        credential_id,
    } = subject;
    let id = PrincipalId::new(principal_id);
    let wire = principal_kind_wire(&principal_kind).ok_or(IssueError::InvalidPrincipalKind)?;
    let access_token = issuing_key.issue_cardless_access_token(
        TokenPrincipalRef {
            id,
            kind: wire,
            tenant_id: conn.data_tenant_id(),
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        },
        roles,
        credential_id,
        settings.access_ttl,
    )?;

    Ok(ExchangedToken {
        access_token: SecretString::from(access_token),
        refresh_token: None,
        token_type: TokenType::Bearer,
        expires_at: Utc::now() + settings.access_ttl,
    })
}

/// Issue an access token and scope-mint audit for a machine principal.
///
/// No refresh token: every caller here holds a durable credential — an API key
/// or a platform-attested assertion — and re-exchanges it for a fresh access
/// token. Minting a long-lived refresh secret for a holder that never needs one
/// only widens the leak surface. Only a human OIDC session, which has no
/// durable credential to re-present, receives and rotates a refresh token.
pub(crate) async fn issue_for_subject(
    conn: &mut TenantConn<'_>,
    issuing_key: &IssuingKey,
    settings: &TokenExchangeSettings,
    subject: IssueSubject,
    request_id: &str,
    mint_kind: CardScopeMintKind,
) -> Result<ExchangedToken, IssueOrSqlError> {
    // A Card-free principal — a tenant administrator or tenant automation —
    // carries no bound Card and therefore no emit scope. It still holds roles
    // and must be able to exchange its credential, or a provisioned tenant
    // would hand back a credential that never works.
    if subject.card_ref.is_none() {
        return issue_cardless_subject(conn, issuing_key, settings, subject).await;
    }
    let IssueSubject {
        principal_id,
        principal_kind,
        card_ref,
        roles,
        credential_id,
    } = subject;
    let id = PrincipalId::new(principal_id);
    let card_ref = card_ref.expect("invariant: card-free subjects returned above");
    let card_ref_scope = resolve_card_ref_scope(conn, &card_ref).await?;
    let kind = match principal_kind.as_str() {
        "service" => PrincipalKindTag::Service,
        "agent" => PrincipalKindTag::Agent,
        _ => return Err(IssueOrSqlError::Issue(IssueError::InvalidPrincipalKind)),
    };
    let access_token = issuing_key
        .issue_card_access_token(
            TokenPrincipalRef {
                id,
                kind,
                tenant_id: conn.data_tenant_id(),
                card_ref: Some(card_ref.clone()),
                card_ref_scope: card_ref_scope.clone(),
            },
            roles.clone(),
            credential_id,
            settings.access_ttl,
        )
        .map_err(|error| issue_or_wyrd_error(error, &card_ref))?;
    let expires_at = Utc::now() + settings.access_ttl;
    write_scope_mint_success_audit(
        conn,
        principal_id,
        &card_ref,
        &card_ref_scope,
        request_id,
        mint_kind,
    )
    .await?;

    Ok(ExchangedToken {
        access_token: SecretString::from(access_token),
        refresh_token: None,
        token_type: TokenType::Bearer,
        expires_at,
    })
}

/// Map card-bound issuer errors into the API-key exchange error channel.
fn issue_or_wyrd_error(error: IssueError, root: &CardRef) -> IssueOrSqlError {
    match issue_scope_error(error, root) {
        IssueErrorOrWyrd::Issue(error) => IssueOrSqlError::Issue(error),
        IssueErrorOrWyrd::Wyrd(error) => IssueOrSqlError::Wyrd(error),
    }
}

/// Map card-bound issuer errors into the delegation error channel.
fn delegate_issue_error(error: IssueError, root: &CardRef) -> DelegateError {
    match issue_scope_error(error, root) {
        IssueErrorOrWyrd::Issue(error) => DelegateError::Issue(error),
        IssueErrorOrWyrd::Wyrd(error) => DelegateError::Wyrd(error),
    }
}

/// Error channel for token issue and database work during subject token minting.
#[derive(Debug, thiserror::Error)]
pub(crate) enum IssueOrSqlError {
    /// Token issuer rejected the mint operation.
    #[error("issue")]
    Issue(#[from] IssueError),
    /// Database operation failed.
    #[error("db")]
    Database(#[from] sqlx::Error),
    /// Public Wyrd contract error.
    #[error("wyrd")]
    Wyrd(#[from] WyrdError),
}

impl From<IssueOrSqlError> for ExchangeError {
    fn from(error: IssueOrSqlError) -> Self {
        match error {
            IssueOrSqlError::Issue(error) => Self::Issue(error),
            IssueOrSqlError::Database(error) => Self::Database(error),
            IssueOrSqlError::Wyrd(error) => Self::Wyrd(error),
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

/// Convert stored role names into runtime role references.
pub(crate) fn role_refs(names: Vec<String>) -> Result<Vec<RoleRef>, wyrd_runtime::InvalidRoleName> {
    names.into_iter().map(|name| RoleRef::new(&name)).collect()
}

/// Convert a stored principal kind string into the token wire enum.
///
/// Public because the kind is not merely informational: the revocation-epoch
/// cache is keyed by it, so any caller fanning out a revocation has to name the
/// stored kind rather than assume one. `None` for an unrecognized value, which
/// every caller must treat as a refusal rather than defaulting.
pub fn principal_kind_wire(value: &str) -> Option<PrincipalKindTag> {
    match value {
        "tenant_admin" => Some(PrincipalKindTag::TenantAdmin),
        "service" => Some(PrincipalKindTag::Service),
        "agent" => Some(PrincipalKindTag::Agent),
        _ => None,
    }
}

/// Rebuild the runtime principal kind from its durable label and Card binding.
///
/// Card binding is a property of a machine principal rather than a
/// precondition, so a `service` row with no Card resolves to a Card-free
/// service that carries no emit scope. A mismatched Card kind resolves to
/// `None`, which the caller turns into a fail-closed refusal.
fn runtime_principal_kind(value: &str, card_ref: Option<CardRef>) -> Option<PrincipalKind> {
    match (value, card_ref) {
        ("tenant_admin", None) => Some(PrincipalKind::TenantAdmin),
        ("service", None) => Some(PrincipalKind::Service {
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        }),
        ("service", Some(card_ref)) if card_ref.kind == CardKind::Service => {
            let card_ref_scope = CardRefScope::own(&card_ref);
            Some(PrincipalKind::Service {
                card_ref: Some(card_ref),
                card_ref_scope,
            })
        }
        ("agent", Some(card_ref)) if card_ref.kind == CardKind::Agent => {
            let card_ref_scope = CardRefScope::own(&card_ref);
            Some(PrincipalKind::Agent {
                card_ref,
                card_ref_scope,
            })
        }
        _ => None,
    }
}

fn principal_ref(
    id: Uuid,
    kind: &str,
    tenant_id: wyrd_spec::DataTenantId,
    card_ref: CardRef,
    card_ref_scope: CardRefScope,
) -> Option<TokenPrincipalRef> {
    Some(TokenPrincipalRef {
        id: PrincipalId::new(id),
        kind: principal_kind_wire(kind)?,
        tenant_id,
        card_ref: Some(card_ref),
        card_ref_scope,
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

/// Hash a bearer secret for storage lookup and comparison.
#[must_use]
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
/// Every credential refusal renders as the same `WYRD_AUTH_401_API_KEY_INVALID`
/// problem, byte for byte. The reason used to be projected in
/// `details.reason` — `revoked`, `expired`, `cross_tenant`, `account_disabled`,
/// `not_found` — which handed an unauthenticated caller the enumeration oracle
/// the constant-cost verification exists to close: learning that a prefix is
/// *revoked* is learning that it exists. The reason is still recorded, in the
/// server's own logs, where the operator reading them has already been
/// authenticated by the deployment.
///
/// Infrastructure failures stay distinct, because a caller that should retry
/// needs to know that it should.
pub async fn map_exchange_error_to_wyrd(
    conn: &mut TenantConn<'_>,
    prefix: &str,
    error: ExchangeError,
) -> WyrdError {
    let reason: &str = match error {
        ExchangeError::CrossTenant => "cross_tenant",
        ExchangeError::AccountDisabled => "account_disabled",
        ExchangeError::HashMismatch => "hash_mismatch",
        ExchangeError::TenantNotAdmitting => "tenant_not_admitting",
        // The lifecycle lookup that used to shape the response now only shapes
        // the log line: an operator still needs to know whether a key was
        // revoked or expired.
        ExchangeError::NotFound => resolve_not_found_reason(conn, prefix).await,
        ExchangeError::Issue(_) | ExchangeError::Join(_) | ExchangeError::InvalidRole => {
            return WyrdError::Internal {
                message: "failed to exchange API key".to_owned(),
                details: json!({}),
            };
        }
        ExchangeError::Wyrd(error) => return error,
        ExchangeError::Database(_) => {
            return WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            };
        }
    };
    tracing::info!(prefix = %prefix, reason, "api key exchange refused");
    api_key_invalid()
}

/// The one refusal every invalid tenant API key earns.
///
/// Identical for every cause on purpose: a caller must not be able to tell a
/// wrong secret from an unknown prefix, a revoked key from an expired one, or a
/// suspended tenant from one that never existed.
#[must_use]
pub fn api_key_invalid() -> WyrdError {
    WyrdError::ApiKeyInvalid {
        message: "API key is not valid".to_owned(),
        details: json!({}),
    }
}

impl From<DelegateError> for WyrdError {
    fn from(error: DelegateError) -> Self {
        match error {
            DelegateError::InvalidSubjectToken(error) => auth_error_to_wyrd(error),
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
            DelegateError::Wyrd(error) => error,
            DelegateError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
        }
    }
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use secrecy::SecretString;
    use sqlx::types::Json;
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_dev_fixtures::cards::seed_backing_card;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{PrincipalId, RbacCheck, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalKindTag, RequestedSubject};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_sql::TenantConn;

    use crate::credential_verify;
    use wyrd_sql::queries::auth::ApiKeyStatus;

    use super::{
        DelegateError, DelegateToken, ExchangeApiKey, ExchangeError, TokenExchangeSettings,
    };
    use crate::issue_api_key::WyrdApiKey;
    use crate::permission_resolver::SqlPermissionResolver;
    use crate::seed::seed_builtin_roles_for_tenant;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    fn test_service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("test-service").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
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
        let user_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(user_id)
        .bind(tenant_id.as_uuid())
        .bind(format!("test-{user_id}@example.com"))
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
        seed_backing_card(conn, card_ref, created_by).await;
        let sa_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref, space, name, version, status, created_by)
             VALUES ($1, $2, 'service', 'Service', $3, $4, $5, $6, $7, 'active', $8)",
        )
        .bind(sa_id)
        .bind(tenant_id.as_uuid())
        .bind(Uuid::now_v7())
        .bind(Json(card_ref.clone()))
        .bind(
            card_ref
                .space
                .as_ref()
                .expect("fixture service card ref has a resolved space")
                .as_str(),
        )
        .bind(format!("svc-{sa_id}"))
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
                 (id, data_tenant_id, principal_id, prefix, key_hash, created_by, expires_at, revoked_at)
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

    /// Render one refusal the way the HTTP boundary would, for comparison.
    ///
    /// Compares the whole projected problem rather than a field, because the
    /// property under test is that two refusals are indistinguishable — and any
    /// field that differs is a field a caller can read.
    fn rendered(error: &WyrdError) -> serde_json::Value {
        serde_json::to_value(error.problem()).expect("a problem serializes")
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
            space: Some(SpaceName::new("prod").expect("static space")),
            uid: None,
        };
        let id_a: PrincipalId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"
            .parse()
            .expect("static id");
        let id_b: PrincipalId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b03"
            .parse()
            .expect("static id");
        let initiator_ref = make_card("initiator");
        let a = DelegationStep {
            principal: RuntimePrincipalRef {
                id: id_a,
                kind: PrincipalKind::Service {
                    card_ref: Some(initiator_ref.clone()),
                    card_ref_scope: CardRefScope::own(&initiator_ref),
                },
            },
        };
        let immediate_ref = make_card("immediate");
        let b = DelegationStep {
            principal: RuntimePrincipalRef {
                id: id_b,
                kind: PrincipalKind::Service {
                    card_ref: Some(immediate_ref.clone()),
                    card_ref_scope: CardRefScope::own(&immediate_ref),
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

    /// A machine credential exchange mints no refresh token and no refresh row.
    ///
    /// Machine clients renew by presenting their durable API key again, so the
    /// grant has nothing to rotate. The response field has to stay absent and —
    /// the half a response assertion cannot see — the transaction must leave
    /// `wyrd.auth_refresh_tokens` empty, because a stored row would be a
    /// long-lived credential nobody ever asked for and nobody rotates.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn api_key_exchange_issues_no_refresh_token_or_row() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &card_ref).await;

        let key = WyrdApiKey::generate(tenant);
        let hash = wyrd_auth_issue::hash_api_key(&key.secret).expect("api key hashes");
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, created_by, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, now() + interval '1 day')",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&key.prefix)
        .bind(&hash)
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("live api key inserts");

        let exchanged = exchange_service()
            .execute(&mut conn, key.secret, "req-api-key-no-refresh")
            .await
            .expect("a live api key exchanges");

        assert!(
            exchanged.refresh_token.is_none(),
            "an api-key exchange must not issue a refresh token"
        );

        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.auth_refresh_tokens WHERE principal_id = $1",
        )
        .bind(sa_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("refresh row count runs");
        assert_eq!(rows, 0, "an api-key exchange stores no refresh row");
    }

    /// Every invalid API-key condition renders the same public problem.
    ///
    /// The reason used to be projected in `details.reason`, which handed an
    /// unauthenticated caller the enumeration oracle the constant-cost
    /// verification exists to close: `revoked` means the prefix exists,
    /// `not_found` means it does not. Comparing the full rendered problem is the
    /// point — a single differing field is a readable field.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn every_invalid_api_key_condition_renders_one_problem() {
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

        let expected = rendered(&super::api_key_invalid());
        for (prefix, error) in [
            ("direct", ExchangeError::CrossTenant),
            ("missing-prefix", ExchangeError::NotFound),
            ("revoked-prefix", ExchangeError::NotFound),
            ("expired-prefix", ExchangeError::NotFound),
            ("direct", ExchangeError::AccountDisabled),
            ("direct", ExchangeError::HashMismatch),
            ("direct", ExchangeError::TenantNotAdmitting),
        ] {
            let mapped = super::map_exchange_error_to_wyrd(&mut conn, prefix, error).await;
            assert_eq!(
                rendered(&mapped),
                expected,
                "the {prefix} refusal is distinguishable from the others"
            );
        }

        // The operator still gets the distinction, in the log line.
        assert_eq!(
            super::reason_from_api_key_status(Err(sqlx::Error::RowNotFound), "prefix"),
            "backend_unavailable"
        );
    }

    /// Give a suspended service account one live API key.
    ///
    /// The suspended-principal refusal needs a credential row that the prefix
    /// lookup finds and then rejects on status, which is a different shape from
    /// the lifecycle rows [`insert_lifecycle_key`] seeds.
    async fn seed_suspended_account_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        created_by: Uuid,
        base_card: &CardRef,
        prefix: &str,
    ) {
        let card_ref = CardRef {
            name: CardName::new("suspended-exchange-subject").expect("static name"),
            ..base_card.clone()
        };
        let sa_id = insert_test_service_account(conn, tenant, created_by, &card_ref).await;
        sqlx::query("UPDATE wyrd.auth_service_accounts SET status = 'suspended' WHERE id = $1")
            .bind(sa_id)
            .execute(&mut **conn.transaction())
            .await
            .expect("service account suspends");
        insert_lifecycle_key(
            conn,
            tenant,
            sa_id,
            created_by,
            prefix,
            Utc::now() + Duration::days(1),
            false,
        )
        .await;
    }

    /// Every refusal path pays for exactly one Argon2 verification.
    ///
    /// The refusal-then-verify order in [`ExchangeApiKey::execute`] is what makes
    /// the paths indistinguishable by clock: a malformed key, another tenant's
    /// key, a tenant that no longer admits credentials, an unknown prefix, a
    /// disabled account, a revoked or expired key, and a live prefix with the
    /// wrong tail must all do the same work. Counting verifications is the only
    /// way to assert that without measuring wall-clock time, which is unstable
    /// under a shared test Postgres.
    ///
    /// The counter is process-global; `cargo nextest` runs each test in its own
    /// process, so the deltas below belong to this test alone.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn every_invalid_api_key_costs_exactly_one_verification() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();

        let unknown = WyrdApiKey::generate(tenant);
        let disabled = WyrdApiKey::generate(tenant);
        let revoked = WyrdApiKey::generate(tenant);
        let wrong_tail = WyrdApiKey::generate(tenant);
        let foreign = WyrdApiKey::generate(DataTenantId::new_v7());
        // A tenant absent from `platform.tenants` does not admit credentials,
        // which is the same answer a suspended one gives.
        let unadmitted_tenant = DataTenantId::new_v7();
        let unadmitted = WyrdApiKey::generate(unadmitted_tenant);

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let live_sa = insert_test_service_account(&mut conn, tenant, user_id, &card_ref).await;

        insert_lifecycle_key(
            &mut conn,
            tenant,
            live_sa,
            user_id,
            &revoked.prefix,
            Utc::now() + Duration::days(1),
            true,
        )
        .await;
        insert_lifecycle_key(
            &mut conn,
            tenant,
            live_sa,
            user_id,
            &wrong_tail.prefix,
            Utc::now() + Duration::days(1),
            false,
        )
        .await;

        seed_suspended_account_key(&mut conn, tenant, user_id, &card_ref, &disabled.prefix).await;

        let expected = rendered(&super::api_key_invalid());
        let service = exchange_service();
        let cases = [
            ("malformed", SecretString::from("not-a-wyrd-api-key")),
            ("cross_tenant", foreign.secret),
            ("unknown_prefix", unknown.secret),
            ("suspended_account", disabled.secret),
            ("revoked", revoked.secret),
            ("wrong_tail", wrong_tail.secret),
        ];
        for (label, presented) in cases {
            let before = credential_verify::verifications_performed();
            let error = service
                .execute(&mut conn, presented, &format!("req-{label}"))
                .await
                .expect_err("an invalid api key is refused");
            assert_eq!(
                credential_verify::verifications_performed() - before,
                1,
                "the {label} path did not perform exactly one verification"
            );
            let prefix = "probe";
            assert_eq!(
                rendered(&super::map_exchange_error_to_wyrd(&mut conn, prefix, error).await),
                expected,
                "the {label} refusal is distinguishable"
            );
        }

        // Admission is decided on the presented key's own tenant, so it needs a
        // connection bound to that tenant rather than the fixture's.
        let mut unadmitted_conn = fixture
            .tenant_conn_for(unadmitted_tenant)
            .await
            .expect("unadmitted tenant conn opens");
        let before = credential_verify::verifications_performed();
        let error = service
            .execute(&mut unadmitted_conn, unadmitted.secret, "req-unadmitted")
            .await
            .expect_err("a tenant that does not admit credentials is refused");
        assert!(matches!(error, ExchangeError::TenantNotAdmitting));
        assert_eq!(
            credential_verify::verifications_performed() - before,
            1,
            "the unadmitted-tenant path did not perform exactly one verification"
        );
        assert_eq!(
            rendered(
                &super::map_exchange_error_to_wyrd(&mut unadmitted_conn, "probe", error).await
            ),
            expected,
            "the unadmitted-tenant refusal is distinguishable"
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
        let result = exchange_service()
            .execute(&mut conn, key.secret, "req-cross-tenant")
            .await;

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
                 (id, data_tenant_id, principal_id, prefix, key_hash, created_by, expires_at, revoked_at)
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

        let result = exchange_service()
            .execute(&mut conn, key.secret, "req-revoked")
            .await;

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
                 (id, data_tenant_id, principal_id, prefix, key_hash, created_by, expires_at)
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

        let result = exchange_service()
            .execute(&mut conn, key.secret, "req-hash-mismatch")
            .await;

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
            .issue_card_access_token(
                TokenPrincipalRef {
                    id: PrincipalId::new(Uuid::new_v4()),
                    kind: PrincipalKindTag::Service,
                    tenant_id: tenant,
                    card_ref: Some(test_service_card_ref()),
                    card_ref_scope: CardRefScope::default(),
                },
                vec![],
                None,
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

    #[tokio::test]
    async fn delegation_issues_no_refresh_token() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        // Seed the per-tenant builtin roles so the resolver maps the delegator's
        // `runtime_admin` role claim to `delegation_issue`, and insert the target
        // Service principal the exchange resolves by card_ref.
        let target_card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("delegation-target").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        };
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        let creator = insert_test_user(&mut conn, tenant).await;
        insert_test_service_account(&mut conn, tenant, creator, &target_card_ref).await;
        conn.commit().await.expect("seed commits");

        let issuing_key = test_issuing_key();
        let subject_token = issuing_key
            .issue_card_access_token(
                TokenPrincipalRef {
                    id: PrincipalId::new(Uuid::new_v4()),
                    kind: PrincipalKindTag::Service,
                    tenant_id: tenant,
                    card_ref: Some(test_service_card_ref()),
                    card_ref_scope: CardRefScope::default(),
                },
                vec![RoleRef::new("runtime_admin").expect("role name is valid")],
                None,
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
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_refresh_tokens")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("refresh row count runs");
        assert_eq!(rows, 0, "delegation stores no refresh row either");
        assert_eq!(exchanged.token_type, wyrd_spec::auth::TokenType::Bearer);
    }
}
