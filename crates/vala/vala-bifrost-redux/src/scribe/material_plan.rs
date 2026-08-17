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
use wyrd_tonic::otlp::metrics::v1::metric;
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::contracts::ScribeError;

/// Maximum native record-batch descriptors retained by one plan.
pub(crate) const MAX_SOURCE_PLANS: usize = 64;
/// Maximum top-level fields in the canonical native schema.
pub(crate) const MAX_NATIVE_FIELDS: usize = 256;
/// Maximum distinct event days in one request.
pub(crate) const MAX_EVENT_DAYS: usize = crate::gate::limits::OTLP_WIRE_LIMITS.event_days;
/// Maximum projected Arrow and IPC bytes in one request.
pub(crate) const MAX_PROJECTED_BYTES: usize = crate::gate::limits::OTLP_WIRE_LIMITS.material_bytes;

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
    /// Sorted-free fixed set of OTLP event-day ordinals encountered by count pass.
    pub(crate) event_days: [u64; MAX_EVENT_DAYS],
    /// Live prefix length in `event_days`.
    pub(crate) event_day_count: usize,
    /// Largest current-only decoded or projected source.
    pub(crate) current_material_bytes: usize,
    /// Aggregate native rows transferred into active memtable ownership.
    pub(crate) active_output_bytes: usize,
    /// Fixed WAL framing/digest workspace.
    pub(crate) wal_workspace_bytes: usize,
    /// Complete simultaneous-live-set charge.
    pub(crate) root_bytes: usize,
}

impl IngestMaterialPlan {
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
            self.wal_workspace_bytes,
        ]
        .into_iter()
        .try_fold(0_usize, |total, value| total.checked_add(value))
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
                    limit: self.limits.otlp.material_bytes,
                })?;
        }
        if rows > self.limits.rows {
            return Err(ScribeError::TooManyRows {
                rows: u64::try_from(rows).unwrap_or(u64::MAX),
                limit: self.limits.rows as u64,
            });
        }
        if total_bytes > self.limits.otlp.material_bytes {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: total_bytes,
                limit: self.limits.otlp.material_bytes,
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
                    limit: self.limits.otlp.material_bytes,
                })?;
        let encoded_bytes = max_ipc_bytes
            .checked_add(managed_bytes)
            .and_then(|value| value.checked_add(self.limits.wal_workspace_bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.otlp.material_bytes,
            })?;
        let current_material_bytes = decoded_bytes.checked_add(encoded_bytes).ok_or(
            ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.otlp.material_bytes,
            },
        )?;
        if current_material_bytes > self.limits.otlp.material_bytes {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: current_material_bytes,
                limit: self.limits.otlp.material_bytes,
            });
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
            source_count: usize::from(rows != 0),
            rows,
            event_days: [0; MAX_EVENT_DAYS],
            event_day_count: 0,
            current_material_bytes,
            active_output_bytes: 0,
            wal_workspace_bytes: self.limits.wal_workspace_bytes,
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
        let mut cursor = 0_usize;
        let mut schema_seen = false;
        let mut native_schema_start = 0_usize;
        let mut native_schema_end = 0_usize;
        let mut eos_seen = false;
        let mut fields = 0_usize;
        let mut field_layouts = [NativeFieldLayout::Null; MAX_NATIVE_FIELDS];
        let mut field_nullable = [false; MAX_NATIVE_FIELDS];
        let mut source_count = 0_usize;
        let mut rows = 0_usize;
        let mut max_source_rows = 0_usize;
        let mut max_body_bytes = 0_usize;
        let mut max_metadata_bytes = 0_usize;
        let mut schema_material_bytes = size_of::<arrow::datatypes::Schema>();
        let mut active_output_bytes = 0_usize;
        let mut sources = [SourceMaterialPlan::default(); MAX_SOURCE_PLANS];
        while cursor < bytes.len() {
            let frame_start = cursor;
            let prefix = read_u32(bytes, cursor)?;
            cursor = cursor.checked_add(4).ok_or(ScribeError::InvalidFrame)?;
            let metadata_len = if prefix == u32::MAX {
                let length = read_u32(bytes, cursor)?;
                cursor = cursor.checked_add(4).ok_or(ScribeError::InvalidFrame)?;
                length
            } else {
                prefix
            };
            if metadata_len == 0 {
                eos_seen = cursor == bytes.len();
                break;
            }
            let metadata_len =
                usize::try_from(metadata_len).map_err(|_| ScribeError::InvalidFrame)?;
            let metadata_end = cursor
                .checked_add(metadata_len)
                .ok_or(ScribeError::InvalidFrame)?;
            let metadata = bytes
                .get(cursor..metadata_end)
                .ok_or(ScribeError::InvalidFrame)?;
            let message = root_as_message(metadata).map_err(|_| ScribeError::InvalidFrame)?;
            if message.version() != MetadataVersion::V5 {
                return Err(ScribeError::InvalidFrame);
            }
            cursor = align_eight(metadata_end)?;
            let body_len =
                usize::try_from(message.bodyLength()).map_err(|_| ScribeError::InvalidFrame)?;
            let body_end = cursor
                .checked_add(body_len)
                .ok_or(ScribeError::InvalidFrame)?;
            if body_end > bytes.len() {
                return Err(ScribeError::InvalidFrame);
            }
            match message.header_type() {
                MessageHeader::Schema if !schema_seen && source_count == 0 && body_len == 0 => {
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
                    fields = schema_fields.len();
                    if fields == 0 || fields > self.limits.native_fields {
                        return Err(ScribeError::InvalidFrame);
                    }
                    for (index, field) in schema_fields.into_iter().enumerate() {
                        field_layouts[index] = native_field_layout(field)?;
                        field_nullable[index] = field.nullable();
                        schema_material_bytes = schema_material_bytes
                            .checked_add(size_of::<arrow::datatypes::Field>())
                            .and_then(|value| {
                                value.checked_add(
                                    size_of::<std::sync::Arc<arrow::datatypes::Field>>(),
                                )
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
                                limit: self.limits.otlp.material_bytes,
                            })?;
                    }
                    schema_seen = true;
                    native_schema_start = frame_start;
                    native_schema_end = cursor;
                    max_metadata_bytes = max_metadata_bytes.max(cursor - frame_start);
                }
                MessageHeader::RecordBatch
                    if schema_seen && source_count < self.limits.native_sources =>
                {
                    let batch = message
                        .header_as_record_batch()
                        .ok_or(ScribeError::InvalidFrame)?;
                    if batch.compression().is_some() || batch.variadicBufferCounts().is_some() {
                        return Err(ScribeError::InvalidFrame);
                    }
                    let batch_rows =
                        usize::try_from(batch.length()).map_err(|_| ScribeError::InvalidFrame)?;
                    rows = rows
                        .checked_add(batch_rows)
                        .ok_or(ScribeError::TooManyRows {
                            rows: u64::MAX,
                            limit: self.limits.rows as u64,
                        })?;
                    if rows > self.limits.rows {
                        return Err(ScribeError::TooManyRows {
                            rows: u64::try_from(rows).unwrap_or(u64::MAX),
                            limit: self.limits.rows as u64,
                        });
                    }
                    let nodes = batch.nodes().ok_or(ScribeError::InvalidFrame)?;
                    if nodes.len() != fields
                        || nodes.into_iter().enumerate().any(|(index, node)| {
                            node.length() != batch.length()
                                || node.null_count() < 0
                                || node.null_count() > node.length()
                                || (!field_nullable[index] && node.null_count() != 0)
                                || (field_layouts[index] == NativeFieldLayout::Null
                                    && node.null_count() != node.length())
                        })
                    {
                        return Err(ScribeError::InvalidFrame);
                    }
                    let buffers = batch.buffers().ok_or(ScribeError::InvalidFrame)?;
                    let body = bytes
                        .get(cursor..body_end)
                        .ok_or(ScribeError::InvalidFrame)?;
                    let buffer_bytes = validate_buffer_layouts(
                        &field_layouts[..fields],
                        nodes.into_iter(),
                        buffers.into_iter(),
                        body,
                    )?;
                    active_output_bytes = active_output_bytes
                        .checked_add(buffer_bytes)
                        .and_then(|value| {
                            managed_projection_bytes(batch_rows, 36)
                                .ok()
                                .and_then(|managed| value.checked_add(managed))
                        })
                        .ok_or(ScribeError::DecodedPayloadTooLarge {
                            bytes: usize::MAX,
                            limit: self.limits.otlp.material_bytes,
                        })?;
                    sources[source_count] = SourceMaterialPlan {
                        rows: batch_rows,
                        body_bytes: buffer_bytes,
                        buffers: buffers.len(),
                        frame_start,
                        body_start: cursor,
                        body_end,
                        frame_end: align_eight(body_end)?,
                    };
                    source_count += 1;
                    max_source_rows = max_source_rows.max(batch_rows);
                    max_body_bytes = max_body_bytes.max(body_len);
                    max_metadata_bytes = max_metadata_bytes.max(cursor - frame_start);
                }
                _ => return Err(ScribeError::InvalidFrame),
            }
            cursor = align_eight(body_end)?;
        }
        if !schema_seen || source_count == 0 || rows == 0 || !eos_seen {
            return Err(ScribeError::InvalidFrame);
        }
        let managed_bytes = managed_projection_bytes(max_source_rows, 36)?;
        let decoded_bytes = schema_material_bytes
            .checked_add(max_metadata_bytes)
            .and_then(|value| value.checked_add(managed_bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.otlp.material_bytes,
            })?;
        let encoded_bytes = max_body_bytes
            .checked_add(managed_bytes)
            .and_then(|value| value.checked_add(self.limits.wal_workspace_bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.otlp.material_bytes,
            })?;
        let current_material_bytes = decoded_bytes.checked_add(encoded_bytes).ok_or(
            ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.otlp.material_bytes,
            },
        )?;
        if current_material_bytes > self.limits.otlp.material_bytes {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: current_material_bytes,
                limit: self.limits.otlp.material_bytes,
            });
        }
        IngestMaterialPlan {
            path: IngestPath::Native,
            request_bytes: bytes.len(),
            planner_bytes: size_of::<IngestMaterialPlan>(),
            name_bytes,
            aligned_copy_bytes: sources[..source_count]
                .iter()
                .filter(|source| {
                    (bytes.as_ptr() as usize)
                        .checked_add(source.body_start)
                        .is_none_or(|address| address & 7 != 0)
                })
                .map(|source| source.body_end - source.body_start)
                .max()
                .unwrap_or(0),
            native_schema_start,
            native_schema_end,
            native_schema_material_bytes: schema_material_bytes,
            native_metadata_scratch_bytes: max_metadata_bytes,
            sources,
            source_count,
            rows,
            event_days: [0; MAX_EVENT_DAYS],
            event_day_count: 0,
            current_material_bytes,
            active_output_bytes,
            wal_workspace_bytes: self.limits.wal_workspace_bytes,
            root_bytes: 0,
        }
        .finish()
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
            count_attributes(
                resource
                    .resource
                    .as_ref()
                    .map_or(&[], |value| &value.attributes),
                &mut counts,
            )?;
            for scope in &resource.scope_spans {
                counts.add_scopes(1)?;
                count_attributes(
                    scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                    &mut counts,
                )?;
                for span in &scope.spans {
                    counts.add_records(1)?;
                    counts.add_event_time(span.start_time_unix_nano)?;
                    counts.add_bytes(span.name.len().saturating_add(span.trace_state.len()))?;
                    count_attributes(&span.attributes, &mut counts)?;
                    for event in &span.events {
                        counts.add_bytes(event.name.len())?;
                        count_attributes(&event.attributes, &mut counts)?;
                    }
                    for link in &span.links {
                        counts.add_bytes(link.trace_state.len())?;
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
            count_attributes(
                resource
                    .resource
                    .as_ref()
                    .map_or(&[], |value| &value.attributes),
                &mut counts,
            )?;
            for scope in &resource.scope_metrics {
                counts.add_scopes(1)?;
                count_attributes(
                    scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                    &mut counts,
                )?;
                for metric in &scope.metrics {
                    counts.add_bytes(
                        metric
                            .name
                            .len()
                            .saturating_add(metric.description.len())
                            .saturating_add(metric.unit.len()),
                    )?;
                    match metric.data.as_ref() {
                        Some(metric::Data::Gauge(value)) => count_points(
                            value.data_points.iter().map(|point| &point.attributes),
                            &mut counts,
                        )?,
                        Some(metric::Data::Sum(value)) => count_points(
                            value.data_points.iter().map(|point| &point.attributes),
                            &mut counts,
                        )?,
                        Some(metric::Data::Histogram(value)) => count_points(
                            value.data_points.iter().map(|point| &point.attributes),
                            &mut counts,
                        )?,
                        Some(metric::Data::ExponentialHistogram(value)) => count_points(
                            value.data_points.iter().map(|point| &point.attributes),
                            &mut counts,
                        )?,
                        Some(metric::Data::Summary(value)) => count_points(
                            value.data_points.iter().map(|point| &point.attributes),
                            &mut counts,
                        )?,
                        None => {}
                    }
                    count_metric_days(metric, &mut counts)?;
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
            count_attributes(
                resource
                    .resource
                    .as_ref()
                    .map_or(&[], |value| &value.attributes),
                &mut counts,
            )?;
            for scope in &resource.scope_logs {
                counts.add_scopes(1)?;
                count_attributes(
                    scope.scope.as_ref().map_or(&[], |value| &value.attributes),
                    &mut counts,
                )?;
                for record in &scope.log_records {
                    counts.add_records(1)?;
                    counts.add_event_time(record.time_unix_nano)?;
                    counts.add_bytes(record.severity_text.len())?;
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
        limit: MAX_PROJECTED_BYTES,
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
    /// Fixed set of nonzero Unix-day ordinals.
    event_days: [u64; MAX_EVENT_DAYS],
    /// Live prefix length in `event_days`.
    event_day_count: usize,
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
            event_days: [0; MAX_EVENT_DAYS],
            event_day_count: 0,
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

    /// Adds one nonzero Unix timestamp to the fixed distinct-day set.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when a request spans more than the
    /// closed 32-day limit.
    fn add_event_time(&mut self, unix_nanos: u64) -> Result<(), ScribeError> {
        if unix_nanos == 0 {
            return Ok(());
        }
        let day = unix_nanos / 86_400_000_000_000;
        if self.event_days[..self.event_day_count].contains(&day) {
            return Ok(());
        }
        if self.event_day_count == self.limits.otlp.event_days {
            return Err(ScribeError::InvalidFrame);
        }
        self.event_days[self.event_day_count] = day;
        self.event_day_count += 1;
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
        let projected_floor = self
            .records
            .checked_mul(256)
            .and_then(|value| value.checked_add(self.value_bytes))
            .and_then(|value| value.checked_add(self.attributes.checked_mul(16)?))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.otlp.material_bytes,
            })?;
        if projected_floor > self.limits.otlp.material_bytes {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: projected_floor,
                limit: self.limits.otlp.material_bytes,
            });
        }
        let mut sources = [SourceMaterialPlan::default(); MAX_SOURCE_PLANS];
        if self.records != 0 {
            sources[0] = SourceMaterialPlan {
                rows: self.records,
                body_bytes: projected_floor,
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
            event_days: self.event_days,
            event_day_count: self.event_day_count,
            current_material_bytes: projected_floor,
            active_output_bytes: 0,
            wal_workspace_bytes: self.limits.wal_workspace_bytes,
            root_bytes: 0,
        }
        .finish()
    }
}

/// Counts event days across the closed metric point variants.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the distinct-day cap is exceeded.
fn count_metric_days(
    metric: &wyrd_tonic::otlp::metrics::v1::Metric,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    match metric.data.as_ref() {
        Some(metric::Data::Gauge(value)) => {
            for point in &value.data_points {
                counts.add_event_time(point.time_unix_nano)?;
            }
        }
        Some(metric::Data::Sum(value)) => {
            for point in &value.data_points {
                counts.add_event_time(point.time_unix_nano)?;
            }
        }
        Some(metric::Data::Histogram(value)) => {
            for point in &value.data_points {
                counts.add_event_time(point.time_unix_nano)?;
            }
        }
        Some(metric::Data::ExponentialHistogram(value)) => {
            for point in &value.data_points {
                counts.add_event_time(point.time_unix_nano)?;
            }
        }
        Some(metric::Data::Summary(value)) => {
            for point in &value.data_points {
                counts.add_event_time(point.time_unix_nano)?;
            }
        }
        None => {}
    }
    Ok(())
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

/// Counts point attributes for any metric data-point iterator.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or byte refusal.
fn count_points<'a>(
    points: impl Iterator<Item = &'a Vec<KeyValue>>,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    for attributes in points {
        counts.add_records(1)?;
        count_attributes(attributes, counts)?;
    }
    Ok(())
}

/// Counts an attribute list and recursively nested values.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or byte refusal.
fn count_attributes(attributes: &[KeyValue], counts: &mut OtlpCounts) -> Result<(), ScribeError> {
    count_attributes_at_depth(attributes, 1, counts)
}

/// Counts attributes while preserving their current recursive value depth.
///
/// # Errors
///
/// Returns a stable cardinality, depth, or cumulative-byte refusal.
fn count_attributes_at_depth(
    attributes: &[KeyValue],
    depth: usize,
    counts: &mut OtlpCounts,
) -> Result<(), ScribeError> {
    counts.attributes = bounded_add(
        counts.attributes,
        attributes.len(),
        counts.limits.otlp.attributes,
    )?;
    for attribute in attributes {
        counts.add_bytes(attribute.key.len())?;
        if let Some(value) = &attribute.value {
            count_value(value, depth, counts)?;
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
    if depth > counts.limits.otlp.value_depth {
        return Err(ScribeError::InvalidFrame);
    }
    match value.value.as_ref() {
        Some(any_value::Value::StringValue(value)) => counts.add_bytes(value.len())?,
        Some(any_value::Value::BytesValue(value)) => counts.add_bytes(value.len())?,
        Some(any_value::Value::ArrayValue(values)) => {
            for value in &values.values {
                count_value(value, depth + 1, counts)?;
            }
        }
        Some(any_value::Value::KvlistValue(values)) => {
            count_attributes_at_depth(&values.values, depth + 1, counts)?;
        }
        Some(any_value::Value::BoolValue(_))
        | Some(any_value::Value::IntValue(_))
        | Some(any_value::Value::DoubleValue(_))
        | None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;
    use wyrd_tonic::otlp::common::v1::{AnyValue, ArrayValue, KeyValue, any_value};
    use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;

    use super::ScribeIngressPlanner;
    use crate::contracts::ScribeError;

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

    /// The canonical fixture remains a valid stream-reader input.
    #[test]
    fn canonical_fixture_is_stream_decodable() {
        let reader =
            arrow::ipc::reader::StreamReader::try_new(Cursor::new(canonical_stream()), None)
                .expect("stream reader");
        assert_eq!(reader.count(), 1);
    }
}
