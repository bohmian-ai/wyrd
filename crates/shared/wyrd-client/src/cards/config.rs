//! Construction-time client configuration resolution.

use crate::GlobalConfig;
use crate::WyrdClient;
use crate::config::ClientConfig;
use crate::environment::Environment;
use crate::error::WyrdClientError;
use secrecy::SecretString;

use crate::cards::error::RegistryEngineError;

/// Resolve the shared Wyrd client for a registry handle from `environment`,
/// applying the explicit credential over the configured one.
///
/// Repository filesystem configuration is intentionally not loaded here.
/// `wyrd-loader` owns authored-card and `wyrd.toml` discovery; this module only
/// assembles the authenticated network client.
///
/// # Errors
///
/// Returns [`RegistryEngineError::Client`] when the global client
/// configuration cannot be read or parsed, when an explicit `server_url` is
/// empty, when the saved-login selection fails, or when the authenticated
/// transport cannot be constructed.
pub(crate) fn load(
    environment: Environment,
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
    let global = GlobalConfig::load_in(&environment)?;
    let mut client_config = ClientConfig::from_environment(environment, &global, server_url, None);
    if credential.is_some() {
        client_config.credential = credential;
    }
    Ok(WyrdClient::with_config(client_config)?)
}
