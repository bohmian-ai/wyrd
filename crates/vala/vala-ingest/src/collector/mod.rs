//! OTLP/gRPC trace collector: a drop-in `TraceService` that authenticates,
//! decodes `ExportTraceServiceRequest` to span rows, and writes them through the
//! live group-commit coordinator on the `traces.spans` table.
//!
//! The service is the gRPC edge; the reusable decode→write core is
//! [`OtlpTraceService::ingest_resource_spans`], which Task C's HTTP OTLP path
//! consumes without re-touching tonic. The `traces.spans` table is a
//! `SystemShared` multi-tenant physical table, so the writer opens with
//! [`TableScope::SystemShared`] and the token-derived data tenant is stamped
//! into `data_tenant_id` by the coordinator (never derived from the wire).
//!
//! Backpressure vs. bad data:
//! - `IngestBusy` (coordinator buffer full) → gRPC `UNAVAILABLE`, retryable, no
//!   `partial_success`; no rows from this request were written.
//! - Individual malformed spans → OTLP `partial_success` with `rejected_spans`
//!   and an `error_message`; the request as a whole still succeeds.
//! - Auth failure and a total decode failure → hard error status.

pub mod map;

use std::sync::Arc;

use vala_bifrost::tables::traces::SpansTable;
use vala_bifrost::writer::BifrostWriteContext;
use vala_bifrost::{TableScope, WyrdCatalog};
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_runtime::{Permission, PermissionCheck, RbacCheck};
use wyrd_tonic::otlp::trace_service::trace_service_server::{TraceService, TraceServiceServer};
use wyrd_tonic::otlp::trace_service::{
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use wyrd_tonic::tonic::{Request, Response, Status};

use crate::auth::{AuthContext, IngestAuthInterceptor};
use crate::error::IngestError;
use map::{MappedSpans, map_resource_spans, spans_to_record_batch};

/// Audit origin recorded on every OTLP-sourced commit.
const OTLP_ORIGIN: &str = "otlp";

/// OTLP/gRPC trace receiver over `vala-bifrost`'s group-commit writer.
///
/// Generic over the resolver-backed verifier's `R` / `I`, static-dispatched like
/// [`crate::service::BifrostIngestGrpc`], so it shares the one auth seam.
pub struct OtlpTraceService<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    auth: IngestAuthInterceptor<R, I>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> OtlpTraceService<R, I> {
    /// Construct from the engine catalog and the shared ingest auth interceptor.
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>, auth: IngestAuthInterceptor<R, I>) -> Self {
        Self { catalog, auth }
    }

    /// Wrap into the generated OTLP server type for mounting on the shared
    /// tonic listener.
    #[must_use]
    pub fn into_server(self) -> TraceServiceServer<Self> {
        TraceServiceServer::new(self)
    }

    /// Decode → write core, reusable by Task C's HTTP OTLP path.
    ///
    /// Authorizes the record-write capability, flattens the spans, writes the
    /// accepted rows through the live coordinator as one commit unit, and
    /// reports the per-span rejection count.
    ///
    /// # Errors
    /// Returns an [`IngestError`] for auth/RBAC/engine failures (including
    /// [`IngestError::WriterBusy`] on backpressure). Per-span decode failures do
    /// not error here — they surface in [`IngestOutcome::rejected_spans`].
    pub async fn ingest_resource_spans(
        &self,
        auth: &AuthContext,
        request: ExportTraceServiceRequest,
    ) -> Result<IngestOutcome, IngestError> {
        // Gate: record-write capability. Tenant isolation is the data partition,
        // stamped server-side; this is the coarse operation gate.
        RbacCheck
            .check(&auth.principal, &Permission::bifrost_record_write())
            .into_result()
            .map_err(IngestError::from_rbac)?;

        let MappedSpans { records, rejected } = map_resource_spans(&request.resource_spans);
        let rejected_spans = rejected.len() as i64;
        let rejection_message = rejected.first().map(|first| first.reason.clone());

        if records.is_empty() {
            return Ok(IngestOutcome {
                accepted_spans: 0,
                rejected_spans,
                rejection_message,
            });
        }

        let batch =
            spans_to_record_batch(&records).map_err(IngestError::Decode)?;
        let accepted_spans = records.len() as i64;

        // traces.spans is SystemShared: bind SYSTEM_OWNER for the registration /
        // commit rows, stamp the data tenant into data_tenant_id. typed_writer
        // carries the table's Sensitive payload class so the coordinator redacts
        // the `attributes` column at flush.
        let writer = self
            .catalog
            .typed_writer::<SpansTable>(TableScope::SystemShared, auth.tenant)
            .await
            .map_err(IngestError::from_engine)?;

        let ctx = BifrostWriteContext {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            origin: OTLP_ORIGIN.to_owned(),
            actor: auth.principal.id.to_string(),
            request_id: auth.request_id.clone(),
            card_ref: auth.principal.card_ref().cloned(),
        };

        writer
            .commit_one(auth.tenant, vec![batch], ctx)
            .await
            .map_err(IngestError::from_engine)?;

        Ok(IngestOutcome {
            accepted_spans,
            rejected_spans,
            rejection_message,
        })
    }
}

/// Result of one accepted OTLP export: how many spans committed and how many the
/// receiver dropped for a per-span reason.
#[derive(Debug)]
pub struct IngestOutcome {
    /// Spans committed to `traces.spans`.
    pub accepted_spans: i64,
    /// Spans dropped for a per-span reason (surfaced as `partial_success`).
    pub rejected_spans: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl IngestOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    #[must_use]
    fn partial_success(&self) -> Option<ExportTracePartialSuccess> {
        if self.rejected_spans == 0 {
            return None;
        }
        Some(ExportTracePartialSuccess {
            rejected_spans: self.rejected_spans,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more spans were rejected".to_owned()),
        })
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> TraceService
    for OtlpTraceService<R, I>
{
    #[tracing::instrument(
        name = "otlp.trace.export",
        skip_all,
        fields(tenant, accepted_spans, rejected_spans)
    )]
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        tracing::Span::current().record("tenant", tracing::field::display(auth.tenant));

        let payload = request.into_inner();
        let outcome = self
            .ingest_resource_spans(&auth, payload)
            .await
            .map_err(map_export_error)?;

        tracing::Span::current().record("accepted_spans", outcome.accepted_spans);
        tracing::Span::current().record("rejected_spans", outcome.rejected_spans);

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

/// Map an ingest error onto its gRPC status. Backpressure is remapped to the
/// retryable OTLP `UNAVAILABLE` (rather than `RESOURCE_EXHAUSTED`) so an OTLP
/// SDK retries the whole export.
fn map_export_error(error: IngestError) -> Status {
    match error {
        IngestError::WriterBusy => Status::unavailable(error.to_string()),
        other => other.into_status(),
    }
}

#[cfg(test)]
mod tests {
    use super::map_export_error;
    use crate::error::IngestError;
    use wyrd_tonic::tonic::Code;

    // Backpressure is a coordinator-local, cross-boundary-state-free condition
    // (channel-full). Forcing genuine channel saturation end-to-end is flaky and
    // materially harder than asserting the load-bearing mapping directly, so the
    // `IngestBusy → UNAVAILABLE` contract is pinned here per the testing doctrine.
    #[test]
    fn ingest_busy_maps_to_unavailable_not_resource_exhausted() {
        let status = map_export_error(IngestError::WriterBusy);
        assert_eq!(
            status.code(),
            Code::Unavailable,
            "OTLP backpressure must be retryable UNAVAILABLE, not RESOURCE_EXHAUSTED"
        );
    }

    #[test]
    fn other_errors_keep_their_ingest_status() {
        let status = map_export_error(IngestError::TableNotFound {
            table: "vala.traces.spans".to_owned(),
        });
        assert_eq!(status.code(), Code::NotFound);
    }
}
