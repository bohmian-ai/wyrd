//! Stock OpenTelemetry OTLP/HTTP exporters that authenticate as a
//! [`WyrdClient`].
//!
//! Each factory builds the stock `opentelemetry-otlp` exporter for one signal,
//! pointed at the client's server, with an HTTP client that asks the
//! [`WyrdClient`] for its current access token on every export and sends it
//! as `x-wyrd-access-token`. The token comes from the client's one shared
//! refresh path, so a long-lived exporter keeps working after any single
//! access token expires; this module holds no token, cache, retry, or
//! refresher of its own.

use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use opentelemetry_otlp::{
    ExporterBuildError, LogExporter, MetricExporter, Protocol, SpanExporter, WithExportConfig as _,
    WithHttpConfig as _,
};
use tokio::runtime::Handle;
use wyrd_client::WyrdClient;

/// Header the Wyrd OTLP collector reads the bearer from.
const ACCESS_TOKEN_HEADER: &str = "x-wyrd-access-token";

/// Build a span exporter that sends OTLP/HTTP protobuf to `client`'s server.
///
/// # Errors
/// Returns [`ExporterBuildError::InternalFailure`] when called outside a Tokio
/// runtime, and the stock builder's error when the endpoint is invalid.
pub fn span_exporter(client: &WyrdClient) -> Result<SpanExporter, ExporterBuildError> {
    SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint(client, "traces"))
        .with_http_client(TokenHttpClient::new(client)?)
        .build()
}

/// Build a log exporter that sends OTLP/HTTP protobuf to `client`'s server.
///
/// # Errors
/// Returns [`ExporterBuildError::InternalFailure`] when called outside a Tokio
/// runtime, and the stock builder's error when the endpoint is invalid.
pub fn log_exporter(client: &WyrdClient) -> Result<LogExporter, ExporterBuildError> {
    LogExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint(client, "logs"))
        .with_http_client(TokenHttpClient::new(client)?)
        .build()
}

/// Build a metric exporter that sends OTLP/HTTP protobuf to `client`'s server.
///
/// # Errors
/// Returns [`ExporterBuildError::InternalFailure`] when called outside a Tokio
/// runtime, and the stock builder's error when the endpoint is invalid.
pub fn metric_exporter(client: &WyrdClient) -> Result<MetricExporter, ExporterBuildError> {
    MetricExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint(client, "metrics"))
        .with_http_client(TokenHttpClient::new(client)?)
        .build()
}

/// The signal-specific OTLP/HTTP URL on `client`'s server.
fn endpoint(client: &WyrdClient, signal: &str) -> String {
    format!("{}/v1/{signal}", client.server_url().trim_end_matches('/'))
}

/// The HTTP client the stock exporters send through.
///
/// The stock batch processors drive exports from their own thread rather than
/// from the caller's runtime, so each send is spawned onto the Tokio runtime
/// captured when the exporter was built, where the [`WyrdClient`]'s token
/// refresh and this client's connection pool live.
#[derive(Debug)]
struct TokenHttpClient {
    /// The client whose access token authenticates every export.
    client: WyrdClient,
    /// The connection pool exports are sent through.
    http: reqwest::Client,
    /// The runtime every send runs on.
    runtime: Handle,
}

impl TokenHttpClient {
    /// Capture `client` and the current Tokio runtime.
    ///
    /// # Errors
    /// Returns [`ExporterBuildError::InternalFailure`] outside a Tokio runtime.
    fn new(client: &WyrdClient) -> Result<Self, ExporterBuildError> {
        let runtime = Handle::try_current().map_err(|_| {
            ExporterBuildError::InternalFailure(
                "a Wyrd OTLP exporter must be built inside a Tokio runtime".to_owned(),
            )
        })?;
        Ok(Self {
            client: client.clone(),
            http: reqwest::Client::new(),
            runtime,
        })
    }

    /// Send one export with a freshly obtained access token.
    ///
    /// # Errors
    /// Returns the client's error when no token can be obtained, and the
    /// transport's error when the request fails or its body cannot be read.
    async fn send(
        client: WyrdClient,
        http: reqwest::Client,
        mut request: Request<Bytes>,
    ) -> Result<Response<Bytes>, HttpError> {
        let token = client.access_token().await?;
        request.headers_mut().insert(
            ACCESS_TOKEN_HEADER,
            format!("Bearer {}", token.expose()).try_into()?,
        );
        let response = http.execute(reqwest::Request::try_from(request)?).await?;
        let mut stock = Response::builder().status(response.status());
        if let Some(headers) = stock.headers_mut() {
            headers.extend(response.headers().clone());
        }
        Ok(stock.body(response.bytes().await?)?)
    }
}

#[async_trait::async_trait]
impl HttpClient for TokenHttpClient {
    /// Run [`Self::send`] on the captured runtime and wait for it.
    ///
    /// # Errors
    /// Returns [`Self::send`]'s error, or the join error when the runtime
    /// shuts down before the send completes.
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        self.runtime
            .spawn(Self::send(self.client.clone(), self.http.clone(), request))
            .await?
    }
}
