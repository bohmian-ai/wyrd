//! `vala.eval.result_items` — the per-task outcome detail of one Eval result.
//!
//! Owns the table's authored schema, sensitive payload columns, and physical
//! layout only; the Eval engine writes it.

use arrow::datatypes::Field;

use crate::tables::fields::{boolean, int32, int64, ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.eval.result_items` — one row per task outcome of one Eval run.
///
/// Both executed and skipped outcomes appear here, which is why every
/// `Ran`-only column is nullable and `skip_reason`/`upstream_task_id` are
/// populated only for a skip. The workflow summary in
/// `vala.verification.results` counts executed tasks alone, so a skipped task
/// is visible only in this table. Rows join their parent result on
/// (`data_tenant_id`, `result_id`). `actual`, `expected`, and `message` carry
/// captured task payloads and are classified sensitive.
pub struct ResultItemsTable;

impl DomainTable for ResultItemsTable {
    /// Lives in `vala.eval`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "eval";
    /// Table segment of the fixed `vala.eval.result_items` FQN.
    const NAME: &'static str = "result_items";
    /// The server appends and stamps the managed Verifier `run_id`, Verifier `card_uid`, and `principal_id`.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// Captured task payloads make projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The captured task payload columns gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["actual", "expected", "message"];

    /// Authored columns in order; `Ran`-only and skip-only columns are nullable because both outcomes share one row shape.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("owner_card_uid", true),
            utf8("subject_card_uid", false),
            utf8("binding_id", true),
            utf8("source_record_id", false),
            utf8("task_id", false),
            utf8("outcome_kind", false),
            boolean("passed", true),
            utf8("actual", true),
            utf8("expected", true),
            utf8("operator", true),
            utf8("message", true),
            int32("stage", true),
            ts_us_utc("started_at", true),
            int64("duration_ms", true),
            utf8("skip_reason", true),
            utf8("upstream_task_id", true),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on `result_id` for the parent-result join.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(vec![sort_desc(WYRD_EVENT_TIME)], &["result_id"])
    }
}
