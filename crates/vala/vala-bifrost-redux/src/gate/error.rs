//! Ingest error taxonomy → `WYRD_VALA_*` codes → `tonic::Status`.
//!
//! Each variant carries a stable Wyrd error code and a gRPC status code. The
//! stable code is attached to the `tonic::Status` details via
//! `google.rpc.ErrorInfo` (`tonic-types`), so the Wyrd client maps a gRPC
//! `Status` and an HTTP `problem+json` to the same taxonomy.

use wyrd_runtime::PermissionDenyReason;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::error::BifrostError;
use wyrd_tonic::tonic::{Code, Status, metadata::MetadataValue};
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
#[derive(Clone, Debug, thiserror::Error)]
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
    /// Callers must back off and retry the full request. The table is retained
    /// when Scribe supplied it so every transport can report the same target.
    #[error("ingest coordinator busy for table {table} — local buffer full")]
    IngestBusy {
        /// Fully-qualified table whose coordinator buffer is saturated.
        table: String,
    },
    /// The pod-wide WAL disk breaker is open.
    #[error("ingest WAL storage is unavailable")]
    WalDiskFull,
    /// Arrow IPC decode failed.
    #[error("ingest arrow decode failed: {0}")]
    Decode(String),
    /// A caller-supplied `wyrd_event_time` value falls outside the server
    /// acceptance window evaluated against per-batch receipt time.
    ///
    /// The whole batch is rejected; no rows are written. The caller must
    /// either supply an in-window event time or omit the column.
    #[error(
        "wyrd_event_time {value_micros} µs outside acceptance window \
         [{past_bound_micros}, {future_bound_micros}] µs"
    )]
    EventTimeOutOfRange {
        /// The offending `wyrd_event_time` in epoch-microseconds.
        value_micros: i64,
        /// Inclusive past bound (receipt − past window) in epoch-microseconds.
        past_bound_micros: i64,
        /// Inclusive future bound (receipt + future window) in epoch-microseconds.
        future_bound_micros: i64,
    },
    /// An internal failure with no stable client remediation.
    #[error("internal ingest error: {0}")]
    Internal(String),
}

impl IngestError {
    /// Project the Gate taxonomy into the derive-backed public Wyrd catalog.
    ///
    /// This is the sole ingest identity projection. HTTP supplies the endpoint
    /// table as `table_hint`; gRPC uses `"unknown"` only where the Gate has no
    /// table-bearing variant. Scribe-supplied table names win over the hint so
    /// both transports retain the same physical target without a second match
    /// table in a serving adapter.
    #[must_use]
    pub fn to_wyrd_error(&self, table_hint: &str) -> WyrdError {
        let fallback_table = || table_hint.to_owned();
        let table = |value: &str| {
            if value.is_empty() {
                fallback_table()
            } else {
                value.to_owned()
            }
        };
        match self {
            Self::Unauthenticated(message) => BifrostError::IngestAuthentication {
                message: message.clone(),
            }
            .into(),
            Self::PrincipalUnresolved => BifrostError::PrincipalUnresolved.into(),
            Self::RequestValidation(detail) | Self::Decode(detail) => {
                BifrostError::OtlpRequestMalformed {
                    table: fallback_table(),
                    detail: detail.clone(),
                }
                .into()
            }
            Self::RbacDenied { detail } => WyrdError::PermissionDeniedRbac {
                message: detail.clone(),
                details: serde_json::Value::Null,
            },
            Self::ReservedBuiltinWriteDenied { table } => {
                BifrostError::ReservedBuiltinWriteDenied {
                    table: table.clone(),
                }
                .into()
            }
            Self::CardScopeDenied { card_ref } => BifrostError::CardScopeDenied {
                card_ref: card_ref.clone(),
            }
            .into(),
            Self::CardUnresolved { card_ref } => BifrostError::CardUnresolved {
                card_ref: card_ref.clone(),
            }
            .into(),
            Self::TableNotFound { table } => BifrostError::TableNotFound {
                table: table.clone(),
            }
            .into(),
            Self::SchemaMismatch { table } => BifrostError::FingerprintMismatch {
                table: table.clone(),
            }
            .into(),
            Self::PayloadTooLarge { bytes, .. } => BifrostError::PayloadTooLarge {
                bytes: usize::try_from(*bytes).unwrap_or(usize::MAX),
            }
            .into(),
            Self::TooManyRows { rows, limit } => BifrostError::IngestOversized {
                rows: *rows,
                limit: *limit,
            }
            .into(),
            Self::IngressClosed => BifrostError::WriterUnavailable {
                table: fallback_table(),
            }
            .into(),
            Self::IngestBusy { table: value } => BifrostError::IngestBusy {
                table: table(value),
            }
            .into(),
            Self::WalDiskFull => BifrostError::WalDiskFull.into(),
            Self::EventTimeOutOfRange {
                value_micros,
                past_bound_micros,
                future_bound_micros,
            } => BifrostError::EventTimeOutOfRange {
                value: value_micros.to_string(),
                past_bound: past_bound_micros.to_string(),
                future_bound: future_bound_micros.to_string(),
            }
            .into(),
            Self::Internal(detail) => BifrostError::Internal {
                detail: detail.clone(),
            }
            .into(),
        }
    }

    /// Map a queued Redux Scribe result onto the Gate-owned ingest taxonomy.
    ///
    /// This conversion retains domain context but does not choose an HTTP or
    /// gRPC identity; [`Self::to_wyrd_error`] performs that projection once at
    /// the serving boundary.
    #[must_use]
    pub fn from_scribe(error: crate::contracts::ScribeError) -> Self {
        match error {
            crate::contracts::ScribeError::IngestBusy { table } => Self::IngestBusy { table },
            crate::contracts::ScribeError::PayloadTooLarge { bytes } => Self::PayloadTooLarge {
                bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(crate::scribe::admission::MAX_REQUEST_BYTES)
                    .unwrap_or(u64::MAX),
            },
            crate::contracts::ScribeError::DecodedPayloadTooLarge { bytes, limit } => {
                Self::PayloadTooLarge {
                    bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
                    limit: u64::try_from(limit).unwrap_or(u64::MAX),
                }
            }
            crate::contracts::ScribeError::FingerprintMismatch { table } => {
                Self::SchemaMismatch { table }
            }
            crate::contracts::ScribeError::TooManyRows { rows, limit } => {
                Self::TooManyRows { rows, limit }
            }
            crate::contracts::ScribeError::InvalidFrame => {
                Self::Decode("ingest frame validation failed".to_owned())
            }
            crate::contracts::ScribeError::EventTimeOutOfRange {
                value_micros,
                past_bound_micros,
                future_bound_micros,
            } => Self::EventTimeOutOfRange {
                value_micros,
                past_bound_micros,
                future_bound_micros,
            },
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

    /// Return the gRPC status class derived from the canonical Wyrd status.
    #[must_use]
    pub fn grpc_code(&self) -> Code {
        grpc_code_for_wyrd(&self.to_wyrd_error("unknown"))
    }

    /// Render as a `tonic::Status` with the canonical code in `ErrorInfo`.
    ///
    /// The Gate's native gRPC adapter has no endpoint table argument, so only
    /// errors carrying their own table retain one here; OTLP/HTTP supplies its
    /// endpoint table to [`Self::to_wyrd_error`] before response rendering.
    #[must_use]
    pub fn into_status(self) -> Status {
        let public = self.to_wyrd_error("unknown");
        let retryable_busy = public.code() == "WYRD_VALA_429_INGEST_BUSY";
        let details = ErrorDetails::with_error_info(
            public.code(),
            WYRD_ERROR_DOMAIN,
            [] as [(String, String); 0],
        );
        let mut status =
            Status::with_error_details(grpc_code_for_wyrd(&public), public.to_string(), details);
        if retryable_busy {
            status
                .metadata_mut()
                .insert("retry-after-ms", MetadataValue::from_static("1000"));
        }
        status
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

/// Convert a canonical Wyrd HTTP status into the ingest gRPC class.
///
/// Ingest treats a schema conflict (`409`) as a failed precondition rather than
/// `AlreadyExists`; all other statuses use the standard Wyrd transport class.
#[must_use]
fn grpc_code_for_wyrd(error: &WyrdError) -> Code {
    match error.status() {
        400 => Code::InvalidArgument,
        401 => Code::Unauthenticated,
        403 => Code::PermissionDenied,
        404 => Code::NotFound,
        409 => Code::FailedPrecondition,
        413 | 429 | 507 => Code::ResourceExhausted,
        500 => Code::Internal,
        503 => Code::Unavailable,
        _ => Code::Unknown,
    }
}

impl From<IngestError> for Status {
    fn from(error: IngestError) -> Self {
        error.into_status()
    }
}

#[cfg(test)]
mod tests {
    use super::IngestError;
    use crate::contracts::ScribeError;
    use wyrd_tonic::tonic::Code;
    use wyrd_tonic::tonic_types::StatusExt;

    /// Build the complete Gate error matrix used by both transport tests.
    fn ingest_cases() -> Vec<(IngestError, &'static str, u16, Code)> {
        let mut cases = access_cases();
        cases.extend(catalog_cases());
        cases.extend(capacity_cases());
        cases.extend(lifecycle_cases());
        cases
    }

    /// Build authentication and authorization cases for the ingest matrix.
    fn access_cases() -> Vec<(IngestError, &'static str, u16, Code)> {
        vec![
            (
                IngestError::Unauthenticated("bad token".to_owned()),
                "WYRD_VALA_401_INGEST_AUTH",
                401,
                Code::Unauthenticated,
            ),
            (
                IngestError::RequestValidation("bad request".to_owned()),
                "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
                400,
                Code::InvalidArgument,
            ),
            (
                IngestError::ReservedBuiltinWriteDenied {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_403_BIFROST_RESERVED_BUILTIN_WRITE",
                403,
                Code::PermissionDenied,
            ),
            (
                IngestError::CardScopeDenied {
                    card_ref: "c".to_owned(),
                },
                "WYRD_VALA_403_BIFROST_CARD_SCOPE",
                403,
                Code::PermissionDenied,
            ),
            (
                IngestError::CardUnresolved {
                    card_ref: "c".to_owned(),
                },
                "WYRD_VALA_403_CARD_UNRESOLVED",
                403,
                Code::PermissionDenied,
            ),
            (
                IngestError::PrincipalUnresolved,
                "WYRD_VALA_401_PRINCIPAL_UNRESOLVED",
                401,
                Code::Unauthenticated,
            ),
            (
                IngestError::RbacDenied {
                    detail: "denied".to_owned(),
                },
                "WYRD_PERMISSION_403_DENIED_RBAC",
                403,
                Code::PermissionDenied,
            ),
        ]
    }

    /// Build catalog lookup and schema-conflict cases for the ingest matrix.
    fn catalog_cases() -> Vec<(IngestError, &'static str, u16, Code)> {
        vec![
            (
                IngestError::TableNotFound {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
                404,
                Code::NotFound,
            ),
            (
                IngestError::SchemaMismatch {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
                409,
                Code::FailedPrecondition,
            ),
        ]
    }

    /// Build request-size and WAL-capacity cases for the ingest matrix.
    fn capacity_cases() -> Vec<(IngestError, &'static str, u16, Code)> {
        vec![
            (
                IngestError::PayloadTooLarge { bytes: 2, limit: 1 },
                "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
                413,
                Code::ResourceExhausted,
            ),
            (
                IngestError::TooManyRows { rows: 2, limit: 1 },
                "WYRD_VALA_413_INGEST_OVERSIZED",
                413,
                Code::ResourceExhausted,
            ),
            (
                IngestError::WalDiskFull,
                "WYRD_VALA_507_WAL_DISK_FULL",
                507,
                Code::ResourceExhausted,
            ),
        ]
    }

    /// Build writer-lifecycle and internal-failure cases for the ingest matrix.
    fn lifecycle_cases() -> Vec<(IngestError, &'static str, u16, Code)> {
        vec![
            (
                IngestError::IngressClosed,
                "WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE",
                503,
                Code::Unavailable,
            ),
            (
                IngestError::IngestBusy {
                    table: "t".to_owned(),
                },
                "WYRD_VALA_429_INGEST_BUSY",
                429,
                Code::ResourceExhausted,
            ),
            (
                IngestError::Decode("bad".to_owned()),
                "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
                400,
                Code::InvalidArgument,
            ),
            (
                IngestError::Internal("broken".to_owned()),
                "WYRD_VALA_500_BIFROST_INTERNAL",
                500,
                Code::Internal,
            ),
        ]
    }

    /// `from_scribe` maps `EventTimeOutOfRange` to the distinct `IngestError` variant,
    /// and `to_wyrd_error` maps it to `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` — not
    /// to `OtlpRequestMalformed`.
    #[test]
    fn event_time_out_of_range_maps_through_gate_without_identity_collapse() {
        let scribe_err = ScribeError::EventTimeOutOfRange {
            value_micros: -1_000_000_i64,
            past_bound_micros: 0_i64,
            future_bound_micros: 1_000_000_i64,
        };
        let ingest_err = IngestError::from_scribe(scribe_err);
        assert!(
            matches!(
                ingest_err,
                IngestError::EventTimeOutOfRange {
                    value_micros: -1_000_000,
                    ..
                }
            ),
            "from_scribe must produce IngestError::EventTimeOutOfRange, got {ingest_err}"
        );

        let public = ingest_err.to_wyrd_error("vala.traces.spans");
        assert_eq!(
            public.code(),
            "WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE",
            "to_wyrd_error must produce the stable EventTimeOutOfRange code"
        );
        assert_eq!(public.status(), 400, "status must be 400");
        // Must NOT collapse to OtlpRequestMalformed.
        assert_ne!(public.code(), "WYRD_VALA_400_OTLP_REQUEST_MALFORMED");
    }

    /// Assert that every ingest condition has one Wyrd identity and matching
    /// gRPC status/ErrorInfo projection.
    #[test]
    fn every_ingest_error_has_one_transport_projection() {
        for (error, expected_code, expected_status, expected_grpc) in ingest_cases() {
            let public = error.to_wyrd_error("vala.traces.spans");
            assert_eq!(public.code(), expected_code, "identity drift for {error}");
            assert_eq!(public.status(), expected_status, "status drift for {error}");

            let grpc = error.into_status();
            assert_eq!(grpc.code(), expected_grpc, "gRPC drift for {public}");
            let info = grpc
                .get_details_error_info()
                .expect("canonical ingest status carries ErrorInfo");
            assert_eq!(info.reason, expected_code, "ErrorInfo drift for {public}");
        }
    }

    /// Stable ingest saturation alone carries the exact gRPC retry marker.
    #[test]
    fn ingest_busy_grpc_retry_metadata() {
        let busy = IngestError::IngestBusy {
            table: "vala.traces.spans".to_owned(),
        }
        .into_status();
        assert_eq!(busy.code(), Code::ResourceExhausted);
        assert_eq!(
            busy.metadata()
                .get("retry-after-ms")
                .and_then(|value| value.to_str().ok()),
            Some("1000")
        );
        for permanent in capacity_cases() {
            assert!(
                permanent
                    .0
                    .into_status()
                    .metadata()
                    .get("retry-after-ms")
                    .is_none()
            );
        }
    }

    /// Assert that Scribe remains domain-only while Gate supplies stable
    /// transport identity and preserves the Scribe-provided table where present.
    #[test]
    fn scribe_errors_project_through_gate_without_fallback_identity() {
        let cases = [
            (
                ScribeError::IngestBusy {
                    table: "vala.metrics.points".to_owned(),
                },
                "WYRD_VALA_429_INGEST_BUSY",
                "vala.metrics.points",
            ),
            (
                ScribeError::PayloadTooLarge { bytes: 2 },
                "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
                "vala.traces.spans",
            ),
            (
                ScribeError::FingerprintMismatch {
                    table: "vala.logs.records".to_owned(),
                },
                "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
                "vala.logs.records",
            ),
            (
                ScribeError::TooManyRows { rows: 2, limit: 1 },
                "WYRD_VALA_413_INGEST_OVERSIZED",
                "vala.traces.spans",
            ),
            (
                ScribeError::InvalidFrame,
                "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
                "vala.traces.spans",
            ),
            (
                ScribeError::CardScopeDenied,
                "WYRD_VALA_403_BIFROST_CARD_SCOPE",
                "vala.traces.spans",
            ),
            (
                ScribeError::CardUnresolved,
                "WYRD_VALA_403_CARD_UNRESOLVED",
                "vala.traces.spans",
            ),
            (
                ScribeError::IngressClosed,
                "WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE",
                "vala.traces.spans",
            ),
            (
                ScribeError::WalDiskFull,
                "WYRD_VALA_507_WAL_DISK_FULL",
                "vala.traces.spans",
            ),
            (
                ScribeError::UnsupportedWalVersion { version: 2 },
                "WYRD_VALA_500_BIFROST_INTERNAL",
                "vala.traces.spans",
            ),
            (
                ScribeError::Internal {
                    detail: "broken".to_owned(),
                },
                "WYRD_VALA_500_BIFROST_INTERNAL",
                "vala.traces.spans",
            ),
        ];

        for (scribe_error, expected_code, table) in cases {
            let ingest_error = IngestError::from_scribe(scribe_error);
            let public = ingest_error.to_wyrd_error(table);
            assert_eq!(public.code(), expected_code);
            if expected_code == "WYRD_VALA_429_INGEST_BUSY" {
                assert_eq!(
                    public.to_string(),
                    "ingest writer busy: vala.metrics.points"
                );
            }
        }
    }
}
