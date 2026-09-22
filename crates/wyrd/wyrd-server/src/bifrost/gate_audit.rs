use async_trait::async_trait;
use vala_bifrost_redux::gate::{AuthContext, GateAudit, IngestError};
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};

use crate::audit;
use crate::postgres::ServerPostgres;

/// Tenant-scoped writer for Gate's `bifrost_record:write` decisions.
///
/// Gate evaluates the permission but owns no database, so the composition root
/// supplies this sink. Each decision commits in its own tenant transaction
/// before Gate admits or refuses the write.
#[derive(Clone)]
pub struct PostgresGateAudit {
    /// Pools and tenant binding used for one decision transaction.
    postgres: ServerPostgres,
}

impl PostgresGateAudit {
    /// Binds the sink to the server's Postgres owner.
    #[must_use]
    pub const fn new(postgres: ServerPostgres) -> Self {
        Self { postgres }
    }
}

#[async_trait]
impl GateAudit for PostgresGateAudit {
    /// Commits one write decision to the caller's tenant audit chain.
    ///
    /// The row is attributed to the verified principal — the subject a
    /// delegated token acts for — and, when the token carries a non-empty
    /// delegation chain, names its actors through the same
    /// [`AuditDetail::DelegationAttribution`] projection HTTP and Oracle audit
    /// use. A direct call keeps no detail, exactly as before delegation.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::AuditUnavailable`] when the tenant connection,
    /// the append, or the commit fails, so Gate refuses the write rather than
    /// admitting it unaudited.
    async fn append_write_decision(
        &self,
        auth: &AuthContext,
        resource: &str,
        outcome: AuditOutcome,
    ) -> Result<(), IngestError> {
        let mut event = AuditEvent::new(
            auth.request_id.clone(),
            None,
            "bifrost.record.write".to_owned(),
            resource.to_owned(),
            auth.principal.card_ref().cloned(),
            auth.principal.id,
            auth.principal.kind.tag(),
            "bifrost:record:write".to_owned(),
            outcome,
        );
        if !auth.delegation_chain.is_empty() {
            event = event.with_detail(AuditDetail::DelegationAttribution {
                delegation_chain: wyrd_runtime::audit_delegation_chain(&auth.delegation_chain),
            });
        }
        let mut conn = self
            .postgres
            .tenant_conn(auth.tenant)
            .await
            .map_err(|error| IngestError::AuditUnavailable(error.to_string()))?;
        audit::append_on(&mut conn, &event)
            .await
            .map_err(|error| IngestError::AuditUnavailable(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| IngestError::AuditUnavailable(error.to_string()))
    }
}
