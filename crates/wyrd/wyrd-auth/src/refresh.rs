//! Refresh-token grant: single-use rotation with reuse detection (F07/F08).

use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_issue::{IssueError, IssuingKey};
use wyrd_auth_verify::RefreshTokenClaims;
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::TokenType;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    consume_active_refresh, insert_audit_token_exchange, insert_refresh_token_rotated,
    list_service_account_roles, refresh_by_hash, revoke_refresh_family, service_account_by_id,
};

use crate::card_scope::{
    IssueErrorOrWyrd, MINT_KIND_REFRESH, issue_scope_error, resolve_card_ref_scope,
    write_scope_mint_success_audit,
};
use crate::exchange_api_key::{
    ExchangedToken, IssueOrSqlError, TokenExchangeSettings, principal_kind_wire, role_refs,
    token_hash,
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
                    "service" | "agent" => {
                        self.issue_rotated_for_service_principal(
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

                // Audit the rotation using the existing audit_token_exchange table (F08).
                // subject = actor = principal_id; no delegation chain on a refresh grant.
                insert_audit_token_exchange(
                    conn,
                    Uuid::new_v4(),
                    principal_id,
                    principal_id,
                    json!([]),
                    request_id,
                    exchanged.expires_at,
                )
                .await?;

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

                    // Audit the family revocation (F08).
                    insert_audit_token_exchange(
                        conn,
                        Uuid::new_v4(),
                        stale.principal_id,
                        stale.principal_id,
                        json!([]),
                        request_id,
                        Utc::now(),
                    )
                    .await?;

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

    /// Issue a new access+refresh pair for a service or agent principal and
    /// insert the successor refresh token row with a `rotated_from` back-link.
    ///
    /// This intentionally does NOT call `issue_for_subject`, which inserts via
    /// `insert_refresh_token` (no `rotated_from`). The rotation path must use
    /// `insert_refresh_token_rotated` to preserve the chain.
    async fn issue_rotated_for_service_principal(
        &self,
        conn: &mut TenantConn<'_>,
        principal_id: Uuid,
        principal_kind: &str,
        rotated_from: Uuid,
        request_id: &str,
    ) -> Result<ExchangedToken, RefreshError> {
        let sa = service_account_by_id(conn, principal_id)
            .await?
            .ok_or_else(|| {
                tracing::warn!(
                    principal_id = %principal_id,
                    "service account missing during refresh rotation"
                );
                sqlx::Error::RowNotFound
            })?;

        let roles: Vec<RoleRef> = role_refs(list_service_account_roles(conn, principal_id).await?)
            .map_err(|_| RefreshError::Issue(IssueError::InvalidPrincipalKind))?;

        let pid = PrincipalId::new(principal_id);
        let tenant_id = conn.data_tenant_id();
        let card_ref = sa.card_ref.0;
        let card_ref_scope = resolve_card_ref_scope(conn, &card_ref).await?;

        let access_token = match principal_kind {
            "service" => self
                .issuing_key
                .issue_service_access_token(
                    pid,
                    tenant_id,
                    card_ref.clone(),
                    card_ref_scope.clone(),
                    roles.clone(),
                    self.settings.access_ttl,
                )
                .map_err(|e| map_refresh_issue_error(e, &card_ref))?,
            "agent" => self
                .issuing_key
                .issue_agent_access_token(
                    pid,
                    tenant_id,
                    card_ref.clone(),
                    card_ref_scope.clone(),
                    roles.clone(),
                    self.settings.access_ttl,
                )
                .map_err(|e| map_refresh_issue_error(e, &card_ref))?,
            _ => return Err(RefreshError::Issue(IssueError::InvalidPrincipalKind)),
        };

        let kind_wire = principal_kind_wire(principal_kind)
            .ok_or(RefreshError::Issue(IssueError::InvalidPrincipalKind))?;

        let refresh_token = self.issuing_key.issue_refresh_token(
            kind_wire,
            pid,
            tenant_id,
            self.settings.refresh_ttl,
        )?;

        let expires_at = Utc::now() + self.settings.access_ttl;
        let refresh_expires_at = Utc::now() + self.settings.refresh_ttl;
        let new_refresh_hash = token_hash(&refresh_token);

        // Insert the successor token with the rotated_from back-link.
        // The predecessor was already atomically revoked by consume_active_refresh.
        insert_refresh_token_rotated(
            conn,
            Uuid::new_v4(),
            principal_kind,
            principal_id,
            &new_refresh_hash,
            refresh_expires_at,
            rotated_from,
        )
        .await?;

        write_scope_mint_success_audit(
            conn,
            principal_id,
            &card_ref,
            &card_ref_scope,
            request_id,
            MINT_KIND_REFRESH,
        )
        .await?;

        Ok(ExchangedToken {
            access_token: SecretString::from(access_token),
            refresh_token: Some(SecretString::from(refresh_token)),
            token_type: TokenType::Bearer,
            expires_at,
        })
    }
}

/// Convert a card-bound issuer error into the refresh error channel.
///
/// Routes `IssueError::CardScopeTooLarge` through `issue_scope_error` so that
/// the resulting `WyrdError::CardScopeTooLarge` always carries `scope_mint_root`
/// in its details, enabling `scope_failure_root` to extract the root for the
/// audit failure write.
fn map_refresh_issue_error(error: IssueError, root: &CardRef) -> RefreshError {
    match issue_scope_error(error, root) {
        IssueErrorOrWyrd::Issue(e) => RefreshError::Issue(e),
        IssueErrorOrWyrd::Wyrd(e) => RefreshError::Wyrd(e),
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
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use secrecy::{ExposeSecret, SecretString};
    use sha2::{Digest, Sha256};
    use uuid::Uuid;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{Kid, PrincipalKindTag};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{insert_refresh_token, insert_service_account, refresh_by_hash};

    use wyrd_dev_fixtures::cards::seed_backing_card;

    use super::{RefreshError, RefreshTokens};
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
            space: SpaceName::new("prod").expect("static space is valid"),
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
            conn, sa_id, "service", card_ref, "test-sa", None, created_by,
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
            "SELECT COUNT(*) FROM wyrd.audit_token_exchange
              WHERE data_tenant_id = $1
                AND subject_principal_id = $2",
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

        let row: (String, String, i32) = sqlx::query_as(
            "SELECT mint_kind, result, scope_member_count
               FROM wyrd.audit_card_scope_mint
              WHERE data_tenant_id = $1
                AND principal_id = $2
              LIMIT 1",
        )
        .bind(tenant.as_uuid())
        .bind(sa_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("audit_card_scope_mint row exists");

        assert_eq!(row.0, "refresh", "mint_kind is refresh");
        assert_eq!(row.1, "success", "result is success");
        assert_eq!(row.2, 1, "single-card scope has member count 1");
    }
}
