//! Protected-edge request timeout with a staged handoff for routes that own a
//! longer deadline.
//!
//! Every protected request is bounded by `LimitsConfig::timeout` while it holds
//! a concurrency slot. `POST /v1/query` and `POST /v1/verification/execute` are
//! staged: body collection, authentication, and admission stay under that
//! generic timer, but once the handler hands the request to the work that
//! carries its own deadline — Oracle dispatch for a query, the Verifier engine
//! for a direct execution — the generic timer stops racing it. That deadline
//! then owns the rest of the request's timing and cleanup.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request};
use tokio_util::sync::CancellationToken;
use tower::timeout::error::Elapsed;
use tower::{BoxError, Layer, Service};

/// Path of the public SQL query route.
const QUERY_PATH: &str = "/v1/query";

/// Path of the direct verification route, whose engine owns a 60-second
/// deadline.
const EXECUTE_PATH: &str = "/v1/verification/execute";

/// Reports whether `request` is the public SQL query, `POST /v1/query`.
///
/// Its waiting belongs to Oracle rather than to the generic protected edge,
/// and edge capacity treats it separately.
pub(crate) fn is_public_query<B>(request: &Request<B>) -> bool {
    request.method() == Method::POST && request.uri().path() == QUERY_PATH
}

/// Reports whether `request`'s edge timer is staged: the public query or a
/// direct verification execution.
fn is_staged<B>(request: &Request<B>) -> bool {
    is_public_query(request)
        || (request.method() == Method::POST && request.uri().path() == EXECUTE_PATH)
}

/// Request extension that ends the generic edge timer for one staged request.
///
/// Inserted only on staged routes. The handler calls [`Self::hand_off`]
/// immediately before the work that captures the request's own deadline;
/// dropping it without a handoff leaves the edge timer armed.
#[derive(Clone, Debug)]
pub struct EdgeTimer {
    /// Sticky handoff signal observed by the edge future.
    handoff: CancellationToken,
}

impl EdgeTimer {
    /// Stops the generic edge timer so the request's own deadline governs the rest.
    ///
    /// Idempotent: repeated calls have no further effect.
    pub fn hand_off(&self) {
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
    /// Staged timeout service placed around the protected body-limit,
    /// authentication, and route stack.
    type Service = EdgeTimeout<S>;

    /// Wraps `inner` with the staged edge timer.
    fn layer(&self, inner: S) -> Self::Service {
        EdgeTimeout {
            inner,
            timeout: self.timeout,
        }
    }
}

/// Service enforcing the generic edge timeout, staged for the routes
/// [`is_staged`] selects.
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
    /// Response produced unchanged by the wrapped protected stack.
    type Response = S::Response;
    /// Boxed error carrying either an inner-stack failure or edge [`Elapsed`],
    /// which the outer error handler maps to the request-timeout problem.
    type Error = BoxError;
    /// Future racing the inner call against the edge limit until completion
    /// or a staged route's handoff.
    type Future = EdgeFuture<S::Response>;

    /// Delegates readiness to the wrapped stack.
    ///
    /// # Errors
    ///
    /// Returns the wrapped stack's readiness failure, boxed.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    /// Races the wrapped call against the edge limit until completion or, for a
    /// staged route, until its handler hands the request off.
    ///
    /// Expiry drops the in-flight call, cancelling pre-handoff work, and returns
    /// [`Elapsed`] for the existing request-timeout problem mapping.
    ///
    /// # Errors
    ///
    /// The returned future resolves to the wrapped stack's failure, boxed, or to
    /// [`Elapsed`] when the edge limit passes before completion or handoff.
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let handoff = is_staged(&request).then(|| {
            let handoff = CancellationToken::new();
            request.extensions_mut().insert(EdgeTimer {
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
