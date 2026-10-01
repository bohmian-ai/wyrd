//! Construction-time client configuration resolution.

use crate::GlobalConfig;
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
///
/// # Errors
///
/// Returns [`RegistryEngineError::Client`] when the global client
/// configuration cannot be read or parsed, when an explicit `server_url` is
/// empty, or when the authenticated transport cannot be constructed.
pub(crate) fn load(
    server_url: Option<&str>,
    credential: Option<SecretString>,
) -> Result<WyrdClient, RegistryEngineError> {
    if server_url.is_some_and(str::is_empty) {
        return Err(WyrdClientError::Config {
            field: "server_url".to_owned(),
            reason: "must not be empty".to_owned(),
        }
        .into());
    }
    let mut client_config =
        ClientConfig::from_global_with_overrides(&GlobalConfig::load()?, server_url, None);
    if credential.is_some() {
        client_config.credential = credential;
    }
    Ok(WyrdClient::with_config(client_config)?)
}
