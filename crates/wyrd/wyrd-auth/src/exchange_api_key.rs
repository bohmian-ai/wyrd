//! API-key exchange and RFC 8693 delegation entry paths.
//!
//! Each verifies only its own grant-specific evidence — the presented API key,
//! or the subject and actor access tokens — then mints through the shared [`TenantTokenIssuer`].

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wyrd_auth_verify::{
    ActClaim, AuthError, TokenAudience, TokenPrincipalRef, TokenVerifier, VerifiedToken,
};
use wyrd_runtime::{DelegationStep, PrincipalRef, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::{
    ApiKeyLookupRow, ApiKeyStatus, api_key_by_prefix, api_key_status_by_prefix,
    touch_api_key_last_used,
};
use wyrd_sql::{SqlError, TenantConn};

use crate::audit::{TOKEN_EXCHANGE_OPERATION, auth_event};
use crate::error::auth_error_to_wyrd;
use crate::issuance::{ExchangedToken, IssuanceError, TenantGrant, TenantTokenIssuer};
use crate::issue_api_key::WyrdApiKey;
use wyrd_auth_issue::secret_matches;

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
    /// Key exists but its secret does not match the stored verifier.
    #[error("api key hash mismatch")]
    HashMismatch,
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
    /// a delegated actor token, an actor that is not a rooted Service, or a
    /// party exchanging with itself.
    #[error("token exchange identity input is malformed: {0}")]
    MalformedIdentity(&'static str),
    /// The actor's principal is missing or inactive at issuance.
    #[error("actor principal not found")]
    ActorNotFound,
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
    /// Whether the failure was a store read, which leaves the transaction
    /// unable to record a decision.
    fn is_store_failure(&self) -> bool {
        matches!(
            self,
            Self::Database(_) | Self::Issuance(IssuanceError::Database(_))
        )
    }
}

impl From<IssuanceError> for DelegateError {
    /// Lift a shared-issuance failure into the exchange error, reporting an
    /// inactive principal as [`DelegateError::ActorNotFound`] because the
    /// exchange only issues for the actor.
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
    /// for its prefix, and the SHA-256 verifier — records the key's use, then
    /// mints through the shared issuance workflow, which refuses an inactive
    /// tenant or principal, resolves current grants, and validates the
    /// explicitly selected `workload_uid` under the account's restriction.
    ///
    /// # Errors
    /// All authentication failures map to `WyrdError::ApiKeyInvalid` at the HTTP
    /// boundary. Internal variants carry distinct failure paths for diagnostics.
    #[tracing::instrument(level = "debug", skip(self, conn, api_key), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        api_key: SecretString,
        workload_uid: Option<CardUid>,
        request_id: &str,
    ) -> Result<ExchangedToken, ExchangeError> {
        let row = verify_api_key(conn, &api_key).await?;
        touch_api_key_last_used(conn, row.api_key_id).await?;
        Ok(self
            .issuer
            .issue(
                conn,
                row.principal_id,
                TenantGrant::ApiKey {
                    credential_id: row.api_key_id,
                    workload_uid,
                },
                request_id,
            )
            .await?)
    }

    /// Authenticate one request by a Wyrd API key, without minting a token.
    ///
    /// The same key verification and use record as [`Self::execute`], then
    /// [`TenantTokenIssuer::verify`]: the request runs as the principal the
    /// exchanged token would name, with the authority it would carry. For
    /// stock exporters that send a static key header on every request.
    ///
    /// # Errors
    /// As [`Self::execute`].
    #[tracing::instrument(level = "debug", skip(self, conn, api_key), err)]
    pub async fn authenticate(
        &self,
        conn: &mut TenantConn<'_>,
        api_key: &SecretString,
        workload_uid: Option<CardUid>,
    ) -> Result<VerifiedToken, ExchangeError> {
        let row = verify_api_key(conn, api_key).await?;
        touch_api_key_last_used(conn, row.api_key_id).await?;
        Ok(self
            .issuer
            .verify(
                conn,
                row.principal_id,
                TenantGrant::ApiKey {
                    credential_id: row.api_key_id,
                    workload_uid,
                },
            )
            .await?)
    }
}

/// Verify a presented tenant API key and return its row.
///
/// Shared by the API-key exchange and the tenant connection recovery-key
/// check, so both refuse identically: the key must parse, name this
/// connection's tenant, match an unexpired unrevoked row by prefix, and match
/// that row's SHA-256 verifier in constant time.
///
/// # Errors
/// Returns [`ExchangeError::NotFound`] for a malformed key or unknown prefix,
/// [`ExchangeError::CrossTenant`] for another tenant's key,
/// [`ExchangeError::HashMismatch`] when the secret does not verify,
/// and [`ExchangeError::Database`] when the lookup fails.
pub(crate) async fn verify_api_key(
    conn: &mut TenantConn<'_>,
    api_key: &SecretString,
) -> Result<ApiKeyLookupRow, ExchangeError> {
    let parsed = WyrdApiKey::parse(api_key.expose_secret()).map_err(|_| ExchangeError::NotFound)?;
    if parsed.tenant_id != conn.data_tenant_id() {
        return Err(ExchangeError::CrossTenant);
    }
    let row = api_key_by_prefix(conn, &parsed.prefix)
        .await?
        .ok_or(ExchangeError::NotFound)?;
    if !secret_matches(api_key.expose_secret(), &row.key_hash) {
        return Err(ExchangeError::HashMismatch);
    }
    Ok(row)
}

impl DelegateToken {
    /// Exchange a subject token and an actor token for a token that lets the
    /// actor act on the subject's behalf at `audience`, committing the
    /// decision on `conn`.
    ///
    /// Both tokens are verified locally for the connection's tenant, so a
    /// cross-tenant pair fails verification; unverifiable or malformed
    /// identity input is refused before any decision and records nothing.
    /// Exactly one canonical audit event is then staged on the process audit stage:
    /// an exchange that mints stages the issuer's token-exchange event; an
    /// issuance refusal stages an allowed event with no effect and rolls its
    /// transaction back.
    /// The issued token names the subject as principal, the actor as its
    /// outermost `act` with the subject token's earlier actors nested inside,
    /// and carries the actor's current permissions narrowed to the subject's.
    /// A store failure commits nothing and serves no token; audit never
    /// refuses or delays the exchange.
    ///
    /// # Errors
    /// Returns [`DelegateError::InvalidSubjectToken`] or
    /// [`DelegateError::InvalidActorToken`] for an unverifiable token,
    /// [`DelegateError::MalformedIdentity`] for input that cannot name one
    /// actor acting for one subject, [`DelegateError::ActorNotFound`] for a missing or
    /// inactive actor, [`DelegateError::Issuance`] when issuance fails,
    /// [`DelegateError::Database`] when a read fails, and
    /// [`DelegateError::Commit`] when the minted grant cannot be committed.
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
        let (chain, actor_workload_uid) = delegation_chain(&subject, &actor)?;
        let grant = TenantGrant::Delegation {
            subject: Box::new(TokenPrincipalRef::from(&subject.principal)),
            subject_roles: subject.principal.roles.clone(),
            subject_permissions: subject.principal.effective_permissions.clone(),
            subject_card_access: subject.principal.card_access.clone(),
            subject_workload_uid: subject.principal.workload_uid.clone(),
            actor_workload_uid,
            prior_act: act_from_chain(&subject.delegation_chain, tenant)?,
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
                self.record_refusal(tenant, &subject, &chain, &actor, audience, request_id);
                Err(error)
            }
        }
    }

    /// Stage the exchange decision for an exchange whose issuance refused.
    ///
    /// A minted token's decision is the issuer's token-exchange row, so this is
    /// only for an issuance refusal. Like every delegated request, the row is
    /// recorded under the subject with the full actor chain, targets the
    /// requested audience, and attaches the credential that authenticated the
    /// actor. It is staged on the process audit stage without waiting; the
    /// refusal's own transaction is rolled back.
    fn record_refusal(
        &self,
        tenant: DataTenantId,
        subject: &VerifiedToken,
        chain: &[DelegationStep],
        actor: &VerifiedToken,
        audience: TokenAudience,
        request_id: &str,
    ) {
        let subject = &subject.principal;
        let mut event = auth_event(
            request_id,
            TOKEN_EXCHANGE_OPERATION,
            wyrd_runtime::delegated_attribution(subject, chain),
            subject.kind.tag(),
            None,
            AuditOutcome::Allowed,
            AuditDetail::DelegationAttribution {
                delegation_chain: wyrd_runtime::audit_delegation_chain(chain),
            },
        )
        .with_credential_id(actor.principal.credential_id);
        audience.as_str().clone_into(&mut event.resource);
        self.issuer.audit().stage(tenant, event);
    }
}

/// Validate the exchange identity and build its actor chain, earliest first.
///
/// The actor must present a direct Service token that selected a root, and
/// must not be the subject itself; the subject must not be the internal
/// SYSTEM writer, which takes part in no delegation or impersonation flow and
/// can never hold a delegated tenant token. The subject's earlier actors
/// precede the new actor in the chain. Returns the chain and the actor's
/// root, which issuance revalidates and never replaces.
///
/// # Errors
/// Returns [`DelegateError::MalformedIdentity`] for a SYSTEM subject, a
/// delegated actor token, a rootless or non-Service actor, or a
/// self-exchange.
fn delegation_chain(
    subject: &VerifiedToken,
    actor: &VerifiedToken,
) -> Result<(Vec<DelegationStep>, CardUid), DelegateError> {
    if matches!(subject.principal.kind, wyrd_runtime::PrincipalKind::System) {
        return Err(DelegateError::MalformedIdentity("subject_is_system"));
    }
    if !actor.delegation_chain.is_empty() {
        return Err(DelegateError::MalformedIdentity("actor_token_is_delegated"));
    }
    if actor.principal.id == subject.principal.id {
        return Err(DelegateError::MalformedIdentity("actor_is_subject"));
    }
    let Some(actor_root) = actor
        .principal
        .workload_uid
        .clone()
        .filter(|_| actor.principal.kind == wyrd_runtime::PrincipalKind::Service)
    else {
        return Err(DelegateError::MalformedIdentity("actor_has_no_root"));
    };
    let chain = subject
        .delegation_chain
        .iter()
        .cloned()
        .chain(std::iter::once(DelegationStep {
            principal: PrincipalRef::from_principal(&actor.principal),
        }))
        .collect();
    Ok((chain, actor_root))
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
        _ => None,
    }
}

/// Rebuild an RFC 8693 `act` chain (current actor outermost) from a verified
/// earliest-first actor chain.
///
/// # Errors
/// Returns [`DelegateError::MalformedIdentity`] for a step without a root,
/// which no verified actor layer lacks.
fn act_from_chain(
    chain: &[wyrd_runtime::DelegationStep],
    tenant_id: wyrd_spec::DataTenantId,
) -> Result<Option<Box<ActClaim>>, DelegateError> {
    chain.iter().try_fold(None, |act, step| {
        let workload_uid = step
            .principal
            .workload_uid
            .clone()
            .ok_or(DelegateError::MalformedIdentity("actor_has_no_root"))?;
        Ok(Some(Box::new(ActClaim {
            sub: step.principal.id.to_string(),
            principal: TokenPrincipalRef::from((&step.principal, tenant_id)),
            workload_uid,
            cid: step.principal.credential_id.map(|id| id.to_string()),
            act,
        })))
    })
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
    /// Map an exchange failure to its public catalog error at the HTTP, MCP,
    /// and SDK boundary so each refusal carries a stable code.
    fn from(error: DelegateError) -> Self {
        match error {
            DelegateError::InvalidSubjectToken(error) | DelegateError::InvalidActorToken(error) => {
                auth_error_to_wyrd(error)
            }
            DelegateError::MalformedIdentity(reason) => WyrdError::Validation {
                message: "token exchange identity input is malformed".to_owned(),
                details: json!({ "reason": reason }),
            },
            DelegateError::ActorNotFound => WyrdError::PrincipalNotFound {
                message: "actor principal not found in tenant".to_owned(),
                details: json!({}),
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

/// Postgres-backed API-key and token-exchange behavior, and the shared
/// fixtures that seed users, service accounts, and live API keys.
#[cfg(test)]
pub(crate) mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::audit::{CARD_SCOPE_MINT_OPERATION, TOKEN_EXCHANGE_OPERATION};

    use chrono::{Duration, Utc};
    use secrecy::{ExposeSecret, SecretString};
    use serde_json::Value as JsonValue;
    use uuid::Uuid;
    use wyrd_auth_issue::{AccessGrant, IssuingKey};
    use wyrd_auth_verify::{
        ActClaim, AuthError, Kid, TokenAudience, TokenPrincipalRef, TokenVerifier,
        WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_dev_fixtures::cards::{seed_backing_card, seed_card_with_spec};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{
        Action, BifrostPermissionScope, BifrostTableScope, Permission, PermissionScope,
        PermissionSet, PrincipalId, Resource,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{
        Attribution, CardAccess, FamilyRestriction, PrincipalKindTag, WorkloadFamily, WorkloadKind,
    };
    use wyrd_spec::envelope::{CardKind, Spec};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, CardUid};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
    use wyrd_spec::vala::audit_detail::AuditDelegationStep;
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::insert_service_account;
    use wyrd_sql::queries::cards::get_card_by_ref;

    use crate::audit::test_audit::RecordedAudit;
    use wyrd_sql::queries::auth::{ApiKeyStatus, grant_role_to_service_account, insert_role};

    use super::{DelegateError, DelegateToken, ExchangeApiKey, ExchangeError};
    use crate::issuance::{TenantTokenIssuer, TokenExchangeSettings};
    use crate::issue_api_key::WyrdApiKey;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    /// The default static Service Card reference, named `test-service`.
    pub(crate) fn test_service_card_ref() -> CardRef {
        named_service_card_ref("test-service")
    }

    /// A static Service Card reference named `name`.
    ///
    /// # Panics
    /// Panics when `name` is not a valid Card name.
    pub(crate) fn named_service_card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
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

    /// The issuer under test, staging its audit on `audit`.
    ///
    /// The test owns `audit` and drains it before its fixture drops, so no audit
    /// commit is still logging in when the fixture drops its database.
    fn test_issuer(audit: &Arc<RecordedAudit>) -> TenantTokenIssuer {
        TenantTokenIssuer::new(
            test_issuing_key(),
            TokenExchangeSettings::default(),
            Arc::clone(audit) as _,
        )
    }

    /// The API-key exchange under test, staging its audit on `audit`.
    fn exchange_service(audit: &Arc<RecordedAudit>) -> ExchangeApiKey {
        ExchangeApiKey {
            issuer: test_issuer(audit),
        }
    }

    /// Build an exchange service whose verifier trusts the test signing key
    /// and whose issuer stages its audit on `audit`.
    ///
    /// # Panics
    /// Panics when the static test public key or key id fails to load.
    fn delegate_service(audit: &Arc<RecordedAudit>) -> DelegateToken {
        let public_key = public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads");
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(Kid::new("k1").expect("kid is valid"), Arc::new(public_key));
        DelegateToken {
            issuer: test_issuer(audit),
            verifier: Arc::new(TokenVerifier::new(
                decoding_keys,
                "wyrd",
                WyrdAuthVerifySettings::default(),
            )),
        }
    }

    /// Mint a `wyrd`-audience token for `principal` carrying `permissions`
    /// and `card_access`, attributed to `credential_id`, with an optional
    /// selected `root` and `act`.
    ///
    /// # Panics
    /// Panics when the test issuing key cannot sign the token.
    fn mint(
        principal: TokenPrincipalRef,
        permissions: PermissionSet,
        credential_id: Option<Uuid>,
        card_access: CardAccess,
        root: Option<CardUid>,
        act: Option<Box<ActClaim>>,
    ) -> String {
        test_issuing_key()
            .issue_access_token(
                AccessGrant {
                    principal,
                    roles: vec![],
                    permissions,
                    card_access,
                    workload_uid: root,
                    credential_id,
                    act,
                    audience: TokenAudience::Wyrd,
                },
                Duration::minutes(5),
            )
            .expect("test token issues")
    }

    /// A Service account principal reference.
    fn service_ref(id: Uuid, tenant: DataTenantId) -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: PrincipalId::new(id),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant,
        }
    }

    /// A fresh UUIDv7 Card UID that names no registered Card.
    ///
    /// # Panics
    ///
    /// Panics when a fresh UUIDv7 is not accepted as a Card UID.
    fn unregistered_uid() -> CardUid {
        CardUid::from_uuid(Uuid::now_v7()).expect("v7 uuid is a Card UID")
    }

    /// The Service family named `name`.
    ///
    /// # Panics
    /// Panics when `name` is not a valid Card name.
    pub(crate) fn service_family(name: &str) -> WorkloadFamily {
        WorkloadFamily {
            kind: WorkloadKind::Service,
            name: CardName::new(name).expect("static name is valid"),
        }
    }

    /// Mint a rootless unrestricted Service subject token carrying
    /// `permissions`.
    fn subject_token(tenant: DataTenantId, permissions: PermissionSet) -> String {
        mint(
            service_ref(Uuid::new_v4(), tenant),
            permissions,
            None,
            CardAccess::AnyRegistered,
            None,
            None,
        )
    }

    /// Seed one active password user in `tenant_id` and return its id, as the
    /// creator other seeded rows name.
    ///
    /// # Panics
    /// Panics when the insert fails.
    pub(crate) async fn insert_test_user(
        conn: &mut TenantConn<'_>,
        tenant_id: DataTenantId,
    ) -> Uuid {
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

    /// Seed one active stable Service account created by `creator`,
    /// restricted to `family` when given, and return its id.
    ///
    /// # Panics
    /// Panics when the account insert fails, including a second live account
    /// for one family.
    pub(crate) async fn insert_test_service_account(
        conn: &mut TenantConn<'_>,
        creator: Uuid,
        family: Option<&WorkloadFamily>,
    ) -> Uuid {
        let sa_id = Uuid::now_v7();
        insert_service_account(
            conn,
            sa_id,
            "service",
            family,
            &format!("svc-{sa_id}"),
            None,
            Attribution::direct(PrincipalId::new(creator)),
        )
        .await
        .expect("service account inserts");
        sa_id
    }

    /// Seed `card_ref`'s backing Card and return its registry UID, the root a
    /// grant can select.
    ///
    /// # Panics
    /// Panics when the Card insert or read-back fails.
    pub(crate) async fn seed_root(
        conn: &mut TenantConn<'_>,
        card_ref: &CardRef,
        creator: Uuid,
    ) -> CardUid {
        seed_backing_card(conn, card_ref, creator).await;
        root_uid(conn, card_ref).await
    }

    /// The registry UID of the already seeded `card_ref`.
    ///
    /// # Panics
    /// Panics when the Card row cannot be read.
    pub(crate) async fn root_uid(conn: &mut TenantConn<'_>, card_ref: &CardRef) -> CardUid {
        get_card_by_ref(
            conn,
            card_ref.kind.clone(),
            &card_ref.name,
            &card_ref.version,
        )
        .await
        .expect("seeded card row loads")
        .card_uid
    }

    async fn insert_lifecycle_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        sa_id: Uuid,
        creator: Uuid,
        prefix: &str,
        expires_at: chrono::DateTime<Utc>,
        revoked: bool,
    ) {
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, sub_principal_id, act_principal_id, expires_at, revoked_at)
             VALUES ($1, $2, $3, $4, 'placeholder-hash', $5, $5, $6, CASE WHEN $7 THEN now() ELSE NULL END)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(prefix)
        .bind(creator)
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

    /// Rebuilding an `act` chain keeps RFC order, each actor's root, and its
    /// credential provenance, and refuses a rootless step instead of dropping
    /// an actor.
    ///
    /// # Panics
    /// Panics when the rebuilt chain differs or a rootless step is accepted.
    #[test]
    fn act_from_chain_roundtrip() {
        use wyrd_runtime::{DelegationStep, PrincipalKind, PrincipalRef as RuntimePrincipalRef};

        let tenant_id = DataTenantId::new_v7();
        let step = |root: Option<CardUid>, credential_id| DelegationStep {
            principal: RuntimePrincipalRef {
                id: PrincipalId::new(Uuid::now_v7()),
                kind: PrincipalKind::Service,
                workload_uid: root,
                credential_id,
            },
        };
        let (root_a, root_b) = (unregistered_uid(), unregistered_uid());
        let a = step(Some(root_a.clone()), None);
        let b = step(Some(root_b.clone()), Some(ACTOR_CREDENTIAL));

        let result = super::act_from_chain(&[a.clone(), b.clone()], tenant_id)
            .expect("verified steps build")
            .expect("non-empty chain builds act");

        assert_eq!(result.sub, b.principal.id.to_string());
        assert_eq!(result.workload_uid, root_b);
        assert_eq!(result.cid, Some(ACTOR_CREDENTIAL.to_string()));
        let inner = result.act.as_ref().expect("inner act present");
        assert_eq!(inner.sub, a.principal.id.to_string());
        assert_eq!(inner.workload_uid, root_a);
        assert_eq!(inner.cid, None);
        assert!(inner.act.is_none());

        assert!(matches!(
            super::act_from_chain(&[a, step(None, None)], tenant_id),
            Err(DelegateError::MalformedIdentity("actor_has_no_root"))
        ));
    }

    /// Seed a live API key for `principal_id` in `conn`'s open transaction and
    /// return its row id and presentable secret.
    ///
    /// Generates a tenant-bound key, stores its SHA-256 verifier and prefix in
    /// `wyrd.auth_api_keys` with a one-day expiry, and writes nothing else; the
    /// caller commits. The row id is what a grant record and the browser
    /// session tests' key-use reads must name, so callers need it rather than a
    /// fresh UUID.
    ///
    /// # Panics
    /// Panics when the generated secret cannot be hashed or the insert fails.
    pub(crate) async fn insert_live_api_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        principal_id: Uuid,
        creator: Uuid,
    ) -> (Uuid, SecretString) {
        let key = WyrdApiKey::generate(tenant);
        let hash = wyrd_auth_issue::hash_secret(key.secret.expose_secret());
        let api_key_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, sub_principal_id, act_principal_id, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $6, now() + interval '1 day')",
        )
        .bind(api_key_id)
        .bind(tenant.as_uuid())
        .bind(principal_id)
        .bind(&key.prefix)
        .bind(&hash)
        .bind(creator)
        .execute(&mut **conn.transaction())
        .await
        .expect("live api key inserts");
        (api_key_id, key.secret)
    }

    /// Count the direct grant records staged on `audit` for a principal, and
    /// read the credential the first record names.
    ///
    /// A direct grant evaluates no dynamic permission, so only records whose
    /// permission is the operation itself are counted; a direct record
    /// carrying any other permission leaves the count at zero.
    fn staged_exchange(
        audit: &RecordedAudit,
        tenant: DataTenantId,
        principal_id: Uuid,
    ) -> (usize, Option<Uuid>) {
        let records: Vec<_> = audit
            .operation(TOKEN_EXCHANGE_OPERATION)
            .into_iter()
            .filter(|(staged, event)| {
                *staged == tenant
                    && event.permission == TOKEN_EXCHANGE_OPERATION
                    && event.sub_principal_id.as_uuid() == principal_id
            })
            .collect();
        let credential = records.first().and_then(|(_, event)| event.credential_id);
        (records.len(), credential)
    }

    /// A rootless tenant grant is recorded and names the key that bought it.
    ///
    /// A tenant administrator selecting no root takes the rootless branch and
    /// writes no scope mint. Without a grant record on that branch the most
    /// privileged tenant principal could exchange its credential repeatedly and
    /// leave nothing behind to attribute afterwards.
    ///
    /// # Panics
    /// Panics when the exchange fails, when the grant is not recorded exactly
    /// once, or when the record does not name the presented key.
    #[tokio::test]
    async fn a_rootless_exchange_commits_one_attributed_grant_record() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let admin_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, name, status, sub_principal_id, act_principal_id)
             VALUES ($1, $2, 'tenant_admin', $3, 'active', $4, $4)",
        )
        .bind(admin_id)
        .bind(tenant.as_uuid())
        .bind(format!("admin-{admin_id}"))
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("card-free administrator inserts");
        let (api_key_id, secret) = insert_live_api_key(&mut conn, tenant, admin_id, user_id).await;

        let service = exchange_service(&audit);
        service
            .execute(&mut conn, secret, None, "req-cardfree-grant")
            .await
            .expect("a card-free administrator exchanges its credential");

        let (count, credential) = staged_exchange(&audit, tenant, admin_id);
        assert_eq!(count, 1, "exactly one grant record is staged");
        assert_eq!(
            credential,
            Some(api_key_id),
            "the grant names the api key that was spent"
        );
    }

    /// A rooted tenant grant records the exchange and the scope mint.
    ///
    /// They answer different questions — which credential bought a token, and
    /// what emit authority the selected root conferred — so one cannot stand
    /// in for the other.
    ///
    /// # Panics
    /// Panics when the exchange fails, when the grant is not recorded exactly
    /// once naming the key, or when the scope mint is missing.
    #[tokio::test]
    async fn a_rooted_exchange_records_the_grant_and_the_scope_mint() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let card_ref = test_service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let root = seed_root(&mut conn, &card_ref, user_id).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, None).await;
        let (api_key_id, secret) = insert_live_api_key(&mut conn, tenant, sa_id, user_id).await;

        let service = exchange_service(&audit);
        service
            .execute(&mut conn, secret, Some(root), "req-cardbound-grant")
            .await
            .expect("a rooted service exchanges its credential");

        let (count, credential) = staged_exchange(&audit, tenant, sa_id);
        assert_eq!(count, 1, "exactly one grant record is staged");
        assert_eq!(
            credential,
            Some(api_key_id),
            "the grant names the api key that was spent"
        );

        let mints = audit
            .operation(CARD_SCOPE_MINT_OPERATION)
            .iter()
            .filter(|(staged, event)| {
                *staged == tenant && event.sub_principal_id.as_uuid() == sa_id
            })
            .count();
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
    ///
    /// # Panics
    ///
    /// Panics when the fixture or key seed fails, the live key does not
    /// exchange, the response carries a refresh token, or a refresh row exists
    /// for the account.
    #[tokio::test]
    async fn api_key_exchange_issues_no_refresh_token_or_row() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, None).await;

        let key = WyrdApiKey::generate(tenant);
        let hash = wyrd_auth_issue::hash_secret(key.secret.expose_secret());
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, sub_principal_id, act_principal_id, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $6, now() + interval '1 day')",
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

        let exchanged = exchange_service(&audit)
            .execute(&mut conn, key.secret, None, "req-api-key-no-refresh")
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

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, None).await;

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
    ///
    /// # Panics
    ///
    /// Panics when the account suspension update or the key seed fails.
    async fn seed_suspended_account_key(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        creator: Uuid,
    ) -> SecretString {
        let sa_id = insert_test_service_account(conn, creator, None).await;
        sqlx::query("UPDATE wyrd.auth_service_accounts SET status = 'suspended' WHERE id = $1")
            .bind(sa_id)
            .execute(&mut **conn.transaction())
            .await
            .expect("service account suspends");
        insert_live_api_key(conn, tenant, sa_id, creator).await.1
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
    async fn seed_unadmitted_tenant_key(fixture: &PgFixture) -> (DataTenantId, SecretString) {
        let unadmitted_tenant = fixture
            .seed_additional_tenant("unadmitted")
            .await
            .expect("second tenant seeds");
        let mut unadmitted_conn = fixture
            .tenant_conn_for(unadmitted_tenant)
            .await
            .expect("unadmitted tenant conn opens");
        let unadmitted_user = insert_test_user(&mut unadmitted_conn, unadmitted_tenant).await;
        let unadmitted_sa =
            insert_test_service_account(&mut unadmitted_conn, unadmitted_user, None).await;
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
            .execute(&fixture.superuser_pool().expect("superuser pool opens"))
            .await
            .expect("second tenant suspends");
        (unadmitted_tenant, unadmitted)
    }

    /// Every refusal renders the one invalid-key error.
    ///
    /// A malformed key, another tenant's key, a tenant that no longer admits
    /// credentials, an unknown prefix, a disabled account, a revoked or
    /// expired key, and a live prefix with the wrong tail are
    /// indistinguishable to the caller, so no refusal is an oracle.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn every_invalid_api_key_is_refused_identically() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();

        let unknown = WyrdApiKey::generate(tenant);
        let revoked = WyrdApiKey::generate(tenant);
        let wrong_tail = WyrdApiKey::generate(tenant);
        let foreign = WyrdApiKey::generate(DataTenantId::new_v7());

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let live_sa = insert_test_service_account(&mut conn, user_id, None).await;

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

        let disabled = seed_suspended_account_key(&mut conn, tenant, user_id).await;

        let expected = rendered(&super::api_key_invalid());
        let service = exchange_service(&audit);
        let cases = [
            ("malformed", SecretString::from("not-a-wyrd-api-key")),
            ("cross_tenant", foreign.secret),
            ("unknown_prefix", unknown.secret),
            ("suspended_account", disabled),
            ("revoked", revoked.secret),
            ("wrong_tail", wrong_tail.secret),
        ];
        for (label, presented) in cases {
            let error = service
                .execute(&mut conn, presented, None, &format!("req-{label}"))
                .await
                .expect_err("an invalid api key is refused");
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
        let (unadmitted_tenant, unadmitted) = seed_unadmitted_tenant_key(&fixture).await;
        let mut unadmitted_conn = fixture
            .tenant_conn_for(unadmitted_tenant)
            .await
            .expect("unadmitted tenant conn reopens");
        let error = service
            .execute(&mut unadmitted_conn, unadmitted, None, "req-unadmitted")
            .await
            .expect_err("a tenant that does not admit credentials is refused");
        assert!(matches!(error, ExchangeError::TenantNotAdmitting));
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

    /// A key whose prefix embeds one tenant is refused as `CrossTenant` when
    /// presented on another tenant's connection, before any database lookup.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or the second tenant's connection fails, or the
    /// exchange returns anything other than `CrossTenant`.
    #[tokio::test]
    async fn cross_tenant_key_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
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
        let result = exchange_service(&audit)
            .execute(&mut conn, key.secret, None, "req-cross-tenant")
            .await;

        assert!(matches!(result, Err(ExchangeError::CrossTenant)));
    }

    /// A revoked key row is invisible to the prefix lookup, so the exchange
    /// refuses it as `NotFound`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or the revoked-key seed fails, or the exchange
    /// returns anything other than `NotFound`.
    #[tokio::test]
    async fn revoked_key_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let key = WyrdApiKey::generate(tenant);

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, None).await;

        // Insert the key row with revoked_at = now(). The prefix-lookup query
        // filters on `revoked_at IS NULL`, so this row is invisible and the
        // service returns NotFound.
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, sub_principal_id, act_principal_id, expires_at, revoked_at)
             VALUES ($1, $2, $3, $4, 'placeholder-hash', $5, $5, now() + interval '1 year', now())",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&key.prefix)
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("revoked api key inserts");

        let result = exchange_service(&audit)
            .execute(&mut conn, key.secret, None, "req-revoked")
            .await;

        assert!(matches!(result, Err(ExchangeError::NotFound)));
    }

    /// A live key row whose stored verifier is not the presented secret's
    /// digest is refused as `HashMismatch`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or the key seed fails, or the exchange returns
    /// anything other than `HashMismatch`.
    #[tokio::test]
    async fn hash_mismatch_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let key = WyrdApiKey::generate(tenant);

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, None).await;

        // Store a verifier that is not this key's SHA-256 digest.
        sqlx::query(
            "INSERT INTO wyrd.auth_api_keys
                 (id, data_tenant_id, principal_id, prefix, key_hash, sub_principal_id, act_principal_id, expires_at)
             VALUES ($1, $2, $3, $4, 'not-a-valid-phc-hash', $5, $5, now() + interval '1 year')",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&key.prefix)
        .bind(user_id)
        .execute(&mut **conn.transaction())
        .await
        .expect("api key with wrong hash inserts");

        let result = exchange_service(&audit)
            .execute(&mut conn, key.secret, None, "req-hash-mismatch")
            .await;

        assert!(matches!(result, Err(ExchangeError::HashMismatch)));
    }

    /// Card name of the seeded actor.
    const ACTOR_CARD: &str = "delegation-actor";
    /// Credential the minted actor token is attributed to.
    const ACTOR_CREDENTIAL: Uuid = Uuid::from_u128(0xAC7);

    /// Card name of the actor root's observation-target child.
    const SHARED_CARD: &str = "delegation-shared";

    /// A seeded delegation actor: its account and the Cards its root reaches.
    struct SeededActor {
        /// The actor's stable Service account id.
        id: Uuid,
        /// The actor's root, `ACTOR_CARD`, inside the actor's family.
        root: CardUid,
        /// The root's one observation-target child, `SHARED_CARD`.
        shared: CardUid,
    }

    /// Seed a Service actor restricted to the `ACTOR_CARD` family, holding
    /// `permissions` through one role, whose root reaches one shared child.
    ///
    /// # Panics
    /// Panics when the tenant connection, a Card, account, role seed, role
    /// grant, or commit fails.
    async fn seed_actor(fixture: &PgFixture, permissions: serde_json::Value) -> SeededActor {
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        let shared_ref = named_service_card_ref(SHARED_CARD);
        let shared = seed_root(&mut conn, &shared_ref, creator).await;
        let root_ref = named_service_card_ref(ACTOR_CARD);
        let root_spec = Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [{
                    "alias": "shared",
                    "ref": { "kind": "Service", "name": SHARED_CARD, "version": "1.0.0" },
                }],
            }),
        )
        .expect("actor root spec decodes");
        seed_card_with_spec(&mut conn, &root_ref, &root_spec, creator).await;
        let root = root_uid(&mut conn, &root_ref).await;
        let id = insert_test_service_account(&mut conn, creator, Some(&service_family(ACTOR_CARD)))
            .await;
        let role_id = Uuid::new_v4();
        insert_role(&mut conn, role_id, "delegation_actor", &permissions, false)
            .await
            .expect("actor role seeds");
        grant_role_to_service_account(&mut conn, id, role_id)
            .await
            .expect("actor role grants");
        conn.commit().await.expect("seed commits");
        SeededActor { id, root, shared }
    }

    /// Mint the actor's own direct token under `root`. Its permissions claim
    /// is empty on purpose: the exchange must read the actor's current grants
    /// instead.
    fn actor_token(actor: Uuid, tenant: DataTenantId, root: &CardUid) -> String {
        mint(
            service_ref(actor, tenant),
            PermissionSet::new(),
            Some(ACTOR_CREDENTIAL),
            CardAccess::Scoped {
                cards: vec![root.clone()],
            },
            Some(root.clone()),
            None,
        )
    }

    /// Exchange `subject` and `actor` tokens for a Bifrost token, staging
    /// every audit decision on `audit`.
    ///
    /// # Errors
    /// Returns the [`DelegateError`] the exchange refuses with, unchanged.
    ///
    /// # Panics
    /// Panics when the tenant connection cannot be opened.
    async fn exchange(
        fixture: &PgFixture,
        audit: &Arc<RecordedAudit>,
        subject: String,
        actor: String,
    ) -> Result<super::ExchangedToken, DelegateError> {
        let conn = fixture.tenant_conn().await.expect("tenant conn opens");
        delegate_service(audit)
            .execute(
                conn,
                SecretString::from(subject),
                SecretString::from(actor),
                TokenAudience::Bifrost,
                &Uuid::now_v7().to_string(),
            )
            .await
    }

    /// Bifrost exchange decisions staged on `audit` as `(outcome, permission,
    /// principal, credential, detail)`, oldest first.
    ///
    /// # Panics
    /// Panics when a staged decision carries no detail.
    fn exchange_decisions(
        audit: &RecordedAudit,
    ) -> Vec<(AuditOutcome, String, Uuid, Option<Uuid>, AuditDetail)> {
        audit
            .operation(TOKEN_EXCHANGE_OPERATION)
            .into_iter()
            .filter(|(_, event)| event.resource == "bifrost")
            .map(|(_, event)| {
                (
                    event.outcome,
                    event.permission,
                    event.sub_principal_id.as_uuid(),
                    event.credential_id,
                    event
                        .detail
                        .expect("an exchange decision carries its detail"),
                )
            })
            .collect()
    }

    /// `(outcome, permission)` of the Bifrost exchange decisions staged on
    /// `audit`, oldest first.
    ///
    /// # Panics
    /// Panics when `exchange_decisions` does.
    fn exchange_outcomes(audit: &RecordedAudit) -> Vec<(AuditOutcome, String)> {
        exchange_decisions(audit)
            .into_iter()
            .map(|(outcome, permission, ..)| (outcome, permission))
            .collect()
    }

    /// An allowed exchange whose actor no longer exists keeps its allowance
    /// with no effect.
    ///
    /// # Panics
    /// Panics when the exchange does not fail with
    /// [`DelegateError::ActorNotFound`] or anything other than one allowed
    /// token-exchange decision commits.
    #[tokio::test]
    async fn an_allowed_exchange_for_a_missing_actor_commits_one_allowed_decision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();

        let result = exchange(
            &fixture,
            &audit,
            subject_token(tenant, PermissionSet::new()),
            actor_token(Uuid::new_v4(), tenant, &unregistered_uid()),
        )
        .await;

        assert!(
            matches!(result, Err(DelegateError::ActorNotFound)),
            "{result:?}"
        );
        assert_eq!(
            exchange_outcomes(&audit),
            [(AuditOutcome::Allowed, TOKEN_EXCHANGE_OPERATION.to_owned())],
            "one allowance names the token-exchange operation"
        );
    }

    /// Unverifiable, cross-tenant, or malformed identity input is refused
    /// before issuance and records nothing.
    ///
    /// # Panics
    /// Panics when a case is not refused with its expected early error or any
    /// decision commits.
    #[tokio::test]
    async fn invalid_or_malformed_identity_input_records_no_decision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
        let tenant = fixture.data_tenant_id();
        let actor = seed_actor(&fixture, serde_json::json!([])).await;
        let subject = subject_token(tenant, PermissionSet::new());
        let earlier = Uuid::now_v7();
        let delegated_actor = mint(
            service_ref(actor.id, tenant),
            PermissionSet::new(),
            None,
            CardAccess::Scoped {
                cards: vec![actor.root.clone()],
            },
            Some(actor.root.clone()),
            Some(Box::new(ActClaim {
                sub: earlier.to_string(),
                principal: service_ref(earlier, tenant),
                workload_uid: unregistered_uid(),
                cid: None,
                act: None,
            })),
        );
        let rootless_actor = mint(
            service_ref(actor.id, tenant),
            PermissionSet::new(),
            None,
            CardAccess::AnyRegistered,
            None,
            None,
        );
        let foreign_actor = actor_token(actor.id, DataTenantId::new_v7(), &actor.root);

        let cases: [(&str, String, String); 6] = [
            (
                "invalid subject",
                "not-a-token".to_owned(),
                actor_token(actor.id, tenant, &actor.root),
            ),
            ("invalid actor", subject.clone(), "not-a-token".to_owned()),
            ("cross-tenant actor", subject.clone(), foreign_actor),
            ("delegated actor", subject.clone(), delegated_actor),
            ("rootless actor", subject.clone(), rootless_actor),
            ("self exchange", subject.clone(), subject),
        ];
        for (label, subject, actor) in cases {
            let result = exchange(&fixture, &audit, subject, actor).await;
            let refused_early = match label {
                "invalid subject" => matches!(result, Err(DelegateError::InvalidSubjectToken(_))),
                "invalid actor" => matches!(result, Err(DelegateError::InvalidActorToken(_))),
                "cross-tenant actor" => matches!(
                    result,
                    Err(DelegateError::InvalidActorToken(AuthError::InvalidToken))
                ),
                _ => matches!(result, Err(DelegateError::MalformedIdentity(_))),
            };
            assert!(
                refused_early,
                "{label} must be refused early, got {result:?}"
            );
        }
        assert!(exchange_outcomes(&audit).is_empty());
    }

    /// A successful exchange names the subject as principal and the actor as
    /// the outer `act` with earlier actors nested in RFC order, carries only
    /// authority both parties hold at the narrower scope, is Bifrost-only,
    /// commits one allowed decision naming both parties, and issues no refresh token.
    ///
    /// # Panics
    /// Panics when the exchange fails, the token verifies on the general Wyrd
    /// audience, the principal, chain, or narrowed permissions differ, a
    /// refresh token is issued, or the single
    /// committed decision does not name both parties.
    #[tokio::test]
    async fn an_exchange_names_subject_and_actor_and_carries_only_the_intersection() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let audit = RecordedAudit::new();
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
        let earlier = service_ref(Uuid::new_v4(), tenant);
        let subject = mint(
            service_ref(subject_id, tenant),
            [
                Permission::card_read(),
                Permission::policy_lock(),
                table_grant.clone(),
            ]
            .into_iter()
            .collect(),
            None,
            CardAccess::Scoped { cards: Vec::new() },
            None,
            Some(Box::new(ActClaim {
                sub: earlier.id.to_string(),
                principal: earlier.clone(),
                workload_uid: unregistered_uid(),
                cid: None,
                act: None,
            })),
        );
        let actor_bearer = actor_token(actor.id, tenant, &actor.root);
        let actor = actor.id;
        let exchanged = exchange(&fixture, &audit, subject, actor_bearer)
            .await
            .expect("exchange succeeds");

        let verifier = delegate_service(&audit).verifier;
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

        let decisions = exchange_decisions(&audit);
        let [(outcome, permission, principal, credential, detail)] = decisions.as_slice() else {
            panic!("exactly one decision commits, got {decisions:?}");
        };
        assert_eq!(*outcome, AuditOutcome::Allowed);
        assert_eq!(permission, TOKEN_EXCHANGE_OPERATION);
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

    /// A gateway exchange carries the exact authority both parties hold, or
    /// refuses with no token when no scope can state it.
    ///
    /// The actor's grants come from a real role merge, so an explicit
    /// deployment grant stored beside `all` must survive into the exchange.
    /// `all` meeting an explicit deployment grant — in either direction — is
    /// "that deployment, only while open", which no scope states: the exchange
    /// refuses with `WYRD_AUTH_403_UNREPRESENTABLE_DELEGATION`, signs nothing,
    /// and stages its one allowed exchange decision with no effect.
    ///
    /// # Panics
    /// Panics when a representable case fails or carries other permissions, or
    /// an unrepresentable case issues a token, maps to another error, or stages
    /// anything but one exchange decision.
    #[tokio::test]
    async fn a_gateway_exchange_carries_exact_authority_or_refuses() {
        use wyrd_spec::auth::GatewayAccess;
        use wyrd_spec::ids::ProviderDeploymentName;
        let explicit = Permission::gateway_invoke(GatewayAccess::Deployment {
            deployment: ProviderDeploymentName::new("restricted").expect("valid deployment"),
        });
        let all_json =
            serde_json::json!({ "resource": "gateway", "action": "invoke", "scope": "all" });
        let explicit_json = serde_json::to_value(&explicit).expect("grant serializes");
        let cases = [
            (
                serde_json::json!([all_json, explicit_json]),
                vec![explicit.clone()],
                Some(vec![explicit.clone()]),
            ),
            (
                serde_json::json!([all_json, explicit_json]),
                vec![Permission::gateway_invoke_any()],
                Some(vec![Permission::gateway_invoke_any()]),
            ),
            (serde_json::json!([all_json]), vec![explicit.clone()], None),
            (
                serde_json::json!([explicit_json]),
                vec![Permission::gateway_invoke_any()],
                None,
            ),
        ];
        for (actor_grants, subject_grants, expected) in cases {
            let fixture = PgFixture::start().await.expect("fixture starts");
            let audit = RecordedAudit::new();
            let tenant = fixture.data_tenant_id();
            let actor = seed_actor(&fixture, actor_grants.clone()).await;
            let result = exchange(
                &fixture,
                &audit,
                subject_token(tenant, subject_grants.into_iter().collect()),
                actor_token(actor.id, tenant, &actor.root),
            )
            .await;
            if let Some(expected) = expected {
                let exchanged = result.expect("representable exchange succeeds");
                let delegated = delegate_service(&audit)
                    .verifier
                    .verify_on(&exchanged.access_token, &tenant, TokenAudience::Bifrost)
                    .expect("delegated token verifies");
                assert_eq!(
                    delegated.principal.effective_permissions,
                    expected.into_iter().collect(),
                    "{actor_grants}"
                );
            } else {
                let error = WyrdError::from(result.expect_err("exchange refuses"));
                assert_eq!(error.code(), "WYRD_AUTH_403_UNREPRESENTABLE_DELEGATION");
                assert_eq!(error.status(), 403);
            }
            assert_eq!(
                exchange_outcomes(&audit),
                [(AuditOutcome::Allowed, TOKEN_EXCHANGE_OPERATION.to_owned())],
                "{actor_grants}"
            );
        }
    }

    /// Delegation keeps the subject's root, adds the actor's revalidated root
    /// and credential provenance as the outer actor, and signs only the exact
    /// intersection of the subject's Card access with the actor's root graph.
    ///
    /// Covers an overlapping scoped subject with an earlier actor, a disjoint
    /// scoped subject (an empty intersection still issues), a rootless
    /// unrestricted subject (`any_registered` is the identity), a rootless
    /// restricted subject (refused: no Card authority to delegate), and an
    /// actor token whose root lies outside the actor's family (refused).
    ///
    /// # Panics
    /// Panics when an exchange's outcome, signed roots, Card access, chain,
    /// provenance, or audit chain differ from the case's expectation.
    #[tokio::test]
    async fn delegation_preserves_roots_and_exact_intersections() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let actor = seed_actor(
            &fixture,
            serde_json::json!([{ "resource": "cards", "action": "read", "scope": "all" }]),
        )
        .await;
        let verifier = delegate_service(&RecordedAudit::new()).verifier;
        let subject_root = unregistered_uid();
        let subject_id = Uuid::now_v7();
        let earlier_id = Uuid::now_v7();
        let earlier_root = unregistered_uid();
        let earlier_credential = Uuid::now_v7();
        let reads: PermissionSet = std::iter::once(Permission::card_read()).collect();
        let actor_bearer = actor_token(actor.id, tenant, &actor.root);
        let scoped = |cards: Vec<CardUid>| CardAccess::Scoped { cards };

        let cases = [
            (
                "overlapping subject with an earlier actor",
                mint(
                    service_ref(subject_id, tenant),
                    reads.clone(),
                    None,
                    scoped(vec![unregistered_uid(), actor.shared.clone()]),
                    Some(subject_root.clone()),
                    Some(Box::new(ActClaim {
                        sub: earlier_id.to_string(),
                        principal: service_ref(earlier_id, tenant),
                        workload_uid: earlier_root.clone(),
                        cid: Some(earlier_credential.to_string()),
                        act: None,
                    })),
                ),
                Some(subject_root.clone()),
                scoped(vec![actor.shared.clone()]),
                true,
            ),
            (
                "disjoint direct subject",
                mint(
                    service_ref(subject_id, tenant),
                    reads.clone(),
                    None,
                    scoped(vec![subject_root.clone()]),
                    Some(subject_root.clone()),
                    None,
                ),
                Some(subject_root.clone()),
                scoped(Vec::new()),
                false,
            ),
            (
                "rootless unrestricted subject",
                subject_token(tenant, reads.clone()),
                None,
                scoped(vec![actor.root.clone(), actor.shared.clone()]),
                false,
            ),
        ];
        for (label, subject, expected_root, expected_access, nested) in cases {
            let audit = RecordedAudit::new();
            let exchanged = exchange(&fixture, &audit, subject, actor_bearer.clone())
                .await
                .unwrap_or_else(|error| panic!("{label}: exchange succeeds, got {error:?}"));
            let delegated = verifier
                .verify_on(&exchanged.access_token, &tenant, TokenAudience::Bifrost)
                .expect("delegated token verifies");
            let principal = &delegated.principal;
            assert_eq!(
                principal.workload_uid, expected_root,
                "{label}: subject root"
            );
            assert_eq!(
                principal.card_access, expected_access,
                "{label}: card access"
            );
            assert_eq!(principal.credential_id, None, "{label}: no top-level cid");
            assert_eq!(
                principal.effective_permissions, reads,
                "{label}: permissions"
            );
            let current = &delegated
                .delegation_chain
                .last()
                .expect("an actor is current")
                .principal;
            assert_eq!(current.id.as_uuid(), actor.id, "{label}: actor");
            assert_eq!(
                current.workload_uid.as_ref(),
                Some(&actor.root),
                "{label}: actor root"
            );
            assert_eq!(
                current.credential_id,
                Some(ACTOR_CREDENTIAL),
                "{label}: actor cid"
            );

            let decisions = exchange_decisions(&audit);
            let [
                (
                    _,
                    _,
                    _,
                    _,
                    AuditDetail::TokenExchange {
                        delegation_chain, ..
                    },
                ),
            ] = decisions.as_slice()
            else {
                panic!("{label}: one token-exchange decision, got {decisions:?}");
            };
            if nested {
                let earlier = &delegated.delegation_chain[0].principal;
                assert_eq!(earlier.id.as_uuid(), earlier_id);
                assert_eq!(earlier.workload_uid.as_ref(), Some(&earlier_root));
                assert_eq!(earlier.credential_id, Some(earlier_credential));
                assert_eq!(
                    delegation_chain,
                    &[AuditDelegationStep {
                        principal_id: PrincipalId::new(earlier_id),
                        principal_kind: PrincipalKindTag::Service,
                        workload_uid: Some(earlier_root.clone()),
                    }],
                    "the audit chain names the earlier actor and its root"
                );
            } else {
                assert_eq!(delegated.delegation_chain.len(), 1, "{label}: chain");
                assert!(delegation_chain.is_empty(), "{label}: audit chain");
            }
        }

        let restricted_subject = mint(
            service_ref(subject_id, tenant),
            reads.clone(),
            None,
            CardAccess::RootRequired {
                restriction: FamilyRestriction::Family(service_family("subject-family")),
            },
            None,
            None,
        );
        let error = exchange(
            &fixture,
            &RecordedAudit::new(),
            restricted_subject,
            actor_bearer,
        )
        .await
        .expect_err("a rootless restricted subject has no Card authority to delegate");
        let error = WyrdError::from(error);
        assert!(
            matches!(error, WyrdError::WorkloadContextRequired { .. }),
            "{error:?}"
        );

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        let foreign_root = seed_root(
            &mut conn,
            &named_service_card_ref("delegation-foreign"),
            creator,
        )
        .await;
        conn.commit().await.expect("foreign root commits");
        let error = exchange(
            &fixture,
            &RecordedAudit::new(),
            subject_token(tenant, reads),
            actor_token(actor.id, tenant, &foreign_root),
        )
        .await
        .expect_err("an actor root outside the actor's family is refused");
        let error = WyrdError::from(error);
        assert!(
            matches!(error, WyrdError::WorkloadUidRefused { .. }),
            "{error:?}"
        );
    }
}
