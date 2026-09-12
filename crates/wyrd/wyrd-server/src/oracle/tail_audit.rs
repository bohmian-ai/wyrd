//! Durable audit collaborator for rejected private Scribe-tail tickets.

use crate::audit;
use crate::postgres::ServerPostgres;
use async_trait::async_trait;
use vala_bifrost_redux::scribe::tail_rpc::{TailReadError, TailSecurityAudit};
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDetail, AuditOutcome, BifrostSecurityPhase, BifrostSecurityViolationKind,
};

/// Audit-staging writer for tail-specific security violations.
#[derive(Clone)]
pub struct PostgresTailSecurityAudit {
    postgres: ServerPostgres,
}

impl PostgresTailSecurityAudit {
    /// Validates the system sentinel before tail RPCs can be exposed.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when the operator capability or sentinel is unavailable.
    pub async fn try_new(postgres: &ServerPostgres) -> Result<Self, TailReadError> {
        let operator = postgres
            .operator_pool()
            .ok_or_else(|| TailReadError::Authorization {
                detail: "tail audit operator capability is unavailable".to_owned(),
            })?;
        let sentinel: Option<(String, String, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as("SELECT slug, display_name, status, deleted_at FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
            .fetch_optional(operator.pool())
            .await
            .map_err(|_| TailReadError::Authorization { detail: "tail audit sentinel lookup failed".to_owned() })?;
        if sentinel
            != Some((
                "wyrd-system".to_owned(),
                "Wyrd System".to_owned(),
                "active".to_owned(),
                None,
            ))
        {
            return Err(TailReadError::Authorization {
                detail: "tail audit sentinel is incompatible".to_owned(),
            });
        }
        Ok(Self {
            postgres: postgres.clone(),
        })
    }

    async fn commit(&self, tenant_id: DataTenantId, reason: &str) -> Result<(), TailReadError> {
        let violation = match reason {
            "audience" => BifrostSecurityViolationKind::TailAudience,
            "claims" | "binding" => BifrostSecurityViolationKind::TailBinding,
            "fence" | "epoch" => BifrostSecurityViolationKind::TailFence,
            "replay" => BifrostSecurityViolationKind::TailReplay,
            _ => BifrostSecurityViolationKind::TailBinding,
        };
        let event = audit::audit_event_unauthenticated(
            RequestId::now_v7(),
            "bifrost.scribe.tail_security",
            "bifrost.scribe.tail",
            "bifrost:query:tail",
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
        let mut conn = self.postgres.tenant_conn(tenant_id).await.map_err(|_| {
            TailReadError::Authorization {
                detail: "tail audit connection failed".to_owned(),
            }
        })?;
        audit::append_on(&mut conn, &event)
            .await
            .map_err(|_| TailReadError::Authorization {
                detail: "tail audit append failed".to_owned(),
            })?;
        conn.commit()
            .await
            .map_err(|_| TailReadError::Authorization {
                detail: "tail audit commit failed".to_owned(),
            })
    }
}

#[async_trait]
impl TailSecurityAudit for PostgresTailSecurityAudit {
    /// Appends an unverified rejection to the system audit chain.
    async fn append_unverified_tail_rejection(&self, reason: &str) -> Result<(), TailReadError> {
        self.commit(DataTenantId::SYSTEM_OWNER, reason).await
    }

    /// Appends a verified-tenant rejection to that tenant's audit chain.
    async fn append_verified_tail_violation(
        &self,
        tenant_id: DataTenantId,
        reason: &str,
    ) -> Result<(), TailReadError> {
        self.commit(tenant_id, reason).await
    }
}
