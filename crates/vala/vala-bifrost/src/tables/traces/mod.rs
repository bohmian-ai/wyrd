use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct SpansTable;

impl DomainTable for SpansTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "spans";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::TRACES_SPANS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["attributes"];

    fn schema() -> SchemaRef {
        generated::traces_spans_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "data_tenant_id".into(),
                ascending: true,
                nulls_first: false,
            },
            SortKey {
                column: "trace_id".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "spans_trace_id_lookup".into(),
                columns: vec!["trace_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "spans_service_bloom".into(),
                columns: vec!["service_name".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        Some(EntityBoundsMapping {
            entity_kind: "trace".into(),
            entity_id_column: "trace_id".into(),
        })
    }
}

pub struct EventsTable;

impl DomainTable for EventsTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "events";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::TRACES_EVENTS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["attributes"];

    fn schema() -> SchemaRef {
        generated::traces_events_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "trace_id".into(),
                ascending: true,
                nulls_first: false,
            },
            SortKey {
                column: "span_id".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![DeclaredIndex {
            name: "events_trace_id_lookup".into(),
            columns: vec!["trace_id".into()],
            kind: IndexKind::BloomFilter,
        }]
    }
}

pub struct LinksTable;

impl DomainTable for LinksTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "links";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::TRACES_LINKS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["attributes"];

    fn schema() -> SchemaRef {
        generated::traces_links_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "trace_id".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "links_trace_id_lookup".into(),
                columns: vec!["trace_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "links_linked_trace_id_lookup".into(),
                columns: vec!["linked_trace_id".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}
