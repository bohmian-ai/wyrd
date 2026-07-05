//! Wyrd-owned request body size limiter.
//!
//! Returns `WyrdError::PayloadTooLarge` rendered as `application/problem+json`
//! when the body exceeds `max_bytes`. Uses bounded buffering: `to_bytes` reads
//! the full body into memory up to `max_bytes + 1`; the worst-case process
//! footprint is `max_bytes * concurrency`.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use tower::{Layer, Service};
use wyrd_spec::error::WyrdError;

use crate::http::error::WyrdErrorResponse;

/// Tower layer that wraps a service with the Wyrd-owned body size limiter.
#[derive(Clone)]
pub struct WyrdBodyLimitLayer {
    max_bytes: usize,
}

impl WyrdBodyLimitLayer {
    /// Create a new body-limit layer.
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }
}

impl<S> Layer<S> for WyrdBodyLimitLayer {
    type Service = WyrdBodyLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        WyrdBodyLimitService {
            inner,
            max_bytes: self.max_bytes,
        }
    }
}

/// Tower service that enforces the body size limit.
#[derive(Clone)]
pub struct WyrdBodyLimitService<S> {
    inner: S,
    max_bytes: usize,
}

type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

impl<S> Service<Request<Body>> for WyrdBodyLimitService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<std::convert::Infallible>,
{
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = BoxFut<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let max_bytes = self.max_bytes;
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Fast path: Content-Length header declares a size that exceeds the limit.
            if let Some(content_length) = request.headers().get(axum::http::header::CONTENT_LENGTH)
                && let Ok(s) = content_length.to_str()
                && let Ok(len) = s.parse::<usize>()
                && len > max_bytes
            {
                let error = WyrdError::PayloadTooLarge {
                    message: format!("request body exceeds the {max_bytes}-byte limit"),
                    details: serde_json::json!({
                        "max_bytes": max_bytes,
                        "declared_bytes": len,
                    }),
                };
                return Ok(WyrdErrorResponse::from(error).into_response());
            }

            // Collect body into memory, capping at max_bytes + 1 via Limited.
            // Using http_body_util::Limited directly lets us distinguish
            // length-limit failures from stream/IO failures by error type.
            let (parts, body) = request.into_parts();
            let limited = Limited::new(body, max_bytes + 1);
            match limited.collect().await {
                Ok(collected) => {
                    let bytes = collected.to_bytes();
                    if bytes.len() > max_bytes {
                        let error = WyrdError::PayloadTooLarge {
                            message: format!("request body exceeds the {max_bytes}-byte limit"),
                            details: serde_json::json!({
                                "max_bytes": max_bytes,
                            }),
                        };
                        return Ok(WyrdErrorResponse::from(error).into_response());
                    }
                    let request = Request::from_parts(parts, Body::from(bytes));
                    inner.call(request).await.map_err(Into::into)
                }
                Err(error) if error.is::<LengthLimitError>() => {
                    let error = WyrdError::PayloadTooLarge {
                        message: format!("request body exceeds the {max_bytes}-byte limit"),
                        details: serde_json::json!({
                            "max_bytes": max_bytes,
                        }),
                    };
                    Ok(WyrdErrorResponse::from(error).into_response())
                }
                Err(error) => {
                    tracing::debug!(
                        error = ?error,
                        "failed to read request body (stream/io error)"
                    );
                    let error = WyrdError::ServiceUnavailable {
                        message: "failed to read request body".to_owned(),
                        details: serde_json::json!({}),
                    };
                    Ok(WyrdErrorResponse::from(error).into_response())
                }
            }
        })
    }
}

/// Convenience constructor for use in `Router::layer` composition.
pub fn wyrd_body_limit(max_bytes: usize) -> WyrdBodyLimitLayer {
    WyrdBodyLimitLayer::new(max_bytes)
}
