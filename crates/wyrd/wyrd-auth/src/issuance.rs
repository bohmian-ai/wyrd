//! The one tenant access-token issuance workflow.
//!
//! API-key exchange, OIDC login, human refresh, workload `jwt-bearer`, and RFC
//! 8693 delegation each verify their own grant-specific evidence, then call
//! [`TenantTokenIssuer::issue`]. That owner alone loads the current tenant and
//! principal, refuses an inactive one, resolves the principal's current grants
//! to one [`PermissionSet`], signs the five-minute token, and records the
//! issuance audit — so a grant change or suspension governs the very next
//! token on every path, and no path can drift from the others.
//!
//! The tenant SYSTEM principal is an attribution identity only: no path here
//! mints a token for it, and every public grant refuses it as inactive.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_issue::{AccessGrant, IssueError, IssuingKey};
use wyrd_auth_verify::{ActClaim, TokenAudience, TokenPrincipalRef, VerifiedToken};
use wyrd_runtime::audit::AuditStage;
use wyrd_runtime::{Permission, PermissionSet, PrincipalId, RoleRef, UnrepresentableIntersection};
use wyrd_spec::auth::{
    Attribution, CardAccess, ExchangeTokenType, FamilyRestriction, PrincipalKindTag, SecretBearer,
    TokenResponse, TokenType, WorkloadFamily,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRefScope;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_spec::vala::audit_detail::AuditDelegationStep;
use wyrd_spec::vala::audit_detail::CardScopeMintKind;
use wyrd_sql::queries::auth::{
    RoleRow, human_connection_is_active, insert_human_refresh_token, list_service_account_roles,
    list_user_roles, lock_human_connection_slot, lock_refresh_family, refresh_issuance_instant,
    roles_by_name, service_account_by_id, user_by_id,
};
use wyrd_sql::queries::platform::tenant_resolver::tenant_admits_credentials;
use wyrd_sql::queries::verification::record_machine_authentication;
use wyrd_sql::row_types::auth::{HumanConnectionBinding, HumanSessionBinding};
use wyrd_sql::{SqlError, TenantConn};

use crate::audit::{TOKEN_EXCHANGE_OPERATION, auth_event};
use crate::card_scope::{
    IssueErrorOrWyrd, MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_JWT_BEARER, issue_scope_error,
    resolve_workload_scope, stage_scope_mint_success_audit,
};
use crate::error::{auth_error_to_wyrd, store_error};
use crate::exchange_api_key::{principal_kind_wire, role_refs};
use wyrd_auth_issue::hash_secret;

/// Tenant token lifetimes.
#[derive(Debug, Clone)]
pub struct TokenExchangeSettings {
    /// Access token lifetime: the bound on how long a revoked credential,
    /// suspended principal, or withdrawn grant stays spendable.
    pub access_ttl: Duration,
    /// Lifetime of a `wyrd-cli` refresh token; each rotation starts a new one.
    pub refresh_ttl: Duration,
}

impl Default for TokenExchangeSettings {
    fn default() -> Self {
        Self {
            access_ttl: Duration::minutes(5),
            refresh_ttl: Duration::days(30),
        }
    }
}

/// A token issued by the workflow, before it becomes a wire response.
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
    pub expires_at: DateTime<Utc>,
}

impl ExchangedToken {
    /// Convert to the RFC 6749 §5.1 token response, with `expires_in` the
    /// whole seconds left until the access token's expiry.
    #[must_use]
    pub fn into_response(self) -> TokenResponse {
        TokenResponse {
            access_token: SecretBearer::new(self.access_token.expose_secret().to_owned()),
            token_type: self.token_type,
            expires_in: u64::try_from((self.expires_at - Utc::now()).num_seconds()).unwrap_or(0),
            refresh_token: self
                .refresh_token
                .map(|token| SecretBearer::new(token.expose_secret().to_owned())),
            issued_token_type: None,
        }
    }

    /// Convert to the RFC 8693 §2.2.1 token-exchange response: the RFC 6749
    /// §5.1 response with `issued_token_type` naming the access token.
    #[must_use]
    pub fn into_exchange_response(self) -> TokenResponse {
        TokenResponse {
            issued_token_type: Some(ExchangeTokenType::AccessToken),
            ..self.into_response()
        }
    }
}

/// The grant a tenant access token is issued under, after its entry path has
/// verified the grant-specific evidence.
///
/// Closed on purpose: these are the complete tenant issuance paths. The
/// variant decides which principal table is read, the credential attribution,
/// the selected workload root, and the Card-scope mint kind — nothing else
/// differs between paths.
#[derive(Debug)]
pub enum TenantGrant {
    /// A verified tenant API key.
    ApiKey {
        /// The API-key row the holder presented.
        credential_id: Uuid,
        /// Explicitly selected execution root, validated against the owning
        /// account's restriction; `None` authenticates the account rootless.
        workload_uid: Option<CardUid>,
    },
    /// A live browser session created with this API key, re-resolved on one
    /// request.
    ///
    /// Confers exactly the authority the key's own exchange would, but the
    /// session's request is not the key's runtime activity, so resolving it
    /// writes nothing.
    BrowserRecovery {
        /// The API-key row the session was created with.
        credential_id: Uuid,
    },
    /// A verified federated OIDC login.
    OidcLogin,
    /// A consumed human refresh token.
    Refresh {
        /// The refresh row presented, consumed by a `wyrd-cli` rotation.
        consumed: Uuid,
    },
    /// A verified workload `jwt-bearer` assertion.
    JwtBearer {
        /// Root chosen by the explicit request UID, else the binding's pinned
        /// default; `None` authenticates the mapped account rootless.
        workload_uid: Option<CardUid>,
    },
    /// An RFC 8693 exchange: the issued principal is the actor, who acts on
    /// behalf of the verified subject.
    ///
    /// The issuer resolves the actor's current row and grants; the token names
    /// the subject as its principal and the actor as its outermost `act`.
    Delegation {
        /// The subject being acted for, from its verified access token; boxed
        /// to keep the grant enum small.
        subject: Box<TokenPrincipalRef>,
        /// The subject's roles; informational metadata only.
        subject_roles: Vec<RoleRef>,
        /// The subject's verified permissions. The token carries only the
        /// actor's current permissions that this set also covers, so
        /// delegation can narrow authority but never amplify either party.
        subject_permissions: PermissionSet,
        /// The subject's signed Card limit, intersected with the actor's
        /// re-resolved root graph.
        subject_card_access: CardAccess,
        /// The subject's own signed root, kept unchanged in the new token.
        subject_workload_uid: Option<CardUid>,
        /// The root the actor's own direct token authenticated; the issuer
        /// revalidates it under the actor's current restriction and never
        /// selects another.
        actor_workload_uid: CardUid,
        /// Earlier actors from the subject token, nested inside the new actor.
        prior_act: Option<Box<ActClaim>>,
        /// Audience the delegated token is issued for.
        audience: TokenAudience,
        /// Credential that authenticated the actor's token, when one did.
        ///
        /// Non-secret provenance: it attributes the exchange to the actor's
        /// credential and travels as the actor layer's `cid`, never the
        /// top-level one.
        actor_credential_id: Option<Uuid>,
    },
}

impl TenantGrant {
    /// Whether the grant names a human user rather than a machine principal.
    fn is_human(&self) -> bool {
        matches!(self, Self::OidcLogin | Self::Refresh { .. })
    }

    /// The stored credential the token is attributed to, when there is one.
    fn credential_id(&self) -> Option<Uuid> {
        match self {
            Self::ApiKey { credential_id, .. } | Self::BrowserRecovery { credential_id } => {
                Some(*credential_id)
            }
            Self::Refresh { consumed } => Some(*consumed),
            Self::OidcLogin | Self::JwtBearer { .. } | Self::Delegation { .. } => None,
        }
    }

    /// The root the issued principal's own Card access is resolved under:
    /// the direct grant's selection, or a delegation actor's authenticated
    /// root. Human and browser-recovery grants select none.
    fn selected_root(&self) -> Option<&CardUid> {
        match self {
            Self::ApiKey { workload_uid, .. } | Self::JwtBearer { workload_uid } => {
                workload_uid.as_ref()
            }
            Self::Delegation {
                actor_workload_uid, ..
            } => Some(actor_workload_uid),
            Self::OidcLogin | Self::Refresh { .. } | Self::BrowserRecovery { .. } => None,
        }
    }

    /// Whether this grant, once it validates a root, is that root's runtime
    /// activity for the account.
    ///
    /// Only a machine's own direct durable-credential exchange — an API key
    /// or a workload `jwt-bearer` assertion — proves a release is running.
    /// Delegation acts for another caller, and human and browser grants
    /// select no root.
    fn records_owner_activity(&self) -> bool {
        matches!(self, Self::ApiKey { .. } | Self::JwtBearer { .. })
    }

    /// The Card-scope mint kind a Card-bound machine grant records; `None`
    /// for human grants, which bind no Card.
    fn scope_mint_kind(&self) -> Option<CardScopeMintKind> {
        match self {
            Self::ApiKey { .. } => Some(MINT_KIND_API_KEY_EXCHANGE),
            Self::JwtBearer { .. } => Some(MINT_KIND_JWT_BEARER),
            // The subject's Card scope was minted with its own token; the
            // exchange confers no new emit authority.
            Self::OidcLogin
            | Self::Refresh { .. }
            | Self::Delegation { .. }
            | Self::BrowserRecovery { .. } => None,
        }
    }

    /// Build the signed-token contents for `issued`, whose current `roles`,
    /// `permissions`, and own Card `access` were just resolved.
    ///
    /// Every grant but a delegation names `issued` with its own authority,
    /// its selected root, and the `wyrd` audience. A delegation names the
    /// subject instead, keeps the subject's root, carries only the exact
    /// intersections of `issued`'s permissions and Card access with the
    /// subject's, and records `issued` — with its root and credential
    /// provenance — as the outermost actor over any earlier actors.
    ///
    /// # Errors
    /// Returns [`IssuanceError::UnrepresentableDelegation`] when a
    /// delegation's exact shared authority has no token representation, and
    /// [`IssuanceError::Wyrd`] with [`WyrdError::WorkloadContextRequired`]
    /// when the subject is a restricted account that has selected no root;
    /// the caller then signs nothing.
    fn into_access_grant(
        self,
        issued: TokenPrincipalRef,
        roles: Vec<RoleRef>,
        permissions: PermissionSet,
        access: CardAccess,
    ) -> Result<AccessGrant, IssuanceError> {
        let credential_id = self.credential_id();
        let workload_uid = self.selected_root().cloned();
        Ok(match self {
            Self::Delegation {
                subject,
                subject_roles,
                subject_permissions,
                subject_card_access,
                subject_workload_uid,
                actor_workload_uid,
                prior_act,
                audience,
                actor_credential_id,
            } => AccessGrant {
                principal: *subject,
                roles: subject_roles,
                permissions: permissions.intersection(&subject_permissions)?,
                card_access: subject_card_access
                    .intersection(&access)
                    .ok_or_else(subject_context_required)?,
                workload_uid: subject_workload_uid,
                credential_id,
                act: Some(Box::new(ActClaim {
                    sub: issued.id.to_string(),
                    principal: issued,
                    workload_uid: actor_workload_uid,
                    cid: actor_credential_id.map(|id| id.to_string()),
                    act: prior_act,
                })),
                audience,
            },
            _ => AccessGrant {
                principal: issued,
                roles,
                permissions,
                card_access: access,
                workload_uid,
                credential_id,
                act: None,
                audience: TokenAudience::Wyrd,
            },
        })
    }
}

/// The refusal for delegating from a restricted subject that has selected no
/// root: it holds no Card authority to delegate.
fn subject_context_required() -> IssuanceError {
    IssuanceError::Wyrd(WyrdError::workload_context_required(
        "the subject token is restricted to one workload family and selected no root",
    ))
}

/// A principal's resolved current authority, before any root is selected.
struct ResolvedPrincipal {
    /// The account reference the token names.
    principal: TokenPrincipalRef,
    /// Current role assignments.
    roles: Vec<RoleRef>,
    /// Union of the roles' current permissions.
    permissions: PermissionSet,
    /// The account's durable restriction; `None` when unrestricted or human.
    restriction: Option<WorkloadFamily>,
}

/// The Card context a selected root, or its absence, yields.
struct ResolvedContext {
    /// The signed Card access of the issued principal itself.
    access: CardAccess,
    /// The root's resolved graph, for the scope-mint audit.
    scope: Option<CardRefScope>,
}

impl ResolvedContext {
    /// Resolve the context for `root` under `restriction`.
    ///
    /// A selected root must be usable and yields its exact graph as
    /// `scoped`; without one a restricted account is `root_required` and any
    /// other caller `any_registered`.
    ///
    /// # Errors
    /// Every [`resolve_workload_scope`] refusal for a selected root.
    async fn resolve(
        conn: &mut TenantConn<'_>,
        root: Option<&CardUid>,
        restriction: Option<&WorkloadFamily>,
    ) -> Result<Self, WyrdError> {
        let Some(root) = root else {
            return Ok(Self {
                access: restriction.map_or(CardAccess::AnyRegistered, |family| {
                    CardAccess::RootRequired {
                        restriction: FamilyRestriction::Family(family.clone()),
                    }
                }),
                scope: None,
            });
        };
        let scope = resolve_workload_scope(conn, root, restriction).await?;
        Ok(Self {
            access: CardAccess::Scoped {
                cards: scope
                    .as_slice()
                    .iter()
                    .filter_map(|m| m.uid.clone())
                    .collect(),
            },
            scope: Some(scope),
        })
    }
}

/// Narrow a verified bearer to the workload root its request selects.
///
/// Returns `None` when the bearer already signs exactly `root`, which leaves
/// it unchanged. A rootless bearer's root is resolved under its signed Card
/// access alone, never broader live grants: a `root_required` account's
/// family must allow it, and any other access is intersected with the root's
/// exact graph, so the selector only ever narrows. Identity, grants, and
/// tenant never change, and no runtime activity is recorded.
///
/// # Errors
/// Returns [`WyrdError::SignedWorkloadMismatch`] when the bearer pins another
/// root, and every [`resolve_workload_scope`] refusal for `root`, unknown and
/// foreign roots alike.
pub async fn select_request_root(
    conn: &mut TenantConn<'_>,
    verified: &VerifiedToken,
    root: &CardUid,
) -> Result<Option<VerifiedToken>, WyrdError> {
    if !wyrd_runtime::needs_root_resolution(&verified.principal, &verified.delegation_chain, root)?
    {
        return Ok(None);
    }
    let signed = &verified.principal.card_access;
    let restriction = match signed {
        CardAccess::RootRequired {
            restriction: FamilyRestriction::Family(family),
        } => Some(family),
        CardAccess::AnyRegistered | CardAccess::Scoped { .. } => None,
    };
    let selected = ResolvedContext::resolve(conn, Some(root), restriction)
        .await?
        .access;
    let access = signed.intersection(&selected).unwrap_or(selected);
    Ok(Some(VerifiedToken {
        principal: verified
            .principal
            .clone()
            .with_card_access(access, Some(root.clone())),
        delegation_chain: verified.delegation_chain.clone(),
        exp: verified.exp,
    }))
}

/// Tenant issuance refusal or failure.
///
/// Entry paths map the refusals onto their own stable public errors (an API
/// key refusal must stay indistinguishable from a wrong secret); the
/// [`From`] conversion is the default for paths with no such constraint.
#[derive(Debug, thiserror::Error)]
pub enum IssuanceError {
    /// The tenant is not in a state that admits credentials.
    #[error("tenant does not admit credentials")]
    TenantNotAdmitting,
    /// The principal does not exist, is suspended or deleted, or has an
    /// unrecognized kind.
    #[error("principal is not active")]
    PrincipalInactive,
    /// A stored role name or permission document is corrupt.
    #[error("role {role} is corrupt")]
    RoleCorrupt {
        /// The corrupt role's stored name.
        role: String,
    },
    /// Token signing rejected the grant.
    #[error("token issue failed")]
    Issue(#[from] IssueError),
    /// A store read or write failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    /// A catalog read through a `SqlError`-returning store query failed, or
    /// returned a row that breaks a stored invariant.
    #[error("store operation failed")]
    Store(#[from] SqlError),
    /// The human connection revision a session is bound to is no longer
    /// Active, so no session may be issued or renewed through it.
    #[error("human connection is no longer active")]
    ConnectionInactive,
    /// A Card-scope or audit step failed with its own stable error.
    #[error("wyrd error")]
    Wyrd(#[from] WyrdError),
    /// A delegation's exact shared authority has no token representation,
    /// so no token was signed.
    #[error("delegated permissions cannot be represented")]
    UnrepresentableDelegation(#[from] UnrepresentableIntersection),
}

impl From<IssuanceError> for WyrdError {
    /// Maps each issuance refusal to its public catalog error: inactive
    /// principals and tenants to revoked credentials, an unrepresentable
    /// delegation to its own refusal naming the permission (no token was
    /// issued), store failures to a retryable unavailable error, and other issue
    /// failures to a logged internal error.
    fn from(error: IssuanceError) -> Self {
        match error {
            IssuanceError::TenantNotAdmitting | IssuanceError::PrincipalInactive => {
                WyrdError::CredentialRevoked {
                    message: "principal or tenant is not active".to_owned(),
                    details: json!({}),
                }
            }
            IssuanceError::ConnectionInactive => WyrdError::InvalidToken {
                message: "the login connection is no longer active".to_owned(),
                details: json!({}),
            },
            IssuanceError::RoleCorrupt { role } => WyrdError::RoleCorrupt {
                message: format!("role {role} has corrupt permissions"),
                details: json!({ "role": role }),
            },
            IssuanceError::Issue(IssueError::DelegationDepthExceeded { max }) => {
                WyrdError::DelegationDepthExceededIssue {
                    message: format!("delegation chain would exceed max depth of {max}"),
                    details: json!({ "max": max }),
                }
            }
            IssuanceError::Issue(IssueError::CardScopeTooLarge { encoded_len, limit }) => {
                WyrdError::CardScopeTooLarge {
                    message: format!("encoded token length {encoded_len} exceeds {limit}"),
                    details: json!({ "encoded_len": encoded_len, "limit": limit }),
                }
            }
            IssuanceError::Issue(error) => {
                tracing::warn!(error = %error, "tenant token issue failed");
                WyrdError::Internal {
                    message: "token issue failed".to_owned(),
                    details: json!({}),
                }
            }
            IssuanceError::Database(_) | IssuanceError::Store(_) => {
                tracing::warn!(error = %error, "tenant token issuance store unavailable");
                WyrdError::AuthVerifyUnavailable {
                    message: "auth backend unavailable".to_owned(),
                    details: json!({ "retry_after_seconds": 1 }),
                }
            }
            IssuanceError::Wyrd(error) => error,
            IssuanceError::UnrepresentableDelegation(error) => {
                WyrdError::UnrepresentableDelegation {
                    message: format!("{error}; the exchange issued no token"),
                    details: json!({ "permission": error.permission }),
                }
            }
        }
    }
}

/// The one tenant access-token issuance owner.
///
/// Holds the signing key, token lifetimes, and the process audit stage; every
/// tenant access token is minted through [`Self::issue`] on the caller's
/// [`TenantConn`], so the token and its credential bookkeeping commit
/// together. The issuance decision is staged on the outbox and never waits on,
/// or fails with, its audit commit.
#[derive(Clone)]
pub struct TenantTokenIssuer {
    /// Key every tenant access and refresh token is signed with.
    issuing_key: Arc<IssuingKey>,
    /// Access and refresh lifetimes.
    settings: TokenExchangeSettings,
    /// Process audit stage every grant decision is staged on.
    audit: Arc<dyn AuditStage>,
}

impl std::fmt::Debug for TenantTokenIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantTokenIssuer")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl TenantTokenIssuer {
    /// Construct the issuer over the deployment signing key and the process
    /// audit outbox its grant decisions are staged on.
    #[must_use]
    pub fn new(
        issuing_key: Arc<IssuingKey>,
        settings: TokenExchangeSettings,
        audit: Arc<dyn AuditStage>,
    ) -> Self {
        Self {
            issuing_key,
            settings,
            audit,
        }
    }

    /// The process audit stage shared by every grant built on this issuer.
    ///
    /// Grant services that record their own refusals (refresh reuse, refused
    /// delegation, failed OIDC or workload exchange) stage them here, so every
    /// auth decision reaches the one audit write path.
    #[must_use]
    pub fn audit(&self) -> &dyn AuditStage {
        &*self.audit
    }

    /// Mint a tenant access token for `principal_id` under `grant`.
    ///
    /// Reads, in the caller's transaction: whether the tenant admits
    /// credentials; the principal's current row (a user for human grants, a
    /// tenant machine principal otherwise), refusing anything not active; the
    /// principal's current role assignments and those roles' permissions; and
    /// its Card access: a selected root's exact graph under the account's
    /// restriction, else rootless semantics. It then signs a token carrying
    /// that authority for the configured access TTL and stages the canonical
    /// token-exchange audit (plus the Card-scope mint audit for a selected
    /// root) on the process audit stage without waiting.
    ///
    /// For a [`TenantGrant::Delegation`], `principal_id` is the actor: the
    /// token instead names the grant's subject as its principal, carries the
    /// intersection of the actor's current permissions with the subject's, and
    /// records the actor as its outermost `act`.
    ///
    /// An API-key or workload `jwt-bearer` grant that selects a root, by an
    /// account that is that root family's runtime account, is the root's
    /// runtime activity for the account. No other grant — delegation, OIDC
    /// login, human refresh — and no rootless grant touches activity.
    ///
    /// Nothing is committed here; the caller commits or rolls back the grant
    /// whole.
    ///
    /// # Errors
    /// Returns [`IssuanceError::TenantNotAdmitting`] or
    /// [`IssuanceError::PrincipalInactive`] for an inactive tenant or
    /// principal, [`IssuanceError::RoleCorrupt`] for a corrupt role,
    /// [`IssuanceError::Issue`] when signing rejects the grant,
    /// [`IssuanceError::Database`] when a read or the activity write fails,
    /// [`IssuanceError::Wyrd`] when Card-scope resolution fails, and
    /// [`IssuanceError::UnrepresentableDelegation`] when a delegation's exact
    /// shared authority has no token representation, before anything is
    /// signed.
    #[tracing::instrument(level = "debug", skip(self, conn, grant), fields(principal_id = %principal_id), err)]
    pub async fn issue(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        grant: TenantGrant,
        request_id: &str,
    ) -> Result<ExchangedToken, IssuanceError> {
        let tenant = conn.data_tenant_id();
        let (principal, context) = self.authorize(conn, principal_id, &grant).await?;
        let expires_at = Utc::now() + self.settings.access_ttl;
        let kind = principal.principal.kind;
        let scope_root = context
            .scope
            .as_ref()
            .and_then(|scope| scope.as_slice().first().cloned());
        let scope_mint = grant.scope_mint_kind().zip(context.scope);
        let event = exchange_audit_event(&principal.principal, &grant, expires_at, request_id);
        let access_token = self
            .issuing_key
            .issue_access_token(
                grant.into_access_grant(
                    principal.principal,
                    principal.roles,
                    principal.permissions,
                    context.access,
                )?,
                self.settings.access_ttl,
            )
            .map_err(|error| match scope_root {
                Some(root) => match issue_scope_error(error, &root) {
                    IssueErrorOrWyrd::Issue(error) => IssuanceError::Issue(error),
                    IssueErrorOrWyrd::Wyrd(error) => IssuanceError::Wyrd(error),
                },
                None => IssuanceError::Issue(error),
            })?;

        // Two decisions, two records. The exchange says a grant was spent to
        // obtain a token and which credential; the scope mint says what emit
        // authority the selected root conferred.
        self.audit.stage(tenant, event);
        if let Some((mint_kind, scope)) = scope_mint {
            stage_scope_mint_success_audit(
                &*self.audit,
                tenant,
                principal_id,
                kind,
                &scope,
                request_id,
                mint_kind,
            );
        }

        Ok(ExchangedToken {
            access_token: SecretString::from(access_token),
            refresh_token: None,
            token_type: TokenType::Bearer,
            expires_at,
        })
    }

    /// Resolve the authority a tenant credential confers on `principal_id`,
    /// without minting a token.
    ///
    /// The per-request form of [`Self::issue`]: a caller that authenticates a
    /// durable credential on every request gets the [`VerifiedToken`] the
    /// exchanged token would have yielded, built from the same resolution and
    /// claim shaping, with the same root-activity side effect. Nothing is
    /// signed, so no token-exchange or scope-mint audit is staged; the
    /// request's own permission checks record its decisions.
    ///
    /// # Errors
    /// Every [`Self::issue`] error except an encoded-size refusal, plus
    /// [`IssuanceError::Wyrd`] when the shaped claims do not convert into a
    /// verified token.
    #[tracing::instrument(level = "debug", skip(self, conn, grant), fields(principal_id = %principal_id), err)]
    pub async fn verify(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        grant: TenantGrant,
    ) -> Result<VerifiedToken, IssuanceError> {
        let (principal, context) = self.authorize(conn, principal_id, &grant).await?;
        let access = grant.into_access_grant(
            principal.principal,
            principal.roles,
            principal.permissions,
            context.access,
        )?;
        self.verified(access)
    }

    /// Resolve the fresh authority a scheduled run of the exact `root` is
    /// accepted under, as the family's runtime `account`.
    ///
    /// The account is read as a Service account and resolved as
    /// [`Self::verify`] resolves a direct grant — tenant admission, active
    /// status, current roles and permissions — and `root` is revalidated
    /// under the account's current restriction, yielding that root's exact
    /// graph. Nothing is presented, signed, or recorded: the result carries no
    /// credential and no delegation, and no activity or audit is written.
    ///
    /// # Errors
    /// Every [`Self::verify`] error except the activity write.
    #[tracing::instrument(level = "debug", skip(self, conn), fields(account = %account, root = %root), err)]
    pub async fn scheduled(
        &self,
        conn: &mut TenantConn<'_>,
        account: Uuid,
        root: &CardUid,
    ) -> Result<VerifiedToken, IssuanceError> {
        let principal = self.resolve(conn, account, false).await?;
        let context =
            ResolvedContext::resolve(conn, Some(root), principal.restriction.as_ref()).await?;
        self.verified(AccessGrant {
            principal: principal.principal,
            roles: principal.roles,
            permissions: principal.permissions,
            card_access: context.access,
            workload_uid: Some(root.clone()),
            credential_id: None,
            act: None,
            audience: TokenAudience::Wyrd,
        })
    }

    /// Shape `access` into the [`VerifiedToken`] its signed token would yield.
    ///
    /// # Errors
    /// [`IssuanceError::Issue`] when the claims are unrepresentable and
    /// [`IssuanceError::Wyrd`] when they do not convert.
    fn verified(&self, access: AccessGrant) -> Result<VerifiedToken, IssuanceError> {
        let claims = self
            .issuing_key
            .access_claims(access, self.settings.access_ttl)?;
        Ok(claims.into_verified().map_err(auth_error_to_wyrd)?)
    }

    /// Resolve `principal_id` and the Card context `grant` selects, recording
    /// root activity when the grant qualifies.
    ///
    /// Activity is recorded only for a direct machine grant whose root
    /// validated and whose account is that root family's runtime account, so
    /// an unrestricted caller of another family's release never activates it.
    ///
    /// # Errors
    /// As [`Self::issue`], less the signing errors.
    async fn authorize(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        grant: &TenantGrant,
    ) -> Result<(ResolvedPrincipal, ResolvedContext), IssuanceError> {
        let principal = self.resolve(conn, principal_id, grant.is_human()).await?;
        let root = grant.selected_root();
        let context = ResolvedContext::resolve(conn, root, principal.restriction.as_ref()).await?;
        if let Some(root) = root.filter(|_| grant.records_owner_activity()) {
            record_machine_authentication(conn, principal.principal.id, root).await?;
        }
        Ok((principal, context))
    }

    /// Resolve `principal_id` — a user when `human`, else a service account —
    /// with its current roles, permissions, and restriction.
    ///
    /// Refuses a tenant that does not admit credentials and a principal that
    /// is missing or not active.
    ///
    /// # Errors
    /// As [`Self::issue`], less the signing and Card-context errors.
    async fn resolve(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        human: bool,
    ) -> Result<ResolvedPrincipal, IssuanceError> {
        let tenant = conn.data_tenant_id();
        if !tenant_admits_credentials(conn, tenant).await? {
            return Err(IssuanceError::TenantNotAdmitting);
        }
        let (principal, restriction, role_names) = if human {
            let user = user_by_id(conn, principal_id)
                .await?
                .filter(|user| user.status == "active")
                .ok_or(IssuanceError::PrincipalInactive)?;
            let principal = TokenPrincipalRef {
                id: PrincipalId::new(user.id),
                kind: PrincipalKindTag::User,
                tenant_id: tenant,
            };
            (principal, None, list_user_roles(conn, user.id).await?)
        } else {
            let row = service_account_by_id(conn, principal_id)
                .await?
                .ok_or(IssuanceError::PrincipalInactive)?;
            let kind =
                principal_kind_wire(&row.principal_kind).ok_or(IssuanceError::PrincipalInactive)?;
            let principal = TokenPrincipalRef {
                id: PrincipalId::new(row.id),
                kind,
                tenant_id: tenant,
            };
            let restriction = row.workload_family()?;
            (
                principal,
                restriction,
                list_service_account_roles(conn, row.id).await?,
            )
        };
        let roles = role_refs(role_names).map_err(|error| IssuanceError::RoleCorrupt {
            role: error.to_string(),
        })?;
        let permissions = resolve_permissions(conn, &roles).await?;
        Ok(ResolvedPrincipal {
            principal,
            roles,
            permissions,
            restriction,
        })
    }

    /// Establish or renew a human session: an access token from
    /// [`Self::issue_human_access`] plus a new refresh token bound to the
    /// connection and OAuth client it logged in through.
    ///
    /// `rotated_from` is absent at first login and carries the consumed
    /// refresh row on a `wyrd-cli` rotation; it is both the family back-link
    /// that makes reuse detectable and the credential attribution of the
    /// renewed access token. The refresh token lives for the configured
    /// `refresh_ttl`. One `PostgreSQL` instant feeds
    /// both its signed `exp` and its durable row expiry. Only human sessions
    /// get a refresh token — a machine re-exchanges its durable credential
    /// instead.
    ///
    /// # Errors
    /// Returns every [`Self::issue_human_access`] error,
    /// [`IssuanceError::Issue`] when the refresh token cannot be signed, and
    /// [`IssuanceError::Database`] when a store step fails. Nothing is
    /// committed here.
    pub async fn issue_human_session(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        rotated_from: Option<Uuid>,
        session: HumanSessionBinding,
        request_id: &str,
    ) -> Result<ExchangedToken, IssuanceError> {
        let access = self
            .issue_human_access(
                conn,
                principal_id,
                rotated_from,
                session.connection,
                request_id,
            )
            .await?;
        let lifetime = self.settings.refresh_ttl;
        // One PostgreSQL instant, sampled in the caller's transaction, feeds both
        // the signed `exp` and the durable row expiry that PostgreSQL later
        // evaluates, so the two can never disagree.
        let issued_at = refresh_issuance_instant(conn).await?;
        let refresh_token = self.issuing_key.issue_refresh_token(
            PrincipalKindTag::User,
            PrincipalId::new(principal_id),
            conn.data_tenant_id(),
            issued_at,
            lifetime,
        )?;
        insert_human_refresh_token(
            conn,
            Uuid::new_v4(),
            principal_id,
            &hash_secret(&refresh_token),
            issued_at + lifetime,
            rotated_from,
            session,
        )
        .await?;
        Ok(ExchangedToken {
            refresh_token: Some(SecretString::from(refresh_token)),
            ..access
        })
    }

    /// Mint a human access token from [`Self::issue`] for a session bound to
    /// `connection`, without a refresh token.
    ///
    /// [`Self::issue_human_session`] builds every new refresh token on it. `refresh_row` is the refresh row presented, if any: it makes
    /// the grant a refresh and attributes the access token to it; `None` is
    /// a login. Before anything is read or minted this takes the User's
    /// tenant-qualified refresh-family lock — the lock rotation and
    /// administrative User revocation hold through their commits — so login
    /// and renewal alike either commit before a revocation, whose family
    /// snapshot then retires the new row, or wait for it and re-read the User
    /// as suspended. The lock is transaction-scoped and held until the
    /// caller's commit or rollback; the owning transaction reacquires it
    /// without waiting. It then takes the tenant's connection slot lock — the
    /// lock every lifecycle mutation takes — and requires that exact revision
    /// to still be Active, so replacement, deactivation, or removal committed
    /// on any replica cuts the session off: once the mutation commits no
    /// token can be minted, and a mutation waiting on the lock commits only
    /// after this transaction does.
    ///
    /// # Errors
    /// Returns [`IssuanceError::ConnectionInactive`] when the bound connection
    /// revision is no longer Active, every [`Self::issue`] error, and
    /// [`IssuanceError::Wyrd`] when a lock fails. Nothing is committed here.
    pub async fn issue_human_access(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        refresh_row: Option<Uuid>,
        connection: HumanConnectionBinding,
        request_id: &str,
    ) -> Result<ExchangedToken, IssuanceError> {
        lock_refresh_family(conn, "user", principal_id)
            .await
            .map_err(store_error)?;
        lock_human_connection_slot(conn)
            .await
            .map_err(store_error)?;
        let active = human_connection_is_active(
            conn,
            connection.connection_id,
            connection.connection_revision,
        )
        .await
        .map_err(store_error)?;
        if !active {
            return Err(IssuanceError::ConnectionInactive);
        }
        let grant = match refresh_row {
            Some(consumed) => TenantGrant::Refresh { consumed },
            None => TenantGrant::OidcLogin,
        };
        self.issue(conn, principal_id, grant, request_id).await
    }
}

/// Resolve role references to the union of their stored permissions.
///
/// A role name with no stored row contributes nothing, so a deleted role
/// simply stops granting.
///
/// # Errors
/// Returns [`IssuanceError::Database`] when the read fails and
/// [`IssuanceError::RoleCorrupt`] when a stored permission document does not
/// decode.
pub(crate) async fn resolve_permissions(
    conn: &mut TenantConn<'_>,
    roles: &[RoleRef],
) -> Result<PermissionSet, IssuanceError> {
    if roles.is_empty() {
        return Ok(PermissionSet::new());
    }
    let names = roles.iter().map(RoleRef::as_str).collect::<Vec<_>>();
    permission_set_from_rows(roles_by_name(conn, &names).await?)
}

/// Decode and merge stored role permission documents into one set.
///
/// Decoding is the only validation a stored grant gets before it becomes JWT
/// authority, so a document the [`Permission`] schema rejects — an unknown
/// action, a scope-less Bifrost grant, a Bifrost object scope on a non-read
/// action — fails the whole issuance naming the corrupt role instead of being
/// skipped.
///
/// # Errors
/// Returns [`IssuanceError::RoleCorrupt`] naming the first role whose
/// permission document does not decode.
fn permission_set_from_rows(rows: Vec<RoleRow>) -> Result<PermissionSet, IssuanceError> {
    let mut permissions = PermissionSet::new();
    for row in rows {
        let granted: Vec<Permission> = serde_json::from_value(row.permissions)
            .map_err(|_| IssuanceError::RoleCorrupt { role: row.name })?;
        for permission in granted {
            permissions.insert(permission);
        }
    }
    Ok(permissions)
}

/// Build the canonical token-exchange audit event for one issuance.
///
/// A direct grant names its principal as both subject and actor, with the
/// spent credential attached. A delegated grant is recorded under the subject
/// being acted for, like every request its token later makes, names `issued`
/// — the actor — as the exchange's actor, records the earlier actors and
/// their roots earliest first, targets the requested audience, and
/// attaches the credential that authenticated the actor, when one did.
fn exchange_audit_event(
    issued: &TokenPrincipalRef,
    grant: &TenantGrant,
    expires_at: DateTime<Utc>,
    request_id: &str,
) -> wyrd_spec::vala::api::AuditEvent {
    let TenantGrant::Delegation {
        subject,
        prior_act,
        audience,
        actor_credential_id,
        ..
    } = grant
    else {
        return auth_event(
            request_id,
            TOKEN_EXCHANGE_OPERATION,
            Attribution::direct(issued.id),
            issued.kind,
            None,
            AuditOutcome::Allowed,
            AuditDetail::TokenExchange {
                subject_principal_id: issued.id,
                actor_principal_id: issued.id,
                delegation_chain: Vec::new(),
                expires_at,
            },
        )
        .with_credential_id(grant.credential_id());
    };
    let mut delegation_chain = Vec::new();
    let mut layer: Option<&ActClaim> = prior_act.as_deref();
    while let Some(act) = layer {
        delegation_chain.push(AuditDelegationStep {
            principal_id: act.principal.id,
            principal_kind: act.principal.kind,
            workload_uid: Some(act.workload_uid.clone()),
        });
        layer = act.act.as_deref();
    }
    delegation_chain.reverse();
    let mut event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        Attribution::new(subject.id, issued.id),
        subject.kind,
        None,
        AuditOutcome::Allowed,
        AuditDetail::TokenExchange {
            subject_principal_id: subject.id,
            actor_principal_id: issued.id,
            delegation_chain,
            expires_at,
        },
    );
    audience.as_str().clone_into(&mut event.resource);
    event.with_credential_id(*actor_credential_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;
    use wyrd_runtime::{
        Action, BifrostPermissionScope, BifrostTableScope, Permission, PermissionScope, Resource,
    };
    use wyrd_sql::queries::auth::RoleRow;

    use super::{IssuanceError, permission_set_from_rows};

    /// Build one stored role row with `permissions` as its JSONB document.
    fn role(name: &str, permissions: serde_json::Value) -> RoleRow {
        RoleRow {
            id: Uuid::nil(),
            name: name.to_owned(),
            permissions,
        }
    }

    /// Builds one Bifrost query-read permission at the supplied object scope.
    fn scoped_query_read(scope: PermissionScope) -> Permission {
        Permission {
            resource: Resource::BifrostQuery,
            action: Action::Read,
            scope,
        }
    }

    /// Assert that decoding `rows` fails naming `name` as the corrupt role.
    fn assert_corrupt(rows: Vec<RoleRow>, name: &str) {
        match permission_set_from_rows(rows) {
            Err(IssuanceError::RoleCorrupt { role }) => assert_eq!(role, name),
            other => panic!("expected {name} to be reported corrupt, got {other:?}"),
        }
    }

    /// Several roles merge into one set, with the wildcard subsuming the rest.
    #[test]
    fn decodes_and_merges_permissions_from_role_rows() {
        let set = permission_set_from_rows(vec![
            role(
                "viewer",
                json!([
                    { "resource": "cards", "action": "read", "scope": "all" },
                    { "resource": "artifacts", "action": "read", "scope": "all" }
                ]),
            ),
            role(
                "admin",
                json!([{ "resource": "wildcard", "action": "wildcard", "scope": "all" }]),
            ),
        ])
        .expect("role permissions decode");

        assert_eq!(set.len(), 1);
        assert!(set.contains(&Permission::wildcard()));
        assert!(set.contains(&Permission::card_read()));
    }

    /// An undecodable document fails issuance naming the role.
    #[test]
    fn reports_role_name_for_malformed_permissions() {
        assert_corrupt(
            vec![role(
                "bad_role",
                json!([{ "resource": "cards", "action": "unknown", "scope": "all" }]),
            )],
            "bad_role",
        );
    }

    /// Stored scoped Bifrost grants decode to exactly their object scopes, and
    /// a scope-less row is surfaced as a corrupt role.
    #[test]
    fn decodes_scoped_bifrost_grants_and_rejects_unscoped_rows() {
        let uid = Uuid::now_v7();
        let set = permission_set_from_rows(vec![role(
            "analyst",
            json!([
                {
                    "resource": "bifrost_query",
                    "action": "read",
                    "scope": {"bifrost": {"schema": {"catalog": "bifrost", "schema": "logs"}}}
                },
                {
                    "resource": "bifrost_query",
                    "action": "read",
                    "scope": {"bifrost": {"table": {
                        "catalog": "bifrost",
                        "schema": "traces",
                        "table_uid": uid.to_string(),
                    }}}
                }
            ]),
        )])
        .expect("scoped role permissions decode");

        assert_eq!(set.len(), 2);
        assert!(set.contains(&scoped_query_read(PermissionScope::Bifrost(
            BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "bifrost".to_owned(),
                schema: "logs".to_owned(),
                table_uid: Uuid::now_v7(),
            })
        ))));
        assert!(set.contains(&scoped_query_read(PermissionScope::Bifrost(
            BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "bifrost".to_owned(),
                schema: "traces".to_owned(),
                table_uid: uid,
            })
        ))));
        assert!(!set.contains(&Permission::bifrost_query_read()));

        assert_corrupt(
            vec![role(
                "unscoped",
                json!([{ "resource": "bifrost_query", "action": "read" }]),
            )],
            "unscoped",
        );
    }

    /// A Bifrost object scope on a non-read action is corrupt, never read
    /// authority. `wildcard` is the dangerous case: it would otherwise cover
    /// the required read while inheriting one schema's narrow scope.
    #[test]
    fn rejects_bifrost_scope_on_a_non_read_action() {
        for action in [
            json!("write"),
            json!("wildcard"),
            json!({"any_of": ["read", "write"]}),
        ] {
            assert_corrupt(
                vec![role(
                    "corrupt_analyst",
                    json!([{
                        "resource": "bifrost_query",
                        "action": action,
                        "scope": {"bifrost": {"schema": {"catalog": "bifrost", "schema": "logs"}}}
                    }]),
                )],
                "corrupt_analyst",
            );
        }
    }
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::{DateTime, Utc};
    use secrecy::SecretString;
    use serde_json::json;
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenAudience, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
        public_key_from_pem,
    };
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_runtime::{Permission, PermissionSet, PrincipalId};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{
        Attribution, CardAccess, FamilyRestriction, OAuthClientId, PrincipalKindTag, RoleSource,
    };
    use wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{
        grant_role_to_service_account, grant_role_to_user, insert_role, insert_service_account,
        insert_user, provision_system_principal, revoke_role_from_service_account,
        suspend_service_account_principal, suspend_user_principal,
    };

    use wyrd_sql::queries::verification::{
        BindingActivation, FrozenTarget, NewBinding, project_bindings,
    };
    use wyrd_sql::row_types::auth::{HumanConnectionBinding, HumanSessionBinding};

    use super::{IssuanceError, TenantGrant, TenantTokenIssuer, TokenExchangeSettings};
    use crate::audit::test_audit::RecordedAudit;
    use crate::exchange_api_key::pg_tests::{
        insert_test_service_account, named_service_card_ref, root_uid, seed_root, service_family,
    };
    use crate::revoke::pg_tests::wait_for_advisory_lock_wait;
    use crate::revoke::revoke_principal_in_conn;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    /// The test signing key, shared by the issuer and the verifier.
    /// A `wyrd-cli` session bound to `connection`.
    fn cli(connection: HumanConnectionBinding) -> HumanSessionBinding {
        HumanSessionBinding {
            connection,
            client: OAuthClientId::WyrdCli,
        }
    }

    fn issuing_key() -> Arc<IssuingKey> {
        Arc::new(
            IssuingKey::from_ed_pem(
                SecretString::from(PRIVATE_KEY_PEM),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("test private key loads"),
        )
    }

    /// The issuer under test with the default lifetimes, staging its audit on
    /// `audit`.
    ///
    /// The test owns `audit` and drains it before its fixture drops, so no audit
    /// commit is still logging in when the fixture drops its database.
    fn issuer(audit: &Arc<RecordedAudit>) -> TenantTokenIssuer {
        TenantTokenIssuer::new(
            issuing_key(),
            TokenExchangeSettings::default(),
            Arc::clone(audit) as _,
        )
    }

    /// The request-path verifier for tokens this issuer signs.
    fn verifier() -> TokenVerifier {
        let pem = issuing_key()
            .verifying_key_pem()
            .expect("verifying key encodes");
        let key = public_key_from_pem(pem.as_bytes()).expect("verifying key decodes");
        let mut keys = HashMap::new();
        keys.insert(Kid::new("k1").expect("kid is valid"), Arc::new(key));
        TokenVerifier::new(keys, "wyrd", WyrdAuthVerifySettings::default())
    }

    /// Seed a role granting exactly `card:read`, returning its id.
    async fn seed_card_reader_role(conn: &mut TenantConn<'_>) -> Uuid {
        let role_id = Uuid::new_v4();
        insert_role(
            conn,
            role_id,
            "card_reader",
            &json!([{ "resource": "cards", "action": "read", "scope": "all" }]),
            false,
        )
        .await
        .expect("role seeds");
        role_id
    }

    /// Seed an active human user, returning its id.
    async fn seed_user(conn: &mut TenantConn<'_>) -> Uuid {
        let user_id = Uuid::new_v4();
        insert_user(
            conn,
            user_id,
            Some(&format!("{user_id}@test.com")),
            "oidc",
            None,
        )
        .await
        .expect("user inserts");
        user_id
    }

    /// Seed an active Card-less tenant automation principal, returning its id.
    async fn seed_automation(conn: &mut TenantConn<'_>, creator: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        insert_service_account(
            conn,
            id,
            "service",
            None,
            "automation",
            None,
            wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(creator)),
        )
        .await
        .expect("service account inserts");
        id
    }

    /// Verify `token` for `tenant` and return whether it carries `card:read`.
    fn grants_card_read(token: &SecretString, tenant: DataTenantId) -> bool {
        verifier()
            .verify(token, &tenant)
            .expect("issued token verifies")
            .principal
            .effective_permissions
            .contains(&Permission::card_read())
    }

    /// The token's authority is the principal's current grants, resolved at
    /// issuance: a grant withdrawn between two issuances is absent from the
    /// second token without any request-time lookup.
    ///
    /// # Panics
    /// Panics when the fixture, seeding, role grant or revoke, or either
    /// issuance fails, or `card:read` is missing from the first token or
    /// present in the second.
    #[tokio::test]
    async fn issue_signs_current_grants_and_follows_grant_changes() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let role_id = seed_card_reader_role(&mut conn).await;
        let creator = seed_user(&mut conn).await;
        let principal = seed_automation(&mut conn, creator).await;
        grant_role_to_service_account(&mut conn, principal, role_id)
            .await
            .expect("role grants");

        let granted = issuer(&audit)
            .issue(
                &mut conn,
                principal,
                TenantGrant::JwtBearer { workload_uid: None },
                "req-granted",
            )
            .await
            .expect("issuance succeeds");
        assert!(grants_card_read(&granted.access_token, tenant));

        revoke_role_from_service_account(&mut conn, principal, role_id)
            .await
            .expect("role revokes");
        let withdrawn = issuer(&audit)
            .issue(
                &mut conn,
                principal,
                TenantGrant::JwtBearer { workload_uid: None },
                "req-withdrawn",
            )
            .await
            .expect("issuance succeeds");
        assert!(
            !grants_card_read(&withdrawn.access_token, tenant),
            "the next token carries only the grants the principal holds now"
        );
    }

    /// A suspended principal is refused by every tenant grant, and the one
    /// refusal is the same owner's decision on every path.
    ///
    /// # Panics
    /// Panics when the fixture, connection, seeding, or suspension fails, or any
    /// grant does not fail with [`IssuanceError::PrincipalInactive`].
    #[tokio::test]
    async fn a_suspended_principal_is_refused_by_every_grant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user = seed_user(&mut conn).await;
        let machine = seed_automation(&mut conn, user).await;
        suspend_user_principal(&mut conn, user)
            .await
            .expect("user suspends");
        suspend_service_account_principal(&mut conn, machine)
            .await
            .expect("machine suspends");

        let subject = TokenPrincipalRef {
            id: PrincipalId::new(Uuid::new_v4()),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
        };
        let cases = [
            (user, TenantGrant::OidcLogin),
            (
                user,
                TenantGrant::Refresh {
                    consumed: Uuid::new_v4(),
                },
            ),
            (
                machine,
                TenantGrant::ApiKey {
                    credential_id: Uuid::new_v4(),
                    workload_uid: None,
                },
            ),
            (machine, TenantGrant::JwtBearer { workload_uid: None }),
            (
                machine,
                TenantGrant::Delegation {
                    subject: Box::new(subject),
                    subject_roles: Vec::new(),
                    subject_permissions: wyrd_runtime::PermissionSet::new(),
                    subject_card_access: CardAccess::AnyRegistered,
                    subject_workload_uid: None,
                    actor_workload_uid: unregistered_uid(),
                    prior_act: None,
                    audience: TokenAudience::Bifrost,
                    actor_credential_id: None,
                },
            ),
        ];
        for (principal, grant) in cases {
            let label = format!("{grant:?}");
            let result = issuer(&audit)
                .issue(&mut conn, principal, grant, "req-suspended")
                .await;
            assert!(
                matches!(result, Err(IssuanceError::PrincipalInactive)),
                "{label} must refuse a suspended principal, got {result:?}"
            );
        }
    }

    /// A fresh UUIDv7 Card UID that names no registered Card.
    ///
    /// # Panics
    /// Panics when a fresh UUIDv7 is not accepted as a Card UID.
    fn unregistered_uid() -> CardUid {
        CardUid::from_uuid(Uuid::now_v7()).expect("v7 uuid is a Card UID")
    }

    /// Seed an active `bound-svc` Service root with one `schedule` binding and
    /// the family's runtime account, returning the account and the root.
    ///
    /// # Panics
    /// Panics when a generated identity is invalid or any fixture write fails.
    async fn seed_bound_service(conn: &mut TenantConn<'_>, creator: Uuid) -> (Uuid, CardUid) {
        let card_uid = seed_root(conn, &named_service_card_ref("bound-svc"), creator).await;
        let id =
            insert_test_service_account(conn, creator, Some(&service_family("bound-svc"))).await;
        let verifier = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
        project_bindings(
            conn,
            &card_uid,
            &CardKind::Service,
            &[NewBinding {
                subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
                card_uid: card_uid.clone(),
                verifier_card_uid: verifier,
                trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
                operators: Vec::new(),
                activation: BindingActivation::Schedule {
                    cron: "0 2 * * *".to_owned(),
                    tz: None,
                },
            }],
        )
        .await
        .expect("binding projects");
        (id, card_uid)
    }

    /// Read one account's activity under `root` and the root's schedule
    /// cursor, if the root has a schedule binding.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn activity(
        conn: &mut TenantConn<'_>,
        principal: Uuid,
        root: &CardUid,
    ) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        sqlx::query_as(
            "SELECT (SELECT last_authenticated_at FROM wyrd.workload_activity \
                      WHERE principal_id = $1 AND workload_card_uid = $2), \
                    (SELECT next_run_at FROM wyrd.verification_bindings \
                      WHERE owner_card_uid = $2)",
        )
        .bind(principal)
        .bind(root.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("activity reads")
    }

    /// A direct API-key grant selecting a root, issued to that family's
    /// runtime account.
    fn api_key(root: Option<CardUid>) -> TenantGrant {
        TenantGrant::ApiKey {
            credential_id: Uuid::now_v7(),
            workload_uid: root,
        }
    }

    /// An API-key exchange under the root by the family's runtime account
    /// activates that root and arms its schedule; a later `jwt-bearer`
    /// issuance under the same root renews activity without moving the cursor.
    ///
    /// # Panics
    /// Panics when the fixture cannot start or seed, an issuance fails, or an
    /// activity or cursor expectation does not hold.
    #[tokio::test]
    async fn qualifying_machine_grants_activate_the_bound_owner() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = seed_user(&mut conn).await;
        let (principal, root) = seed_bound_service(&mut conn, creator).await;
        assert_eq!(activity(&mut conn, principal, &root).await, (None, None));

        issuer(&audit)
            .issue(
                &mut conn,
                principal,
                api_key(Some(root.clone())),
                "req-api-key",
            )
            .await
            .expect("api-key issuance succeeds");
        let (first, armed) = activity(&mut conn, principal, &root).await;
        let first = first.expect("api-key exchange records activity");
        let armed = armed.expect("first exchange arms the schedule");
        assert!(armed > first, "the cursor is the next future boundary");

        issuer(&audit)
            .issue(
                &mut conn,
                principal,
                TenantGrant::JwtBearer {
                    workload_uid: Some(root.clone()),
                },
                "req-jwt-bearer",
            )
            .await
            .expect("jwt-bearer issuance succeeds");
        let (renewed, cursor) = activity(&mut conn, principal, &root).await;
        assert!(renewed.expect("renewal records activity") >= first);
        assert_eq!(cursor, Some(armed), "renewal never moves an armed cursor");
    }

    /// A delegation acting under the root, scheduled resolution, rootless
    /// grants by the family's account, and rooted grants by an unrestricted
    /// account never record activity or arm the root's schedule.
    ///
    /// # Panics
    /// Panics when the fixture cannot start or seed, an issuance fails, or any
    /// activity or cursor is written.
    #[tokio::test]
    async fn non_qualifying_grants_never_touch_activity() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = seed_user(&mut conn).await;
        let (bound, root) = seed_bound_service(&mut conn, creator).await;
        let automation = seed_automation(&mut conn, creator).await;
        let subject = TokenPrincipalRef {
            id: PrincipalId::new(Uuid::new_v4()),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
        };

        issuer(&audit)
            .issue(
                &mut conn,
                bound,
                TenantGrant::Delegation {
                    subject: Box::new(subject),
                    subject_roles: Vec::new(),
                    subject_permissions: PermissionSet::new(),
                    subject_card_access: CardAccess::AnyRegistered,
                    subject_workload_uid: None,
                    actor_workload_uid: root.clone(),
                    prior_act: None,
                    audience: TokenAudience::Wyrd,
                    actor_credential_id: None,
                },
                "req-delegation",
            )
            .await
            .expect("delegated issuance succeeds");
        issuer(&audit)
            .scheduled(&mut conn, bound, &root)
            .await
            .expect("scheduled resolution succeeds");
        for grant in [api_key(None), TenantGrant::JwtBearer { workload_uid: None }] {
            issuer(&audit)
                .issue(&mut conn, bound, grant, "req-rootless")
                .await
                .expect("rootless issuance succeeds");
        }
        for grant in [
            api_key(Some(root.clone())),
            TenantGrant::JwtBearer {
                workload_uid: Some(root.clone()),
            },
        ] {
            issuer(&audit)
                .issue(&mut conn, automation, grant, "req-unrestricted")
                .await
                .expect("an unrestricted account may select the root");
        }
        assert_eq!(activity(&mut conn, bound, &root).await, (None, None));
        assert_eq!(activity(&mut conn, automation, &root).await, (None, None));
    }

    /// One stable account keeps its id and grants while each issuance selects
    /// its own context: a family account signs `root_required` rootless and
    /// exactly the selected release's graph and root otherwise, roots outside
    /// the family and unknown roots are refused identically, an unrestricted
    /// account signs `any_registered` rootless, and only the family account's
    /// direct grants record activity, per root.
    ///
    /// # Panics
    /// Panics when a fixture write or issuance fails or any signed principal,
    /// grant, Card access, root, refusal, or activity differs.
    #[tokio::test]
    async fn stable_account_root_selection_preserves_grants() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = seed_user(&mut conn).await;
        let role_id = seed_card_reader_role(&mut conn).await;
        let family = service_family("checkout");
        let account = insert_test_service_account(&mut conn, creator, Some(&family)).await;
        let automation = seed_automation(&mut conn, creator).await;
        for principal in [account, automation] {
            grant_role_to_service_account(&mut conn, principal, role_id)
                .await
                .expect("role grants");
        }
        let release_1 = named_service_card_ref("checkout");
        let release_2 = CardRef {
            version: VersionBlock::parse("2.0.0").expect("static version is valid"),
            ..release_1.clone()
        };
        let r1 = seed_root(&mut conn, &release_1, creator).await;
        let r2 = seed_root(&mut conn, &release_2, creator).await;
        let foreign = seed_root(&mut conn, &named_service_card_ref("billing"), creator).await;
        assert_eq!(root_uid(&mut conn, &release_2).await, r2);

        let scoped = |root: &CardUid| CardAccess::Scoped {
            cards: vec![root.clone()],
        };
        let cases = [
            (
                account,
                None,
                CardAccess::RootRequired {
                    restriction: FamilyRestriction::Family(family.clone()),
                },
            ),
            (account, Some(r1.clone()), scoped(&r1)),
            (account, Some(r2.clone()), scoped(&r2)),
            (automation, None, CardAccess::AnyRegistered),
            (automation, Some(r1.clone()), scoped(&r1)),
        ];
        for (principal, root, card_access) in cases {
            let token = issuer(&audit)
                .issue(&mut conn, principal, api_key(root.clone()), "req-select")
                .await
                .expect("issuance succeeds");
            let verified = verifier()
                .verify(&token.access_token, &tenant)
                .expect("issued token verifies")
                .principal;
            assert_eq!(verified.id.as_uuid(), principal, "identity never changes");
            assert!(
                verified
                    .effective_permissions
                    .contains(&Permission::card_read()),
                "grants never change with context"
            );
            assert_eq!(verified.card_access, card_access);
            assert_eq!(verified.workload_uid, root);
        }

        for root in [foreign.clone(), unregistered_uid()] {
            let refused = issuer(&audit)
                .issue(&mut conn, account, api_key(Some(root)), "req-refused")
                .await;
            assert!(
                matches!(
                    refused,
                    Err(IssuanceError::Wyrd(WyrdError::WorkloadUidRefused { .. }))
                ),
                "{refused:?}"
            );
        }

        assert!(activity(&mut conn, account, &r1).await.0.is_some());
        assert!(activity(&mut conn, account, &r2).await.0.is_some());
        assert_eq!(activity(&mut conn, automation, &r1).await.0, None);

        let scheduled = issuer(&audit)
            .scheduled(&mut conn, account, &r2)
            .await
            .expect("scheduled resolution succeeds");
        assert_eq!(scheduled.principal.card_access, scoped(&r2));
        let refused = issuer(&audit).scheduled(&mut conn, account, &foreign).await;
        assert!(
            matches!(
                refused,
                Err(IssuanceError::Wyrd(WyrdError::WorkloadUidRefused { .. }))
            ),
            "{refused:?}"
        );
    }

    /// A request selector narrows a rootless bearer under its signed access
    /// only: the family account's root becomes its exact scope, a foreign or
    /// unknown root is refused identically, a signed root is unchanged by an
    /// equal selector and never rebound by a different one, and no activity
    /// is recorded.
    ///
    /// # Panics
    /// Panics when a fixture write, issuance, or verification fails, or any
    /// selection differs.
    #[tokio::test]
    async fn request_selector_narrows_only_rootless_bearers() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = seed_user(&mut conn).await;
        let family = service_family("checkout");
        let account = insert_test_service_account(&mut conn, creator, Some(&family)).await;
        let r1 = seed_root(&mut conn, &named_service_card_ref("checkout"), creator).await;
        let foreign = seed_root(&mut conn, &named_service_card_ref("billing"), creator).await;
        let rootless = verifier()
            .verify(
                &issuer(&audit)
                    .issue(&mut conn, account, api_key(None), "req-rootless")
                    .await
                    .expect("rootless issuance succeeds")
                    .access_token,
                &tenant,
            )
            .expect("rootless token verifies");
        let selected = super::select_request_root(&mut conn, &rootless, &r1)
            .await
            .expect("family root selects")
            .expect("rootless bearer narrows");
        assert_eq!(selected.principal.id, rootless.principal.id);
        assert_eq!(
            selected.principal.effective_permissions,
            rootless.principal.effective_permissions
        );
        assert_eq!(
            selected.principal.card_access,
            CardAccess::Scoped {
                cards: vec![r1.clone()]
            }
        );
        assert_eq!(selected.principal.workload_uid, Some(r1.clone()));
        for root in [foreign.clone(), unregistered_uid()] {
            let refused = super::select_request_root(&mut conn, &rootless, &root).await;
            assert!(
                matches!(refused, Err(WyrdError::WorkloadUidRefused { .. })),
                "{refused:?}"
            );
        }
        assert!(
            super::select_request_root(&mut conn, &selected, &r1)
                .await
                .expect("equal selector")
                .is_none()
        );
        let rebound = super::select_request_root(&mut conn, &selected, &foreign).await;
        assert!(
            matches!(rebound, Err(WyrdError::SignedWorkloadMismatch { .. })),
            "{rebound:?}"
        );
        assert_eq!(activity(&mut conn, account, &r1).await.0, None);
    }

    /// A human session carries the user's current grants and a refresh token.
    #[tokio::test]
    async fn a_human_session_carries_current_grants_and_a_refresh_token() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let role_id = seed_card_reader_role(&mut conn).await;
        let user = seed_user(&mut conn).await;
        grant_role_to_user(&mut conn, user, role_id, RoleSource::Idp)
            .await
            .expect("role grants");

        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");

        let session = issuer(&audit)
            .issue_human_session(&mut conn, user, None, cli(binding), "req-session")
            .await
            .expect("session issues");

        assert!(grants_card_read(&session.access_token, tenant));
        assert!(session.refresh_token.is_some(), "a human session renews");
    }

    /// No public tenant grant can mint for the SYSTEM writer: API-key exchange,
    /// workload `jwt-bearer`, delegation, token exchange by id, OIDC login, and
    /// human refresh all refuse it as an inactive principal.
    ///
    /// # Panics
    /// Panics when any public grant issues a token for the writer.
    #[tokio::test]
    async fn public_grants_refuse_the_system_principal() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let writer = provision_system_principal(&mut conn, Attribution::unresolved())
            .await
            .expect("writer provisions");
        let subject = TokenPrincipalRef {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
        };
        let grants = [
            TenantGrant::ApiKey {
                credential_id: Uuid::now_v7(),
                workload_uid: None,
            },
            TenantGrant::JwtBearer { workload_uid: None },
            TenantGrant::Delegation {
                subject: Box::new(subject),
                subject_roles: Vec::new(),
                subject_permissions: PermissionSet::from_iter([Permission::bifrost_record_write()]),
                subject_card_access: CardAccess::AnyRegistered,
                subject_workload_uid: None,
                actor_workload_uid: unregistered_uid(),
                prior_act: None,
                audience: TokenAudience::Wyrd,
                actor_credential_id: None,
            },
            TenantGrant::OidcLogin,
            TenantGrant::Refresh {
                consumed: Uuid::now_v7(),
            },
        ];
        for grant in grants {
            let label = format!("{grant:?}");
            let result = issuer(&audit)
                .issue(&mut conn, writer, grant, "req-system")
                .await;
            assert!(
                matches!(result, Err(IssuanceError::PrincipalInactive)),
                "{label} must refuse the SYSTEM writer, got {result:?}"
            );
        }
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        let session = issuer(&audit)
            .issue_human_session(&mut conn, writer, None, cli(binding), "req-system-session")
            .await;
        assert!(
            matches!(session, Err(IssuanceError::PrincipalInactive)),
            "a human session cannot be opened for the writer: {session:?}"
        );
    }

    /// A session bound to a connection revision that is no longer Active is
    /// refused before any token or refresh row is minted.
    #[tokio::test]
    async fn a_human_session_bound_to_an_inactive_connection_is_refused() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user = seed_user(&mut conn).await;
        let active = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        let stale = HumanConnectionBinding {
            connection_revision: active.connection_revision + 1,
            ..active
        };

        let result = issuer(&audit)
            .issue_human_session(&mut conn, user, None, cli(stale), "req-stale")
            .await;

        assert!(
            matches!(result, Err(IssuanceError::ConnectionInactive)),
            "a stale binding must refuse, got {result:?}"
        );
    }

    /// Read backend `conn`'s process id so a peer can observe its lock waits.
    ///
    /// # Panics
    /// Panics when the statement fails.
    async fn backend_pid(conn: &mut TenantConn<'_>) -> i32 {
        sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("backend pid reads")
    }

    /// Count `user`'s refresh rows in committed state, all and still active.
    ///
    /// # Panics
    /// Panics when the connection or the count fails.
    async fn committed_family(fixture: &PgFixture, user: Uuid) -> (i64, i64) {
        let mut fresh = fixture.tenant_conn().await.expect("fresh conn opens");
        sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE revoked_at IS NULL)
               FROM wyrd.auth_refresh_tokens
              WHERE principal_kind = 'user' AND principal_id = $1",
        )
        .bind(user)
        .fetch_one(&mut **fresh.transaction())
        .await
        .expect("family counts read")
    }

    /// First-login issuance and administrative User revocation serialize on
    /// the User's refresh-family lock in both orders.
    ///
    /// Issuance first: with the session's row inserted and its transaction
    /// open, revocation parks on the family lock, then retires the new row
    /// once issuance commits, so no active family row survives. Revocation
    /// first: with the User suspended and its transaction open, issuance parks
    /// on the family lock before reading the User, then re-reads it as
    /// suspended once revocation commits and refuses without writing a row.
    ///
    /// # Panics
    /// Panics when either side fails to wait on the family lock, a transaction
    /// fails, issuance is not refused after revocation, or any row is left
    /// renewable.
    #[tokio::test]
    async fn initial_session_issuance_and_user_revocation_serialize() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let mut setup = fixture.tenant_conn().await.expect("setup conn opens");
        let early = seed_user(&mut setup).await;
        let late = seed_user(&mut setup).await;
        let binding = seed_active_human_connection(&mut setup)
            .await
            .expect("connection seeds");
        setup.commit().await.expect("setup commits");

        let mut issuing = fixture.tenant_conn().await.expect("issuing conn opens");
        issuer(&audit)
            .issue_human_session(&mut issuing, early, None, cli(binding), "req-first-login")
            .await
            .expect("first login issues");
        let mut revoking = fixture.tenant_conn().await.expect("revoking conn opens");
        let revoke_pid = backend_pid(&mut revoking).await;
        let (revoked, ()) = tokio::join!(
            revoke_principal_in_conn(
                &mut revoking,
                PrincipalId::new(early),
                PrincipalKindTag::User,
                tenant,
            ),
            async {
                wait_for_advisory_lock_wait(&fixture, revoke_pid).await;
                issuing.commit().await.expect("issuance commits");
            }
        );
        revoked.expect("revocation succeeds");
        revoking.commit().await.expect("revocation commits");
        assert_eq!(
            committed_family(&fixture, early).await,
            (1, 0),
            "the first-login row exists and revocation retired it"
        );

        let mut revoking = fixture.tenant_conn().await.expect("revoking conn opens");
        revoke_principal_in_conn(
            &mut revoking,
            PrincipalId::new(late),
            PrincipalKindTag::User,
            tenant,
        )
        .await
        .expect("revocation succeeds");
        let mut issuing = fixture.tenant_conn().await.expect("issuing conn opens");
        let issue_pid = backend_pid(&mut issuing).await;
        let late_issuer = issuer(&audit);
        let (issued, ()) = tokio::join!(
            late_issuer.issue_human_session(
                &mut issuing,
                late,
                None,
                cli(binding),
                "req-late-login"
            ),
            async {
                wait_for_advisory_lock_wait(&fixture, issue_pid).await;
                revoking.commit().await.expect("revocation commits");
            }
        );
        assert!(
            matches!(issued, Err(IssuanceError::PrincipalInactive)),
            "issuance after revocation re-reads the suspended User, got {issued:?}"
        );
        drop(issuing);
        assert_eq!(
            committed_family(&fixture, late).await,
            (0, 0),
            "a refused first login writes no refresh row"
        );
    }
}
