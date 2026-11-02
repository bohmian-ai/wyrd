//! `bifrost.verification.execution_outputs` — the detailed judgment of one
//! Verifier result.
//!
//! Owns the table's authored schema and physical layout only; the server
//! stages its row with its result summary through Scribe and reads it back to
//! assemble a full result.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.execution_outputs` — one row per Verifier result's
/// detailed judgment.
///
/// `result_id` is the summary row in `bifrost.verification.results` this
/// detail belongs to and `execution_id` its execution. [`OUTPUT`] holds
/// exactly the result's `requirements` and `steps` — scores, reasons, computed
/// actual values, citations, check and provenance detail, custom data, drift
/// per-feature detail, and captured step outputs — as one Variant; never the
/// evidence map, which `bifrost.verification.execution_inputs` holds, nor a
/// second verdict. `ended_at` is the result's completion time. The managed
/// columns attribute the row like its summary.
pub struct ExecutionOutputsTable;

/// Variant column holding a result's `requirements` and `steps`.
pub const OUTPUT: &str = "output";

impl DomainTable for ExecutionOutputsTable {
    /// Lives in `bifrost.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `bifrost.verification.execution_outputs` FQN.
    const NAME: &'static str = "execution_outputs";
    /// Open actual values, step outputs, and citations make projecting this
    /// table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The output column gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[OUTPUT];

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("result_id", false),
            utf8("execution_id", false),
            ts_us_utc("ended_at", false),
            variant_field(OUTPUT, false),
        ])
    }

    /// Daily partitions on `ended_at`, attribution-sorted, Bloom-pruned on the
    /// shared attribution set and the result and execution a read joins on.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(daily_layout, "ended_at", &["result_id", "execution_id"])
    }
}
