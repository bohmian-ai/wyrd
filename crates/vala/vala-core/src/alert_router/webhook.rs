//! Server-internal webhook configuration consumed by the alert router.
//!
//! `WebhookConfig` is the typed config for signed-HTTP webhook delivery.
//! The signer that consumes it lands in `vala-core::alert_router::signer`.
//!
//! ## Signing protocol
//!
//! Canonical signing string:
//! ```text
//! v1:{timestamp}:{body_sha256_hex}
//! ```
//! where `{timestamp}` is the Unix timestamp in seconds and
//! `{body_sha256_hex}` is the lowercase hex-encoded SHA-256 of the serialized
//! JSON request body bytes. The signature is
//! `HMAC-SHA256(secret, canonical_string)` encoded as lowercase hex.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wyrd_spec::security::{SecretRef, TlsConfig};

/// Default header name used by [`WebhookConfig`] for the hex-encoded HMAC.
pub const WEBHOOK_SIGNATURE_HEADER_DEFAULT: &str = "X-Wyrd-Signature";
/// Default header name used by [`WebhookConfig`] for the Unix timestamp.
pub const WEBHOOK_TIMESTAMP_HEADER_DEFAULT: &str = "X-Wyrd-Timestamp";
/// Default per-request timeout, in milliseconds, used by [`WebhookConfig`].
pub const WEBHOOK_DEFAULT_TIMEOUT_MS: u64 = 10_000;
/// Locked signing-string version tag.
pub const WEBHOOK_SIGNING_VERSION: &str = "v1";

/// HTTP method for webhook delivery.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum WebhookMethod {
    /// HTTP POST.
    #[default]
    Post,
    /// HTTP PUT.
    Put,
}

/// Configuration for a signed HTTP webhook destination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebhookConfig {
    /// Destination URL. Must be non-empty.
    #[serde(default)]
    pub url: String,

    /// HTTP method for delivery.
    #[serde(default)]
    pub method: WebhookMethod,

    /// HMAC-SHA256 signing key. `None` disables signing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<SecretRef>,

    /// Header name that carries the hex-encoded HMAC-SHA256 signature.
    #[serde(default = "default_signature_header")]
    pub signature_header: String,

    /// Header name that carries the Unix timestamp used by the signing string.
    #[serde(default = "default_timestamp_header")]
    pub timestamp_header: String,

    /// Request timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Non-sensitive static HTTP headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,

    /// Sensitive HTTP headers whose values resolve from the runtime secret store.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secret_headers: BTreeMap<String, SecretRef>,

    /// Optional TLS configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
}

const HEADER_NAME_FULL_DENYLIST: &[&str] = &[
    "authorization",
    "cookie",
    "x-api-key",
    "x-amz-security-token",
];

const HEADER_NAME_SUFFIX_DENYLIST: &[&str] = &["-auth", "-token", "-key", "-secret", "-credential"];

fn header_name_is_secret_shaped(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    HEADER_NAME_FULL_DENYLIST.iter().any(|deny| lower == *deny)
        || HEADER_NAME_SUFFIX_DENYLIST
            .iter()
            .any(|suffix| lower.ends_with(suffix))
}

fn default_signature_header() -> String {
    WEBHOOK_SIGNATURE_HEADER_DEFAULT.to_string()
}

fn default_timestamp_header() -> String {
    WEBHOOK_TIMESTAMP_HEADER_DEFAULT.to_string()
}

fn default_timeout_ms() -> u64 {
    WEBHOOK_DEFAULT_TIMEOUT_MS
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: WebhookMethod::Post,
            secret_ref: None,
            signature_header: WEBHOOK_SIGNATURE_HEADER_DEFAULT.to_string(),
            timestamp_header: WEBHOOK_TIMESTAMP_HEADER_DEFAULT.to_string(),
            timeout_ms: WEBHOOK_DEFAULT_TIMEOUT_MS,
            headers: BTreeMap::new(),
            secret_headers: BTreeMap::new(),
            tls: None,
        }
    }
}

/// Local validation error for alert-router config types.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AlertConfigError {
    /// A field violated a structural rule.
    #[error("alert config invalid: {field}: {reason}")]
    Invalid {
        /// Dotted-path field name.
        field: String,
        /// Short human-readable reason.
        reason: String,
    },
}

impl WebhookConfig {
    /// Validate the webhook config.
    ///
    /// # Errors
    /// Returns [`AlertConfigError::Invalid`] when a required value is empty,
    /// the timeout is zero, or a static header could leak credential material.
    pub fn validate(&self) -> Result<(), AlertConfigError> {
        if self.url.is_empty() {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.url".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if self.signature_header.is_empty() {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.signature_header".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if self.timestamp_header.is_empty() {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.timestamp_header".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if self.timeout_ms == 0 {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.timeout_ms".to_string(),
                reason: "must be at least 1".to_string(),
            });
        }

        let signature_header = self.signature_header.to_ascii_lowercase();
        let timestamp_header = self.timestamp_header.to_ascii_lowercase();
        if signature_header == timestamp_header {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.timestamp_header".to_string(),
                reason: "must differ from webhook_config.signature_header".to_string(),
            });
        }

        for name in self.headers.keys() {
            let lower = name.to_ascii_lowercase();
            if header_name_is_secret_shaped(name) {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.headers.{name}"),
                    reason: "secret-shaped header name; use webhook_config.secret_headers for credential values".to_string(),
                });
            }
            if lower == signature_header || lower == timestamp_header {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.headers.{name}"),
                    reason: "collides with the signing or timestamp header".to_string(),
                });
            }
        }

        for name in self.secret_headers.keys() {
            let lower = name.to_ascii_lowercase();
            if lower == signature_header || lower == timestamp_header {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.secret_headers.{name}"),
                    reason: "collides with the signing or timestamp header".to_string(),
                });
            }
        }

        for name in self.headers.keys() {
            let lower = name.to_ascii_lowercase();
            if self
                .secret_headers
                .keys()
                .any(|key| key.to_ascii_lowercase() == lower)
            {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.headers.{name}"),
                    reason: "appears in both `headers` and `secret_headers`".to_string(),
                });
            }
        }

        Ok(())
    }
}
