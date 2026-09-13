//! Construction-time client configuration resolution.

use crate::WyrdClient;
use crate::config::ClientConfig;
use crate::error::WyrdClientError;
use secrecy::SecretString;

use crate::cards::error::RegistryEngineError;

/// Resolve the shared Wyrd client for a registry handle.
///
/// Repository filesystem configuration is intentionally not loaded here.
/// `wyrd-loader` owns authored-card and `wyrd.toml` discovery; this module only
/// assembles the authenticated network client.
pub(crate) fn load(
    server_url: Option<&str>,
    api_key: Option<SecretString>,
) -> Result<WyrdClient, RegistryEngineError> {
    let mut client_config = ClientConfig::from_global()?;
    if let Some(server_url) = server_url {
        if server_url.is_empty() {
            return Err(WyrdClientError::Config {
                field: "server_url".to_owned(),
                reason: "must not be empty".to_owned(),
            }
            .into());
        }
        client_config.http.base_url = server_url.to_owned();
    }
    if api_key.is_some() {
        client_config.credential = api_key;
    }
    Ok(WyrdClient::with_config(client_config)?)
}
