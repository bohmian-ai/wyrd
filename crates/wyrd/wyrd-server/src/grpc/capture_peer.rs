//! Private tonic adapter for server-internal writes submitted by a pod without
//! Scribe: audit decisions, gateway capture, Verifier results, and tenant
//! copies of server spans.
//!
//! Mounted only on the mutually authenticated peer listener of a pod that runs
//! Scribe, so every caller already presented the cluster `wyrd-peer`
//! certificate. That admits a trusted cluster process, not a tenant: the
//! request names its tenant and destination explicitly and carries no token.
//! The service accepts only the audit log, the two capture destinations, and
//! the Verifier result and execution input tables, whose rows carry the subject and actor
//! Scribe stamps, and the reserved system tenant only for audit history,
//! then submits through the same frame the in-process Scribe outbox route
//! uses.

use std::sync::Arc;

use vala_bifrost_redux::contracts::Scribe;
use vala_bifrost_redux::gate::IngestError;
use vala_bifrost_redux::gate::attribution::CardRegistry;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_server::{
    ScribeCapturePeerService, ScribeCapturePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{IngestCaptureRequest, IngestCaptureResponse};

use crate::scribe_outbox::{ScribeBatch, ScribeTable};

/// Private gRPC adapter writing peer-submitted capture into this pod's Scribe.
pub struct ScribeCapturePeerGrpc {
    /// This pod's Scribe.
    scribe: Arc<dyn Scribe>,
    /// The tenant registry each frame's Cards are described from; `None`
    /// stamps no shared dimension.
    cards: Option<CardRegistry>,
}

impl ScribeCapturePeerGrpc {
    /// Creates the adapter around this pod's Scribe, describing frames from
    /// `cards` when given.
    #[must_use]
    pub fn new(scribe: Arc<dyn Scribe>, cards: Option<CardRegistry>) -> Self {
        Self { scribe, cards }
    }

    /// Returns the generated service wrapper for peer-router mounting.
    #[must_use]
    pub fn into_server(self) -> ScribeCapturePeerServiceServer<Self> {
        ScribeCapturePeerServiceServer::new(self)
    }

    /// Validates one request into the batch the in-process writer would build.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArgument` for a malformed tenant, batch, or request id,
    /// and `PermissionDenied` for the reserved
    /// system tenant outside audit history or any table other than the six
    /// server-internal destinations.
    fn batch(request: IngestCaptureRequest) -> Result<ScribeBatch, Status> {
        let tenant = request
            .tenant_id
            .parse::<DataTenantId>()
            .map_err(|_| Status::invalid_argument("capture tenant_id is not a tenant id"))?;
        let table = ScribeTable::from_fqn(&request.table).ok_or_else(|| {
            Status::permission_denied(format!(
                "the peer writer accepts only the audit, gateway capture, and Verifier result and input tables, not {}",
                request.table
            ))
        })?;
        if tenant == DataTenantId::SYSTEM_OWNER && table != ScribeTable::AuditLog {
            return Err(Status::permission_denied(
                "only audit history writes the reserved system tenant",
            ));
        }
        let batch_id = request
            .batch_id
            .parse::<uuid::Uuid>()
            .map_err(|_| Status::invalid_argument("capture batch_id is not a UUID"))?;
        let request_id = request
            .request_id
            .parse::<RequestId>()
            .map_err(|_| Status::invalid_argument("capture request_id is invalid"))?;
        Ok(ScribeBatch {
            tenant,
            table,
            batch_id,
            request_id,
            ipc: request.arrow_ipc,
        })
    }
}

#[wyrd_tonic::tonic::async_trait]
impl ScribeCapturePeerService for ScribeCapturePeerGrpc {
    /// Submits one capture batch to this pod's Scribe and answers on its ACK.
    ///
    /// Scribe refusals cross through Gate's one ingest status mapping, so the
    /// caller sees saturation as `ResourceExhausted` and a closed writer as
    /// `Unavailable`, exactly as public ingest would report them.
    ///
    /// # Errors
    ///
    /// - A malformed frame is refused before any Scribe work: `InvalidArgument`
    ///   for an unparseable tenant, batch, or request id, and `PermissionDenied`
    ///   for a table outside the server-internal destinations or the reserved
    ///   system tenant outside audit history.
    /// - `Unavailable` when the tenant registry cannot describe the frame's
    ///   Cards, so the peer retries rather than writing unstamped rows.
    /// - A Scribe refusal carries Gate's ingest status with its Wyrd error code:
    ///   `ResourceExhausted` for saturation (with `retry-after-ms`), payload
    ///   limits, or a full WAL disk; `Unavailable` for a closed writer;
    ///   `NotFound` for a missing table; `FailedPrecondition` for a schema
    ///   fingerprint mismatch; `InvalidArgument` for an invalid frame or a
    ///   row contract violation;
    ///   `PermissionDenied` for a Card scope refusal; and `Internal` otherwise.
    async fn ingest_capture(
        &self,
        request: Request<IngestCaptureRequest>,
    ) -> Result<Response<IngestCaptureResponse>, Status> {
        let frame = Self::batch(request.into_inner())?
            .into_frame(self.cards.as_ref())
            .await
            .map_err(|_| Status::unavailable("the tenant registry is unavailable"))?;
        self.scribe
            .ingest_frame(frame)
            .await
            .map_err(|error| IngestError::from_scribe(error).into_status())?;
        Ok(Response::new(IngestCaptureResponse {}))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use vala_bifrost_redux::contracts::ScribeError;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_tonic::tonic::{Code, Request};
    use wyrd_tonic::wyrd::v1::IngestCaptureRequest;
    use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_server::ScribeCapturePeerService;

    use super::ScribeCapturePeerGrpc;
    use crate::components::gateway::recording::RecordingScribe;

    /// A well-formed request for `tenant` naming `table`.
    fn request(tenant: DataTenantId, table: &str) -> IngestCaptureRequest {
        IngestCaptureRequest {
            tenant_id: tenant.to_string(),
            table: table.to_owned(),
            batch_id: uuid::Uuid::now_v7().to_string(),
            request_id: RequestId::now_v7().as_str().to_owned(),
            arrow_ipc: Bytes::new(),
        }
    }

    /// Proves the peer service accepts exactly the audit log (for a tenant or
    /// the system owner), the two capture destinations, and the Verifier
    /// result and execution input tables, and refuses the reserved system tenant elsewhere, every other
    /// table, and malformed ids, all before any Scribe work.
    ///
    /// # Panics
    ///
    /// Panics when a request validates differently.
    #[test]
    fn requests_are_confined_to_tenant_capture_destinations() {
        let tenant = DataTenantId::new_v7();
        for (tenant, table) in [
            (tenant, "bifrost.system.audit_log"),
            (DataTenantId::SYSTEM_OWNER, "bifrost.system.audit_log"),
            (tenant, "bifrost.gateway.calls"),
            (tenant, "bifrost.traces.spans"),
            (tenant, "bifrost.verification.results"),
            (tenant, "bifrost.verification.execution_outputs"),
            (tenant, "bifrost.verification.execution_inputs"),
        ] {
            let batch = ScribeCapturePeerGrpc::batch(request(tenant, table)).expect("accepted");
            assert_eq!((batch.tenant, batch.table.fqn()), (tenant, table));
        }
        let refusals = [
            (
                request(DataTenantId::SYSTEM_OWNER, "bifrost.gateway.calls"),
                Code::PermissionDenied,
            ),
            (
                request(DataTenantId::SYSTEM_OWNER, "bifrost.verification.results"),
                Code::PermissionDenied,
            ),
            (
                request(tenant, "bifrost.drift.observations"),
                Code::PermissionDenied,
            ),
            (request(tenant, "user.events"), Code::PermissionDenied),
            (
                IngestCaptureRequest {
                    tenant_id: "not-a-tenant".to_owned(),
                    ..request(tenant, "bifrost.gateway.calls")
                },
                Code::InvalidArgument,
            ),
            (
                IngestCaptureRequest {
                    batch_id: "not-a-uuid".to_owned(),
                    ..request(tenant, "bifrost.gateway.calls")
                },
                Code::InvalidArgument,
            ),
        ];
        for (refused, code) in refusals {
            let table = refused.table.clone();
            let status = ScribeCapturePeerGrpc::batch(refused).expect_err("refused");
            assert_eq!(status.code(), code, "{table}: {status}");
        }
    }

    /// Proves a Scribe refusal crosses the peer plane through Gate's ingest
    /// status mapping, so saturation reaches the writer as retryable.
    ///
    /// # Panics
    ///
    /// Panics when the refusal maps to another status.
    #[tokio::test]
    async fn scribe_saturation_answers_resource_exhausted() {
        let scribe = Arc::new(RecordingScribe::default());
        scribe.refuse_next(ScribeError::IngestBusy {
            table: "bifrost.gateway.calls".to_owned(),
        });
        let service = ScribeCapturePeerGrpc::new(Arc::clone(&scribe) as _, None);
        let status = service
            .ingest_capture(Request::new(request(
                DataTenantId::new_v7(),
                "bifrost.gateway.calls",
            )))
            .await
            .expect_err("saturated");
        assert_eq!(status.code(), Code::ResourceExhausted, "{status}");
        assert_eq!(scribe.attempts(), 1);
    }
}
