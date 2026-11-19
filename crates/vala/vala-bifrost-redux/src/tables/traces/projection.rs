//! Table-owned OTLP trace projection for `bifrost.traces.spans`.
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

use arrow::array::{ArrayRef, AsArray};
use arrow::datatypes::{Fields, Schema, TimestampNanosecondType};
use arrow::record_batch::RecordBatch;
use std::borrow::Cow;
use std::sync::{Arc, LazyLock};
use wyrd_queue::variant::VariantColumnEncoder;
use wyrd_spec::vala::BifrostError;
use wyrd_tonic::otlp::common::v1::InstrumentationScope;
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::span::Event;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, Span};

use super::input::{EventInput, LinkInput, SpanInput, TraceInput};
use super::spans::{SPAN_EVENT_ELEMENT, SPAN_FIELDS, SPAN_LINK_ELEMENT};
use crate::otlp_contract::IngestOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    Attributes, ExceptionPromotions, OutputBudget, RecordCorrelation, ResourceEnvelope,
    ResourcePromotionColumns, ScopeEnvelope, ValueRef, VariantFailure, append_attributes,
    bool_column, checked_i64, distinct_keys, entity_refs_column, fixed_binary_column,
    fixed_binary_opt_column, fixed_row_bytes, i32_column, i32_opt_column, i64_column,
    i64_opt_column, internal, last_attribute, last_string_attribute, list_column, nested_fields,
    otlp_instant_column, projected_signal_schema, promoted_int, refuse_forged, span_id_bytes,
    stored_attributes, struct_column, trace_id_bytes, u32_as_i64_column, utf8_column,
    utf8_opt_column, utf8_opt_column_from, variant_column,
};
use wyrd_spec::ids::CardUid;

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
    card_scope: Option<&[CardUid]>,
    output_limit_bytes: usize,
) -> Result<(RecordBatch, IngestOutcome), TableError> {
    project_trace_input(
        &TraceInput::from_generated(resource_spans),
        card_scope,
        output_limit_bytes,
    )
}

/// Project one flattened trace export into one canonical span batch.
///
/// This is the projection [`project_resource_spans`] describes, read from a
/// [`TraceInput`]; both entry points produce identical batches and outcomes
/// for the same export.
///
/// # Errors
///
/// Returns the errors [`project_resource_spans`] returns.
pub fn project_trace_input(
    input: &TraceInput<'_>,
    card_scope: Option<&[CardUid]>,
    output_limit_bytes: usize,
) -> Result<(RecordBatch, IngestOutcome), TableError> {
    let mut columns = SpanColumns::default();
    // Envelopes are retained once per group and referenced by each accepted
    // row, so `(index, repeated bytes)` is all a span needs to carry.
    let mut budget = OutputBudget::new(output_limit_bytes);
    let widths = SpanOutputWidths::new();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;

    let mut ordinal: u64 = 0;

    for resource in &input.resources {
        let envelope = ResourceEnvelope::project(resource.resource.as_deref(), resource.schema_url)
            .map(|envelope| columns.retain_resource(envelope));
        let resource_defect = validate_resource(resource.resource.as_deref());
        for scope in &input.scopes[resource.scopes.clone()] {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_deref(), scope.schema_url)
                .map(|envelope| columns.retain_scope(envelope));
            let scope_defect = resource_defect.or_else(|| validate_scope(scope.scope.as_deref()));
            for span in &input.spans[scope.spans.clone()] {
                let row = ordinal;
                ordinal = ordinal.saturating_add(1);
                let outcome = match (scope_defect, &envelope, &scope_envelope) {
                    (Some(reason), _, _) => Err(Cow::Borrowed(reason)),
                    (None, Err(failure), _) | (None, Ok(_), Err(failure)) => {
                        Err(failure.reason(row))
                    }
                    (None, Ok(resource), Ok(scope)) => columns
                        .push(
                            input,
                            span,
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

/// Projected span schema, built once: every span batch shares this `Arc`, so
/// batches of one destination compare by pointer and no row rebuilds or
/// frees the schema tree.
static SPAN_SCHEMA: LazyLock<Arc<Schema>> = LazyLock::new(|| projected_signal_schema(SPAN_FIELDS));

/// Span output widths, derived once from the constant span ledger.
static SPAN_WIDTHS: LazyLock<SpanOutputWidths> = LazyLock::new(|| SpanOutputWidths {
    row: fixed_row_bytes(SPAN_SCHEMA.fields()),
    event: fixed_row_bytes(&Fields::from(vec![SPAN_EVENT_ELEMENT.to_arrow()])),
    link: fixed_row_bytes(&Fields::from(vec![SPAN_LINK_ELEMENT.to_arrow()])),
});

impl SpanOutputWidths {
    /// The widths of the canonical span ledger, derived once per process.
    #[must_use]
    pub fn new() -> Self {
        *SPAN_WIDTHS
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
fn validate_resource(resource: Option<&Resource>) -> Option<&'static str> {
    let attributes = resource.map_or(&[][..], |value| value.attributes.as_slice());
    (distinct_keys(attributes) > MAX_RESOURCE_ATTRIBUTE_KEYS)
        .then_some("resource declares too many distinct attribute keys")
}

/// Validate the scope-level facts every descendant span inherits.
fn validate_scope(scope: Option<&InstrumentationScope>) -> Option<&'static str> {
    let scope = scope?;
    if scope.name.len() > MAX_NAME_BYTES {
        return Some("scope name exceeds the accepted length");
    }
    if scope.version.len() > MAX_SCOPE_VERSION_BYTES {
        return Some("scope version exceeds the accepted length");
    }
    (distinct_keys(&scope.attributes) > MAX_SCOPE_ATTRIBUTE_KEYS)
        .then_some("scope declares too many distinct attribute keys")
}

/// The resource and scope envelopes one accepted span repeats, as indices
/// into [`SpanColumns::resources`] and [`SpanColumns::scopes`].
#[derive(Debug, Clone, Copy)]
struct Context {
    /// Index of the span's resource envelope.
    resource: usize,
    /// Index of the span's scope envelope.
    scope: usize,
}

/// Column-wise accumulator for the accepted canonical span rows.
///
/// One vector per declared ledger field, plus flattened child storage and per
/// row lengths for the three repeated collections. Span text and identifiers
/// borrow the decoded request, and each resource and scope envelope is
/// retained once and referenced per row, so no value is copied before its
/// final Arrow column. Rows are appended only after the whole span validates,
/// so a rejected span leaves no partial state behind.
#[derive(Debug, Default)]
struct SpanColumns<'a> {
    rows: usize,
    /// Every projected resource envelope, in request order.
    resources: Vec<ResourceEnvelope>,
    /// Every projected scope envelope, in request order.
    scopes: Vec<ScopeEnvelope>,
    /// Each accepted row's envelopes.
    context: Vec<Context>,
    trace_id: Vec<[u8; 16]>,
    span_id: Vec<[u8; 8]>,
    parent_span_id: Vec<Option<[u8; 8]>>,
    trace_state: Vec<&'a str>,
    flags: Vec<u32>,
    name: Vec<&'a str>,
    kind: Vec<i32>,
    start_time_unix_nano: Vec<i64>,
    end_time_unix_nano: Vec<i64>,
    duration_nano: Vec<i64>,
    status_present: Vec<bool>,
    status_code: Vec<Option<i32>>,
    status_message: Vec<Option<&'a str>>,
    /// Span attributes as one Variant object, final duplicate key kept.
    attributes: VariantColumnEncoder,
    dropped_attributes_count: Vec<u32>,
    event_lengths: Vec<Option<usize>>,
    event_time: Vec<i64>,
    event_name: Vec<&'a str>,
    /// Flattened span-event attributes, one Variant object each.
    event_attributes: VariantColumnEncoder,
    event_dropped: Vec<u32>,
    dropped_events_count: Vec<u32>,
    link_lengths: Vec<Option<usize>>,
    link_trace_id: Vec<&'a [u8]>,
    link_span_id: Vec<&'a [u8]>,
    link_trace_state: Vec<&'a str>,
    link_flags: Vec<u32>,
    /// Flattened span-link attributes, one Variant object each.
    link_attributes: VariantColumnEncoder,
    /// Duplicate-key stack shared by every attribute object of the request.
    variant_scratch: Vec<usize>,
    link_dropped: Vec<u32>,
    dropped_links_count: Vec<u32>,
    gen_ai_strings: [Vec<Option<&'a str>>; 4],
    gen_ai_ints: [Vec<Option<i64>>; 2],
    /// Promoted `http.request.method` attribute string.
    http_request_method: Vec<Option<&'a str>>,
    /// Promoted `http.route` attribute string.
    http_route: Vec<Option<&'a str>>,
    /// Promoted `http.response.status_code` attribute integer.
    http_response_status_code: Vec<Option<i64>>,
    /// Promoted `url.full` attribute string.
    url_full: Vec<Option<&'a str>>,
    /// Promoted `url.path` attribute string.
    url_path: Vec<Option<&'a str>>,
    /// Promoted `url.query` attribute string.
    url_query: Vec<Option<&'a str>>,
    /// Promoted `exception.type` attribute string.
    exception_type: Vec<Option<&'a str>>,
    /// Promoted `exception.message` attribute string.
    exception_message: Vec<Option<&'a str>>,
    /// Promoted `exception.stacktrace` attribute string.
    exception_stacktrace: Vec<Option<&'a str>>,
    /// Whether the span carries any event named `exception`.
    is_exception: Vec<bool>,
    card_uid: Vec<Option<String>>,
    run_id: Vec<Option<String>>,
}

impl<'a> SpanColumns<'a> {
    /// Retain one projected resource envelope for the rows beneath it.
    ///
    /// Returns its index and the payload bytes it adds to each row.
    fn retain_resource(&mut self, envelope: ResourceEnvelope) -> (usize, usize) {
        let retained = (self.resources.len(), envelope.repeated_bytes());
        self.resources.push(envelope);
        retained
    }

    /// Retain one projected scope envelope for the rows beneath it.
    ///
    /// Returns its index and the payload bytes it adds to each row.
    fn retain_scope(&mut self, envelope: ScopeEnvelope) -> (usize, usize) {
        let retained = (self.scopes.len(), envelope.repeated_bytes());
        self.scopes.push(envelope);
        retained
    }

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
        input: &'a TraceInput<'a>,
        span: &'a SpanInput<'a>,
        context: Context,
        card_scope: Option<&[CardUid]>,
        row: u64,
    ) -> Result<usize, Cow<'static, str>> {
        let attributes = input.attributes(span.attributes);
        let events = input.events(span);
        let links = input.links(span);
        let trace_id = trace_id_bytes(span.trace_id)?;
        let span_id = span_id_bytes(span.span_id)?;
        let parent = if span.parent_span_id.is_empty() {
            None
        } else {
            Some(span_id_bytes(span.parent_span_id)?)
        };
        if span.name.is_empty() || span.name.len() > MAX_NAME_BYTES {
            return Err("span name is empty or exceeds the accepted length".into());
        }
        if span.trace_state.len() > MAX_TRACE_STATE_BYTES {
            return Err("span trace_state exceeds the accepted length".into());
        }
        if distinct_keys(attributes) > MAX_SPAN_ATTRIBUTE_KEYS {
            return Err("span declares too many distinct attribute keys".into());
        }
        let start = checked_i64(span.start_time_unix_nano)?;
        let end = checked_i64(span.end_time_unix_nano)?;
        let duration = end
            .checked_sub(start)
            .filter(|duration| *duration >= 0)
            .ok_or("span ends before it starts")?;

        let promotions = GenAiPromotions::extract(attributes)?;
        let correlation = RecordCorrelation::extract(attributes, card_scope)?;
        Self::validate_events(input, span)?;
        Self::validate_links(input, links)?;
        let variant_bytes = self
            .append_variants(input, attributes, events, links)
            .map_err(|failure| failure.reason(row))?;
        let http = HttpPromotions::extract(attributes);
        let exception = Self::exception_promotions(
            events
                .iter()
                .map(|event| (event.name, input.attributes(event.attributes))),
        );
        let payload_bytes = variant_bytes
            + self.push_events(events)
            + self.push_links(links)
            + span.trace_state.len()
            + span.name.len()
            + span.status.map_or(0, |status| status.message.len())
            + http.bytes()
            + exception.bytes()
            + promotions
                .strings
                .iter()
                .flatten()
                .map(|text| text.len())
                .sum::<usize>()
            + correlation.card_uid.as_ref().map_or(0, String::len)
            + correlation.run_id.as_ref().map_or(0, String::len);

        self.trace_id.push(trace_id);
        self.span_id.push(span_id);
        self.parent_span_id.push(parent);
        self.trace_state.push(span.trace_state);
        self.flags.push(span.flags);
        self.name.push(span.name);
        self.kind.push(span.kind);
        self.start_time_unix_nano.push(start);
        self.end_time_unix_nano.push(end);
        self.duration_nano.push(duration);
        self.status_present.push(span.status.is_some());
        self.status_code.push(span.status.map(|status| status.code));
        self.status_message
            .push(span.status.map(|status| status.message));
        self.dropped_attributes_count
            .push(span.dropped_attributes_count);
        self.event_lengths.push(Some(span.events.len()));
        self.dropped_events_count.push(span.dropped_events_count);
        self.link_lengths.push(Some(span.links.len()));
        self.dropped_links_count.push(span.dropped_links_count);

        self.context.push(context);
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
        self.url_path.push(http.url_path);
        self.url_query.push(http.url_query);
        self.exception_type.push(exception.exception_type);
        self.exception_message.push(exception.message);
        self.exception_stacktrace.push(exception.stacktrace);
        self.is_exception.push(
            events
                .iter()
                .any(|event| event.name == EXCEPTION_EVENT_NAME),
        );
        self.card_uid.push(correlation.card_uid);
        self.run_id.push(correlation.run_id);

        self.rows += 1;
        Ok(payload_bytes)
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
    fn validate_events(input: &TraceInput<'_>, span: &SpanInput<'_>) -> Result<(), &'static str> {
        for event in input.events(span) {
            if event.name.is_empty() || event.name.len() > MAX_NAME_BYTES {
                return Err("span event name is empty or exceeds the accepted length");
            }
            if distinct_keys(input.attributes(event.attributes)) > MAX_CHILD_ATTRIBUTE_KEYS {
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
    fn validate_links(input: &TraceInput<'_>, links: &[LinkInput<'_>]) -> Result<(), &'static str> {
        for link in links {
            trace_id_bytes(link.trace_id)?;
            span_id_bytes(link.span_id)?;
            if link.trace_state.len() > MAX_TRACE_STATE_BYTES {
                return Err("span link trace_state exceeds the accepted length");
            }
            if distinct_keys(input.attributes(link.attributes)) > MAX_CHILD_ATTRIBUTE_KEYS {
                return Err("span link declares too many distinct attribute keys");
            }
        }
        Ok(())
    }

    /// Read the `exception.*` conventions from the last of a span's `(name,
    /// attributes)` events named `exception`; every value is null when no such
    /// event exists.
    fn exception_promotions<'e>(
        events: impl DoubleEndedIterator<Item = (&'e str, Attributes<'e>)>,
    ) -> ExceptionPromotions<'e> {
        events
            .rev()
            .find(|(name, _)| *name == EXCEPTION_EVENT_NAME)
            .map(|(_, attributes)| ExceptionPromotions::from_attributes(attributes))
            .unwrap_or_default()
    }

    /// Encode the span's, then each event's, then each link's attribute set
    /// straight into their Variant columns.
    ///
    /// This is the span's last fallible step: every other column is appended
    /// only after it succeeds. On failure every Variant column is rolled back
    /// to where the span started, so a refused span leaves no attribute row.
    /// Returns the encoded bytes appended.
    ///
    /// # Errors
    ///
    /// Returns the [`VariantFailure`] of the first set that cannot be stored.
    fn append_variants(
        &mut self,
        input: &TraceInput<'_>,
        attributes: Attributes<'_>,
        events: &[EventInput<'_>],
        links: &[LinkInput<'_>],
    ) -> Result<usize, VariantFailure> {
        let span_mark = self.attributes.checkpoint();
        let event_mark = self.event_attributes.checkpoint();
        let link_mark = self.link_attributes.checkpoint();
        let scratch = &mut self.variant_scratch;
        let encoded = append_attributes(&mut self.attributes, "attributes", attributes, scratch)
            .and_then(|bytes| {
                events.iter().try_fold(bytes, |bytes, event| {
                    append_attributes(
                        &mut self.event_attributes,
                        "events",
                        input.attributes(event.attributes),
                        scratch,
                    )
                    .map(|event| bytes + event)
                })
            })
            .and_then(|bytes| {
                links.iter().try_fold(bytes, |bytes, link| {
                    append_attributes(
                        &mut self.link_attributes,
                        "links",
                        input.attributes(link.attributes),
                        scratch,
                    )
                    .map(|link| bytes + link)
                })
            });
        if encoded.is_err() {
            self.attributes.rollback(span_mark);
            self.event_attributes.rollback(event_mark);
            self.link_attributes.rollback(link_mark);
        }
        encoded
    }

    /// Append one validated span's ordered events into the flattened storage.
    ///
    /// The events' attribute sets are already in their Variant column.
    /// Returns the event names' payload bytes.
    ///
    /// # Panics
    ///
    /// Panics only if a validated event time exceeds `i64`, which validation
    /// against the span's checked end time rules out.
    fn push_events(&mut self, events: &[EventInput<'a>]) -> usize {
        let mut payload_bytes = 0;
        for event in events {
            self.event_time.push(
                checked_i64(event.time_unix_nano)
                    .expect("a validated event time never exceeds its span's checked end"),
            );
            payload_bytes += event.name.len();
            self.event_name.push(event.name);
            self.event_dropped.push(event.dropped_attributes_count);
        }
        payload_bytes
    }

    /// Append one validated span's ordered links into the flattened storage.
    ///
    /// The links' attribute sets are already in their Variant column.
    /// Returns the links' `trace_state` payload bytes;
    /// fixed-width identifiers are charged with the link's fixed width.
    fn push_links(&mut self, links: &[LinkInput<'a>]) -> usize {
        let mut payload_bytes = 0;
        for link in links {
            self.link_trace_id.push(link.trace_id);
            self.link_span_id.push(link.span_id);
            payload_bytes += link.trace_state.len();
            self.link_trace_state.push(link.trace_state);
            self.link_flags.push(link.flags);
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
                otlp_instant_column(self.event_time),
                utf8_column(&self.event_name),
                self.event_attributes
                    .finish()
                    .map_err(|error| TableError::Internal(error.to_string()))?,
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
                utf8_column(&self.link_trace_state),
                u32_as_i64_column(self.link_flags),
                self.link_attributes
                    .finish()
                    .map_err(|error| TableError::Internal(error.to_string()))?,
                u32_as_i64_column(self.link_dropped),
            ],
            None,
        )
        .map_err(internal)?;

        let resource = || self.context.iter().map(|row| &self.resources[row.resource]);
        let scope = || self.context.iter().map(|row| &self.scopes[row.scope]);
        let resource_text = |pick: fn(&ResourceEnvelope) -> Option<&str>| {
            utf8_opt_column_from(resource().map(pick))
        };
        let scope_text = |pick: fn(&ScopeEnvelope) -> &str| {
            utf8_opt_column_from(scope().map(|row| Some(pick(row))))
        };
        let entity_ref_lengths: Vec<Option<usize>> =
            resource().map(|row| Some(row.entity_refs.len())).collect();

        let mut gen_ai_strings = self.gen_ai_strings.into_iter();
        let mut gen_ai_ints = self.gen_ai_ints.into_iter();
        let mut columns: Vec<ArrayRef> = vec![
            fixed_binary_column(16, &self.trace_id).map_err(internal)?,
            fixed_binary_column(8, &self.span_id).map_err(internal)?,
            fixed_binary_opt_column(8, &self.parent_span_id).map_err(internal)?,
            utf8_column(&self.trace_state),
            u32_as_i64_column(self.flags),
            utf8_column(&self.name),
            i32_column(self.kind),
            otlp_instant_column(self.start_time_unix_nano),
            otlp_instant_column(self.end_time_unix_nano),
            i64_column(self.duration_nano),
            bool_column(self.status_present),
            i32_opt_column(self.status_code),
            utf8_opt_column(&self.status_message),
            self.attributes
                .finish()
                .map_err(|error| TableError::Internal(error.to_string()))?,
            u32_as_i64_column(self.dropped_attributes_count),
            list_column(&SPAN_EVENT_ELEMENT.to_arrow(), events, &self.event_lengths)
                .map_err(internal)?,
            u32_as_i64_column(self.dropped_events_count),
            list_column(&SPAN_LINK_ELEMENT.to_arrow(), links, &self.link_lengths)
                .map_err(internal)?,
            u32_as_i64_column(self.dropped_links_count),
            bool_column(resource().map(|row| row.present).collect()),
            variant_column(resource().map(|row| Some(&row.attributes))),
            u32_as_i64_column(resource().map(|row| row.dropped_attributes_count).collect()),
            resource_text(|row| Some(&row.schema_url)),
            entity_refs_column(
                resource().flat_map(|row| &row.entity_refs),
                &entity_ref_lengths,
            )?,
            bool_column(scope().map(|row| row.present).collect()),
            scope_text(|row| &row.name),
            scope_text(|row| &row.version),
            variant_column(scope().map(|row| Some(&row.attributes))),
            u32_as_i64_column(scope().map(|row| row.dropped_attributes_count).collect()),
            scope_text(|row| &row.schema_url),
            resource_text(|row| row.promotions.service_name.as_deref()),
            utf8_opt_column(&gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(&gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(&gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(&gen_ai_strings.next().unwrap_or_default()),
            i64_opt_column(gen_ai_ints.next().unwrap_or_default()),
            i64_opt_column(gen_ai_ints.next().unwrap_or_default()),
            resource_text(|row| row.promotions.service_version.as_deref()),
            resource_text(|row| row.promotions.deployment_environment.as_deref()),
        ];
        columns.extend(ResourcePromotionColumns::from_rows(
            resource().map(|row| &row.promotions),
        ));
        columns.extend([
            utf8_opt_column(&self.http_request_method),
            utf8_opt_column(&self.http_route),
            i64_opt_column(self.http_response_status_code),
            utf8_opt_column(&self.url_full),
            utf8_opt_column(&self.url_path),
            utf8_opt_column(&self.url_query),
            utf8_opt_column(&self.exception_type),
            utf8_opt_column(&self.exception_message),
            utf8_opt_column(&self.exception_stacktrace),
            bool_column(self.is_exception),
            utf8_opt_column(&self.card_uid),
            utf8_opt_column(&self.run_id),
        ]);

        RecordBatch::try_new(Arc::clone(&SPAN_SCHEMA), columns)
            .map_err(|error| TableError::Internal(format!("canonical span batch: {error}")))
    }
}

/// Refuse a native canonical span batch whose derived columns differ from
/// what [`project_resource_spans`] derives from the same source fields.
///
/// `duration_nano`, the `GenAI`, HTTP, URL, and exception promotions, and
/// `is_exception` are re-derived from each row's times, `attributes`, and
/// `events` with the projection's own extractors, so a native writer cannot
/// store a filter value the projection would not, and a wrongly typed `GenAI`
/// source refuses the batch as it rejects an OTLP span.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] for a span that ends before it
/// starts, a wrongly typed `GenAI` source, a malformed nested column, or the
/// first derived column that differs.
pub(crate) fn refuse_forged_span_derivations(batch: &RecordBatch) -> Result<(), BifrostError> {
    let refused = |detail: String| BifrostError::SchemaParse { detail };
    let column = |name: &str| {
        batch
            .column_by_name(name)
            .ok_or_else(|| refused(format!("canonical batch is missing column {name}")))
    };
    let attributes = column("attributes")?;
    let events = column("events")?
        .as_list_opt::<i32>()
        .ok_or_else(|| refused("events is not a list".to_owned()))?;
    let event_fields = events
        .values()
        .as_struct_opt()
        .ok_or_else(|| refused("events does not hold structs".to_owned()))?;
    let event_names = event_fields
        .column_by_name("name")
        .and_then(|names| names.as_string_opt::<i32>())
        .ok_or_else(|| refused("events carry no name".to_owned()))?;
    let event_attributes = event_fields
        .column_by_name("attributes")
        .ok_or_else(|| refused("events carry no attributes".to_owned()))?;
    let instant = |name: &str| {
        column(name)?
            .as_primitive_opt::<TimestampNanosecondType>()
            .ok_or_else(|| refused(format!("{name} is not an OTLP instant")))
            .cloned()
    };
    let (start, end) = (
        instant("start_time_unix_nano")?,
        instant("end_time_unix_nano")?,
    );

    let rows = batch.num_rows();
    let mut duration = Vec::with_capacity(rows);
    let mut spans = Vec::with_capacity(rows);
    for row in 0..rows {
        duration.push(
            end.value(row)
                .checked_sub(start.value(row))
                .filter(|duration| *duration >= 0)
                .ok_or_else(|| refused(format!("span row {row} ends before it starts")))?,
        );
        let offsets = events.value_offsets();
        spans.push(Span {
            attributes: stored_attributes(attributes.as_ref(), row)?,
            events: (offsets[row]..offsets[row + 1])
                .map(|event| {
                    let event = usize::try_from(event).unwrap_or(usize::MAX);
                    Ok(Event {
                        name: event_names.value(event).to_owned(),
                        attributes: stored_attributes(event_attributes.as_ref(), event)?,
                        ..Event::default()
                    })
                })
                .collect::<Result<_, BifrostError>>()?,
            ..Span::default()
        });
    }
    let mut gen_ai = Vec::with_capacity(rows);
    let mut http = Vec::with_capacity(rows);
    let mut exception = Vec::with_capacity(rows);
    let mut is_exception = Vec::with_capacity(rows);
    for (row, span) in spans.iter().enumerate() {
        gen_ai.push(
            GenAiPromotions::extract((&span.attributes).into())
                .map_err(|reason| refused(format!("span row {row}: {reason}")))?,
        );
        http.push(HttpPromotions::extract((&span.attributes).into()));
        exception.push(SpanColumns::exception_promotions(span.events.iter().map(
            |event| (event.name.as_str(), Attributes::from(&event.attributes)),
        )));
        is_exception.push(
            span.events
                .iter()
                .any(|event| event.name == EXCEPTION_EVENT_NAME),
        );
    }

    let text = |values: Vec<Option<&str>>| utf8_opt_column(&values);
    // Each `GenAI` column is its convention key with `.` spelled `_`.
    let column_of = |key: &str| key.replace('.', "_");
    let mut derived: Vec<(String, ArrayRef)> =
        vec![("duration_nano".to_owned(), i64_column(duration))];
    for (index, key) in GEN_AI_STRING_PROMOTIONS.into_iter().enumerate() {
        let values = gen_ai.iter().map(|row| row.strings[index]).collect();
        derived.push((column_of(key), text(values)));
    }
    for (index, key) in GEN_AI_INT_PROMOTIONS.into_iter().enumerate() {
        let values = gen_ai.iter().map(|row| row.ints[index]).collect();
        derived.push((column_of(key), i64_opt_column(values)));
    }
    derived.extend([
        (
            "http_request_method".to_owned(),
            text(http.iter().map(|row| row.request_method).collect()),
        ),
        (
            "http_route".to_owned(),
            text(http.iter().map(|row| row.route).collect()),
        ),
        (
            "http_response_status_code".to_owned(),
            i64_opt_column(http.iter().map(|row| row.response_status_code).collect()),
        ),
        (
            "url_full".to_owned(),
            text(http.iter().map(|row| row.url_full).collect()),
        ),
        (
            "url_path".to_owned(),
            text(http.iter().map(|row| row.url_path).collect()),
        ),
        (
            "url_query".to_owned(),
            text(http.iter().map(|row| row.url_query).collect()),
        ),
        (
            "exception_type".to_owned(),
            text(exception.iter().map(|row| row.exception_type).collect()),
        ),
        (
            "exception_message".to_owned(),
            text(exception.iter().map(|row| row.message).collect()),
        ),
        (
            "exception_stacktrace".to_owned(),
            text(exception.iter().map(|row| row.stacktrace).collect()),
        ),
        ("is_exception".to_owned(), bool_column(is_exception)),
    ]);
    derived
        .iter()
        .try_for_each(|(name, values)| refuse_forged(batch, name, values))
}

/// Return the canonical user-column schema of `bifrost.traces.spans`.
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
struct GenAiPromotions<'a> {
    /// String promotions in [`GEN_AI_STRING_PROMOTIONS`] order.
    strings: [Option<&'a str>; 4],
    /// Signed-integer promotions in [`GEN_AI_INT_PROMOTIONS`] order.
    ints: [Option<i64>; 2],
}

impl<'a> GenAiPromotions<'a> {
    /// Extract every pinned promotion, rejecting a wrongly typed source.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason when a pinned source attribute is
    /// present but is not the exact protocol type the convention defines.
    fn extract(attributes: Attributes<'a>) -> Result<Self, &'static str> {
        let mut promotions = Self::default();
        for (slot, key) in promotions.strings.iter_mut().zip(GEN_AI_STRING_PROMOTIONS) {
            *slot = match last_attribute(attributes, key) {
                None => None,
                Some(entry) => match entry.value {
                    ValueRef::String(text) => Some(text),
                    _ => return Err("a promoted gen_ai attribute has the wrong type"),
                },
            };
        }
        for (slot, key) in promotions.ints.iter_mut().zip(GEN_AI_INT_PROMOTIONS) {
            *slot = match last_attribute(attributes, key) {
                None => None,
                Some(entry) => match entry.value {
                    ValueRef::Int(number) => Some(number),
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
struct HttpPromotions<'a> {
    /// `http.request.method`.
    request_method: Option<&'a str>,
    /// `http.route`.
    route: Option<&'a str>,
    /// `http.response.status_code`, as a signed integer.
    response_status_code: Option<i64>,
    /// `url.full`.
    url_full: Option<&'a str>,
    /// `url.path`.
    url_path: Option<&'a str>,
    /// `url.query`.
    url_query: Option<&'a str>,
}

impl<'a> HttpPromotions<'a> {
    /// Read the HTTP conventions from one span's attributes, borrowing them.
    fn extract(attributes: Attributes<'a>) -> Self {
        Self {
            request_method: last_string_attribute(attributes, "http.request.method"),
            route: last_string_attribute(attributes, "http.route"),
            response_status_code: promoted_int(attributes, "http.response.status_code"),
            url_full: last_string_attribute(attributes, "url.full"),
            url_path: last_string_attribute(attributes, "url.path"),
            url_query: last_string_attribute(attributes, "url.query"),
        }
    }

    /// Returns the promoted text bytes these values add to one row.
    fn bytes(&self) -> usize {
        [
            self.request_method,
            self.route,
            self.url_full,
            self.url_path,
            self.url_query,
        ]
        .into_iter()
        .flatten()
        .map(str::len)
        .sum()
    }
}
