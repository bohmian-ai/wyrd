//! Server-internal webhook configuration consumed by the alert router.
//!
//! `WebhookConfig` is the typed config for signed-HTTP webhook delivery.
//! `vala-core::alert_router::signer` (PR4.3) uses it to sign and deliver
//! alert notifications to external receivers.
//!
//! ## Ownership note
//!
//! This type is server-internal: it ships in `vala-core`, not `wyrd-spec`,
//! and is not Python-visible. A future observability phase will introduce
//! a public `AlertCard` / `AlertSpec`; at that point this type migrates
//! to `wyrd_spec::observability::alerts` and the Python wrapper lands.
//! Until then, alert routes are configured by Wyrd admins via server
//! config files, not via Wyrd client SDKs.
//!
//! `SecretRef` and `TlsConfig` are imported from `wyrd_spec::security`
//! because both the external client (`wyrd-client::transport`) and this
//! server-internal module share the same durable security primitives.
//!
//! ## Signing protocol (locked in this type, implemented in PR4.3)
//!
//! Canonical signing string:
//! ```text
//! v1:{timestamp}:{body_sha256_hex}
//! ```
//! where:
//! - `{timestamp}` is the Unix timestamp (seconds) at delivery time, carried
//!   in `timestamp_header`.
//! - `{body_sha256_hex}` is the lowercase hex-encoded SHA-256 of the JSON
//!   request body bytes (post-serialize, pre-compression).
//!
//! Signature: `HMAC-SHA256(secret, canonical_string)` -> lowercase hex ->
//! `signature_header`. The receiver verifies by recomputing the canonical
//! string and comparing the hex-encoded HMAC. The `v1:` prefix lets
//! receivers detect the signing version and reject unknown formats.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wyrd_spec::security::{SecretRef, TlsConfig};

/// Default header name used by [`WebhookConfig`] for the hex-encoded HMAC.
pub const WEBHOOK_SIGNATURE_HEADER_DEFAULT: &str = "X-Wyrd-Signature";
/// Default header name used by [`WebhookConfig`] for the Unix timestamp.
pub const WEBHOOK_TIMESTAMP_HEADER_DEFAULT: &str = "X-Wyrd-Timestamp";
/// Default per-request timeout in milliseconds used by [`WebhookConfig`].
pub const WEBHOOK_DEFAULT_TIMEOUT_MS: u64 = 10_000;
/// Locked signing-string version tag. The signer uses this exact byte sequence.
pub const WEBHOOK_SIGNING_VERSION: &str = "v1";

/// HTTP method for webhook delivery.
///
/// Wire representation is uppercase HTTP verb: `"POST"` or `"PUT"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum WebhookMethod {
    /// HTTP POST.
    Post,
    /// HTTP PUT.
    Put,
}

/// Configuration for a signed HTTP webhook destination.
///
/// Used by `crate::alert_router::signer` (PR4.3) to deliver alert
/// notifications to external receivers. The signing protocol is HMAC-SHA256
/// over a canonical string; see the module-level doc for the exact format.
///
/// When `secret_ref` is `None`, the webhook is delivered unsigned. Unsigned
/// delivery is only recommended for internal endpoints that enforce
/// network-level isolation.
///
/// # Default
///
/// `WebhookConfig::default()` returns a config with the canonical Wyrd
/// header names, `WEBHOOK_DEFAULT_TIMEOUT_MS`, no `secret_ref`, no `tls`,
/// and an empty `headers` map. The `url` field defaults to an empty
/// string: the `Default` impl is a starting point for builders and tests,
/// not a deployable config. `WebhookConfig::default().validate()` returns
/// `Err` because of the empty URL, by design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebhookConfig {
    /// Destination URL. Must be non-empty.
    ///
    /// Example: `"https://hooks.example.com/wyrd-alerts"`.
    ///
    /// The `Default` impl leaves this empty so callers explicitly set it.
    /// `validate()` rejects an empty URL.
    #[serde(default)]
    pub url: String,

    /// HTTP method for delivery. Typically `Post`; some receivers require
    /// `Put`.
    ///
    /// Default: `WebhookMethod::Post`.
    #[serde(default)]
    pub method: WebhookMethod,

    /// HMAC-SHA256 signing key. The signature is carried on the header named
    /// by [`signature_header`](Self::signature_header). `None` disables
    /// signing, which is recommended only for internal endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<SecretRef>,

    /// Name of the HTTP header that carries the hex-encoded HMAC-SHA256
    /// signature.
    ///
    /// Default: [`WEBHOOK_SIGNATURE_HEADER_DEFAULT`] (`"X-Wyrd-Signature"`).
    #[serde(default = "default_signature_header")]
    pub signature_header: String,

    /// Name of the HTTP header that carries the Unix timestamp in seconds
    /// used in the canonical signing string.
    ///
    /// Default: [`WEBHOOK_TIMESTAMP_HEADER_DEFAULT`] (`"X-Wyrd-Timestamp"`).
    #[serde(default = "default_timestamp_header")]
    pub timestamp_header: String,

    /// Request timeout in milliseconds.
    ///
    /// Default: [`WEBHOOK_DEFAULT_TIMEOUT_MS`] (`10_000`, or 10 s).
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Non-sensitive static HTTP headers to include on every request. The
    /// map is ordered (`BTreeMap`) for deterministic serialization.
    ///
    /// **Plaintext secrets MUST NOT be stored here.** `validate()` rejects
    /// the config if any entry collides with `Authorization`, `Cookie`,
    /// the configured `signature_header`, the configured `timestamp_header`,
    /// or carries a secret-shaped name (case-insensitive match against
    /// `authorization`, `cookie`, names ending in `-auth`, `-token`,
    /// `-key`, `-secret`, `-credential`, plus `x-api-key`,
    /// `x-amz-security-token`). Header-name comparison is
    /// case-insensitive. Use [`secret_headers`](Self::secret_headers) for
    /// credential values so resolution happens at send time via the Wyrd
    /// auth shell and is never serialized to disk.
    ///
    /// Examples of values that belong here: `Content-Type` overrides,
    /// `User-Agent`, `X-App-Name`, `X-Request-Id` correlation headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,

    /// Sensitive HTTP headers whose values resolve from the runtime secret
    /// store. The signer (PR4.3) resolves each `SecretRef` via
    /// `wyrd_auth::resolve` immediately before sending the request and
    /// never persists the plaintext. The key is the header name. The same
    /// case-insensitive signing/timestamp collision rules as
    /// [`headers`](Self::headers) apply, but `Authorization` is permitted
    /// here because that is exactly the case `SecretRef` indirection is
    /// designed for. The value is the secret reference.
    ///
    /// `validate()` rejects duplicate keys across `headers` and
    /// `secret_headers` so callers cannot accidentally shadow a credential
    /// with a plaintext value.
    ///
    /// Default: empty. The field is omitted from serialized output when
    /// empty (`skip_serializing_if`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secret_headers: BTreeMap<String, SecretRef>,

    /// Optional TLS configuration. When `None`, the scheme in `url` governs
    /// TLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
}

/// Case-insensitive header name denylist for `WebhookConfig.headers`.
/// Plaintext values for any of these header names are rejected by
/// `WebhookConfig::validate`; sensitive headers belong in `secret_headers`.
///
/// `Authorization` and `Cookie` are full matches; the suffix forms catch
/// `*-Auth`, `*-Token`, `*-Key`, `*-Secret`, `*-Credential` patterns.
const HEADER_NAME_FULL_DENYLIST: &[&str] = &[
    "authorization",
    "cookie",
    "x-api-key",
    "x-amz-security-token",
];

const HEADER_NAME_SUFFIX_DENYLIST: &[&str] = &["-auth", "-token", "-key", "-secret", "-credential"];

/// Returns `true` if `name` matches the plaintext-headers denylist.
///
/// Matching is case-insensitive. This is pure and IO-free.
pub(crate) fn header_name_is_secret_shaped(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if HEADER_NAME_FULL_DENYLIST.iter().any(|&deny| lower == deny) {
        return true;
    }
    HEADER_NAME_SUFFIX_DENYLIST
        .iter()
        .any(|&suffix| lower.ends_with(suffix))
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

impl Default for WebhookMethod {
    /// Returns `WebhookMethod::Post`, the common verb for signed webhook
    /// delivery.
    fn default() -> Self {
        Self::Post
    }
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
///
/// This is a server-internal error owned by `vala-core::alert_router`. It
/// does NOT impl `From<&AlertConfigError> for WyrdError` because
/// `WebhookConfig` is server-internal in this phase and no public route
/// surfaces it. PR4.3 (admin API for alert routing) is responsible for
/// mapping this error to the reserved public catalog code
/// `WYRD_VALA_400_ALERT_CONFIG_INVALID` at the route boundary.
///
/// `WYRD_SPEC_400_VALIDATION` is not used: that code is reserved for genuine
/// `wyrd-spec` contract violations (cards, specs, security refs). Webhook
/// configuration is not a `wyrd-spec` contract.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AlertConfigError {
    /// A field violated a structural rule. `field` is a dotted path and
    /// `reason` is a short human description. The boundary mapper in PR4.3
    /// forwards both verbatim into the catalog payload.
    #[error("alert config invalid: {field}: {reason}")]
    Invalid {
        /// Dotted-path field name, e.g. `"webhook_config.url"`.
        field: String,
        /// Short human reason, e.g. `"must not be empty"`.
        reason: String,
    },
}

impl WebhookConfig {
    /// Validate the config.
    ///
    /// Returns `Err` if any of:
    ///
    /// - `url` is empty
    /// - `signature_header` is empty
    /// - `timestamp_header` is empty
    /// - `timeout_ms == 0`
    /// - any `headers` key matches the secret-shaped denylist
    ///   (case-insensitive) or collides with the configured
    ///   `signature_header` / `timestamp_header` (case-insensitive)
    /// - any `secret_headers` key collides with the configured
    ///   `signature_header` / `timestamp_header` (case-insensitive)
    /// - the same header name appears in both `headers` and
    ///   `secret_headers` (case-insensitive)
    ///
    /// All header-name comparisons are case-insensitive. Plaintext
    /// credentials in `headers` are the highest-priority failure: they
    /// must never reach disk.
    ///
    /// Returns the server-internal [`AlertConfigError`]. In this phase
    /// the error never reaches a public route. The alert router calls
    /// `validate()` at config load and bubbles failures into the server
    /// startup error path. PR4.3 will map this error to
    /// `WYRD_VALA_400_ALERT_CONFIG_INVALID` when the admin API for alert
    /// routing lands.
    pub fn validate(&self) -> Result<(), AlertConfigError> {
        if self.url.is_empty() {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.url".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if self.secret_ref.is_some() && !self.url.starts_with("https://") {
            return Err(AlertConfigError::Invalid {
                field: "webhook_config.url".to_string(),
                reason: "must use https:// when a signing secret is configured".to_string(),
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

        let sig_lower = self.signature_header.to_ascii_lowercase();
        let ts_lower = self.timestamp_header.to_ascii_lowercase();

        for name in self.headers.keys() {
            let lower = name.to_ascii_lowercase();
            if header_name_is_secret_shaped(name) {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.headers.{name}"),
                    reason: "secret-shaped header name; use webhook_config.secret_headers for credential values"
                        .to_string(),
                });
            }
            if lower == sig_lower || lower == ts_lower {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.headers.{name}"),
                    reason: "collides with the signing or timestamp header".to_string(),
                });
            }
            if self
                .secret_headers
                .keys()
                .any(|k| k.to_ascii_lowercase() == lower)
            {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.headers.{name}"),
                    reason: "appears in both `headers` and `secret_headers`".to_string(),
                });
            }
        }

        // Authorization and similar credential names are permitted here
        // because SecretRef indirection is designed for that case.
        for name in self.secret_headers.keys() {
            let lower = name.to_ascii_lowercase();
            if lower == sig_lower || lower == ts_lower {
                return Err(AlertConfigError::Invalid {
                    field: format!("webhook_config.secret_headers.{name}"),
                    reason: "collides with the signing or timestamp header".to_string(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod webhook_tests {
    use std::collections::BTreeMap;

    use crate::alert_router::webhook::{
        AlertConfigError, WEBHOOK_DEFAULT_TIMEOUT_MS, WEBHOOK_SIGNATURE_HEADER_DEFAULT,
        WEBHOOK_TIMESTAMP_HEADER_DEFAULT, WebhookConfig, WebhookMethod,
    };
    use wyrd_spec::security::SecretRef;

    fn base_config() -> WebhookConfig {
        WebhookConfig {
            url: "https://hooks.example.com/wyrd-alerts".to_string(),
            ..WebhookConfig::default()
        }
    }

    #[test]
    fn webhook_default_constants_match_struct_default() {
        let d = WebhookConfig::default();
        assert_eq!(d.method, WebhookMethod::Post);
        assert_eq!(d.signature_header, WEBHOOK_SIGNATURE_HEADER_DEFAULT);
        assert_eq!(d.timestamp_header, WEBHOOK_TIMESTAMP_HEADER_DEFAULT);
        assert_eq!(d.timeout_ms, WEBHOOK_DEFAULT_TIMEOUT_MS);
        assert!(d.secret_ref.is_none());
        assert!(d.tls.is_none());
        assert!(d.headers.is_empty());
        assert!(d.url.is_empty());
    }

    #[test]
    fn webhook_default_rejects_validation_because_url_is_empty() {
        let d = WebhookConfig::default();
        let err = d.validate().unwrap_err().to_string();
        assert_eq!(
            err,
            "alert config invalid: webhook_config.url: must not be empty"
        );
    }

    #[test]
    fn webhook_default_round_trips() {
        let d = WebhookConfig::default();
        let s = serde_json::to_string(&d).unwrap();
        let back: WebhookConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn webhook_deserialize_uses_serde_defaults_for_missing_fields() {
        let h: WebhookConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(h, WebhookConfig::default());
    }

    #[test]
    fn webhook_method_post_round_trips() {
        let s = serde_json::to_string(&WebhookMethod::Post).unwrap();
        assert_eq!(s, "\"POST\"");
        let back: WebhookMethod = serde_json::from_str(&s).unwrap();
        assert_eq!(back, WebhookMethod::Post);
    }

    #[test]
    fn webhook_method_put_round_trips() {
        let s = serde_json::to_string(&WebhookMethod::Put).unwrap();
        assert_eq!(s, "\"PUT\"");
        let back: WebhookMethod = serde_json::from_str(&s).unwrap();
        assert_eq!(back, WebhookMethod::Put);
    }

    #[test]
    fn webhook_config_round_trips() {
        let c = base_config();
        let s = serde_json::to_string(&c).unwrap();
        let back: WebhookConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn webhook_config_with_secret_ref_round_trips() {
        let c = WebhookConfig {
            secret_ref: Some(SecretRef::Env {
                name: "WEBHOOK_SECRET".to_string(),
            }),
            ..base_config()
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: WebhookConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn webhook_config_headers_map_round_trips() {
        let mut headers = BTreeMap::new();
        headers.insert("X-App-Name".to_string(), "wyrd-alerts".to_string());
        headers.insert("X-Version".to_string(), "1".to_string());
        let c = WebhookConfig {
            headers,
            ..base_config()
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: WebhookConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
        assert!(s.contains("X-App-Name"));
    }

    #[test]
    fn webhook_config_default_signature_header() {
        let c = base_config();
        assert_eq!(c.signature_header, "X-Wyrd-Signature");
        assert_eq!(c.timestamp_header, "X-Wyrd-Timestamp");
    }

    #[test]
    fn webhook_config_validate_rejects_empty_url() {
        let c = WebhookConfig {
            url: String::new(),
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_config_validate_accepts_non_empty_url() {
        let c = base_config();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn webhook_method_lowercase_rejected() {
        let r: Result<WebhookMethod, _> = serde_json::from_str(r#""post""#);
        assert!(r.is_err());
        let r: Result<WebhookMethod, _> = serde_json::from_str(r#""put""#);
        assert!(r.is_err());
    }

    #[test]
    fn webhook_method_unsupported_verb_rejected() {
        let r: Result<WebhookMethod, _> = serde_json::from_str(r#""GET""#);
        assert!(r.is_err());
    }

    #[test]
    fn webhook_config_validate_rejects_empty_signature_header() {
        let c = WebhookConfig {
            signature_header: String::new(),
            ..base_config()
        };
        assert_eq!(
            c.validate().unwrap_err().to_string(),
            "alert config invalid: webhook_config.signature_header: must not be empty"
        );
    }

    #[test]
    fn webhook_config_validate_rejects_empty_timestamp_header() {
        let c = WebhookConfig {
            timestamp_header: String::new(),
            ..base_config()
        };
        assert_eq!(
            c.validate().unwrap_err().to_string(),
            "alert config invalid: webhook_config.timestamp_header: must not be empty"
        );
    }

    #[test]
    fn webhook_config_validate_rejects_zero_timeout() {
        let c = WebhookConfig {
            timeout_ms: 0,
            ..base_config()
        };
        assert_eq!(
            c.validate().unwrap_err().to_string(),
            "alert config invalid: webhook_config.timeout_ms: must be at least 1"
        );
    }

    #[test]
    fn webhook_config_secret_ref_none_means_signing_disabled() {
        let c = WebhookConfig {
            secret_ref: None,
            ..base_config()
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn webhook_config_secret_ref_none_omits_field() {
        let c = base_config();
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("secret_ref"));
    }

    #[test]
    fn webhook_config_rejects_unknown_fields() {
        let r: Result<WebhookConfig, _> = serde_json::from_str(
            r#"{"url":"https://h.example.com","method":"POST","signature_header":"X-Sig","timestamp_header":"X-Ts","timeout_ms":1000,"unknown":true}"#,
        );
        assert!(r.is_err());
    }

    #[test]
    fn webhook_headers_denylist_rejects_authorization() {
        for name in ["Authorization", "authorization", "AUTHORIZATION"] {
            let mut h = BTreeMap::new();
            h.insert(name.to_string(), "Bearer xyz".to_string());
            let c = WebhookConfig {
                headers: h,
                ..base_config()
            };
            let err = c.validate().unwrap_err().to_string();
            assert!(err.contains("secret-shaped header name"), "got: {err}");
        }
    }

    #[test]
    fn webhook_headers_denylist_rejects_cookie() {
        let mut h = BTreeMap::new();
        h.insert("Cookie".to_string(), "session=abc".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_headers_denylist_rejects_suffix_patterns() {
        for name in [
            "X-Custom-Token",
            "X-Service-Key",
            "X-App-Secret",
            "X-User-Auth",
            "X-Vendor-Credential",
        ] {
            let mut h = BTreeMap::new();
            h.insert(name.to_string(), "value".to_string());
            let c = WebhookConfig {
                headers: h,
                ..base_config()
            };
            assert!(c.validate().is_err(), "name: {name}");
        }
    }

    #[test]
    fn webhook_headers_denylist_rejects_x_api_key_explicit() {
        let mut h = BTreeMap::new();
        h.insert("X-API-Key".to_string(), "abcd1234".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_headers_denylist_allows_non_sensitive_names() {
        let mut h = BTreeMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        h.insert("X-App-Name".to_string(), "wyrd-alerts".to_string());
        h.insert("X-Request-Id".to_string(), "req-1".to_string());
        h.insert("User-Agent".to_string(), "wyrd/1.0".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn webhook_headers_rejects_signature_header_collision() {
        let mut h = BTreeMap::new();
        h.insert("X-Wyrd-Signature".to_string(), "v1:abc".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("collides with the signing or timestamp header")
        );
    }

    #[test]
    fn webhook_headers_rejects_signature_header_collision_case_insensitive() {
        let mut h = BTreeMap::new();
        h.insert("x-wyrd-signature".to_string(), "v1:abc".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_headers_rejects_timestamp_header_collision() {
        let mut h = BTreeMap::new();
        h.insert("X-Wyrd-Timestamp".to_string(), "1700000000".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_headers_collisions_use_configured_header_names() {
        let mut h = BTreeMap::new();
        h.insert("X-Custom-Sig".to_string(), "v1:abc".to_string());
        let c = WebhookConfig {
            signature_header: "X-Custom-Sig".to_string(),
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_secret_headers_round_trips() {
        let mut sh = BTreeMap::new();
        sh.insert(
            "Authorization".to_string(),
            SecretRef::Env {
                name: "WYRD_WEBHOOK_BEARER".to_string(),
            },
        );
        let c = WebhookConfig {
            secret_headers: sh,
            ..base_config()
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: WebhookConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn webhook_secret_headers_default_empty_omitted_from_serialized_output() {
        let c = base_config();
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("secret_headers"));
    }

    #[test]
    fn webhook_secret_headers_allows_authorization() {
        let mut sh = BTreeMap::new();
        sh.insert(
            "Authorization".to_string(),
            SecretRef::Env {
                name: "WYRD_WEBHOOK_BEARER".to_string(),
            },
        );
        let c = WebhookConfig {
            secret_headers: sh,
            ..base_config()
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn webhook_secret_headers_rejects_signing_header_collision() {
        let mut sh = BTreeMap::new();
        sh.insert(
            "X-Wyrd-Signature".to_string(),
            SecretRef::Env {
                name: "WYRD_SIG".to_string(),
            },
        );
        let c = WebhookConfig {
            secret_headers: sh,
            ..base_config()
        };
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("collides with the signing or timestamp header")
        );
    }

    #[test]
    fn webhook_secret_headers_rejects_timestamp_header_collision() {
        let mut sh = BTreeMap::new();
        sh.insert(
            "X-Wyrd-Timestamp".to_string(),
            SecretRef::Env {
                name: "WYRD_TS".to_string(),
            },
        );
        let c = WebhookConfig {
            secret_headers: sh,
            ..base_config()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn webhook_rejects_duplicate_key_across_headers_and_secret_headers() {
        let mut h = BTreeMap::new();
        h.insert("X-App-Name".to_string(), "wyrd".to_string());
        let mut sh = BTreeMap::new();
        sh.insert(
            "x-app-name".to_string(),
            SecretRef::Env {
                name: "WYRD_APP".to_string(),
            },
        );
        let c = WebhookConfig {
            headers: h,
            secret_headers: sh,
            ..base_config()
        };
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("appears in both")
        );
    }

    #[test]
    fn webhook_validate_rejects_http_url_with_secret() {
        let cfg = WebhookConfig {
            url: "http://hooks.example.com/wyrd".to_string(),
            secret_ref: Some(SecretRef::Env {
                name: "WEBHOOK_KEY".to_string(),
            }),
            ..WebhookConfig::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, AlertConfigError::Invalid { ref field, .. } if field == "webhook_config.url"),
            "expected Invalid {{ field: webhook_config.url, .. }}, got: {err:?}"
        );
        assert!(err.to_string().contains("https://"));
    }

    #[test]
    fn webhook_validate_allows_http_url_without_secret() {
        let cfg = WebhookConfig {
            url: "http://internal.corp/wyrd-alerts".to_string(),
            ..WebhookConfig::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn webhook_validate_allows_https_url_with_secret() {
        let cfg = WebhookConfig {
            url: "https://hooks.example.com/wyrd".to_string(),
            secret_ref: Some(SecretRef::Env {
                name: "WEBHOOK_KEY".to_string(),
            }),
            ..WebhookConfig::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn webhook_secret_headers_with_secret_ref_round_trips_alongside_static_headers() {
        let mut h = BTreeMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        let mut sh = BTreeMap::new();
        sh.insert(
            "Authorization".to_string(),
            SecretRef::Vault {
                key: "secret/data/wyrd/webhook".to_string(),
            },
        );
        let c = WebhookConfig {
            headers: h,
            secret_headers: sh,
            ..base_config()
        };
        assert!(c.validate().is_ok());
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains("secret_headers"));
        assert!(s.contains("Authorization"));
        let back: WebhookConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}

#[cfg(test)]
mod signing_canonical {
    use crate::alert_router::webhook::WEBHOOK_SIGNING_VERSION;

    #[test]
    fn canonical_string_format_locked() {
        let timestamp = 1_700_000_000u64;
        let body_sha256 = "abc123def456".to_string();
        let canonical = format!("{WEBHOOK_SIGNING_VERSION}:{timestamp}:{body_sha256}");
        assert!(canonical.starts_with("v1:"));
        let parts: Vec<&str> = canonical.splitn(3, ':').collect();
        assert_eq!(parts[0], "v1");
        assert_eq!(parts[1], "1700000000");
        assert_eq!(parts[2], body_sha256);
    }

    #[test]
    fn canonical_string_byte_for_byte_golden() {
        // SHA-256 hex of `b"hello world"` is computed independently:
        //   b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        //
        // This test does not compute SHA-256 or HMAC bytes; it only pins the
        // canonical-string concatenation shape. The HMAC byte contract is
        // pinned in `signing_hmac.rs` (PR4.3).
        let timestamp: u64 = 1_717_977_600;
        let body_sha256_hex = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let canonical = format!("v1:{timestamp}:{body_sha256_hex}");
        assert_eq!(
            canonical,
            "v1:1717977600:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}

#[cfg(test)]
mod schema_drift {
    use schemars::schema_for;
    use std::fs;

    const META_SCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

    fn generate_normalized<T: schemars::JsonSchema>() -> String {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some(META_SCHEMA.to_string());
        let json = serde_json::to_string_pretty(&schema).unwrap();
        format!("{json}\n")
    }

    fn load_golden(name: &str) -> String {
        let path = format!("{}/tests/schemas/{}.json", env!("CARGO_MANIFEST_DIR"), name);
        fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "golden missing: {path}\n\
                 Run: cargo run --locked -p vala-core --example gen_schemas"
            )
        })
    }

    fn assert_schema_matches<T: schemars::JsonSchema>(golden_name: &str) {
        let generated = generate_normalized::<T>();
        let golden = load_golden(golden_name);
        assert_eq!(generated, golden, "schema drift: {golden_name}");
    }

    #[test]
    fn webhook_config_schema_matches_golden() {
        use crate::alert_router::webhook::WebhookConfig;
        assert_schema_matches::<WebhookConfig>("alert_router_webhook_config");
    }

    #[test]
    fn webhook_method_schema_matches_golden() {
        use crate::alert_router::webhook::WebhookMethod;
        assert_schema_matches::<WebhookMethod>("alert_router_webhook_method");
    }
}
