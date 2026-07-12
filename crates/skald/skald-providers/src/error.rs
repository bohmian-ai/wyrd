//! Provider error catalog with stable `SKALD_PROVIDERS_*` codes.

use reqwest::StatusCode;
use tracing::debug;

const MAX_BODY_BYTES: usize = 512;

/// Result alias for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Provider HTTP, auth, and decode failures.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Provider authentication failed or credentials were missing.
    #[error("{provider} authentication failed: {detail}")]
    Auth { provider: String, detail: String },
    /// Provider rate-limited the request.
    #[error("{provider} rate limited request")]
    RateLimit {
        provider: String,
        retry_after_ms: Option<u64>,
    },
    /// Provider returned a 5xx or transport-level upstream failure.
    #[error("{provider} upstream error: status={status} body={body}")]
    Upstream {
        provider: String,
        status: u16,
        body: String,
    },
    /// Provider request timed out.
    #[error("{provider} request timed out")]
    Timeout { provider: String },
    /// Provider rejected the request body.
    #[error("{provider} rejected request: {detail}")]
    BadRequest { provider: String, detail: String },
    /// Provider response could not be decoded as the expected native shape.
    #[error("{provider} response decode failed: {message}")]
    Decode { provider: String, message: String },
    /// Request variant did not match the concrete provider client.
    #[error("request variant {sent} does not match provider {provider}")]
    VariantMismatch { sent: String, provider: String },
}

impl ProviderError {
    /// Creates an auth error without exposing any secret value.
    pub fn auth(provider: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::Auth {
            provider: provider.into(),
            detail: detail.into(),
        }
    }

    /// Creates a bad-request error.
    pub fn bad_request(provider: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::BadRequest {
            provider: provider.into(),
            detail: detail.into(),
        }
    }

    /// Creates a decode error.
    pub fn decode(provider: impl Into<String>, source: impl std::fmt::Display) -> Self {
        Self::Decode {
            provider: provider.into(),
            message: source.to_string(),
        }
    }

    /// Creates a timeout error.
    pub fn timeout(provider: impl Into<String>) -> Self {
        Self::Timeout {
            provider: provider.into(),
        }
    }

    /// Creates an upstream error.
    pub fn upstream(provider: impl Into<String>, status: u16, body: impl Into<String>) -> Self {
        Self::Upstream {
            provider: provider.into(),
            status,
            body: body.into(),
        }
    }

    /// Creates a variant-mismatch error.
    pub fn variant_mismatch(provider: impl Into<String>, sent: impl Into<String>) -> Self {
        Self::VariantMismatch {
            provider: provider.into(),
            sent: sent.into(),
        }
    }

    /// Maps a provider HTTP status and response body to the stable catalog.
    pub fn from_status(
        provider: impl Into<String>,
        status: StatusCode,
        body: impl Into<String>,
        retry_after_ms: Option<u64>,
    ) -> Self {
        let provider = provider.into();
        let body = body.into();
        let truncated = truncate_body(&body);
        match status.as_u16() {
            401 | 403 => Self::auth(provider, "provider rejected credentials"),
            408 => Self::timeout(provider),
            429 => Self::RateLimit {
                provider,
                retry_after_ms,
            },
            500..=599 => Self::upstream(provider, status.as_u16(), truncated),
            _ => Self::bad_request(provider, truncated),
        }
    }

    /// Returns the stable machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Auth { .. } => "SKALD_PROVIDERS_401_AUTH",
            Self::RateLimit { .. } => "SKALD_PROVIDERS_429_RATE_LIMIT",
            Self::Upstream { .. } => "SKALD_PROVIDERS_5XX_UPSTREAM",
            Self::Timeout { .. } => "SKALD_PROVIDERS_408_TIMEOUT",
            Self::BadRequest { .. } => "SKALD_PROVIDERS_400_BAD_REQUEST",
            Self::Decode { .. } => "SKALD_PROVIDERS_502_DECODE",
            Self::VariantMismatch { .. } => "SKALD_PROVIDERS_400_VARIANT_MISMATCH",
        }
    }
}

fn truncate_body(body: &str) -> String {
    debug!(full_body = %body, "provider upstream response body");
    if body.len() <= MAX_BODY_BYTES {
        body.to_owned()
    } else {
        format!("{}…[truncated]", &body[..MAX_BODY_BYTES])
    }
}

#[cfg(test)]
mod error_mapping {
    use reqwest::StatusCode;

    use crate::ProviderError;

    #[test]
    fn status_errors_map_to_stable_codes() {
        assert_eq!(
            ProviderError::from_status("openai", StatusCode::UNAUTHORIZED, "", None).code(),
            "SKALD_PROVIDERS_401_AUTH"
        );
        assert_eq!(
            ProviderError::from_status("openai", StatusCode::TOO_MANY_REQUESTS, "", Some(1000))
                .code(),
            "SKALD_PROVIDERS_429_RATE_LIMIT"
        );
        assert_eq!(
            ProviderError::from_status(
                "anthropic",
                StatusCode::from_u16(529).unwrap(),
                "overloaded",
                None
            )
            .code(),
            "SKALD_PROVIDERS_5XX_UPSTREAM"
        );
        assert_eq!(
            ProviderError::from_status("google", StatusCode::BAD_REQUEST, "bad", None).code(),
            "SKALD_PROVIDERS_400_BAD_REQUEST"
        );
        assert_eq!(
            ProviderError::decode("openai", "bad json").code(),
            "SKALD_PROVIDERS_502_DECODE"
        );
    }
}
