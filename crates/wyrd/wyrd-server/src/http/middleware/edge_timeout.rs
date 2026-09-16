//! Protected-edge request timeout with one staged query handoff.
//!
//! Every protected request is bounded by `LimitsConfig::timeout` while it holds
//! a concurrency slot. `POST /v1/query` alone is staged: body collection,
//! authentication, and capability admission stay under that generic timer, but
//! once the handler hands the request to Oracle dispatch — which captures the
//! request's own query deadline — the generic timer stops racing it. Oracle then
//! owns preparation, first-batch wait, and terminal-stream timing and cleanup.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request};
use tokio_util::sync::CancellationToken;
use tower::timeout::error::Elapsed;
use tower::{BoxError, Layer, Service};

/// Path of the only route whose edge timeout is staged.
const QUERY_PATH: &str = "/v1/query";

/// Request extension that ends the generic edge timer for one query.
///
/// Inserted only on `POST /v1/query`. The query handler calls
/// [`Self::hand_off_to_oracle`] immediately before Oracle dispatch captures the
/// query deadline; dropping it without a handoff leaves the edge timer armed.
#[derive(Clone, Debug)]
pub struct QueryEdgeTimer {
    /// Sticky handoff signal observed by the edge future.
    handoff: CancellationToken,
}

impl QueryEdgeTimer {
    /// Stops the generic edge timer so the query's own deadline governs the rest.
    ///
    /// Idempotent: repeated calls have no further effect.
    pub fn hand_off_to_oracle(&self) {
        self.handoff.cancel();
    }
}

/// Tower layer applying [`EdgeTimeout`] with the protected edge's limit.
#[derive(Clone, Copy, Debug)]
pub struct EdgeTimeoutLayer {
    /// Generic per-request limit from `LimitsConfig::timeout`.
    timeout: Duration,
}

impl EdgeTimeoutLayer {
    /// Creates a layer bounding every protected request by `timeout`.
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl<S> Layer<S> for EdgeTimeoutLayer {
    type Service = EdgeTimeout<S>;

    /// Wraps `inner` with the staged edge timer.
    fn layer(&self, inner: S) -> Self::Service {
        EdgeTimeout {
            inner,
            timeout: self.timeout,
        }
    }
}

/// Service enforcing the generic edge timeout, staged for `POST /v1/query`.
#[derive(Clone, Debug)]
pub struct EdgeTimeout<S> {
    /// Wrapped body-limit, authentication, and route stack.
    inner: S,
    /// Generic per-request limit from `LimitsConfig::timeout`.
    timeout: Duration,
}

/// Boxed response future returned by [`EdgeTimeout`].
type EdgeFuture<T> = Pin<Box<dyn Future<Output = Result<T, BoxError>> + Send + 'static>>;

impl<S> Service<Request<Body>> for EdgeTimeout<S>
where
    S: Service<Request<Body>>,
    S::Error: Into<BoxError>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = EdgeFuture<S::Response>;

    /// Delegates readiness to the wrapped stack.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    /// Races the wrapped call against the edge limit until completion or, for a
    /// query, until its handler hands the request to Oracle.
    ///
    /// Expiry drops the in-flight call, cancelling pre-Oracle work, and returns
    /// [`Elapsed`] for the existing request-timeout problem mapping.
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let handoff = (request.method() == Method::POST && request.uri().path() == QUERY_PATH)
            .then(|| {
                let handoff = CancellationToken::new();
                request.extensions_mut().insert(QueryEdgeTimer {
                    handoff: handoff.clone(),
                });
                handoff
            });
        let response = self.inner.call(request);
        let timeout = self.timeout;
        Box::pin(async move {
            tokio::pin!(response);
            let handed_off = async {
                match &handoff {
                    Some(handoff) => handoff.cancelled().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;
                result = &mut response => return result.map_err(Into::into),
                () = handed_off => {}
                () = tokio::time::sleep(timeout) => return Err(Elapsed::new().into()),
            }
            response.await.map_err(Into::into)
        })
    }
}
