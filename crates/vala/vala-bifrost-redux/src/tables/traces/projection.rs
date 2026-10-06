//! Table-owned OTLP trace projection for `vala.traces.spans`.
//!
//! [`project_resource_spans`] is the single deterministic, IO-free conversion
//! from decoded OTLP resource spans into the canonical span row. It performs no
//! authentication, stamps no server-managed identity, writes no WAL, and
//! touches no storage: it validates each span completely, accepts or rejects it
//! whole, and materializes the accepted subset into one canonical user-column
//! batch plus the existing [`IngestOutcome`].
//!
//! Column order and field meaning come exclusively from
//! [`super::spans::SPAN_FIELDS`]; nothing in this module restates them.

use arrow::array::ArrayRef;
use arrow::datatypes::{Fields, Schema};
use arrow::record_batch::RecordBatch;
use std::borrow::Cow;
use std::sync::Arc;
use wyrd_queue::variant::EncodedVariant;
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{EntityRef, KeyValue};
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};

use super::spans::{SPAN_EVENT_ELEMENT, SPAN_FIELDS, SPAN_LINK_ELEMENT};
use crate::otlp_contract::IngestOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    ExceptionPromotions, OutputBudget, RecordCorrelation, ResourceEnvelope, ScopeEnvelope,
    attributes_variant, bool_column, checked_i64, entity_refs_column, fixed_binary_column,
    fixed_binary_opt_column, fixed_row_bytes, i32_column, i32_opt_column, i64_column,
    i64_opt_column, internal, last_attribute, list_column, nested_fields, projected_signal_schema,
    promoted_int, promoted_string, span_id_bytes, struct_column, trace_id_bytes, u32_as_i64_column,
    utf8_column, utf8_opt_column, variant_column,
};
use wyrd_spec::reference::CardRefScope;

/// Largest accepted span or event name, in bytes.
const MAX_NAME_BYTES: usize = 256;
/// Largest accepted `trace_state` value, in bytes.
const MAX_TRACE_STATE_BYTES: usize = 512;
/// Largest accepted number of distinct span attribute keys.
const MAX_SPAN_ATTRIBUTE_KEYS: usize = 256;
/// Largest accepted number of distinct resource attribute keys.
const MAX_RESOURCE_ATTRIBUTE_KEYS: usize = 260;
/// Largest accepted number of distinct scope attribute keys.
const MAX_SCOPE_ATTRIBUTE_KEYS: usize = 32;
/// Largest accepted number of distinct event or link attribute keys.
const MAX_CHILD_ATTRIBUTE_KEYS: usize = 128;
/// Largest accepted scope version, in bytes.
const MAX_SCOPE_VERSION_BYTES: usize = 64;

/// Name of the span event whose attributes carry the `exception.*`
/// conventions; the last such event is the one promoted.
const EXCEPTION_EVENT_NAME: &str = "exception";

/// The pinned `GenAI` string promotions, as (attribute key, canonical column).
///
/// The mapping follows `OpenTelemetry` `GenAI` semantic conventions commit
/// `94f432d7126f5884d30a2cdde6f4e89908ebb6fd`. There is no legacy alias and no
/// fallback source: a missing attribute promotes to null and a present
/// attribute of the wrong type rejects the containing span.
const GEN_AI_STRING_PROMOTIONS: [&str; 4] = [
    "gen_ai.operation.name",
    "gen_ai.provider.name",
    "gen_ai.request.model",
    "gen_ai.conversation.id",
];

/// The pinned `GenAI` signed-integer promotions, in canonical column order.
const GEN_AI_INT_PROMOTIONS: [&str; 2] =
    ["gen_ai.usage.input_tokens", "gen_ai.usage.output_tokens"];

/// Project decoded OTLP resource spans into one canonical span batch.
///
/// Each span is atomic: it is either accepted with every supported field
/// projected losslessly, or rejected whole. A shared resource or scope defect
/// rejects each descendant span it affects. Accepted spans retain their
/// relative request order, and the returned [`IngestOutcome`] carries the exact
/// accepted and rejected counts plus the first rejection reason in traversal
/// order, which is what the existing OTLP partial-success response encodes.
/// A value that cannot be stored as Variant rejects its span with a reason
/// that starts with the catalogued Variant error code and names the span's
/// zero-based traversal ordinal; nothing is truncated.
///
/// # Errors
///
/// Returns [`TableError::OutputTooLarge`] as soon as the running Arrow output
/// of accepted spans crosses `output_limit_bytes`, before the batch is built.
/// Returns [`TableError::Internal`] only when the accepted rows cannot be
/// assembled into the canonical Arrow batch, which indicates a defect in this
/// projection rather than caller input.
pub fn project_resource_spans(
    resource_spans: &[ResourceSpans],
    card_scope: Option<&CardRefScope>,
    output_limit_bytes: usize,
) -> Result<(RecordBatch, IngestOutcome), TableError> {
    let mut columns = SpanColumns::default();
    let mut budget = OutputBudget::new(output_limit_bytes);
    let widths = SpanOutputWidths::new();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;

    let mut ordinal: u64 = 0;

    for resource in resource_spans {
        let envelope = ResourceEnvelope::project(resource.resource.as_ref(), &resource.schema_url);
        let resource_defect = validate_resource(resource);
        for scope in &resource.scope_spans {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_ref(), &scope.schema_url);
            let scope_defect = resource_defect.or_else(|| validate_scope(scope));
            for span in &scope.spans {
                let row = ordinal;
                ordinal = ordinal.saturating_add(1);
                let outcome = match (scope_defect, &envelope, &scope_envelope) {
                    (Some(reason), _, _) => Err(Cow::Borrowed(reason)),
                    (None, Err(failure), _) | (None, Ok(_), Err(failure)) => {
                        Err(failure.reason(row))
                    }
                    (None, Ok(envelope), Ok(scope_envelope)) => columns
                        .push(span, envelope, scope_envelope, card_scope, row)
                        .map(|bytes| {
                            bytes + envelope.repeated_bytes() + scope_envelope.repeated_bytes()
                        }),
                };
                match outcome {
                    Ok(payload_bytes) => budget.charge(
                        widths.row
                            + payload_bytes
                            + span.events.len() * widths.event
                            + span.links.len() * widths.link,
                    )?,
                    Err(reason) => {
                        rejected = rejected.saturating_add(1);
                        rejection_message.get_or_insert_with(|| reason.into_owned());
                    }
                }
            }
        }
    }

    let accepted = i64::try_from(columns.rows)
        .map_err(|_| TableError::Internal("accepted span count exceeds i64".to_owned()))?;
    let batch = columns.finish()?;
    Ok((
        batch,
        IngestOutcome {
            accepted_spans: accepted,
            rejected_spans: rejected,
            rejection_message,
        },
    ))
}

/// Fixed Arrow bytes one projected span row and each nested element retain.
///
/// The span projector charges these with each accepted row, and the server's
/// OTLP preflight charges the same widths per wire record and element, so both
/// bounds share one definition of a row's fixed output.
#[derive(Clone, Copy, Debug)]
pub struct SpanOutputWidths {
    /// Fixed bytes of one span row, correlation columns included.
    pub row: usize,
    /// Fixed bytes of one nested span event.
    pub event: usize,
    /// Fixed bytes of one nested span link.
    pub link: usize,
}

impl SpanOutputWidths {
    /// Derives the widths from the canonical span ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            row: fixed_row_bytes(projected_signal_schema(SPAN_FIELDS).fields()),
            event: fixed_row_bytes(&Fields::from(vec![SPAN_EVENT_ELEMENT.to_arrow()])),
            link: fixed_row_bytes(&Fields::from(vec![SPAN_LINK_ELEMENT.to_arrow()])),
        }
    }
}

impl Default for SpanOutputWidths {
    /// Derives the widths from the canonical ledger, as [`Self::new`] does.
    fn default() -> Self {
        Self::new()
    }
}

/// Validate the resource-level facts every descendant span inherits.
///
/// Returns the stable rejection reason, or `None` when the resource is
/// acceptable. A defect here rejects every span beneath it rather than being
/// silently repaired.
fn validate_resource(resource: &ResourceSpans) -> Option<&'static str> {
    let attributes = resource
        .resource
        .as_ref()
        .map_or(&[][..], |value| value.attributes.as_slice());
    (unique_keys(attributes) > MAX_RESOURCE_ATTRIBUTE_KEYS)
        .then_some("resource declares too many distinct attribute keys")
}

/// Validate the scope-level facts every descendant span inherits.
fn validate_scope(scope: &ScopeSpans) -> Option<&'static str> {
    let scope = scope.scope.as_ref()?;
    if scope.name.len() > MAX_NAME_BYTES {
        return Some("scope name exceeds the accepted length");
    }
    if scope.version.len() > MAX_SCOPE_VERSION_BYTES {
        return Some("scope version exceeds the accepted length");
    }
    (unique_keys(&scope.attributes) > MAX_SCOPE_ATTRIBUTE_KEYS)
        .then_some("scope declares too many distinct attribute keys")
}

/// Count the distinct keys in one OTLP attribute collection.
///
/// OTLP carries a logical map as repeated entries, so a duplicated key is one
/// key for cardinality purposes and resolves to its final value on read.
fn unique_keys(attributes: &[KeyValue]) -> usize {
    let mut seen: Vec<&str> = Vec::with_capacity(attributes.len());
    for attribute in attributes {
        if !seen.contains(&attribute.key.as_str()) {
            seen.push(attribute.key.as_str());
        }
    }
    seen.len()
}

/// Column-wise accumulator for the accepted canonical span rows.
///
/// One vector per declared ledger field, plus flattened child storage and per
/// row lengths for the three repeated collections. Rows are appended only after
/// the whole span validates, so a rejected span leaves no partial state behind.
#[derive(Debug, Default)]
struct SpanColumns {
    rows: usize,
    trace_id: Vec<Vec<u8>>,
    span_id: Vec<Vec<u8>>,
    parent_span_id: Vec<Option<Vec<u8>>>,
    trace_state: Vec<String>,
    flags: Vec<u32>,
    name: Vec<String>,
    kind: Vec<i32>,
    start_time_unix_nano: Vec<i64>,
    end_time_unix_nano: Vec<i64>,
    duration_nano: Vec<i64>,
    status_present: Vec<bool>,
    status_code: Vec<Option<i32>>,
    status_message: Vec<Option<String>>,
    attributes: Vec<EncodedVariant>,
    dropped_attributes_count: Vec<u32>,
    event_lengths: Vec<Option<usize>>,
    event_time: Vec<i64>,
    event_name: Vec<String>,
    event_attributes: Vec<EncodedVariant>,
    event_dropped: Vec<u32>,
    dropped_events_count: Vec<u32>,
    link_lengths: Vec<Option<usize>>,
    link_trace_id: Vec<Vec<u8>>,
    link_span_id: Vec<Vec<u8>>,
    link_trace_state: Vec<String>,
    link_flags: Vec<u32>,
    link_attributes: Vec<EncodedVariant>,
    link_dropped: Vec<u32>,
    dropped_links_count: Vec<u32>,
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
    gen_ai_strings: [Vec<Option<String>>; 4],
    gen_ai_ints: [Vec<Option<i64>>; 2],
    service_version: Vec<Option<String>>,
    deployment_environment: Vec<Option<String>>,
    http_request_method: Vec<Option<String>>,
    http_route: Vec<Option<String>>,
    http_response_status_code: Vec<Option<i64>>,
    url_full: Vec<Option<String>>,
    exception_type: Vec<Option<String>>,
    exception_message: Vec<Option<String>>,
    exception_stacktrace: Vec<Option<String>>,
    card_ref: Vec<Option<String>>,
    run_id: Vec<Option<String>>,
}

impl SpanColumns {
    /// Validate one span completely, then append its canonical row.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an invalid identifier, an
    /// out-of-range timestamp pair, an over-long or empty name, an attribute
    /// cardinality breach, an invalid event or link, a `GenAI` promotion whose
    /// source attribute carries the wrong protocol type, an optional
    /// correlation attribute that is wrongly typed or malformed, or a span,
    /// event, or link attribute set that cannot be stored as Variant. A Variant
    /// rejection reason starts with its catalogued code and names `row`, the
    /// span's zero-based traversal ordinal. Nothing is appended when an error
    /// is returned.
    ///
    /// On success returns the variable-length payload bytes the span itself
    /// appended (strings, attribute encodings, promotions, and event and link
    /// payloads); the caller adds fixed widths and the repeated resource and
    /// scope bytes.
    fn push(
        &mut self,
        span: &Span,
        resource: &ResourceEnvelope,
        scope: &ScopeEnvelope,
        card_scope: Option<&CardRefScope>,
        row: u64,
    ) -> Result<usize, Cow<'static, str>> {
        let trace_id = trace_id_bytes(&span.trace_id)?;
        let span_id = span_id_bytes(&span.span_id)?;
        let parent = if span.parent_span_id.is_empty() {
            None
        } else {
            Some(span_id_bytes(&span.parent_span_id)?.to_vec())
        };
        if span.name.is_empty() || span.name.len() > MAX_NAME_BYTES {
            return Err("span name is empty or exceeds the accepted length".into());
        }
        if span.trace_state.len() > MAX_TRACE_STATE_BYTES {
            return Err("span trace_state exceeds the accepted length".into());
        }
        if unique_keys(&span.attributes) > MAX_SPAN_ATTRIBUTE_KEYS {
            return Err("span declares too many distinct attribute keys".into());
        }
        let start = checked_i64(span.start_time_unix_nano)?;
        let end = checked_i64(span.end_time_unix_nano)?;
        let duration = end
            .checked_sub(start)
            .filter(|duration| *duration >= 0)
            .ok_or("span ends before it starts")?;

        let promotions = GenAiPromotions::extract(&span.attributes)?;
        let correlation = RecordCorrelation::extract(&span.attributes, card_scope)?;
        Self::validate_events(span)?;
        Self::validate_links(span)?;
        let attributes = attributes_variant("attributes", &span.attributes)
            .map_err(|failure| failure.reason(row))?;
        let event_attributes = Self::encode_children(
            "events",
            span.events.iter().map(|event| event.attributes.as_slice()),
            row,
        )?;
        let link_attributes = Self::encode_children(
            "links",
            span.links.iter().map(|link| link.attributes.as_slice()),
            row,
        )?;
        let http = HttpPromotions::extract(&span.attributes);
        let exception = Self::exception_promotions(span);
        let payload_bytes = self.push_events(span, event_attributes)
            + self.push_links(span, link_attributes)
            + span.trace_state.len()
            + span.name.len()
            + span
                .status
                .as_ref()
                .map_or(0, |status| status.message.len())
            + attributes.encoded_bytes()
            + http.bytes()
            + exception.bytes()
            + promotions
                .strings
                .iter()
                .flatten()
                .map(String::len)
                .sum::<usize>()
            + correlation.card_ref.as_ref().map_or(0, String::len)
            + correlation.run_id.as_ref().map_or(0, String::len);

        self.trace_id.push(trace_id.to_vec());
        self.span_id.push(span_id.to_vec());
        self.parent_span_id.push(parent);
        self.trace_state.push(span.trace_state.clone());
        self.flags.push(span.flags);
        self.name.push(span.name.clone());
        self.kind.push(span.kind);
        self.start_time_unix_nano.push(start);
        self.end_time_unix_nano.push(end);
        self.duration_nano.push(duration);
        self.status_present.push(span.status.is_some());
        self.status_code
            .push(span.status.as_ref().map(|status| status.code));
        self.status_message
            .push(span.status.as_ref().map(|status| status.message.clone()));
        self.attributes.push(attributes);
        self.dropped_attributes_count
            .push(span.dropped_attributes_count);
        self.event_lengths.push(Some(span.events.len()));
        self.dropped_events_count.push(span.dropped_events_count);
        self.link_lengths.push(Some(span.links.len()));
        self.dropped_links_count.push(span.dropped_links_count);

        self.push_context(resource, scope);
        for (column, value) in self.gen_ai_strings.iter_mut().zip(promotions.strings) {
            column.push(value);
        }
        for (column, value) in self.gen_ai_ints.iter_mut().zip(promotions.ints) {
            column.push(value);
        }
        self.http_request_method.push(http.request_method);
        self.http_route.push(http.route);
        self.http_response_status_code
            .push(http.response_status_code);
        self.url_full.push(http.url_full);
        self.exception_type.push(exception.exception_type);
        self.exception_message.push(exception.message);
        self.exception_stacktrace.push(exception.stacktrace);
        self.card_ref.push(correlation.card_ref);
        self.run_id.push(correlation.run_id);

        self.rows += 1;
        Ok(payload_bytes)
    }

    /// Append the resource and scope envelope one accepted span repeats,
    /// including the promoted resource conventions.
    fn push_context(&mut self, resource: &ResourceEnvelope, scope: &ScopeEnvelope) {
        self.resource_present.push(resource.present);
        self.resource_attributes.push(resource.attributes.clone());
        self.resource_dropped_attributes_count
            .push(resource.dropped_attributes_count);
        self.resource_schema_url.push(resource.schema_url.clone());
        self.entity_ref_lengths
            .push(Some(resource.entity_refs.len()));
        self.entity_refs.extend_from_slice(&resource.entity_refs);
        self.service_name.push(resource.service_name.clone());
        self.service_version.push(resource.service_version.clone());
        self.deployment_environment
            .push(resource.deployment_environment.clone());

        self.scope_present.push(scope.present);
        self.scope_name.push(scope.name.clone());
        self.scope_version.push(scope.version.clone());
        self.scope_attributes.push(scope.attributes.clone());
        self.scope_dropped_attributes_count
            .push(scope.dropped_attributes_count);
        self.scope_schema_url.push(scope.schema_url.clone());
    }

    /// Validate one span's ordered events without mutating any column.
    ///
    /// Validation is separated from appending so a rejected span can never
    /// leave partially staged child rows behind.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an empty or over-long event
    /// name, an attribute cardinality breach, or an event outside its span's
    /// time window.
    fn validate_events(span: &Span) -> Result<(), &'static str> {
        for event in &span.events {
            if event.name.is_empty() || event.name.len() > MAX_NAME_BYTES {
                return Err("span event name is empty or exceeds the accepted length");
            }
            if unique_keys(&event.attributes) > MAX_CHILD_ATTRIBUTE_KEYS {
                return Err("span event declares too many distinct attribute keys");
            }
            if event.time_unix_nano < span.start_time_unix_nano
                || event.time_unix_nano > span.end_time_unix_nano
            {
                return Err("span event falls outside its span time window");
            }
        }
        Ok(())
    }

    /// Validate one span's ordered links without mutating any column.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an invalid linked identifier, an
    /// over-long `trace_state`, or an attribute cardinality breach.
    fn validate_links(span: &Span) -> Result<(), &'static str> {
        for link in &span.links {
            trace_id_bytes(&link.trace_id)?;
            span_id_bytes(&link.span_id)?;
            if link.trace_state.len() > MAX_TRACE_STATE_BYTES {
                return Err("span link trace_state exceeds the accepted length");
            }
            if unique_keys(&link.attributes) > MAX_CHILD_ATTRIBUTE_KEYS {
                return Err("span link declares too many distinct attribute keys");
            }
        }
        Ok(())
    }

    /// Read the `exception.*` conventions from the span's last event named
    /// `exception`; every value is null when no such event exists.
    fn exception_promotions(span: &Span) -> ExceptionPromotions {
        span.events
            .iter()
            .rev()
            .find(|event| event.name == EXCEPTION_EVENT_NAME)
            .map(|event| ExceptionPromotions::from_attributes(&event.attributes))
            .unwrap_or_default()
    }

    /// Encode each nested event's or link's attribute set as a Variant object.
    ///
    /// Encoding happens before any column is touched, so a child that cannot
    /// be stored rejects the whole span without leaving staged rows behind.
    ///
    /// # Errors
    ///
    /// Returns the Variant rejection reason, naming `field` and the span's
    /// traversal ordinal `row`, for the first child set that cannot be stored.
    fn encode_children<'a>(
        field: &'static str,
        children: impl Iterator<Item = &'a [KeyValue]>,
        row: u64,
    ) -> Result<Vec<EncodedVariant>, Cow<'static, str>> {
        children
            .map(|attributes| {
                attributes_variant(field, attributes).map_err(|failure| failure.reason(row))
            })
            .collect()
    }

    /// Append one validated span's ordered events into the flattened storage.
    ///
    /// `attributes` holds each event's encoded attribute set in event order.
    /// Returns the event names and attribute encodings' payload bytes.
    ///
    /// # Panics
    ///
    /// Panics only if a validated event time exceeds `i64`, which validation
    /// against the span's checked end time rules out.
    fn push_events(&mut self, span: &Span, attributes: Vec<EncodedVariant>) -> usize {
        let mut payload_bytes = 0;
        for (event, attributes) in span.events.iter().zip(attributes) {
            self.event_time.push(
                checked_i64(event.time_unix_nano)
                    .expect("a validated event time never exceeds its span's checked end"),
            );
            payload_bytes += event.name.len() + attributes.encoded_bytes();
            self.event_name.push(event.name.clone());
            self.event_attributes.push(attributes);
            self.event_dropped.push(event.dropped_attributes_count);
        }
        payload_bytes
    }

    /// Append one validated span's ordered links into the flattened storage.
    ///
    /// `attributes` holds each link's encoded attribute set in link order.
    /// Returns the links' `trace_state` and attribute encodings' payload bytes;
    /// fixed-width identifiers are charged with the link's fixed width.
    fn push_links(&mut self, span: &Span, attributes: Vec<EncodedVariant>) -> usize {
        let mut payload_bytes = 0;
        for (link, attributes) in span.links.iter().zip(attributes) {
            self.link_trace_id.push(link.trace_id.clone());
            self.link_span_id.push(link.span_id.clone());
            payload_bytes += link.trace_state.len() + attributes.encoded_bytes();
            self.link_trace_state.push(link.trace_state.clone());
            self.link_flags.push(link.flags);
            self.link_attributes.push(attributes);
            self.link_dropped.push(link.dropped_attributes_count);
        }
        payload_bytes
    }

    /// Assemble the accepted rows into the canonical span batch.
    ///
    /// Column order comes from [`SPAN_FIELDS`], so this method and the ledger
    /// cannot drift apart without the assembly failing. The two nullable
    /// correlation columns follow the ledger and belong to Scribe's stamping
    /// contract, not to the declared schema.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when a column cannot be built or the
    /// assembled columns do not match the canonical schema.
    fn finish(self) -> Result<RecordBatch, TableError> {
        let event_children = nested_fields(&SPAN_EVENT_ELEMENT.to_arrow())?;
        let link_children = nested_fields(&SPAN_LINK_ELEMENT.to_arrow())?;

        let events = struct_column(
            &event_children,
            vec![
                i64_column(self.event_time),
                utf8_column(self.event_name),
                variant_column(self.event_attributes.iter().map(Some)),
                u32_as_i64_column(self.event_dropped),
            ],
            None,
        )
        .map_err(internal)?;
        let links = struct_column(
            &link_children,
            vec![
                fixed_binary_column(16, &self.link_trace_id).map_err(internal)?,
                fixed_binary_column(8, &self.link_span_id).map_err(internal)?,
                utf8_column(self.link_trace_state),
                u32_as_i64_column(self.link_flags),
                variant_column(self.link_attributes.iter().map(Some)),
                u32_as_i64_column(self.link_dropped),
            ],
            None,
        )
        .map_err(internal)?;

        let mut gen_ai_strings = self.gen_ai_strings.into_iter();
        let mut gen_ai_ints = self.gen_ai_ints.into_iter();
        let columns: Vec<ArrayRef> = vec![
            fixed_binary_column(16, &self.trace_id).map_err(internal)?,
            fixed_binary_column(8, &self.span_id).map_err(internal)?,
            fixed_binary_opt_column(8, &self.parent_span_id).map_err(internal)?,
            utf8_column(self.trace_state),
            u32_as_i64_column(self.flags),
            utf8_column(self.name),
            i32_column(self.kind),
            i64_column(self.start_time_unix_nano),
            i64_column(self.end_time_unix_nano),
            i64_column(self.duration_nano),
            bool_column(self.status_present),
            i32_opt_column(self.status_code),
            utf8_opt_column(self.status_message),
            variant_column(self.attributes.iter().map(Some)),
            u32_as_i64_column(self.dropped_attributes_count),
            list_column(&SPAN_EVENT_ELEMENT.to_arrow(), events, &self.event_lengths)
                .map_err(internal)?,
            u32_as_i64_column(self.dropped_events_count),
            list_column(&SPAN_LINK_ELEMENT.to_arrow(), links, &self.link_lengths)
                .map_err(internal)?,
            u32_as_i64_column(self.dropped_links_count),
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
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            i64_opt_column(gen_ai_ints.next().unwrap_or_default()),
            i64_opt_column(gen_ai_ints.next().unwrap_or_default()),
            utf8_opt_column(self.service_version),
            utf8_opt_column(self.deployment_environment),
            utf8_opt_column(self.http_request_method),
            utf8_opt_column(self.http_route),
            i64_opt_column(self.http_response_status_code),
            utf8_opt_column(self.url_full),
            utf8_opt_column(self.exception_type),
            utf8_opt_column(self.exception_message),
            utf8_opt_column(self.exception_stacktrace),
            utf8_opt_column(self.card_ref),
            utf8_opt_column(self.run_id),
        ];

        RecordBatch::try_new(projected_signal_schema(SPAN_FIELDS), columns)
            .map_err(|error| TableError::Internal(format!("canonical span batch: {error}")))
    }
}

/// Return the canonical user-column schema of `vala.traces.spans`.
///
/// The schema is derived from the ledger on every call rather than cached, so
/// there is exactly one place a span column order can come from.
#[must_use]
pub fn canonical_span_schema() -> Arc<Schema> {
    Arc::new(Schema::new(canonical_arrow_fields(SPAN_FIELDS)))
}

/// The pinned `GenAI` promotions extracted from one span's attributes.
///
/// Values are the exact typed values retained in the canonical attributes
/// payload, so a promoted column can never disagree with its canonical source.
#[derive(Debug, Default)]
struct GenAiPromotions {
    /// String promotions in [`GEN_AI_STRING_PROMOTIONS`] order.
    strings: [Option<String>; 4],
    /// Signed-integer promotions in [`GEN_AI_INT_PROMOTIONS`] order.
    ints: [Option<i64>; 2],
}

impl GenAiPromotions {
    /// Extract every pinned promotion, rejecting a wrongly typed source.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason when a pinned source attribute is
    /// present but is not the exact protocol type the convention defines.
    fn extract(attributes: &[KeyValue]) -> Result<Self, &'static str> {
        let mut promotions = Self::default();
        for (slot, key) in promotions.strings.iter_mut().zip(GEN_AI_STRING_PROMOTIONS) {
            *slot = match last_attribute(attributes, key) {
                None => None,
                Some(entry) => match entry.value.as_ref().and_then(|value| value.value.as_ref()) {
                    Some(Value::StringValue(text)) => Some(text.clone()),
                    _ => return Err("a promoted gen_ai attribute has the wrong type"),
                },
            };
        }
        for (slot, key) in promotions.ints.iter_mut().zip(GEN_AI_INT_PROMOTIONS) {
            *slot = match last_attribute(attributes, key) {
                None => None,
                Some(entry) => match entry.value.as_ref().and_then(|value| value.value.as_ref()) {
                    Some(Value::IntValue(number)) => Some(*number),
                    _ => return Err("a promoted gen_ai attribute has the wrong type"),
                },
            };
        }
        Ok(promotions)
    }
}

/// The promoted HTTP semantic conventions of one span's attributes.
///
/// Unlike the `GenAI` promotions, a missing or wrongly typed source is null
/// rather than a rejection; the canonical attributes keep the original value.
#[derive(Debug, Default)]
struct HttpPromotions {
    /// `http.request.method`.
    request_method: Option<String>,
    /// `http.route`.
    route: Option<String>,
    /// `http.response.status_code`, as a signed integer.
    response_status_code: Option<i64>,
    /// `url.full`.
    url_full: Option<String>,
}

impl HttpPromotions {
    /// Read the HTTP conventions from one span's attributes.
    fn extract(attributes: &[KeyValue]) -> Self {
        Self {
            request_method: promoted_string(attributes, "http.request.method"),
            route: promoted_string(attributes, "http.route"),
            response_status_code: promoted_int(attributes, "http.response.status_code"),
            url_full: promoted_string(attributes, "url.full"),
        }
    }

    /// Returns the promoted text bytes these values add to one row.
    fn bytes(&self) -> usize {
        [&self.request_method, &self.route, &self.url_full]
            .into_iter()
            .flatten()
            .map(String::len)
            .sum()
    }
}
