//! `bifrost.verification.execution_inputs` — the one input record of a
//! Verifier execution.
//!
//! Owns the table's authored schema and physical layout only; the server
//! stages its rows through Scribe and reads them back to run queued work.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{boolean, ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.execution_inputs` — one row per Verifier execution's inputs.
///
/// `origin` is how the execution was created: `direct`, `submitted`, `group`,
/// or `schedule`. `inputs` holds a `direct` or `submitted` execution's named
/// input declarations as one Variant, and a `group` or `schedule`
/// execution's internal selector of the source records it reads; uploaded
/// file bytes stay in tenant object storage and are named only by digest.
/// `queue_execution` is true for every queued execution, submitted, group,
/// or schedule, whose acknowledged row the server hands to the batched
/// queue writer; a direct execution records its inputs with it false so it
/// can never queue work. `activation` holds a submitted execution's accepted request context
/// — exact Verifier, route overrides, idempotency key and request hash, and
/// the accepted authority — and is null for every other execution. `recorded_at` is
/// the server-recorded input time, distinct from execution start/completion
/// and from sample window time. The managed `workload_card_uid` is the
/// binding owner or authenticated caller's owner Service, null when none was
/// available, and `group_id` the caller's group, null when ungrouped; grouped
/// bindings discover source rows by both. The shared `deployment_environment`
/// dimension is the correlation tag the execution was accepted with, null
/// when it carried none. The managed `run_id` is the application Run, null
/// when there is none; the managed `subject_card_uid` is the subject verified.
pub struct ExecutionInputsTable;

/// Variant column holding the named input declarations.
pub const INPUTS: &str = "inputs";

/// Variant column holding a queued execution's accepted request context.
pub const ACTIVATION: &str = "activation";

impl DomainTable for ExecutionInputsTable {
    /// Lives in `bifrost.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `bifrost.verification.execution_inputs` FQN.
    const NAME: &'static str = "execution_inputs";
    /// Raw input values make projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The input and request-context columns gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[INPUTS, ACTIVATION];

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("execution_id", false),
            utf8("verifier_card_uid", false),
            utf8("binding_id", true),
            utf8("group_id", true),
            utf8("origin", false),
            boolean("queue_execution", false),
            variant_field(INPUTS, false),
            variant_field(ACTIVATION, true),
            ts_us_utc("recorded_at", false),
        ])
    }

    /// Daily partitions on `recorded_at`, attribution-sorted, Bloom-pruned on the
    /// shared attribution set, the execution a runner reads back, and the
    /// Verifier, binding, and group a grouped binding discovers.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(
            daily_layout,
            "recorded_at",
            &[
                "execution_id",
                "verifier_card_uid",
                "binding_id",
                "group_id",
            ],
        )
    }
}
