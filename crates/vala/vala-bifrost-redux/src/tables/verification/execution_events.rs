//! `bifrost.verification.execution_events` — the progress history of queued
//! Verifier executions.
//!
//! Owns the table's authored schema and physical layout only; the server
//! commits each event with the execution transition it records and projects
//! it here through Scribe.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{int64, ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.execution_events` — one row per committed
/// execution, invocation, or publication transition.
///
/// `event_id` is the server-assigned identity of the event and `sequence`
/// its position in its tenant's committed history, so ordering never rests
/// on `occurred_at`. `event` is `execution` for an execution status change,
/// with `status` the new status and `reason` its error code; `invocation`
/// for a started (`status` `started`) or settled invocation attempt, with
/// `status` the step status or requirement outcome and `reason` the
/// recorded reason; `invocation_abandoned` for an attempt whose unfinished
/// checkpoint was forgotten; and `publication` for an accepted publication,
/// with `publication_name`, `publication_kind`, its total `size_bytes`,
/// `file_count`, a file's `sha256`, and a JSON publication's bounded
/// [`VALUE`]. Producer identity is `step_id`, `item_index`,
/// `iteration_index`, and `attempt`. Bytes never appear here: files stay in
/// verification File storage, named by digest. The managed columns attribute
/// the row to the execution's subject and actor.
pub struct ExecutionEventsTable;

/// Variant column holding a JSON publication's value.
pub const VALUE: &str = "value";

impl DomainTable for ExecutionEventsTable {
    /// Lives in `bifrost.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `bifrost.verification.execution_events` FQN.
    const NAME: &'static str = "execution_events";
    /// Published JSON values make projecting this table require the elevated
    /// payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The value column gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[VALUE];

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("event_id", false),
            utf8("execution_id", false),
            int64("sequence", false),
            utf8("event", false),
            utf8("status", true),
            utf8("verifier_card_uid", false),
            utf8("step_id", true),
            int64("item_index", true),
            int64("iteration_index", true),
            int64("attempt", true),
            utf8("publication_name", true),
            utf8("publication_kind", true),
            int64("size_bytes", true),
            int64("file_count", true),
            utf8("sha256", true),
            utf8("reason", true),
            variant_field(VALUE, true),
            ts_us_utc("occurred_at", false),
        ])
    }

    /// Daily partitions on `occurred_at`, attribution-sorted, Bloom-pruned on
    /// the shared attribution set, the execution an inspector follows, and
    /// its Verifier.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(
            daily_layout,
            "occurred_at",
            &["event_id", "execution_id", "verifier_card_uid"],
        )
    }
}
