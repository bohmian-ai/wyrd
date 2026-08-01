//! Shared construction mechanics for Wyrd-owned tonic transports.

use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

/// Failures while constructing a Wyrd-owned tonic endpoint.
#[derive(Debug, thiserror::Error)]
pub enum EndpointBuildError {
    /// Process-wide Rustls provider ownership conflicts with Wyrd.
    #[error(transparent)]
    CryptoProvider(#[from] wyrd_tls::InstallError),
    /// Tonic rejected the endpoint URI or TLS configuration.
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
}

/// Builds an endpoint that authenticates a server against one PEM CA and DNS name.
///
/// The process AWS-LC provider is installed before tonic constructs any Rustls
/// state. Callers retain ownership of application authentication and retry policy.
///
/// # Errors
///
/// Returns [`EndpointBuildError::CryptoProvider`] when another Rustls provider
/// already owns the process, or [`EndpointBuildError::Transport`] when the
/// endpoint URI or TLS configuration is invalid.
pub fn authenticated_tls_endpoint(
    address: String,
    ca_certificate_pem: &[u8],
    server_name: String,
) -> Result<Endpoint, EndpointBuildError> {
    wyrd_tls::install_crypto_provider()?;
    Ok(Endpoint::from_shared(address)?.tls_config(
        ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_certificate_pem))
            .domain_name(server_name),
    )?)
}

/// Builds a plaintext development endpoint after installing the process provider.
///
/// Installing the provider here keeps later TLS use deterministic even when a
/// development caller creates a plaintext channel first.
///
/// # Errors
///
/// Returns [`EndpointBuildError::CryptoProvider`] when another Rustls provider
/// already owns the process, or [`EndpointBuildError::Transport`] when the
/// endpoint URI is invalid.
pub fn plaintext_endpoint(address: String) -> Result<Endpoint, EndpointBuildError> {
    wyrd_tls::install_crypto_provider()?;
    Ok(Endpoint::from_shared(address)?)
}
