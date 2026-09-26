//! Refresh-token grant: single-use rotation with reuse detection (F07/F08).

use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wyrd_auth_issue::IssueError;
use wyrd_auth_verify::RefreshTokenClaims;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    consume_active_refresh, lock_refresh_family, refresh_by_hash, revoke_refresh_family,
};
use wyrd_sql::row_types::auth::HumanConnectionBinding;

use crate::audit::{
    REFRESH_FAMILY_REVOKE_OPERATION, append_auth_audit, auth_event, principal_kind_tag,
};
use crate::exchange_api_key::token_hash;
use crate::issuance::{ExchangedToken, IssuanceError, TenantTokenIssuer};

/// Refresh-token rotation service.
///
/// Owns only the single-use rotation and reuse containment; the successor
/// session is minted by the shared [`TenantTokenIssuer`], so a suspended user
/// or withdrawn grant governs a renewal exactly as it governs a first login.
#[derive(Debug, Clone)]
pub struct RefreshTokens {
    /// The tenant issuance owner that mints the successor session.
    pub issuer: TenantTokenIssuer,
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
    /// The shared issuance owner refused or failed to mint the successor.
    #[error("token issuance error")]
    Issuance(#[from] IssuanceError),
    /// Wyrd contract error.
    #[error("wyrd error")]
    Wyrd(#[from] WyrdError),
}

impl From<RefreshError> for WyrdError {
    fn from(error: RefreshError) -> Self {
        match error {
            RefreshError::Reused => WyrdError::RefreshReused {
                message: "Refresh token family revoked due to reuse of a rotated token".to_owned(),
                details: json!({}),
            },
            // A suspended user, a tenant that stopped admitting credentials,
            // or a login connection that is no longer Active ends the session
            // the same way a revoked refresh row does.
            RefreshError::NotFound
            | RefreshError::Issuance(
                IssuanceError::PrincipalInactive
                | IssuanceError::TenantNotAdmitting
                | IssuanceError::ConnectionInactive,
            ) => WyrdError::RefreshRevoked {
                message: "Refresh token not found, expired, or revoked".to_owned(),
                details: json!({}),
            },
            RefreshError::Database(_) => WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            },
            RefreshError::Issuance(error) => error.into(),
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
    /// 2. `refresh_by_hash` — resolve the stored row only far enough to name
    ///    its principal family; no row → return `NotFound`.
    /// 3. `lock_refresh_family` — serialize every refresh operation for that
    ///    family until the caller's commit, before the connection slot lock
    ///    issuance takes. A replay of an ancestor therefore classifies and
    ///    revokes only after a concurrent rotation of the current row has
    ///    committed or rolled back, so its successor cannot escape containment.
    /// 4. `consume_active_refresh` — atomic `UPDATE … RETURNING` under the lock.
    ///    - Active → mint successor pair, insert with `rotated_from`, audit, return OK.
    ///    - Stale → reuse detected; family revoked, audit, return Reused.
    ///
    /// # Errors
    /// Returns [`RefreshError::NotFound`] when no row matches the presented
    /// token, [`RefreshError::Reused`] when a stale row is presented — the
    /// family is revoked and the containment audited before returning — and a
    /// store or issuance error when the successor cannot be minted. A machine
    /// refresh row cannot rotate and is refused, and a human row whose bound
    /// connection revision is missing or no longer Active is refused with
    /// [`IssuanceError::ConnectionInactive`]. The consumed row is only
    /// retired if the caller commits, so a refused rotation leaves nothing
    /// written.
    #[tracing::instrument(level = "debug", skip(self, conn, presented), err)]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        presented: SecretString,
        request_id: &str,
    ) -> Result<ExchangedToken, RefreshError> {
        let hash = token_hash(presented.expose_secret());
        let Some(stored) = refresh_by_hash(conn, &hash).await? else {
            tracing::debug!("refresh token not found for presented hash");
            return Err(RefreshError::NotFound);
        };
        lock_refresh_family(conn, &stored.principal_kind, stored.principal_id).await?;

        let Some(active) = consume_active_refresh(conn, &hash).await? else {
            // The presented row exists but is no longer active: it was
            // already rotated, revoked, or expired. Revoke the entire
            // principal's token family as a theft response. The family lock
            // makes this statement see every successor a concurrent
            // rotation committed.
            let revoked = revoke_refresh_family(
                conn,
                &stored.principal_kind,
                stored.principal_id,
                "reuse_detected",
            )
            .await?;

            // Audit the family revocation (F08) as a refused grant.
            let owner = PrincipalId::new(stored.principal_id);
            let owner_kind = principal_kind_tag(&stored.principal_kind);
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
            )
            // The containment record names the row that was replayed.
            // Successful rotation already attributes its consumed
            // predecessor this way; without it the one event that
            // reports a theft is the only one that cannot say which of
            // a principal's refresh rows was presented.
            .with_credential_id(Some(stored.id));
            append_auth_audit(conn, &event).await?;

            tracing::warn!(
                principal_id = %stored.principal_id,
                principal_kind = %stored.principal_kind,
                revoked_family_rows = revoked,
                "refresh token reuse detected; family revoked"
            );

            return Err(RefreshError::Reused);
        };

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
            return Err(RefreshError::Issuance(IssuanceError::Issue(
                IssueError::InvalidPrincipalKind,
            )));
        }

        // A human family belongs to the exact connection revision it
        // logged in through; a row carrying no binding predates that
        // provenance and has no connection that could still admit it.
        let (Some(connection_id), Some(connection_revision)) =
            (active.human_connection_id, active.human_connection_revision)
        else {
            tracing::warn!(
                principal_id = %principal_id,
                "refresh rotation refused for a family with no login connection"
            );
            return Err(RefreshError::Issuance(IssuanceError::ConnectionInactive));
        };
        let connection = HumanConnectionBinding {
            connection_id,
            connection_revision,
        };

        // The successor is minted from the user's current status and
        // grants, not the consumed token's, so a suspension or revoked
        // role does not survive a renewal, and it inherits the family's
        // connection binding, so a replaced, deactivated, or removed
        // connection ends the family at its next rotation.
        let exchanged = self
            .issuer
            .issue_human_session(conn, principal_id, Some(active.id), connection, request_id)
            .await?;

        tracing::debug!(
            principal_id = %principal_id,
            rotated_from = %active.id,
            "human refresh token rotated"
        );

        Ok(exchanged)
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
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_runtime::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{
        insert_human_refresh_token, insert_refresh_token, insert_role, insert_service_account,
        list_user_roles, refresh_by_hash, replace_user_roles,
    };

    use wyrd_dev_fixtures::cards::seed_backing_card;

    use super::{RefreshError, RefreshTokens};
    use crate::audit::REFRESH_FAMILY_REVOKE_OPERATION;
    use crate::issuance::{IssuanceError, TenantTokenIssuer, TokenExchangeSettings};

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
            issuer: TenantTokenIssuer::new(test_issuing_key(), TokenExchangeSettings::default()),
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
                Utc::now(),
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

    /// Seed one active refresh row and return its durable id.
    ///
    /// The id is the value audit attributes a consumed or replayed row to, so
    /// the tests that assert attribution need it rather than a fresh UUID. A
    /// `user` row is bound to the tenant's Active human connection, seeding
    /// one when absent, exactly as a real login binds it; any other kind is
    /// written unbound.
    async fn seed_active_refresh(
        conn: &mut TenantConn<'_>,
        principal_kind: &str,
        principal_id: Uuid,
        token_hash: &str,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::days(30);
        if principal_kind == "user" {
            let binding = seed_active_human_connection(conn)
                .await
                .expect("connection seeds");
            insert_human_refresh_token(
                conn,
                id,
                principal_id,
                token_hash,
                expires_at,
                None,
                binding,
            )
            .await
            .expect("human refresh token inserts");
        } else {
            insert_refresh_token(
                conn,
                id,
                principal_kind,
                principal_id,
                token_hash,
                expires_at,
            )
            .await
            .expect("refresh token inserts");
        }
        id
    }

    /// A rotated successor inherits its family's exact connection binding.
    #[tokio::test]
    async fn rotation_copies_the_connection_binding() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;
        let refresh_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let original_hash = hash_of(&refresh_jwt);
        seed_active_refresh(&mut conn, "user", user_id, &original_hash).await;
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection reads");

        let exchanged = refresh_service()
            .execute(&mut conn, refresh_jwt, "req-binding")
            .await
            .expect("rotation succeeds");

        let new_hash = hash_of(exchanged.refresh_token.as_ref().expect("refresh token"));
        let new_row = refresh_by_hash(&mut conn, &new_hash)
            .await
            .expect("lookup")
            .expect("new row exists");
        assert_eq!(new_row.human_connection_id, Some(binding.connection_id));
        assert_eq!(
            new_row.human_connection_revision,
            Some(binding.connection_revision)
        );
    }

    /// A family whose connection is no longer Active, or which carries no
    /// binding at all, is refused without writing a successor.
    #[tokio::test]
    async fn rotation_refuses_an_inactive_or_unbound_connection() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = insert_test_user(&mut conn, tenant).await;

        let unbound_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        insert_refresh_token(
            &mut conn,
            Uuid::new_v4(),
            "user",
            user_id,
            &hash_of(&unbound_jwt),
            Utc::now() + Duration::days(30),
        )
        .await
        .expect("unbound row inserts");
        let unbound = refresh_service()
            .execute(&mut conn, unbound_jwt, "req-unbound")
            .await;
        assert!(
            matches!(
                unbound,
                Err(RefreshError::Issuance(IssuanceError::ConnectionInactive))
            ),
            "an unbound family must refuse, got {unbound:?}"
        );

        let bound_jwt = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        seed_active_refresh(&mut conn, "user", user_id, &hash_of(&bound_jwt)).await;
        sqlx::query("UPDATE wyrd.auth_human_connections SET state = 'Inactive'")
            .execute(&mut **conn.transaction())
            .await
            .expect("connection deactivates");
        let result = refresh_service()
            .execute(&mut conn, bound_jwt, "req-inactive")
            .await;
        assert!(
            matches!(
                result,
                Err(RefreshError::Issuance(IssuanceError::ConnectionInactive))
            ),
            "a deactivated connection must refuse, got {result:?}"
        );
        let user_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.auth_refresh_tokens WHERE rotated_from IS NOT NULL",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("successor count reads");
        assert_eq!(
            user_rows, 0,
            "no successor is written for a refused rotation"
        );
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

        // The signed successor and its durable row are derived from one
        // whole-second PostgreSQL issuance instant, so the `exp` the verifier
        // enforces and the expiry PostgreSQL enforces are the same instant.
        let successor_claims = super::claims_from_refresh_jwt(
            exchanged
                .refresh_token
                .as_ref()
                .expect("rotation issues a refresh token")
                .expose_secret(),
        )
        .expect("successor refresh jwt decodes");
        assert_eq!(
            new_row.expires_at.timestamp_subsec_nanos(),
            0,
            "the stored successor expiry is a whole second"
        );
        assert_eq!(
            i64::try_from(successor_claims.exp).expect("exp fits in i64"),
            new_row.expires_at.timestamp(),
            "signed exp equals the stored successor row expiry"
        );
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
        let consumed_id =
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

        // The containment row names the refresh row that was actually
        // presented. Without it the only event that reports a theft cannot say
        // which of the principal's rows the attacker held.
        let attributed: Option<Uuid> = sqlx::query_scalar(
            "SELECT credential_id FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = $2
                AND principal_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(REFRESH_FAMILY_REVOKE_OPERATION)
        .bind(user_id)
        .fetch_one(&mut **conn_c.transaction())
        .await
        .expect("credential attribution query runs");
        assert_eq!(
            attributed,
            Some(consumed_id),
            "the replay record names the consumed refresh row"
        );

        let successor_replay = refresh_service()
            .execute(&mut conn_c, successor, "req-successor")
            .await;
        assert!(
            matches!(successor_replay, Err(RefreshError::Reused)),
            "the successor cannot rotate: {successor_replay:?}"
        );
    }

    /// Replaying ancestor `A` while current token `B` rotates still revokes
    /// `B`'s successor `C` once both transactions commit.
    ///
    /// The rotation of `B` is held open until the replay's backend is observed
    /// waiting on a lock, so the replay's containment must be decided after
    /// the rotation commits. Every assertion reads committed state from a
    /// fresh transaction: `C` is revoked, cannot rotate, and exactly one
    /// containment audit names the replayed row `A`.
    #[tokio::test]
    async fn ancestor_replay_overlapping_rotation_revokes_successor() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = test_issuing_key();
        let service = refresh_service();

        let mut setup = fixture.tenant_conn().await.expect("setup conn opens");
        let user_id = insert_test_user(&mut setup, tenant).await;
        let ancestor = issue_refresh_jwt(&key, PrincipalKindTag::User, user_id, tenant);
        let ancestor_id =
            seed_active_refresh(&mut setup, "user", user_id, &hash_of(&ancestor)).await;
        let current = service
            .execute(
                &mut setup,
                SecretString::from(ancestor.expose_secret().to_owned()),
                "req-rotate-a",
            )
            .await
            .expect("A rotates to B")
            .refresh_token
            .expect("rotation issues B");
        setup.commit().await.expect("setup commits");

        // The legitimate rotation of B, held open with C written.
        let mut rotating = fixture.tenant_conn().await.expect("rotating conn opens");
        let successor = service
            .execute(&mut rotating, current, "req-rotate-b")
            .await
            .expect("B rotates to C")
            .refresh_token
            .expect("rotation issues C");

        let mut replaying = fixture.tenant_conn().await.expect("replay conn opens");
        let replay_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut **replaying.transaction())
            .await
            .expect("replay pid reads");
        let (replay, ()) = tokio::join!(
            service.execute(&mut replaying, ancestor, "req-replay-a"),
            async {
                tokio::time::timeout(std::time::Duration::from_secs(30), async {
                    loop {
                        let waiting: bool = sqlx::query_scalar(
                            "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE pid = $1 AND NOT granted)",
                        )
                        .bind(replay_pid)
                        .fetch_one(fixture.app_pool())
                        .await
                        .expect("replay lock state reads");
                        if waiting {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("the replay waits on the open rotation");
                rotating.commit().await.expect("rotation commits");
            }
        );
        assert!(
            matches!(replay, Err(RefreshError::Reused)),
            "replaying A is refused: {replay:?}"
        );
        replaying.commit().await.expect("the route commits Reused");

        let mut fresh = fixture.tenant_conn().await.expect("fresh conn opens");
        let successor_row = refresh_by_hash(&mut fresh, &hash_of(&successor))
            .await
            .expect("lookup")
            .expect("C exists");
        assert_eq!(
            successor_row.revoked_reason.as_deref(),
            Some("reuse_detected"),
            "C is revoked in committed state"
        );
        let containments: Vec<Option<Uuid>> = sqlx::query_scalar(
            "SELECT credential_id FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND operation = $2 AND principal_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(REFRESH_FAMILY_REVOKE_OPERATION)
        .bind(user_id)
        .fetch_all(&mut **fresh.transaction())
        .await
        .expect("audit query runs");
        assert_eq!(
            containments,
            vec![Some(ancestor_id)],
            "exactly one containment audit names the replayed A"
        );
        let successor_rotation = service.execute(&mut fresh, successor, "req-rotate-c").await;
        assert!(
            matches!(successor_rotation, Err(RefreshError::Reused)),
            "C cannot rotate: {successor_rotation:?}"
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
                Err(RefreshError::Issuance(IssuanceError::Issue(
                    IssueError::InvalidPrincipalKind
                )))
            ),
            "a machine refresh row is refused: {result:?}"
        );
    }

    /// The unverified payload read exposes the three routing claims intact.
    ///
    /// Rotation reads tenant, principal, and kind out of the presented token
    /// before it can verify anything — the tenant is what selects the verifier
    /// — so this decode has to be exact even though it grants nothing.
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
                Utc::now(),
                Duration::days(30),
            )
            .expect("refresh jwt issues");

        let claims = super::claims_from_refresh_jwt(&jwt).expect("claims decode");

        assert_eq!(claims.tenant_id, tenant_id);
        assert_eq!(claims.principal_id.as_uuid(), principal_id);
        assert_eq!(claims.principal_kind, PrincipalKindTag::Service);
    }

    /// A token that is not a three-part JWT is refused, not guessed at.
    #[test]
    fn claims_from_refresh_jwt_rejects_malformed_input() {
        assert!(super::claims_from_refresh_jwt("not.a.jwt").is_err());
        assert!(super::claims_from_refresh_jwt("onlyone").is_err());
    }

    /// The tenant-only read agrees with the full claims read.
    ///
    /// Routing to a tenant verifier needs just the tenant, and that narrower
    /// path must not diverge from the one rotation itself uses.
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
                Utc::now(),
                Duration::days(30),
            )
            .expect("refresh jwt issues");

        let extracted = super::tenant_from_refresh_jwt(&jwt).expect("tenant extracted");

        assert_eq!(extracted, tenant_id);
    }
}
