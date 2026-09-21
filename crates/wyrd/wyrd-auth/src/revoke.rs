//! Principal revocation domain operations.

use std::fmt::Display;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;

use crate::exchange_api_key::principal_kind_wire;
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    revoke_refresh_family, service_account_by_id, suspend_service_account_principal,
    suspend_user_principal, user_by_id,
};

/// Revoke `target_id` in the table the caller's declared `kind` names.
///
/// Revocation suspends the principal, which every tenant issuance path reads
/// before it mints, so the principal's next token is refused on every grant;
/// tokens it already holds lapse at their five-minute expiry. A User also
/// carries refresh authority, so the User branch retires the refresh family on
/// the same [`TenantConn`] and the caller's commit retires both or neither.
///
/// The kind is a selector, not a hint: principal ids are unique only within
/// their table, so revoking on the caller's claim rather than on whichever
/// table happens to answer first keeps a `user` request from silently
/// retiring a service account that shares the id. A row found under a
/// different kind than the one declared is reported as not found, so a caller
/// cannot use the refusal to enumerate the other table. A miss writes nothing.
///
/// # Errors
/// Returns [`WyrdError::PrincipalNotFound`] when no principal of that kind
/// exists in the tenant, and [`WyrdError::Internal`] when the lookup or the
/// revocation write fails or the stored kind is unrecognized.
pub async fn revoke_principal_in_conn(
    conn: &mut TenantConn<'_>,
    target_id: PrincipalId,
    kind: PrincipalKindTag,
    tenant: DataTenantId,
) -> Result<(), WyrdError> {
    let id_uuid = target_id.as_uuid();

    if kind == PrincipalKindTag::User {
        if user_by_id(conn, id_uuid)
            .await
            .map_err(internal_error)?
            .is_none()
        {
            return Err(not_found(target_id, tenant));
        }
        suspend_user_principal(conn, id_uuid)
            .await
            .map_err(internal_error)?;
        revoke_refresh_family(conn, "user", id_uuid, "principal_revoked")
            .await
            .map_err(internal_error)?;
        return Ok(());
    }

    let Some(row) = service_account_by_id(conn, id_uuid)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found(target_id, tenant));
    };
    let stored = principal_kind_wire(&row.principal_kind).ok_or_else(|| {
        internal_error(format!(
            "principal {target_id} has unrecognized kind {}",
            row.principal_kind
        ))
    })?;
    if stored != kind {
        return Err(not_found(target_id, tenant));
    }
    suspend_service_account_principal(conn, id_uuid)
        .await
        .map_err(internal_error)?;
    Ok(())
}

/// Build the single non-enumerating refusal shared by every miss.
fn not_found(target_id: PrincipalId, tenant: DataTenantId) -> WyrdError {
    WyrdError::PrincipalNotFound {
        message: format!("principal {target_id} not found in tenant {tenant}"),
        details: serde_json::json!({ "id": target_id.to_string() }),
    }
}

/// Refuse a revocation without telling the caller what failed.
///
/// The cause names the store, the stored kind, or the statement that rejected
/// the write; the problem renderer publishes `message` verbatim, so the cause
/// goes to the trace and the public body carries one stable sentence.
fn internal_error(cause: impl Display) -> WyrdError {
    tracing::error!(cause = %cause, "principal revocation failed");
    WyrdError::Internal {
        message: "principal revocation failed".to_owned(),
        details: serde_json::Value::Null,
    }
}

#[cfg(test)]
mod pg_tests {
    use chrono::{Duration, Utc};
    use uuid::Uuid;
    use wyrd_dev_fixtures::cards::seed_backing_card;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::queries::auth::{
        insert_refresh_token, insert_service_account, insert_user, refresh_by_hash, service_account_by_id,
        user_by_id,
    };

    use super::revoke_principal_in_conn;

    fn make_service_card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("valid name"),
            version: VersionBlock::parse("1.0.0").expect("valid version"),
            space: Some(SpaceName::new("test").expect("valid space")),
            uid: None,
        }
    }

    /// A user is revocable only as a user, and the revocation suspends it.
    ///
    /// The requested kind selects the table the id is resolved in, so naming
    /// the wrong kind must miss rather than revoke a same-id row of another
    /// kind. Suspension is the half that refuses the user's next token.
    #[tokio::test]
    async fn a_user_is_revoked_under_the_user_kind() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let user_id = Uuid::new_v4();
        insert_user(&mut conn, user_id, Some("user@test.com"), "oidc", None)
            .await
            .expect("user inserts");

        revoke_principal_in_conn(
            &mut conn,
            PrincipalId::new(user_id),
            PrincipalKindTag::User,
            tenant,
        )
        .await
        .expect("revocation succeeds");

        let row = user_by_id(&mut conn, user_id)
            .await
            .expect("user lookup runs")
            .expect("the user still exists");
        assert_eq!(row.status, "suspended", "revocation suspends the user");
    }

    /// Revoking a User must retire the refresh authority in the same commit.
    ///
    /// Suspension already refuses a rotation, but a live refresh row would
    /// outlast a later reactivation and restore the revoked session. Both
    /// writes share the caller's transaction, so this asserts the committed
    /// state a served revoke leaves behind.
    ///
    /// # Panics
    /// Panics when the revocation fails, when a live refresh row survives, or
    /// when the user is not suspended.
    #[tokio::test]
    async fn revoking_a_user_retires_its_refresh_authority() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let user_id = Uuid::new_v4();
        let refresh_hash = format!("{user_id:x}-refresh");
        {
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            insert_user(&mut conn, user_id, Some("revoked@test.com"), "oidc", None)
                .await
                .expect("user inserts");
            insert_refresh_token(
                &mut conn,
                Uuid::new_v4(),
                "user",
                user_id,
                &refresh_hash,
                Utc::now() + Duration::days(30),
            )
            .await
            .expect("refresh row inserts");
            conn.commit().await.expect("seed commits");
        }

        {
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            revoke_principal_in_conn(
                &mut conn,
                PrincipalId::new(user_id),
                PrincipalKindTag::User,
                tenant,
            )
            .await
            .expect("revocation succeeds");
            conn.commit().await.expect("revocation commits");
        }

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = refresh_by_hash(&mut conn, &refresh_hash)
            .await
            .expect("refresh lookup runs")
            .expect("the refresh row still exists as history");
        assert_eq!(
            row.revoked_reason.as_deref(),
            Some("principal_revoked"),
            "the refresh row is retired by the revocation, not left live"
        );
        assert!(
            row.revoked_at.is_some(),
            "a retired refresh row carries its revocation time"
        );
        let user = user_by_id(&mut conn, user_id)
            .await
            .expect("user lookup runs")
            .expect("the user still exists");
        assert_eq!(
            user.status, "suspended",
            "the same revocation suspended the user"
        );
    }

    /// A service account is revocable only as a service.
    ///
    /// Same contract as the user case, from the machine side: an agent-kinded
    /// request must not reach a service row.
    #[tokio::test]
    async fn a_service_account_is_revoked_under_the_service_kind() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let creator = Uuid::new_v4();
        let sa_id = Uuid::new_v4();
        let card_ref = make_service_card_ref("svc-revoke-service");
        seed_backing_card(&mut conn, &card_ref, creator).await;
        insert_service_account(
            &mut conn,
            sa_id,
            "service",
            Some(&card_ref),
            "svc-revoke-service",
            None,
            creator,
        )
        .await
        .expect("service account inserts");

        revoke_principal_in_conn(
            &mut conn,
            PrincipalId::new(sa_id),
            PrincipalKindTag::Service,
            tenant,
        )
        .await
        .expect("revocation succeeds");

        assert!(
            service_account_by_id(&mut conn, sa_id)
                .await
                .expect("service account lookup runs")
                .is_none(),
            "revocation suspends the service account, so no active row remains"
        );
    }

    /// A declared kind that does not match the stored row must refuse exactly
    /// like an unknown id, so the refusal cannot be read as "wrong table".
    #[tokio::test]
    async fn a_mismatched_kind_is_refused_as_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let creator = Uuid::new_v4();
        let sa_id = Uuid::new_v4();
        let card_ref = make_service_card_ref("svc-revoke-mismatch");
        seed_backing_card(&mut conn, &card_ref, creator).await;
        insert_service_account(
            &mut conn,
            sa_id,
            "service",
            Some(&card_ref),
            "svc-revoke-mismatch",
            None,
            creator,
        )
        .await
        .expect("service account inserts");

        let result = revoke_principal_in_conn(
            &mut conn,
            PrincipalId::new(sa_id),
            PrincipalKindTag::Agent,
            tenant,
        )
        .await;

        assert!(
            matches!(result, Err(WyrdError::PrincipalNotFound { .. })),
            "a service account must not be revocable as an agent, got: {result:?}"
        );
    }

    /// An agent account is revocable only as an agent.
    ///
    /// The third arm of the kind separation, and the one that also checks the
    /// refusal message names nothing the caller did not already send.
    #[tokio::test]
    async fn an_agent_account_is_revoked_under_the_agent_kind() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let creator = Uuid::new_v4();
        let sa_id = Uuid::new_v4();
        let card_ref = CardRef {
            kind: CardKind::Agent,
            name: CardName::new("agent-revoke-test").expect("valid name"),
            version: VersionBlock::parse("1.0.0").expect("valid version"),
            space: Some(SpaceName::new("test").expect("valid space")),
            uid: None,
        };
        seed_backing_card(&mut conn, &card_ref, creator).await;
        insert_service_account(
            &mut conn,
            sa_id,
            "agent",
            Some(&card_ref),
            "agent-revoke-test",
            None,
            creator,
        )
        .await
        .expect("agent account inserts");

        revoke_principal_in_conn(
            &mut conn,
            PrincipalId::new(sa_id),
            PrincipalKindTag::Agent,
            tenant,
        )
        .await
        .expect("revocation succeeds");
    }

    /// A failed revocation write says so without saying how.
    ///
    /// `WyrdError::Internal.message` is published verbatim by the problem
    /// renderer, so a SQL error placed there would hand a caller the table, the
    /// role, and the statement that refused. The cause belongs in the trace.
    #[tokio::test]
    async fn a_failed_revocation_write_keeps_its_cause_server_side() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let creator = Uuid::new_v4();
        let sa_id = Uuid::new_v4();
        let card_ref = make_service_card_ref("svc-revoke-denied");
        {
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            seed_backing_card(&mut conn, &card_ref, creator).await;
            insert_service_account(
                &mut conn,
                sa_id,
                "service",
                Some(&card_ref),
                "svc-revoke-denied",
                None,
                creator,
            )
            .await
            .expect("service account inserts");
            conn.commit().await.expect("seed commits");
        }

        // Remove the runtime role's write privilege so the revocation statement
        // fails exactly as an unavailable or misconfigured store would.
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("REVOKE UPDATE ON wyrd.auth_service_accounts FROM wyrd_app")
            .execute(&admin)
            .await
            .expect("privilege revoked");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let result = revoke_principal_in_conn(
            &mut conn,
            PrincipalId::new(sa_id),
            PrincipalKindTag::Service,
            tenant,
        )
        .await;
        drop(conn);

        sqlx::query("GRANT UPDATE ON wyrd.auth_service_accounts TO wyrd_app")
            .execute(&admin)
            .await
            .expect("privilege restored");

        let Err(WyrdError::Internal { message, .. }) = result else {
            panic!("a refused write must surface as an internal failure, got: {result:?}");
        };
        assert_eq!(message, "principal revocation failed");
        for leaked in ["auth_service_accounts", "permission", "wyrd_app", "UPDATE"] {
            assert!(
                !message.contains(leaked),
                "the public message must not name {leaked}: {message}"
            );
        }
    }

    /// An id that exists in no table is a not-found, not a silent success.
    ///
    /// Revocation reports zero rows as a refusal so an operator never reads a
    /// typo as a completed revocation.
    #[tokio::test]
    async fn an_unknown_principal_is_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let unknown_id = PrincipalId::new(Uuid::new_v4());
        let result =
            revoke_principal_in_conn(&mut conn, unknown_id, PrincipalKindTag::Service, tenant)
                .await;

        assert!(
            matches!(result, Err(WyrdError::PrincipalNotFound { .. })),
            "unknown principal should return PrincipalNotFound, got: {result:?}"
        );
    }
}
