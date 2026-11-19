//! Borrowed trace input that [`super::project_trace_input`] projects.
//!
//! A trace export reaches projection as one flat [`TraceInput`]: every
//! resource group, scope group, span, event, and link in request order, each
//! group or span naming its children by range. Text and bytes borrow their
//! source. A server decoder fills it straight from the protobuf wire, with
//! attributes in an [`AttributePool`]; [`TraceInput::from_generated`] adapts
//! an already decoded request by borrowing its messages. Projection reads only
//! this type, so both sources are validated and stored by one implementation.

use std::borrow::Cow;
use std::ops::Range;

use wyrd_tonic::otlp::common::v1::{InstrumentationScope, KeyValue};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::ResourceSpans;
use wyrd_tonic::otlp::trace::v1::span::{Event, Link};

use crate::tables::signal::{AttributePool, Attributes, PoolRange};

/// One flattened trace export.
#[derive(Debug, Default)]
pub struct TraceInput<'a> {
    /// Resource groups in request order.
    pub resources: Vec<ResourceInput<'a>>,
    /// Scope groups of every resource, in request order.
    pub scopes: Vec<ScopeInput<'a>>,
    /// Spans of every scope, in request order.
    pub spans: Vec<SpanInput<'a>>,
    /// Events of every span, in request order.
    pub events: Vec<EventInput<'a>>,
    /// Links of every span, in request order.
    pub links: Vec<LinkInput<'a>>,
    /// Attributes of a wire-decoded input; empty for a generated one.
    pub pool: AttributePool<'a>,
}

/// One resource group.
///
/// The resource itself is decoded once per group, so a wire decoder owns it
/// and a generated input borrows it.
#[derive(Debug)]
pub struct ResourceInput<'a> {
    /// The group's resource, when present.
    pub resource: Option<Cow<'a, Resource>>,
    /// The group's schema URL.
    pub schema_url: &'a str,
    /// The group's scope groups in [`TraceInput::scopes`].
    pub scopes: Range<usize>,
}

/// One scope group.
#[derive(Debug)]
pub struct ScopeInput<'a> {
    /// The group's instrumentation scope, when present.
    pub scope: Option<Cow<'a, InstrumentationScope>>,
    /// The group's schema URL.
    pub schema_url: &'a str,
    /// The group's spans in [`TraceInput::spans`].
    pub spans: Range<usize>,
}

/// One attribute list, by source.
#[derive(Debug, Clone, Copy)]
pub enum AttributeList<'a> {
    /// A generated message's entries.
    Generated(&'a [KeyValue]),
    /// Entries of [`TraceInput::pool`].
    Pooled(PoolRange),
}

/// One span's protocol fields.
#[derive(Debug, Clone)]
pub struct SpanInput<'a> {
    /// `trace_id`.
    pub trace_id: &'a [u8],
    /// `span_id`.
    pub span_id: &'a [u8],
    /// `trace_state`.
    pub trace_state: &'a str,
    /// `parent_span_id`.
    pub parent_span_id: &'a [u8],
    /// `flags`.
    pub flags: u32,
    /// `name`.
    pub name: &'a str,
    /// `kind`.
    pub kind: i32,
    /// `start_time_unix_nano`.
    pub start_time_unix_nano: u64,
    /// `end_time_unix_nano`.
    pub end_time_unix_nano: u64,
    /// `attributes`.
    pub attributes: AttributeList<'a>,
    /// `dropped_attributes_count`.
    pub dropped_attributes_count: u32,
    /// The span's events in [`TraceInput::events`].
    pub events: Range<usize>,
    /// `dropped_events_count`.
    pub dropped_events_count: u32,
    /// The span's links in [`TraceInput::links`].
    pub links: Range<usize>,
    /// `dropped_links_count`.
    pub dropped_links_count: u32,
    /// `status`, when present.
    pub status: Option<StatusInput<'a>>,
}

/// One span status.
#[derive(Debug, Clone, Copy)]
pub struct StatusInput<'a> {
    /// `message`.
    pub message: &'a str,
    /// `code`.
    pub code: i32,
}

/// One span event.
#[derive(Debug, Clone, Copy)]
pub struct EventInput<'a> {
    /// `time_unix_nano`.
    pub time_unix_nano: u64,
    /// `name`.
    pub name: &'a str,
    /// `attributes`.
    pub attributes: AttributeList<'a>,
    /// `dropped_attributes_count`.
    pub dropped_attributes_count: u32,
}

/// One span link.
#[derive(Debug, Clone, Copy)]
pub struct LinkInput<'a> {
    /// `trace_id`.
    pub trace_id: &'a [u8],
    /// `span_id`.
    pub span_id: &'a [u8],
    /// `trace_state`.
    pub trace_state: &'a str,
    /// `attributes`.
    pub attributes: AttributeList<'a>,
    /// `dropped_attributes_count`.
    pub dropped_attributes_count: u32,
    /// `flags`.
    pub flags: u32,
}

impl<'a> TraceInput<'a> {
    /// Borrow an already decoded request's resource spans.
    ///
    /// Only the group, span, event, and link vectors are allocated; every
    /// message, string, and attribute list is borrowed.
    #[must_use]
    pub fn from_generated(resource_spans: &'a [ResourceSpans]) -> Self {
        let scopes = || resource_spans.iter().flat_map(|group| &group.scope_spans);
        let spans = || scopes().flat_map(|group| &group.spans);
        let mut input = Self {
            resources: Vec::with_capacity(resource_spans.len()),
            scopes: Vec::with_capacity(scopes().count()),
            spans: Vec::with_capacity(spans().count()),
            events: Vec::with_capacity(spans().map(|span| span.events.len()).sum()),
            links: Vec::with_capacity(spans().map(|span| span.links.len()).sum()),
            pool: AttributePool::default(),
        };
        for group in resource_spans {
            let first_scope = input.scopes.len();
            for scope in &group.scope_spans {
                let first_span = input.spans.len();
                for span in &scope.spans {
                    let first_event = input.events.len();
                    input
                        .events
                        .extend(span.events.iter().map(EventInput::from));
                    let first_link = input.links.len();
                    input.links.extend(span.links.iter().map(LinkInput::from));
                    input.spans.push(SpanInput {
                        trace_id: &span.trace_id,
                        span_id: &span.span_id,
                        trace_state: &span.trace_state,
                        parent_span_id: &span.parent_span_id,
                        flags: span.flags,
                        name: &span.name,
                        kind: span.kind,
                        start_time_unix_nano: span.start_time_unix_nano,
                        end_time_unix_nano: span.end_time_unix_nano,
                        attributes: AttributeList::Generated(&span.attributes),
                        dropped_attributes_count: span.dropped_attributes_count,
                        events: first_event..input.events.len(),
                        dropped_events_count: span.dropped_events_count,
                        links: first_link..input.links.len(),
                        dropped_links_count: span.dropped_links_count,
                        status: span.status.as_ref().map(|status| StatusInput {
                            message: &status.message,
                            code: status.code,
                        }),
                    });
                }
                input.scopes.push(ScopeInput {
                    scope: scope.scope.as_ref().map(Cow::Borrowed),
                    schema_url: &scope.schema_url,
                    spans: first_span..input.spans.len(),
                });
            }
            input.resources.push(ResourceInput {
                resource: group.resource.as_ref().map(Cow::Borrowed),
                schema_url: &group.schema_url,
                scopes: first_scope..input.scopes.len(),
            });
        }
        input
    }

    /// Read one attribute list.
    #[must_use]
    pub fn attributes(&self, list: AttributeList<'a>) -> Attributes<'_> {
        match list {
            AttributeList::Generated(entries) => Attributes::Generated(entries),
            AttributeList::Pooled(range) => self.pool.attributes(range),
        }
    }

    /// One span's events.
    #[must_use]
    pub fn events(&self, span: &SpanInput<'_>) -> &[EventInput<'a>] {
        &self.events[span.events.clone()]
    }

    /// One span's links.
    #[must_use]
    pub fn links(&self, span: &SpanInput<'_>) -> &[LinkInput<'a>] {
        &self.links[span.links.clone()]
    }
}

impl<'a> From<&'a Event> for EventInput<'a> {
    fn from(event: &'a Event) -> Self {
        Self {
            time_unix_nano: event.time_unix_nano,
            name: &event.name,
            attributes: AttributeList::Generated(&event.attributes),
            dropped_attributes_count: event.dropped_attributes_count,
        }
    }
}

impl<'a> From<&'a Link> for LinkInput<'a> {
    fn from(link: &'a Link) -> Self {
        Self {
            trace_id: &link.trace_id,
            span_id: &link.span_id,
            trace_state: &link.trace_state,
            attributes: AttributeList::Generated(&link.attributes),
            dropped_attributes_count: link.dropped_attributes_count,
            flags: link.flags,
        }
    }
}
