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

/// Builds an endpoint that both authenticates the server and presents a client identity.
///
/// This is the mutual-TLS form of [`authenticated_tls_endpoint`]: in addition to
/// pinning the server chain to `ca_certificate_pem` and requiring `server_name`
/// on the presented certificate, the returned endpoint offers
/// `client_certificate_chain_pem`/`client_private_key_pem` when the peer
/// requests a client certificate. Callers still own application authentication;
/// the certificate only admits the transport.
///
/// # Errors
///
/// Returns [`EndpointBuildError::CryptoProvider`] when another Rustls provider
/// already owns the process, or [`EndpointBuildError::Transport`] when the
/// endpoint URI, CA material, or client identity is invalid.
pub fn mutually_authenticated_tls_endpoint(
    address: String,
    ca_certificate_pem: &[u8],
    server_name: String,
    client_certificate_chain_pem: &[u8],
    client_private_key_pem: &[u8],
) -> Result<Endpoint, EndpointBuildError> {
    wyrd_tls::install_crypto_provider()?;
    Ok(Endpoint::from_shared(address)?.tls_config(
        ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_certificate_pem))
            .domain_name(server_name)
            .identity(tonic::transport::Identity::from_pem(
                client_certificate_chain_pem,
                client_private_key_pem,
            )),
    )?)
}
