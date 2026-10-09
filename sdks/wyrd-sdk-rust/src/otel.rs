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
//!
//! [`start_telemetry`] installs the process's `tracing` pipeline over the span
//! exporter, stamping every span started inside a `Run::scope` with that Run
//! and Card.

use std::sync::OnceLock;
use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{Context, KeyValue};
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use opentelemetry_otlp::{
    ExporterBuildError, LogExporter, MetricExporter, Protocol, SpanExporter, WithExportConfig as _,
    WithHttpConfig as _,
};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SdkTracerProvider, Span, SpanData, SpanProcessor};
use serde_json::json;
use tokio::runtime::Handle;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use wyrd_client::WyrdClient;
use wyrd_client::observe::current_run_scope;
use wyrd_client::state::WyrdState;
use wyrd_spec::error::WyrdError;

/// Header the Wyrd OTLP collector reads the bearer from.
const ACCESS_TOKEN_HEADER: &str = "x-wyrd-access-token";

/// The provider [`start_telemetry`] installed, which every later call returns.
static INSTALLED: OnceLock<SdkTracerProvider> = OnceLock::new();

/// The `tracing` pipeline [`start_telemetry`] installed for this process.
#[derive(Clone, Debug)]
pub struct Telemetry {
    /// The provider exporting every INFO-and-above span as the state's client.
    provider: SdkTracerProvider,
}

impl Telemetry {
    /// Export every finished span; call it at graceful shutdown, from a
    /// multi-threaded Tokio runtime, since the export runs on that runtime
    /// while this call blocks.
    ///
    /// A global subscriber cannot be removed, so the pipeline stays installed
    /// and keeps exporting spans started later in the process.
    ///
    /// # Errors
    /// Returns `WYRD_CLIENT_400_CONFIG_INVALID` naming the provider's failure
    /// when an export fails.
    pub fn shutdown(&self) -> Result<(), WyrdError> {
        self.provider
            .force_flush()
            .map_err(|error| WyrdError::ClientConfigInvalid {
                message: format!("telemetry flush failed: {error}"),
                details: json!({ "field": "telemetry" }),
            })
    }
}

/// Install the global `tracing` subscriber exporting spans to `state`'s server
/// as the state's client.
///
/// The subscriber carries one `tracing-opentelemetry` layer over an SDK
/// provider whose batch exporter is [`span_exporter`], so every export asks
/// the client for a fresh access token, plus a processor stamping spans
/// started inside `Run::scope` with that Run and Card. Only INFO-and-above
/// spans export, and only tracing is installed. A later call in the same
/// process returns the installed pipeline.
///
/// # Errors
/// Returns `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` when the application
/// already installed a global subscriber (add [`span_exporter`] to its own
/// pipeline instead), the credential error when the state's client cannot
/// resolve, and `WYRD_CLIENT_400_CONFIG_INVALID` when the exporter cannot be
/// built, such as outside a Tokio runtime.
pub fn start_telemetry(state: &WyrdState) -> Result<Telemetry, WyrdError> {
    if let Some(provider) = INSTALLED.get() {
        return Ok(Telemetry {
            provider: provider.clone(),
        });
    }
    let exporter =
        span_exporter(state.client()?).map_err(|error| WyrdError::ClientConfigInvalid {
            message: format!("the telemetry span exporter cannot be built: {error}"),
            details: json!({ "field": "telemetry" }),
        })?;
    let provider = SdkTracerProvider::builder()
        .with_span_processor(RunCorrelation)
        .with_batch_exporter(exporter)
        .build();
    let layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("wyrd"));
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(layer),
    )
    .map_err(|_| WyrdError::SdkTelemetryProviderExists {
        message: "a global tracing subscriber is already installed".to_owned(),
        details: json!({}),
    })?;
    let _ = INSTALLED.set(provider.clone());
    Ok(Telemetry { provider })
}

/// Stamps the innermost `Run::scope` on every span started inside it.
#[derive(Debug)]
struct RunCorrelation;

impl SpanProcessor for RunCorrelation {
    /// Copy the current task's Run scope onto `span` as `wyrd.card_ref` and
    /// `wyrd.run_id`; a span started outside every scope is unchanged.
    fn on_start(&self, span: &mut Span, _cx: &Context) {
        use opentelemetry::trace::Span as _;
        if let Some((card_ref, run_id)) = current_run_scope() {
            span.set_attribute(KeyValue::new("wyrd.card_ref", card_ref));
            span.set_attribute(KeyValue::new("wyrd.run_id", run_id));
        }
    }

    /// Nothing to do: the batch processor exports the ended span.
    fn on_end(&self, _span: SpanData) {}

    /// Nothing buffered here.
    ///
    /// # Errors
    /// Never fails.
    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    /// Nothing to release here.
    ///
    /// # Errors
    /// Never fails.
    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }
}

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
