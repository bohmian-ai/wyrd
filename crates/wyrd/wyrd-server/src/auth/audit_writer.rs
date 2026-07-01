//! Authz-check audit writer seam.

use async_trait::async_trait;
use wyrd_auth_check::AuthzCheckContext;
use wyrd_spec::card::policy::PolicyDecision;
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;

/// Audit writer used by the authz-check route.
#[async_trait]
pub trait AuthzAuditWriter: Send + Sync {
    /// Write one authz-check audit fact.
    async fn write_authz_check(
        &self,
        conn: &mut TenantConn<'_>,
        ctx: &AuthzCheckContext,
        decision: &PolicyDecision,
    ) -> Result<(), WyrdError>;

    /// Whether this writer is a placeholder default unsuitable for production.
    fn is_stub_default(&self) -> bool {
        false
    }
}

/// Placeholder authz-check audit writer.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAuthzAuditWriter;

#[async_trait]
impl AuthzAuditWriter for NoopAuthzAuditWriter {
    async fn write_authz_check(
        &self,
        _: &mut TenantConn<'_>,
        _: &AuthzCheckContext,
        _: &PolicyDecision,
    ) -> Result<(), WyrdError> {
        Ok(())
    }

    fn is_stub_default(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use wyrd_auth_check::{AuthzCheckContext, AuthzCheckRequest};
    use wyrd_auth_verify::VerifiedToken;
    use wyrd_runtime::{
        DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind, PrincipalRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::policy::PolicyDecision;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;

    use super::{AuthzAuditWriter, NoopAuthzAuditWriter};

    #[tokio::test]
    async fn noop_returns_ok() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        NoopAuthzAuditWriter
            .write_authz_check(&mut conn, &context(), &PolicyDecision::Allow)
            .await
            .expect("noop writer succeeds");
    }

    fn context() -> AuthzCheckContext {
        let tenant = DataTenantId::new_v7();
        let caller = principal("caller", tenant);
        let callee = principal("callee", tenant);
        let verified = VerifiedToken {
            principal: callee,
            delegation_chain: vec![DelegationStep {
                principal: PrincipalRef::from_principal(&caller),
            }],
            exp: chrono::Utc::now(),
            iat: chrono::Utc::now(),
        };

        AuthzCheckContext::from_verified(
            &verified,
            AuthzCheckRequest {
                target: card_ref("callee"),
                action: "card_write".to_owned(),
                context: serde_json::json!({}),
            },
            None,
            RequestId::parse(&uuid::Uuid::now_v7().to_string()).expect("generated UUIDv7 is valid"),
        )
        .expect("context builds")
    }

    fn principal(name: &str, tenant_id: DataTenantId) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref(name),
            },
            tenant_id,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            card_scope: wyrd_runtime::CardScope::default(),
        }
    }

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }
}
