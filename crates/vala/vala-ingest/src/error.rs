//! Ingest error taxonomy → `WYRD_VALA_*` codes → `tonic::Status`.
//!
//! Each variant carries a stable Wyrd error code and a gRPC status code. The
//! stable code is attached to the `tonic::Status` details via
//! `google.rpc.ErrorInfo` (`tonic-types`), so the Wyrd client maps a gRPC
//! `Status` and an HTTP `problem+json` to the same taxonomy.

use vala_bifrost::BifrostError;
use wyrd_runtime::PermissionDenyReason;
use wyrd_tonic::tonic::{Code, Status};
use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt};

/// Error domain used in the attached `google.rpc.ErrorInfo`.
const WYRD_ERROR_DOMAIN: &str = "wyrd.dev";

/// Failures raised while serving a Bifrost ingest stream.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// Bearer token missing, malformed, or rejected by the verifier.
    #[error("ingest authentication failed: {0}")]
    Unauthenticated(String),
    /// Frame sequence violated the stream contract (mismatched table/batch_id,
    /// too many frames, malformed batch id).
    #[error("ingest stream protocol violation: {0}")]
    StreamProtocolViolation(String),
    /// A write to a `SystemShared` / reserved (`vala.system.*`) table.
    #[error("write to system/reserved table denied: {table}")]
    SystemTableWriteDenied {
        /// Fully-qualified table name the caller attempted to write.
        table: String,
    },
    /// A per-row `card_ref` outside the principal's card scope (or null/absent).
    #[error("card_ref {card_ref} outside principal card scope")]
    CardScopeDenied {
        /// Canonical string form of the refused card reference.
        card_ref: String,
    },
    /// The principal lacks `bifrost_record:write`.
    #[error("principal lacks bifrost_record:write: {detail}")]
    RbacDenied {
        /// Human-readable denial detail.
        detail: String,
    },
    /// The target table does not exist.
    #[error("bifrost table not found: {table}")]
    TableNotFound {
        /// Fully-qualified table name that was not found.
        table: String,
    },
    /// The batch schema does not match the registered table schema.
    #[error("schema fingerprint mismatch: {table}")]
    SchemaMismatch {
        /// Fully-qualified table name whose schema does not match.
        table: String,
    },
    /// The stream exceeded an aggregate byte/row bound.
    #[error("ingest stream too large ({bytes} > {limit})")]
    BatchTooLarge {
        /// Observed running total when the bound tripped.
        bytes: u64,
        /// The configured limit.
        limit: u64,
    },
    /// An idle-frame gap or total-stream deadline elapsed.
    #[error("ingest stream idle/deadline exceeded")]
    StreamIdle,
    /// The per-tenant concurrent-stream limit is saturated.
    #[error("too many concurrent ingest streams for tenant")]
    TooManyStreams,
    /// The writer coordinator stopped before the commit completed.
    #[error("ingest writer closed")]
    WriterClosed,
    /// Arrow IPC decode failed.
    #[error("ingest arrow decode failed: {0}")]
    Decode(String),
    /// An internal failure with no stable client remediation.
    #[error("internal ingest error: {0}")]
    Internal(String),
}

impl IngestError {
    /// The stable `WYRD_VALA_*` (or reused `WYRD_PERMISSION_*`) code.
    #[must_use]
    pub fn wyrd_code(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) => "WYRD_VALA_401_INGEST_AUTH",
            Self::StreamProtocolViolation(_) | Self::Decode(_) => "WYRD_VALA_400_INGEST_PROTO",
            Self::SystemTableWriteDenied { .. } => "WYRD_VALA_403_BIFROST_SYSTEM_TABLE_WRITE",
            Self::CardScopeDenied { .. } => "WYRD_VALA_403_BIFROST_CARD_SCOPE",
            Self::RbacDenied { .. } => "WYRD_PERMISSION_403_DENIED_RBAC",
            Self::TableNotFound { .. } => "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
            Self::SchemaMismatch { .. } => "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
            Self::BatchTooLarge { .. } => "WYRD_VALA_413_INGEST_OVERSIZED",
            Self::StreamIdle => "WYRD_VALA_408_INGEST_IDLE_TIMEOUT",
            Self::TooManyStreams => "WYRD_VALA_429_INGEST_TOO_MANY_STREAMS",
            Self::WriterClosed => "WYRD_VALA_409_INGEST_WRITER_CLOSED",
            Self::Internal(_) => "WYRD_VALA_500_INGEST_INTERNAL",
        }
    }

    /// The gRPC status code this error maps to.
    #[must_use]
    pub fn grpc_code(&self) -> Code {
        match self {
            Self::Unauthenticated(_) => Code::Unauthenticated,
            Self::StreamProtocolViolation(_) | Self::Decode(_) => Code::InvalidArgument,
            Self::SystemTableWriteDenied { .. }
            | Self::CardScopeDenied { .. }
            | Self::RbacDenied { .. } => Code::PermissionDenied,
            Self::TableNotFound { .. } => Code::NotFound,
            Self::SchemaMismatch { .. } => Code::FailedPrecondition,
            Self::BatchTooLarge { .. } | Self::TooManyStreams => Code::ResourceExhausted,
            Self::StreamIdle => Code::DeadlineExceeded,
            Self::WriterClosed => Code::Aborted,
            Self::Internal(_) => Code::Internal,
        }
    }

    /// Render as a `tonic::Status` with the stable code in `ErrorInfo` details.
    #[must_use]
    pub fn into_status(self) -> Status {
        let code = self.grpc_code();
        let reason = self.wyrd_code();
        let message = self.to_string();
        let details =
            ErrorDetails::with_error_info(reason, WYRD_ERROR_DOMAIN, [] as [(String, String); 0]);
        Status::with_error_details(code, message, details)
    }

    /// Map an RBAC verdict denial into the reused RBAC code.
    #[must_use]
    pub fn from_rbac(reason: PermissionDenyReason) -> Self {
        let PermissionDenyReason::Rbac {
            required,
            principal,
        } = reason;
        Self::RbacDenied {
            detail: format!("principal {principal} lacks {required}"),
        }
    }

    /// Map an engine [`BifrostError`] surfaced by the writer into an ingest error.
    #[must_use]
    pub fn from_engine(error: BifrostError) -> Self {
        match error {
            BifrostError::WriterUnavailable(_) => Self::WriterClosed,
            BifrostError::TableNotFound(table) => Self::TableNotFound { table },
            BifrostError::FingerprintMismatch(table) => Self::SchemaMismatch { table },
            BifrostError::ReservedColumn(column) => Self::StreamProtocolViolation(format!(
                "batch carries reserved system column {column}"
            )),
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<IngestError> for Status {
    fn from(error: IngestError) -> Self {
        error.into_status()
    }
}
