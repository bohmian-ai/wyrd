//! Axum adapters for the query service functions.
//!
//! Query route adapters.

use std::io;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::StreamExt;
use vala_bifrost_redux::oracle::OracleQueryStream;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_tonic::frame_codec::FrameEncoder;

use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::query::service;
use crate::state::AppState;

/// Content type of the length-delimited Bifrost query frame stream.
const QUERY_STREAM_CONTENT_TYPE: &str = "application/vnd.wyrd.bifrost-query-stream";

/// Standalone query router for the `/v1` group.
pub fn router() -> Router<AppState> {
    Router::new().route("/query", post(sync_query))
}

#[utoipa::path(
    post,
    path = "/v1/query",
    request_body = BifrostQueryRequest,
    responses(
        (
            status = 200,
            description = "Length-delimited protobuf QueryStreamFrame stream",
            content_type = "application/vnd.wyrd.bifrost-query-stream",
            body = Vec<u8>,
            headers(
                (
                    "x-wyrd-schema-fingerprint" = String,
                    description = "Hex SHA-256 fingerprint of the response schema"
                )
            )
        ),
        (status = 503, description = "Oracle role is unavailable")
    ),
    tag = "Bifrost"
)]
/// Streams one authenticated SQL query as canonical protobuf frames.
pub(crate) async fn sync_query(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<BifrostQueryRequest>,
) -> Response {
    let result = match service::stream_query(state.clone(), caller, body).await {
        Ok(result) => result,
        Err(error) => return query_error_response(error),
    };
    #[cfg(feature = "test-support")]
    let fault = state
        .query_stream_fault
        .as_ref()
        .and_then(|controller| controller.claim());
    #[cfg(feature = "test-support")]
    let stall = match fault {
        Some(crate::state::QueryStreamFault::StallAfterSchema) => state
            .query_stream_fault
            .as_ref()
            .and_then(|controller| controller.claim_stall()),
        _ => None,
    };
    #[cfg(not(feature = "test-support"))]
    let fault = None;
    #[cfg(not(feature = "test-support"))]
    let stall = None;
    query_stream_response_with_fault(result, fault, stall)
}

/// Converts an admitted Oracle stream into the stable HTTP frame transport.
///
/// The body pulls exactly one logical frame at a time, preserving downstream
/// backpressure. Dropping the body drops Oracle's stream guards and propagates
/// cancellation without a buffering task.
///
/// # Errors
///
/// Frame encoding and late Oracle errors surface as body-stream IO failures;
/// response construction failures return the stable internal problem response.
#[cfg(test)]
pub(crate) fn query_stream_response(result: OracleQueryStream) -> Response {
    query_stream_response_with_fault(result, None, None)
}

/// Drop guard notifying test waiters after the stalled body releases its stream.
#[cfg(feature = "test-support")]
struct QueryBodyDropNotifier {
    /// Shared lifecycle state paired with the scheduled stall.
    stall: Option<std::sync::Arc<crate::state::QueryStreamStall>>,
}

#[cfg(feature = "test-support")]
impl Drop for QueryBodyDropNotifier {
    /// Publishes response-owner release after later-declared stream state drops.
    fn drop(&mut self) {
        if let Some(stall) = &self.stall {
            stall.mark_dropped();
        }
    }
}

/// Converts an admitted Oracle stream into HTTP frames, optionally truncating
/// one test-tier response after its schema or first batch.
///
/// The truncation is claimed atomically before the body starts and is never
/// represented on the public request or response contract. Dropping the body
/// still drops the original Oracle frame stream and its admission guards.
#[cfg(feature = "test-support")]
fn query_stream_response_with_fault(
    result: OracleQueryStream,
    fault: Option<crate::state::QueryStreamFault>,
    stall: Option<std::sync::Arc<crate::state::QueryStreamStall>>,
) -> Response {
    let schema_fingerprint = result.schema_fingerprint.clone();
    if let Some(stall) = &stall {
        stall.bind_resource_probe(result.resource_probe_for_test());
    }
    let body = Body::from_stream(async_stream::stream! {
        let _drop_notifier = QueryBodyDropNotifier { stall: stall.clone() };
        let mut stream = Some(result);
        let mut emitted = 0_u8;
        while let Some(frame) = match stream.as_mut() {
            Some(query) => query.frames.next().await,
            None => None,
        } {
            let frame = match frame {
                Ok(frame) => FrameEncoder::encode(
                    &wyrd_tonic::wyrd::v1::QueryStreamFrame::from(frame),
                )
                .map(Bytes::from)
                .map_err(|error| io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(error.to_string())),
            };
            yield frame;
            emitted = emitted.saturating_add(1);
            match fault {
                Some(crate::state::QueryStreamFault::EofAfterSchema) if emitted >= 1 => {
                    if let Some(query) = stream.take() {
                        query.cancel().await;
                    }
                    return;
                }
                Some(crate::state::QueryStreamFault::EofAfterBatch) if emitted >= 2 => {
                    if let Some(query) = stream.take() {
                        query.cancel().await;
                    }
                    return;
                }
                Some(crate::state::QueryStreamFault::StallAfterSchema) if emitted >= 1 => {
                    if let Some(stall) = &stall {
                        stall.mark_entered();
                        std::future::pending::<()>().await;
                    }
                    return;
                }
                _ => {}
            }
        }
    });

    match Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, QUERY_STREAM_CONTENT_TYPE)
        .header("x-wyrd-schema-fingerprint", schema_fingerprint)
        .body(body)
    {
        Ok(response) => response,
        Err(error) => query_error_response(WyrdError::Internal {
            message: "failed to build query response".to_owned(),
            details: serde_json::json!({ "detail": error.to_string() }),
        }),
    }
}

#[cfg(not(feature = "test-support"))]
fn query_stream_response_with_fault(
    result: OracleQueryStream,
    _fault: Option<()>,
    _stall: Option<()>,
) -> Response {
    let schema_fingerprint = result.schema_fingerprint;
    let mut frames = result.frames;
    let body = Body::from_stream(async_stream::stream! {
        while let Some(frame) = frames.next().await {
            let frame = match frame {
                Ok(frame) => FrameEncoder::encode(
                    &wyrd_tonic::wyrd::v1::QueryStreamFrame::from(frame),
                )
                .map(Bytes::from)
                .map_err(|error| io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(error.to_string())),
            };
            yield frame;
        }
    });
    match Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, QUERY_STREAM_CONTENT_TYPE)
        .header("x-wyrd-schema-fingerprint", schema_fingerprint)
        .body(body)
    {
        Ok(response) => response,
        Err(error) => query_error_response(WyrdError::Internal {
            message: "failed to build query response".to_owned(),
            details: serde_json::json!({ "detail": error.to_string() }),
        }),
    }
}

/// Renders pre-stream query errors and marks unavailable roles retryable.
pub(crate) fn query_error_response(error: WyrdError) -> Response {
    let retryable = error.status() == 503;
    let mut response = WyrdErrorResponse::from(error).into_response();
    if retryable {
        response.headers_mut().insert(
            header::RETRY_AFTER,
            "1".parse().expect("static header is valid"),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use axum::body::Body;
    use http_body_util::BodyExt;
    use wyrd_runtime::permission::{Permission, PermissionSet};
    use wyrd_runtime::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        FreshnessPolicy, QueryBatchFrame, QueryErrorDetail, QueryFreshness, QuerySchemaFrame,
        QuerySource, QueryStreamFrame, QueryTerminalError, QueryTerminalErrorCode,
        QueryTerminalFrame, QueryTerminalOutcome, SourceCompletion, SourceCompletionOutcome,
        VisibilityMode,
    };
    use wyrd_tonic::frame_codec::FrameDecoder;

    use super::*;

    /// Builds the required complete source set for a published-only terminal.
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

    /// Builds one authorized caller for a real pre-byte service dispatch.
    ///
    /// # Panics
    ///
    /// Panics when the shared fixture tenant cannot be resolved.
    async fn query_caller() -> Caller {
        let tenant = crate::test_support::test_tenant().await;
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                Vec::new(),
                PermissionSet::from_iter([Permission::bifrost_query_read()]),
            ),
            request_id: RequestId::now_v7(),
        }
    }

    /// Builds real server state with no local Gate or Oracle role.
    ///
    /// # Panics
    ///
    /// Panics when shared Postgres, storage, or catalog fixtures cannot start.
    async fn state_without_oracle() -> AppState {
        AppState::new(
            crate::test_support::test_server_postgres().await,
            crate::test_support::test_storage().await,
            crate::test_support::test_catalog().await,
        )
    }

    /// Proves HTTP emits canonical length-delimited frames and only the schema header.
    #[tokio::test]
    async fn http_query_stream_has_exact_media_headers_and_frames() {
        let expected = vec![
            QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "abcd".to_owned(),
                arrow_ipc_schema: vec![1, 2],
            }),
            QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: vec![3, 4, 5],
            }),
            QueryStreamFrame::Terminal(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Degraded,
                freshness: QueryFreshness::Degraded,
                row_count: 1,
                warnings: Vec::new(),
                source_completion: Vec::new(),
                error: None,
            }),
        ];
        let frames = futures_util::stream::iter(expected.clone().into_iter().map(Ok));
        let response = query_stream_response(OracleQueryStream::test_new(
            "abcd".to_owned(),
            Box::pin(frames),
            tokio_util::sync::CancellationToken::new(),
        ));
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(
                &QUERY_STREAM_CONTENT_TYPE
                    .parse()
                    .expect("static media type")
            )
        );
        assert_eq!(
            response.headers().get("x-wyrd-schema-fingerprint"),
            Some(&"abcd".parse().expect("static fingerprint"))
        );
        assert!(!response.headers().contains_key("x-wyrd-row-count"));

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("stream body completes")
            .to_bytes();
        let mut decoder = FrameDecoder::new(1024);
        let decoded = decoder
            .push::<wyrd_tonic::wyrd::v1::QueryStreamFrame>(&bytes)
            .expect("length-delimited frames decode");
        decoder.finish().expect("body ends between frames");
        assert_eq!(
            decoded,
            expected
                .into_iter()
                .map(wyrd_tonic::wyrd::v1::QueryStreamFrame::from)
                .collect::<Vec<_>>()
        );
    }

    /// Proves role mismatch is a typed retryable problem before any body stream.
    #[tokio::test]
    async fn oracle_role_unavailable_has_retry_after() {
        let response = query_error_response(
            wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into(),
        );
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(header::RETRY_AFTER),
            Some(&"1".parse().expect("static retry header"))
        );
        let body = Body::new(response.into_body());
        assert!(
            !body
                .collect()
                .await
                .expect("problem body")
                .to_bytes()
                .is_empty()
        );
    }

    /// Proves an empty query result emits schema then terminal with no batch.
    #[tokio::test]
    async fn http_query_stream_preserves_empty_result() {
        let expected = vec![
            QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "abcd".to_owned(),
                arrow_ipc_schema: vec![1],
            }),
            QueryStreamFrame::Terminal(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Success,
                freshness: QueryFreshness::Complete,
                row_count: 0,
                warnings: Vec::new(),
                source_completion: complete_sources(),
                error: None,
            }),
        ];
        let bytes = query_stream_response(OracleQueryStream::test_new(
            "abcd".to_owned(),
            Box::pin(futures_util::stream::iter(
                expected.clone().into_iter().map(Ok),
            )),
            tokio_util::sync::CancellationToken::new(),
        ))
        .into_body()
        .collect()
        .await
        .expect("empty result body")
        .to_bytes();
        let mut decoder = FrameDecoder::new(1024);
        let actual = decoder
            .push::<wyrd_tonic::wyrd::v1::QueryStreamFrame>(&bytes)
            .expect("empty result frames");
        assert_eq!(
            actual,
            expected
                .into_iter()
                .map(wyrd_tonic::wyrd::v1::QueryStreamFrame::from)
                .collect::<Vec<_>>()
        );
    }

    /// Proves a real pre-byte role error never constructs the query frame transport.
    #[test]
    fn http_query_prebyte_service_error_has_no_frame_headers() {
        wyrd_runtime::runtime().block_on(async {
            let response = sync_query(
                State(state_without_oracle().await),
                query_caller().await,
                Json(BifrostQueryRequest {
                    sql: "SELECT 1".to_owned(),
                    visibility: VisibilityMode::PublishedOnly,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(1_000),
                }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_ne!(
                response.headers().get(header::CONTENT_TYPE),
                Some(
                    &QUERY_STREAM_CONTENT_TYPE
                        .parse()
                        .expect("static stream media type")
                )
            );
            assert!(
                !response.headers().contains_key("x-wyrd-schema-fingerprint"),
                "pre-byte errors must not expose stream metadata"
            );
        });
    }

    /// Proves a post-byte failure remains a terminal frame, not a status rewrite.
    #[tokio::test]
    async fn http_query_stream_preserves_late_failed_terminal() {
        let terminal = QueryStreamFrame::Terminal(QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            row_count: 1,
            warnings: Vec::new(),
            source_completion: Vec::new(),
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: Some(QueryErrorDetail::new("worker failed").expect("scrubbed detail")),
            }),
        });
        let frames = futures_util::stream::iter([
            Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "abcd".to_owned(),
                arrow_ipc_schema: vec![1],
            })),
            Ok(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: vec![2],
            })),
            Ok(terminal.clone()),
        ]);
        let bytes = query_stream_response(OracleQueryStream::test_new(
            "abcd".to_owned(),
            Box::pin(frames),
            tokio_util::sync::CancellationToken::new(),
        ))
        .into_body()
        .collect()
        .await
        .expect("late failure remains a valid body")
        .to_bytes();
        let mut decoder = FrameDecoder::new(1024);
        let decoded = decoder
            .push::<wyrd_tonic::wyrd::v1::QueryStreamFrame>(&bytes)
            .expect("failed terminal decodes");
        assert_eq!(
            decoded.last(),
            Some(&wyrd_tonic::wyrd::v1::QueryStreamFrame::from(terminal))
        );
    }

    /// Proves dropping an HTTP body drops the retained Oracle frame stream.
    #[tokio::test]
    async fn http_query_body_drop_propagates_cancellation() {
        /// Marks when the synthetic Oracle stream is dropped.
        struct DropSignal(Arc<AtomicBool>);

        impl Drop for DropSignal {
            /// Records cancellation at the same ownership boundary as Oracle guards.
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let stream_dropped = Arc::clone(&dropped);
        let frames = async_stream::stream! {
            let _signal = DropSignal(stream_dropped);
            yield Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "abcd".to_owned(),
                arrow_ipc_schema: vec![1],
            }));
            std::future::pending::<()>().await;
        };
        let response = query_stream_response(OracleQueryStream::test_new(
            "abcd".to_owned(),
            Box::pin(frames),
            tokio_util::sync::CancellationToken::new(),
        ));
        let mut body = response.into_body();
        let _first = body.frame().await.expect("first body frame");
        drop(body);
        assert!(
            dropped.load(Ordering::Acquire),
            "transport body drop must release Oracle stream guards"
        );
    }
}
