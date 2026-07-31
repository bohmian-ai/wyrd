//! Authenticated public Bifrost query gRPC adapter.

use std::pin::Pin;

use futures_util::{Stream, StreamExt};
use wyrd_spec::error::WyrdError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::bifrost_query_service_server::{
    BifrostQueryService, BifrostQueryServiceServer,
};
use wyrd_tonic::wyrd::v1::{self as proto, BifrostQueryRequest};

use crate::AppState;
use crate::components::auth::Caller;

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
    type QueryStream =
        Pin<Box<dyn Stream<Item = Result<proto::QueryStreamFrame, Status>> + Send + 'static>>;

    /// Authenticates, authorizes, and starts one bounded Oracle query stream.
    ///
    /// Dropping the returned stream propagates cancellation through Oracle's
    /// retained stream guards.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, request-validation, authorization,
    /// admission, role-unavailable, planning, or pre-stream audit failures.
    async fn query(
        &self,
        request: Request<BifrostQueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        let caller = caller(self.state.clone(), request.metadata().clone()).await?;
        let request = wyrd_spec::vala::api::BifrostQueryRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let result = crate::query::service::stream_query(self.state.clone(), caller, request)
            .await
            .map_err(query_status)?;
        Ok(query_stream_response(result))
    }
}

/// Converts one Oracle logical stream to the canonical gRPC frame transport.
pub(crate) fn query_stream_response(
    result: vala_bifrost_redux::oracle::OracleQueryStream,
) -> Response<Pin<Box<dyn Stream<Item = Result<proto::QueryStreamFrame, Status>> + Send + 'static>>>
{
    let mut frames = result.frames;
    let output = async_stream::stream! {
        while let Some(frame) = frames.next().await {
            yield frame
                .map(proto::QueryStreamFrame::from)
                .map_err(WyrdError::from)
                .map_err(query_status);
        }
    };
    let mut response = Response::new(Box::pin(output)
        as Pin<Box<dyn Stream<Item = Result<proto::QueryStreamFrame, Status>> + Send + 'static>>);
    if let Ok(value) = result.schema_fingerprint.parse() {
        response
            .metadata_mut()
            .insert("x-wyrd-schema-fingerprint", value);
    }
    response
}

/// Converts a public Wyrd error to its closest tonic status class.
pub(crate) fn query_status(error: WyrdError) -> Status {
    let mut status = match error.status() {
        400 | 422 => Status::invalid_argument(error.to_string()),
        401 => Status::unauthenticated(error.to_string()),
        403 => Status::permission_denied(error.to_string()),
        404 => Status::not_found(error.to_string()),
        409 => Status::aborted(error.to_string()),
        429 => Status::resource_exhausted(error.to_string()),
        503 => Status::unavailable(error.to_string()),
        504 => Status::deadline_exceeded(error.to_string()),
        _ => Status::internal(error.to_string()),
    };
    if matches!(error.status(), 429 | 503) {
        status.metadata_mut().insert(
            "retry-after-ms",
            "1000".parse().expect("static metadata is valid"),
        );
    }
    status
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use futures_util::StreamExt;
    use http_body_util::BodyExt;
    use vala_bifrost_redux::oracle::OracleQueryStream;
    use wyrd_spec::vala::api::{
        QueryBatchFrame, QueryErrorDetail, QueryFreshness, QuerySchemaFrame, QuerySource,
        QueryStreamFrame, QueryTerminalError, QueryTerminalErrorCode, QueryTerminalFrame,
        QueryTerminalOutcome, QueryWarning, SourceCompletion, SourceCompletionOutcome,
    };
    use wyrd_tonic::frame_codec::FrameDecoder;
    use wyrd_tonic::tonic::Code;

    use super::{proto, query_status, query_stream_response};

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
        OracleQueryStream {
            schema_fingerprint: "abcd".to_owned(),
            frames: Box::pin(futures_util::stream::iter(frames.into_iter().map(Ok))),
        }
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

    /// Proves dropping a gRPC stream releases the retained Oracle frame owner.
    #[tokio::test]
    async fn grpc_query_stream_drop_propagates_cancellation() {
        /// Marks when the synthetic Oracle stream is dropped.
        struct DropSignal(Arc<AtomicBool>);

        impl Drop for DropSignal {
            /// Records cancellation at the Oracle stream ownership boundary.
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let stream_dropped = Arc::clone(&dropped);
        let frames = async_stream::stream! {
            let _signal = DropSignal(stream_dropped);
            yield Ok(schema_frame());
            std::future::pending::<()>().await;
        };
        let mut stream = query_stream_response(OracleQueryStream {
            schema_fingerprint: "abcd".to_owned(),
            frames: Box::pin(frames),
        })
        .into_inner();
        let _first = stream.next().await.expect("schema frame");
        drop(stream);
        assert!(
            dropped.load(Ordering::Acquire),
            "gRPC cancellation must drop Oracle stream guards"
        );
    }
}
