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

    /// Maps one `OpenDAL` failure into this closed vocabulary.
    ///
    /// The owner is the only place backend errors are classified, so the
    /// Iceberg adapter never has to reason about `OpenDAL` kinds to decide
    /// whether another attempt is allowed. The detail is `OpenDAL`'s own kind
    /// phrase rather than its rendered message: the message can embed the
    /// object key it failed on, and a scrubbed detail is worth more here than a
    /// specific one that leaks a path into a log line.
    #[must_use]
    pub fn from_opendal(error: &opendal::Error) -> Self {
        let detail = || error.kind().to_string();
        match error.kind() {
            opendal::ErrorKind::NotFound => Self::NotFound { detail: detail() },
            opendal::ErrorKind::PermissionDenied => Self::PermissionDenied { detail: detail() },
            opendal::ErrorKind::RateLimited => Self::RateLimited { detail: detail() },
            opendal::ErrorKind::ConfigInvalid => Self::InvalidConfiguration { detail: detail() },
            opendal::ErrorKind::Unexpected => Self::Backend { detail: detail() },
            _ => Self::InvalidData { detail: detail() },
        }
    }

    /// Returns the bounded request-terminal outcome for this failure.
    ///
    /// One place maps the closed error vocabulary onto the closed telemetry
    /// vocabulary, so a new error variant cannot quietly acquire a second,
    /// divergent label.
    #[must_use]
    pub const fn request_outcome(&self) -> crate::storage::telemetry::StorageRequestOutcome {
        use crate::storage::telemetry::StorageRequestOutcome as Outcome;
        match self {
            Self::NotFound { .. } => Outcome::NotFound,
            Self::PermissionDenied { .. } => Outcome::PermissionDenied,
            Self::Timeout { .. } => Outcome::Timeout,
            Self::RateLimited { .. } => Outcome::RateLimited,
            Self::InvalidData { .. } => Outcome::InvalidData,
            Self::Backend { .. } => Outcome::Backend,
            Self::Cancelled => Outcome::Cancelled,
            Self::Deadline => Outcome::Deadline,
            Self::Closed => Outcome::Closed,
            Self::InvalidConfiguration { .. } => Outcome::InvalidConfiguration,
        }
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
        // `External` is the only variant that did not come from the bytes: it
        // is whatever the reader beneath Parquet failed with, which for a hot
        // object is an object-store range read. Classifying it as a backend
        // failure is what makes the read retryable, and classifying everything
        // else as invalid data is what stops the owner retrying a footer that
        // will never decode.
        match error {
            parquet::errors::ParquetError::External(_) => Self::Backend {
                detail: "the object store failed a parquet metadata range read".to_owned(),
            },
            parquet::errors::ParquetError::EOF(_) => Self::InvalidData {
                detail: "unexpected end of parquet metadata".to_owned(),
            },
            parquet::errors::ParquetError::ArrowError(_) => Self::InvalidData {
                detail: "parquet metadata is not arrow".to_owned(),
            },
            _ => Self::InvalidData {
                detail: "parquet metadata did not decode".to_owned(),
            },
        }
    }
}
