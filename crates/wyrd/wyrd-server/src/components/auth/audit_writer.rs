//! Authz-check audit writer seam.

use async_trait::async_trait;
use vala_sql::queries::audit_staging::append_audit;
use wyrd_auth_check::AuthzCheckContext;
use wyrd_spec::card::policy::PolicyDecision;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_sql::TenantConn;

/// Audit writer used by the authz-check route.
#[async_trait]
pub trait AuthzAuditWriter: Send + Sync {
    /// Append the authorization decision to `vala.audit_staging` on the
    /// caller's own transaction, so the decision and whatever it authorized
    /// commit or roll back together.
    ///
    /// # Errors
    /// Returns [`WyrdError`] when the canonical append fails, which fails the
    /// decision closed rather than serving an unaudited allowance.
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

/// Production authz-check audit writer that appends to `vala.audit_staging`.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealAuthzAuditWriter;

#[async_trait]
impl AuthzAuditWriter for RealAuthzAuditWriter {
    async fn write_authz_check(
        &self,
        conn: &mut TenantConn<'_>,
        ctx: &AuthzCheckContext,
        decision: &PolicyDecision,
    ) -> Result<(), WyrdError> {
        let outcome = match decision {
            PolicyDecision::Allow => AuditOutcome::Allowed,
            _ => AuditOutcome::Denied,
        };
        let event = AuditEvent {
            request_id: ctx.request_id.clone(),
            trace_id: None,
            operation: "authz.check".to_owned(),
            resource: ctx.request.target.to_string(),
            card_ref: ctx.caller.card_ref().cloned(),
            principal_id: ctx.caller.id,
            principal_kind: ctx.caller.kind.tag(),
            // The row is attributed to the immediate delegator, and a delegated
            // token is minted from a token rather than from a credential, so
            // there is no credential of that principal's to name here.
            credential_id: None,
            permission: ctx.request.action.clone(),
            outcome,
            detail: None,
        };
        append_audit(conn, &event)
            .await
            .map(|_| ())
            .map_err(crate::audit::audit_unavailable)
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
mod pg_tests {
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
    use wyrd_spec::reference::{CardRef, CardRefScope};
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
        let card_ref = card_ref(name);
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        }
    }

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }
}
