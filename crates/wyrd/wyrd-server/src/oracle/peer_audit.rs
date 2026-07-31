//! Durable standard-outbox audit collaborator for rejected Oracle peer tickets.

use async_trait::async_trait;
use vala_bifrost_redux::oracle::peer::{PeerSecurityAudit, PeerSecurityAuditError};
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditResult, BifrostSecurityPhase, BifrostSecurityViolationKind,
};

use crate::audit;
use crate::postgres::ServerPostgres;

/// Server-owned peer-security writer using the canonical tenant audit outbox.
#[derive(Clone)]
pub struct PostgresPeerSecurityAudit {
    /// Runtime-ready Postgres owner used to acquire tenant-scoped transactions.
    postgres: ServerPostgres,
}

impl PostgresPeerSecurityAudit {
    /// Verifies the exact durable system sentinel before enabling peer auditing.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when the operator capability is absent,
    /// the sentinel cannot be read, or any canonical attribute differs.
    pub async fn try_new(postgres: &ServerPostgres) -> Result<Self, PeerSecurityAuditError> {
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
        Ok(Self {
            postgres: postgres.clone(),
        })
    }

    /// Commits one scrubbed rejection under the selected trusted audit tenant.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when acquire, append, or commit fails.
    async fn commit(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        let event = audit::audit_event_unauthenticated(
            RequestId::now_v7(),
            "bifrost.query.security_violation",
            "bifrost.oracle.peer",
            "bifrost:query:peer_execute",
            AuditDecision::Deny,
            AuditResult::Failure,
            "scrubbed Oracle peer security rejection",
        )
        .with_detail(AuditDetail::BifrostSecurityViolation {
            violation,
            phase: BifrostSecurityPhase::Peer,
            query_digest: None,
        });
        let mut conn = self
            .postgres
            .tenant_conn(tenant_id)
            .await
            .map_err(|_| PeerSecurityAuditError)?;
        audit::append_on(&mut conn, &event)
            .await
            .map_err(|_| PeerSecurityAuditError)?;
        conn.commit().await.map_err(|_| PeerSecurityAuditError)
    }
}

#[async_trait]
impl PeerSecurityAudit for PostgresPeerSecurityAudit {
    /// Commits an untrusted-ticket rejection to the platform/system audit chain.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when the system-chain row cannot commit.
    async fn append_unverified_ticket_rejection(
        &self,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        self.commit(DataTenantId::SYSTEM_OWNER, violation).await
    }

    /// Commits a signed-ticket rejection to the cryptographically verified tenant chain.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when the tenant-chain row cannot commit.
    async fn append_verified_ticket_violation(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        self.commit(tenant_id, violation).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;

    /// Production peer audit writes exact system and verified-tenant identities.
    #[tokio::test]
    async fn oracle_peer_postgres_audit_routes_security_identity_and_detail() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let postgres = ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        );
        let writer = PostgresPeerSecurityAudit::try_new(&postgres)
            .await
            .expect("exact sentinel enables peer audit");
        writer
            .append_unverified_ticket_rejection(BifrostSecurityViolationKind::PeerUnknownKey)
            .await
            .expect("system audit commits");
        writer
            .append_verified_ticket_violation(
                fixture.data_tenant_id(),
                BifrostSecurityViolationKind::PeerAudience,
            )
            .await
            .expect("tenant audit commits");

        let pool = fixture.superuser_pool().await.expect("assertion pool");
        let rows: Vec<(
            uuid::Uuid,
            uuid::Uuid,
            String,
            String,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT data_tenant_id, principal_id, principal_kind, auth_method, \
                        decision, result, detail \
                   FROM vala.audit_outbox \
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
            assert_eq!(row.3, "internal");
            assert_eq!(row.4, "deny");
            assert_eq!(row.5, "failure");
        }
        let system = rows
            .iter()
            .find(|row| row.0 == DataTenantId::SYSTEM_OWNER.as_uuid())
            .expect("system audit row");
        assert!(system.6.contains("\"violation\":\"peer_unknown_key\""));
        let tenant = rows
            .iter()
            .find(|row| row.0 == fixture.data_tenant_id().as_uuid())
            .expect("verified tenant audit row");
        assert!(tenant.6.contains("\"violation\":\"peer_audience\""));
    }

    /// Missing or incompatible system state prevents peer audit readiness.
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
            PostgresPeerSecurityAudit::try_new(&postgres).await.is_err(),
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
            PostgresPeerSecurityAudit::try_new(&postgres).await.is_err(),
            "incompatible sentinel must prevent peer runtime construction"
        );
    }
}
