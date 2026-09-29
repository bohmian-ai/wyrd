//! `vala.eval.observations` — the fixed Eval input table.
//!
//! Owns the authored column sequence the SDK's Eval projection must match
//! exactly at `start_bifrost`, plus the table's sensitivity and layout.

use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.eval.observations` — one row per committed Eval input record.
///
/// A committed row is what later activates every matching `observations_ready`
/// binding, so the raw input is stored once and attributed to its observed
/// subject through the managed `card_uid`; it is never duplicated per binding
/// and carries no Verifier or binding identity. `trace_id` and `span_id` use
/// the same fixed-width binary identities as `vala.traces.spans` so an Eval
/// record joins directly to the span that produced it. `context` and `media`
/// are canonical JSON text and are classified sensitive, so projecting them
/// requires the elevated payload permission.
pub struct ObservationsTable;

impl DomainTable for ObservationsTable {
    /// Lives in `vala.eval`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "eval";
    /// Table segment of the fixed `vala.eval.observations` FQN.
    const NAME: &'static str = "observations";
    /// The server appends and stamps the managed `run_id`, observed `card_uid`, and `principal_id`.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// Raw Eval input makes projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The raw input columns gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["context", "media"];

    /// Authored columns in the exact order the SDK Eval projection must match at `start_bifrost`.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            utf8("context", false),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            utf8("media", true),
        ]
    }

    /// Daily partitions sorted newest first; rows are read by record, not by a pruned key.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(vec![sort_desc(WYRD_EVENT_TIME)], &[])
    }
}
