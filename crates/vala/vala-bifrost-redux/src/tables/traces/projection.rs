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
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use wyrd_tonic::otlp::common::v1::KeyValue;
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};

use super::spans::{
    RESOURCE_ENTITY_REF_ELEMENT, SPAN_EVENT_ELEMENT, SPAN_FIELDS, SPAN_LINK_ELEMENT,
};
use crate::otlp_contract::IngestOutcome;
use crate::tables::TableError;
use crate::tables::fields::canonical_arrow_fields;
use crate::tables::signal::{
    ResourceEnvelope, ScopeEnvelope, binary_column, bool_column, encode_attributes,
    fixed_binary_column, fixed_binary_opt_column, i32_column, i32_opt_column, i64_opt_column,
    internal, last_attribute, last_string_attribute, list_column, nested_fields, span_id_bytes,
    struct_column, trace_id_bytes, u32_column, u64_column, utf8_column, utf8_opt_column,
};

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

/// Canonical resource attribute the nullable `service_name` column promotes.
const SERVICE_NAME_ATTRIBUTE: &str = "service.name";

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
///
/// # Errors
///
/// Returns [`TableError::Internal`] only when the accepted rows cannot be
/// assembled into the canonical Arrow batch, which indicates a defect in this
/// projection rather than caller input.
pub fn project_resource_spans(
    resource_spans: &[ResourceSpans],
) -> Result<(RecordBatch, IngestOutcome), TableError> {
    let mut columns = SpanColumns::default();
    let mut rejected: i64 = 0;
    let mut rejection_message: Option<String> = None;

    for resource in resource_spans {
        let envelope = ResourceEnvelope::project(resource.resource.as_ref(), &resource.schema_url);
        let resource_defect = validate_resource(resource);
        let service_name = resource
            .resource
            .as_ref()
            .and_then(|value| last_string_attribute(&value.attributes, SERVICE_NAME_ATTRIBUTE));
        for scope in &resource.scope_spans {
            let scope_envelope = ScopeEnvelope::project(scope.scope.as_ref(), &scope.schema_url);
            let scope_defect = resource_defect.or_else(|| validate_scope(scope));
            for span in &scope.spans {
                let outcome = match scope_defect {
                    Some(reason) => Err(reason),
                    None => columns.push(span, &envelope, &scope_envelope, service_name),
                };
                if let Err(reason) = outcome {
                    rejected = rejected.saturating_add(1);
                    rejection_message.get_or_insert_with(|| reason.to_owned());
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
    start_time_unix_nano: Vec<u64>,
    end_time_unix_nano: Vec<u64>,
    duration_nano: Vec<u64>,
    status_present: Vec<bool>,
    status_code: Vec<Option<i32>>,
    status_message: Vec<Option<String>>,
    attributes: Vec<Vec<u8>>,
    dropped_attributes_count: Vec<u32>,
    event_lengths: Vec<Option<usize>>,
    event_time: Vec<u64>,
    event_name: Vec<String>,
    event_attributes: Vec<Vec<u8>>,
    event_dropped: Vec<u32>,
    dropped_events_count: Vec<u32>,
    link_lengths: Vec<Option<usize>>,
    link_trace_id: Vec<Vec<u8>>,
    link_span_id: Vec<Vec<u8>>,
    link_trace_state: Vec<String>,
    link_flags: Vec<u32>,
    link_attributes: Vec<Vec<u8>>,
    link_dropped: Vec<u32>,
    dropped_links_count: Vec<u32>,
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
    service_name: Vec<Option<String>>,
    gen_ai_strings: [Vec<Option<String>>; 4],
    gen_ai_ints: [Vec<Option<i64>>; 2],
}

impl SpanColumns {
    /// Validate one span completely, then append its canonical row.
    ///
    /// # Errors
    ///
    /// Returns the stable rejection reason for an invalid identifier, an
    /// out-of-range timestamp pair, an over-long or empty name, an attribute
    /// cardinality breach, an invalid event or link, or a `GenAI` promotion whose
    /// source attribute carries the wrong protocol type. Nothing is appended
    /// when an error is returned.
    fn push(
        &mut self,
        span: &Span,
        resource: &ResourceEnvelope,
        scope: &ScopeEnvelope,
        service_name: Option<&str>,
    ) -> Result<(), &'static str> {
        let trace_id = trace_id_bytes(&span.trace_id)?;
        let span_id = span_id_bytes(&span.span_id)?;
        let parent = if span.parent_span_id.is_empty() {
            None
        } else {
            Some(span_id_bytes(&span.parent_span_id)?.to_vec())
        };
        if span.name.is_empty() || span.name.len() > MAX_NAME_BYTES {
            return Err("span name is empty or exceeds the accepted length");
        }
        if span.trace_state.len() > MAX_TRACE_STATE_BYTES {
            return Err("span trace_state exceeds the accepted length");
        }
        if unique_keys(&span.attributes) > MAX_SPAN_ATTRIBUTE_KEYS {
            return Err("span declares too many distinct attribute keys");
        }
        let duration = span
            .end_time_unix_nano
            .checked_sub(span.start_time_unix_nano)
            .ok_or("span ends before it starts")?;

        let promotions = GenAiPromotions::extract(&span.attributes)?;
        Self::validate_events(span)?;
        Self::validate_links(span)?;
        self.push_events(span);
        self.push_links(span);

        self.trace_id.push(trace_id.to_vec());
        self.span_id.push(span_id.to_vec());
        self.parent_span_id.push(parent);
        self.trace_state.push(span.trace_state.clone());
        self.flags.push(span.flags);
        self.name.push(span.name.clone());
        self.kind.push(span.kind);
        self.start_time_unix_nano.push(span.start_time_unix_nano);
        self.end_time_unix_nano.push(span.end_time_unix_nano);
        self.duration_nano.push(duration);
        self.status_present.push(span.status.is_some());
        self.status_code
            .push(span.status.as_ref().map(|status| status.code));
        self.status_message
            .push(span.status.as_ref().map(|status| status.message.clone()));
        self.attributes.push(encode_attributes(&span.attributes));
        self.dropped_attributes_count
            .push(span.dropped_attributes_count);
        self.event_lengths.push(Some(span.events.len()));
        self.dropped_events_count.push(span.dropped_events_count);
        self.link_lengths.push(Some(span.links.len()));
        self.dropped_links_count.push(span.dropped_links_count);

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

        self.service_name
            .push(service_name.map(std::borrow::ToOwned::to_owned));
        for (column, value) in self.gen_ai_strings.iter_mut().zip(promotions.strings) {
            column.push(value);
        }
        for (column, value) in self.gen_ai_ints.iter_mut().zip(promotions.ints) {
            column.push(value);
        }

        self.rows += 1;
        Ok(())
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

    /// Append one validated span's ordered events into the flattened storage.
    fn push_events(&mut self, span: &Span) {
        for event in &span.events {
            self.event_time.push(event.time_unix_nano);
            self.event_name.push(event.name.clone());
            self.event_attributes
                .push(encode_attributes(&event.attributes));
            self.event_dropped.push(event.dropped_attributes_count);
        }
    }

    /// Append one validated span's ordered links into the flattened storage.
    fn push_links(&mut self, span: &Span) {
        for link in &span.links {
            self.link_trace_id.push(link.trace_id.clone());
            self.link_span_id.push(link.span_id.clone());
            self.link_trace_state.push(link.trace_state.clone());
            self.link_flags.push(link.flags);
            self.link_attributes
                .push(encode_attributes(&link.attributes));
            self.link_dropped.push(link.dropped_attributes_count);
        }
    }

    /// Assemble the accepted rows into the canonical span batch.
    ///
    /// Column order comes from [`SPAN_FIELDS`], so this method and the ledger
    /// cannot drift apart without the assembly failing.
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
                u64_column(self.event_time),
                utf8_column(self.event_name),
                binary_column(&self.event_attributes),
                u32_column(self.event_dropped),
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
                u32_column(self.link_flags),
                binary_column(&self.link_attributes),
                u32_column(self.link_dropped),
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
            u32_column(self.flags),
            utf8_column(self.name),
            i32_column(self.kind),
            u64_column(self.start_time_unix_nano),
            u64_column(self.end_time_unix_nano),
            u64_column(self.duration_nano),
            bool_column(self.status_present),
            i32_opt_column(self.status_code),
            utf8_opt_column(self.status_message),
            binary_column(&self.attributes),
            u32_column(self.dropped_attributes_count),
            list_column(&SPAN_EVENT_ELEMENT.to_arrow(), events, &self.event_lengths)
                .map_err(internal)?,
            u32_column(self.dropped_events_count),
            list_column(&SPAN_LINK_ELEMENT.to_arrow(), links, &self.link_lengths)
                .map_err(internal)?,
            u32_column(self.dropped_links_count),
            bool_column(self.resource_present),
            binary_column(&self.resource_attributes),
            u32_column(self.resource_dropped_attributes_count),
            utf8_column(self.resource_schema_url),
            list_column(
                &RESOURCE_ENTITY_REF_ELEMENT.to_arrow(),
                binary_column(&self.entity_refs),
                &self.entity_ref_lengths,
            )
            .map_err(internal)?,
            bool_column(self.scope_present),
            utf8_column(self.scope_name),
            utf8_column(self.scope_version),
            binary_column(&self.scope_attributes),
            u32_column(self.scope_dropped_attributes_count),
            utf8_column(self.scope_schema_url),
            utf8_opt_column(self.service_name),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            utf8_opt_column(gen_ai_strings.next().unwrap_or_default()),
            i64_opt_column(gen_ai_ints.next().unwrap_or_default()),
            i64_opt_column(gen_ai_ints.next().unwrap_or_default()),
        ];

        RecordBatch::try_new(canonical_span_schema(), columns)
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
