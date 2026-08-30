//! Closed failure vocabulary for the node's Bifrost storage owner.

/// One closed storage failure with a scrubbed operator-facing detail.
///
/// The six backend classes are the complete mapping target for a failed read;
/// the lifecycle variants describe an outcome that never reached the backend at
/// all. Splitting them keeps "the object could not be read" distinct from "we
/// stopped trying", which are different operator actions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BifrostStorageError {
    /// The object does not exist at the validated location.
    #[error("bifrost storage object not found: {detail}")]
    NotFound {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// The backend refused the request for authorization reasons.
    #[error("bifrost storage permission denied: {detail}")]
    PermissionDenied {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// The request exceeded the configured request timeout.
    #[error("bifrost storage request timed out: {detail}")]
    Timeout {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// The backend applied rate limiting or throttling.
    #[error("bifrost storage rate limited: {detail}")]
    RateLimited {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// Bytes were returned but did not decode as the expected format.
    #[error("bifrost storage returned invalid data: {detail}")]
    InvalidData {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// Any other backend failure that does not fit a narrower class.
    #[error("bifrost storage backend failure: {detail}")]
    Backend {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// The caller's cancellation token fired before a terminal result.
    #[error("bifrost storage request was cancelled")]
    Cancelled,
    /// The caller's absolute deadline elapsed before a terminal result.
    #[error("bifrost storage request exceeded its deadline")]
    Deadline,
    /// The storage owner is closing or closed and admits no new work.
    #[error("bifrost storage owner is closed")]
    Closed,
    /// The configured policy or storage locator is not usable.
    #[error("bifrost storage configuration is invalid: {detail}")]
    InvalidConfiguration {
        /// Scrubbed description naming the field and the bound it violated.
        detail: String,
    },
}

impl BifrostStorageError {
    /// Returns the bounded `error_kind` label for this failure.
    ///
    /// These values are the complete emitted inventory. They are `'static` on
    /// purpose: an `error_kind` derived from backend text would make the field
    /// unbounded and unsafe to correlate on.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::Timeout { .. } => "timeout",
            Self::RateLimited { .. } => "rate_limited",
            Self::InvalidData { .. } => "invalid_data",
            Self::Backend { .. } => "backend",
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline",
            Self::Closed => "closed",
            Self::InvalidConfiguration { .. } => "invalid_configuration",
        }
    }

    /// Returns whether another attempt could plausibly succeed.
    ///
    /// Only the transient backend classes retry. A missing object, a refused
    /// permission, or undecodable bytes are all definite for the immutable
    /// objects Bifrost reads, so retrying them spends the budget without
    /// changing the answer.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. } | Self::RateLimited { .. } | Self::Backend { .. }
        )
    }

    /// Maps one Parquet decode failure into this closed vocabulary.
    ///
    /// Everything Parquet reports at this boundary is either a range read that
    /// already failed and was wrapped, or bytes that are not valid metadata;
    /// both are definite for an immutable object, so neither retries. The
    /// detail is a fixed phrase rather than the error's own message so no
    /// object path can reach a log line through this path.
    #[must_use]
    pub fn from_parquet(error: &parquet::errors::ParquetError) -> Self {
        Self::InvalidData {
            detail: match error {
                parquet::errors::ParquetError::EOF(_) => "unexpected end of parquet metadata",
                parquet::errors::ParquetError::ArrowError(_) => "parquet metadata is not arrow",
                _ => "parquet metadata did not decode",
            }
            .to_owned(),
        }
    }
}
