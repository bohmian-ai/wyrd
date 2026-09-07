//! Table-owned `OTLP` log projection for `vala.logs.records`.
//!
//! [`project_resource_logs`] is the single deterministic, IO-free conversion
//! from decoded `OTLP` resource logs into the canonical log row. A record
//! supplied through the Logs signal stays a log record: this module never
//! translates one into a span event, never derives a `GenAI` path, and never
//! copies a payload into another table.
//!
//! Column order and field meaning come exclusively from
//! [`super::records::LOG_FIELDS`].

use arrow::array::ArrayRef;
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs};

use super::records::{LOG_ENTITY_REF_ELEMENT, LOG_FIELDS};
use crate::otlp_contract::LogsOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    ResourceEnvelope, ScopeEnvelope, binary_column, binary_opt_column, bool_column, checked_i64,
    encode_any_value, encode_attributes, fixed_binary_opt_column, i32_column, i64_column,
    list_column, span_id_bytes, trace_id_bytes, u32_as_i64_column, utf8_column, utf8_opt_column,
};

/// Largest accepted severity text, in bytes.
const MAX_SEVERITY_TEXT_BYTES: usize = 64;
/// Largest accepted event name, in bytes.
const MAX_EVENT_NAME_BYTES: usize = 256;

/// Project decoded `OTLP` resource logs into one canonical log batch.
///
/// Each record is atomic: it is accepted with every supported field projected
/// losslessly or rejected whole. Accepted records retain their relative request
/// order and the returned [`LogsOutcome`] carries the exact counts plus the
/// first rejection reason, which the existing `OTLP` partial-success response
/// encodes unchanged.
///
/// # Errors
///
/// Returns [`TableError::Internal`] only when the accepted rows cannot be
/// assembled into the canonical Arrow batch.
pub fn project_resource_logs(
    resource_logs: &[ResourceLogs],
) -> Result<(RecordBatch, LogsOutcome), TableError> {
    let mut columns = LogColumns::default();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;

    for resource in resource_logs {
        let envelope = ResourceEnvelope::project(resource.resource.as_ref(), &resource.schema_url);
        for scope in &resource.scope_logs {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_ref(), &scope.schema_url);
            for record in &scope.log_records {
                if let Err(reason) = columns.push(record, &envelope, &scope_envelope) {
                    rejected = rejected.saturating_add(1);
                    rejection_message.get_or_insert_with(|| reason.to_owned());
                }
            }
        }
    }

    let accepted = i64::try_from(columns.rows)
        .map_err(|_| TableError::Internal("accepted log count exceeds i64".to_owned()))?;
    let batch = columns.finish()?;
    Ok((
        batch,
        LogsOutcome {
            accepted_records: accepted,
            rejected_records: rejected,
            rejection_message,
        },
    ))
}

/// Return the canonical user-column schema of `vala.logs.records`.
#[must_use]
pub fn canonical_log_schema() -> Arc<Schema> {
    Arc::new(Schema::new(canonical_arrow_fields(LOG_FIELDS)))
}

/// Column-wise accumulator for the accepted canonical log rows.
#[derive(Debug, Default)]
struct LogColumns {
    rows: usize,
    time_unix_nano: Vec<i64>,
    observed_time_unix_nano: Vec<i64>,
    severity_number: Vec<i32>,
    severity_text: Vec<String>,
    event_name: Vec<Option<String>>,
    body: Vec<Option<Vec<u8>>>,
    trace_id: Vec<Option<Vec<u8>>>,
    span_id: Vec<Option<Vec<u8>>>,
    flags: Vec<u32>,
    attributes: Vec<Vec<u8>>,
    dropped_attributes_count: Vec<u32>,
    resource_present: Vec<bool>,
    resource_attributes: Vec<Vec<u8>>,
    resource_dropped_attributes_count: Vec<u32>,
    resource_schema_url: Vec<String>,
    entity_ref_lengths: Vec<Option<usize>>,
    entity_refs: Vec<Vec<u8>>,
    scope_present: Vec<bool>,
    scope_name: Vec<String>,
    scope_version: Vec<String>,
    scope_attributes: Vec<Vec<u8>>,
    scope_dropped_attributes_count: Vec<u32>,
    scope_schema_url: Vec<String>,
}

impl LogColumns {
    /// Validate one log record completely, then append its canonical row.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an invalid correlation
    /// identifier, an over-long severity text or event name, or a body or
    /// attribute payload beyond the configured material limits. Nothing is
    /// appended when an error is returned.
    fn push(
        &mut self,
        record: &LogRecord,
        resource: &ResourceEnvelope,
        scope: &ScopeEnvelope,
    ) -> Result<(), &'static str> {
        if record.severity_text.len() > MAX_SEVERITY_TEXT_BYTES {
            return Err("log severity_text exceeds the accepted length");
        }
        if record.event_name.len() > MAX_EVENT_NAME_BYTES {
            return Err("log event_name exceeds the accepted length");
        }
        let trace_id = if record.trace_id.is_empty() {
            None
        } else {
            Some(trace_id_bytes(&record.trace_id)?.to_vec())
        };
        let span_id = if record.span_id.is_empty() {
            None
        } else {
            Some(span_id_bytes(&record.span_id)?.to_vec())
        };
        let time_unix_nano = checked_i64(record.time_unix_nano)?;
        let observed_time_unix_nano = checked_i64(record.observed_time_unix_nano)?;
        let body = record.body.as_ref().map(encode_any_value);
        if body
            .as_ref()
            .is_some_and(|bytes| bytes.len() > wyrd_spec::vala::logs::record::MAX_LOG_BODY_BYTES)
        {
            return Err("log body exceeds the accepted payload size");
        }
        let attributes = encode_attributes(&record.attributes);
        if attributes.len() > wyrd_spec::vala::logs::record::MAX_LOG_ATTRIBUTES_BYTES {
            return Err("log attributes exceed the accepted payload size");
        }

        self.time_unix_nano.push(time_unix_nano);
        self.observed_time_unix_nano.push(observed_time_unix_nano);
        self.severity_number.push(record.severity_number);
        self.severity_text.push(record.severity_text.clone());
        self.event_name
            .push((!record.event_name.is_empty()).then(|| record.event_name.clone()));
        self.body.push(body);
        self.trace_id.push(trace_id);
        self.span_id.push(span_id);
        self.flags.push(record.flags);
        self.attributes.push(attributes);
        self.dropped_attributes_count
            .push(record.dropped_attributes_count);

        self.resource_present.push(resource.present);
        self.resource_attributes.push(resource.attributes.clone());
        self.resource_dropped_attributes_count
            .push(resource.dropped_attributes_count);
        self.resource_schema_url.push(resource.schema_url.clone());
        self.entity_ref_lengths
            .push(Some(resource.entity_refs.len()));
        self.entity_refs
            .extend(resource.entity_refs.iter().cloned());

        self.scope_present.push(scope.present);
        self.scope_name.push(scope.name.clone());
        self.scope_version.push(scope.version.clone());
        self.scope_attributes.push(scope.attributes.clone());
        self.scope_dropped_attributes_count
            .push(scope.dropped_attributes_count);
        self.scope_schema_url.push(scope.schema_url.clone());

        self.rows += 1;
        Ok(())
    }

    /// Assemble the accepted rows into the canonical log batch.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when a column cannot be built or the
    /// assembled columns do not match the canonical schema.
    fn finish(self) -> Result<RecordBatch, TableError> {
        let columns: Vec<ArrayRef> = vec![
            i64_column(self.time_unix_nano),
            i64_column(self.observed_time_unix_nano),
            i32_column(self.severity_number),
            utf8_column(self.severity_text),
            utf8_opt_column(self.event_name),
            binary_opt_column(&self.body),
            fixed_binary_opt_column(16, &self.trace_id).map_err(internal)?,
            fixed_binary_opt_column(8, &self.span_id).map_err(internal)?,
            u32_as_i64_column(self.flags),
            binary_column(&self.attributes),
            u32_as_i64_column(self.dropped_attributes_count),
            bool_column(self.resource_present),
            binary_column(&self.resource_attributes),
            u32_as_i64_column(self.resource_dropped_attributes_count),
            utf8_column(self.resource_schema_url),
            list_column(
                &LOG_ENTITY_REF_ELEMENT.to_arrow(),
                binary_column(&self.entity_refs),
                &self.entity_ref_lengths,
            )
            .map_err(internal)?,
            bool_column(self.scope_present),
            utf8_column(self.scope_name),
            utf8_column(self.scope_version),
            binary_column(&self.scope_attributes),
            u32_as_i64_column(self.scope_dropped_attributes_count),
            utf8_column(self.scope_schema_url),
        ];
        RecordBatch::try_new(canonical_log_schema(), columns)
            .map_err(|error| TableError::Internal(format!("canonical log batch: {error}")))
    }
}

/// Wrap a stable assembly reason as an internal projection defect.
fn internal(reason: &'static str) -> TableError {
    TableError::Internal(reason.to_owned())
}
