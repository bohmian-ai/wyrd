//! Scribe contracts per CONTRACTS §5.
//!
//! The `Scribe` trait is the boundary between the Gate (dispatch) and the
//! Scribe (WAL + memtable + seal). Resource reservation and readiness checks
//! are synchronous; logical-frame ingestion is asynchronous through its
//! durability boundary. Fallible operations return `ScribeError`.
//!
//! `Scribe::ingest_frame` returns a `FrameAdmission` only after the batch has
//! been WAL-synced and inserted into the active memtable. Idempotency is
//! tracked by the batch identity `batch_id` during retention; the recovery
//! path keys off the batch and seal key.
//!
//! `OracleQueryDispatch` is the read-side counterpart: the seam through which
//! Gate reaches an Oracle it does not own. Unlike `Scribe`, its signature is
//! stated in Oracle types, so this module depends on `crate::oracle`. That
//! direction is deliberate and acyclic — `crate::oracle` never imports this
//! module — and is recorded here so it does not read as drift.

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use bytes::Bytes;
use wyrd_runtime::principal::Principal;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuditEvent;

use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::error::BifrostError;

use crate::catalog::TableRef;
use crate::oracle::{AuthorizedQueryContext, OracleQueryStream};
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

/// One logical batch crossing the Gate-to-Scribe boundary.
#[derive(Debug)]
pub struct ScribeIngressFrame {
    /// The server-verified principal that owns the write.
    pub principal: Principal,
    /// Authenticated tenant selected by the transport boundary.
    pub authenticated_tenant: wyrd_spec::DataTenantId,
    /// Requested logical table, unchanged by physical resolution.
    pub table: TableRef,
    /// Engine-only expected fingerprint for already projected fixture rows.
    pub expected_schema_fingerprint: Option<SchemaFingerprint>,
    /// The request correlation identifier.
    pub request_id: RequestId,
    /// The client idempotency identifier for this batch.
    pub batch_id: uuid::Uuid,
    /// The server-created audit event for this batch.
    pub audit_event: AuditEvent,
    /// Server-measured bytes after transport decompression.
    pub measured_wire_bytes: usize,
    /// Native Arrow IPC, engine-only projected Arrow, or a fixed-capacity
    /// typed OTLP request paired with its move-only transport-decode owner.
    pub payload: IngressPayload,
}

/// In-process projected-frame adapter retained for engine-only tests and
/// benchmark fixtures. Gate alone constructs `ScribeIngressFrame` for public
/// transport traffic.
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
///
/// Public OTLP adapters transfer typed, fixed-capacity requests and their
/// decode owners through Gate; they do not project Arrow before Scribe.
#[derive(Debug)]
pub enum IngressPayload {
    /// One self-contained Arrow IPC stream from the native transport.
    ArrowIpc(Bytes),
    /// Engine-internal preprojected Arrow batches used outside public OTLP
    /// adapter and Gate routing.
    ProjectedArrow(Vec<RecordBatch>),
    /// Fixed-capacity typed OTLP trace request plus its move-only decode owner.
    OtlpTraces(DecodedOtlp<wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest>),
    /// Fixed-capacity typed OTLP metrics request plus its move-only decode owner.
    OtlpMetrics(DecodedOtlp<wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest>),
    /// Fixed-capacity typed OTLP logs request plus its move-only decode owner.
    OtlpLogs(DecodedOtlp<wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest>),
}

/// Move-only adapter-decoded OTLP request paired with its root-backed owner.
#[derive(Debug)]
pub struct DecodedOtlp<T> {
    /// Fixed-capacity typed request produced by the server adapter.
    pub request: Box<T>,
    /// Actual encoded bytes observed before typed decoding.
    pub wire_bytes: usize,
    /// Exact generated-request capacity admitted before typed construction.
    pub decode_bytes: usize,
    /// Decode allocation owner adopted by Scribe's complete admission root.
    pub(crate) owner: Option<OtlpDecodeOwner>,
}

impl<T> DecodedOtlp<T> {
    /// Couples one typed adapter result to the exact wire fact and decode owner.
    #[must_use]
    pub fn new(request: T, wire_bytes: usize, owner: OtlpDecodeOwner) -> Self {
        let decode_bytes = owner.bytes();
        Self {
            request: Box::new(request),
            wire_bytes,
            decode_bytes,
            owner: Some(owner),
        }
    }
}

/// Opaque move-only ownership of server-side OTLP decode capacity.
#[derive(Debug)]
pub struct OtlpDecodeOwner {
    /// Move-only transport-decode child adopted into Scribe's complete root.
    pub(crate) memory: crate::resources::ScribeMemoryLease,
}

/// Temporary adapter-decode capacity split from an OTLP typed owner.
///
/// The server adapter keeps this guard alive while it uses bounded lexical
/// scratch. Dropping the guard returns only that scratch capacity, leaving the
/// paired [`OtlpDecodeOwner`] with the exact generated-request capacity that
/// crosses Gate into Scribe.
#[derive(Debug)]
pub struct OtlpDecodeScratch {
    /// Root-backed scratch lease released when adapter construction finishes.
    _memory: crate::resources::ScribeMemoryLease,
}

impl OtlpDecodeOwner {
    /// Returns the exact adapter-decode capacity retained by this owner.
    #[must_use]
    fn bytes(&self) -> usize {
        self.memory.bytes()
    }

    /// Splits bounded temporary scratch from the retained typed-request owner.
    ///
    /// The split performs no second admission. The returned guard must remain
    /// live for the complete adapter construction pass and be dropped before
    /// this owner is paired with the decoded request.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when `bytes` exceeds the capacity
    /// admitted for this owner or root accounting cannot perform the split.
    pub fn split_scratch(&mut self, bytes: usize) -> Result<OtlpDecodeScratch, ScribeError> {
        let memory = self
            .memory
            .split(bytes)
            .map_err(|error| ScribeError::Internal {
                detail: format!("OTLP decode scratch split failed: {error}"),
            })?;
        Ok(OtlpDecodeScratch { _memory: memory })
    }

    /// Atomically grows the decode child into Scribe's one complete root.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] without losing this owner when root capacity
    /// cannot cover the immutable material plan.
    pub(crate) fn complete(
        mut self,
        root_bytes: usize,
    ) -> Result<crate::resources::ScribeMemoryLease, ScribeError> {
        self.memory.resize_ingress(root_bytes)?;
        Ok(self.memory)
    }
}

/// The portion of a batch admission visible to the transport.
#[derive(Debug)]
pub struct FrameAdmission {
    /// Exact batch identity durably admitted by Scribe.
    pub batch_id: uuid::Uuid,
    /// Number of rows durably admitted into the active ingest pipeline.
    pub rows_accepted: u64,
    /// OTLP projection result returned to its transport adapter.
    pub otlp_outcome: Option<ScribeOtlpOutcome>,
}

/// Closed OTLP projection outcome returned through the private Scribe seam.
#[derive(Debug, Clone)]
pub enum ScribeOtlpOutcome {
    /// Trace export counts and partial-success detail.
    Traces(crate::otlp_contract::IngestOutcome),
    /// Metrics export counts and partial-success detail.
    Metrics(crate::otlp_contract::MetricsOutcome),
    /// Log export counts and partial-success detail.
    Logs(crate::otlp_contract::LogsOutcome),
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

    /// The decoded Arrow ownership plus its measured wire frame cannot fit in
    /// one active persistence bucket, so no WAL or memtable work was started.
    #[error("decoded ingest payload too large: {bytes} > {limit} bytes")]
    DecodedPayloadTooLarge {
        /// Combined decoded Arrow and measured wire bytes.
        bytes: usize,
        /// Active-bucket ceiling applied to this request.
        limit: usize,
    },

    #[error("schema fingerprint mismatch for table: {table}")]
    FingerprintMismatch { table: String },

    /// The tenant-scoped logical table does not exist in the Scribe-owned catalog.
    #[error("bifrost table not found: {table}")]
    TableNotFound {
        /// Fully-qualified logical table requested by the authenticated writer.
        table: String,
    },

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
            Self::DecodedPayloadTooLarge { bytes, limit } => Self::DecodedPayloadTooLarge {
                bytes: *bytes,
                limit: *limit,
            },
            Self::FingerprintMismatch { table } => Self::FingerprintMismatch {
                table: table.clone(),
            },
            Self::TableNotFound { table } => Self::TableNotFound {
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

/// The one seam through which Gate reaches an Oracle it does not own.
///
/// Gate owns authentication, admission closure, and the closed request-metric
/// taxonomy for a public SQL query; it does not own role selection. A
/// server-tier implementation may execute locally, forward to a fenced peer, or
/// refuse. Gate treats every outcome identically: it hands the authorized
/// request across and accounts for whatever comes back.
///
/// Typed logical plans are deliberately absent. They are local-only, never
/// forwarded, and carry no Gate request metrics, so routing them through this
/// seam would invent behavior.
#[async_trait]
pub trait OracleQueryDispatch: Send + Sync + 'static {
    /// Executes one already-authenticated public SQL request.
    ///
    /// The implementation owns role selection and every retry or fencing
    /// decision it needs; Gate observes only the returned stream or error.
    ///
    /// # Errors
    ///
    /// Returns the stable Bifrost validation, catalog, role, transport,
    /// security, admission, timeout, or execution failure.
    /// [`BifrostError::OracleRoleUnavailable`] means no Oracle was reachable,
    /// whether local or remote.
    async fn dispatch_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, BifrostError>;
}

/// Durable transport-neutral Scribe write boundary.
///
/// `FrameAdmission` is the durable Scribe acknowledgment. It is returned only
/// after WAL append, grouped `sync_data`, and active memtable insertion. Its
/// Decode reservation and readiness are synchronous; ingest performs
/// asynchronous durability work. The server composition facade is the only
/// external producer of owned logical frames.
#[async_trait]
pub trait Scribe: Send + Sync {
    /// Acquires exact root-backed capacity before an OTLP adapter decodes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when another owner occupies the
    /// ingress envelope, or [`ScribeError::Internal`] when accounting fails or
    /// an implementation does not expose transport-decode ownership.
    fn reserve_otlp_decode(&self, _bytes: usize) -> Result<OtlpDecodeOwner, ScribeError> {
        Err(ScribeError::Internal {
            detail: "test Scribe does not expose decode ownership".to_owned(),
        })
    }
    /// Report whether recovery completed and the durable write path accepts work.
    ///
    /// Implementations that do not have a startup recovery phase are ready by
    /// default. The server-owned Scribe overrides this while replay is active
    /// or has failed.
    fn is_ready(&self) -> bool {
        true
    }

    /// Validate, resolve, project, and durably admit one logical frame.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when validation, resolution, projection,
    /// admission, persistence, or durable acknowledgment fails.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError>;
}
