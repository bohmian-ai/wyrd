//! Generic authed gRPC channel.
//!
//! [`GrpcConnection`] dials the endpoint through the `wyrd-tonic` transport
//! re-export, applies HTTP/2 keepalive and max-message limits, and holds the
//! shared [`AuthMiddleware`]. It names no typed gRPC service — the
//! kind-specific sink builds its client from [`GrpcConnection::channel`].

use std::{sync::Arc, time::Duration};

use wyrd_tonic::tonic::transport::{Channel, ClientTlsConfig, Endpoint};

use crate::{auth::AuthMiddleware, error::WyrdClientError, transport::config::GrpcConfig};

/// Generic, service-agnostic authed gRPC connection.
///
/// Dials the endpoint from [`GrpcConfig`], applies HTTP/2 keepalive and
/// max-message-size limits, and carries a shared [`AuthMiddleware`] for
/// per-call bearer injection. Names no typed gRPC service — the sink builds
/// its client from [`channel()`][GrpcConnection::channel].
///
/// `Channel` is a clone-cheap tonic handle to the underlying HTTP/2
/// connection. Cloning `GrpcConnection` does **not** redial.
#[derive(Clone)]
pub struct GrpcConnection {
    channel: Channel,
    auth: Arc<AuthMiddleware>,
    max_message_bytes: usize,
}

impl std::fmt::Debug for GrpcConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrpcConnection")
            .field("auth", &self.auth)
            .field("max_message_bytes", &self.max_message_bytes)
            .finish_non_exhaustive()
    }
}

impl GrpcConnection {
    /// Dial the endpoint configured in `config`, applying keepalive and
    /// max-message limits, and return a connected [`GrpcConnection`].
    ///
    /// The connection is multiplexed — call [`channel()`][Self::channel] to
    /// hand the handle to a service client; do not redial per call. If the
    /// initial dial fails, the call is retried up to `config.connect_retries`
    /// additional times, with exponential backoff between attempts (the same
    /// 100 ms → 1 s → 5 s schedule the HTTP retry loop uses).
    ///
    /// Status divergence from the HTTP transport: an unreachable server surfaces
    /// here, at connection-establish time, as [`WyrdClientError::TransportDown`] (503).
    /// The HTTP transport has no separate connect phase, so it surfaces the same
    /// condition per-request as [`WyrdError::Internal`][wyrd_spec::error::WyrdError::Internal]
    /// (500). The divergence is intentional and documented in both modules.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
    /// already owns the process, the endpoint URI is invalid, or the dial fails
    /// (DNS, TCP, or TLS) on all attempts. Cancellation stops retries and drops
    /// the in-progress channel without retaining a connection.
    pub async fn connect(
        config: &GrpcConfig,
        auth: Arc<AuthMiddleware>,
    ) -> Result<Self, WyrdClientError> {
        let endpoint = build_endpoint(config)?;

        let max_attempts = 1u32.saturating_add(config.connect_retries);
        let mut last_err = None;
        for attempt in 0..max_attempts {
            match endpoint.connect().await {
                Ok(channel) => {
                    return Ok(Self {
                        channel,
                        auth,
                        max_message_bytes: config.max_message_bytes,
                    });
                }
                Err(err) => {
                    last_err = Some(err);
                    // Back off before the next attempt, but never after the last.
                    if attempt + 1 < max_attempts {
                        tokio::time::sleep(Duration::from_millis(crate::transport::backoff_ms(
                            attempt,
                        )))
                        .await;
                    }
                }
            }
        }
        Err(WyrdClientError::TransportDown {
            transport: "grpc".to_owned(),
            message: last_err.expect("at least one attempt was made").to_string(),
        })
    }

    /// Return a clone of the underlying [`Channel`].
    ///
    /// Tonic `Channel` is a multiplexed HTTP/2 handle — cloning it is O(1)
    /// and does **not** redial. Pass this to the service client constructor.
    #[must_use]
    pub fn channel(&self) -> Channel {
        self.channel.clone()
    }

    /// Return a clone of the shared [`AuthMiddleware`] handle.
    ///
    /// Use the returned handle to obtain the current bearer token via
    /// [`AuthMiddleware::bearer`] and inject it as per-call gRPC metadata
    /// alongside the `wyrd-request-id` spine header.
    #[must_use]
    pub fn auth(&self) -> Arc<AuthMiddleware> {
        Arc::clone(&self.auth)
    }

    /// Return the configured maximum gRPC message size in bytes.
    ///
    /// Apply this as `max_decoding_message_size` and
    /// `max_encoding_message_size` on the service client built from
    /// [`channel()`][Self::channel]. Must match the server's cap so large
    /// ingest batches do not fail decode.
    #[must_use]
    pub fn max_message_bytes(&self) -> usize {
        self.max_message_bytes
    }
}

/// Build and configure an [`Endpoint`] from [`GrpcConfig`].
///
/// Applies per-call timeout, connect timeout, HTTP/2 keepalive settings, and
/// TLS when the endpoint scheme is `https://`.
///
/// Kept separate so the retry loop in [`GrpcConnection::connect`] can reuse
/// it without repeating validation logic.
///
/// # Errors
///
/// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
/// already owns the process, the endpoint URI is invalid, or its TLS
/// configuration cannot be constructed.
fn build_endpoint(config: &GrpcConfig) -> Result<Endpoint, WyrdClientError> {
    wyrd_tls::install_crypto_provider().map_err(|error| WyrdClientError::TransportDown {
        transport: "grpc".to_owned(),
        message: error.to_string(),
    })?;
    let endpoint = Endpoint::from_shared(config.endpoint.clone()).map_err(|err| {
        WyrdClientError::TransportDown {
            transport: "grpc".to_owned(),
            message: format!("invalid endpoint URI: {err}"),
        }
    })?;

    let timeout = Duration::from_millis(config.timeout_ms);
    let endpoint = endpoint.timeout(timeout).connect_timeout(timeout);

    let endpoint = if config.keepalive_interval_ms > 0 {
        endpoint
            .http2_keep_alive_interval(Duration::from_millis(config.keepalive_interval_ms))
            .keep_alive_timeout(Duration::from_millis(config.keepalive_timeout_ms))
            .keep_alive_while_idle(true)
    } else {
        endpoint
    };

    if config.endpoint.starts_with("https://") {
        endpoint
            .tls_config(ClientTlsConfig::new())
            .map_err(|err| WyrdClientError::TransportDown {
                transport: "grpc".to_owned(),
                message: format!("TLS config error: {err}"),
            })
    } else {
        Ok(endpoint)
    }
}
