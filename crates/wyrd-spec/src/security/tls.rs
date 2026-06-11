//! TLS configuration for transport connections.
//!
//! Certificate material is referenced through `SecretRef`; it is never stored
//! inline in production configuration.

use serde::{Deserialize, Serialize};

use crate::security::SecretRef;

/// TLS configuration for a transport connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    /// PEM-encoded CA certificate bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_cert: Option<SecretRef>,

    /// PEM-encoded client certificate for mTLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<SecretRef>,

    /// PEM-encoded client private key for mTLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<SecretRef>,

    /// Override the server name used in SNI and certificate verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name_override: Option<String>,

    /// Disable peer certificate verification. Defaults to `false`.
    #[serde(default)]
    pub insecure_skip_verify: bool,
}
