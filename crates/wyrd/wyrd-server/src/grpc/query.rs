//! Authenticated public Bifrost query gRPC adapter.

use std::collections::HashMap;
use std::pin::Pin;

use futures_util::Stream;
use vala_bifrost_redux::oracle::OracleQueryStream;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt as _};
use wyrd_tonic::wyrd::v1::bifrost_query_service_server::{
    BifrostQueryService, BifrostQueryServiceServer,
};
use wyrd_tonic::wyrd::v1::{self as proto, BifrostQueryRequest};

use crate::AppState;
use crate::components::auth::Caller;

/// Owned server-streaming gRPC frame transport returned by the query service.
pub(crate) type QueryGrpcStream =
    Pin<Box<dyn Stream<Item = Result<proto::QueryStreamFrame, Status>> + Send + 'static>>;

/// Owns one Oracle stream through terminal completion or synchronous transport drop.
struct QueryGrpcStreamOwner {
    /// Oracle stream whose admission and renewal guards must be cancelled.
    query: Option<vala_bifrost_redux::oracle::OracleQueryStream>,
}

impl Stream for QueryGrpcStreamOwner {
    type Item = Result<proto::QueryStreamFrame, Status>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let Some(query) = self.query.as_mut() else {
            return std::task::Poll::Ready(None);
        };
        match query.frames.as_mut().poll_next(context) {
            std::task::Poll::Ready(None) => {
                self.query.take();
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Ready(Some(frame)) => std::task::Poll::Ready(Some(
                frame
                    .map(proto::QueryStreamFrame::from)
                    .map_err(WyrdError::from)
                    .map_err(query_status),
            )),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// Public query service backed by the process-retained Oracle runtime.
pub struct BifrostQueryGrpc {
    /// Shared server state containing auth, Gate, and Oracle role state.
    state: AppState,
}

impl BifrostQueryGrpc {
    /// Creates the public query adapter over shared application state.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Wraps this adapter in the generated tonic server.
    #[must_use]
    pub fn into_server(self) -> BifrostQueryServiceServer<Self> {
        BifrostQueryServiceServer::new(self)
    }
}

/// Authenticates owned metadata before any local role-availability lookup.
///
/// # Errors
///
/// Returns unauthenticated when the bearer credential is missing or invalid,
/// and unavailable when the verifier was not configured.
async fn caller(
    state: AppState,
    metadata: wyrd_tonic::tonic::metadata::MetadataMap,
) -> Result<Caller, Status> {
    let verifier = state
        .auth
        .token_verifier
        .clone()
        .ok_or_else(|| Status::unavailable("auth backend not configured"))?;
    let auth = vala_bifrost_redux::gate::auth::authenticate_owned(verifier, metadata.clone())
        .await
        .map_err(|error| Status::unauthenticated(error.to_string()))?;
    Ok(Caller {
        data_tenant_id: auth.tenant,
        principal: auth.principal,
        request_id: vala_bifrost_redux::gate::auth::read_or_mint_request_id(&metadata),
    })
}

#[wyrd_tonic::tonic::async_trait]
impl BifrostQueryService for BifrostQueryGrpc {
    /// Server-streaming protobuf response type.
    type QueryStream = QueryGrpcStream;

    /// Authenticates, authorizes, and starts one bounded Oracle query stream.
    ///
    /// Dropping the returned stream propagates cancellation through Oracle's
    /// retained stream guards.
    ///
    /// The stream-query future is boxed solely to bound the generated tonic
    /// trait future's compile-time layout; its runtime semantics are unchanged.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, request-validation, authorization,
    /// admission, role-unavailable, planning, or pre-stream audit failures.
    async fn query(
        &self,
        request: Request<BifrostQueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        let lifecycle = crate::app::metrics::GateRequestLifecycle::begin("query");
        let result = async {
            let caller = caller(self.state.clone(), request.metadata().clone()).await?;
            let request_id = caller.request_id.clone();
            let request = wyrd_spec::vala::api::BifrostQueryRequest::try_from(request.into_inner())
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
            let result = Box::pin(crate::query::service::stream_query(
                self.state.clone(),
                caller,
                request,
            ))
            .await
            .map_err(query_status)?;
            let mut response = query_stream_response(result);
            if let Ok(value) = request_id.as_str().parse() {
                response.metadata_mut().insert("x-wyrd-request-id", value);
            }
            Ok(response)
        }
        .await;
        lifecycle.complete(if result.is_ok() { "success" } else { "failed" });
        result
    }

    /// Lists active queries for the authenticated tenant.
    ///
    /// Cancelling the RPC future abandons the pending owner lookup without
    /// changing any query lifecycle.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, audit, role-availability,
    /// or owner-control status.
    async fn list_running_queries(
        &self,
        request: Request<proto::ListRunningQueriesRequest>,
    ) -> Result<Response<proto::ListRunningQueriesResponse>, Status> {
        let caller = caller(self.state.clone(), request.metadata().clone()).await?;
        let request_id = caller.request_id.clone();
        let queries = crate::query::service::list_running_queries(&self.state, &caller)
            .await
            .map_err(query_status)?;
        let mut response =
            Response::new(wyrd_spec::vala::api::ListRunningQueriesResponse { queries }.into());
        insert_request_id(&mut response, &request_id);
        Ok(response)
    }

    /// Gets one active query for the authenticated tenant.
    ///
    /// Cancelling the RPC future abandons the pending owner lookup without
    /// changing the active query.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, request-validation, authorization,
    /// audit, not-found, role-availability, or owner-conflict status.
    async fn get_running_query(
        &self,
        request: Request<proto::GetRunningQueryRequest>,
    ) -> Result<Response<proto::RunningQuerySummary>, Status> {
        let caller = caller(self.state.clone(), request.metadata().clone()).await?;
        let request_id = caller.request_id.clone();
        let lookup = wyrd_spec::vala::api::GetRunningQueryRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let summary =
            crate::query::service::get_running_query(&self.state, &caller, lookup.request_id)
                .await
                .map_err(query_status)?;
        let mut response = Response::new(summary.into());
        insert_request_id(&mut response, &request_id);
        Ok(response)
    }

    /// Requests idempotent cancellation for one active authenticated-tenant query.
    ///
    /// Once an exact owner accepts cancellation, cancelling the RPC future does
    /// not reverse that owner-side transition.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, request-validation, authorization,
    /// audit, not-found, role-availability, or owner-conflict status.
    async fn cancel_running_query(
        &self,
        request: Request<proto::CancelRunningQueryRequest>,
    ) -> Result<Response<proto::CancelRunningQueryResponse>, Status> {
        let caller = caller(self.state.clone(), request.metadata().clone()).await?;
        let request_id = caller.request_id.clone();
        let cancellation =
            wyrd_spec::vala::api::CancelRunningQueryRequest::try_from(request.into_inner())
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let cancelled = crate::query::service::cancel_running_query(
            &self.state,
            &caller,
            cancellation.request_id,
        )
        .await
        .map_err(query_status)?;
        let mut response = Response::new(cancelled.into());
        insert_request_id(&mut response, &request_id);
        Ok(response)
    }
}

/// Echoes one verified public request identity on a unary gRPC response.
fn insert_request_id<T>(response: &mut Response<T>, request_id: &RequestId) {
    if let Ok(value) = request_id.as_str().parse() {
        response.metadata_mut().insert("x-wyrd-request-id", value);
    }
}

/// Converts one Oracle logical stream to the canonical gRPC frame transport.
pub(crate) fn query_stream_response(result: OracleQueryStream) -> Response<QueryGrpcStream> {
    let schema_fingerprint = result.schema_fingerprint.clone();
    let output = QueryGrpcStreamOwner {
        query: Some(result),
    };
    let mut response = Response::new(Box::pin(output) as QueryGrpcStream);
    if let Ok(value) = schema_fingerprint.parse() {
        response
            .metadata_mut()
            .insert("x-wyrd-schema-fingerprint", value);
    }
    response
}

/// Converts a public Wyrd error to its closest tonic status class.
pub(crate) fn query_status(error: WyrdError) -> Status {
    let status_code = error.status();
    let code = match status_code {
        400 | 422 => wyrd_tonic::tonic::Code::InvalidArgument,
        401 => wyrd_tonic::tonic::Code::Unauthenticated,
        403 => wyrd_tonic::tonic::Code::PermissionDenied,
        404 => wyrd_tonic::tonic::Code::NotFound,
        409 => wyrd_tonic::tonic::Code::Aborted,
        429 => wyrd_tonic::tonic::Code::ResourceExhausted,
        503 => wyrd_tonic::tonic::Code::Unavailable,
        504 => wyrd_tonic::tonic::Code::DeadlineExceeded,
        _ => wyrd_tonic::tonic::Code::Internal,
    };
    let details = ErrorDetails::with_error_info(error.code(), "wyrd.dev", HashMap::new());
    let mut status = Status::with_error_details(code, error.to_string(), details);
    if matches!(status_code, 429 | 503) {
        status.metadata_mut().insert(
            "retry-after-ms",
            "1000".parse().expect("static metadata is valid"),
        );
    }
    status
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use http_body_util::BodyExt;
    use vala_bifrost_redux::oracle::OracleQueryStream;
    use wyrd_spec::vala::BifrostError;
    use wyrd_spec::vala::api::{
        QueryBatchFrame, QueryErrorDetail, QueryFreshness, QuerySchemaFrame, QuerySource,
        QueryStreamFrame, QueryTerminalError, QueryTerminalErrorCode, QueryTerminalFrame,
        QueryTerminalOutcome, QueryWarning, SourceCompletion, SourceCompletionOutcome,
    };
    use wyrd_tonic::frame_codec::FrameDecoder;
    use wyrd_tonic::tonic::Code;
    use wyrd_tonic::tonic_types::StatusExt as _;

    use super::{
        BifrostQueryGrpc, BifrostQueryService, insert_request_id, proto, query_status,
        query_stream_response,
    };

    /// Public gRPC lifecycle conversion preserves tenant opacity and idempotent cancellation.
    #[test]
    fn running_query_controls_are_tenant_scoped() {
        /// Requires the production adapter to implement every generated public query RPC.
        fn assert_public_adapter<T: BifrostQueryService>() {}

        assert_public_adapter::<BifrostQueryGrpc>();
        let owner = wyrd_spec::DataTenantId::new_v7();
        let other = wyrd_spec::DataTenantId::new_v7();
        let request_id = wyrd_spec::request_id::RequestId::now_v7();
        let registry = vala_bifrost_redux::oracle::RunningQueryRegistry::new();
        assert!(
            registry.insert(crate::oracle::lifecycle_service::pg_tests::running_entry(
                owner,
                request_id.clone(),
            ),)
        );
        assert!(registry.list(other).is_empty());
        assert!(registry.cancel(other, &request_id).is_none());
        assert!(
            registry
                .cancel(owner, &request_id)
                .expect("owner cancels")
                .cancellation_started
        );
        assert!(
            !registry
                .cancel(owner, &request_id)
                .expect("idempotent cancel")
                .cancellation_started
        );
        let summary = registry.list(owner).pop().expect("summary retained");
        let wire = proto::RunningQuerySummary::from(summary.clone());
        let decoded = wyrd_spec::vala::api::RunningQuerySummary::try_from(wire)
            .expect("public summary round-trips");
        assert_eq!(decoded.request_id, summary.request_id);
        assert_eq!(decoded.query_class, summary.query_class);
        assert_eq!(decoded.state, summary.state);
        assert_eq!(decoded.progress, summary.progress);
        assert_eq!(
            decoded.cancellation_requested,
            summary.cancellation_requested
        );
        assert_eq!(
            decoded.started_at.timestamp_millis(),
            summary.started_at.timestamp_millis()
        );
        assert_eq!(
            decoded.deadline.timestamp_millis(),
            summary.deadline.timestamp_millis()
        );
        let response_id = wyrd_spec::request_id::RequestId::now_v7();
        let mut response = wyrd_tonic::tonic::Response::new(());
        insert_request_id(&mut response, &response_id);
        assert_eq!(
            response.metadata().get("x-wyrd-request-id"),
            Some(&response_id.as_str().parse().expect("request ID metadata"))
        );
        let absent =
            query_status(wyrd_spec::vala::error::BifrostError::RunningQueryNotFound.into());
        assert_eq!(absent.code(), Code::NotFound);
        assert!(!absent.message().contains(request_id.as_str()));
    }

    /// Builds the exact complete source set for a published-only terminal.
    fn complete_sources() -> Vec<SourceCompletion> {
        vec![
            SourceCompletion {
                source: QuerySource::Iceberg,
                outcome: SourceCompletionOutcome::Complete,
            },
            SourceCompletion {
                source: QuerySource::HotSealed,
                outcome: SourceCompletionOutcome::Complete,
            },
        ]
    }

    /// Builds one canonical schema frame used by both public transports.
    fn schema_frame() -> QueryStreamFrame {
        QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "abcd".to_owned(),
            arrow_ipc_schema: vec![1, 2],
        })
    }

    /// Wraps logical frames in the retained Oracle stream contract.
    fn oracle_stream(frames: Vec<QueryStreamFrame>) -> OracleQueryStream {
        OracleQueryStream::test_new(
            "abcd".to_owned(),
            Box::pin(futures_util::stream::iter(frames.into_iter().map(Ok))),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    /// Collects the protobuf frames emitted by the HTTP adapter.
    ///
    /// # Panics
    ///
    /// Panics when the adapter emits malformed length-delimited protobuf bytes.
    async fn http_frames(frames: Vec<QueryStreamFrame>) -> Vec<proto::QueryStreamFrame> {
        let bytes = crate::query::routes::query_stream_response(oracle_stream(frames))
            .into_body()
            .collect()
            .await
            .expect("HTTP query body")
            .to_bytes();
        let mut decoder = FrameDecoder::new(1024);
        let decoded = decoder
            .push::<proto::QueryStreamFrame>(&bytes)
            .expect("HTTP protobuf frames");
        decoder.finish().expect("complete HTTP frame boundary");
        decoded
    }

    /// Collects the protobuf frames emitted by the gRPC adapter.
    ///
    /// # Panics
    ///
    /// Panics when the adapter maps a logical frame to a gRPC status.
    async fn grpc_frames(frames: Vec<QueryStreamFrame>) -> Vec<proto::QueryStreamFrame> {
        query_stream_response(oracle_stream(frames))
            .into_inner()
            .map(|frame| frame.expect("gRPC query frame"))
            .collect()
            .await
    }

    /// Proves an absent local Oracle maps to retryable gRPC unavailability.
    #[test]
    fn oracle_role_unavailable_is_retryable_grpc_status() {
        let status =
            query_status(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into());
        assert_eq!(status.code(), Code::Unavailable);
        assert_eq!(
            status.metadata().get("retry-after-ms"),
            Some(&"1000".parse().expect("static retry metadata"))
        );
    }

    /// Proves admission saturation is resource exhaustion rather than readiness failure.
    #[test]
    fn admission_rejection_is_resource_exhausted() {
        let status =
            query_status(wyrd_spec::vala::error::BifrostError::QueryAdmissionRejected.into());
        assert_eq!(status.code(), Code::ResourceExhausted);
        assert_eq!(
            status.metadata().get("retry-after-ms"),
            Some(&"1000".parse().expect("static retry metadata"))
        );
    }

    /// Retry metadata is emitted only for transient query capacity.
    #[test]
    fn grpc_retry_metadata_only_for_retryable_capacity() {
        let retryable = query_status(BifrostError::QueryAdmissionRejected.into());
        assert_eq!(retryable.metadata().get("retry-after-ms").unwrap(), "1000");
        for error in [
            BifrostError::QueryMemoryRequestTooLarge,
            BifrostError::QueryExecutionFailed,
        ] {
            assert!(
                query_status(error.into())
                    .metadata()
                    .get("retry-after-ms")
                    .is_none()
            );
        }
    }

    /// ErrorInfo retains stable capacity and poison codes without message parsing.
    #[test]
    fn grpc_error_info_preserves_capacity_and_poison_codes() {
        for error in [
            BifrostError::QueryAdmissionRejected,
            BifrostError::QueryMemoryRequestTooLarge,
            BifrostError::QueryExecutionFailed,
        ] {
            let expected = error.code().to_owned();
            let status = query_status(error.into());
            let info = status.get_details_error_info().expect("query ErrorInfo");
            assert_eq!(info.reason, expected);
        }
    }

    /// Proves an empty success stream and schema metadata cross gRPC unchanged.
    #[tokio::test]
    async fn grpc_query_stream_preserves_empty_terminal_and_schema_metadata() {
        let expected = vec![
            schema_frame(),
            QueryStreamFrame::Terminal(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Success,
                freshness: QueryFreshness::Complete,
                row_count: 0,
                warnings: Vec::new(),
                source_completion: complete_sources(),
                error: None,
            }),
        ];
        let response = query_stream_response(oracle_stream(expected.clone()));
        assert_eq!(
            response.metadata().get("x-wyrd-schema-fingerprint"),
            Some(&"abcd".parse().expect("static schema metadata"))
        );
        let mut stream = response.into_inner();
        let mut actual = Vec::new();
        while let Some(frame) = stream.next().await {
            actual.push(frame.expect("gRPC frame"));
        }
        assert_eq!(
            actual,
            expected
                .into_iter()
                .map(wyrd_tonic::wyrd::v1::QueryStreamFrame::from)
                .collect::<Vec<_>>()
        );
    }

    /// Proves HTTP and gRPC emit identical degraded and late-failed frame sequences.
    #[tokio::test]
    async fn http_and_grpc_query_frame_parity_covers_degraded_and_late_failed() {
        let mut degraded_sources = complete_sources();
        degraded_sources.push(SourceCompletion {
            source: QuerySource::LiveTail,
            outcome: SourceCompletionOutcome::Unavailable,
        });
        let degraded = vec![
            schema_frame(),
            QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: vec![3, 4, 5],
            }),
            QueryStreamFrame::Terminal(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Degraded,
                freshness: QueryFreshness::Degraded,
                row_count: 1,
                warnings: vec![QueryWarning::LiveTailUnavailable],
                source_completion: degraded_sources,
                error: None,
            }),
        ];
        let failed = vec![
            schema_frame(),
            QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: vec![6, 7],
            }),
            QueryStreamFrame::Terminal(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Failed,
                freshness: QueryFreshness::Complete,
                row_count: 1,
                warnings: Vec::new(),
                source_completion: complete_sources(),
                error: Some(QueryTerminalError {
                    code: QueryTerminalErrorCode::QueryExecutionFailed,
                    detail: Some(QueryErrorDetail::new("worker failed").expect("scrubbed detail")),
                }),
            }),
        ];

        for fixture in [degraded, failed] {
            assert_eq!(
                http_frames(fixture.clone()).await,
                grpc_frames(fixture).await
            );
        }
    }

    /// Proves dropping a gRPC stream synchronously destroys its Oracle frame owner.
    #[test]
    fn grpc_query_stream_drop_releases_owner_without_runtime() {
        /// Pending synthetic source whose destruction exposes inline owner release.
        struct DropObservedStream {
            /// Shared observation set only by this source's destructor.
            dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
        }

        impl futures_util::Stream for DropObservedStream {
            type Item = Result<QueryStreamFrame, BifrostError>;

            /// Remains pending so only transport destruction can release the owner.
            fn poll_next(
                self: std::pin::Pin<&mut Self>,
                _context: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Self::Item>> {
                std::task::Poll::Pending
            }
        }

        impl Drop for DropObservedStream {
            /// Records synchronous destruction of the retained Oracle source.
            fn drop(&mut self) {
                self.dropped
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }

        let cancellation = tokio_util::sync::CancellationToken::new();
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stream = query_stream_response(OracleQueryStream::test_new(
            "abcd".to_owned(),
            Box::pin(DropObservedStream {
                dropped: std::sync::Arc::clone(&dropped),
            }),
            cancellation,
        ))
        .into_inner();
        drop(stream);
        assert!(dropped.load(std::sync::atomic::Ordering::Acquire));
    }

    /// Prevents reintroducing asynchronous work into the gRPC stream destructor.
    #[test]
    fn grpc_query_stream_owner_has_no_detached_drop() {
        let source = include_str!("query.rs");
        let owner = source
            .split("struct QueryGrpcStreamOwner")
            .nth(1)
            .expect("gRPC query stream owner exists");
        let owner = owner
            .split("pub struct BifrostQueryGrpc")
            .next()
            .expect("gRPC query owner section ends before service");

        assert!(!owner.contains("impl Drop for QueryGrpcStreamOwner"));
        assert!(!owner.contains("tokio::spawn"));
    }
}
