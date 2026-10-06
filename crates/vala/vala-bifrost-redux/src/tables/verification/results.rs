//! `vala.verification.results` — one row per completed Verifier run.
//!
//! Owns the shared verdict schema, its sensitive `drift_report` and
//! `eval_summary` Struct payloads, and the Bloom columns a result lookup by id,
//! subject, or binding prunes on.

use arrow::datatypes::{DataType, Field, Fields};

use wyrd_queue::variant::variant_field;

use crate::tables::fields::{float64, int32, int64, ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.verification.results` — exactly one row per completed Verifier run.
///
/// Drift and Eval share this table so a dashboard reads one verdict surface
/// regardless of implementation; `implementation` selects which detail table
/// and which summary column applies. Only `completed` runs appear: errored,
/// timed-out, and still-running executions have no row here, so the presence
/// of a row is itself the statement that the run produced a verdict.
/// A scored Drift result sets only [`DRIFT_REPORT`], the typed `DriftReport`;
/// a scored Eval result sets only [`EVAL_SUMMARY`], the typed
/// `EvalWorkflowSummary`; a completed Drift execution that could not score
/// valid input sets neither, which is recorded rather than fabricated.
/// The managed `run_id` is the Verifier run and the managed `card_uid` is the
/// Verifier Card; the verified subject and binding owner are separate payload
/// columns, both null for a direct Verifier run with no binding.
pub struct ResultsTable;

/// Typed `DriftReport` column: method, open per-feature reports, and verdict.
pub const DRIFT_REPORT: &str = "drift_report";

/// Typed `EvalWorkflowSummary` column: executed-task counts and duration.
pub const EVAL_SUMMARY: &str = "eval_summary";

impl ResultsTable {
    /// Children of [`DRIFT_REPORT`] in persisted order.
    ///
    /// `method` and `verdict` hold the owning enums' serialized names;
    /// `features` is a Variant because feature names are open. Every child is
    /// nullable so a Parquet read keeps the null an absent report gives each
    /// child: a required leaf would read back padded values, which `get_field`
    /// exposes. The table validator refuses a partly present report.
    #[must_use]
    pub fn drift_report_fields() -> Fields {
        Fields::from(vec![
            utf8("method", true),
            variant_field("features", true),
            utf8("verdict", true),
        ])
    }

    /// Children of [`EVAL_SUMMARY`] in persisted order.
    ///
    /// Every child is nullable for the reason given on
    /// [`Self::drift_report_fields`].
    #[must_use]
    pub fn eval_summary_fields() -> Fields {
        Fields::from(vec![
            int32("total_tasks", true),
            int32("passed_tasks", true),
            int32("failed_tasks", true),
            float64("pass_rate", true),
            int64("duration_ms", true),
        ])
    }
}

impl DomainTable for ResultsTable {
    /// Lives in `vala.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `vala.verification.results` FQN.
    const NAME: &'static str = "results";
    /// The server appends and stamps the managed Verifier `run_id`, Verifier `card_uid`, and `principal_id`.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// The implementation summaries make projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The implementation-specific summary columns gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[DRIFT_REPORT, EVAL_SUMMARY];
    /// A report or summary is written whole or not at all.
    const WHOLE_STRUCTS: &'static [&'static str] = &[DRIFT_REPORT, EVAL_SUMMARY];

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
            Field::new(
                DRIFT_REPORT,
                DataType::Struct(Self::drift_report_fields()),
                true,
            ),
            Field::new(
                EVAL_SUMMARY,
                DataType::Struct(Self::eval_summary_fields()),
                true,
            ),
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
