//! Refresh-token grant: single-use rotation with reuse detection (F07/F08).

use std::sync::Arc;

use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wyrd_auth_issue::{IssueError, IssuingKey};
use wyrd_auth_verify::RefreshTokenClaims;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    consume_active_refresh, list_user_roles, refresh_by_hash, revoke_refresh_family,
};

use crate::audit::{
    REFRESH_FAMILY_REVOKE_OPERATION, append_auth_audit, auth_event, principal_kind_tag,
};

use crate::callback::issue_and_record_user_session;
use crate::exchange_api_key::{
    ExchangedToken, IssueOrSqlError, TokenExchangeSettings, role_refs, token_hash,
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
        let conn_tenant = conn.data_tenant_id();

        match consume_active_refresh(conn, &hash).await? {
            Some(active) => {
                let principal_id = active.principal_id;
                let principal_kind = active.principal_kind.clone();

                // Only a human session holds a refresh token. A machine
                // client re-exchanges its durable API key or workload
                // assertion, so a machine row here is either pre-existing state
                // from before that split or a forgery, and neither may rotate.
                if principal_kind.as_str() != "user" {
                    tracing::warn!(
                        principal_kind = %principal_kind,
                        "refresh rotation refused for a non-human principal"
                    );
                    return Err(RefreshError::Issue(IssueError::InvalidPrincipalKind));
                }

                // The successor carries the roles the user holds now, not the
                // ones the consumed token was minted with, so a revoked role
                // does not survive a renewal. Issuance, the family back-link,
                // and the audited grant all run through the same owner first
                // login used.
                let roles = role_refs(list_user_roles(conn, principal_id).await?)
                    .map_err(|_| RefreshError::Issue(IssueError::InvalidPrincipalKind))?;
                let exchanged = issue_and_record_user_session(
                    conn,
                    self.issuing_key.as_ref(),
                    conn_tenant,
                    principal_id,
                    roles,
                    Some(active.id),
                    request_id,
                )
                .await?;

                tracing::debug!(
                    principal_id = %principal_id,
                    rotated_from = %active.id,
                    "human refresh token rotated"
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
    use wyrd_auth_issue::{IssueError, IssuingKey};
    use wyrd_auth_verify::{AccessTokenClaims, Kid, public_key_from_pem, verify_eddsa};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{
        insert_refresh_token, insert_role, insert_service_account, list_user_roles,
        refresh_by_hash, replace_user_roles,
    };

    use wyrd_dev_fixtures::cards::seed_backing_card;

    use super::{RefreshError, RefreshTokens};
    use crate::audit::REFRESH_FAMILY_REVOKE_OPERATION;
    use crate::exchange_api_key::TokenExchangeSettings;

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

    /// A human session rotates: the consumed row is retired and the successor
    /// links back to it.
    #[tokio::test]
    async fn happy_rotation_mints_new_pair_and_revokes_old_row() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let original_hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "user", user_id, &original_hash).await;

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

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let stale_hash = hash_of(&refresh_jwt);

        // Insert the token as already-revoked (simulates a previously rotated token).
        sqlx::query(
            r"
            INSERT INTO wyrd.auth_refresh_tokens
                (id, data_tenant_id, principal_kind, principal_id, token_hash,
                 expires_at, revoked_at, revoked_reason)
            VALUES ($1, $2, 'user', $3, $4, now() + interval '30 days', now(), 'rotated')
            ",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.as_uuid())
        .bind(user_id)
        .bind(&stale_hash)
        .execute(&mut **conn.transaction())
        .await
        .expect("stale token inserts");

        // Insert a second active token for the same principal (a sibling in the family).
        seed_active_refresh(&mut conn, "user", user_id, "hash-active-sibling").await;

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

        let mut setup_conn = fixture.tenant_conn().await.expect("setup conn opens");
        let user_id = insert_test_user(&mut setup_conn, tenant).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let original_hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut setup_conn, "user", user_id, &original_hash).await;
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

    /// The rotation's audited grant names the refresh row it consumed.
    ///
    /// Without it, a renewed human session and a first federated sign-in are
    /// indistinguishable in retained evidence, even though one of them was
    /// authenticated by a stored credential this deployment can revoke.
    #[tokio::test]
    async fn f08_audit_row_written_on_rotation() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "user", user_id, &hash).await;
        let consumed = refresh_by_hash(&mut conn, &hash)
            .await
            .expect("lookup")
            .expect("seeded row exists")
            .id;

        refresh_service()
            .execute(&mut conn, refresh_jwt, "req-audit-check")
            .await
            .expect("rotation succeeds");

        let credentials: Vec<Option<Uuid>> = sqlx::query_scalar(
            "SELECT credential_id FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = 'auth.token.exchange'
                AND principal_id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(user_id)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit query runs");

        assert_eq!(
            credentials,
            vec![Some(consumed)],
            "the rotation is audited once, naming the consumed refresh row"
        );
    }

    /// A rotated human session keeps the authority its login established.
    ///
    /// Rotation re-reads the grant table rather than trusting the consumed
    /// token, so the federated login path has to have persisted what the
    /// provider asserted. This seeds that state the way login writes it —
    /// including a name with no local role row, which resolves to no
    /// permission anywhere and is therefore not stored — and proves the
    /// successor access token is signed with the roles that survived.
    #[tokio::test]
    async fn rotation_carries_the_roles_login_persisted() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        insert_role(
            &mut conn,
            Uuid::new_v4(),
            "runtime_admin",
            &serde_json::json!([]),
            false,
        )
        .await
        .expect("role seeds");
        replace_user_roles(&mut conn, user_id, &["runtime_admin", "only-at-the-idp"])
            .await
            .expect("login persists the asserted roles");
        assert_eq!(
            list_user_roles(&mut conn, user_id)
                .await
                .expect("roles list"),
            vec!["runtime_admin".to_owned()],
            "a name with no local role row is not recorded as authority"
        );

        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "user", user_id, &hash).await;

        let exchanged = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-rotation-roles")
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
        .expect("successor access token verifies");

        assert_eq!(
            claims
                .roles
                .iter()
                .map(|role| role.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["runtime_admin".to_owned()],
            "the successor carries the session's authority"
        );
    }

    /// R2-4: replay containment survives the refused request.
    ///
    /// Reuse is the one refusal that also writes. The route commits the staged
    /// family revocation before rendering its `401`, so this drives the same
    /// sequence a real caller does — rotate, replay, commit the refusal — and
    /// then opens a *fresh* transaction to prove the containment is durable
    /// rather than rolled back with the failed request: the attacker's
    /// successor is dead, it cannot itself rotate, and the family revocation
    /// was audited exactly once.
    #[tokio::test]
    async fn f09_replay_containment_commits_and_kills_the_successor() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();

        let mut setup_conn = fixture.tenant_conn().await.expect("setup conn opens");
        let user_id = insert_test_user(&mut setup_conn, tenant).await;
        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let original_hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut setup_conn, "user", user_id, &original_hash).await;
        setup_conn.commit().await.expect("setup commits");

        // The legitimate rotation, committed the way the route commits it.
        let mut conn_a = fixture.tenant_conn().await.expect("conn_a opens");
        let rotated = refresh_service()
            .execute(
                &mut conn_a,
                SecretString::from(refresh_jwt.expose_secret().to_owned()),
                "req-replay-rotate",
            )
            .await
            .expect("rotation succeeds");
        conn_a.commit().await.expect("conn_a commits");
        let successor = rotated
            .refresh_token
            .expect("rotation issues a refresh token");
        let successor_hash = hash_of(&successor);

        // The replay. The route commits this transaction for `Reused` alone.
        let mut conn_b = fixture.tenant_conn().await.expect("conn_b opens");
        let replay = refresh_service()
            .execute(
                &mut conn_b,
                SecretString::from(refresh_jwt.expose_secret().to_owned()),
                "req-replay",
            )
            .await;
        assert!(
            matches!(replay, Err(RefreshError::Reused)),
            "replaying the consumed token is refused: {replay:?}"
        );
        conn_b
            .commit()
            .await
            .expect("the refused request still commits");

        // A separate transaction: everything below is committed state.
        let mut conn_c = fixture.tenant_conn().await.expect("conn_c opens");
        let successor_row = refresh_by_hash(&mut conn_c, &successor_hash)
            .await
            .expect("lookup")
            .expect("successor row exists");
        assert_eq!(
            successor_row.revoked_reason.as_deref(),
            Some("reuse_detected"),
            "the successor is revoked in committed state"
        );

        let revocations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = $2
                AND principal_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(REFRESH_FAMILY_REVOKE_OPERATION)
        .bind(user_id)
        .fetch_one(&mut **conn_c.transaction())
        .await
        .expect("audit query runs");
        assert_eq!(
            revocations, 1,
            "exactly one family revocation is visible from another transaction"
        );

        let successor_replay = refresh_service()
            .execute(&mut conn_c, successor, "req-successor")
            .await;
        assert!(
            matches!(successor_replay, Err(RefreshError::Reused)),
            "the successor cannot rotate: {successor_replay:?}"
        );
    }

    /// A machine principal's refresh row cannot rotate.
    ///
    /// Machine clients re-exchange a durable credential; they are never issued
    /// a refresh token. A row claiming otherwise is either pre-split residue or
    /// a forgery, and rotating it would hand out a long-lived successor to a
    /// holder the split says should not have one.
    #[tokio::test]
    async fn a_machine_refresh_row_cannot_rotate() {
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

        let result = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-machine-rotate")
            .await;

        assert!(
            matches!(
                result,
                Err(RefreshError::Issue(IssueError::InvalidPrincipalKind))
            ),
            "a machine refresh row is refused: {result:?}"
        );
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
}
