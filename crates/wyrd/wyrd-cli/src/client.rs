//! The CLI's one construction of the shared Wyrd client.
//!
//! Every command that calls a Wyrd route reaches the server through
//! [`wyrd_client`]: the CLI owns no transport, assembles no authentication
//! header, and maps no status code of its own. A command that needs something
//! the shared client cannot express is evidence the client is missing a
//! capability, not licence for the command to hand-roll one.
//!
//! Commands take `--server` and `--token` because an operator administering a
//! deployment is usually not the workload that lives in it: the ambient
//! credential chain [`ClientConfig::resolve_credential`] reads is the right
//! default for `wyrd apply`, and the wrong one for pointing a recovery command
//! at a second deployment. The token is passed as an explicit credential, which
//! the client classifies — an API key is exchanged, an access token is presented
//! verbatim — so an operator never has to say which kind they hold.

use std::sync::Arc;

use secrecy::SecretString;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::{HttpConfig, HttpTransport};
use wyrd_client::{Principals, WyrdClient};

use crate::error::WyrdCliError;

/// Build an authenticated client for one deployment and one operator token.
///
/// # Errors
/// Returns [`WyrdCliError::ClientConfig`] for a rejected endpoint and
/// [`WyrdCliError::ClientTransport`] when the HTTP stack cannot be assembled.
pub fn client(server: &str, token: &str) -> Result<WyrdClient, WyrdCliError> {
    assemble(ClientConfig {
        http: HttpConfig {
            base_url: server.trim_end_matches('/').to_owned(),
            ..HttpConfig::default()
        },
        credential: Some(SecretString::from(token.to_owned())),
        ..ClientConfig::default()
    })
}

/// Build a client from the ambient configuration, optionally re-pointed.
///
/// This is the default for a command that administers the deployment the user
/// is already configured against — `wyrd apply`, `wyrd card get` — where the
/// credential comes from the same chain the SDKs read. `server` overrides only
/// the endpoint, so `--server` can aim an otherwise ambient invocation
/// elsewhere.
///
/// # Errors
/// Returns [`WyrdCliError::NoCredentials`] when the chain yields nothing,
/// [`WyrdCliError::ClientConfig`] for a rejected endpoint, and
/// [`WyrdCliError::ClientTransport`] when the HTTP stack cannot be assembled.
pub fn from_global(server: Option<&str>) -> Result<WyrdClient, WyrdCliError> {
    let mut config = ClientConfig::from_global().map_err(map_client_error)?;
    if let Some(server) = server {
        config.http.base_url = server.to_owned();
    }
    assemble(config)
}

/// Validate one configuration and stack the client layers over it.
///
/// The single place the CLI walks the shared client's assembly ladder:
/// validate the endpoint, resolve the credential, wrap it in the token
/// middleware, and give the transport that middleware to authenticate with.
///
/// # Errors
/// Returns [`WyrdCliError::NoCredentials`], [`WyrdCliError::ClientConfig`], or
/// [`WyrdCliError::ClientTransport`] as the failing rung dictates.
fn assemble(config: ClientConfig) -> Result<WyrdClient, WyrdCliError> {
    config.http.validate().map_err(map_client_error)?;
    let credential = config.resolve_credential().map_err(map_client_error)?;
    let auth = AuthMiddleware::new(&config, credential).map_err(map_client_error)?;
    let http = HttpTransport::new(&config.http, Arc::clone(&auth)).map_err(map_client_error)?;
    Ok(WyrdClient::from_parts(auth, http, config.grpc))
}

/// Build the tenant principal-administration handle for one operator token.
///
/// # Errors
/// Returns the same construction errors as [`client`].
pub fn principals(server: &str, token: &str) -> Result<Principals, WyrdCliError> {
    Ok(Principals::with_client(client(server, token)?))
}

/// Project a client-assembly failure onto the CLI's local error catalog.
///
/// Only construction reaches here. A failure of the request itself is already a
/// stable [`wyrd_spec::error::WyrdError`] from the shared client and is reported
/// as one, never re-mapped from a status code.
pub fn map_client_error(error: WyrdClientError) -> WyrdCliError {
    match error {
        WyrdClientError::NoCredentials => WyrdCliError::NoCredentials,
        WyrdClientError::Config { field, reason } => WyrdCliError::ClientConfig {
            detail: format!("{field}: {reason}"),
        },
        WyrdClientError::TransportDown { transport, message } => WyrdCliError::ClientTransport {
            detail: format!("{transport}: {message}"),
        },
    }
}
