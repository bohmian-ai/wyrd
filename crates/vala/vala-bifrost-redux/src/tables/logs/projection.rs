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
use std::borrow::Cow;
use std::sync::Arc;
use wyrd_queue::variant::EncodedVariant;
use wyrd_tonic::otlp::common::v1::EntityRef;
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs};

use super::records::LOG_FIELDS;
use crate::otlp_contract::LogsOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    ExceptionPromotions, OutputBudget, RecordCorrelation, ResourceEnvelope, ScopeEnvelope,
    any_value_variant, attributes_variant, bool_column, checked_i64, entity_refs_column,
    fixed_binary_opt_column, fixed_row_bytes, i32_column, i64_column, projected_signal_schema,
    span_id_bytes, trace_id_bytes, u32_as_i64_column, utf8_column, utf8_opt_column, variant_column,
};
use wyrd_spec::reference::CardRefScope;

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
/// encodes unchanged. A body or attribute set that cannot be stored as Variant
/// rejects its record with a reason that starts with the catalogued Variant
/// error code and names the record's zero-based traversal ordinal; nothing is
/// truncated.
///
/// # Errors
///
/// Returns [`TableError::OutputTooLarge`] as soon as the running Arrow output
/// of accepted records crosses `output_limit_bytes`, before the batch is
/// built. Returns [`TableError::Internal`] only when the accepted rows cannot
/// be assembled into the canonical Arrow batch.
pub fn project_resource_logs(
    resource_logs: &[ResourceLogs],
    card_scope: Option<&CardRefScope>,
    output_limit_bytes: usize,
) -> Result<(RecordBatch, LogsOutcome), TableError> {
    let mut columns = LogColumns::default();
    let mut budget = OutputBudget::new(output_limit_bytes);
    let row_bytes = log_row_output_bytes();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;

    let mut ordinal: u64 = 0;

    for resource in resource_logs {
        let envelope = ResourceEnvelope::project(resource.resource.as_ref(), &resource.schema_url);
        for scope in &resource.scope_logs {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_ref(), &scope.schema_url);
            for record in &scope.log_records {
                let row = ordinal;
                ordinal = ordinal.saturating_add(1);
                let outcome = match (&envelope, &scope_envelope) {
                    (Err(failure), _) | (Ok(_), Err(failure)) => Err(failure.reason(row)),
                    (Ok(envelope), Ok(scope_envelope)) => columns
                        .push(record, envelope, scope_envelope, card_scope, row)
                        .map(|bytes| {
                            bytes + envelope.repeated_bytes() + scope_envelope.repeated_bytes()
                        }),
                };
                match outcome {
                    Ok(payload_bytes) => budget.charge(row_bytes + payload_bytes)?,
                    Err(reason) => {
                        rejected = rejected.saturating_add(1);
                        rejection_message.get_or_insert_with(|| reason.into_owned());
                    }
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

/// Returns the fixed Arrow bytes one projected log row retains.
///
/// The log projector charges this with each accepted row, and the server's
/// OTLP preflight charges it per wire record.
#[must_use]
pub fn log_row_output_bytes() -> usize {
    fixed_row_bytes(projected_signal_schema(LOG_FIELDS).fields())
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
    body: Vec<Option<EncodedVariant>>,
    trace_id: Vec<Option<Vec<u8>>>,
    span_id: Vec<Option<Vec<u8>>>,
    flags: Vec<u32>,
    attributes: Vec<EncodedVariant>,
    dropped_attributes_count: Vec<u32>,
    resource_present: Vec<bool>,
    resource_attributes: Vec<EncodedVariant>,
    resource_dropped_attributes_count: Vec<u32>,
    resource_schema_url: Vec<String>,
    entity_ref_lengths: Vec<Option<usize>>,
    entity_refs: Vec<EntityRef>,
    scope_present: Vec<bool>,
    scope_name: Vec<String>,
    scope_version: Vec<String>,
    scope_attributes: Vec<EncodedVariant>,
    scope_dropped_attributes_count: Vec<u32>,
    scope_schema_url: Vec<String>,
    service_name: Vec<Option<String>>,
    service_version: Vec<Option<String>>,
    deployment_environment: Vec<Option<String>>,
    exception_type: Vec<Option<String>>,
    exception_message: Vec<Option<String>>,
    exception_stacktrace: Vec<Option<String>>,
    body_text: Vec<Option<String>>,
    card_ref: Vec<Option<String>>,
    run_id: Vec<Option<String>>,
}

impl LogColumns {
    /// Validate one log record completely, then append its canonical row.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an invalid correlation
    /// identifier, a malformed or wrong-typed `wyrd.card_ref` / `wyrd.run_id`
    /// record attribute, an over-long severity text or event name, a body or
    /// attribute payload beyond the configured material limits, or a body or
    /// attribute set that cannot be stored as Variant. A Variant rejection
    /// reason starts with its catalogued code and names `row`, the record's
    /// zero-based traversal ordinal. Nothing is appended when an error is
    /// returned.
    ///
    /// On success returns the variable-length payload bytes the record itself
    /// appended; the caller adds fixed widths and the repeated resource and
    /// scope bytes.
    fn push(
        &mut self,
        record: &LogRecord,
        resource: &ResourceEnvelope,
        scope: &ScopeEnvelope,
        card_scope: Option<&CardRefScope>,
        row: u64,
    ) -> Result<usize, Cow<'static, str>> {
        if record.severity_text.len() > MAX_SEVERITY_TEXT_BYTES {
            return Err("log severity_text exceeds the accepted length".into());
        }
        if record.event_name.len() > MAX_EVENT_NAME_BYTES {
            return Err("log event_name exceeds the accepted length".into());
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
        let body = record
            .body
            .as_ref()
            .map(|value| any_value_variant("body", value))
            .transpose()
            .map_err(|failure| failure.reason(row))?;
        if body.as_ref().is_some_and(|encoded| {
            encoded.encoded_bytes() > wyrd_spec::vala::logs::record::MAX_LOG_BODY_BYTES
        }) {
            return Err("log body exceeds the accepted payload size".into());
        }
        let attributes = attributes_variant("attributes", &record.attributes)
            .map_err(|failure| failure.reason(row))?;
        if attributes.encoded_bytes() > wyrd_spec::vala::logs::record::MAX_LOG_ATTRIBUTES_BYTES {
            return Err("log attributes exceed the accepted payload size".into());
        }
        let correlation = RecordCorrelation::extract(&record.attributes, card_scope)?;
        let exception = ExceptionPromotions::from_attributes(&record.attributes);
        let body_text = match record.body.as_ref().and_then(|value| value.value.as_ref()) {
            Some(Value::StringValue(text)) => Some(text.clone()),
            _ => None,
        };
        let payload_bytes = record.severity_text.len()
            + record.event_name.len()
            + body.as_ref().map_or(0, EncodedVariant::encoded_bytes)
            + attributes.encoded_bytes()
            + exception.bytes()
            + body_text.as_ref().map_or(0, String::len)
            + correlation.card_ref.as_ref().map_or(0, String::len)
            + correlation.run_id.as_ref().map_or(0, String::len);

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
        self.entity_refs.extend_from_slice(&resource.entity_refs);

        self.scope_present.push(scope.present);
        self.scope_name.push(scope.name.clone());
        self.scope_version.push(scope.version.clone());
        self.scope_attributes.push(scope.attributes.clone());
        self.scope_dropped_attributes_count
            .push(scope.dropped_attributes_count);
        self.scope_schema_url.push(scope.schema_url.clone());

        self.service_name.push(resource.service_name.clone());
        self.service_version.push(resource.service_version.clone());
        self.deployment_environment
            .push(resource.deployment_environment.clone());
        self.exception_type.push(exception.exception_type);
        self.exception_message.push(exception.message);
        self.exception_stacktrace.push(exception.stacktrace);
        self.body_text.push(body_text);

        self.card_ref.push(correlation.card_ref);
        self.run_id.push(correlation.run_id);

        self.rows += 1;
        Ok(payload_bytes)
    }

    /// Assemble the accepted rows into the projected log batch.
    ///
    /// The batch carries the canonical ledger columns plus the two nullable
    /// `card_ref` and `run_id` correlation columns Scribe consumes.
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
            variant_column(self.body.iter().map(Option::as_ref)),
            fixed_binary_opt_column(16, &self.trace_id).map_err(internal)?,
            fixed_binary_opt_column(8, &self.span_id).map_err(internal)?,
            u32_as_i64_column(self.flags),
            variant_column(self.attributes.iter().map(Some)),
            u32_as_i64_column(self.dropped_attributes_count),
            bool_column(self.resource_present),
            variant_column(self.resource_attributes.iter().map(Some)),
            u32_as_i64_column(self.resource_dropped_attributes_count),
            utf8_column(self.resource_schema_url),
            entity_refs_column(&self.entity_refs, &self.entity_ref_lengths)?,
            bool_column(self.scope_present),
            utf8_column(self.scope_name),
            utf8_column(self.scope_version),
            variant_column(self.scope_attributes.iter().map(Some)),
            u32_as_i64_column(self.scope_dropped_attributes_count),
            utf8_column(self.scope_schema_url),
            utf8_opt_column(self.service_name),
            utf8_opt_column(self.service_version),
            utf8_opt_column(self.deployment_environment),
            utf8_opt_column(self.exception_type),
            utf8_opt_column(self.exception_message),
            utf8_opt_column(self.exception_stacktrace),
            utf8_opt_column(self.body_text),
            utf8_opt_column(self.card_ref),
            utf8_opt_column(self.run_id),
        ];
        RecordBatch::try_new(projected_signal_schema(LOG_FIELDS), columns)
            .map_err(|error| TableError::Internal(format!("canonical log batch: {error}")))
    }
}

/// Wrap a stable assembly reason as an internal projection defect.
fn internal(reason: &'static str) -> TableError {
    TableError::Internal(reason.to_owned())
}
