//! API-key exchange and RFC 8693 delegation entry paths.
//!
//! Each verifies only its own grant-specific evidence — the presented API key
//! or the caller's access token and delegation permission — then mints through
//! the shared [`TenantTokenIssuer`].

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use sha2::{Digest, Sha256};
use wyrd_auth_issue::DelegationCaller;
use wyrd_auth_verify::{ActClaim, AuthError, TokenPrincipalRef, TokenVerifier};
use wyrd_runtime::{Permission, PermissionCheck, RoleRef};
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::RequestedSubject;
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    ApiKeyStatus, api_key_by_prefix, api_key_status_by_prefix, service_account_by_id,
    touch_api_key_last_used,
};

use crate::credential_verify::verify_presented;
use crate::error::auth_error_to_wyrd;
use crate::issuance::{ExchangedToken, IssuanceError, TenantGrant, TenantTokenIssuer};
use crate::issue_api_key::{WyrdApiKey, principal_kind_for_card};

/// API-key exchange service.
#[derive(Clone, Debug)]
pub struct ExchangeApiKey {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
}

/// Delegated token service.
#[derive(Clone)]
pub struct DelegateToken {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
    /// Wyrd access-token verifier for the caller's subject token.
    pub verifier: std::sync::Arc<TokenVerifier>,
    /// Permission checker.
    pub permission_check: std::sync::Arc<dyn PermissionCheck>,
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

/// Delegated token failure.
#[derive(Debug, thiserror::Error)]
pub enum DelegateError {
    /// Subject token failed verification.
    #[error("subject token invalid")]
    InvalidSubjectToken(#[from] AuthError),
    /// Requested subject not found, inactive, or not Card-bound.
    #[error("requested subject not found")]
    SubjectNotFound,
    /// Permission denied.
    #[error("caller lacks delegation issue permission")]
    PermissionDenied,
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    /// The shared issuance workflow failed.
    #[error("token issuance failed")]
    Issuance(IssuanceError),
}

impl From<IssuanceError> for DelegateError {
    fn from(error: IssuanceError) -> Self {
        match error {
            IssuanceError::PrincipalInactive => Self::SubjectNotFound,
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
    /// Exchange an access token for a delegated Service/Agent token.
    ///
    /// Verifies the caller's access token locally, requires the delegation
    /// permission in its claims, and resolves the requested Card-bound
    /// subject, then mints through the shared issuance workflow with the
    /// caller as the newest `act` layer.
    ///
    /// # Errors
    /// Returns a typed error when verification, permission, subject resolution,
    /// issuance, or database work fails.
    #[tracing::instrument(level = "debug", skip(self, conn, subject_token), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        subject_token: SecretString,
        requested_subject: RequestedSubject,
        request_id: &str,
    ) -> Result<ExchangedToken, DelegateError> {
        let tenant = conn.data_tenant_id();
        let verified = self.verifier.verify(&subject_token, &tenant)?;
        self.permission_check
            .check(&verified.principal, &Permission::delegation_issue())
            .into_result()
            .map_err(|_| DelegateError::PermissionDenied)?;

        let row = resolve_requested_subject(conn, requested_subject).await?;
        // Delegation hands a caller's authority to a deployed workload, so
        // the target must be Card-bound.
        if row.card_ref.is_none() {
            return Err(DelegateError::SubjectNotFound);
        }
        let caller = DelegationCaller {
            sub: verified.delegation_chain.first().map_or_else(
                || verified.principal.id.to_string(),
                |step| step.principal.id.to_string(),
            ),
            principal: TokenPrincipalRef::from(&verified.principal),
            act: act_from_chain(&verified.delegation_chain, tenant),
        };
        // Delegated tokens are short-lived and non-refreshable by design
        // (RFC 8693): the caller re-delegates when the token expires.
        Ok(self
            .issuer
            .issue(conn, row.id, TenantGrant::Delegation(caller), request_id)
            .await?)
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

/// Rebuild an RFC 8693 `act` chain (newest layer outermost) from a verified
/// initiator-first delegation chain.
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
            DelegateError::InvalidSubjectToken(error) => auth_error_to_wyrd(error),
            DelegateError::SubjectNotFound => WyrdError::PrincipalNotFound {
                message: "requested principal not found in tenant".to_owned(),
                details: json!({}),
            },
            DelegateError::PermissionDenied => WyrdError::PermissionDeniedRbac {
                message: "caller lacks delegation issue permission".to_owned(),
                details: json!({ "required": Permission::delegation_issue() }),
            },
            DelegateError::Database(error) => IssuanceError::Database(error).into(),
            DelegateError::Issuance(error) => error.into(),
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
    use wyrd_auth_issue::{AccessGrant, IssuingKey};
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_dev_fixtures::cards::seed_backing_card;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, PermissionSet, PrincipalId, RbacCheck};
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

    use super::{DelegateError, DelegateToken, ExchangeApiKey, ExchangeError};
    use crate::issuance::{TenantTokenIssuer, TokenExchangeSettings};
    use crate::issue_api_key::WyrdApiKey;
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

    fn test_issuer() -> TenantTokenIssuer {
        TenantTokenIssuer::new(test_issuing_key(), TokenExchangeSettings::default())
    }

    fn exchange_service() -> ExchangeApiKey {
        ExchangeApiKey {
            issuer: test_issuer(),
        }
    }

    /// Build a delegation service whose verifier trusts the test signing key.
    fn delegate_service() -> DelegateToken {
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
            permission_check: Arc::new(RbacCheck),
        }
    }

    /// Mint a Card-bound Service subject token carrying `permissions`.
    fn subject_token(tenant: DataTenantId, permissions: PermissionSet) -> String {
        test_issuing_key()
            .issue_access_token(
                AccessGrant {
                    principal: TokenPrincipalRef {
                        id: PrincipalId::new(Uuid::new_v4()),
                        kind: PrincipalKindTag::Service,
                        tenant_id: tenant,
                        card_ref: Some(test_service_card_ref()),
                        card_ref_scope: CardRefScope::default(),
                    },
                    roles: vec![],
                    permissions,
                    credential_id: None,
                    delegated_by: None,
                },
                Duration::minutes(5),
            )
            .expect("subject token issues")
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

    #[tokio::test]
    async fn delegation_permission_denied() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        // A subject token with no permissions fails the delegation_issue
        // check before any database lookup.
        let subject_token = subject_token(tenant, PermissionSet::new());
        let delegate = delegate_service();

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

        // The delegator's token carries `delegation_issue` directly; seed the
        // builtin roles and the target Service principal the exchange
        // resolves by card_ref.
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

        let subject_token = subject_token(
            tenant,
            std::iter::once(Permission::delegation_issue()).collect(),
        );
        let delegate = delegate_service();

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
