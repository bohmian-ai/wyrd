//! `bifrost.verification.execution_events` — the accepted recoverable state
//! of every Verifier execution, one row per accepted revision.
//!
//! Owns the table's authored schema and physical layout only; the server
//! writes each revision through Scribe under a batch id derived from its
//! execution and revision.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{int64, ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.execution_events` — one row per accepted revision
/// of one execution's recoverable state.
///
/// `execution_id` is the state key and `sequence` its revision. Each
/// revision is written under a batch id derived from both, so the first
/// accepted batch keeps the revision, a second writer's different rows for it
/// are refused as a batch conflict, and its durable batch fence names the
/// revision as accepted whether or not a query can see the row yet.
/// `event` is `checkpoint`, `status` the execution status the revision
/// records, and [`VALUE`] the complete versioned logical state: pinned
/// inputs, attempts, settlements, accepted publications with their exact
/// File descriptors, and the selected terminal outcome. Producer, publication,
/// and size columns stay null. `event_id` identifies the revision. Bytes never
/// appear here: files stay in verification File storage, named by digest.
/// The managed columns attribute the row to the execution's subject and
/// actor.
pub struct ExecutionEventsTable;

/// Variant column holding the complete versioned execution state.
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
    /// A revision is accepted only once its batch fence commits.
    const FENCED_ONLY: bool = true;

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
