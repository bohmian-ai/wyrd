//! Table-owned `OTLP` log projection for `bifrost.logs.records`.
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
use std::sync::{Arc, LazyLock};
use wyrd_queue::variant::VariantColumnEncoder;
use wyrd_spec::vala::logs::record::{MAX_LOG_ATTRIBUTES_BYTES, MAX_LOG_BODY_BYTES};
use wyrd_tonic::otlp::logs::v1::ResourceLogs;

use super::input::{LogInput, RecordInput};
use super::records::LOG_FIELDS;
use crate::otlp_contract::LogsOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    Attributes, Context, ExceptionPromotions, OutputBudget, RecordCorrelation, ResourceEnvelope,
    ResourcePromotionColumns, RowEnvelopes, ScopeEnvelope, ValueRef, append_attributes,
    append_value, bool_column, checked_i64, fixed_binary_opt_column, fixed_row_bytes, i32_column,
    otlp_instant_column, projected_signal_schema, refuse_forged, span_id_bytes, stored_attributes,
    stored_text, trace_id_bytes, u32_as_i64_column, utf8_column, utf8_opt_column,
};
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::BifrostError;

/// Refuse a native canonical log batch whose derived columns differ from
/// what [`project_resource_logs`] derives from the same source fields, or
/// whose body is not stored the way the projection stores it.
///
/// The exception promotions and `is_exception` are re-derived from each row's
/// `attributes` with the projection's own rules, so a native writer cannot
/// store a filter value the projection would not. A string body lives only in
/// `body_text`, so a row whose `body` Variant holds a string, or that sets
/// both `body` and `body_text`, is refused.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] for a malformed source column, a
/// body stored twice, or the first derived column that differs.
pub(crate) fn refuse_forged_log_derivations(batch: &RecordBatch) -> Result<(), BifrostError> {
    let column = |name: &str| {
        batch
            .column_by_name(name)
            .ok_or_else(|| BifrostError::SchemaParse {
                detail: format!("canonical batch is missing column {name}"),
            })
    };
    let (attributes, body, body_text) =
        (column("attributes")?, column("body")?, column("body_text")?);
    let mut stored = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        stored.push(stored_attributes(attributes.as_ref(), row)?);
        if !body.is_null(row)
            && (!body_text.is_null(row) || stored_text(body.as_ref(), row)?.is_some())
        {
            return Err(BifrostError::SchemaParse {
                detail: format!(
                    "canonical log row {row} stores its body twice: a string body \
                     belongs only in body_text"
                ),
            });
        }
    }
    let exception: Vec<ExceptionPromotions<'_>> = stored
        .iter()
        .map(ExceptionPromotions::from_attributes)
        .collect();
    let text = |pick: for<'r> fn(&ExceptionPromotions<'r>) -> Option<&'r str>| {
        utf8_opt_column(&exception.iter().map(pick).collect::<Vec<_>>())
    };
    [
        ("exception_type", text(|row| row.exception_type)),
        ("exception_message", text(|row| row.message)),
        ("exception_stacktrace", text(|row| row.stacktrace)),
        (
            "is_exception",
            bool_column(exception.iter().map(ExceptionPromotions::any).collect()),
        ),
    ]
    .iter()
    .try_for_each(|(name, derived)| refuse_forged(batch, name, derived))
}

/// Bytes a string Variant adds to its text at most: its metadata and its
/// long-string value header.
const STRING_VARIANT_OVERHEAD_BYTES: usize = 16;

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
    card_scope: Option<&[CardUid]>,
    output_limit_bytes: usize,
) -> Result<(RecordBatch, LogsOutcome), TableError> {
    project_log_input(
        &LogInput::from_generated(resource_logs),
        card_scope,
        output_limit_bytes,
    )
}

/// Project one flattened log export into one canonical log batch.
///
/// This is the projection [`project_resource_logs`] describes, read from a
/// [`LogInput`]; both entry points produce identical batches and outcomes for
/// the same export.
///
/// # Errors
///
/// Returns the errors [`project_resource_logs`] returns.
pub fn project_log_input(
    input: &LogInput<'_>,
    card_scope: Option<&[CardUid]>,
    output_limit_bytes: usize,
) -> Result<(RecordBatch, LogsOutcome), TableError> {
    let mut columns = LogColumns::default();
    let mut budget = OutputBudget::new(output_limit_bytes);
    let row_bytes = log_row_output_bytes();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;

    let mut ordinal: u64 = 0;

    for resource in &input.resources {
        let envelope = ResourceEnvelope::project(resource.resource.as_deref(), resource.schema_url)
            .map(|envelope| columns.envelopes.retain_resource(envelope));
        for scope in &input.scopes[resource.scopes.clone()] {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_deref(), scope.schema_url)
                .map(|envelope| columns.envelopes.retain_scope(envelope));
            for record in &input.records[scope.records.clone()] {
                let row = ordinal;
                ordinal = ordinal.saturating_add(1);
                let outcome = match (&envelope, &scope_envelope) {
                    (Err(failure), _) | (Ok(_), Err(failure)) => Err(failure.reason(row)),
                    (Ok(resource), Ok(scope)) => columns
                        .push(
                            input,
                            record,
                            Context {
                                resource: resource.0,
                                scope: scope.0,
                            },
                            card_scope,
                            row,
                        )
                        .map(|bytes| bytes + resource.1 + scope.1),
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

/// Projected log schema, built once: every log batch shares this `Arc`, so
/// batches of one destination compare by pointer and no row rebuilds or
/// frees the schema tree.
static LOG_SCHEMA: LazyLock<Arc<Schema>> = LazyLock::new(|| projected_signal_schema(LOG_FIELDS));

/// Fixed Arrow bytes of one projected log row, derived once.
static LOG_ROW_BYTES: LazyLock<usize> = LazyLock::new(|| fixed_row_bytes(LOG_SCHEMA.fields()));

/// Returns the fixed Arrow bytes one projected log row retains.
///
/// The log projector charges this with each accepted row, and the server's
/// OTLP preflight charges it per wire record.
#[must_use]
pub fn log_row_output_bytes() -> usize {
    *LOG_ROW_BYTES
}

/// Return the canonical user-column schema of `bifrost.logs.records`.
#[must_use]
pub fn canonical_log_schema() -> Arc<Schema> {
    Arc::new(Schema::new(canonical_arrow_fields(LOG_FIELDS)))
}

/// Column-wise accumulator for the accepted canonical log rows.
///
/// Record text borrows the decoded request, bodies and attributes are encoded
/// straight into their Variant columns, and each resource and scope envelope
/// is retained once and referenced per row, so no value is copied before its
/// final Arrow column. A rejected record leaves no row behind.
#[derive(Debug, Default)]
struct LogColumns<'a> {
    rows: usize,
    /// Each resource and scope envelope, and the ones every row repeats.
    envelopes: RowEnvelopes,
    time_unix_nano: Vec<i64>,
    observed_time_unix_nano: Vec<i64>,
    severity_number: Vec<i32>,
    severity_text: Vec<&'a str>,
    event_name: Vec<Option<&'a str>>,
    /// Record body as a Variant; null when the record carries none or a
    /// string, which `body_text` stores alone.
    body: VariantColumnEncoder,
    trace_id: Vec<Option<[u8; 16]>>,
    span_id: Vec<Option<[u8; 8]>>,
    flags: Vec<u32>,
    /// Record attributes as one Variant object, final duplicate key kept.
    attributes: VariantColumnEncoder,
    /// Duplicate-key stack shared by every Variant value of the request.
    variant_scratch: Vec<usize>,
    dropped_attributes_count: Vec<u32>,
    /// Promoted `exception.type` attribute string.
    exception_type: Vec<Option<&'a str>>,
    /// Promoted `exception.message` attribute string.
    exception_message: Vec<Option<&'a str>>,
    /// Promoted `exception.stacktrace` attribute string.
    exception_stacktrace: Vec<Option<&'a str>>,
    /// The body when it is a string, stored only here; else null.
    body_text: Vec<Option<&'a str>>,
    /// Whether any promoted exception string is present.
    is_exception: Vec<bool>,
    card_uid: Vec<Option<String>>,
    run_id: Vec<Option<String>>,
}

impl<'a> LogColumns<'a> {
    /// Validate one log record completely, then append its canonical row.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an invalid correlation
    /// identifier, a malformed or wrong-typed `wyrd.card_uid` / `wyrd.run_id`
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
        input: &'a LogInput<'a>,
        record: &RecordInput<'a>,
        context: Context,
        card_scope: Option<&[CardUid]>,
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
            Some(trace_id_bytes(record.trace_id)?)
        };
        let span_id = if record.span_id.is_empty() {
            None
        } else {
            Some(span_id_bytes(record.span_id)?)
        };
        let observed_time_unix_nano = checked_i64(record.observed_time_unix_nano)?;
        // OTLP leaves `time_unix_nano` 0 when the source recorded no event
        // time; the time column then holds the collector's observed time, as
        // the OpenTelemetry ClickHouse exporter stores it.
        let time_unix_nano = match record.time_unix_nano {
            0 => observed_time_unix_nano,
            time => checked_i64(time)?,
        };
        let attributes = input.attributes(record.attributes);
        let body_mark = self.body.checkpoint();
        let attributes_mark = self.attributes.checkpoint();
        let (body_text, body_bytes, attribute_bytes, correlation) =
            match self.append_variants(input, record, attributes, card_scope, row) {
                Ok(appended) => appended,
                Err(reason) => {
                    self.body.rollback(body_mark);
                    self.attributes.rollback(attributes_mark);
                    return Err(reason);
                }
            };
        if body_text.is_some() || record.body.is_none() {
            self.body.append_null();
        }
        let exception = ExceptionPromotions::from_attributes(attributes);
        let payload_bytes = record.severity_text.len()
            + record.event_name.len()
            + body_bytes
            + attribute_bytes
            + exception.bytes()
            + correlation.card_uid.as_ref().map_or(0, String::len)
            + correlation.run_id.as_ref().map_or(0, String::len);

        self.envelopes.push(context);
        self.time_unix_nano.push(time_unix_nano);
        self.observed_time_unix_nano.push(observed_time_unix_nano);
        self.severity_number.push(record.severity_number);
        self.severity_text.push(record.severity_text);
        self.event_name
            .push((!record.event_name.is_empty()).then_some(record.event_name));
        self.trace_id.push(trace_id);
        self.span_id.push(span_id);
        self.flags.push(record.flags);
        self.dropped_attributes_count
            .push(record.dropped_attributes_count);
        self.is_exception.push(exception.any());
        self.exception_type.push(exception.exception_type);
        self.exception_message.push(exception.message);
        self.exception_stacktrace.push(exception.stacktrace);
        self.body_text.push(body_text);
        self.card_uid.push(correlation.card_uid);
        self.run_id.push(correlation.run_id);

        self.rows += 1;
        Ok(payload_bytes)
    }

    /// Encode the record's body and attributes into their Variant columns,
    /// then read its correlation.
    ///
    /// A string body is stored only as `body_text`: it is encoded, and then
    /// removed again, only when it nears the limit, so it is refused with
    /// exactly the reason and boundary a stored Variant body would have. Any
    /// other present body is appended to the body column; the caller appends
    /// the null row a string or absent body stores. On error the caller rolls
    /// both columns back.
    ///
    /// Returns the string body, the body's and the attributes' payload bytes,
    /// and the correlation.
    ///
    /// # Errors
    ///
    /// Returns the first refusal in order: an unstorable or oversized body,
    /// unstorable or oversized attributes, then an invalid correlation.
    fn append_variants(
        &mut self,
        input: &'a LogInput<'a>,
        record: &RecordInput<'a>,
        attributes: Attributes<'a>,
        card_scope: Option<&[CardUid]>,
        row: u64,
    ) -> Result<(Option<&'a str>, usize, usize, RecordCorrelation), Cow<'static, str>> {
        let scratch = &mut self.variant_scratch;
        let body = record.body.map(|body| input.body(body));
        let body_text = match body {
            Some(ValueRef::String(text)) => Some(text),
            _ => None,
        };
        let near_limit = body_text
            .is_some_and(|text| text.len() + STRING_VARIANT_OVERHEAD_BYTES > MAX_LOG_BODY_BYTES);
        let body_bytes = match (body, body_text) {
            (Some(value), None) => append_value(&mut self.body, "body", value, scratch)
                .map_err(|failure| failure.reason(row))?,
            (Some(value), Some(_)) if near_limit => {
                let mark = self.body.checkpoint();
                let encoded = append_value(&mut self.body, "body", value, scratch)
                    .map_err(|failure| failure.reason(row))?;
                self.body.rollback(mark);
                encoded
            }
            (_, text) => text.map_or(0, str::len),
        };
        if body_bytes > MAX_LOG_BODY_BYTES {
            return Err("log body exceeds the accepted payload size".into());
        }
        let attribute_bytes =
            append_attributes(&mut self.attributes, "attributes", attributes, scratch)
                .map_err(|failure| failure.reason(row))?;
        if attribute_bytes > MAX_LOG_ATTRIBUTES_BYTES {
            return Err("log attributes exceed the accepted payload size".into());
        }
        let correlation = RecordCorrelation::extract(attributes, card_scope)?;
        Ok((body_text, body_bytes, attribute_bytes, correlation))
    }

    /// Assemble the accepted rows into the projected log batch.
    ///
    /// The batch carries the canonical ledger columns plus the two nullable
    /// `card_uid` and `run_id` correlation columns Scribe consumes.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when a column cannot be built or the
    /// assembled columns do not match the canonical schema.
    fn finish(self) -> Result<RecordBatch, TableError> {
        let variant = |column: VariantColumnEncoder| {
            column
                .finish()
                .map_err(|error| TableError::Internal(error.to_string()))
        };
        let mut columns: Vec<ArrayRef> = vec![
            otlp_instant_column(self.time_unix_nano),
            otlp_instant_column(self.observed_time_unix_nano),
            i32_column(self.severity_number),
            utf8_column(&self.severity_text),
            utf8_opt_column(&self.event_name),
            variant(self.body)?,
            fixed_binary_opt_column(16, &self.trace_id).map_err(internal)?,
            fixed_binary_opt_column(8, &self.span_id).map_err(internal)?,
            u32_as_i64_column(self.flags),
            variant(self.attributes)?,
            u32_as_i64_column(self.dropped_attributes_count),
        ];
        columns.extend(self.envelopes.columns()?);
        columns.extend([
            self.envelopes
                .resource_text(|row| row.promotions.service_name.as_deref()),
            self.envelopes
                .resource_text(|row| row.promotions.service_version.as_deref()),
            self.envelopes
                .resource_text(|row| row.promotions.deployment_environment.as_deref()),
        ]);
        columns.extend(ResourcePromotionColumns::from_rows(
            self.envelopes.resources().map(|row| &row.promotions),
        ));
        columns.extend([
            utf8_opt_column(&self.exception_type),
            utf8_opt_column(&self.exception_message),
            utf8_opt_column(&self.exception_stacktrace),
            utf8_opt_column(&self.body_text),
            bool_column(self.is_exception),
            utf8_opt_column(&self.card_uid),
            utf8_opt_column(&self.run_id),
        ]);
        RecordBatch::try_new(Arc::clone(&LOG_SCHEMA), columns)
            .map_err(|error| TableError::Internal(format!("canonical log batch: {error}")))
    }
}

/// Wrap a stable assembly reason as an internal projection defect.
fn internal(reason: &'static str) -> TableError {
    TableError::Internal(reason.to_owned())
}
