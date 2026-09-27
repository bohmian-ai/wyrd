//! Authenticated private tonic adapter for Scribe active-stream discovery.

use std::sync::Arc;

use crate::oracle::ScribeTailAuthority;
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailService, TailReadError, TailTicketAudience, TailTicketBinding, TailTicketVerifier,
};
use wyrd_runtime::PrincipalKind;
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
    ScribeTailService, ScribeTailServiceServer,
};
use wyrd_tonic::wyrd::v1::{self as proto, ListActiveStreamsRequest, ListActiveStreamsResponse};

use crate::AppState;

/// Private gRPC adapter that validates a workload caller before listing live streams.
pub struct ScribeTailGrpc {
    /// Server auth state used to derive the tenant from verified metadata.
    state: AppState,
    /// Scribe-owned live source whose active partitions are listed.
    source: Arc<FetchLiveTailService>,
    /// Optional domain-separated ticket verifier for production server-to-server calls.
    authority: Option<Arc<ScribeTailAuthority>>,
}

impl ScribeTailGrpc {
    /// Creates the adapter around the server's Scribe live source.
    #[must_use]
    pub fn new(state: AppState, source: Arc<FetchLiveTailService>) -> Self {
        Self {
            state,
            source,
            authority: None,
        }
    }

    /// Creates the adapter with the server-owned Scribe-tail authority.
    #[must_use]
    pub fn new_with_authority(
        state: AppState,
        source: Arc<FetchLiveTailService>,
        authority: Arc<ScribeTailAuthority>,
    ) -> Self {
        Self {
            state,
            source,
            authority: Some(authority),
        }
    }

    /// Returns the generated service wrapper for application-router mounting.
    #[must_use]
    pub fn into_server(self) -> ScribeTailServiceServer<Self> {
        ScribeTailServiceServer::new(self)
    }

    /// Authenticates a service workload and returns its server-derived tenant.
    async fn authenticated_tenant(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<wyrd_spec::DataTenantId, Status> {
        let verifier = self
            .state
            .auth
            .token_verifier
            .as_ref()
            .ok_or_else(|| Status::unavailable("auth backend not configured"))?;
        let auth = vala_bifrost_redux::gate::auth::authenticate(verifier.as_ref(), metadata)
            .map_err(|error| Status::unauthenticated(error.to_string()))?;
        if !matches!(auth.principal.kind, PrincipalKind::Service { .. }) {
            return Err(Status::permission_denied(
                "private Scribe tail requires a workload service identity",
            ));
        }
        Ok(auth.tenant)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl ScribeTailService for ScribeTailGrpc {
    /// Lists active tenant/table event-day scopes; returns metadata, never rows.
    async fn list_active_streams(
        &self,
        request: Request<ListActiveStreamsRequest>,
    ) -> Result<Response<ListActiveStreamsResponse>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let request = request.into_inner();
        let query_id = uuid::Uuid::from_slice(&request.query_id)
            .map_err(|_| Status::invalid_argument("tail query id is invalid"))?;
        let binding = request
            .binding
            .ok_or_else(|| Status::invalid_argument("tail binding is required"))?;
        if tenant != wyrd_spec::DataTenantId::SYSTEM_OWNER
            && binding.tenant_id != tenant.as_uuid().to_string()
        {
            if let Some(authority) = &self.authority {
                authority
                    .audit_unverified_rejection("binding")
                    .await
                    .map_err(tail_status)?;
            }
            return Err(Status::permission_denied(
                "tail binding tenant does not match caller",
            ));
        }
        let binding = wyrd_spec::vala::api::TenantTableBinding::try_from(binding)
            .map_err(conversion_status)?;
        if let Some(authority) = &self.authority {
            let claims = authority
                .verify_tail_ticket_unbound(&request.tail_ticket, TailTicketAudience::List)
                .await
                .map_err(tail_status)?;
            let canonical = canonical_table_name(&binding.namespace, &binding.table);
            let stream = self.source.stream();
            authority
                .verify_tail_ticket_binding(
                    &claims,
                    &TailTicketBinding {
                        query_id,
                        tenant_id: binding.tenant_id,
                        canonical_table: canonical,
                        node_id: stream.node_id.as_uuid(),
                        writer_epoch: u64::try_from(stream.writer_epoch.as_i64()).unwrap_or(0),
                        deadline: claims.deadline,
                    },
                )
                .await
                .map_err(tail_status)?;
        } else if request.tail_ticket.is_empty() {
            return Err(Status::permission_denied("tail ticket is required"));
        }
        let streams = self
            .source
            .list_active_streams(&binding)
            .map_err(tail_status)?;
        Ok(Response::new(ListActiveStreamsResponse {
            streams: streams
                .into_iter()
                .map(|(time_partition, stream)| proto::ActiveTailStream {
                    time_partition: Some(time_partition.into()),
                    stream: Some(stream.into()),
                })
                .collect(),
        }))
    }
}

/// Maps malformed private wire fields to an unauthenticated-safe invalid argument status.
fn conversion_status(error: PrivateConversionError) -> Status {
    Status::invalid_argument(error.to_string())
}

/// Builds the domain-qualified table identity used by signed private tickets.
fn canonical_table_name(namespace: &str, table: &str) -> String {
    if namespace.starts_with("vala.") {
        format!("{namespace}.{table}")
    } else {
        format!("vala.{namespace}.{table}")
    }
}

/// Maps local discovery failures to tonic statuses at the private boundary.
fn tail_status(error: TailReadError) -> Status {
    match error {
        TailReadError::DeadlineElapsed => Status::deadline_exceeded(error.to_string()),
        TailReadError::Binding => Status::invalid_argument(error.to_string()),
        TailReadError::Authorization { .. } => Status::permission_denied(error.to_string()),
        TailReadError::State { .. } => Status::internal(error.to_string()),
        TailReadError::Unavailable { .. } => Status::unavailable(error.to_string()),
        TailReadError::StaleIdentity => Status::failed_precondition(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use wyrd_tonic::tonic::Code;

    use vala_bifrost_redux::scribe::tail_rpc::TailReadError;

    use super::tail_status;

    /// Proves Scribe state failures become opaque internal statuses only at the boundary.
    ///
    /// # Panics
    ///
    /// Panics when the boundary mapper exposes a non-internal tonic status.
    #[test]
    fn tail_state_error_maps_to_internal_status() {
        let status = tail_status(TailReadError::State {
            detail: "fixture failure".to_owned(),
        });

        assert_eq!(status.code(), Code::Internal);
    }
}
