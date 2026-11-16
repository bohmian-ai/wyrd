//! Protected-edge load shedding that owner-admitted routes bypass.
//!
//! Every protected request is shed when the protected edge's concurrency limit
//! is full, except `POST /v1/query` and `POST /v1/verification/{execute,enqueue}`.
//! A public query that finds Oracle's execution slots busy waits in Oracle's
//! own bounded, tenant-fair queue under its query deadline, and a verification
//! waits in hosted admission over process memory under its arrival deadline;
//! the generic edge must not refuse them earlier, and must not let long waits
//! take the concurrency slots other routes rely on.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::Request;
use tower::limit::ConcurrencyLimit;
use tower::load_shed::LoadShed;
use tower::{BoxError, Layer, Service, ServiceExt as _};

use super::edge_timeout::owns_admission;

/// Tower layer applying [`EdgeCapacity`] with the protected edge's limit.
#[derive(Clone, Copy, Debug)]
pub struct EdgeCapacityLayer {
    /// In-flight request limit from `LimitsConfig::concurrency`.
    concurrency: usize,
}

impl EdgeCapacityLayer {
    /// Creates a layer that sheds protected requests beyond `concurrency`.
    #[must_use]
    pub const fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }
}

impl<S: Clone> Layer<S> for EdgeCapacityLayer {
    /// Service routing each request to the limited or the direct stack.
    type Service = EdgeCapacity<S>;

    /// Wraps one copy of `inner` in load shedding and the concurrency limit and
    /// keeps a second, unlimited copy for owner-admitted routes.
    fn layer(&self, inner: S) -> Self::Service {
        EdgeCapacity {
            limited: LoadShed::new(ConcurrencyLimit::new(inner.clone(), self.concurrency)),
            direct: inner,
        }
    }
}

/// Service that sheds protected requests at the concurrency limit, except
/// owner-admitted routes, which it passes straight to the inner stack.
#[derive(Clone, Debug)]
pub struct EdgeCapacity<S> {
    /// Inner stack behind load shedding and the concurrency limit.
    limited: LoadShed<ConcurrencyLimit<S>>,
    /// The same inner stack without edge capacity limits.
    direct: S,
}

/// Boxed response future returned by [`EdgeCapacity`].
type CapacityFuture<T> = Pin<Box<dyn Future<Output = Result<T, BoxError>> + Send + 'static>>;

impl<S> Service<Request<Body>> for EdgeCapacity<S>
where
    S: Service<Request<Body>> + Clone + Send + 'static,
    S::Error: Into<BoxError>,
    S::Future: Send + 'static,
{
    /// Response produced unchanged by the inner stack.
    type Response = S::Response;
    /// Inner failure, or the load-shed overload the outer error handler maps.
    type Error = BoxError;
    /// Future of whichever stack the request was routed to.
    type Future = CapacityFuture<S::Response>;

    /// Reports readiness of the direct stack.
    ///
    /// The limited stack is driven to readiness per request by `oneshot`, so
    /// no concurrency permit is reserved for a request that bypasses it.
    ///
    /// # Errors
    ///
    /// Returns the inner stack's readiness failure, boxed.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.direct.poll_ready(cx).map_err(Into::into)
    }

    /// Sends an owner-admitted request to the direct stack and every other
    /// request through load shedding and the concurrency limit.
    ///
    /// # Errors
    ///
    /// The returned future resolves to the inner stack's failure, boxed, or to
    /// the load-shed overload when another request finds the limit full.
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        if owns_admission(&request) {
            let response = self.direct.call(request);
            return Box::pin(async move { response.await.map_err(Into::into) });
        }
        Box::pin(self.limited.clone().oneshot(request))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::time::Duration;

    use axum::http::Method;
    use tokio::sync::Notify;
    use tower::load_shed::error::Overloaded;
    use tower::service_fn;

    use super::*;

    /// Builds a request with `method` and `path` and an empty body.
    fn request(method: Method, path: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .expect("static test request")
    }

    /// With the single concurrency slot held by a stalled request, another
    /// route is shed immediately while public queries and verification
    /// execute and enqueue still reach the inner stack and its own admission.
    ///
    /// # Panics
    ///
    /// Panics when a bypassing route is shed or another route is not.
    #[tokio::test]
    async fn owner_admitted_routes_bypass_the_full_concurrency_limit() {
        let release = std::sync::Arc::new(Notify::new());
        let inner = service_fn({
            let release = std::sync::Arc::clone(&release);
            move |request: Request<Body>| {
                let release = std::sync::Arc::clone(&release);
                async move {
                    if request.uri().path() == "/v1/stall" {
                        release.notified().await;
                    }
                    Ok::<_, Infallible>(())
                }
            }
        });
        let service = EdgeCapacityLayer::new(1).layer(inner);

        let stalled = tokio::spawn(service.clone().oneshot(request(Method::GET, "/v1/stall")));
        tokio::task::yield_now().await;
        let shed = service
            .clone()
            .oneshot(request(Method::GET, "/v1/cards"))
            .await
            .expect_err("another route is shed while the only slot is held");
        assert!(shed.is::<Overloaded>());
        for path in [
            "/v1/query",
            "/v1/query",
            "/v1/verification/execute",
            "/v1/verification/enqueue",
        ] {
            tokio::time::timeout(
                Duration::from_secs(5),
                service.clone().oneshot(request(Method::POST, path)),
            )
            .await
            .unwrap_or_else(|_| panic!("{path} is not held behind the limit"))
            .unwrap_or_else(|_| panic!("{path} is not shed"));
        }
        let shed = service
            .clone()
            .oneshot(request(Method::POST, "/v1/verification/evidence"))
            .await
            .expect_err("evidence uploads keep the limit");
        assert!(shed.is::<Overloaded>());

        release.notify_one();
        stalled
            .await
            .expect("stalled task")
            .expect("the stalled request completes");
    }
}
