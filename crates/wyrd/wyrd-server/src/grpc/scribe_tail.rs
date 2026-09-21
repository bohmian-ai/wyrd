//! Authenticated private tonic adapter for bounded Scribe tail fences.

use std::sync::Arc;

use crate::oracle::ScribeTailAuthority;
use vala_bifrost_redux::scribe::tail_rpc::{
    ScribeTailReader, TailReadError, encode_tail_batch_exact,
};
use vala_bifrost_redux::scribe::tail_rpc::{
    TailTicketAudience, TailTicketBinding, TailTicketMinter, TailTicketVerifier,
};
use wyrd_runtime::PrincipalKind;
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
    ScribeTailService, ScribeTailServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, AcquireTailFenceRequest, ListActiveStreamsRequest, ListActiveStreamsResponse,
    ReleaseTailFenceRequest, TailPageRequest,
};

use crate::AppState;

/// Private gRPC adapter that validates a workload caller before touching a tail fence.
pub struct ScribeTailGrpc {
    /// Server auth state used to derive the tenant from verified metadata.
    state: AppState,
    /// Scribe-owned fence state shared with local Oracle reads.
    reader: Arc<ScribeTailReader>,
    /// Optional domain-separated ticket verifier for production server-to-server calls.
    authority: Option<Arc<ScribeTailAuthority>>,
}

impl ScribeTailGrpc {
    /// Creates the adapter around the server's retained tail reader.
    #[must_use]
    pub fn new(state: AppState, reader: Arc<ScribeTailReader>) -> Self {
        Self {
            state,
            reader,
            authority: None,
        }
    }

    /// Creates the adapter with the server-owned Scribe-tail authority.
    #[must_use]
    pub fn new_with_authority(
        state: AppState,
        reader: Arc<ScribeTailReader>,
        authority: Arc<ScribeTailAuthority>,
    ) -> Self {
        Self {
            state,
            reader,
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
    /// Lists active tenant/table event-day scopes without allocating a fence.
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
            let stream = self.reader.stream_identity();
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
            .reader
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

    /// Acquires only fence metadata after authenticating and tenant-validating the binding.
    async fn acquire_fence(
        &self,
        request: Request<AcquireTailFenceRequest>,
    ) -> Result<Response<proto::TailReadFence>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let raw = request.into_inner();
        let ticket = raw.tail_ticket.clone();
        let query_id = uuid::Uuid::from_slice(&raw.query_id)
            .map_err(|_| Status::invalid_argument("tail query id is invalid"))?;
        let request = wyrd_spec::vala::api::AcquireTailFenceRequest::try_from(raw)
            .map_err(conversion_status)?;
        let mut verified_claims = None;
        if tenant != wyrd_spec::DataTenantId::SYSTEM_OWNER && request.binding.tenant_id != tenant {
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
        if let Some(authority) = &self.authority {
            let claims = authority
                .verify_tail_ticket_unbound(&ticket, TailTicketAudience::Acquire)
                .await
                .map_err(tail_status)?;
            verified_claims = Some(claims.clone());
            let stream = self.reader.stream_identity();
            let epoch = u64::try_from(stream.writer_epoch.as_i64()).unwrap_or(0);
            let canonical =
                canonical_table_name(&request.binding.namespace, &request.binding.table);
            authority
                .verify_tail_ticket_binding(
                    &claims,
                    &TailTicketBinding {
                        query_id,
                        tenant_id: request.binding.tenant_id,
                        canonical_table: canonical,
                        node_id: stream.node_id.as_uuid(),
                        writer_epoch: epoch,
                        deadline: request.deadline,
                    },
                )
                .await
                .map_err(tail_status)?;
        }
        let fence = self
            .reader
            .acquire_fence(request)
            .await
            .map_err(tail_status)?;
        let mut response: proto::TailReadFence = fence.clone().into();
        if let (Some(authority), Some(claims)) = (&self.authority, verified_claims) {
            response.capability = authority
                .mint_tail_capability(&claims, &fence)
                .map_err(tail_status)?;
        }
        Ok(Response::new(response))
    }

    /// Reads one bounded owned-frame page only after authenticating the caller.
    async fn read_fence_page(
        &self,
        request: Request<TailPageRequest>,
    ) -> Result<Response<proto::TailPage>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let raw = request.into_inner();
        let capability = raw.tail_capability.clone();
        let request =
            wyrd_spec::vala::api::TailPageRequest::try_from(raw).map_err(conversion_status)?;
        let mut access_tenant = tenant;
        if let Some(authority) = &self.authority {
            let (_cap_query_id, cap_tenant, table) = authority
                .decode_tail_capability(&capability, TailTicketAudience::Page)
                .await
                .map_err(tail_status)?;
            let fence = match self.reader.fence_metadata(request.fence_id) {
                Ok(fence) => fence,
                Err(error) => {
                    authority
                        .audit_verified_violation(cap_tenant, "fence")
                        .await
                        .map_err(tail_status)?;
                    return Err(tail_status(error));
                }
            };
            authority
                .verify_tail_capability(
                    &capability,
                    request.query_id,
                    cap_tenant,
                    &table,
                    &fence,
                    TailTicketAudience::Page,
                )
                .await
                .map_err(tail_status)?;
            if cap_tenant != fence.binding.tenant_id
                || (tenant != wyrd_spec::DataTenantId::SYSTEM_OWNER && cap_tenant != tenant)
                || table != canonical_table_name(&fence.binding.namespace, &fence.binding.table)
            {
                authority
                    .audit_verified_violation(cap_tenant, "binding")
                    .await
                    .map_err(tail_status)?;
                return Err(Status::permission_denied(
                    "tail capability binding is invalid",
                ));
            }
            if tenant == wyrd_spec::DataTenantId::SYSTEM_OWNER {
                access_tenant = cap_tenant;
            }
        }
        let page = self
            .reader
            .read_page_for_tenant(access_tenant, request)
            .map_err(tail_status)?;
        let batches = page
            .batches
            .iter()
            .map(|batch| encode_tail_batch_exact(batch.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(tail_status)?;
        Ok(Response::new(proto::TailPage {
            arrow_ipc_batches: batches,
            next_cursor: page
                .next
                .map(Into::into)
                .map(proto::tail_page::NextCursor::Next),
            complete: page.complete,
        }))
    }

    /// Releases a retained fence idempotently after authenticating the caller.
    async fn release_fence(
        &self,
        request: Request<ReleaseTailFenceRequest>,
    ) -> Result<Response<proto::ReleaseTailFenceResponse>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let raw = request.into_inner();
        let capability = raw.tail_capability.clone();
        let request = wyrd_spec::vala::api::ReleaseTailFenceRequest::try_from(raw)
            .map_err(conversion_status)?;
        let mut access_tenant = tenant;
        if let Some(authority) = &self.authority {
            let (_cap_query_id, cap_tenant, table) = authority
                .decode_tail_capability(&capability, TailTicketAudience::Page)
                .await
                .map_err(tail_status)?;
            let fence = match self.reader.fence_metadata(request.fence_id) {
                Ok(fence) => fence,
                Err(error) => {
                    authority
                        .audit_verified_violation(cap_tenant, "fence")
                        .await
                        .map_err(tail_status)?;
                    return Err(tail_status(error));
                }
            };
            authority
                .verify_tail_capability(
                    &capability,
                    request.query_id,
                    cap_tenant,
                    &table,
                    &fence,
                    TailTicketAudience::Page,
                )
                .await
                .map_err(tail_status)?;
            if cap_tenant != fence.binding.tenant_id
                || (tenant != wyrd_spec::DataTenantId::SYSTEM_OWNER && cap_tenant != tenant)
                || table != canonical_table_name(&fence.binding.namespace, &fence.binding.table)
            {
                authority
                    .audit_verified_violation(cap_tenant, "binding")
                    .await
                    .map_err(tail_status)?;
                return Err(Status::permission_denied(
                    "tail capability binding is invalid",
                ));
            }
            if tenant == wyrd_spec::DataTenantId::SYSTEM_OWNER {
                access_tenant = cap_tenant;
            }
        }
        self.reader
            .release_fence_for_tenant(access_tenant, request.fence_id)
            .map_err(tail_status)?;
        Ok(Response::new(proto::ReleaseTailFenceResponse {}))
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

/// Maps local tail failures without revealing fence existence to unauthenticated callers.
fn tail_status(error: TailReadError) -> Status {
    match error {
        TailReadError::DeadlineElapsed => Status::deadline_exceeded(error.to_string()),
        TailReadError::Capacity => Status::resource_exhausted(error.to_string()),
        TailReadError::UnsupportedProtocol { .. }
        | TailReadError::Binding
        | TailReadError::SchemaMismatch => Status::invalid_argument(error.to_string()),
        TailReadError::AccessDenied => Status::permission_denied(error.to_string()),
        TailReadError::WriterEpochMismatch | TailReadError::CursorOutOfRange => {
            Status::failed_precondition(error.to_string())
        }
        TailReadError::OversizeRow => Status::out_of_range(error.to_string()),
        TailReadError::Authorization { .. } => Status::permission_denied(error.to_string()),
        TailReadError::Encode { .. } | TailReadError::State { .. } => {
            Status::internal(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use arrow::array::Int32Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::reader::StreamReader;
    use arrow::record_batch::RecordBatch;
    use wyrd_tonic::tonic::Code;

    use vala_bifrost_redux::scribe::tail_rpc::{TailReadError, encode_tail_batch_exact};

    use super::tail_status;

    /// Proves the local encoder produces a complete owned Arrow IPC stream.
    ///
    /// # Panics
    ///
    /// Panics when the fixture batch cannot be built, encoded, decoded, or
    /// compared with its round-tripped values.
    #[test]
    fn tail_batch_encoding_round_trips_arrow_ipc() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int32,
            false,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(Int32Array::from(vec![1, 2]))])
            .expect("valid tail batch fixture");

        let bytes = encode_tail_batch_exact(&batch).expect("tail batch encodes");
        assert_eq!(bytes.len(), bytes.capacity());
        let decoded = StreamReader::try_new(Cursor::new(bytes), None)
            .expect("encoded stream initializes")
            .next()
            .expect("encoded stream contains one batch")
            .expect("encoded batch decodes");

        assert_eq!(decoded, batch);
    }

    /// Proves local encoding failures become opaque internal statuses only at the boundary.
    ///
    /// # Panics
    ///
    /// Panics when the boundary mapper exposes a non-internal tonic status.
    #[test]
    fn tail_batch_encoding_error_maps_to_internal_status() {
        let status = tail_status(TailReadError::Encode {
            detail: "fixture failure".to_owned(),
        });

        assert_eq!(status.code(), Code::Internal);
    }
}
