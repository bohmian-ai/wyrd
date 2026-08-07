//! Scribe contracts per CONTRACTS §5.
//!
//! The `Scribe` trait is the boundary between the Gate (dispatch) and the
//! Scribe (WAL + memtable + seal). Every method is async; every error is
//! `ScribeError`.
//!
//! `Scribe::ingest_frame` returns a `FrameAdmission` only after the batch has
//! been WAL-synced and inserted into the active memtable. Idempotency is
//! tracked by the batch identity `batch_id` during retention; the recovery
//! path keys off the batch and seal key.

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use bytes::Bytes;
use wyrd_runtime::principal::Principal;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::{TableRef, TenantTableBinding};
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::stream_identity::StreamIdentity;

/// Computes the user-visible fingerprint from a physical Scribe Arrow schema.
///
/// Tail planning and fenced reads use this same projection as ingest admission:
/// server-owned correlation and `wyrd_*` fields never change a table's user
/// schema identity.
#[must_use]
pub(crate) fn projected_source_schema_fingerprint(
    schema: &arrow::datatypes::Schema,
) -> SchemaFingerprint {
    let fields = schema
        .fields()
        .iter()
        .filter(|field| {
            !matches!(
                field.name().as_str(),
                wyrd_spec::vala::CARD_REF
                    | wyrd_spec::vala::CARD_UID
                    | wyrd_spec::vala::PRINCIPAL_ID
                    | "run_id"
                    | "data_tenant_id"
            ) && !field.name().starts_with("wyrd_")
        })
        .map(|field| field.as_ref().clone())
        .collect::<Vec<_>>();
    SchemaFingerprint::from_arrow_schema(&arrow::datatypes::Schema::new(fields))
}

/// One fully resolved batch crossing the Gate-to-Scribe boundary.
#[derive(Debug)]
pub struct ScribeIngressFrame {
    /// The server-verified principal that owns the write.
    pub principal: Principal,
    /// The server-resolved tenant/table binding.
    pub binding: TenantTableBinding,
    /// The catalog fingerprint expected for the source payload.
    pub expected_schema_fingerprint: SchemaFingerprint,
    /// The request correlation identifier.
    pub request_id: RequestId,
    /// The client idempotency identifier for this batch.
    pub batch_id: uuid::Uuid,
    /// The server-created audit event for this batch.
    pub audit_event: AuditEvent,
    /// Server-measured bytes after transport decompression.
    pub measured_wire_bytes: usize,
    /// Native Arrow IPC or a protocol-specific projected Arrow payload.
    pub payload: IngressPayload,
}

/// In-process projected-frame adapter retained for engine-only tests and
/// benchmark fixtures. Public transports use [`ScribeIngressFrame`] directly.
#[derive(Debug, Clone)]
pub struct ScribeAppend {
    /// Server-verified principal.
    pub principal: Principal,
    /// Server-resolved logical table.
    pub table: TableRef,
    /// Already projected Arrow rows.
    pub rows: RecordBatch,
    /// Source schema fingerprint.
    pub schema_fingerprint: SchemaFingerprint,
    /// Correlation identifier.
    pub request_id: RequestId,
    /// Frame identity batch identifier.
    pub batch_id: uuid::Uuid,
    /// Server-measured source bytes.
    pub measured_wire_bytes: usize,
}

/// Payload forms accepted by the transport-neutral Scribe boundary.
#[derive(Debug)]
pub enum IngressPayload {
    /// One self-contained Arrow IPC stream from the native transport.
    ArrowIpc(Bytes),
    /// Arrow batches projected by a protocol adapter such as OTLP.
    ProjectedArrow(Vec<RecordBatch>),
}

/// The portion of a batch admission visible to the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameAdmission {
    /// Exact batch identity durably admitted by Scribe.
    pub batch_id: uuid::Uuid,
    /// Number of rows durably admitted into the active ingest pipeline.
    pub rows_accepted: u64,
}

/// Scribe-layer errors per CONTRACTS §10.
#[derive(Debug, thiserror::Error)]
pub enum ScribeError {
    #[error("ingest busy for table: {table}")]
    IngestBusy { table: String },

    #[error("WAL disk full")]
    WalDiskFull,

    #[error("unsupported WAL version: {version}")]
    UnsupportedWalVersion { version: u16 },

    #[error("ingest payload too large: {bytes} bytes")]
    PayloadTooLarge { bytes: usize },

    #[error("schema fingerprint mismatch for table: {table}")]
    FingerprintMismatch { table: String },

    #[error("ingest request has too many rows: {rows} > {limit}")]
    TooManyRows { rows: u64, limit: u64 },

    #[error("ingest frame validation failed")]
    InvalidFrame,

    /// A caller-supplied `wyrd_event_time` value falls outside the server
    /// acceptance window evaluated against per-batch receipt time.
    ///
    /// The entire batch is rejected pre-admission; no rows are written. The
    /// caller must supply a value within the configured window or omit the
    /// column to let the server stamp receipt time.
    #[error(
        "wyrd_event_time {value_micros} µs is outside acceptance window \
         [{past_bound_micros}, {future_bound_micros}] µs"
    )]
    EventTimeOutOfRange {
        /// The offending `wyrd_event_time` value in epoch-microseconds.
        value_micros: i64,
        /// The inclusive past bound (receipt − past window) in epoch-microseconds.
        past_bound_micros: i64,
        /// The inclusive future bound (receipt + future window) in epoch-microseconds.
        future_bound_micros: i64,
    },

    #[error("ingest card scope validation failed")]
    CardScopeDenied,

    #[error("ingest card identity could not be resolved")]
    CardUnresolved,

    #[error("ingress dispatcher is closed")]
    IngressClosed,

    #[error("object store PUT failed")]
    ObjectStorePutFailed(#[source] opendal::Error),

    #[error("live-tail stream mismatch: requested={requested}, actual={actual}")]
    StreamMismatch {
        requested: StreamIdentity,
        actual: StreamIdentity,
    },

    #[error("internal scribe failure: {detail}")]
    Internal { detail: String },
}

impl ScribeError {
    /// Create an owned completion-safe copy for every request in one failed
    /// shard group. Transport errors may carry non-cloneable source errors,
    /// so those are reduced to the stable internal category at this boundary.
    pub(crate) fn completion_copy(&self) -> Self {
        match self {
            Self::IngestBusy { table } => Self::IngestBusy {
                table: table.clone(),
            },
            Self::WalDiskFull => Self::WalDiskFull,
            Self::UnsupportedWalVersion { version } => {
                Self::UnsupportedWalVersion { version: *version }
            }
            Self::PayloadTooLarge { bytes } => Self::PayloadTooLarge { bytes: *bytes },
            Self::FingerprintMismatch { table } => Self::FingerprintMismatch {
                table: table.clone(),
            },
            Self::TooManyRows { rows, limit } => Self::TooManyRows {
                rows: *rows,
                limit: *limit,
            },
            Self::InvalidFrame => Self::InvalidFrame,
            Self::EventTimeOutOfRange {
                value_micros,
                past_bound_micros,
                future_bound_micros,
            } => Self::EventTimeOutOfRange {
                value_micros: *value_micros,
                past_bound_micros: *past_bound_micros,
                future_bound_micros: *future_bound_micros,
            },
            Self::CardScopeDenied => Self::CardScopeDenied,
            Self::CardUnresolved => Self::CardUnresolved,
            Self::IngressClosed => Self::IngressClosed,
            Self::ObjectStorePutFailed(error) => Self::Internal {
                detail: format!("object store PUT failed: {error}"),
            },
            Self::StreamMismatch { requested, actual } => Self::StreamMismatch {
                requested: *requested,
                actual: *actual,
            },
            Self::Internal { detail } => Self::Internal {
                detail: detail.clone(),
            },
        }
    }
}

impl From<vala_sql::SqlError> for ScribeError {
    fn from(e: vala_sql::SqlError) -> Self {
        Self::Internal {
            detail: e.to_string(),
        }
    }
}

/// Scribe trait — the durable write boundary.
///
/// `FrameAdmission` is the durable Scribe acknowledgment. It is returned only
/// after WAL append, grouped `sync_data`, and active memtable insertion.
#[async_trait]
pub trait Scribe: Send + Sync {
    /// Report whether recovery completed and the durable write path accepts work.
    ///
    /// Implementations that do not have a startup recovery phase are ready by
    /// default. The server-owned Scribe overrides this while replay is active
    /// or has failed.
    fn is_ready(&self) -> bool {
        true
    }

    /// Validate, prepare, and admit one resolved frame.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError>;

    /// Adapt an already-projected in-process frame to the resolved seam and
    /// return the durable batch acknowledgment.
    /// Network transports never call this method.
    async fn append_durable(&self, req: ScribeAppend) -> Result<FrameAdmission, ScribeError> {
        if req
            .rows
            .schema()
            .index_of(wyrd_spec::vala::WYRD_EVENT_TIME)
            .is_err()
        {
            return Err(ScribeError::Internal {
                detail: "projected append is missing wyrd_event_time".to_owned(),
            });
        }
        let binding = TenantTableBinding::resolve((req.principal.tenant_id, req.table.clone()))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        if req.schema_fingerprint
            != SchemaFingerprint::from_arrow_schema(req.rows.schema().as_ref())
        {
            return Err(ScribeError::FingerprintMismatch {
                table: req.table.fqn(),
            });
        }
        let audit_event = AuditEvent {
            request_id: req.request_id.clone(),
            trace_id: None,
            operation: "bifrost.append".to_owned(),
            resource: req.table.fqn(),
            card_ref: req.principal.card_ref().cloned(),
            principal_id: req.principal.id,
            principal_kind: req.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:append".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: format!("{} rows", req.rows.num_rows()),
            detail: None,
        };
        self.ingest_frame(ScribeIngressFrame {
            principal: req.principal,
            binding,
            expected_schema_fingerprint: projected_source_schema_fingerprint(
                req.rows.schema().as_ref(),
            ),
            request_id: req.request_id,
            batch_id: req.batch_id,
            audit_event,
            measured_wire_bytes: req.measured_wire_bytes,
            payload: IngressPayload::ProjectedArrow(vec![req.rows]),
        })
        .await
    }

    /// Adapt an already-projected in-process frame to the resolved seam.
    /// Network transports use the unary wire response with the same identity.
    async fn append(&self, req: ScribeAppend) -> Result<(), ScribeError> {
        self.append_durable(req).await.map(|_| ())
    }
}
