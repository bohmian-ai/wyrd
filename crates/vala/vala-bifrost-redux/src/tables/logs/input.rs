//! Borrowed log input that [`super::project_log_input`] projects.
//!
//! A log export reaches projection as one flat [`LogInput`]: every resource
//! group, scope group, and log record in request order, each group naming its
//! children by range. Text and bytes borrow their source. A server decoder
//! fills it straight from the protobuf wire, with attributes and bodies in an
//! [`AttributePool`]; [`LogInput::from_generated`] adapts an already decoded
//! request by borrowing its messages. Projection reads only this type, so both
//! sources are validated and stored by one implementation.

use std::borrow::Cow;

use wyrd_tonic::otlp::common::v1::AnyValue;
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs};

use crate::tables::signal::{
    AttributeList, AttributePool, Attributes, ResourceInput, ScopeInput, ValueRef,
};

/// One flattened log export.
#[derive(Debug, Default)]
pub struct LogInput<'a> {
    /// Resource groups in request order.
    pub resources: Vec<ResourceInput<'a>>,
    /// Scope groups of every resource, in request order.
    pub scopes: Vec<ScopeInput<'a>>,
    /// Log records of every scope, in request order.
    pub records: Vec<RecordInput<'a>>,
    /// Attributes and bodies of a wire-decoded input; empty for a generated
    /// one.
    pub pool: AttributePool<'a>,
}

/// One log record body, by source.
#[derive(Debug, Clone, Copy)]
pub enum BodyInput<'a> {
    /// A generated message's body.
    Generated(&'a AnyValue),
    /// The value at this position of [`AttributePool::values`].
    Pooled(usize),
}

/// One log record's protocol fields.
#[derive(Debug, Clone, Copy)]
pub struct RecordInput<'a> {
    /// `time_unix_nano`.
    pub time_unix_nano: u64,
    /// `observed_time_unix_nano`.
    pub observed_time_unix_nano: u64,
    /// `severity_number`.
    pub severity_number: i32,
    /// `severity_text`.
    pub severity_text: &'a str,
    /// `body`, when present; a present but unset body is a Variant null.
    pub body: Option<BodyInput<'a>>,
    /// `attributes`.
    pub attributes: AttributeList<'a>,
    /// `dropped_attributes_count`.
    pub dropped_attributes_count: u32,
    /// `flags`.
    pub flags: u32,
    /// `trace_id`.
    pub trace_id: &'a [u8],
    /// `span_id`.
    pub span_id: &'a [u8],
    /// `event_name`.
    pub event_name: &'a str,
}

impl<'a> LogInput<'a> {
    /// Borrow an already decoded request's resource logs.
    ///
    /// Only the group and record vectors are allocated; every message, string,
    /// and attribute list is borrowed.
    #[must_use]
    pub fn from_generated(resource_logs: &'a [ResourceLogs]) -> Self {
        let scopes = || resource_logs.iter().flat_map(|group| &group.scope_logs);
        let mut input = Self {
            resources: Vec::with_capacity(resource_logs.len()),
            scopes: Vec::with_capacity(scopes().count()),
            records: Vec::with_capacity(scopes().map(|scope| scope.log_records.len()).sum()),
            pool: AttributePool::default(),
        };
        for group in resource_logs {
            let first_scope = input.scopes.len();
            for scope in &group.scope_logs {
                let first_record = input.records.len();
                input
                    .records
                    .extend(scope.log_records.iter().map(RecordInput::from));
                input.scopes.push(ScopeInput {
                    scope: scope.scope.as_ref().map(Cow::Borrowed),
                    schema_url: &scope.schema_url,
                    records: first_record..input.records.len(),
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
        self.pool.list(list)
    }

    /// Read one record body.
    #[must_use]
    pub fn body(&self, body: BodyInput<'a>) -> ValueRef<'_> {
        match body {
            BodyInput::Generated(value) => ValueRef::of(value),
            BodyInput::Pooled(index) => self.pool.value(index),
        }
    }
}

impl<'a> From<&'a LogRecord> for RecordInput<'a> {
    fn from(record: &'a LogRecord) -> Self {
        Self {
            time_unix_nano: record.time_unix_nano,
            observed_time_unix_nano: record.observed_time_unix_nano,
            severity_number: record.severity_number,
            severity_text: &record.severity_text,
            body: record.body.as_ref().map(BodyInput::Generated),
            attributes: AttributeList::Generated(&record.attributes),
            dropped_attributes_count: record.dropped_attributes_count,
            flags: record.flags,
            trace_id: &record.trace_id,
            span_id: &record.span_id,
            event_name: &record.event_name,
        }
    }
}
