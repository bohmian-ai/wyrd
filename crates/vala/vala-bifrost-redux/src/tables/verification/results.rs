//! `bifrost.verification.results` — one row per produced Verifier result.
//!
//! Owns the canonical payload-free result summary and the Bloom columns a
//! result lookup by id, execution, Verifier, or binding prunes on.

use arrow::datatypes::Field;

use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{DomainTable, attributed_layout, daily_layout, with_shared_dimensions};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.results` — one row per produced Verifier result.
///
/// Every Verifier writes this one surface whatever its checks, so a dashboard
/// reads one result contract. Each result carries its own `result_id`, and
/// `execution_id` correlates it with its execution. A queued result is staged
/// before its execution completes, so an execution that is later abandoned or
/// recovered can leave a row that is not its outcome; the queue's selected
/// `result_id` on the completed execution is authoritative.
/// The row is a payload-free summary: the verdict, status, timing, and
/// identities. The result's `requirements` and `steps` live in
/// `bifrost.verification.execution_outputs` under the same `result_id`, and
/// its evidence in `bifrost.verification.execution_inputs` under its
/// `execution_id`.
/// `execution_id` is the Verifier execution; the managed `run_id` is the
/// application Run judged, null when there is none, as for scheduled drift;
/// the managed `subject_card_uid` is the Card verified. The Verifier Card is
/// the `verifier_card_uid` payload column; the binding owner is the managed
/// `workload_card_uid` and `binding_id` the binding, both null for an
/// execution with no binding; a caller execution records its authenticated
/// owner Service instead, when one was available. `group_id` is the caller's
/// group, null when ungrouped. The shared `deployment_environment` dimension
/// is the correlation tag the execution was accepted with, null when it
/// carried none.
/// The managed `sub_principal_id`/`act_principal_id` pair records whom the
/// execution served and who executed it.
pub struct ResultsTable;

impl DomainTable for ResultsTable {
    /// Lives in `bifrost.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `bifrost.verification.results` FQN.
    const NAME: &'static str = "results";

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("result_id", false),
            utf8("execution_id", false),
            utf8("execution_status", false),
            utf8("verdict", false),
            utf8("verifier_version", false),
            utf8("verifier_card_uid", false),
            utf8("binding_id", true),
            utf8("group_id", true),
            utf8("trigger_identity", true),
            ts_us_utc("window_start", true),
            ts_us_utc("window_end", true),
            ts_us_utc("started_at", false),
            ts_us_utc("ended_at", false),
        ])
    }

    /// Daily partitions on `ended_at`, attribution-sorted, Bloom-pruned on the
    /// shared attribution set and the result, execution, Verifier, binding, and
    /// group lookups.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(
            daily_layout,
            "ended_at",
            &[
                "result_id",
                "execution_id",
                "verifier_card_uid",
                "binding_id",
                "group_id",
            ],
        )
    }
}
