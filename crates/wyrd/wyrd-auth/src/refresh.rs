//! Refresh-token grant: single-use rotation with reuse detection (F07/F08).

use std::sync::Arc;

use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_issue::{IssueError, IssuingKey};
use wyrd_auth_verify::RefreshTokenClaims;
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    consume_active_refresh, list_service_account_roles, refresh_by_hash, revoke_refresh_family,
    service_account_by_id,
};

use crate::audit::{
    REFRESH_FAMILY_REVOKE_OPERATION, TOKEN_EXCHANGE_OPERATION, append_auth_audit, auth_event,
    principal_kind_tag,
};

use crate::card_scope::MINT_KIND_REFRESH;
use crate::exchange_api_key::{
    ExchangedToken, IssueOrSqlError, IssueSubject, RefreshPolicy, TokenExchangeSettings,
    issue_for_subject, role_refs, token_hash,
};

/// Refresh-token rotation service.
pub struct RefreshTokens {
    /// JWT issuing key for minting access and refresh tokens.
    pub issuing_key: Arc<IssuingKey>,
    /// Token lifetime settings.
    pub settings: TokenExchangeSettings,
}

/// Refresh grant failure modes.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    /// Presented token was already rotated or revoked; family has been revoked.
    #[error("refresh token reuse detected")]
    Reused,
    /// Presented token was never valid for this tenant, has expired, or was revoked.
    #[error("refresh token not found or expired")]
    NotFound,
    /// Database operation failed.
    #[error("database error")]
    Database(#[from] sqlx::Error),
    /// Token issue failed.
    #[error("token issue error")]
    Issue(#[from] IssueError),
    /// Wyrd contract error.
    #[error("wyrd error")]
    Wyrd(#[from] WyrdError),
}

impl From<IssueOrSqlError> for RefreshError {
    fn from(error: IssueOrSqlError) -> Self {
        match error {
            IssueOrSqlError::Issue(e) => Self::Issue(e),
            IssueOrSqlError::Database(e) => Self::Database(e),
            IssueOrSqlError::Wyrd(e) => Self::Wyrd(e),
        }
    }
}

impl From<RefreshError> for WyrdError {
    fn from(error: RefreshError) -> Self {
        match error {
            RefreshError::Reused => WyrdError::RefreshReused {
                message: "Refresh token family revoked due to reuse of a rotated token".to_owned(),
                details: json!({}),
            },
            RefreshError::NotFound => WyrdError::RefreshRevoked {
                message: "Refresh token not found, expired, or revoked".to_owned(),
                details: json!({}),
            },
            RefreshError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
            RefreshError::Issue(_) => WyrdError::Internal {
                message: "token issue failed during refresh rotation".to_owned(),
                details: json!({}),
            },
            RefreshError::Wyrd(error) => error,
        }
    }
}

impl RefreshTokens {
    /// Execute the refresh-token grant.
    ///
    /// The full rotation, reuse-detection, and audit write run inside the
    /// transaction owned by `conn`. The caller must call `conn.commit()` on
    /// success.
    ///
    /// Algorithm (F07 atomicity):
    /// 1. SHA-256 the presented JWT string.
    /// 2. `consume_active_refresh` — atomic `UPDATE … RETURNING`. Two concurrent
    ///    callers race on the same row write; exactly one wins.
    ///    - Win → mint successor pair, insert with `rotated_from`, audit, return OK.
    ///    - Loss → `refresh_by_hash`:
    ///      - Stale row found → reuse detected; family revoked, audit, return Reused.
    ///      - No row → return `NotFound`.
    #[tracing::instrument(level = "debug", skip(self, conn, presented), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        presented: SecretString,
        request_id: &str,
    ) -> Result<ExchangedToken, RefreshError> {
        let hash = token_hash(presented.expose_secret());

        match consume_active_refresh(conn, &hash).await? {
            Some(active) => {
                let principal_id = active.principal_id;
                let principal_kind = active.principal_kind.clone();

                let exchanged = match principal_kind.as_str() {
                    // A Card-free tenant administrator rotates through the same
                    // owner as a Card-bound workload. It was excluded before, so
                    // the refresh token its API-key exchange returns always failed.
                    "service" | "agent" | "tenant_admin" => {
                        self.rotate_stored_principal(
                            conn,
                            principal_id,
                            &principal_kind,
                            active.id,
                            request_id,
                        )
                        .await?
                    }
                    "user" => {
                        // Deferred to commit 06 (user-role lookup helper).
                        todo!("user refresh rotation: implement in commit 06")
                    }
                    other => {
                        tracing::error!(
                            principal_kind = other,
                            "unknown principal_kind in refresh token row"
                        );
                        return Err(RefreshError::Issue(IssueError::InvalidPrincipalKind));
                    }
                };

                // Audit the rotation (F08): subject = actor; a refresh grant
                // carries no delegation chain.
                let rotated = PrincipalId::new(principal_id);
                let event = auth_event(
                    request_id,
                    TOKEN_EXCHANGE_OPERATION,
                    rotated,
                    principal_kind_tag(&principal_kind),
                    None,
                    AuditOutcome::Allowed,
                    AuditDetail::TokenExchange {
                        subject_principal_id: rotated,
                        actor_principal_id: rotated,
                        delegation_chain: Vec::new(),
                        expires_at: exchanged.expires_at,
                    },
                );
                append_auth_audit(conn, &event).await?;

                tracing::debug!(
                    principal_id = %principal_id,
                    principal_kind = %principal_kind,
                    rotated_from = %active.id,
                    "refresh token rotated"
                );

                Ok(exchanged)
            }

            None => {
                // consume_active_refresh returned no row. Check whether the token
                // ever existed (reuse of a rotated token) or is unknown.
                if let Some(stale) = refresh_by_hash(conn, &hash).await? {
                    // Reuse detected: this token was already rotated or revoked.
                    // Revoke the entire principal's token family as a theft response.
                    let revoked = revoke_refresh_family(
                        conn,
                        &stale.principal_kind,
                        stale.principal_id,
                        "reuse_detected",
                    )
                    .await?;

                    // Audit the family revocation (F08) as a refused grant.
                    let owner = PrincipalId::new(stale.principal_id);
                    let owner_kind = principal_kind_tag(&stale.principal_kind);
                    let event = auth_event(
                        request_id,
                        REFRESH_FAMILY_REVOKE_OPERATION,
                        owner,
                        owner_kind,
                        None,
                        AuditOutcome::Denied,
                        AuditDetail::RefreshFamilyRevocation {
                            principal_id: owner,
                            principal_kind: owner_kind,
                            revoked_token_count: revoked,
                        },
                    );
                    append_auth_audit(conn, &event).await?;

                    tracing::warn!(
                        principal_id = %stale.principal_id,
                        principal_kind = %stale.principal_kind,
                        revoked_family_rows = revoked,
                        "refresh token reuse detected; family revoked"
                    );

                    Err(RefreshError::Reused)
                } else {
                    tracing::debug!("refresh token not found for presented hash");
                    Err(RefreshError::NotFound)
                }
            }
        }
    }

    /// Issue the successor access+refresh pair for a stored principal.
    ///
    /// The subject is re-read from the durable row rather than trusted from the
    /// consumed token, so a principal whose roles or Card binding changed since
    /// the last exchange rotates under its current state. Issuance itself is the
    /// same owner the initial API-key exchange uses; this path only supplies the
    /// `rotated_from` back-link that keeps the family chain — and therefore reuse
    /// detection — intact.
    ///
    /// # Errors
    /// Returns [`RefreshError::Database`] when the principal row or its roles
    /// cannot be read, and [`RefreshError::Issue`] when the principal has
    /// disappeared, carries an unusable role, or cannot be issued for.
    async fn rotate_stored_principal(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        principal_kind: &str,
        rotated_from: Uuid,
        request_id: &str,
    ) -> Result<ExchangedToken, RefreshError> {
        let stored = service_account_by_id(conn, principal_id)
            .await?
            .ok_or_else(|| {
                tracing::warn!(
                    principal_id = %principal_id,
                    "principal missing during refresh rotation"
                );
                sqlx::Error::RowNotFound
            })?;

        let roles: Vec<RoleRef> = role_refs(list_service_account_roles(conn, principal_id).await?)
            .map_err(|_| RefreshError::Issue(IssueError::InvalidPrincipalKind))?;

        issue_for_subject(
            conn,
            &self.issuing_key,
            &self.settings,
            IssueSubject {
                principal_id,
                principal_kind: principal_kind.to_owned(),
                card_ref: stored.card_ref.map(|card_ref| card_ref.0),
                roles,
            },
            RefreshPolicy::Rotate(rotated_from),
            request_id,
            MINT_KIND_REFRESH,
        )
        .await
        .map_err(RefreshError::from)
    }
}

/// Decode refresh token claims from the JWT payload without signature
/// verification.
///
/// Tenant routing uses these claims to select the correct `TenantConn` before
/// opening the database. The hash lookup and atomic consume under RLS are the
/// real security authorities; this decode only routes the request to the right
/// tenant. A forged or malformed token that cannot be decoded is rejected
/// immediately as `RefreshRevoked`.
pub fn claims_from_refresh_jwt(token: &str) -> Result<RefreshTokenClaims, WyrdError> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(bad_refresh_token_format)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| bad_refresh_token_format())?;
    serde_json::from_slice::<RefreshTokenClaims>(&bytes).map_err(|_| bad_refresh_token_format())
}

/// Extract the tenant id from an unverified refresh JWT.
pub fn tenant_from_refresh_jwt(token: &str) -> Result<DataTenantId, WyrdError> {
    claims_from_refresh_jwt(token).map(|c| c.tenant_id)
}

fn bad_refresh_token_format() -> WyrdError {
    WyrdError::RefreshRevoked {
        message: "refresh token is not a valid Wyrd JWT".to_owned(),
        details: json!({}),
    }
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use secrecy::{ExposeSecret, SecretString};
    use sha2::{Digest, Sha256};
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        AccessTokenClaims, Kid, PermissionResolver, ResolveError, public_key_from_pem, verify_eddsa,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{PermissionSet, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::{CardKind, Spec};
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{insert_refresh_token, insert_service_account, refresh_by_hash};
    use wyrd_sql::queries::cards::get_card_by_ref;

    use wyrd_dev_fixtures::cards::{seed_backing_card, seed_card_with_spec};

    use super::{RefreshError, RefreshTokens};
    use crate::exchange_api_key::TokenExchangeSettings;

    /// Return the resolved space of a test card reference.
    fn space_of(card_ref: &CardRef) -> &SpaceName {
        card_ref.space.as_ref().expect("test card ref has a space")
    }

    /// Resolver stub for token projection: this test asserts signed Card scope,
    /// not role permissions, so it grants nothing.
    #[derive(Debug)]
    struct AllowNothingResolver;

    impl PermissionResolver for AllowNothingResolver {
        async fn resolve(
            &self,
            _tenant_id: &DataTenantId,
            _roles: &[RoleRef],
        ) -> Result<PermissionSet, ResolveError> {
            Ok(PermissionSet::new())
        }
    }

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

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

    fn refresh_service() -> RefreshTokens {
        RefreshTokens {
            issuing_key: test_issuing_key(),
            settings: TokenExchangeSettings::default(),
        }
    }

    fn service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("test-service").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn hash_of(token: &SecretString) -> String {
        format!("{:x}", Sha256::digest(token.expose_secret().as_bytes()))
    }

    fn issue_refresh_jwt(
        key: &IssuingKey,
        principal_kind: PrincipalKindTag,
        principal_id: Uuid,
        tenant_id: DataTenantId,
    ) -> SecretString {
        let jwt = key
            .issue_refresh_token(
                principal_kind,
                PrincipalId::new(principal_id),
                tenant_id,
                Duration::days(30),
            )
            .expect("refresh token issues");
        SecretString::from(jwt)
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
        created_by: Uuid,
        card_ref: &CardRef,
    ) -> Uuid {
        seed_backing_card(conn, card_ref, created_by).await;
        let sa_id = Uuid::new_v4();
        insert_service_account(
            conn,
            sa_id,
            "service",
            Some(card_ref),
            "test-sa",
            None,
            created_by,
        )
        .await
        .expect("service account inserts");
        sa_id
    }

    async fn seed_active_refresh(
        conn: &mut TenantConn<'_>,
        principal_kind: &str,
        principal_id: Uuid,
        token_hash: &str,
    ) {
        insert_refresh_token(
            conn,
            Uuid::new_v4(),
            principal_kind,
            principal_id,
            token_hash,
            Utc::now() + Duration::days(30),
        )
        .await
        .expect("refresh token inserts");
    }

    #[tokio::test]
    async fn happy_rotation_mints_new_pair_and_revokes_old_row() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, &card_ref).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let original_hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "service", sa_id, &original_hash).await;

        let result = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-happy-rotation")
            .await;

        assert!(result.is_ok(), "rotation succeeds: {result:?}");
        let exchanged = result.unwrap();

        // Old row is revoked with reason='rotated'.
        let old_row = refresh_by_hash(&mut conn, &original_hash)
            .await
            .expect("lookup")
            .expect("old row exists");
        assert_eq!(old_row.revoked_reason.as_deref(), Some("rotated"));

        // New row exists and links back via rotated_from.
        let new_hash = hash_of(
            exchanged
                .refresh_token
                .as_ref()
                .expect("rotation issues a refresh token"),
        );
        let new_row = refresh_by_hash(&mut conn, &new_hash)
            .await
            .expect("lookup")
            .expect("new row exists");
        assert_eq!(new_row.rotated_from, Some(old_row.id));
        assert!(new_row.revoked_at.is_none(), "new token is active");
    }

    #[tokio::test]
    async fn reuse_detection_revokes_family() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, &card_ref).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let stale_hash = hash_of(&refresh_jwt);

        // Insert the token as already-revoked (simulates a previously rotated token).
        sqlx::query(
            r"
            INSERT INTO wyrd.auth_refresh_tokens
                (id, data_tenant_id, principal_kind, principal_id, token_hash,
                 expires_at, revoked_at, revoked_reason)
            VALUES ($1, $2, 'service', $3, $4, now() + interval '30 days', now(), 'rotated')
            ",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&stale_hash)
        .execute(&mut **conn.transaction())
        .await
        .expect("stale token inserts");

        // Insert a second active token for the same principal (a sibling in the family).
        seed_active_refresh(&mut conn, "service", sa_id, "hash-active-sibling").await;

        let result = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-reuse")
            .await;

        assert!(
            matches!(result, Err(RefreshError::Reused)),
            "reuse detected: {result:?}"
        );

        // The sibling should also be revoked.
        let sibling = refresh_by_hash(&mut conn, "hash-active-sibling")
            .await
            .expect("lookup")
            .expect("sibling exists");
        assert_eq!(
            sibling.revoked_reason.as_deref(),
            Some("reuse_detected"),
            "sibling revoked by family revoke"
        );
    }

    /// F07 race: the second caller presenting the same token after the first has
    /// committed the rotation gets the Reused response, not a second Ok.
    #[tokio::test]
    async fn f07_race_second_caller_gets_reused_after_commit() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let card_ref = service_card_ref();

        let mut setup_conn = fixture.tenant_conn().await.expect("setup conn opens");
        let user_id = insert_test_user(&mut setup_conn, tenant).await;
        let sa_id = insert_test_service_account(&mut setup_conn, user_id, &card_ref).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let original_hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut setup_conn, "service", sa_id, &original_hash).await;
        setup_conn.commit().await.expect("setup commits");

        // First caller: present the token, rotate, and commit.
        let mut conn_a = fixture.tenant_conn().await.expect("conn_a opens");
        let token_a = SecretString::from(refresh_jwt.expose_secret().to_owned());
        let result_a = refresh_service()
            .execute(&mut conn_a, token_a, "req-race-a")
            .await;
        assert!(result_a.is_ok(), "first caller rotates: {result_a:?}");
        conn_a.commit().await.expect("conn_a commits");

        // Second caller: present the same original token after the first has committed.
        let mut conn_b = fixture.tenant_conn().await.expect("conn_b opens");
        let token_b = SecretString::from(refresh_jwt.expose_secret().to_owned());
        let result_b = refresh_service()
            .execute(&mut conn_b, token_b, "req-race-b")
            .await;

        assert!(
            matches!(result_b, Err(RefreshError::Reused)),
            "second caller gets Reused: {result_b:?}"
        );

        // The successor token from conn_a's rotation should also be revoked
        // by the family revoke triggered by reuse detection.
        let exchanged_a = result_a.unwrap();
        let successor_hash = hash_of(
            exchanged_a
                .refresh_token
                .as_ref()
                .expect("rotation issues a refresh token"),
        );
        let successor = refresh_by_hash(&mut conn_b, &successor_hash)
            .await
            .expect("lookup")
            .expect("successor exists");
        assert_eq!(
            successor.revoked_reason.as_deref(),
            Some("reuse_detected"),
            "successor token revoked by reuse-detection family revoke"
        );
    }

    #[tokio::test]
    async fn unknown_token_returns_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        // A valid JWT that has never been inserted in the DB.
        let refresh_jwt =
            issue_refresh_jwt(&key, PrincipalKindTag::Service, Uuid::new_v4(), tenant);

        let result = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-not-found")
            .await;

        assert!(matches!(result, Err(RefreshError::NotFound)));
    }

    #[tokio::test]
    async fn expired_token_is_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, &card_ref).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let hash = hash_of(&refresh_jwt);

        // Insert with expires_at in the past and no revoked_at.
        // consume_active_refresh filters on expires_at > now(), so this row is not consumed.
        // refresh_by_hash has no lifecycle filter, so it finds this row and returns Reused.
        sqlx::query(
            r"
            INSERT INTO wyrd.auth_refresh_tokens
                (id, data_tenant_id, principal_kind, principal_id, token_hash, expires_at)
            VALUES ($1, $2, 'service', $3, $4, now() - interval '1 hour')
            ",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .bind(&hash)
        .execute(&mut **conn.transaction())
        .await
        .expect("expired token inserts");

        let result = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-expired")
            .await;

        // Expired row is found by refresh_by_hash (no lifecycle filter) → Reused.
        assert!(
            matches!(result, Err(RefreshError::Reused)),
            "expired token triggers reuse detection: {result:?}"
        );
    }

    #[tokio::test]
    async fn f08_audit_row_written_on_rotation() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, &card_ref).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "service", sa_id, &hash).await;

        refresh_service()
            .execute(&mut conn, refresh_jwt, "req-audit-check")
            .await
            .expect("rotation succeeds");

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = 'auth.token.exchange'
                AND principal_id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("audit count query runs");

        assert!(count.0 >= 1, "at least one audit row written on rotation");
    }

    #[test]
    fn claims_from_refresh_jwt_decodes_payload() {
        let key = test_issuing_key();
        let tenant_id: DataTenantId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id");
        let principal_id = Uuid::new_v4();
        let jwt = key
            .issue_refresh_token(
                PrincipalKindTag::Service,
                PrincipalId::new(principal_id),
                tenant_id,
                Duration::days(30),
            )
            .expect("refresh jwt issues");

        let claims = super::claims_from_refresh_jwt(&jwt).expect("claims decode");

        assert_eq!(claims.tenant_id, tenant_id);
        assert_eq!(claims.principal_id.as_uuid(), principal_id);
        assert_eq!(claims.principal_kind, PrincipalKindTag::Service);
    }

    #[test]
    fn claims_from_refresh_jwt_rejects_malformed_input() {
        assert!(super::claims_from_refresh_jwt("not.a.jwt").is_err());
        assert!(super::claims_from_refresh_jwt("onlyone").is_err());
    }

    #[test]
    fn tenant_from_refresh_jwt_extracts_tenant() {
        let key = test_issuing_key();
        let tenant_id: DataTenantId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id");
        let jwt = key
            .issue_refresh_token(
                PrincipalKindTag::Service,
                PrincipalId::new(Uuid::new_v4()),
                tenant_id,
                Duration::days(30),
            )
            .expect("refresh jwt issues");

        let extracted = super::tenant_from_refresh_jwt(&jwt).expect("tenant extracted");

        assert_eq!(extracted, tenant_id);
    }

    /// Ingest resolves Card correlation from signed claims alone, so a real
    /// rotation must sign, verify, and project every scope member's registry
    /// UID through to the runtime `Principal`.
    #[tokio::test]
    async fn refresh_signs_resolved_scope_uids() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let root = service_card_ref();
        let secondary = CardRef {
            name: CardName::new("scope-secondary").expect("static name is valid"),
            ..service_card_ref()
        };

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        seed_backing_card(&mut conn, &secondary, user_id).await;
        let root_spec = Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [{
                    "alias": "secondary",
                    "ref": {
                        "kind": "Service",
                        "space": space_of(&secondary).as_str(),
                        "name": secondary.name.as_str(),
                        "version": secondary.version.as_str(),
                    },
                }],
            }),
        )
        .expect("root service spec decodes");
        seed_card_with_spec(&mut conn, &root, &root_spec, user_id).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, &root).await;

        let expected_root_uid = get_card_by_ref(
            &mut conn,
            root.kind.clone(),
            space_of(&root),
            &root.name,
            &root.version,
        )
        .await
        .expect("root card row loads")
        .card_uid;
        let expected_secondary_uid = get_card_by_ref(
            &mut conn,
            secondary.kind.clone(),
            space_of(&secondary),
            &secondary.name,
            &secondary.version,
        )
        .await
        .expect("secondary card row loads")
        .card_uid;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "service", sa_id, &hash).await;

        let exchanged = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-scope-uids")
            .await
            .expect("rotation succeeds");

        let verifying_pem = key.verifying_key_pem().expect("verifying key encodes");
        let decoding_key =
            public_key_from_pem(verifying_pem.as_bytes()).expect("verifying key decodes");
        let claims: AccessTokenClaims = verify_eddsa(
            exchanged.access_token.expose_secret(),
            &decoding_key,
            Some("wyrd"),
        )
        .expect("signed access token verifies");

        let verified = claims
            .into_verified(&AllowNothingResolver)
            .await
            .expect("verified token projects");
        let PrincipalKind::Service { card_ref_scope, .. } = verified.principal.kind.clone() else {
            panic!("service rotation yields a service principal");
        };

        assert_eq!(
            card_ref_scope.len(),
            2,
            "root and secondary are both signed"
        );
        let signed_root = card_ref_scope
            .as_slice()
            .iter()
            .find(|member| member.same_identity(&root))
            .expect("root member is signed");
        assert_eq!(
            signed_root.uid.as_ref(),
            Some(&expected_root_uid),
            "verified root member keeps its registry uid"
        );
        let signed_secondary = card_ref_scope
            .as_slice()
            .iter()
            .find(|member| member.same_identity(&secondary))
            .expect("secondary member is signed");
        assert_eq!(
            signed_secondary.uid.as_ref(),
            Some(&expected_secondary_uid),
            "verified secondary member keeps its registry uid"
        );
    }

    #[tokio::test]
    async fn rotation_writes_card_scope_mint_audit_row() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, user_id, &card_ref).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::Service, sa_id, tenant);
        let hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "service", sa_id, &hash).await;

        refresh_service()
            .execute(&mut conn, refresh_jwt, "req-scope-audit")
            .await
            .expect("rotation succeeds");

        let (outcome, detail): (String, String) = sqlx::query_as(
            "SELECT outcome, detail
               FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = 'auth.card_scope.mint'
                AND principal_id = $2
              LIMIT 1",
        )
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("card scope mint audit event is staged");
        let detail: serde_json::Value =
            serde_json::from_str(&detail).expect("audit detail is json");

        assert_eq!(detail["mint_kind"], "refresh", "mint_kind is refresh");
        assert_eq!(outcome, "allowed", "mint is allowed");
        assert_eq!(
            detail["scope_member_count"], 1,
            "single-card scope has member count 1"
        );
    }
}
