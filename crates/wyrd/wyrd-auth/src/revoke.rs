//! Principal revocation domain operations.

use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    revoke_service_account_principal, revoke_user_principal, service_account_by_id, user_by_id,
};

/// Look up `target_id`, write the revocation timestamp, and return the principal kind.
///
/// Returns `PrincipalNotFound` when `target_id` does not exist in the tenant.
///
/// # Errors
/// Returns a Wyrd error when the target principal is not found or the revocation
/// write fails.
pub async fn revoke_principal_in_conn(
    conn: &mut TenantConn<'_>,
    target_id: PrincipalId,
    tenant: DataTenantId,
) -> Result<PrincipalKindTag, WyrdError> {
    let id_uuid = target_id.as_uuid();

    if user_by_id(conn, id_uuid).await.ok().flatten().is_some() {
        revoke_user_principal(conn, id_uuid)
            .await
            .map_err(internal_error)?;
        return Ok(PrincipalKindTag::User);
    }

    if let Some(row) = service_account_by_id(conn, id_uuid).await.ok().flatten() {
        revoke_service_account_principal(conn, id_uuid)
            .await
            .map_err(internal_error)?;
        let kind = if row.principal_kind == "agent" {
            PrincipalKindTag::Agent
        } else {
            PrincipalKindTag::Service
        };
        return Ok(kind);
    }

    Err(WyrdError::PrincipalNotFound {
        message: format!("principal {target_id} not found in tenant {tenant}"),
        details: serde_json::json!({ "id": target_id.to_string() }),
    })
}

fn internal_error(error: impl std::fmt::Display) -> WyrdError {
    WyrdError::Internal {
        message: error.to_string(),
        details: serde_json::Value::Null,
    }
}

#[cfg(test)]
mod pg_tests {
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
    use wyrd_sql::queries::auth::{insert_service_account, insert_user};

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

    #[tokio::test]
    async fn user_revocation_returns_user_kind() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let user_id = Uuid::new_v4();
        insert_user(&mut conn, user_id, Some("user@test.com"), "oidc", None)
            .await
            .expect("user inserts");

        let kind = revoke_principal_in_conn(&mut conn, PrincipalId::new(user_id), tenant)
            .await
            .expect("revocation succeeds");

        assert_eq!(kind, PrincipalKindTag::User);
    }

    #[tokio::test]
    async fn service_account_revocation_returns_service_kind() {
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

        let kind = revoke_principal_in_conn(&mut conn, PrincipalId::new(sa_id), tenant)
            .await
            .expect("revocation succeeds");

        assert_eq!(kind, PrincipalKindTag::Service);
    }

    #[tokio::test]
    async fn agent_service_account_revocation_returns_agent_kind() {
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

        let kind = revoke_principal_in_conn(&mut conn, PrincipalId::new(sa_id), tenant)
            .await
            .expect("revocation succeeds");

        assert_eq!(kind, PrincipalKindTag::Agent);
    }

    #[tokio::test]
    async fn unknown_principal_returns_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let unknown_id = PrincipalId::new(Uuid::new_v4());
        let result = revoke_principal_in_conn(&mut conn, unknown_id, tenant).await;

        assert!(
            matches!(result, Err(WyrdError::PrincipalNotFound { .. })),
            "unknown principal should return PrincipalNotFound, got: {result:?}"
        );
    }
}
