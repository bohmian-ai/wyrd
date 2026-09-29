//! Provider error catalog with stable `SKALD_PROVIDERS_*` codes.

use reqwest::StatusCode;

/// Result alias for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Provider HTTP, auth, and decode failures.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Local provider authentication material was missing or invalid.
    #[error("{provider} authentication failed: {detail}")]
    Auth { provider: String, detail: String },
    /// The provider answered with a non-success HTTP status.
    ///
    /// `body` keeps the complete bounded answer so callers can relay the
    /// provider's own error; the display names only the provider and status,
    /// because provider bodies can echo request content or credentials. The
    /// stable code follows the status.
    #[error("{provider} answered HTTP {status}")]
    Status {
        /// Provider label.
        provider: String,
        /// Upstream HTTP status.
        status: u16,
        /// Complete answer body.
        body: String,
        /// `Retry-After` hint in milliseconds, when the provider sent one.
        retry_after_ms: Option<u64>,
    },
    /// No connection to the provider was established, so it never received
    /// the request.
    #[error("{provider} connection failed: {detail}")]
    Connect {
        /// Provider label.
        provider: String,
        /// Transport failure detail.
        detail: String,
    },
    /// The exchange failed after the provider may have received the request.
    #[error("{provider} upstream error: status={status} body={body}")]
    Upstream {
        provider: String,
        status: u16,
        body: String,
    },
    /// Provider request timed out.
    #[error("{provider} request timed out")]
    Timeout { provider: String },
    /// The request was rejected before it reached the provider.
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

    /// Records a non-success provider answer, keeping its status and body.
    pub fn from_status(
        provider: impl Into<String>,
        status: StatusCode,
        body: impl Into<String>,
        retry_after_ms: Option<u64>,
    ) -> Self {
        Self::Status {
            provider: provider.into(),
            status: status.as_u16(),
            body: body.into(),
            retry_after_ms,
        }
    }

    /// Returns the stable machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Auth { .. } => "SKALD_PROVIDERS_401_AUTH",
            Self::Status { status, .. } => match *status {
                401 | 403 => "SKALD_PROVIDERS_401_AUTH",
                408 => "SKALD_PROVIDERS_408_TIMEOUT",
                429 => "SKALD_PROVIDERS_429_RATE_LIMIT",
                500..=599 => "SKALD_PROVIDERS_5XX_UPSTREAM",
                _ => "SKALD_PROVIDERS_400_BAD_REQUEST",
            },
            Self::Connect { .. } => "SKALD_PROVIDERS_503_CONNECT",
            Self::Upstream { .. } => "SKALD_PROVIDERS_5XX_UPSTREAM",
            Self::Timeout { .. } => "SKALD_PROVIDERS_408_TIMEOUT",
            Self::BadRequest { .. } => "SKALD_PROVIDERS_400_BAD_REQUEST",
            Self::Decode { .. } => "SKALD_PROVIDERS_502_DECODE",
            Self::VariantMismatch { .. } => "SKALD_PROVIDERS_400_VARIANT_MISMATCH",
        }
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

    /// A status answer keeps its complete body for relay, while its display
    /// names only the provider and status, so formatting never exposes
    /// provider content.
    #[test]
    fn status_errors_keep_the_body_out_of_display() {
        let body = format!("{{\"error\":\"echoed sk-canary {}\"}}", "é".repeat(400));
        let error = ProviderError::from_status("openai", StatusCode::BAD_REQUEST, &*body, None);

        assert!(
            matches!(&error, ProviderError::Status { status: 400, body: kept, .. } if *kept == body)
        );
        assert_eq!(error.to_string(), "openai answered HTTP 400");
    }
}
