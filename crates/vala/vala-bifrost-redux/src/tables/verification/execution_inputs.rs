//! `bifrost.verification.execution_inputs` — the pinned inputs of one Verifier
//! execution.
//!
//! Owns the table's authored schema and physical layout only; the server
//! stages its rows through Scribe and reads them back to run queued work.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{boolean, ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.execution_inputs` — one row per Verifier execution's inputs.
///
/// `inputs` holds the execution's named, pinned input declarations as one
/// Variant; uploaded file bytes stay in tenant object storage and are named
/// only by digest. `queue_execution` is true only for a submitted execution,
/// whose acknowledged row the server hands to the batched queue writer; a
/// direct execution records its inputs with it false so it can never queue
/// work. `activation` holds a queued execution's accepted request context —
/// exact Verifier, route overrides, idempotency key and request hash, and the
/// accepted authority — and is null for a direct execution. `recorded_at` is
/// the server-recorded input time, distinct from execution start/completion
/// and from sample window time. `owner_card_uid` is the authenticated
/// caller's owner Service, null when none was available, and `group_id` the
/// caller's group, null when ungrouped; grouped bindings discover source rows
/// by both. `deployment_environment` is the correlation tag the execution
/// was accepted with, null when it carried none. The managed `run_id` is the application Run,
/// null when there is none; the managed `card_uid` is the subject verified.
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
    /// The server appends and stamps the managed application `run_id`, subject `card_uid`, and subject/actor pair.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// Raw input values make projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The input and request-context columns gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[INPUTS, ACTIVATION];

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("execution_id", false),
            utf8("owner_card_uid", true),
            utf8("verifier_card_uid", false),
            utf8("binding_id", true),
            utf8("group_id", true),
            utf8("deployment_environment", true),
            boolean("queue_execution", false),
            variant_field(INPUTS, false),
            variant_field(ACTIVATION, true),
            ts_us_utc("recorded_at", false),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on the execution a
    /// runner reads back and the group a grouped binding discovers.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            "recorded_at",
            vec![sort_desc("recorded_at")],
            &[
                "run_id",
                "card_uid",
                "sub_principal_id",
                "act_principal_id",
                "execution_id",
                "group_id",
            ],
        )
    }
}
