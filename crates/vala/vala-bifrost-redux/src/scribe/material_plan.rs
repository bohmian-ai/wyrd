//! Synchronous, pre-allocation material planning for Scribe ingress.
//!
//! The planner borrows transport-owned input. Native planning validates the
//! closed Arrow IPC V1 subset without invoking an Arrow reader. OTLP planning
//! walks the already framework-decoded typed request without building domain
//! records or Arrow arrays.

use std::io::Write;
use std::mem::size_of;

use arrow::ipc::writer::StreamWriter;
use arrow::ipc::{
    DateUnit, Endianness, IntervalUnit, MessageHeader, MetadataVersion, Precision, TimeUnit, Type,
    root_as_message,
};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, NumberDataPoint, SummaryDataPoint,
    metric,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::contracts::ScribeError;

/// Maximum native record-batch descriptors retained by one plan.
pub(crate) const MAX_SOURCE_PLANS: usize = 64;
/// Maximum top-level fields in the canonical native schema.
pub(crate) const MAX_NATIVE_FIELDS: usize = 256;
/// Metadata-only facts for one current source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SourceMaterialPlan {
    /// Rows declared by the source metadata.
    pub(crate) rows: usize,
    /// Body or projected bytes owned while this source is current.
    pub(crate) body_bytes: usize,
    /// Buffer range count validated for this source.
    pub(crate) buffers: usize,
    /// Start of this record-batch framing word in retained transport bytes.
    pub(crate) frame_start: usize,
    /// Start of this record-batch body after framed metadata and padding.
    pub(crate) body_start: usize,
    /// Exclusive end of the declared record-batch body.
    pub(crate) body_end: usize,
    /// Exclusive end including public eight-byte stream padding.
    pub(crate) frame_end: usize,
}

/// Closed ingress path represented by a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IngestPath {
    /// Canonical V1 Arrow IPC.
    Native,
    /// Framework-decoded typed OTLP.
    Otlp,
}

/// Immutable facts used by one complete Scribe root admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IngestMaterialPlan {
    /// Transport path covered by this plan.
    pub(crate) path: IngestPath,
    /// Encoded request bytes retained through admission.
    pub(crate) request_bytes: usize,
    /// Public layout bytes occupied by this fixed planner value.
    pub(crate) planner_bytes: usize,
    /// Bounded physical-name and binding construction bytes.
    pub(crate) name_bytes: usize,
    /// Admitted overlap for a native aligned source copy.
    pub(crate) aligned_copy_bytes: usize,
    /// Start of the canonical schema message framing word.
    pub(crate) native_schema_start: usize,
    /// Exclusive end of the schema message including metadata padding.
    pub(crate) native_schema_end: usize,
    /// Exact public Arrow schema ownership decoded after root admission.
    pub(crate) native_schema_material_bytes: usize,
    /// Largest current framed-metadata scratch retained by `StreamDecoder`.
    pub(crate) native_metadata_scratch_bytes: usize,
    /// Fixed-capacity source descriptors.
    pub(crate) sources: [SourceMaterialPlan; MAX_SOURCE_PLANS],
    /// Live prefix length in `sources`.
    pub(crate) source_count: usize,
    /// Total logical rows counted without materialization.
    pub(crate) rows: usize,
    /// Distinct time partitions this plan admits for the request.
    ///
    /// Always at least one for a non-empty request: an OTLP export derives one
    /// partition from its single receipt time, while a native or projected
    /// Arrow source is admitted for the full per-source ceiling because its
    /// rows are not decoded during planning. The value therefore matches the
    /// durable slice metadata the plan reserves rather than being left at zero.
    pub(crate) time_partition_count: usize,
    /// Largest current-only decoded or projected source.
    pub(crate) current_material_bytes: usize,
    /// Aggregate native rows transferred into active memtable ownership.
    pub(crate) active_output_bytes: usize,
    /// Conservative Arrow bytes that can form one later file candidate.
    pub(crate) persistence_candidate_bytes: usize,
    /// Authorized fixed backing ceiling for WAL-durable slice descriptors.
    pub(crate) durable_metadata_bytes: usize,
    /// Fixed WAL framing/digest workspace.
    pub(crate) wal_workspace_bytes: usize,
    /// Conservative immutable-plus-serial-workspace replayability peak.
    pub(crate) persistence_replay_bytes: usize,
    /// Complete simultaneous-live-set charge.
    pub(crate) root_bytes: usize,
}

/// Canonical two-phase materialization plan used by every Scribe producer.
///
/// The alias keeps the existing field-level planner representation while
/// making the preflight owner explicit at ingress and recovery call sites.
pub(crate) type MaterialPlan = IngestMaterialPlan;

/// Derives the largest configured preflight or persistence envelope.
///
/// Native buffers cannot exceed the configured frame, while typed OTLP may
/// retain its encoded request, bounded value material, and managed Wyrd columns
/// together. The result includes the durable slice layout, WAL workspace, and
/// the same candidate-local persistence projection used after materialization.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when configured bound
/// arithmetic cannot be represented on this platform.
pub(crate) fn configured_maximum_envelope_bytes(
    limits: crate::gate::limits::IngestLimits,
) -> Result<usize, ScribeError> {
    let overflow = || ScribeError::DecodedPayloadTooLarge {
        bytes: usize::MAX,
        limit: usize::MAX,
    };
    let managed = managed_projection_bytes(limits.rows, 36)?;
    let native_candidate = limits
        .max_frame_bytes
        .checked_add(managed)
        .ok_or_else(overflow)?;
    let otlp_candidate = limits
        .otlp
        .request_bytes
        .checked_add(limits.otlp.value_bytes)
        .and_then(|bytes| bytes.checked_add(managed))
        .ok_or_else(overflow)?;
    let candidate = native_candidate.max(otlp_candidate);
    let durable_metadata = limits
        .native_sources
        .max(1)
        .checked_mul(limits.otlp.time_partitions.max(1))
        .and_then(|count| count.checked_mul(crate::scribe::shards::DURABLE_SLICE_LAYOUT_BYTES))
        .ok_or_else(overflow)?;
    let preflight = limits
        .max_frame_bytes
        .checked_add(candidate)
        .and_then(|bytes| bytes.checked_add(durable_metadata))
        .and_then(|bytes| bytes.checked_add(limits.wal_workspace_bytes))
        .ok_or_else(overflow)?;
    let persistence = candidate
        .checked_add(crate::scribe::memory::parquet_candidate_incremental_bytes(
            candidate,
        )?)
        .ok_or_else(overflow)?;
    Ok(preflight.max(persistence))
}

/// Pre-mutation decision for one plan against the node's replayable envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MaximumEnvelopeDecision {
    /// The complete planned live set can be accepted and replayed.
    Fits,
    /// The request can never fit this node even when all temporary occupancy drains.
    IntrinsicRefusal {
        /// Request-specific upper-bound demand computed before materialization.
        demand_bytes: usize,
        /// Maximum root envelope available to Scribe on this node.
        limit_bytes: usize,
    },
}

impl IngestMaterialPlan {
    /// Classifies intrinsic envelope compatibility without observing temporary occupancy.
    #[must_use]
    pub(crate) fn maximum_envelope_decision(
        &self,
        maximum_scribe_envelope_bytes: usize,
    ) -> MaximumEnvelopeDecision {
        let demand_bytes = self.root_bytes.max(self.persistence_replay_bytes);
        if demand_bytes <= maximum_scribe_envelope_bytes {
            MaximumEnvelopeDecision::Fits
        } else {
            MaximumEnvelopeDecision::IntrinsicRefusal {
                demand_bytes,
                limit_bytes: maximum_scribe_envelope_bytes,
            }
        }
    }

    /// Completes checked simultaneous-live-set arithmetic.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::DecodedPayloadTooLarge`] on checked overflow.
    /// The existing T5A Scribe root applies the authoritative live-capacity
    /// refusal to the resulting exact charge.
    fn finish(mut self) -> Result<Self, ScribeError> {
        self.root_bytes = [
            self.request_bytes,
            self.planner_bytes,
            self.name_bytes,
            self.aligned_copy_bytes,
            self.current_material_bytes,
            self.active_output_bytes,
            self.durable_metadata_bytes,
            self.wal_workspace_bytes,
        ]
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .ok_or(ScribeError::DecodedPayloadTooLarge {
            bytes: usize::MAX,
            limit: usize::MAX,
        })?;
        let candidate_upper_bound = self.persistence_candidate_bytes;
        self.persistence_replay_bytes = candidate_upper_bound
            .checked_add(crate::scribe::memory::parquet_candidate_incremental_bytes(
                candidate_upper_bound,
            )?)
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: usize::MAX,
            })?;
        Ok(self)
    }
}

/// Scribe-owned planner for native and typed OTLP ingress.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScribeIngressPlanner {
    /// Immutable operator-selected limits validated by server boot.
    limits: crate::gate::limits::IngestLimits,
}

impl ScribeIngressPlanner {
    /// Constructs one planner from the same frozen limits snapshot used by Gate.
    #[must_use]
    pub(crate) const fn new(limits: crate::gate::limits::IngestLimits) -> Self {
        Self { limits }
    }
}

impl Default for ScribeIngressPlanner {
    /// Uses immutable V1 maxima for embedded and unit-test construction.
    fn default() -> Self {
        Self::new(crate::gate::limits::IngestLimits::default())
    }
}

/// Scalar buffer layout retained from the schema for batch validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum NativeFieldLayout {
    /// Null values have a node and no buffers.
    #[default]
    Null,
    /// Boolean values use validity and bit-packed value buffers.
    Boolean,
    /// Fixed-width values use validity and exact-width value buffers.
    Fixed(usize),
    /// Variable values use validity, offset, and value buffers.
    Variable(usize),
}

/// Owns the bounded state accumulated while scanning one native Arrow stream.
struct NativeScan {
    /// Frozen ingress limits shared with the public planner.
    limits: crate::gate::limits::IngestLimits,
    /// Current aligned frame cursor.
    cursor: usize,
    /// Whether the single required schema has been observed.
    schema_seen: bool,
    /// Inclusive schema-frame start.
    schema_start: usize,
    /// Exclusive aligned schema-frame end.
    schema_end: usize,
    /// Whether the terminal end-of-stream marker has been observed.
    eos_seen: bool,
    /// Number of schema fields used to validate every batch.
    fields: usize,
    /// Closed layout for each schema field.
    field_layouts: [NativeFieldLayout; MAX_NATIVE_FIELDS],
    /// Nullability for each schema field.
    field_nullable: [bool; MAX_NATIVE_FIELDS],
    /// Number of admitted record-batch sources.
    source_count: usize,
    /// Aggregate admitted rows.
    rows: usize,
    /// Largest frame metadata region required as decode scratch.
    max_metadata_bytes: usize,
    /// Exact schema object and retained string material.
    schema_material_bytes: usize,
    /// Aggregate active Arrow and managed-column output.
    active_output_bytes: usize,
    /// Fixed source descriptors populated in traversal order.
    sources: [SourceMaterialPlan; MAX_SOURCE_PLANS],
}

impl NativeScan {
    /// Creates an empty scan under one frozen limit snapshot.
    fn new(limits: crate::gate::limits::IngestLimits) -> Self {
        Self {
            limits,
            cursor: 0,
            schema_seen: false,
            schema_start: 0,
            schema_end: 0,
            eos_seen: false,
            fields: 0,
            field_layouts: [NativeFieldLayout::Null; MAX_NATIVE_FIELDS],
            field_nullable: [false; MAX_NATIVE_FIELDS],
            source_count: 0,
            rows: 0,
            max_metadata_bytes: 0,
            schema_material_bytes: size_of::<arrow::datatypes::Schema>(),
            active_output_bytes: 0,
            sources: [SourceMaterialPlan::default(); MAX_SOURCE_PLANS],
        }
    }

    /// Scans framing and delegates schema and batch invariants to focused stages.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for malformed framing, metadata,
    /// message order, body ranges, or a missing terminal marker.
    fn scan(&mut self, bytes: &Bytes) -> Result<(), ScribeError> {
        while self.cursor < bytes.len() {
            let frame_start = self.cursor;
            let prefix = read_u32(bytes, self.cursor)?;
            self.cursor = self
                .cursor
                .checked_add(4)
                .ok_or(ScribeError::InvalidFrame)?;
            let metadata_len = if prefix == u32::MAX {
                let length = read_u32(bytes, self.cursor)?;
                self.cursor = self
                    .cursor
                    .checked_add(4)
                    .ok_or(ScribeError::InvalidFrame)?;
                length
            } else {
                prefix
            };
            if metadata_len == 0 {
                self.eos_seen = self.cursor == bytes.len();
                break;
            }
            let metadata_len =
                usize::try_from(metadata_len).map_err(|_| ScribeError::InvalidFrame)?;
            let metadata_end = self
                .cursor
                .checked_add(metadata_len)
                .ok_or(ScribeError::InvalidFrame)?;
            let metadata = bytes
                .get(self.cursor..metadata_end)
                .ok_or(ScribeError::InvalidFrame)?;
            let message = root_as_message(metadata).map_err(|_| ScribeError::InvalidFrame)?;
            if message.version() != MetadataVersion::V5 {
                return Err(ScribeError::InvalidFrame);
            }
            self.cursor = align_eight(metadata_end)?;
            let body_len =
                usize::try_from(message.bodyLength()).map_err(|_| ScribeError::InvalidFrame)?;
            let body_end = self
                .cursor
                .checked_add(body_len)
                .ok_or(ScribeError::InvalidFrame)?;
            if body_end > bytes.len() {
                return Err(ScribeError::InvalidFrame);
            }
            match message.header_type() {
                MessageHeader::Schema => {
                    self.visit_schema(&message, frame_start, body_len)?;
                }
                MessageHeader::RecordBatch => {
                    self.visit_batch(&message, bytes, frame_start, body_end)?;
                }
                _ => return Err(ScribeError::InvalidFrame),
            }
            self.cursor = align_eight(body_end)?;
        }
        if !self.schema_seen || self.source_count == 0 || self.rows == 0 || !self.eos_seen {
            return Err(ScribeError::InvalidFrame);
        }
        Ok(())
    }

    /// Validates and retains the stream's single leading schema.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for ordering, schema features,
    /// unsupported field layouts, or field-count violations, and a material
    /// error when checked schema accounting overflows.
    fn visit_schema(
        &mut self,
        message: &arrow::ipc::Message<'_>,
        frame_start: usize,
        body_len: usize,
    ) -> Result<(), ScribeError> {
        if self.schema_seen || self.source_count != 0 || body_len != 0 {
            return Err(ScribeError::InvalidFrame);
        }
        let schema = message
            .header_as_schema()
            .ok_or(ScribeError::InvalidFrame)?;
        if schema.endianness() != Endianness::Little
            || schema.features().is_some_and(|value| !value.is_empty())
            || schema
                .custom_metadata()
                .is_some_and(|value| !value.is_empty())
        {
            return Err(ScribeError::InvalidFrame);
        }
        let schema_fields = schema.fields().ok_or(ScribeError::InvalidFrame)?;
        self.fields = schema_fields.len();
        if self.fields == 0 || self.fields > self.limits.native_fields {
            return Err(ScribeError::InvalidFrame);
        }
        for (index, field) in schema_fields.into_iter().enumerate() {
            self.field_layouts[index] = native_field_layout(field)?;
            self.field_nullable[index] = field.nullable();
            self.schema_material_bytes = self
                .schema_material_bytes
                .checked_add(size_of::<arrow::datatypes::Field>())
                .and_then(|value| {
                    value.checked_add(size_of::<std::sync::Arc<arrow::datatypes::Field>>())
                })
                .and_then(|value| value.checked_add(field.name().map_or(0, str::len)))
                .and_then(|value| {
                    value.checked_add(
                        field
                            .type_as_timestamp()
                            .and_then(|timestamp| timestamp.timezone())
                            .map_or(0, str::len),
                    )
                })
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?;
        }
        self.schema_seen = true;
        self.schema_start = frame_start;
        self.schema_end = self.cursor;
        self.max_metadata_bytes = self.max_metadata_bytes.max(self.cursor - frame_start);
        Ok(())
    }

    /// Validates one record batch and appends its exact source descriptor.
    ///
    /// # Errors
    ///
    /// Returns stable row, frame, layout, and material errors for invalid batch
    /// metadata, buffer ranges, checked accounting, or configured limits.
    fn visit_batch(
        &mut self,
        message: &arrow::ipc::Message<'_>,
        bytes: &Bytes,
        frame_start: usize,
        body_end: usize,
    ) -> Result<(), ScribeError> {
        if !self.schema_seen || self.source_count == self.limits.native_sources {
            return Err(ScribeError::InvalidFrame);
        }
        let batch = message
            .header_as_record_batch()
            .ok_or(ScribeError::InvalidFrame)?;
        if batch.compression().is_some() || batch.variadicBufferCounts().is_some() {
            return Err(ScribeError::InvalidFrame);
        }
        let batch_rows = usize::try_from(batch.length()).map_err(|_| ScribeError::InvalidFrame)?;
        self.rows = self
            .rows
            .checked_add(batch_rows)
            .ok_or(ScribeError::TooManyRows {
                rows: u64::MAX,
                limit: self.limits.rows as u64,
            })?;
        if self.rows > self.limits.rows {
            return Err(ScribeError::TooManyRows {
                rows: u64::try_from(self.rows).unwrap_or(u64::MAX),
                limit: self.limits.rows as u64,
            });
        }
        let nodes = batch.nodes().ok_or(ScribeError::InvalidFrame)?;
        if nodes.len() != self.fields
            || nodes.into_iter().enumerate().any(|(index, node)| {
                node.length() != batch.length()
                    || node.null_count() < 0
                    || node.null_count() > node.length()
                    || (!self.field_nullable[index] && node.null_count() != 0)
                    || (self.field_layouts[index] == NativeFieldLayout::Null
                        && node.null_count() != node.length())
            })
        {
            return Err(ScribeError::InvalidFrame);
        }
        let buffers = batch.buffers().ok_or(ScribeError::InvalidFrame)?;
        let body = bytes
            .get(self.cursor..body_end)
            .ok_or(ScribeError::InvalidFrame)?;
        let buffer_bytes = validate_buffer_layouts(
            &self.field_layouts[..self.fields],
            nodes.into_iter(),
            buffers.into_iter(),
            body,
        )?;
        self.active_output_bytes = self
            .active_output_bytes
            .checked_add(buffer_bytes)
            .and_then(|value| {
                managed_projection_bytes(batch_rows, 36)
                    .ok()
                    .and_then(|managed| value.checked_add(managed))
            })
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.max_frame_bytes,
            })?;
        self.sources[self.source_count] = SourceMaterialPlan {
            rows: batch_rows,
            body_bytes: buffer_bytes,
            buffers: buffers.len(),
            frame_start,
            body_start: self.cursor,
            body_end,
            frame_end: align_eight(body_end)?,
        };
        self.source_count += 1;
        self.max_metadata_bytes = self.max_metadata_bytes.max(self.cursor - frame_start);
        Ok(())
    }

    /// Freezes a successful scan into the native ingress material plan.
    ///
    /// # Errors
    ///
    /// Returns a material error when checked capacity or root arithmetic overflows.
    fn finish(self, bytes: &Bytes, name_bytes: usize) -> Result<IngestMaterialPlan, ScribeError> {
        let current_material_bytes = self
            .schema_material_bytes
            .checked_add(self.max_metadata_bytes)
            .and_then(|value| value.checked_add(self.active_output_bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.max_frame_bytes,
            })?;
        IngestMaterialPlan {
            path: IngestPath::Native,
            request_bytes: bytes.len(),
            planner_bytes: size_of::<IngestMaterialPlan>(),
            name_bytes,
            aligned_copy_bytes: self.sources[..self.source_count]
                .iter()
                .filter(|source| {
                    (bytes.as_ptr() as usize)
                        .checked_add(source.body_start)
                        .is_none_or(|address| address & 7 != 0)
                })
                .map(|source| source.body_end - source.body_start)
                .max()
                .unwrap_or(0),
            native_schema_start: self.schema_start,
            native_schema_end: self.schema_end,
            native_schema_material_bytes: self.schema_material_bytes,
            native_metadata_scratch_bytes: self.max_metadata_bytes,
            sources: self.sources,
            source_count: self.source_count,
            rows: self.rows,
            time_partition_count: self.limits.otlp.time_partitions,
            current_material_bytes,
            active_output_bytes: self.active_output_bytes,
            persistence_candidate_bytes: self.active_output_bytes,
            durable_metadata_bytes: self
                .source_count
                .checked_mul(self.limits.otlp.time_partitions)
                .and_then(|count| {
                    count.checked_mul(crate::scribe::shards::DURABLE_SLICE_LAYOUT_BYTES)
                })
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?,
            wal_workspace_bytes: self.limits.wal_workspace_bytes,
            persistence_replay_bytes: 0,
            root_bytes: 0,
        }
        .finish()
    }
}

impl ScribeIngressPlanner {
    /// Plans an already projected engine-only fixture from observable Arrow facts.
    ///
    /// Production Gate traffic never uses this path. It preserves the existing
    /// embedded append seam while ensuring Scribe still acquires its root before
    /// physical binding and durable preprocessing.
    ///
    /// # Errors
    ///
    /// Returns a stable row or material refusal on checked overflow or excess.
    pub(crate) fn plan_projected(
        &self,
        batches: &[RecordBatch],
        request_bytes: usize,
        name_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        let mut rows = 0_usize;
        let mut total_bytes = 0_usize;
        let mut max_ipc_bytes = 0_usize;
        for batch in batches {
            rows = rows
                .checked_add(batch.num_rows())
                .ok_or(ScribeError::TooManyRows {
                    rows: u64::MAX,
                    limit: self.limits.rows as u64,
                })?;
            max_ipc_bytes = max_ipc_bytes.max(count_ipc_bytes(batch)?);
            total_bytes = total_bytes
                .checked_add(batch.get_array_memory_size())
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?;
        }
        if rows > self.limits.rows {
            return Err(ScribeError::TooManyRows {
                rows: u64::try_from(rows).unwrap_or(u64::MAX),
                limit: self.limits.rows as u64,
            });
        }
        let mut sources = [SourceMaterialPlan::default(); MAX_SOURCE_PLANS];
        if rows != 0 {
            sources[0] = SourceMaterialPlan {
                rows,
                body_bytes: total_bytes,
                buffers: 0,
                frame_start: 0,
                body_start: 0,
                body_end: 0,
                frame_end: 0,
            };
        }
        let managed_bytes = managed_projection_bytes(rows, 36)?;
        let decoded_bytes =
            total_bytes
                .checked_add(managed_bytes)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?;
        let encoded_bytes = max_ipc_bytes
            .checked_add(managed_bytes)
            .and_then(|value| value.checked_add(self.limits.wal_workspace_bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.max_frame_bytes,
            })?;
        let current_material_bytes = decoded_bytes.checked_add(encoded_bytes).ok_or(
            ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.max_frame_bytes,
            },
        )?;
        IngestMaterialPlan {
            path: IngestPath::Otlp,
            request_bytes,
            planner_bytes: size_of::<IngestMaterialPlan>(),
            name_bytes,
            aligned_copy_bytes: 0,
            native_schema_start: 0,
            native_schema_end: 0,
            native_schema_material_bytes: 0,
            native_metadata_scratch_bytes: 0,
            sources,
            source_count: usize::from(rows != 0),
            rows,
            time_partition_count: self.limits.otlp.time_partitions,
            current_material_bytes,
            active_output_bytes: 0,
            persistence_candidate_bytes: decoded_bytes,
            durable_metadata_bytes: batches
                .len()
                .checked_mul(crate::scribe::shards::DURABLE_SLICE_LAYOUT_BYTES)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?,
            wal_workspace_bytes: self.limits.wal_workspace_bytes,
            persistence_replay_bytes: 0,
            root_bytes: 0,
        }
        .finish()
    }

    /// Scans a canonical native stream without Arrow decode or body allocation.
    ///
    /// # Errors
    ///
    /// Returns stable invalid or material-too-large Scribe errors for malformed
    /// framing, non-V5 metadata, dictionaries, compression, unsupported
    /// layouts, invalid ranges, missing EOS, overflow, or ceiling excess.
    pub(crate) fn plan_native(
        &self,
        bytes: &Bytes,
        name_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        if bytes.len() > self.limits.max_frame_bytes {
            return Err(ScribeError::PayloadTooLarge { bytes: bytes.len() });
        }
        let mut scan = NativeScan::new(self.limits);
        scan.scan(bytes)?;
        scan.finish(bytes, name_bytes)
    }

    /// Counts a typed trace request without projecting records.
    ///
    /// # Errors
    ///
    /// Returns a stable refusal when a V1 cardinality, depth, byte, or
    /// projected-material ceiling is exceeded.
    pub(crate) fn plan_traces(
        &self,
        request: &ExportTraceServiceRequest,
        request_bytes: usize,
        name_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        let mut counts = OtlpCounts::new(self.limits);
        for resource in &request.resource_spans {
            counts.add_resources(1)?;
            counts.add_bytes(resource.schema_url.len())?;
            count_resource(resource.resource.as_ref(), &mut counts)?;
            for scope in &resource.scope_spans {
                counts.add_scopes(1)?;
                counts.add_bytes(scope.schema_url.len())?;
                if let Some(identity) = &scope.scope {
                    counts.add_bytes(identity.name.len())?;
                    counts.add_bytes(identity.version.len())?;
                }
                count_attributes(
                    scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                    &mut counts,
                )?;
                for span in &scope.spans {
                    counts.add_records(1)?;
                    counts.add_bytes(span.name.len())?;
                    counts.add_bytes(span.trace_state.len())?;
                    counts.add_bytes(span.trace_id.len())?;
                    counts.add_bytes(span.span_id.len())?;
                    counts.add_bytes(span.parent_span_id.len())?;
                    if let Some(status) = &span.status {
                        counts.add_bytes(status.message.len())?;
                    }
                    count_attributes(&span.attributes, &mut counts)?;
                    for event in &span.events {
                        counts.add_bytes(event.name.len())?;
                        count_attributes(&event.attributes, &mut counts)?;
                    }
                    for link in &span.links {
                        counts.add_bytes(link.trace_state.len())?;
                        counts.add_bytes(link.trace_id.len())?;
                        counts.add_bytes(link.span_id.len())?;
                        count_attributes(&link.attributes, &mut counts)?;
                    }
                }
            }
        }
        counts.finish(request_bytes, name_bytes)
    }

    /// Counts a typed metrics request without projecting records.
    ///
    /// # Errors
    ///
    /// Returns a stable refusal when a V1 cardinality, depth, byte, or
    /// projected-material ceiling is exceeded.
    pub(crate) fn plan_metrics(
        &self,
        request: &ExportMetricsServiceRequest,
        request_bytes: usize,
        name_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        let mut counts = OtlpCounts::new(self.limits);
        for resource in &request.resource_metrics {
            counts.add_resources(1)?;
            counts.add_bytes(resource.schema_url.len())?;
            count_resource(resource.resource.as_ref(), &mut counts)?;
            for scope in &resource.scope_metrics {
                counts.add_scopes(1)?;
                counts.add_bytes(scope.schema_url.len())?;
                if let Some(identity) = &scope.scope {
                    counts.add_bytes(identity.name.len())?;
                    counts.add_bytes(identity.version.len())?;
                }
                count_attributes(
                    scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                    &mut counts,
                )?;
                for metric in &scope.metrics {
                    counts.add_bytes(metric.name.len())?;
                    counts.add_bytes(metric.description.len())?;
                    counts.add_bytes(metric.unit.len())?;
                    count_attributes(&metric.metadata, &mut counts)?;
                    match metric.data.as_ref() {
                        Some(metric::Data::Gauge(value)) => {
                            for point in &value.data_points {
                                count_number_point(point, &mut counts)?;
                            }
                        }
                        Some(metric::Data::Sum(value)) => {
                            for point in &value.data_points {
                                count_number_point(point, &mut counts)?;
                            }
                        }
                        Some(metric::Data::Histogram(value)) => {
                            for point in &value.data_points {
                                count_histogram_point(point, &mut counts)?;
                            }
                        }
                        Some(metric::Data::ExponentialHistogram(value)) => {
                            for point in &value.data_points {
                                count_exponential_point(point, &mut counts)?;
                            }
                        }
                        Some(metric::Data::Summary(value)) => {
                            for point in &value.data_points {
                                count_summary_point(point, &mut counts)?;
                            }
                        }
                        None => {}
                    }
                }
            }
        }
        counts.finish(request_bytes, name_bytes)
    }

    /// Counts a typed logs request without projecting records.
    ///
    /// # Errors
    ///
    /// Returns a stable refusal when a V1 cardinality, depth, byte, or
    /// projected-material ceiling is exceeded.
    pub(crate) fn plan_logs(
        &self,
        request: &ExportLogsServiceRequest,
        request_bytes: usize,
        name_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        let mut counts = OtlpCounts::new(self.limits);
        for resource in &request.resource_logs {
            counts.add_resources(1)?;
            counts.add_bytes(resource.schema_url.len())?;
            count_resource(resource.resource.as_ref(), &mut counts)?;
            for scope in &resource.scope_logs {
                counts.add_scopes(1)?;
                counts.add_bytes(scope.schema_url.len())?;
                if let Some(identity) = &scope.scope {
                    counts.add_bytes(identity.name.len())?;
                    counts.add_bytes(identity.version.len())?;
                }
                count_attributes(
                    scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                    &mut counts,
                )?;
                for record in &scope.log_records {
                    counts.add_records(1)?;
                    counts.add_bytes(record.severity_text.len())?;
                    counts.add_bytes(record.trace_id.len())?;
                    counts.add_bytes(record.span_id.len())?;
                    counts.add_bytes(record.event_name.len())?;
                    count_attributes(&record.attributes, &mut counts)?;
                    if let Some(body) = &record.body {
                        count_value(body, 1, &mut counts)?;
                    }
                }
            }
        }
        counts.finish(request_bytes, name_bytes)
    }
}

/// Byte-counting sink used to derive one exact IPC output capacity.
#[derive(Debug, Default)]
struct IpcByteCounter {
    /// Checked bytes emitted by Arrow's public stream writer.
    bytes: usize,
}

impl Write for IpcByteCounter {
    /// Counts one writer chunk without retaining it.
    ///
    /// # Errors
    ///
    /// Returns an IO error when the checked byte count overflows.
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len())
            .ok_or_else(|| std::io::Error::other("Arrow IPC output length overflow"))?;
        Ok(buffer.len())
    }

    /// Counting has no buffered IO to flush.
    ///
    /// # Errors
    ///
    /// This implementation does not fail because it retains no IO state.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Counts the exact public Arrow IPC stream bytes for one batch without output.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when Arrow cannot initialize, encode, or
/// finish the count pass.
pub(super) fn count_ipc_bytes(rows: &RecordBatch) -> Result<usize, ScribeError> {
    let mut counter = IpcByteCounter::default();
    let mut writer = StreamWriter::try_new(&mut counter, &rows.schema()).map_err(|error| {
        ScribeError::Internal {
            detail: format!("Arrow IPC count init failed: {error}"),
        }
    })?;
    writer.write(rows).map_err(|error| ScribeError::Internal {
        detail: format!("Arrow IPC count failed: {error}"),
    })?;
    writer.finish().map_err(|error| ScribeError::Internal {
        detail: format!("Arrow IPC count finish failed: {error}"),
    })?;
    Ok(counter.bytes)
}

/// Reads one little-endian stream framing word.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when four bytes are unavailable.
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ScribeError> {
    let end = offset.checked_add(4).ok_or(ScribeError::InvalidFrame)?;
    let raw: [u8; 4] = bytes
        .get(offset..end)
        .and_then(|value| value.try_into().ok())
        .ok_or(ScribeError::InvalidFrame)?;
    Ok(u32::from_le_bytes(raw))
}

/// Rounds one cursor to the public eight-byte IPC alignment.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked overflow.
fn align_eight(value: usize) -> Result<usize, ScribeError> {
    value
        .checked_add(7)
        .map(|next| next & !7)
        .ok_or(ScribeError::InvalidFrame)
}

/// Validates one top-level field against the closed non-nested subset.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for dictionary, extension, nested,
/// view, union, map, list, run-end, unknown, or unsupported field types.
fn native_field_layout(field: arrow::ipc::Field<'_>) -> Result<NativeFieldLayout, ScribeError> {
    if field.dictionary().is_some()
        || field
            .children()
            .is_some_and(|children| !children.is_empty())
        || field
            .custom_metadata()
            .is_some_and(|metadata| !metadata.is_empty())
    {
        return Err(ScribeError::InvalidFrame);
    }
    let fixed = |bits: i32| {
        usize::try_from(bits)
            .ok()
            .filter(|value| *value != 0 && *value % 8 == 0)
            .map(|value| NativeFieldLayout::Fixed(value / 8))
            .ok_or(ScribeError::InvalidFrame)
    };
    match field.type_type() {
        Type::Null => Ok(NativeFieldLayout::Null),
        Type::Bool => Ok(NativeFieldLayout::Boolean),
        Type::Int => match field.type_as_int().map(|value| value.bitWidth()) {
            Some(bits @ (8 | 16 | 32 | 64)) => fixed(bits),
            _ => Err(ScribeError::InvalidFrame),
        },
        Type::FloatingPoint => match field
            .type_as_floating_point()
            .map(|value| value.precision())
        {
            Some(Precision::HALF) => Ok(NativeFieldLayout::Fixed(2)),
            Some(Precision::SINGLE) => Ok(NativeFieldLayout::Fixed(4)),
            Some(Precision::DOUBLE) => Ok(NativeFieldLayout::Fixed(8)),
            _ => Err(ScribeError::InvalidFrame),
        },
        Type::Binary | Type::Utf8 => Ok(NativeFieldLayout::Variable(4)),
        Type::LargeBinary | Type::LargeUtf8 => Ok(NativeFieldLayout::Variable(8)),
        Type::Decimal => {
            let decimal = field.type_as_decimal().ok_or(ScribeError::InvalidFrame)?;
            let maximum_precision = match decimal.bitWidth() {
                128 => 38,
                256 => 76,
                _ => return Err(ScribeError::InvalidFrame),
            };
            let precision = decimal.precision();
            let scale = decimal.scale();
            if precision < 1
                || precision > maximum_precision
                || scale.unsigned_abs() > u32::try_from(precision).unwrap_or(0)
            {
                return Err(ScribeError::InvalidFrame);
            }
            fixed(decimal.bitWidth())
        }
        Type::Date => match field.type_as_date().map(|value| value.unit()) {
            Some(DateUnit::DAY) => Ok(NativeFieldLayout::Fixed(4)),
            Some(DateUnit::MILLISECOND) => Ok(NativeFieldLayout::Fixed(8)),
            _ => Err(ScribeError::InvalidFrame),
        },
        Type::Time => {
            let time = field.type_as_time().ok_or(ScribeError::InvalidFrame)?;
            match (time.unit(), time.bitWidth()) {
                (TimeUnit::SECOND | TimeUnit::MILLISECOND, 32) => fixed(32),
                (TimeUnit::MICROSECOND | TimeUnit::NANOSECOND, 64) => fixed(64),
                _ => Err(ScribeError::InvalidFrame),
            }
        }
        Type::Timestamp | Type::Duration => Ok(NativeFieldLayout::Fixed(8)),
        Type::Interval => match field.type_as_interval().map(|value| value.unit()) {
            Some(IntervalUnit::YEAR_MONTH) => Ok(NativeFieldLayout::Fixed(4)),
            Some(IntervalUnit::DAY_TIME) => Ok(NativeFieldLayout::Fixed(8)),
            Some(IntervalUnit::MONTH_DAY_NANO) => Ok(NativeFieldLayout::Fixed(16)),
            _ => Err(ScribeError::InvalidFrame),
        },
        Type::FixedSizeBinary => field
            .type_as_fixed_size_binary()
            .and_then(|value| usize::try_from(value.byteWidth()).ok())
            .filter(|width| *width != 0)
            .map(NativeFieldLayout::Fixed)
            .ok_or(ScribeError::InvalidFrame),
        _ => Err(ScribeError::InvalidFrame),
    }
}

/// Validates exact scalar buffer cardinality, ranges, and logical lengths.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for a layout mismatch, negative or
/// unaligned range, overlap, overflow, out-of-body descriptor, or invalid
/// variable-width offset sequence.
fn validate_buffer_layouts<'a>(
    layouts: &[NativeFieldLayout],
    nodes: impl ExactSizeIterator<Item = &'a arrow::ipc::FieldNode>,
    buffers: impl ExactSizeIterator<Item = &'a arrow::ipc::Buffer> + Clone,
    body: &[u8],
) -> Result<usize, ScribeError> {
    let mut previous_end = 0_usize;
    let mut buffer_bytes = 0_usize;
    for buffer in buffers.clone() {
        let offset = usize::try_from(buffer.offset()).map_err(|_| ScribeError::InvalidFrame)?;
        let length = usize::try_from(buffer.length()).map_err(|_| ScribeError::InvalidFrame)?;
        if offset % 8 != 0 || offset < previous_end {
            return Err(ScribeError::InvalidFrame);
        }
        let end = offset
            .checked_add(length)
            .ok_or(ScribeError::InvalidFrame)?;
        if end > body.len() {
            return Err(ScribeError::InvalidFrame);
        }
        buffer_bytes = buffer_bytes
            .checked_add(length)
            .ok_or(ScribeError::InvalidFrame)?;
        previous_end = end;
    }
    let expected_buffers = layouts.iter().try_fold(0_usize, |total, layout| {
        total.checked_add(match layout {
            NativeFieldLayout::Null => 0,
            NativeFieldLayout::Boolean | NativeFieldLayout::Fixed(_) => 2,
            NativeFieldLayout::Variable(_) => 3,
        })
    });
    if expected_buffers != Some(buffers.len()) || nodes.len() != layouts.len() {
        return Err(ScribeError::InvalidFrame);
    }
    let mut buffers = buffers;
    for (layout, node) in layouts.iter().copied().zip(nodes) {
        let rows = usize::try_from(node.length()).map_err(|_| ScribeError::InvalidFrame)?;
        if layout == NativeFieldLayout::Null {
            continue;
        }
        let validity = buffers.next().ok_or(ScribeError::InvalidFrame)?;
        let validity_length =
            usize::try_from(validity.length()).map_err(|_| ScribeError::InvalidFrame)?;
        let bitmap_bytes = rows.checked_add(7).ok_or(ScribeError::InvalidFrame)? / 8;
        if validity_length != 0 && validity_length != bitmap_bytes {
            return Err(ScribeError::InvalidFrame);
        }
        if validity_length == 0 {
            if node.null_count() != 0 {
                return Err(ScribeError::InvalidFrame);
            }
        } else {
            validate_validity(body, validity, rows, node.null_count())?;
        }
        let values = buffers.next().ok_or(ScribeError::InvalidFrame)?;
        let values_length =
            usize::try_from(values.length()).map_err(|_| ScribeError::InvalidFrame)?;
        match layout {
            NativeFieldLayout::Null => return Err(ScribeError::InvalidFrame),
            NativeFieldLayout::Boolean => {
                let expected = rows.checked_add(7).ok_or(ScribeError::InvalidFrame)? / 8;
                if values_length != expected {
                    return Err(ScribeError::InvalidFrame);
                }
            }
            NativeFieldLayout::Fixed(width) => {
                if values_length != rows.checked_mul(width).ok_or(ScribeError::InvalidFrame)? {
                    return Err(ScribeError::InvalidFrame);
                }
            }
            NativeFieldLayout::Variable(offset_width) => {
                let expected = rows
                    .checked_add(1)
                    .and_then(|value| value.checked_mul(offset_width))
                    .ok_or(ScribeError::InvalidFrame)?;
                if values_length != expected {
                    return Err(ScribeError::InvalidFrame);
                }
                let data = buffers.next().ok_or(ScribeError::InvalidFrame)?;
                validate_offsets(body, values, data, rows, offset_width)?;
            }
        }
    }
    if buffers.next().is_some() {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(buffer_bytes)
}

/// Confirms the validity bitmap agrees with the record-batch null count.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the bitmap range is unavailable
/// or its set-bit cardinality contradicts the field node.
fn validate_validity(
    body: &[u8],
    validity: &arrow::ipc::Buffer,
    rows: usize,
    null_count: i64,
) -> Result<(), ScribeError> {
    let start = usize::try_from(validity.offset()).map_err(|_| ScribeError::InvalidFrame)?;
    let length = usize::try_from(validity.length()).map_err(|_| ScribeError::InvalidFrame)?;
    let bitmap = body
        .get(start..start.checked_add(length).ok_or(ScribeError::InvalidFrame)?)
        .ok_or(ScribeError::InvalidFrame)?;
    let valid_rows = (0..rows).try_fold(0_usize, |count, index| {
        let byte = bitmap.get(index / 8).ok_or(ScribeError::InvalidFrame)?;
        count
            .checked_add(usize::from(byte & (1 << (index % 8)) != 0))
            .ok_or(ScribeError::InvalidFrame)
    })?;
    let actual_nulls = rows
        .checked_sub(valid_rows)
        .ok_or(ScribeError::InvalidFrame)?;
    if i64::try_from(actual_nulls).map_err(|_| ScribeError::InvalidFrame)? != null_count {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(())
}

/// Validates one variable-width offset buffer against its value range.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for a nonzero first offset, negative
/// or descending offsets, an unsupported width, or a terminal offset that does
/// not equal the declared values length.
fn validate_offsets(
    body: &[u8],
    offsets: &arrow::ipc::Buffer,
    values: &arrow::ipc::Buffer,
    rows: usize,
    width: usize,
) -> Result<(), ScribeError> {
    let offset_start = usize::try_from(offsets.offset()).map_err(|_| ScribeError::InvalidFrame)?;
    let values_length = usize::try_from(values.length()).map_err(|_| ScribeError::InvalidFrame)?;
    let mut prior = 0_usize;
    for index in 0..=rows {
        let start = index
            .checked_mul(width)
            .and_then(|value| offset_start.checked_add(value))
            .ok_or(ScribeError::InvalidFrame)?;
        let end = start.checked_add(width).ok_or(ScribeError::InvalidFrame)?;
        let encoded = body.get(start..end).ok_or(ScribeError::InvalidFrame)?;
        let value = match width {
            4 => usize::try_from(i32::from_le_bytes(
                encoded.try_into().map_err(|_| ScribeError::InvalidFrame)?,
            ))
            .map_err(|_| ScribeError::InvalidFrame)?,
            8 => usize::try_from(i64::from_le_bytes(
                encoded.try_into().map_err(|_| ScribeError::InvalidFrame)?,
            ))
            .map_err(|_| ScribeError::InvalidFrame)?,
            _ => return Err(ScribeError::InvalidFrame),
        };
        if (index == 0 && value != 0) || value < prior || value > values_length {
            return Err(ScribeError::InvalidFrame);
        }
        prior = value;
    }
    if prior != values_length {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(())
}

/// Computes public Arrow buffer capacities for Scribe-managed columns.
///
/// The calculation includes the nullable `run_id`/`card_uid` validity and
/// offsets, three non-null UTF-8 columns, one aliased receipt timestamp buffer,
/// fixed batch identity, and row ordinals. Source-owned buffers remain in the
/// native body fact and are not charged twice.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when checked Arrow capacity
/// arithmetic overflows.
fn managed_projection_bytes(rows: usize, request_id_bytes: usize) -> Result<usize, ScribeError> {
    let overflow = || ScribeError::DecodedPayloadTooLarge {
        bytes: usize::MAX,
        limit: usize::MAX,
    };
    let offsets = rows
        .checked_add(1)
        .and_then(|value| value.checked_mul(size_of::<i32>()))
        .ok_or_else(overflow)?;
    let validity = rows.checked_add(7).ok_or_else(overflow)? / 8;
    let repeated_values = rows
        .checked_mul(
            36_usize
                .checked_add(request_id_bytes)
                .ok_or_else(overflow)?,
        )
        .and_then(|value| value.checked_add(rows.checked_mul(36)?))
        .and_then(|value| value.checked_add(rows.checked_mul(36)?))
        .ok_or_else(overflow)?;
    let fixed_values = rows
        .checked_mul(size_of::<i64>() + 16 + size_of::<i32>())
        .ok_or_else(overflow)?;
    let array_owners = size_of::<arrow::array::StringArray>()
        .checked_mul(5)
        .and_then(|value| {
            value.checked_add(size_of::<arrow::array::TimestampMicrosecondArray>() * 2)
        })
        .and_then(|value| value.checked_add(size_of::<arrow::array::FixedSizeBinaryArray>()))
        .and_then(|value| value.checked_add(size_of::<arrow::array::Int32Array>()))
        .ok_or_else(overflow)?;
    offsets
        .checked_mul(5)
        .and_then(|value| value.checked_add(validity.checked_mul(2)?))
        .and_then(|value| value.checked_add(repeated_values))
        .and_then(|value| value.checked_add(fixed_values))
        .and_then(|value| value.checked_add(array_owners))
        .ok_or_else(overflow)
}

/// Constant-space typed OTLP counters.
#[derive(Debug)]
struct OtlpCounts {
    /// Immutable operator-selected ceilings for this count pass.
    limits: crate::gate::limits::IngestLimits,
    /// Resource groups visited.
    resources: usize,
    /// Scope groups visited.
    scopes: usize,
    /// Signal records visited.
    records: usize,
    /// Attribute nodes visited.
    attributes: usize,
    /// Cumulative borrowed key/value/body bytes.
    value_bytes: usize,
}

impl OtlpCounts {
    /// Starts a constant-space count pass under one frozen limits snapshot.
    #[must_use]
    fn new(limits: crate::gate::limits::IngestLimits) -> Self {
        Self {
            limits,
            resources: 0,
            scopes: 0,
            records: 0,
            attributes: 0,
            value_bytes: 0,
        }
    }

    /// Adds resources under the hard cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on overflow or cap+1.
    fn add_resources(&mut self, count: usize) -> Result<(), ScribeError> {
        self.resources = bounded_add(self.resources, count, self.limits.otlp.resources)?;
        Ok(())
    }

    /// Adds scopes under the hard cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on overflow or cap+1.
    fn add_scopes(&mut self, count: usize) -> Result<(), ScribeError> {
        self.scopes = bounded_add(self.scopes, count, self.limits.otlp.scopes)?;
        Ok(())
    }

    /// Adds signal records under the row cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::TooManyRows`] on overflow or cap+1.
    fn add_records(&mut self, count: usize) -> Result<(), ScribeError> {
        self.records = self
            .records
            .checked_add(count)
            .ok_or(ScribeError::TooManyRows {
                rows: u64::MAX,
                limit: self.limits.otlp.records as u64,
            })?;
        if self.records > self.limits.otlp.records || self.records > self.limits.rows {
            let limit = self.limits.otlp.records.min(self.limits.rows);
            return Err(ScribeError::TooManyRows {
                rows: u64::try_from(self.records).unwrap_or(u64::MAX),
                limit: limit as u64,
            });
        }
        Ok(())
    }

    /// Adds borrowed value bytes under the cumulative cap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::DecodedPayloadTooLarge`] on overflow or cap+1.
    fn add_bytes(&mut self, bytes: usize) -> Result<(), ScribeError> {
        self.value_bytes =
            self.value_bytes
                .checked_add(bytes)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.otlp.value_bytes,
                })?;
        if self.value_bytes > self.limits.otlp.value_bytes {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: self.value_bytes,
                limit: self.limits.otlp.value_bytes,
            });
        }
        Ok(())
    }

    /// Freezes counters into one conservative fixed-capacity material plan.
    ///
    /// # Errors
    ///
    /// Returns a stable material-too-large refusal for checked capacity excess.
    fn finish(
        self,
        request_bytes: usize,
        name_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        if request_bytes > self.limits.otlp.request_bytes {
            return Err(ScribeError::PayloadTooLarge {
                bytes: request_bytes,
            });
        }
        let current_material_bytes = request_bytes
            .checked_add(self.value_bytes)
            .and_then(|value| {
                managed_projection_bytes(self.records, 36)
                    .ok()
                    .and_then(|managed| value.checked_add(managed))
            })
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: usize::MAX,
            })?;
        let mut sources = [SourceMaterialPlan::default(); MAX_SOURCE_PLANS];
        if self.records != 0 {
            sources[0] = SourceMaterialPlan {
                rows: self.records,
                body_bytes: current_material_bytes,
                buffers: 0,
                frame_start: 0,
                body_start: 0,
                body_end: 0,
                frame_end: 0,
            };
        }
        IngestMaterialPlan {
            path: IngestPath::Otlp,
            request_bytes,
            planner_bytes: size_of::<IngestMaterialPlan>(),
            name_bytes,
            aligned_copy_bytes: 0,
            native_schema_start: 0,
            native_schema_end: 0,
            native_schema_material_bytes: 0,
            native_metadata_scratch_bytes: 0,
            sources,
            source_count: usize::from(self.records != 0),
            rows: self.records,
            time_partition_count: usize::from(self.records != 0),
            current_material_bytes,
            active_output_bytes: 0,
            persistence_candidate_bytes: 0,
            durable_metadata_bytes: usize::from(self.records != 0)
                .checked_mul(crate::scribe::shards::DURABLE_SLICE_LAYOUT_BYTES)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?,
            wal_workspace_bytes: self.limits.wal_workspace_bytes,
            persistence_replay_bytes: 0,
            root_bytes: 0,
        }
        .finish()
    }
}

/// Adds one bounded counter.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on overflow or cap+1.
fn bounded_add(current: usize, count: usize, limit: usize) -> Result<usize, ScribeError> {
    let next = current
        .checked_add(count)
        .ok_or(ScribeError::InvalidFrame)?;
    if next > limit {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(next)
}

/// Counts one gauge or sum point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_number_point(point: &NumberDataPoint, counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    count_exemplars(&point.exemplars, counts)
}

/// Counts one explicit histogram point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_histogram_point(
    point: &HistogramDataPoint,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    count_exemplars(&point.exemplars, counts)
}

/// Counts one exponential histogram point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_exponential_point(
    point: &ExponentialHistogramDataPoint,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    count_exemplars(&point.exemplars, counts)
}

/// Counts one summary point's adapter-visible semantic fields.
///
/// # Errors
///
/// Returns a stable row, cardinality, depth, or byte refusal.
fn count_summary_point(
    point: &SummaryDataPoint,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.add_records(1)?;
    count_attributes(&point.attributes, counts)?;
    Ok(())
}

/// Counts raw scalable fields retained by one OTLP resource.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_resource(
    resource: Option<&wyrd_tonic::otlp::resource::v1::Resource>,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    let Some(resource) = resource else {
        return Ok(());
    };
    count_attributes(&resource.attributes, counts)?;
    for entity in &resource.entity_refs {
        counts.add_bytes(entity.schema_url.len())?;
        counts.add_bytes(entity.r#type.len())?;
        for key in &entity.id_keys {
            counts.add_bytes(key.len())?;
        }
        for key in &entity.description_keys {
            counts.add_bytes(key.len())?;
        }
    }
    Ok(())
}

/// Counts exemplar identifiers and filtered attributes exactly once.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_exemplars(values: &[Exemplar], counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    for exemplar in values {
        count_attributes(&exemplar.filtered_attributes, counts)?;
        counts.add_bytes(exemplar.trace_id.len())?;
        counts.add_bytes(exemplar.span_id.len())?;
    }
    Ok(())
}

/// Counts an attribute list and recursively nested values.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or byte refusal.
fn count_attributes(attributes: &[KeyValue], counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    count_attribute_nodes(attributes, 1, counts)
}

/// Counts attributes while preserving their current recursive value depth.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_attribute_nodes(
    attributes: &[KeyValue],
    depth: usize,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    if depth > counts.limits.otlp.value_depth {
        return Err(ScribeError::InvalidFrame);
    }
    counts.attributes = bounded_add(
        counts.attributes,
        attributes.len(),
        counts.limits.otlp.attributes,
    )?;
    for attribute in attributes {
        counts.add_bytes(attribute.key.len())?;
        if let Some(value) = &attribute.value {
            count_value_nodes(value, depth, counts)?;
        }
    }
    Ok(())
}

/// Counts one nested OTLP value without constructing JSON.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] beyond the depth cap and a stable
/// material refusal beyond the cumulative key/value cap.
fn count_value(value: &AnyValue, depth: usize, counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    count_value_nodes(value, depth, counts)
}

/// Traverses recursive OTLP values while charging only retained raw bytes.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] beyond the depth or nested attribute cap.
fn count_value_nodes(
    value: &AnyValue,
    depth: usize,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    if depth > counts.limits.otlp.value_depth {
        return Err(ScribeError::InvalidFrame);
    }
    match value.value.as_ref() {
        Some(any_value::Value::ArrayValue(values)) => {
            for value in &values.values {
                count_value_nodes(value, depth + 1, counts)?;
            }
        }
        Some(any_value::Value::KvlistValue(values)) => {
            count_attribute_nodes(&values.values, depth + 1, counts)?;
        }
        Some(any_value::Value::StringValue(value)) => counts.add_bytes(value.len())?,
        Some(any_value::Value::BytesValue(value)) => counts.add_bytes(value.len())?,
        Some(
            any_value::Value::BoolValue(_)
            | any_value::Value::IntValue(_)
            | any_value::Value::DoubleValue(_),
        )
        | None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use arrow::array::{
        ArrayRef, Decimal128Array, Int64Array, NullArray, StringArray, Time64MicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;
    use wyrd_tonic::otlp::common::v1::{AnyValue, ArrayValue, KeyValue, KeyValueList, any_value};
    use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
    use wyrd_tonic::otlp::metrics::v1::{
        Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, NumberDataPoint,
        SummaryDataPoint, exponential_histogram_data_point, summary_data_point,
    };

    use super::{
        OtlpCounts, ScribeIngressPlanner, count_exponential_point, count_histogram_point,
        count_number_point, count_summary_point, count_value, root_as_message,
    };
    use crate::contracts::ScribeError;

    /// Builds OTLP counters with focused value and node limits.
    fn otlp_counts(value_bytes: usize, attributes: usize) -> OtlpCounts {
        let mut limits = crate::gate::limits::IngestLimits::default();
        limits.otlp.value_bytes = value_bytes;
        limits.otlp.attributes = attributes;
        OtlpCounts::new(limits)
    }

    /// Encodes one canonical V5 stream for scanner tests.
    ///
    /// # Panics
    ///
    /// Panics only when the test fixture cannot construct valid Arrow.
    fn canonical_stream() -> Bytes {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1_i64, 2])),
                Arc::new(StringArray::from(vec!["one", "two"])),
            ],
        )
        .expect("valid scanner batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("stream writer");
        writer.write(&batch).expect("batch write");
        writer.finish().expect("stream finish");
        Bytes::from(bytes)
    }

    /// Encodes one canonical scalar column for metadata mutation tests.
    ///
    /// # Panics
    ///
    /// Panics when the supplied Arrow field and array cannot form or encode a
    /// valid one-row stream fixture.
    fn scalar_stream(field: Field, array: ArrayRef) -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![field]));
        let batch = RecordBatch::try_new(Arc::clone(&schema), vec![array]).expect("scalar batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("stream writer");
        writer.write(&batch).expect("batch write");
        writer.finish().expect("stream finish");
        bytes
    }

    /// Returns the first schema metadata range in one canonical stream.
    ///
    /// # Panics
    ///
    /// Panics when the test fixture lacks the canonical framing prefix.
    fn schema_metadata_bounds(bytes: &[u8]) -> (usize, usize) {
        let prefix = u32::from_le_bytes(bytes[..4].try_into().expect("schema prefix"));
        let start = if prefix == u32::MAX { 8 } else { 4 };
        let len = if prefix == u32::MAX {
            usize::try_from(u32::from_le_bytes(
                bytes[4..8].try_into().expect("schema length"),
            ))
            .expect("metadata length")
        } else {
            usize::try_from(prefix).expect("metadata length")
        };
        (start, start + len)
    }

    /// Encodes a nullable array, then marks its IPC field nonnullable in place.
    ///
    /// The mutation retains otherwise canonical `FlatBuffer` structure while
    /// creating the contradictory field-node null count rejected by preflight.
    ///
    /// # Panics
    ///
    /// Panics only if Arrow cannot encode or expose the canonical test frame.
    fn nonnullable_stream_with_null() -> Bytes {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![Some(1_i64), None]))],
        )
        .expect("nullable fixture");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("stream writer");
        writer.write(&batch).expect("batch write");
        writer.finish().expect("stream finish");
        let (metadata_start, metadata_end) = schema_metadata_bounds(&bytes);
        let nullable_offset = {
            let message = root_as_message(&bytes[metadata_start..metadata_end]).expect("message");
            let field = message
                .header_as_schema()
                .and_then(|value| value.fields())
                .map(|value| value.get(0))
                .expect("schema field");
            field._tab.loc() + usize::from(field._tab.vtable().get(arrow::ipc::Field::VT_NULLABLE))
        };
        bytes[metadata_start + nullable_offset] = 0;
        Bytes::from(bytes)
    }

    /// Canonical native streams are planned before Arrow decode.
    #[test]
    fn native_preflight_accepts_canonical_v1_stream() {
        let bytes = canonical_stream();
        let plan = ScribeIngressPlanner::default()
            .plan_native(&bytes, 32)
            .expect("canonical stream plan");
        assert_eq!(plan.source_count, 1);
        assert_eq!(plan.rows, 2);
        assert!(plan.sources[0].body_end > plan.sources[0].body_start);
        let reader = arrow::ipc::reader::StreamReader::try_new(Cursor::new(bytes), None)
            .expect("schema observation reader");
        let schema = reader.schema();
        assert_eq!(
            plan.native_schema_material_bytes,
            std::mem::size_of::<Schema>() + schema.fields.size()
        );
        assert!(plan.native_metadata_scratch_bytes > 0);
    }

    /// Missing EOS is refused before Arrow decode.
    #[test]
    fn native_preflight_rejects_missing_eos() {
        let bytes = canonical_stream();
        let truncated = bytes.slice(..bytes.len() - 8);
        assert!(matches!(
            ScribeIngressPlanner::default().plan_native(&truncated, 0),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Nonnullable metadata rejects a nonzero node null count before decode.
    #[test]
    fn native_preflight_rejects_nonnullable_nulls() {
        assert!(matches!(
            ScribeIngressPlanner::default().plan_native(&nonnullable_stream_with_null(), 0),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Illegal Time unit/bit-width pairs are rejected from schema metadata.
    #[test]
    fn native_preflight_rejects_illegal_time_width() {
        let mut bytes = scalar_stream(
            Field::new(
                "time",
                DataType::Time64(arrow::datatypes::TimeUnit::Microsecond),
                false,
            ),
            Arc::new(Time64MicrosecondArray::from(vec![1_i64])),
        );
        let (start, end) = schema_metadata_bounds(&bytes);
        let bit_width_offset = {
            let message = root_as_message(&bytes[start..end]).expect("message");
            let field = message
                .header_as_schema()
                .and_then(|value| value.fields())
                .map(|value| value.get(0))
                .expect("schema field");
            let time = field.type_as_time().expect("time metadata");
            time._tab.loc() + usize::from(time._tab.vtable().get(arrow::ipc::Time::VT_BITWIDTH))
        };
        bytes[start + bit_width_offset..start + bit_width_offset + 4]
            .copy_from_slice(&32_i32.to_le_bytes());
        assert!(matches!(
            ScribeIngressPlanner::default().plan_native(&Bytes::from(bytes), 0),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Decimal precision beyond the public width limit is rejected pre-decode.
    #[test]
    fn native_preflight_rejects_decimal_precision_overflow() {
        let array = Decimal128Array::from(vec![Some(123_i128)])
            .with_precision_and_scale(10, 2)
            .expect("decimal fixture");
        let mut bytes = scalar_stream(
            Field::new("amount", DataType::Decimal128(10, 2), true),
            Arc::new(array),
        );
        let (start, end) = schema_metadata_bounds(&bytes);
        let precision_offset = {
            let message = root_as_message(&bytes[start..end]).expect("message");
            let field = message
                .header_as_schema()
                .and_then(|value| value.fields())
                .map(|value| value.get(0))
                .expect("schema field");
            let decimal = field.type_as_decimal().expect("decimal metadata");
            decimal._tab.loc()
                + usize::from(decimal._tab.vtable().get(arrow::ipc::Decimal::VT_PRECISION))
        };
        bytes[start + precision_offset..start + precision_offset + 4]
            .copy_from_slice(&39_i32.to_le_bytes());
        assert!(matches!(
            ScribeIngressPlanner::default().plan_native(&Bytes::from(bytes), 0),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Null arrays require every declared row to be represented as null.
    #[test]
    fn native_preflight_rejects_inconsistent_null_node() {
        let mut bytes = scalar_stream(
            Field::new("empty", DataType::Null, true),
            Arc::new(NullArray::new(1)),
        );
        let original = Bytes::copy_from_slice(&bytes);
        let plan = ScribeIngressPlanner::default()
            .plan_native(&original, 0)
            .expect("canonical null stream");
        let frame_start = plan.sources[0].frame_start;
        let prefix = u32::from_le_bytes(
            bytes[frame_start..frame_start + 4]
                .try_into()
                .expect("batch prefix"),
        );
        let metadata_start = frame_start + if prefix == u32::MAX { 8 } else { 4 };
        let metadata_end = plan.sources[0].body_start;
        let null_count_offset = {
            let metadata = &bytes[metadata_start..metadata_end];
            let message = root_as_message(metadata).expect("batch message");
            let node = message
                .header_as_record_batch()
                .and_then(|value| value.nodes())
                .map(|value| value.get(0))
                .expect("field node");
            (std::ptr::from_ref::<arrow::ipc::FieldNode>(node) as usize)
                .checked_sub(metadata.as_ptr() as usize)
                .expect("node belongs to metadata")
                + 8
        };
        bytes[metadata_start + null_count_offset..metadata_start + null_count_offset + 8]
            .copy_from_slice(&0_i64.to_le_bytes());
        assert!(matches!(
            ScribeIngressPlanner::default().plan_native(&Bytes::from(bytes), 0),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// OTLP nesting beyond the closed depth is refused by the count pass.
    #[test]
    fn otlp_count_pass_rejects_excess_depth() {
        let mut value = AnyValue {
            value: Some(any_value::Value::StringValue("leaf".to_owned())),
        };
        for _ in 0..crate::gate::limits::OTLP_WIRE_LIMITS.value_depth {
            value = AnyValue {
                value: Some(any_value::Value::ArrayValue(ArrayValue {
                    values: vec![value],
                })),
            };
        }
        let request = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                scope_logs: vec![ScopeLogs {
                    log_records: vec![LogRecord {
                        attributes: vec![KeyValue {
                            key: "nested".to_owned(),
                            value: Some(value),
                        }],
                        ..LogRecord::default()
                    }],
                    ..ScopeLogs::default()
                }],
                ..ResourceLogs::default()
            }],
        };
        assert!(matches!(
            ScribeIngressPlanner::default().plan_logs(&request, 1, 0),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Nested `KeyValue` entries consume the same cardinality budget as top-level attributes.
    #[test]
    fn otlp_count_pass_rejects_nested_attribute_cap_plus_one() {
        let value = AnyValue {
            value: Some(any_value::Value::KvlistValue(KeyValueList {
                values: vec![KeyValue::default(); 3],
            })),
        };
        let mut counts = otlp_counts(usize::MAX, 2);
        assert!(matches!(
            count_value(&value, 1, &mut counts),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// `AnyValue` admission charges raw retained string and byte values exactly once.
    #[test]
    fn otlp_count_pass_uses_exact_raw_any_value_bytes() {
        let value = AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: vec![
                    AnyValue {
                        value: Some(any_value::Value::StringValue("quoted \"value\"".to_owned())),
                    },
                    AnyValue {
                        value: Some(any_value::Value::BytesValue(vec![1, 2, 3, 4])),
                    },
                    AnyValue {
                        value: Some(any_value::Value::IntValue(-12)),
                    },
                ],
            })),
        };
        let exact = "quoted \"value\"".len() + 4;
        let mut boundary = otlp_counts(exact, 0);
        count_value(&value, 1, &mut boundary).expect("exact byte boundary");
        assert_eq!(boundary.value_bytes, exact);
        let mut over = otlp_counts(exact - 1, 0);
        assert!(matches!(
            count_value(&value, 1, &mut over),
            Err(ScribeError::DecodedPayloadTooLarge { .. })
        ));
    }

    /// Every metric point shape contributes records while semantic bytes stay adapter-parity exact.
    #[test]
    fn otlp_count_pass_covers_every_metric_shape() {
        let exemplar = Exemplar {
            filtered_attributes: vec![KeyValue {
                key: "filtered".to_owned(),
                value: Some(AnyValue {
                    value: Some(any_value::Value::StringValue("yes".to_owned())),
                }),
            }],
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            ..Exemplar::default()
        };
        let number = NumberDataPoint {
            exemplars: vec![exemplar.clone()],
            ..NumberDataPoint::default()
        };
        let histogram = HistogramDataPoint {
            bucket_counts: vec![1, 2],
            explicit_bounds: vec![0.5],
            ..HistogramDataPoint::default()
        };
        let exponential = ExponentialHistogramDataPoint {
            positive: Some(exponential_histogram_data_point::Buckets {
                offset: -2,
                bucket_counts: vec![3, 4],
            }),
            negative: Some(exponential_histogram_data_point::Buckets {
                offset: 1,
                bucket_counts: vec![5],
            }),
            ..ExponentialHistogramDataPoint::default()
        };
        let summary = SummaryDataPoint {
            quantile_values: vec![summary_data_point::ValueAtQuantile {
                quantile: 0.5,
                value: 9.0,
            }],
            ..SummaryDataPoint::default()
        };
        let mut counts = otlp_counts(usize::MAX, usize::MAX);
        count_number_point(&number, &mut counts).expect("number point");
        count_number_point(&NumberDataPoint::default(), &mut counts).expect("sum point");
        count_histogram_point(&histogram, &mut counts).expect("histogram point");
        count_exponential_point(&exponential, &mut counts).expect("exponential point");
        count_summary_point(&summary, &mut counts).expect("summary point");
        let expected =
            "filtered".len() + "yes".len() + exemplar.trace_id.len() + exemplar.span_id.len();
        assert_eq!(counts.records, 5);
        assert_eq!(counts.attributes, 1);
        assert_eq!(counts.value_bytes, expected);
    }

    /// The canonical fixture remains a valid stream-reader input.
    #[test]
    fn canonical_fixture_is_stream_decodable() {
        let reader =
            arrow::ipc::reader::StreamReader::try_new(Cursor::new(canonical_stream()), None)
                .expect("stream reader");
        assert_eq!(reader.count(), 1);
    }
}
