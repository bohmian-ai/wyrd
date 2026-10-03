//! Non-blocking audit collaborator for rejected Oracle peer tickets.

use std::sync::Arc;

use vala_bifrost_redux::oracle::peer::{PeerSecurityAudit, PeerSecurityAuditError};
use vala_sql::audit_outbox::AuditOutbox;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDetail, AuditOutcome, BifrostSecurityPhase, BifrostSecurityViolationKind,
};

use crate::audit;
use crate::postgres::ServerPostgres;

/// Server-owned peer-security auditor staging on the process audit outbox.
#[derive(Clone)]
pub struct PostgresPeerSecurityAudit {
    /// The process audit outbox every rejection is staged on.
    audit: Arc<AuditOutbox>,
}

impl PostgresPeerSecurityAudit {
    /// Verifies the exact durable system sentinel before enabling peer auditing.
    ///
    /// The sentinel must exist because unverified rejections stage on the
    /// platform/system chain; `audit` is the process outbox they stage on.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when the operator capability is absent,
    /// the sentinel cannot be read, or any canonical attribute differs.
    pub async fn try_new(
        postgres: &ServerPostgres,
        audit: Arc<AuditOutbox>,
    ) -> Result<Self, PeerSecurityAuditError> {
        let operator = postgres.operator_pool().ok_or(PeerSecurityAuditError)?;
        let sentinel: Option<(
            String,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
        )> = sqlx::query_as(
            "SELECT slug, display_name, status, deleted_at
                   FROM platform.tenants
                  WHERE data_tenant_id = $1",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .fetch_optional(operator.pool())
        .await
        .map_err(|_| PeerSecurityAuditError)?;
        if sentinel
            != Some((
                "wyrd-system".to_owned(),
                "Wyrd System".to_owned(),
                "active".to_owned(),
                None,
            ))
        {
            return Err(PeerSecurityAuditError);
        }
        Ok(Self { audit })
    }

    /// Stages one scrubbed rejection under the selected trusted audit tenant.
    ///
    /// Returns immediately; the outbox commits the row in the background and
    /// counts a failed commit instead of reporting it.
    fn stage(&self, tenant_id: DataTenantId, violation: BifrostSecurityViolationKind) {
        let event = audit::audit_event_unauthenticated(
            RequestId::now_v7(),
            "bifrost.query.security_violation",
            "bifrost.oracle.peer",
            "bifrost:query:peer_execute",
            AuditOutcome::Denied,
        )
        .with_detail(AuditDetail::BifrostSecurityViolation {
            violation,
            phase: BifrostSecurityPhase::Peer,
            query_digest: None,
            // A rejected peer or tail presenter never became an authenticated
            // Wyrd caller, so there is no verified chain to attribute.
            delegation_chain: Vec::new(),
        });
        self.audit.stage(tenant_id, event);
    }
}

impl PeerSecurityAudit for PostgresPeerSecurityAudit {
    /// Stages an untrusted-ticket rejection on the platform/system audit chain.
    fn append_unverified_ticket_rejection(&self, violation: BifrostSecurityViolationKind) {
        self.stage(DataTenantId::SYSTEM_OWNER, violation);
    }

    /// Stages a signed-ticket rejection on the cryptographically verified tenant chain.
    fn append_verified_ticket_violation(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
    ) {
        self.stage(tenant_id, violation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;

    /// Production peer audit stages exact system and verified-tenant identities.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, the sentinel is rejected, the
    /// staged rejections do not commit, or a committed row is misattributed.
    #[tokio::test]
    async fn oracle_peer_postgres_audit_routes_security_identity_and_detail() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let postgres = ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        );
        let outbox = AuditOutbox::new(fixture.vala_postgres().clone());
        let writer = PostgresPeerSecurityAudit::try_new(&postgres, Arc::clone(&outbox))
            .await
            .expect("exact sentinel enables peer audit");
        writer.append_unverified_ticket_rejection(BifrostSecurityViolationKind::PeerUnknownKey);
        writer.append_verified_ticket_violation(
            fixture.data_tenant_id(),
            BifrostSecurityViolationKind::PeerAudience,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        assert_eq!(outbox.shutdown(deadline).await, 0, "both rejections commit");

        let pool = fixture.superuser_pool().await.expect("assertion pool");
        let rows: Vec<(uuid::Uuid, uuid::Uuid, String, String, String, String)> = sqlx::query_as(
            "SELECT data_tenant_id, principal_id, principal_kind, permission, \
                        outcome, detail \
                   FROM vala.audit_staging \
                  WHERE operation = 'bifrost.query.security_violation' \
                  ORDER BY data_tenant_id",
        )
        .fetch_all(&pool)
        .await
        .expect("audit rows");

        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert_eq!(row.1, PLATFORM_AUDIT_PRINCIPAL.as_uuid());
            assert_eq!(row.2, "service");
            assert_eq!(row.3, "bifrost:query:peer_execute");
            assert_eq!(row.4, "denied");
        }
        let system = rows
            .iter()
            .find(|row| row.0 == DataTenantId::SYSTEM_OWNER.as_uuid())
            .expect("system audit row");
        assert!(system.5.contains("\"violation\":\"peer_unknown_key\""));
        let tenant = rows
            .iter()
            .find(|row| row.0 == fixture.data_tenant_id().as_uuid())
            .expect("verified tenant audit row");
        assert!(tenant.5.contains("\"violation\":\"peer_audience\""));
    }

    /// Missing or incompatible system state prevents peer audit readiness.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, the sentinel cannot be changed,
    /// or construction succeeds without the exact sentinel.
    #[tokio::test]
    async fn oracle_peer_postgres_audit_rejects_missing_or_incompatible_sentinel() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let postgres = ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        );
        sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
            .execute(fixture.operator_pool().pool())
            .await
            .expect("remove sentinel");
        assert!(
            PostgresPeerSecurityAudit::try_new(
                &postgres,
                AuditOutbox::new(fixture.vala_postgres().clone())
            )
            .await
            .is_err(),
            "missing sentinel must prevent peer runtime construction"
        );

        sqlx::query(
            "INSERT INTO platform.tenants
                (data_tenant_id, slug, display_name, status)
             VALUES ($1, 'wyrd-system', 'Wrong System', 'active')",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .execute(fixture.operator_pool().pool())
        .await
        .expect("stage incompatible sentinel");
        assert!(
            PostgresPeerSecurityAudit::try_new(
                &postgres,
                AuditOutbox::new(fixture.vala_postgres().clone())
            )
            .await
            .is_err(),
            "incompatible sentinel must prevent peer runtime construction"
        );
    }
}
