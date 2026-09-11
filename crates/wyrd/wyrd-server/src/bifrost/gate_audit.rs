use async_trait::async_trait;
use vala_bifrost_redux::gate::{AuthContext, GateAudit, IngestError};
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

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
        let event = AuditEvent::new(
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
