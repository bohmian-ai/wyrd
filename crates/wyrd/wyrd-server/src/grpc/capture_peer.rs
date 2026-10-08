//! Private tonic adapter for server-internal writes submitted by a pod without
//! Scribe: gateway capture and Verifier results.
//!
//! Mounted only on the mutually authenticated peer listener of a pod that runs
//! Scribe, so every caller already presented the cluster `wyrd-peer`
//! certificate. That admits a trusted cluster process, not a tenant: the
//! request names its tenant and destination explicitly and carries no token.
//! The service accepts only the two capture destinations and the three
//! Verifier result tables, a result batch only with its Verifier attribution,
//! and never a reserved system tenant, then submits through the same frame the
//! in-process Scribe outbox route uses.

use std::sync::Arc;

use vala_bifrost_redux::contracts::Scribe;
use vala_bifrost_redux::gate::IngestError;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_server::{
    ScribeCapturePeerService, ScribeCapturePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{IngestCaptureRequest, IngestCaptureResponse};

use crate::scribe_outbox::{ScribeBatch, ScribeTable, VerifierAttribution};

/// Private gRPC adapter writing peer-submitted capture into this pod's Scribe.
pub struct ScribeCapturePeerGrpc {
    /// This pod's Scribe.
    scribe: Arc<dyn Scribe>,
}

impl ScribeCapturePeerGrpc {
    /// Creates the adapter around this pod's Scribe.
    #[must_use]
    pub fn new(scribe: Arc<dyn Scribe>) -> Self {
        Self { scribe }
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
    /// Returns `InvalidArgument` for a malformed tenant, batch, request id, or
    /// Verifier attribution, and `PermissionDenied` for the reserved system
    /// tenant, any table other than the five server-internal destinations, a
    /// result batch without attribution, or a capture batch with one.
    fn batch(request: IngestCaptureRequest) -> Result<ScribeBatch, Status> {
        let tenant = request
            .tenant_id
            .parse::<DataTenantId>()
            .map_err(|_| Status::invalid_argument("capture tenant_id is not a tenant id"))?;
        if tenant == DataTenantId::SYSTEM_OWNER {
            return Err(Status::permission_denied(
                "capture never writes the reserved system tenant",
            ));
        }
        let table = ScribeTable::from_fqn(&request.table).ok_or_else(|| {
            Status::permission_denied(format!(
                "the peer writer accepts only the gateway capture and Verifier result tables, not {}",
                request.table
            ))
        })?;
        let verifier = match (table.is_result(), &request.verifier) {
            (true, Some(wire)) => {
                Some(VerifierAttribution::from_wire(wire).map_err(Status::invalid_argument)?)
            }
            (false, None) => None,
            (true, None) => {
                return Err(Status::permission_denied(
                    "a Verifier result batch must name its Verifier and SYSTEM principal",
                ));
            }
            (false, Some(_)) => {
                return Err(Status::permission_denied(
                    "a gateway capture batch carries no Verifier attribution",
                ));
            }
        };
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
            verifier,
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
    async fn ingest_capture(
        &self,
        request: Request<IngestCaptureRequest>,
    ) -> Result<Response<IngestCaptureResponse>, Status> {
        let frame = Self::batch(request.into_inner())?.into_frame();
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
    use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_server::ScribeCapturePeerService;
    use wyrd_tonic::wyrd::v1::{IngestCaptureRequest, VerifierResultAttribution};

    use super::ScribeCapturePeerGrpc;
    use crate::components::gateway::recording::RecordingScribe;

    /// A well-formed request for `tenant` naming `table`, attributed to a
    /// Verifier exactly when `table` is a result table.
    fn request(tenant: DataTenantId, table: &str) -> IngestCaptureRequest {
        let result = table.starts_with("vala.verification.")
            || table.ends_with(".result_features")
            || table.ends_with(".result_items");
        IngestCaptureRequest {
            tenant_id: tenant.to_string(),
            table: table.to_owned(),
            batch_id: uuid::Uuid::now_v7().to_string(),
            request_id: RequestId::now_v7().as_str().to_owned(),
            arrow_ipc: Bytes::new(),
            verifier: result.then(attribution),
        }
    }

    /// A well-formed Verifier result attribution.
    fn attribution() -> VerifierResultAttribution {
        VerifierResultAttribution {
            verifier_ref: "default/Verifier/drift-check@1.0.0".to_owned(),
            verifier_uid: uuid::Uuid::now_v7().to_string(),
            principal_id: uuid::Uuid::now_v7().to_string(),
        }
    }

    /// Proves the peer service accepts exactly the two capture destinations
    /// without attribution and the three Verifier result tables with it, and
    /// refuses the reserved system tenant, every other table, a result batch
    /// without its attribution, a capture batch with one, a reference to a
    /// non-Verifier Card, and malformed ids, all before any Scribe work.
    ///
    /// # Panics
    ///
    /// Panics when a request validates differently.
    #[test]
    fn requests_are_confined_to_tenant_capture_destinations() {
        let tenant = DataTenantId::new_v7();
        for table in ["vala.gateway.calls", "vala.traces.spans"] {
            let batch = ScribeCapturePeerGrpc::batch(request(tenant, table)).expect("accepted");
            assert_eq!((batch.tenant, batch.table.fqn()), (tenant, table));
            assert!(batch.verifier.is_none());
        }
        for table in [
            "vala.verification.results",
            "vala.drift.result_features",
            "vala.eval.result_items",
        ] {
            let wire = request(tenant, table);
            let sent = wire.verifier.clone().expect("attributed");
            let batch = ScribeCapturePeerGrpc::batch(wire).expect("accepted");
            assert_eq!((batch.tenant, batch.table.fqn()), (tenant, table));
            let attribution = batch.verifier.expect("result batches carry attribution");
            assert_eq!(attribution.principal.to_string(), sent.principal_id);
            assert_eq!(
                attribution.verifier.uid.map(|uid| uid.to_string()),
                Some(sent.verifier_uid)
            );
        }
        let refusals = [
            (
                request(DataTenantId::SYSTEM_OWNER, "vala.gateway.calls"),
                Code::PermissionDenied,
            ),
            (
                request(DataTenantId::SYSTEM_OWNER, "vala.verification.results"),
                Code::PermissionDenied,
            ),
            (
                request(tenant, "vala.system.audit_log"),
                Code::PermissionDenied,
            ),
            (
                request(tenant, "vala.drift.observations"),
                Code::PermissionDenied,
            ),
            (request(tenant, "user.events"), Code::PermissionDenied),
            (
                IngestCaptureRequest {
                    verifier: None,
                    ..request(tenant, "vala.verification.results")
                },
                Code::PermissionDenied,
            ),
            (
                IngestCaptureRequest {
                    verifier: Some(attribution()),
                    ..request(tenant, "vala.gateway.calls")
                },
                Code::PermissionDenied,
            ),
            (
                IngestCaptureRequest {
                    verifier: Some(VerifierResultAttribution {
                        verifier_ref: "default/Service/checkout@1.0.0".to_owned(),
                        ..attribution()
                    }),
                    ..request(tenant, "vala.verification.results")
                },
                Code::InvalidArgument,
            ),
            (
                IngestCaptureRequest {
                    verifier: Some(VerifierResultAttribution {
                        verifier_uid: String::new(),
                        ..attribution()
                    }),
                    ..request(tenant, "vala.eval.result_items")
                },
                Code::InvalidArgument,
            ),
            (
                IngestCaptureRequest {
                    tenant_id: "not-a-tenant".to_owned(),
                    ..request(tenant, "vala.gateway.calls")
                },
                Code::InvalidArgument,
            ),
            (
                IngestCaptureRequest {
                    batch_id: "not-a-uuid".to_owned(),
                    ..request(tenant, "vala.gateway.calls")
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
            table: "vala.gateway.calls".to_owned(),
        });
        let service = ScribeCapturePeerGrpc::new(Arc::clone(&scribe) as _);
        let status = service
            .ingest_capture(Request::new(request(
                DataTenantId::new_v7(),
                "vala.gateway.calls",
            )))
            .await
            .expect_err("saturated");
        assert_eq!(status.code(), Code::ResourceExhausted, "{status}");
        assert_eq!(scribe.attempts(), 1);
    }
}
