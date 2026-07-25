//! Ingest error taxonomy → `WYRD_VALA_*` codes → `tonic::Status`.
//!
//! Each variant carries a stable Wyrd error code and a gRPC status code. The
//! stable code is attached to the `tonic::Status` details via
//! `google.rpc.ErrorInfo` (`tonic-types`), so the Wyrd client maps a gRPC
//! `Status` and an HTTP `problem+json` to the same taxonomy.

use wyrd_runtime::PermissionDenyReason;
use wyrd_tonic::tonic::{Code, Status};
use wyrd_tonic::tonic_types::{ErrorDetails, StatusExt};

/// Error domain used in the attached `google.rpc.ErrorInfo`.
const WYRD_ERROR_DOMAIN: &str = "wyrd.dev";

/// Failures returned by the server's catalog adapter.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("bifrost table not found: {0}")]
    TableNotFound(String),
    #[error("schema fingerprint mismatch: {0}")]
    FingerprintMismatch(String),
    #[error("catalog failure: {0}")]
    Internal(String),
}

/// Failures raised while serving a Bifrost ingest request.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// Bearer token missing, malformed, or rejected by the verifier.
    #[error("ingest authentication failed: {0}")]
    Unauthenticated(String),
    /// A request field violated the native ingest contract.
    #[error("ingest request validation failed: {0}")]
    RequestValidation(String),
    /// A write to a server-managed built-in table.
    #[error("write to reserved built-in table denied: {table}")]
    ReservedBuiltinWriteDenied {
        /// Fully-qualified table name the caller attempted to write.
        table: String,
    },
    /// A per-row `card_ref` outside the principal's card scope (or null/absent).
    #[error("card_ref {card_ref} outside principal card scope")]
    CardScopeDenied {
        /// Canonical string form of the refused card reference.
        card_ref: String,
    },
    /// A per-row `card_ref` was present but could not be resolved to a `card_uid`
    /// by the server (unknown card, cross-tenant, or uid not available). Fail-closed
    /// (M-11): an authenticated write with an unresolvable `card_ref` is always rejected.
    #[error("card_ref {card_ref} cannot be resolved to card_uid (WYRD_VALA_403_CARD_UNRESOLVED)")]
    CardUnresolved {
        /// Canonical string form of the unresolvable card reference.
        card_ref: String,
    },
    /// An authenticated write was missing a required `principal_id`.
    #[error("principal_id cannot be stamped for authenticated write")]
    PrincipalUnresolved,
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
    /// The server-measured canonical payload exceeded Scribe's 32 MiB limit.
    #[error("ingest payload too large ({bytes} > {limit} bytes)")]
    PayloadTooLarge {
        /// Measured canonical transport bytes.
        bytes: u64,
        /// Scribe's fixed limit.
        limit: u64,
    },
    /// The request exceeded the aggregate row bound.
    #[error("ingest request has too many rows ({rows} > {limit})")]
    TooManyRows {
        /// Observed running row total when the bound tripped.
        rows: u64,
        /// The configured row limit.
        limit: u64,
    },
    /// The ingest coordinator stopped before the commit completed.
    #[error("ingest coordinator closed")]
    IngressClosed,
    /// The bounded ingest coordinator's local buffer is full.
    ///
    /// Local backpressure only (Q5) — no rows from this request were written.
    /// Callers must back off and retry the full request.
    #[error("ingest coordinator busy — local buffer full")]
    IngestBusy,
    /// The pod-wide WAL disk breaker is open.
    #[error("ingest WAL storage is unavailable")]
    WalDiskFull,
    /// Arrow IPC decode failed.
    #[error("ingest arrow decode failed: {0}")]
    Decode(String),
    /// An internal failure with no stable client remediation.
    #[error("internal ingest error: {0}")]
    Internal(String),
}

impl IngestError {
    /// Map a queued Redux Scribe result onto the transport-neutral ingest
    /// taxonomy used by HTTP and gRPC adapters.
    pub fn from_scribe(error: crate::contracts::ScribeError) -> Self {
        match error {
            crate::contracts::ScribeError::IngestBusy { .. } => Self::IngestBusy,
            crate::contracts::ScribeError::PayloadTooLarge { bytes } => Self::PayloadTooLarge {
                bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(crate::scribe::admission::MAX_REQUEST_BYTES)
                    .unwrap_or(u64::MAX),
            },
            crate::contracts::ScribeError::FingerprintMismatch { table } => {
                Self::SchemaMismatch { table }
            }
            crate::contracts::ScribeError::TooManyRows { rows, limit } => {
                Self::TooManyRows { rows, limit }
            }
            crate::contracts::ScribeError::InvalidFrame => {
                Self::Decode("ingest frame validation failed".to_owned())
            }
            crate::contracts::ScribeError::CardScopeDenied => Self::CardScopeDenied {
                card_ref: "<server-validation>".to_owned(),
            },
            crate::contracts::ScribeError::CardUnresolved => Self::CardUnresolved {
                card_ref: "<server-validation>".to_owned(),
            },
            crate::contracts::ScribeError::IngressClosed => Self::IngressClosed,
            crate::contracts::ScribeError::WalDiskFull => Self::WalDiskFull,
            other => {
                tracing::error!(error = %other, "Scribe ingest failed after transport validation");
                Self::Internal("Scribe ingest failed".to_owned())
            }
        }
    }

    /// The stable `WYRD_VALA_*` (or reused `WYRD_PERMISSION_*`) code.
    #[must_use]
    pub fn wyrd_code(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) => "WYRD_VALA_401_INGEST_AUTH",
            Self::RequestValidation(_) | Self::Decode(_) => "WYRD_VALA_400_INGEST_PROTO",
            Self::ReservedBuiltinWriteDenied { .. } => {
                "WYRD_VALA_403_BIFROST_RESERVED_BUILTIN_WRITE"
            }
            Self::CardScopeDenied { .. } => "WYRD_VALA_403_BIFROST_CARD_SCOPE",
            Self::CardUnresolved { .. } => "WYRD_VALA_403_CARD_UNRESOLVED",
            Self::PrincipalUnresolved => "WYRD_VALA_401_PRINCIPAL_UNRESOLVED",
            Self::RbacDenied { .. } => "WYRD_PERMISSION_403_DENIED_RBAC",
            Self::TableNotFound { .. } => "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
            Self::SchemaMismatch { .. } => "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
            Self::TooManyRows { .. } => "WYRD_VALA_413_INGEST_OVERSIZED",
            Self::PayloadTooLarge { .. } => "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
            Self::IngressClosed => "WYRD_VALA_409_INGEST_WRITER_CLOSED",
            Self::IngestBusy => "WYRD_VALA_429_INGEST_BUSY",
            Self::WalDiskFull => "WYRD_VALA_507_INGEST_WAL_UNAVAILABLE",
            Self::Internal(_) => "WYRD_VALA_500_INGEST_INTERNAL",
        }
    }

    /// The gRPC status code this error maps to.
    #[must_use]
    pub fn grpc_code(&self) -> Code {
        match self {
            Self::Unauthenticated(_) | Self::PrincipalUnresolved => Code::Unauthenticated,
            Self::RequestValidation(_) | Self::Decode(_) => Code::InvalidArgument,
            Self::ReservedBuiltinWriteDenied { .. }
            | Self::CardScopeDenied { .. }
            | Self::CardUnresolved { .. }
            | Self::RbacDenied { .. } => Code::PermissionDenied,
            Self::TableNotFound { .. } => Code::NotFound,
            Self::SchemaMismatch { .. } => Code::FailedPrecondition,
            Self::TooManyRows { .. }
            | Self::PayloadTooLarge { .. }
            | Self::IngestBusy
            | Self::WalDiskFull => Code::ResourceExhausted,
            Self::IngressClosed => Code::Aborted,
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

    /// Map a server catalog lookup failure into an ingest error.
    #[must_use]
    pub fn from_catalog(error: CatalogError) -> Self {
        match error {
            CatalogError::TableNotFound(table) => Self::TableNotFound { table },
            CatalogError::FingerprintMismatch(table) => Self::SchemaMismatch { table },
            CatalogError::Internal(detail) => Self::Internal(detail),
        }
    }
}

impl From<IngestError> for Status {
    fn from(error: IngestError) -> Self {
        error.into_status()
    }
}
