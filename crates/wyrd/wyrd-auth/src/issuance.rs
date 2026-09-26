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
//! The internal mints, [`TenantTokenIssuer::issue_system_token`] and
//! [`TenantTokenIssuer::issue_system_drift_read_token`], sign the tenant's
//! credentialless SYSTEM principal a token scoped to one Verifier for exactly
//! one fixed purpose. They have no grant evidence to verify and no public entry
//! path.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use vala_sql::queries::olap_catalog::get_by_fqn;
use wyrd_auth_issue::{AccessGrant, IssueError, IssuingKey};
use wyrd_auth_verify::{ActClaim, TokenAudience, TokenPrincipalRef};
use wyrd_runtime::{Permission, PermissionSet, PrincipalId, RoleRef};
use wyrd_spec::auth::{PrincipalKindTag, SecretBearer, TokenResponse, TokenType};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_spec::vala::audit_detail::CardScopeMintKind;
use wyrd_sql::queries::auth::{
    RoleRow, insert_refresh_token, insert_refresh_token_rotated, list_service_account_roles,
    list_user_roles, refresh_issuance_instant, roles_by_name, service_account_by_id,
    system_principal_id, user_by_id,
};
use wyrd_sql::queries::platform::tenant_resolver::tenant_admits_credentials;
use wyrd_sql::queries::verification::record_machine_authentication;
use wyrd_sql::{SqlError, TenantConn};

use crate::audit::{TOKEN_EXCHANGE_OPERATION, append_auth_audit, auth_event};
use crate::card_scope::{
    IssueErrorOrWyrd, MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_JWT_BEARER, issue_scope_error,
    resolve_card_ref_scope, write_scope_mint_success_audit,
};
use crate::exchange_api_key::{
    DELEGATION_POLICY_ACTION, principal_kind_wire, role_refs, token_hash,
};

/// Canonical Bifrost table the SYSTEM Drift reader may read.
const DRIFT_OBSERVATIONS: &str = "vala.drift.observations";

/// Tenant token lifetimes.
#[derive(Debug, Clone)]
pub struct TokenExchangeSettings {
    /// Access token lifetime: the bound on how long a revoked credential,
    /// suspended principal, or withdrawn grant stays spendable.
    pub access_ttl: Duration,
    /// Human refresh token lifetime.
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

/// The grant a tenant access token is issued under, after its entry path has
/// verified the grant-specific evidence.
///
/// Closed on purpose: these are the complete tenant issuance paths. The
/// variant decides which principal table is read, the credential attribution,
/// and the Card-scope mint kind — nothing else differs between paths.
#[derive(Debug)]
pub enum TenantGrant {
    /// A verified tenant API key.
    ApiKey {
        /// The API-key row the holder presented.
        credential_id: Uuid,
    },
    /// A verified federated OIDC login.
    OidcLogin,
    /// A consumed human refresh token.
    Refresh {
        /// The refresh row consumed by this rotation.
        consumed: Uuid,
    },
    /// A verified workload `jwt-bearer` assertion.
    JwtBearer,
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
        /// Earlier actors from the subject token, nested inside the new actor.
        prior_act: Option<Box<ActClaim>>,
        /// Audience the delegated token is issued for.
        audience: TokenAudience,
        /// Credential that authenticated the actor's token, when one did.
        ///
        /// Audit-only: it attributes the exchange to the actor's credential and
        /// never enters the delegated token.
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
            Self::ApiKey { credential_id } => Some(*credential_id),
            Self::Refresh { consumed } => Some(*consumed),
            Self::OidcLogin | Self::JwtBearer | Self::Delegation { .. } => None,
        }
    }

    /// Whether this grant is a Card-bound owner's runtime activity.
    ///
    /// Only a machine's own durable-credential exchange — an API key or a
    /// workload `jwt-bearer` assertion — proves the workload is running.
    /// Delegation acts for another caller, and human grants bind no Card.
    fn records_owner_activity(&self) -> bool {
        matches!(self, Self::ApiKey { .. } | Self::JwtBearer)
    }

    /// The Card-scope mint kind a Card-bound machine grant records; `None`
    /// for human grants, which bind no Card.
    fn scope_mint_kind(&self) -> Option<CardScopeMintKind> {
        match self {
            Self::ApiKey { .. } => Some(MINT_KIND_API_KEY_EXCHANGE),
            Self::JwtBearer => Some(MINT_KIND_JWT_BEARER),
            // The subject's Card scope was minted with its own token; the
            // exchange confers no new emit authority.
            Self::OidcLogin | Self::Refresh { .. } | Self::Delegation { .. } => None,
        }
    }

    /// Build the signed-token contents for `issued`, whose current `roles`
    /// and `permissions` were just resolved.
    ///
    /// Every grant but a delegation names `issued` with its own authority and
    /// the `wyrd` audience. A delegation names the subject instead, carries
    /// only the intersection of `issued`'s permissions with the subject's, and
    /// records `issued` as the outermost actor over any earlier actors. The
    /// actor is attribution only, so its Card scope stays out of the token.
    fn into_access_grant(
        self,
        issued: TokenPrincipalRef,
        roles: Vec<RoleRef>,
        permissions: PermissionSet,
    ) -> AccessGrant {
        let credential_id = self.credential_id();
        match self {
            Self::Delegation {
                subject,
                subject_roles,
                subject_permissions,
                prior_act,
                audience,
                ..
            } => AccessGrant {
                principal: *subject,
                roles: subject_roles,
                permissions: permissions.intersection(&subject_permissions),
                credential_id,
                act: Some(Box::new(ActClaim {
                    sub: issued.id.to_string(),
                    principal: TokenPrincipalRef {
                        card_ref_scope: CardRefScope::default(),
                        ..issued
                    },
                    act: prior_act,
                })),
                audience,
            },
            _ => AccessGrant {
                principal: issued,
                roles,
                permissions,
                credential_id,
                act: None,
                audience: TokenAudience::Wyrd,
            },
        }
    }
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
    /// A Card-scope or audit step failed with its own stable error.
    #[error("wyrd error")]
    Wyrd(#[from] WyrdError),
    /// The tenant has no provisioned SYSTEM writer, or its stored id is not a
    /// `UUIDv7`, so no verification-result token can be minted for it.
    #[error("tenant system principal is missing or malformed")]
    SystemPrincipalInvalid,
    /// The requested SYSTEM scope is not a UID-bearing Verifier Card.
    #[error("system token scope must be one UID-bearing Verifier card")]
    SystemScopeInvalid,
}

impl From<IssuanceError> for WyrdError {
    fn from(error: IssuanceError) -> Self {
        match error {
            IssuanceError::TenantNotAdmitting | IssuanceError::PrincipalInactive => {
                WyrdError::CredentialRevoked {
                    message: "principal or tenant is not active".to_owned(),
                    details: json!({}),
                }
            }
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
            IssuanceError::SystemPrincipalInvalid | IssuanceError::SystemScopeInvalid => {
                tracing::error!(error = %error, "system token mint refused");
                WyrdError::Internal {
                    message: "system token mint refused".to_owned(),
                    details: json!({}),
                }
            }
        }
    }
}

/// The one tenant access-token issuance owner.
///
/// Holds the signing key and token lifetimes; every tenant access token is
/// minted through [`Self::issue`] on the caller's [`TenantConn`], so the
/// token, its audit, and any credential bookkeeping commit together.
#[derive(Clone)]
pub struct TenantTokenIssuer {
    issuing_key: Arc<IssuingKey>,
    settings: TokenExchangeSettings,
}

impl std::fmt::Debug for TenantTokenIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantTokenIssuer")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl TenantTokenIssuer {
    /// Construct the issuer over the deployment signing key.
    #[must_use]
    pub fn new(issuing_key: Arc<IssuingKey>, settings: TokenExchangeSettings) -> Self {
        Self {
            issuing_key,
            settings,
        }
    }

    /// Mint a tenant access token for `principal_id` under `grant`.
    ///
    /// Reads, in the caller's transaction: whether the tenant admits
    /// credentials; the principal's current row (a user for human grants, a
    /// tenant machine principal otherwise), refusing anything not active; the
    /// principal's current role assignments and those roles' permissions; and,
    /// for a Card-bound principal, its transitive Card scope. It then signs a
    /// token carrying that `PermissionSet` for the configured access TTL and
    /// appends the canonical token-exchange audit (plus the Card-scope mint
    /// audit for a Card-bound principal).
    ///
    /// For a [`TenantGrant::Delegation`], `principal_id` is the actor: the
    /// token instead names the grant's subject as its principal, carries the
    /// intersection of the actor's current permissions with the subject's, and
    /// records the actor as its outermost `act`.
    ///
    /// An API-key or workload `jwt-bearer` grant for a Card-bound Service or
    /// Agent is that owner's runtime activity: it records the issuance time as
    /// the principal's last authentication and arms the owner's still-unarmed
    /// verification schedules. No other grant — delegation, OIDC login, human
    /// refresh — and no Card-free principal touches activity.
    ///
    /// Nothing is committed here; the caller commits or rolls back the grant
    /// whole.
    ///
    /// # Errors
    /// Returns [`IssuanceError::TenantNotAdmitting`] or
    /// [`IssuanceError::PrincipalInactive`] for an inactive tenant or
    /// principal, [`IssuanceError::RoleCorrupt`] for a corrupt role,
    /// [`IssuanceError::Issue`] when signing rejects the grant,
    /// [`IssuanceError::Database`] when a read or the activity write fails, and
    /// [`IssuanceError::Wyrd`] when Card-scope resolution or an audit append
    /// fails.
    #[tracing::instrument(level = "debug", skip(self, conn, grant), fields(principal_id = %principal_id), err)]
    pub async fn issue(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        grant: TenantGrant,
        request_id: &str,
    ) -> Result<ExchangedToken, IssuanceError> {
        let tenant = conn.data_tenant_id();
        if !tenant_admits_credentials(conn, tenant).await? {
            return Err(IssuanceError::TenantNotAdmitting);
        }
        let (principal, role_names) = if grant.is_human() {
            let user = user_by_id(conn, principal_id)
                .await?
                .filter(|user| user.status == "active")
                .ok_or(IssuanceError::PrincipalInactive)?;
            let principal = TokenPrincipalRef {
                id: PrincipalId::new(user.id),
                kind: PrincipalKindTag::User,
                tenant_id: tenant,
                card_ref: None,
                card_ref_scope: CardRefScope::default(),
            };
            (principal, list_user_roles(conn, user.id).await?)
        } else {
            let row = service_account_by_id(conn, principal_id)
                .await?
                .ok_or(IssuanceError::PrincipalInactive)?;
            let kind =
                principal_kind_wire(&row.principal_kind).ok_or(IssuanceError::PrincipalInactive)?;
            let card_ref = row.card_ref.map(|card_ref| card_ref.0);
            let card_ref_scope = match &card_ref {
                Some(card_ref) => resolve_card_ref_scope(conn, card_ref).await?,
                None => CardRefScope::default(),
            };
            let principal = TokenPrincipalRef {
                id: PrincipalId::new(row.id),
                kind,
                tenant_id: tenant,
                card_ref,
                card_ref_scope,
            };
            (principal, list_service_account_roles(conn, row.id).await?)
        };
        let roles = role_refs(role_names).map_err(|error| IssuanceError::RoleCorrupt {
            role: error.to_string(),
        })?;
        let permissions = resolve_permissions(conn, &roles).await?;

        if grant.records_owner_activity() && principal.card_ref.is_some() {
            record_machine_authentication(conn, principal.id).await?;
        }
        let expires_at = Utc::now() + self.settings.access_ttl;
        let scope_mint = grant
            .scope_mint_kind()
            .zip(principal.card_ref.clone())
            .map(|(kind, root)| (kind, root, principal.card_ref_scope.clone()));
        let event = exchange_audit_event(&principal, &grant, expires_at, request_id);
        let access_token = self
            .issuing_key
            .issue_access_token(
                grant.into_access_grant(principal, roles, permissions),
                self.settings.access_ttl,
            )
            .map_err(|error| match &scope_mint {
                Some((_, root, _)) => match issue_scope_error(error, root) {
                    IssueErrorOrWyrd::Issue(error) => IssuanceError::Issue(error),
                    IssueErrorOrWyrd::Wyrd(error) => IssuanceError::Wyrd(error),
                },
                None => IssuanceError::Issue(error),
            })?;

        // Two decisions, two records. The exchange says a grant was spent to
        // obtain a token and which credential; the scope mint says what emit
        // authority the Card conferred.
        append_auth_audit(conn, &event).await?;
        if let Some((mint_kind, root, scope)) = scope_mint {
            write_scope_mint_success_audit(
                conn,
                principal_id,
                &root,
                &scope,
                request_id,
                mint_kind,
            )
            .await?;
        }

        Ok(ExchangedToken {
            access_token: SecretString::from(access_token),
            refresh_token: None,
            token_type: TokenType::Bearer,
            expires_at,
        })
    }

    /// Mint the tenant's internal SYSTEM writer a token scoped to one Verifier.
    ///
    /// The server's own verification-result writes run under this token. It
    /// reads, in the caller's transaction, whether the tenant admits
    /// credentials and the tenant's persisted SYSTEM principal, then signs a
    /// normal tenant access token whose principal is `kind=system` with no root
    /// Card, whose scope is exactly `verifier`, and whose authority is exactly
    /// `bifrost_record:write` — no roles, no credential attribution, no
    /// delegation. The lifetime is the configured access TTL capped at five
    /// minutes. This is internal plumbing, not an authorization decision, so it
    /// appends no audit; Gate audits each admission the token is spent on.
    ///
    /// # Errors
    /// Returns [`IssuanceError::SystemScopeInvalid`] when `verifier` is not a
    /// UID-bearing Verifier Card, [`IssuanceError::TenantNotAdmitting`] for a
    /// tenant that admits no credentials,
    /// [`IssuanceError::SystemPrincipalInvalid`] when the tenant has no SYSTEM
    /// principal or its id is not a `UUIDv7`, [`IssuanceError::Database`] when a
    /// read fails, and [`IssuanceError::Issue`] when signing fails.
    #[tracing::instrument(level = "debug", skip(self, conn), fields(verifier = %verifier), err)]
    pub async fn issue_system_token(
        &self,
        conn: &mut TenantConn<'_>,
        verifier: &CardRef,
    ) -> Result<ExchangedToken, IssuanceError> {
        self.issue_system(conn, verifier, Permission::bifrost_record_write())
            .await
    }

    /// Mint the tenant's SYSTEM Drift reader a token scoped to one Verifier.
    ///
    /// The Drift runner reads its run's observations through the ordinary query
    /// service under this token. It resolves, in the caller's transaction, the
    /// UID of the tenant's registered `vala.drift.observations` table without
    /// creating it, then signs the same closed SYSTEM claim set as
    /// [`Self::issue_system_token`] whose only authority is
    /// [`Permission::drift_table_read`] of that table. The Verifier scope is
    /// attribution only: Oracle's table authorization enforces the read, and
    /// the runner's fixed SQL, not this token, limits subject, series, and
    /// window. Like the result-write mint it appends no audit; Oracle audits
    /// each read the token is spent on.
    ///
    /// Returns `Ok(None)` when the tenant has never registered the observation
    /// table, which the runner scores as an empty window.
    ///
    /// # Errors
    /// Returns the errors of [`Self::issue_system_token`], and
    /// [`IssuanceError::Store`] when the table row cannot be read or its
    /// stored UID is not 16 bytes.
    #[tracing::instrument(level = "debug", skip(self, conn), fields(verifier = %verifier), err)]
    pub async fn issue_system_drift_read_token(
        &self,
        conn: &mut TenantConn<'_>,
        verifier: &CardRef,
    ) -> Result<Option<ExchangedToken>, IssuanceError> {
        let Some(table) = get_by_fqn(conn, DRIFT_OBSERVATIONS).await? else {
            return Ok(None);
        };
        let table_uid = Uuid::from_slice(&table.table_uid).map_err(|_| {
            IssuanceError::Store(SqlError::InvariantViolation {
                detail: "stored Bifrost table UID is not 16 bytes".to_owned(),
            })
        })?;
        self.issue_system(conn, verifier, Permission::drift_table_read(table_uid))
            .await
            .map(Some)
    }

    /// Sign the closed SYSTEM claim set for `verifier` carrying only `permission`.
    ///
    /// # Errors
    /// Returns the errors documented on [`Self::issue_system_token`].
    async fn issue_system(
        &self,
        conn: &mut TenantConn<'_>,
        verifier: &CardRef,
        permission: Permission,
    ) -> Result<ExchangedToken, IssuanceError> {
        if verifier.kind != CardKind::Verifier || verifier.uid.is_none() {
            return Err(IssuanceError::SystemScopeInvalid);
        }
        let tenant = conn.data_tenant_id();
        if !tenant_admits_credentials(conn, tenant).await? {
            return Err(IssuanceError::TenantNotAdmitting);
        }
        let principal_id = system_principal_id(conn)
            .await?
            .filter(|id| id.get_version_num() == 7)
            .ok_or(IssuanceError::SystemPrincipalInvalid)?;
        let ttl = self.settings.access_ttl.min(Duration::minutes(5));
        let expires_at = Utc::now() + ttl;
        let access_token = self.issuing_key.issue_access_token(
            AccessGrant {
                principal: TokenPrincipalRef {
                    id: PrincipalId::new(principal_id),
                    kind: PrincipalKindTag::System,
                    tenant_id: tenant,
                    card_ref: None,
                    card_ref_scope: CardRefScope::own(verifier),
                },
                roles: Vec::new(),
                permissions: PermissionSet::from_iter([permission]),
                credential_id: None,
                act: None,
                audience: TokenAudience::Wyrd,
            },
            ttl,
        )?;
        Ok(ExchangedToken {
            access_token: SecretString::from(access_token),
            refresh_token: None,
            token_type: TokenType::Bearer,
            expires_at,
        })
    }

    /// Establish or renew a human session: an access token from [`Self::issue`]
    /// plus a rotated refresh token.
    ///
    /// `rotated_from` is absent at first login and carries the consumed refresh
    /// row on renewal; it is both the family back-link that makes reuse
    /// detectable and the credential attribution of the renewed access token.
    /// Only human sessions get a refresh token — a machine re-exchanges its
    /// durable credential instead.
    ///
    /// # Errors
    /// Returns every [`Self::issue`] error, [`IssuanceError::Issue`] when the
    /// refresh token cannot be signed, and [`IssuanceError::Database`] when
    /// the refresh row cannot be written. Nothing is committed here.
    pub async fn issue_human_session(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        rotated_from: Option<Uuid>,
        request_id: &str,
    ) -> Result<ExchangedToken, IssuanceError> {
        let grant = match rotated_from {
            Some(consumed) => TenantGrant::Refresh { consumed },
            None => TenantGrant::OidcLogin,
        };
        let access = self.issue(conn, principal_id, grant, request_id).await?;
        // One PostgreSQL instant, sampled in the caller's transaction, feeds both
        // the signed `exp` and the durable row expiry that PostgreSQL later
        // evaluates, so the two can never disagree.
        let issued_at = refresh_issuance_instant(conn).await?;
        let refresh_token = self.issuing_key.issue_refresh_token(
            PrincipalKindTag::User,
            PrincipalId::new(principal_id),
            conn.data_tenant_id(),
            issued_at,
            self.settings.refresh_ttl,
        )?;
        let refresh_expires_at = issued_at + self.settings.refresh_ttl;
        let hash = token_hash(&refresh_token);
        let successor = Uuid::new_v4();
        match rotated_from {
            Some(predecessor) => {
                insert_refresh_token_rotated(
                    conn,
                    successor,
                    "user",
                    principal_id,
                    &hash,
                    refresh_expires_at,
                    predecessor,
                )
                .await?;
            }
            None => {
                insert_refresh_token(
                    conn,
                    successor,
                    "user",
                    principal_id,
                    &hash,
                    refresh_expires_at,
                )
                .await?;
            }
        }
        Ok(ExchangedToken {
            refresh_token: Some(SecretString::from(refresh_token)),
            ..access
        })
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
async fn resolve_permissions(
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
/// — the actor — as the exchange's actor, records the Card references of the
/// earlier actors earliest first, targets the requested audience, records
/// the invoke action the exchange's policy evaluated as its permission, and
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
            issued.id,
            issued.kind,
            issued.card_ref.clone(),
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
        delegation_chain.extend(act.principal.card_ref.clone());
        layer = act.act.as_deref();
    }
    delegation_chain.reverse();
    let mut event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        subject.id,
        subject.kind,
        subject.card_ref.clone(),
        AuditOutcome::Allowed,
        AuditDetail::TokenExchange {
            subject_principal_id: subject.id,
            actor_principal_id: issued.id,
            delegation_chain,
            expires_at,
        },
    );
    audience.as_str().clone_into(&mut event.resource);
    DELEGATION_POLICY_ACTION.clone_into(&mut event.permission);
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
                "reader",
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
                    "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}
                },
                {
                    "resource": "bifrost_query",
                    "action": "read",
                    "scope": {"bifrost": {"table": {
                        "catalog": "vala",
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
                catalog: "vala".to_owned(),
                schema: "logs".to_owned(),
                table_uid: Uuid::now_v7(),
            })
        ))));
        assert!(set.contains(&scoped_query_read(PermissionScope::Bifrost(
            BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "vala".to_owned(),
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
                        "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}
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
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, PermissionSet, PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{
        grant_role_to_service_account, grant_role_to_user, insert_role, insert_service_account,
        insert_user, provision_system_principal, revoke_role_from_service_account,
        suspend_service_account_principal, suspend_user_principal,
    };

    use wyrd_sql::queries::verification::{
        BindingActivation, FrozenTarget, NewBinding, project_bindings,
    };

    use super::{IssuanceError, TenantGrant, TenantTokenIssuer, TokenExchangeSettings};

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    /// The test signing key, shared by the issuer and the verifier.
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

    /// The issuer under test with the default lifetimes.
    fn issuer() -> TenantTokenIssuer {
        TenantTokenIssuer::new(issuing_key(), TokenExchangeSettings::default())
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
    async fn seed_automation(conn: &mut TenantConn<'_>, created_by: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        insert_service_account(conn, id, "service", None, "automation", None, created_by)
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
    #[tokio::test]
    async fn issue_signs_current_grants_and_follows_grant_changes() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let role_id = seed_card_reader_role(&mut conn).await;
        let creator = seed_user(&mut conn).await;
        let principal = seed_automation(&mut conn, creator).await;
        grant_role_to_service_account(&mut conn, principal, role_id)
            .await
            .expect("role grants");

        let granted = issuer()
            .issue(&mut conn, principal, TenantGrant::JwtBearer, "req-granted")
            .await
            .expect("issuance succeeds");
        assert!(grants_card_read(&granted.access_token, tenant));

        revoke_role_from_service_account(&mut conn, principal, role_id)
            .await
            .expect("role revokes");
        let withdrawn = issuer()
            .issue(
                &mut conn,
                principal,
                TenantGrant::JwtBearer,
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
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
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
                },
            ),
            (machine, TenantGrant::JwtBearer),
            (
                machine,
                TenantGrant::Delegation {
                    subject: Box::new(subject),
                    subject_roles: Vec::new(),
                    subject_permissions: wyrd_runtime::PermissionSet::new(),
                    prior_act: None,
                    audience: TokenAudience::Bifrost,
                    actor_credential_id: None,
                },
            ),
        ];
        for (principal, grant) in cases {
            let label = format!("{grant:?}");
            let result = issuer()
                .issue(&mut conn, principal, grant, "req-suspended")
                .await;
            assert!(
                matches!(result, Err(IssuanceError::PrincipalInactive)),
                "{label} must refuse a suspended principal, got {result:?}"
            );
        }
    }

    /// Seed an active Service Card with one `schedule` binding and its
    /// Card-bound principal, returning the principal id.
    ///
    /// # Panics
    /// Panics when a generated identity is invalid or any fixture write fails.
    async fn seed_bound_service(conn: &mut TenantConn<'_>, created_by: Uuid) -> Uuid {
        let card_uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
        sqlx::query(
            "INSERT INTO wyrd.cards \
                (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
             VALUES ($1, wyrd.current_tenant(), 'Service', 'default', 'bound-svc', '1.0.0', \
                     '{}'::jsonb, 'spec-hash', 'active')",
        )
        .bind(card_uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("service card inserts");
        let card_ref: CardRef = serde_json::from_value(json!({
            "kind": "Service", "name": "bound-svc", "version": "1.0.0",
            "space": "default", "uid": card_uid.as_str()
        }))
        .expect("card ref decodes");
        let id = Uuid::now_v7();
        insert_service_account(
            conn,
            id,
            "service",
            Some(&card_ref),
            "bound-svc",
            None,
            created_by,
        )
        .await
        .expect("bound service account inserts");
        let verifier = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
        project_bindings(
            conn,
            &card_uid,
            &CardKind::Service,
            &[NewBinding {
                subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
                subject_card_uid: card_uid.clone(),
                verifier_uid: verifier,
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
        id
    }

    /// Read a machine principal's activity and its binding's schedule cursor.
    ///
    /// # Panics
    /// Panics when the read fails or the principal has no schedule binding.
    async fn activity(
        conn: &mut TenantConn<'_>,
        principal: Uuid,
    ) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        sqlx::query_as(
            "SELECT p.last_authenticated_at, b.next_run_at \
               FROM wyrd.auth_service_accounts p \
               LEFT JOIN wyrd.verification_bindings b ON b.owner_card_uid = p.card_uid \
              WHERE p.id = $1",
        )
        .bind(principal)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("activity reads")
    }

    /// An API-key exchange activates a Card-bound owner and arms its schedule;
    /// a later `jwt-bearer` issuance renews activity without moving the cursor.
    ///
    /// # Panics
    /// Panics when the fixture cannot start or seed, an issuance fails, or an
    /// activity or cursor expectation does not hold.
    #[tokio::test]
    async fn qualifying_machine_grants_activate_the_bound_owner() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = seed_user(&mut conn).await;
        let principal = seed_bound_service(&mut conn, creator).await;
        assert_eq!(activity(&mut conn, principal).await, (None, None));

        issuer()
            .issue(
                &mut conn,
                principal,
                TenantGrant::ApiKey {
                    credential_id: Uuid::new_v4(),
                },
                "req-api-key",
            )
            .await
            .expect("api-key issuance succeeds");
        let (first, armed) = activity(&mut conn, principal).await;
        let first = first.expect("api-key exchange records activity");
        let armed = armed.expect("first exchange arms the schedule");
        assert!(armed > first, "the cursor is the next future boundary");

        issuer()
            .issue(
                &mut conn,
                principal,
                TenantGrant::JwtBearer,
                "req-jwt-bearer",
            )
            .await
            .expect("jwt-bearer issuance succeeds");
        let (renewed, cursor) = activity(&mut conn, principal).await;
        assert!(renewed.expect("renewal records activity") >= first);
        assert_eq!(cursor, Some(armed), "renewal never moves an armed cursor");
    }

    /// Delegation for a Card-bound owner and any grant for a Card-free
    /// principal never record activity or arm a schedule.
    ///
    /// # Panics
    /// Panics when the fixture cannot start or seed, an issuance fails, or any
    /// activity or cursor is written.
    #[tokio::test]
    async fn non_qualifying_grants_never_touch_activity() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = seed_user(&mut conn).await;
        let bound = seed_bound_service(&mut conn, creator).await;
        let automation = seed_automation(&mut conn, creator).await;
        let subject = TokenPrincipalRef {
            id: PrincipalId::new(Uuid::new_v4()),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        };

        issuer()
            .issue(
                &mut conn,
                bound,
                TenantGrant::Delegation {
                    subject: Box::new(subject),
                    subject_roles: Vec::new(),
                    subject_permissions: PermissionSet::new(),
                    prior_act: None,
                    audience: TokenAudience::Wyrd,
                    actor_credential_id: None,
                },
                "req-delegation",
            )
            .await
            .expect("delegated issuance succeeds");
        assert_eq!(activity(&mut conn, bound).await, (None, None));

        for grant in [
            TenantGrant::ApiKey {
                credential_id: Uuid::new_v4(),
            },
            TenantGrant::JwtBearer,
        ] {
            issuer()
                .issue(&mut conn, automation, grant, "req-card-free")
                .await
                .expect("card-free issuance succeeds");
        }
        assert_eq!(activity(&mut conn, automation).await, (None, None));
    }

    /// A human session carries the user's current grants and a refresh token.
    #[tokio::test]
    async fn a_human_session_carries_current_grants_and_a_refresh_token() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let role_id = seed_card_reader_role(&mut conn).await;
        let user = seed_user(&mut conn).await;
        grant_role_to_user(&mut conn, user, role_id)
            .await
            .expect("role grants");

        let session = issuer()
            .issue_human_session(&mut conn, user, None, "req-session")
            .await
            .expect("session issues");

        assert!(grants_card_read(&session.access_token, tenant));
        assert!(session.refresh_token.is_some(), "a human session renews");
    }

    /// Build a UID-bearing Verifier reference, the only admissible SYSTEM scope.
    ///
    /// # Panics
    /// Panics when a static identity component is invalid.
    fn verifier_card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Verifier,
            name: wyrd_spec::ids::CardName::new(name).expect("static name is valid"),
            version: wyrd_semver::VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(wyrd_spec::ids::SpaceName::new("prod").expect("static space is valid")),
            uid: Some(CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID")),
        }
    }

    /// Count every staged audit row across tenants.
    ///
    /// # Panics
    /// Panics when the migrator pool or the count fails.
    async fn staged_audit_rows(fixture: &PgFixture) -> i64 {
        let pool = fixture.superuser_pool().await.expect("migrator pool");
        sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging")
            .fetch_one(&pool)
            .await
            .expect("audit rows count")
    }

    /// The SYSTEM mint round-trips through the request-path verifier as the
    /// tenant's persisted writer scoped to exactly one Verifier, with only
    /// `bifrost_record:write`, no roles, credential, or delegation, a lifetime
    /// of at most five minutes, and no audit row of its own. The token is
    /// refused under any other tenant.
    ///
    /// # Panics
    /// Panics when minting fails, the verified principal deviates from the
    /// SYSTEM contract, the mint stages audit, or another tenant accepts it.
    #[tokio::test]
    async fn issue_system_token_round_trips_one_verifier_scope_without_audit() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let other_tenant = fixture
            .seed_additional_tenant(&format!("system-other-{}", Uuid::now_v7()))
            .await
            .expect("second tenant seeds");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let writer = provision_system_principal(&mut conn)
            .await
            .expect("writer provisions");
        let verifier_ref = verifier_card_ref("drift");
        let audit_before = staged_audit_rows(&fixture).await;
        let long_lived = TenantTokenIssuer::new(
            issuing_key(),
            TokenExchangeSettings {
                access_ttl: chrono::Duration::hours(1),
                refresh_ttl: chrono::Duration::days(1),
            },
        );

        let issued_at = Utc::now();
        let minted = long_lived
            .issue_system_token(&mut conn, &verifier_ref)
            .await
            .expect("system token mints");
        conn.commit().await.expect("mint transaction commits");

        assert_eq!(
            staged_audit_rows(&fixture).await,
            audit_before,
            "minting is not an authorization decision and stages no audit"
        );
        assert!(minted.refresh_token.is_none());
        assert!(
            minted.expires_at
                <= issued_at + chrono::Duration::minutes(5) + chrono::Duration::seconds(1)
        );
        let verified = verifier()
            .verify(&minted.access_token, &tenant)
            .expect("system token verifies");
        assert_eq!(verified.principal.id, PrincipalId::new(writer));
        assert_eq!(
            verified.principal.kind,
            PrincipalKind::System {
                card_ref_scope: CardRefScope::own(&verifier_ref)
            }
        );
        assert_eq!(verified.principal.card_ref(), None);
        assert!(verified.principal.roles.is_empty());
        assert_eq!(verified.principal.credential_id, None);
        assert!(verified.delegation_chain.is_empty());
        assert_eq!(
            verified.principal.effective_permissions,
            PermissionSet::from_iter([Permission::bifrost_record_write()])
        );
        assert!(verified.principal.authorizes_card(&verifier_ref));
        assert!(
            !verified
                .principal
                .authorizes_card(&verifier_card_ref("other")),
            "the scope names exactly the requested Verifier"
        );
        assert!(
            verified.exp <= issued_at + chrono::Duration::minutes(5) + chrono::Duration::seconds(1),
            "the token outlives no five-minute window"
        );
        assert!(
            verifier()
                .verify(&minted.access_token, &other_tenant)
                .is_err(),
            "a SYSTEM token cannot be replayed in another tenant"
        );
    }

    /// The SYSTEM mint refuses a scope that is not exactly one UID-bearing
    /// Verifier, a tenant with no provisioned writer, and a writer whose
    /// stored id is not a `UUIDv7`.
    ///
    /// # Panics
    /// Panics when any malformed request mints a token.
    #[tokio::test]
    async fn issue_system_token_refuses_bad_scopes_and_missing_or_malformed_writers() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let verifier_ref = verifier_card_ref("drift");

        let missing = issuer().issue_system_token(&mut conn, &verifier_ref).await;
        assert!(
            matches!(missing, Err(IssuanceError::SystemPrincipalInvalid)),
            "an unprovisioned tenant has no writer: {missing:?}"
        );

        provision_system_principal(&mut conn)
            .await
            .expect("writer provisions");
        let service = CardRef {
            kind: CardKind::Service,
            ..verifier_ref.clone()
        };
        let uidless = CardRef {
            uid: None,
            ..verifier_ref.clone()
        };
        for scope in [service, uidless] {
            let result = issuer().issue_system_token(&mut conn, &scope).await;
            assert!(
                matches!(result, Err(IssuanceError::SystemScopeInvalid)),
                "{scope} is not an admissible SYSTEM scope: {result:?}"
            );
        }
        conn.commit().await.expect("writer commits");

        let pool = fixture.superuser_pool().await.expect("migrator pool");
        sqlx::query(
            "UPDATE wyrd.auth_service_accounts SET id = gen_random_uuid() \
              WHERE data_tenant_id = $1 AND principal_kind = 'system'",
        )
        .bind(fixture.data_tenant_id().as_uuid())
        .execute(&pool)
        .await
        .expect("writer id is replaced with a UUIDv4");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn reopens");
        let malformed = issuer().issue_system_token(&mut conn, &verifier_ref).await;
        assert!(
            matches!(malformed, Err(IssuanceError::SystemPrincipalInvalid)),
            "a non-UUIDv7 writer is refused: {malformed:?}"
        );
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
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let writer = provision_system_principal(&mut conn)
            .await
            .expect("writer provisions");
        let subject = TokenPrincipalRef {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        };
        let grants = [
            TenantGrant::ApiKey {
                credential_id: Uuid::now_v7(),
            },
            TenantGrant::JwtBearer,
            TenantGrant::Delegation {
                subject: Box::new(subject),
                subject_roles: Vec::new(),
                subject_permissions: PermissionSet::from_iter([Permission::bifrost_record_write()]),
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
            let result = issuer().issue(&mut conn, writer, grant, "req-system").await;
            assert!(
                matches!(result, Err(IssuanceError::PrincipalInactive)),
                "{label} must refuse the SYSTEM writer, got {result:?}"
            );
        }
        let session = issuer()
            .issue_human_session(&mut conn, writer, None, "req-system-session")
            .await;
        assert!(
            matches!(session, Err(IssuanceError::PrincipalInactive)),
            "a human session cannot be opened for the writer: {session:?}"
        );
    }
}
