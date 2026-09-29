//! `vala.verification.results` — one row per completed Verifier run.
//!
//! Owns the shared verdict schema, its sensitive `details` payload, and the
//! Bloom columns a result lookup by id, subject, or binding prunes on.

use arrow::datatypes::Field;

use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.verification.results` — exactly one row per completed Verifier run.
///
/// Drift and Eval share this table so a dashboard reads one verdict surface
/// regardless of implementation; `implementation` selects which detail table
/// and which `details` payload shape applies. Only `completed` runs appear:
/// errored, timed-out, and still-running executions have no row here, so the
/// presence of a row is itself the statement that the run produced a verdict.
/// `details` is the one implementation-specific summary — a `DriftReport` or an
/// `EvalWorkflowSummary` — and is null only for a completed Drift execution
/// that could not score valid input, which is recorded rather than fabricated.
/// The managed `run_id` is the Verifier run and the managed `card_uid` is the
/// Verifier Card; the verified subject and binding owner are separate payload
/// columns, both null for a direct Verifier run with no binding.
pub struct ResultsTable;

impl DomainTable for ResultsTable {
    /// Lives in `vala.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `vala.verification.results` FQN.
    const NAME: &'static str = "results";
    /// The server appends and stamps the managed Verifier `run_id`, Verifier `card_uid`, and `principal_id`.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// The `details` summary makes projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The implementation-specific summary column gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["details"];

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("implementation", false),
            utf8("execution_status", false),
            utf8("verdict", false),
            utf8("verifier_version", false),
            utf8("owner_card_uid", true),
            utf8("subject_card_uid", false),
            utf8("binding_id", true),
            utf8("trigger_identity", true),
            utf8("source_record_id", true),
            ts_us_utc("window_start", true),
            ts_us_utc("window_end", true),
            ts_us_utc("started_at", false),
            ts_us_utc("ended_at", false),
            utf8("details", true),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on result id, subject, and binding lookups.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            vec![sort_desc(WYRD_EVENT_TIME)],
            &["result_id", "subject_card_uid", "binding_id"],
        )
    }
}
