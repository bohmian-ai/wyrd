//! TLS configuration for transport connections.
//!
//! `TlsConfig` covers mutual-TLS (mTLS) scenarios (CA + client cert + key) as
//! well as one-way TLS (CA override only). All certificate material is
//! referenced via `SecretRef`, never embedded inline — except in tests via
//! `SecretRef::Inline`.

use serde::{Deserialize, Serialize};

use crate::security::secret_ref::SecretRef;

/// TLS configuration for a transport connection.
///
/// All certificate fields are optional; an absent field means "use the system
/// default / skip that verification step". Setting `insecure_skip_verify` to
/// `true` disables peer certificate validation and SHOULD NOT be used outside
/// local development.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    /// PEM-encoded CA certificate bundle. When set, overrides the system CA
    /// pool for this connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_cert: Option<SecretRef>,

    /// PEM-encoded client certificate for mTLS. Must be paired with
    /// `client_key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<SecretRef>,

    /// PEM-encoded client private key for mTLS. Must be paired with
    /// `client_cert`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<SecretRef>,

    /// Override the server name used in SNI and certificate verification. Useful
    /// when the server's certificate CN does not match the address in `endpoint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name_override: Option<String>,

    /// Disable peer certificate verification. Defaults to `false`.
    ///
    /// **WARNING:** setting this to `true` opens the connection to
    /// man-in-the-middle attacks. Use only in local development.
    #[serde(default)]
    pub insecure_skip_verify: bool,
}
