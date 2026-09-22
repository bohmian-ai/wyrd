//! API-key exchange and RFC 8693 delegation entry paths.
//!
//! Each verifies only its own grant-specific evidence — the presented API key,
//! or the subject and actor access tokens plus the invoke-policy decision —
//! then mints through the shared [`TenantTokenIssuer`].

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use wyrd_auth_check::{AuthzCheckContext, AuthzCheckRequest, PolicyHook};
use wyrd_auth_verify::{
    ActClaim, AuthError, TokenAudience, TokenPrincipalRef, TokenVerifier, VerifiedToken,
};
use wyrd_runtime::{DelegationStep, PrincipalRef, RoleRef};
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::card::policy::PolicyDecision;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::{
    ApiKeyStatus, api_key_by_prefix, api_key_status_by_prefix, touch_api_key_last_used,
};
use wyrd_sql::{SqlError, TenantConn};

use crate::audit::{TOKEN_EXCHANGE_OPERATION, append_auth_audit, audit_request_id, auth_event};
use crate::credential_verify::verify_presented;
use crate::error::auth_error_to_wyrd;
use crate::issuance::{ExchangedToken, IssuanceError, TenantGrant, TenantTokenIssuer};
use crate::issue_api_key::WyrdApiKey;

/// Invoke-policy action evaluated for an A-to-B token exchange.
///
/// The policy is asked whether the subject may invoke the actor's Card, and
/// the answer decides whether the actor may act on the subject's behalf.
pub const DELEGATION_POLICY_ACTION: &str = "invoke";

/// API-key exchange service.
#[derive(Clone, Debug)]
pub struct ExchangeApiKey {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
}

/// RFC 8693 token-exchange service: an actor obtains a token to act on behalf
/// of a subject.
#[derive(Clone)]
pub struct DelegateToken {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
    /// Wyrd access-token verifier for the subject and actor tokens.
    pub verifier: std::sync::Arc<TokenVerifier>,
    /// Cross-service invoke policy deciding whether the actor may act for the
    /// subject.
    pub policy: std::sync::Arc<dyn PolicyHook>,
}

impl std::fmt::Debug for DelegateToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateToken")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
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
    /// Key exists but the associated principal is not active.
    #[error("principal is not active")]
    AccountDisabled,
    /// The key's tenant is not in a state that admits credentials.
    ///
    /// Distinct from [`ExchangeError::AccountDisabled`] internally so a
    /// suspended tenant is diagnosable, and rendered identically at the
    /// boundary so a caller cannot tell a suspended tenant from a disabled
    /// principal or an unknown key.
    #[error("tenant does not admit credentials")]
    TenantNotAdmitting,
    /// Key exists but Argon2 hash verification failed.
    #[error("api key hash mismatch")]
    HashMismatch,
    /// Blocking task failed.
    #[error("api key verify task failed")]
    Join(#[from] tokio::task::JoinError),
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    /// The shared issuance workflow failed for a reason other than an
    /// inactive tenant or principal.
    #[error("token issuance failed")]
    Issuance(IssuanceError),
}

impl From<IssuanceError> for ExchangeError {
    fn from(error: IssuanceError) -> Self {
        match error {
            IssuanceError::TenantNotAdmitting => Self::TenantNotAdmitting,
            IssuanceError::PrincipalInactive => Self::AccountDisabled,
            IssuanceError::Database(error) => Self::Database(error),
            other => Self::Issuance(other),
        }
    }
}

/// Token-exchange failure.
#[derive(Debug, thiserror::Error)]
pub enum DelegateError {
    /// The subject token failed verification.
    #[error("subject token invalid")]
    InvalidSubjectToken(AuthError),
    /// The actor token failed verification.
    #[error("actor token invalid")]
    InvalidActorToken(AuthError),
    /// The identity input cannot express one actor acting for one subject:
    /// a delegated actor token, an actor that is not a Card-bound Service or
    /// Agent, or a party exchanging with itself.
    #[error("token exchange identity input is malformed: {0}")]
    MalformedIdentity(&'static str),
    /// The actor's principal is missing or inactive at issuance.
    #[error("actor principal not found")]
    ActorNotFound,
    /// The invoke policy refused to let the actor act for the subject.
    #[error("invoke policy denied the exchange: {0}")]
    PolicyDenied(String),
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    /// The shared issuance workflow failed.
    #[error("token issuance failed")]
    Issuance(IssuanceError),
    /// The exchange decision could not be committed.
    #[error("token exchange decision commit failed")]
    Commit(#[from] SqlError),
}

impl DelegateError {
    /// Whether the failure left the transaction unable to record a decision:
    /// a store read, or an audit append that already failed.
    fn is_store_failure(&self) -> bool {
        matches!(
            self,
            Self::Database(_)
                | Self::Issuance(
                    IssuanceError::Database(_)
                        | IssuanceError::Wyrd(WyrdError::AuditUnavailable { .. })
                )
        )
    }
}

impl From<IssuanceError> for DelegateError {
    fn from(error: IssuanceError) -> Self {
        match error {
            IssuanceError::PrincipalInactive => Self::ActorNotFound,
            other => Self::Issuance(other),
        }
    }
}

impl ExchangeApiKey {
    /// Exchange a Wyrd API key for a tenant access token.
    ///
    /// Verifies the key itself — format, tenant, an unexpired unrevoked row
    /// for its prefix, and the Argon2 secret — at a fixed cost, records the
    /// key's use, then mints through the shared issuance workflow, which
    /// refuses an inactive tenant or principal and resolves current grants.
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
        // what a refusal costs. A malformed key, another tenant's key, or an
        // unknown prefix must not return before verification runs, or a live
        // prefix with a wrong tail would take measurably longer than any of
        // them — enough to enumerate live prefixes by clock.
        let (row, refusal) = match WyrdApiKey::parse(api_key.expose_secret()) {
            Err(_) => (None, Some(ExchangeError::NotFound)),
            Ok(parsed) if parsed.tenant_id != conn.data_tenant_id() => {
                (None, Some(ExchangeError::CrossTenant))
            }
            Ok(parsed) => match api_key_by_prefix(conn, &parsed.prefix).await? {
                None => (None, Some(ExchangeError::NotFound)),
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

        touch_api_key_last_used(conn, row.api_key_id).await?;
        Ok(self
            .issuer
            .issue(
                conn,
                row.principal_id,
                TenantGrant::ApiKey {
                    credential_id: row.api_key_id,
                },
                request_id,
            )
            .await?)
    }
}

impl DelegateToken {
    /// Exchange a subject token and an actor token for a token that lets the
    /// actor act on the subject's behalf at `audience`, committing the
    /// decision on `conn`.
    ///
    /// Both tokens are verified locally for the connection's tenant, so a
    /// cross-tenant pair fails verification; unverifiable or malformed
    /// identity input is refused before any decision and records nothing.
    /// The invoke policy is then asked whether the subject may invoke the
    /// actor's Card for `audience`, and exactly one canonical audit row is
    /// committed: a denial commits its denied row; an allowance that mints
    /// commits the issuer's token-exchange row with the token; an allowance
    /// followed by an issuance refusal commits an allowed row with no effect.
    /// The issued token names the subject as principal, the actor as its
    /// outermost `act` with the subject token's earlier actors nested inside,
    /// and carries the actor's current permissions narrowed to the subject's.
    /// A store or audit failure commits nothing and serves no token.
    ///
    /// # Errors
    /// Returns [`DelegateError::InvalidSubjectToken`] or
    /// [`DelegateError::InvalidActorToken`] for an unverifiable token,
    /// [`DelegateError::MalformedIdentity`] for input that cannot name one
    /// actor acting for one subject, [`DelegateError::PolicyDenied`] when the
    /// invoke policy refuses, [`DelegateError::ActorNotFound`] for a missing or
    /// inactive actor, [`DelegateError::Issuance`] when issuance or an audit
    /// append fails, [`DelegateError::Database`] when a read fails, and
    /// [`DelegateError::Commit`] when the decision cannot be committed.
    #[tracing::instrument(
        level = "debug",
        skip(self, conn, subject_token, actor_token),
        fields(audience = audience.as_str()),
        err
    )]
    pub async fn execute(
        &self,
        mut conn: TenantConn<'_>,
        subject_token: SecretString,
        actor_token: SecretString,
        audience: TokenAudience,
        request_id: &str,
    ) -> Result<ExchangedToken, DelegateError> {
        let tenant = conn.data_tenant_id();
        let subject = self
            .verifier
            .verify(&subject_token, &tenant)
            .map_err(DelegateError::InvalidSubjectToken)?;
        let actor = self
            .verifier
            .verify(&actor_token, &tenant)
            .map_err(DelegateError::InvalidActorToken)?;
        let context = policy_context(&subject, &actor, audience, request_id)?;
        let decision = self.policy.evaluate(&context).await;
        if !matches!(decision, PolicyDecision::Allow) {
            record_decision(
                &mut conn,
                &context,
                &actor,
                audience,
                AuditOutcome::Denied,
                request_id,
            )
            .await?;
            conn.commit().await?;
            return Err(DelegateError::PolicyDenied(match decision {
                PolicyDecision::Deny { reason } => reason,
                _ => "unsupported_decision".to_owned(),
            }));
        }
        let grant = TenantGrant::Delegation {
            subject: TokenPrincipalRef::from(&subject.principal),
            subject_roles: subject.principal.roles.clone(),
            subject_permissions: subject.principal.effective_permissions.clone(),
            prior_act: act_from_chain(&subject.delegation_chain, tenant),
            audience,
            actor_credential_id: actor.principal.credential_id,
        };
        match self
            .issuer
            .issue(&mut conn, actor.principal.id.as_uuid(), grant, request_id)
            .await
            .map_err(DelegateError::from)
        {
            Ok(exchanged) => {
                conn.commit().await?;
                Ok(exchanged)
            }
            Err(error) if error.is_store_failure() => Err(error),
            Err(error) => {
                record_decision(
                    &mut conn,
                    &context,
                    &actor,
                    audience,
                    AuditOutcome::Allowed,
                    request_id,
                )
                .await?;
                conn.commit().await?;
                Err(error)
            }
        }
    }
}

/// Build the invoke-policy question for one exchange: may the subject invoke
/// the actor's Card for `audience`?
///
/// The actor must present a direct token for a Card-bound Service or Agent,
/// and must not be the subject itself; the subject's earlier actors precede
/// the new actor in the chain.
///
/// # Errors
/// Returns [`DelegateError::MalformedIdentity`] for a delegated actor token,
/// an actor without a Service or Agent Card, or a self-exchange.
fn policy_context(
    subject: &VerifiedToken,
    actor: &VerifiedToken,
    audience: TokenAudience,
    request_id: &str,
) -> Result<AuthzCheckContext, DelegateError> {
    if !actor.delegation_chain.is_empty() {
        return Err(DelegateError::MalformedIdentity("actor_token_is_delegated"));
    }
    if actor.principal.id == subject.principal.id {
        return Err(DelegateError::MalformedIdentity("actor_is_subject"));
    }
    let Some(target) = actor.principal.card_ref().cloned() else {
        return Err(DelegateError::MalformedIdentity("actor_not_card_bound"));
    };
    let actor_ref = PrincipalRef::from_principal(&actor.principal);
    let chain = subject
        .delegation_chain
        .iter()
        .map(|step| step.principal.clone())
        .chain(std::iter::once(actor_ref.clone()))
        .collect();
    Ok(AuthzCheckContext {
        subject: subject.principal.clone(),
        actor: actor_ref,
        chain,
        request: AuthzCheckRequest {
            target,
            action: DELEGATION_POLICY_ACTION.to_owned(),
            context: json!({ "audience": audience.as_str() }),
        },
        metadata: None,
        request_id: audit_request_id(request_id),
    })
}

/// Append the exchange decision for an exchange that minted no token.
///
/// A minted token's decision is the issuer's token-exchange row, so this is
/// only for a policy denial or an allowance whose issuance then refused. Like
/// every delegated request, the row is recorded under the subject with the
/// full actor chain, targets the requested audience, and attaches the
/// credential that authenticated the actor.
///
/// # Errors
/// Returns [`DelegateError::Issuance`] carrying the audit-unavailable error
/// when the append fails.
async fn record_decision(
    conn: &mut TenantConn<'_>,
    context: &AuthzCheckContext,
    actor: &VerifiedToken,
    audience: TokenAudience,
    outcome: AuditOutcome,
    request_id: &str,
) -> Result<(), DelegateError> {
    let subject = &context.subject;
    let chain: Vec<DelegationStep> = context
        .chain
        .iter()
        .map(|principal| DelegationStep {
            principal: principal.clone(),
        })
        .collect();
    let mut event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        subject.id,
        subject.kind.tag(),
        subject.card_ref().cloned(),
        outcome,
        AuditDetail::DelegationAttribution {
            delegation_chain: wyrd_runtime::audit_delegation_chain(&chain),
        },
    )
    .with_credential_id(actor.principal.credential_id);
    event.resource = audience.as_str().to_owned();
    append_auth_audit(conn, &event)
        .await
        .map_err(|error| DelegateError::Issuance(IssuanceError::Wyrd(error)))
}

/// Convert stored role names into runtime role references.
pub(crate) fn role_refs(names: Vec<String>) -> Result<Vec<RoleRef>, wyrd_runtime::InvalidRoleName> {
    names.into_iter().map(|name| RoleRef::new(&name)).collect()
}

/// Convert a stored tenant machine-principal kind into the token wire enum.
///
/// `None` for an unrecognized value, which every caller must treat as a
/// refusal rather than defaulting.
pub fn principal_kind_wire(value: &str) -> Option<PrincipalKindTag> {
    match value {
        "tenant_admin" => Some(PrincipalKindTag::TenantAdmin),
        "service" => Some(PrincipalKindTag::Service),
        "agent" => Some(PrincipalKindTag::Agent),
        _ => None,
    }
}

/// Rebuild an RFC 8693 `act` chain (current actor outermost) from a verified
/// earliest-first actor chain.
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
        ExchangeError::Join(_) => {
            return WyrdError::Internal {
                message: "failed to exchange API key".to_owned(),
                details: json!({}),
            };
        }
        ExchangeError::Issuance(error) => return error.into(),
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
            DelegateError::InvalidSubjectToken(error)
            | DelegateError::InvalidActorToken(error) => auth_error_to_wyrd(error),
            DelegateError::MalformedIdentity(reason) => WyrdError::Validation {
                message: "token exchange identity input is malformed".to_owned(),
                details: json!({ "reason": reason }),
            },
            DelegateError::ActorNotFound => WyrdError::PrincipalNotFound {
                message: "actor principal not found in tenant".to_owned(),
                details: json!({}),
            },
            DelegateError::PolicyDenied(reason) => WyrdError::PolicyDenied {
                message: "invoke policy denied the token exchange".to_owned(),
                details: json!({ "reason": reason }),
            },
            DelegateError::Database(error) => IssuanceError::Database(error).into(),
            DelegateError::Issuance(error) => error.into(),
            DelegateError::Commit(error) => {
                tracing::warn!(error = %error, "token exchange decision commit failed");
                WyrdError::AuthVerifyUnavailable {
                    message: "auth backend unavailable".to_owned(),
                    details: json!({ "retry_after_seconds": 1 }),
                }
            }
        }
    }
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::audit::{CARD_SCOPE_MINT_OPERATION, TOKEN_EXCHANGE_OPERATION};

    use chrono::{Duration, Utc};
    use secrecy::SecretString;
    use serde_json::Value as JsonValue;
    use sqlx::types::Json;
    use uuid::Uuid;
    use wyrd_auth_check::{DenyAllPolicyHook, PolicyHook, RecordingPolicyHook};
    use wyrd_auth_issue::{AccessGrant, IssuingKey};
    use wyrd_auth_verify::{
        ActClaim, AuthError, Kid, TokenAudience, TokenPrincipalRef, TokenVerifier,
        WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_dev_fixtures::cards::seed_backing_card;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{
        Action, BifrostPermissionScope, BifrostTableScope, Permission, PermissionScope,
        PermissionSet, PrincipalId, Resource,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::vala::api::AuditDetail;
    use wyrd_sql::TenantConn;

    use crate::credential_verify;
    use wyrd_sql::queries::auth::{ApiKeyStatus, grant_role_to_service_account, insert_role};

    use super::{DelegateError, DelegateToken, ExchangeApiKey, ExchangeError};
    use crate::issuance::{IssuanceError, TenantTokenIssuer, TokenExchangeSettings};
    use crate::issue_api_key::WyrdApiKey;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    fn test_service_card_ref() -> CardRef {
        named_service_card_ref("test-service")
    }

    /// A static Service Card reference named `name`.
    fn named_service_card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static name is valid"),
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

    fn test_issuer() -> TenantTokenIssuer {
        TenantTokenIssuer::new(test_issuing_key(), TokenExchangeSettings::default())
    }

    fn exchange_service() -> ExchangeApiKey {
        ExchangeApiKey {
            issuer: test_issuer(),
        }
    }

    /// Build an exchange service whose verifier trusts the test signing key and
    /// whose invoke policy is `policy`.
    fn delegate_service(policy: Arc<dyn PolicyHook>) -> DelegateToken {
        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(Kid::new("k1").expect("kid is valid"), Arc::new(public_key));
        DelegateToken {
            issuer: test_issuer(),
            verifier: Arc::new(TokenVerifier::new(
                decoding_keys,
                "wyrd",
                WyrdAuthVerifySettings::default(),
            )),
            policy,
        }
    }

    /// Mint a direct `wyrd`-audience token for `principal` carrying
    /// `permissions`, attributed to `credential_id`, with an optional `act`.
    fn mint(
        principal: TokenPrincipalRef,
        permissions: PermissionSet,
        credential_id: Option<Uuid>,
        act: Option<Box<ActClaim>>,
    ) -> String {
        test_issuing_key()
            .issue_access_token(
                AccessGrant {
                    principal,
                    roles: vec![],
                    permissions,
                    credential_id,
                    act,
                    audience: TokenAudience::Wyrd,
                },
                Duration::minutes(5),
            )
            .expect("test token issues")
    }

    /// A Card-bound Service principal reference named `name`.
    fn service_ref(id: Uuid, tenant: DataTenantId, name: &str) -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: PrincipalId::new(id),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
            card_ref: Some(named_service_card_ref(name)),
            card_ref_scope: CardRefScope::default(),
        }
    }

    /// Mint a Card-bound Service subject token carrying `permissions`.
    fn subject_token(tenant: DataTenantId, permissions: PermissionSet) -> String {
        mint(
            service_ref(Uuid::new_v4(), tenant, "test-service"),
            permissions,
            None,
            None,
        )
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
    fn rendered(error: &WyrdError) -> JsonValue {
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
    /// Seed a live API key for a principal and return its row id and secret.
    ///
    /// The row id is what a grant record must name, so the attribution tests
    /// need it rather than a fresh UUID.
    async fn insert_live_api_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        principal_id: Uuid,
        created_by: Uuid,
    ) -> (Uuid, SecretString) {
        let key = WyrdApiKey::generate(tenant);
        let hash = wyrd_auth_issue::hash_api_key(&key.secret).expect("api key hashes");
        let api_key_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, created_by, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, now() + interval '1 day')",
        )
        .bind(api_key_id)
        .bind(tenant.as_uuid())
        .bind(principal_id)
        .bind(&key.prefix)
        .bind(&hash)
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("live api key inserts");
        (api_key_id, key.secret)
    }

    /// Count the grant records staged for a principal, and read the credential
    /// the single record names.
    async fn staged_exchange(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        principal_id: Uuid,
    ) -> (i64, Option<Uuid>) {
        let rows: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "SELECT principal_id, credential_id FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND operation = $2 AND principal_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(TOKEN_EXCHANGE_OPERATION)
        .bind(principal_id)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("grant record query runs");
        let credential = rows.first().and_then(|(_, credential)| *credential);
        (
            i64::try_from(rows.len()).expect("a test never stages more rows than an i64 holds"),
            credential,
        )
    }

    /// A Card-free tenant grant is recorded and names the key that bought it.
    ///
    /// A tenant administrator binds no Card, so it takes the Card-free branch
    /// and writes no scope mint. Without a grant record on that branch the most
    /// privileged tenant principal could exchange its credential repeatedly and
    /// leave nothing behind to attribute afterwards.
    ///
    /// # Panics
    /// Panics when the exchange fails, when the grant is not recorded exactly
    /// once, or when the record does not name the presented key.
    #[tokio::test]
    async fn a_card_free_exchange_commits_one_attributed_grant_record() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let admin_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, name, status, created_by)
             VALUES ($1, $2, 'tenant_admin', $3, 'active', $4)",
        )
        .bind(admin_id)
        .bind(tenant.as_uuid())
        .bind(format!("admin-{admin_id}"))
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("card-free administrator inserts");
        let (api_key_id, secret) = insert_live_api_key(&mut conn, tenant, admin_id, user_id).await;

        exchange_service()
            .execute(&mut conn, secret, "req-cardfree-grant")
            .await
            .expect("a card-free administrator exchanges its credential");

        let (count, credential) = staged_exchange(&mut conn, tenant, admin_id).await;
        assert_eq!(count, 1, "exactly one grant record is staged");
        assert_eq!(
            credential,
            Some(api_key_id),
            "the grant names the api key that was spent"
        );
    }

    /// A Card-bound tenant grant records the exchange and the scope mint.
    ///
    /// They answer different questions — which credential bought a token, and
    /// what emit authority the Card conferred — so one cannot stand in for the
    /// other.
    ///
    /// # Panics
    /// Panics when the exchange fails, when the grant is not recorded exactly
    /// once naming the key, or when the scope mint is missing.
    #[tokio::test]
    async fn a_card_bound_exchange_records_the_grant_and_the_scope_mint() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, user_id, &card_ref).await;
        let (api_key_id, secret) = insert_live_api_key(&mut conn, tenant, sa_id, user_id).await;

        exchange_service()
            .execute(&mut conn, secret, "req-cardbound-grant")
            .await
            .expect("a card-bound service exchanges its credential");

        let (count, credential) = staged_exchange(&mut conn, tenant, sa_id).await;
        assert_eq!(count, 1, "exactly one grant record is staged");
        assert_eq!(
            credential,
            Some(api_key_id),
            "the grant names the api key that was spent"
        );

        let mints: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND operation = $2 AND principal_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(CARD_SCOPE_MINT_OPERATION)
        .bind(sa_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("scope mint query runs");
        assert_eq!(
            mints, 1,
            "the distinct scope-mint decision is still recorded on its own"
        );
    }

    /// A machine exchange returns access only and writes no refresh row.
    ///
    /// The key itself is the renewable authority, so a stored refresh row
    /// would be a second one an operator cannot see or revoke. Both halves are
    /// asserted: the absent token in the response and the absent row in
    /// `wyrd.auth_refresh_tokens`.
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

    /// Give a suspended service account one live API key and return its secret.
    ///
    /// The suspended-principal refusal needs a credential that verifies and is
    /// then refused by the shared issuer on status, which is a different shape
    /// from the placeholder-hash lifecycle rows [`insert_lifecycle_key`] seeds.
    async fn seed_suspended_account_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        created_by: Uuid,
        base_card: &CardRef,
    ) -> SecretString {
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
        insert_live_api_key(conn, tenant, sa_id, created_by).await.1
    }

    /// Seed a live service-account API key in a second tenant, then suspend
    /// that tenant so it no longer admits credentials.
    ///
    /// Admission is decided on the presented key's own tenant, so the refusal
    /// under test needs a key whose tenant exists, held a live credential, and
    /// was suspended afterwards. Returns the suspended tenant and the key.
    ///
    /// # Panics
    ///
    /// Panics when any seed write, commit, or the suspension fails.
    async fn seed_unadmitted_tenant_key(
        fixture: &PgFixture,
        card_ref: &CardRef,
    ) -> (DataTenantId, SecretString) {
        let unadmitted_tenant = fixture
            .seed_additional_tenant("unadmitted")
            .await
            .expect("second tenant seeds");
        let mut unadmitted_conn = fixture
            .tenant_conn_for(unadmitted_tenant)
            .await
            .expect("unadmitted tenant conn opens");
        let unadmitted_user = insert_test_user(&mut unadmitted_conn, unadmitted_tenant).await;
        let unadmitted_sa = insert_test_service_account(
            &mut unadmitted_conn,
            unadmitted_tenant,
            unadmitted_user,
            card_ref,
        )
        .await;
        let (_, unadmitted) = insert_live_api_key(
            &mut unadmitted_conn,
            unadmitted_tenant,
            unadmitted_sa,
            unadmitted_user,
        )
        .await;
        unadmitted_conn
            .commit()
            .await
            .expect("second tenant seed commits");
        sqlx::query("UPDATE platform.tenants SET status = 'suspended' WHERE data_tenant_id = $1")
            .bind(unadmitted_tenant.as_uuid())
            .execute(
                &fixture
                    .superuser_pool()
                    .await
                    .expect("superuser pool opens"),
            )
            .await
            .expect("second tenant suspends");
        (unadmitted_tenant, unadmitted)
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
        let revoked = WyrdApiKey::generate(tenant);
        let wrong_tail = WyrdApiKey::generate(tenant);
        let foreign = WyrdApiKey::generate(DataTenantId::new_v7());

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

        let disabled = seed_suspended_account_key(&mut conn, tenant, user_id, &card_ref).await;

        let expected = rendered(&super::api_key_invalid());
        let service = exchange_service();
        let cases = [
            ("malformed", SecretString::from("not-a-wyrd-api-key")),
            ("cross_tenant", foreign.secret),
            ("unknown_prefix", unknown.secret),
            ("suspended_account", disabled),
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

        // Admission is decided on the presented key's own tenant, so the key
        // lives in a second tenant that is suspended once it holds a live
        // credential; the shared issuer then refuses it after verification.
        let (unadmitted_tenant, unadmitted) = seed_unadmitted_tenant_key(&fixture, &card_ref).await;
        let mut unadmitted_conn = fixture
            .tenant_conn_for(unadmitted_tenant)
            .await
            .expect("unadmitted tenant conn reopens");
        let before = credential_verify::verifications_performed();
        let error = service
            .execute(&mut unadmitted_conn, unadmitted, "req-unadmitted")
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

    /// Card name of the seeded actor.
    const ACTOR_CARD: &str = "delegation-actor";
    /// Credential the minted actor token is attributed to.
    const ACTOR_CREDENTIAL: Uuid = Uuid::from_u128(0xAC7);

    /// Seed a Card-bound Service actor holding `permissions` through one role
    /// and return its id.
    async fn seed_actor(fixture: &PgFixture, permissions: serde_json::Value) -> Uuid {
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        let actor = insert_test_service_account(
            &mut conn,
            tenant,
            creator,
            &named_service_card_ref(ACTOR_CARD),
        )
        .await;
        let role_id = Uuid::new_v4();
        insert_role(&mut conn, role_id, "delegation_actor", &permissions, false)
            .await
            .expect("actor role seeds");
        grant_role_to_service_account(&mut conn, actor, role_id)
            .await
            .expect("actor role grants");
        conn.commit().await.expect("seed commits");
        actor
    }

    /// Mint the actor's own direct token. Its permissions claim is empty on
    /// purpose: the exchange must read the actor's current grants instead.
    fn actor_token(actor: Uuid, tenant: DataTenantId) -> String {
        mint(
            service_ref(actor, tenant, ACTOR_CARD),
            PermissionSet::new(),
            Some(ACTOR_CREDENTIAL),
            None,
        )
    }

    /// Exchange `subject` and `actor` tokens for a Bifrost token under `policy`.
    async fn exchange(
        fixture: &PgFixture,
        policy: Arc<dyn PolicyHook>,
        subject: String,
        actor: String,
    ) -> Result<super::ExchangedToken, DelegateError> {
        let conn = fixture.tenant_conn().await.expect("tenant conn opens");
        delegate_service(policy)
            .execute(
                conn,
                SecretString::from(subject),
                SecretString::from(actor),
                TokenAudience::Bifrost,
                &Uuid::now_v7().to_string(),
            )
            .await
    }

    /// Committed Bifrost exchange decisions `(outcome, principal, credential,
    /// detail)`, oldest first.
    async fn exchange_decisions(
        fixture: &PgFixture,
    ) -> Vec<(String, Uuid, Option<Uuid>, AuditDetail)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let rows: Vec<(String, Uuid, Option<Uuid>, String)> = sqlx::query_as(
            "SELECT outcome, principal_id, credential_id, detail FROM vala.audit_staging
              WHERE operation = $1 AND resource = 'bifrost' ORDER BY seq",
        )
        .bind(TOKEN_EXCHANGE_OPERATION)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("decision query runs");
        rows.into_iter()
            .map(|(outcome, principal, credential, detail)| {
                let detail = serde_json::from_str(&detail).expect("detail decodes");
                (outcome, principal, credential, detail)
            })
            .collect()
    }

    /// Outcomes of the committed Bifrost exchange decisions, oldest first.
    async fn exchange_outcomes(fixture: &PgFixture) -> Vec<String> {
        exchange_decisions(fixture)
            .await
            .into_iter()
            .map(|(outcome, ..)| outcome)
            .collect()
    }

    /// The invoke policy that allows every exchange.
    fn allow() -> Arc<dyn PolicyHook> {
        Arc::new(RecordingPolicyHook::default())
    }

    /// A policy-denied actor is refused and the denial commits under the
    /// subject, naming the actor as the current actor and its credential.
    #[tokio::test]
    async fn a_policy_denied_exchange_commits_one_denied_decision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let actor = seed_actor(&fixture, serde_json::json!([])).await;
        let subject = Uuid::new_v4();
        let deny = Arc::new(DenyAllPolicyHook {
            reason: "a_may_not_invoke_b".to_owned(),
        });

        let result = exchange(
            &fixture,
            deny,
            mint(
                service_ref(subject, tenant, "test-service"),
                PermissionSet::new(),
                None,
                None,
            ),
            actor_token(actor, tenant),
        )
        .await;

        assert!(
            matches!(&result, Err(DelegateError::PolicyDenied(reason)) if reason == "a_may_not_invoke_b"),
            "{result:?}"
        );
        let decisions = exchange_decisions(&fixture).await;
        let [(outcome, principal, credential, detail)] = decisions.as_slice() else {
            panic!("exactly one decision commits, got {decisions:?}");
        };
        assert_eq!(outcome, "denied");
        assert_eq!(*principal, subject);
        assert_eq!(*credential, Some(ACTOR_CREDENTIAL));
        let AuditDetail::DelegationAttribution { delegation_chain } = detail else {
            panic!("a denial carries the actor chain, got {detail:?}");
        };
        assert_eq!(
            delegation_chain
                .iter()
                .map(|step| step.principal_id.as_uuid())
                .collect::<Vec<_>>(),
            [actor]
        );
    }

    /// An allowed exchange whose actor no longer exists keeps its allowance
    /// with no effect.
    #[tokio::test]
    async fn an_allowed_exchange_for_a_missing_actor_commits_one_allowed_decision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let result = exchange(
            &fixture,
            allow(),
            subject_token(tenant, PermissionSet::new()),
            actor_token(Uuid::new_v4(), tenant),
        )
        .await;

        assert!(matches!(result, Err(DelegateError::ActorNotFound)), "{result:?}");
        assert_eq!(exchange_outcomes(&fixture).await, ["allowed"]);
    }

    /// Unverifiable, cross-tenant, or malformed identity input is refused
    /// before the policy decides and records nothing.
    #[tokio::test]
    async fn invalid_or_malformed_identity_input_records_no_decision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let actor = seed_actor(&fixture, serde_json::json!([])).await;
        let subject = subject_token(tenant, PermissionSet::new());
        let actor_card = service_ref(actor, tenant, ACTOR_CARD);
        let delegated_actor = mint(
            actor_card.clone(),
            PermissionSet::new(),
            None,
            Some(Box::new(ActClaim {
                sub: Uuid::new_v4().to_string(),
                principal: service_ref(Uuid::new_v4(), tenant, "earlier"),
                act: None,
            })),
        );
        let card_free_actor = mint(
            TokenPrincipalRef {
                card_ref: None,
                ..actor_card
            },
            PermissionSet::new(),
            None,
            None,
        );
        let foreign_actor = actor_token(actor, DataTenantId::new_v7());
        let policy = Arc::new(RecordingPolicyHook::default());

        let cases: [(&str, String, String); 6] = [
            ("invalid subject", "not-a-token".to_owned(), actor_token(actor, tenant)),
            ("invalid actor", subject.clone(), "not-a-token".to_owned()),
            ("cross-tenant actor", subject.clone(), foreign_actor),
            ("delegated actor", subject.clone(), delegated_actor),
            ("card-free actor", subject.clone(), card_free_actor),
            ("self exchange", subject.clone(), subject),
        ];
        for (label, subject, actor) in cases {
            let result = exchange(&fixture, policy.clone(), subject, actor).await;
            let refused_early = match label {
                "invalid subject" => matches!(result, Err(DelegateError::InvalidSubjectToken(_))),
                "invalid actor" => matches!(result, Err(DelegateError::InvalidActorToken(_))),
                "cross-tenant actor" => matches!(
                    result,
                    Err(DelegateError::InvalidActorToken(AuthError::InvalidToken))
                ),
                _ => matches!(result, Err(DelegateError::MalformedIdentity(_))),
            };
            assert!(refused_early, "{label} must be refused early, got {result:?}");
        }
        assert!(policy.calls().is_empty(), "no refusal reaches the policy");
        assert!(exchange_outcomes(&fixture).await.is_empty());
    }

    /// A successful exchange names the subject as principal and the actor as
    /// the outer `act` with earlier actors nested in RFC order, carries only
    /// authority both parties hold at the narrower scope, is Bifrost-only,
    /// asks the policy the directed A-to-B question, commits one allowed
    /// decision naming both parties, and issues no refresh token.
    #[tokio::test]
    async fn an_exchange_names_subject_and_actor_and_carries_only_the_intersection() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let table = Uuid::from_u128(0x51);
        let actor = seed_actor(
            &fixture,
            serde_json::json!([
                { "resource": "cards", "action": "wildcard", "scope": "all" },
                { "resource": "audit", "action": "read", "scope": "all" },
                { "resource": "bifrost_record", "action": "write", "scope": "all" },
                { "resource": "bifrost_query", "action": "read",
                  "scope": { "bifrost": { "schema": { "catalog": "vala", "schema": "logs" } } } }
            ]),
        )
        .await;
        let table_grant = Permission {
            resource: Resource::BifrostQuery,
            action: Action::Read,
            scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "vala".to_owned(),
                schema: "logs".to_owned(),
                table_uid: table,
            })),
        };
        let subject_id = Uuid::new_v4();
        let earlier = service_ref(Uuid::new_v4(), tenant, "earlier-actor");
        let subject = mint(
            service_ref(subject_id, tenant, "test-service"),
            [
                Permission::card_read(),
                Permission::policy_lock(),
                table_grant.clone(),
            ]
            .into_iter()
            .collect(),
            None,
            Some(Box::new(ActClaim {
                sub: earlier.id.to_string(),
                principal: earlier.clone(),
                act: None,
            })),
        );
        let policy = Arc::new(RecordingPolicyHook::default());

        let exchanged = exchange(&fixture, policy.clone(), subject, actor_token(actor, tenant))
            .await
            .expect("exchange succeeds");

        let verifier = delegate_service(allow()).verifier;
        assert!(
            verifier.verify(&exchanged.access_token, &tenant).is_err(),
            "a Bifrost token is refused on a general Wyrd surface"
        );
        let delegated = verifier
            .verify_on(&exchanged.access_token, &tenant, TokenAudience::Bifrost)
            .expect("delegated token verifies on Bifrost");
        assert_eq!(delegated.principal.id.as_uuid(), subject_id);
        assert_eq!(
            delegated
                .delegation_chain
                .iter()
                .map(|step| step.principal.id.as_uuid())
                .collect::<Vec<_>>(),
            [earlier.id.as_uuid(), actor],
            "earlier actors come first and the new actor is current"
        );
        assert_eq!(
            delegated.principal.effective_permissions,
            [Permission::card_read(), table_grant].into_iter().collect(),
            "wildcard narrows to the subject's read, schema narrows to the subject's table, \
             and one-sided grants such as the actor's write are dropped"
        );
        assert_eq!(delegated.principal.credential_id, None);
        assert!(exchanged.refresh_token.is_none());

        let asked = policy.last().expect("the policy decided");
        assert_eq!(asked.subject.id.as_uuid(), subject_id);
        assert_eq!(asked.actor.id.as_uuid(), actor);
        assert_eq!(asked.request.target, named_service_card_ref(ACTOR_CARD));
        assert_eq!(asked.request.action, super::DELEGATION_POLICY_ACTION);
        assert_eq!(asked.request.context["audience"], "bifrost");

        let decisions = exchange_decisions(&fixture).await;
        let [(outcome, principal, credential, detail)] = decisions.as_slice() else {
            panic!("exactly one decision commits, got {decisions:?}");
        };
        assert_eq!(outcome, "allowed");
        assert_eq!(*principal, subject_id);
        assert_eq!(*credential, Some(ACTOR_CREDENTIAL));
        let AuditDetail::TokenExchange {
            subject_principal_id,
            actor_principal_id,
            ..
        } = detail
        else {
            panic!("an issued exchange carries the token-exchange detail, got {detail:?}");
        };
        assert_eq!(subject_principal_id.as_uuid(), subject_id);
        assert_eq!(actor_principal_id.as_uuid(), actor);
    }

    /// An audit store that refuses the append fails the exchange closed: no
    /// token and no committed decision.
    #[tokio::test]
    async fn a_refused_exchange_audit_issues_no_token() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let actor = seed_actor(&fixture, serde_json::json!([])).await;
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("REVOKE INSERT ON vala.audit_staging FROM wyrd_app")
            .execute(&admin)
            .await
            .expect("append privilege revoked");

        let result = exchange(
            &fixture,
            allow(),
            subject_token(tenant, PermissionSet::new()),
            actor_token(actor, tenant),
        )
        .await;

        sqlx::query("GRANT INSERT ON vala.audit_staging TO wyrd_app")
            .execute(&admin)
            .await
            .expect("append privilege restored");
        assert!(
            matches!(
                result,
                Err(DelegateError::Issuance(IssuanceError::Wyrd(
                    WyrdError::AuditUnavailable { .. }
                )))
            ),
            "an unrecordable exchange is refused as audit-unavailable, got: {result:?}"
        );
        assert!(exchange_outcomes(&fixture).await.is_empty());
    }
}
