//! `vala.verification.results` — one row per produced Verifier result payload.
//!
//! Owns the canonical result schema, its sensitive `outcome` Variant, and the
//! Bloom columns a result lookup by id, execution, Verifier, or binding prunes
//! on.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `vala.verification.results` — one row per produced Verifier result payload.
///
/// Every Verifier writes this one surface whatever its checks, so a dashboard
/// reads one result contract. Each payload carries its own `result_id`, and
/// `execution_id` correlates it with its execution. A queued result is staged
/// before its execution completes, so an execution that is later abandoned or
/// recovered can leave a row that is not its outcome; the queue's selected
/// `result_id` on the completed execution is authoritative.
/// [`OUTCOME`] holds the typed `VerificationOutcome` — verdict, requirement
/// breakdown, step results, and original evidence identities — as one Variant.
/// `execution_id` is the Verifier execution; the managed `run_id` is the
/// application Run judged, null when there is none, as for scheduled drift;
/// the managed `card_uid` is the Card verified. The Verifier Card is the
/// `verifier_card_uid` payload column; the binding owner is `owner_card_uid`
/// and `binding_id` the binding, both null for an execution with no binding;
/// a caller execution records its authenticated owner Service instead, when
/// one was available. `group_id` is the caller's group, null when ungrouped.
/// `deployment_environment` is the correlation tag the execution was accepted
/// with, null when it carried none.
/// The managed `sub_principal_id`/`act_principal_id` pair records whom the
/// execution served and who executed it.
pub struct ResultsTable;

/// Variant column holding the result's typed `VerificationOutcome`.
///
/// The outcome carries open actual values, step outputs, and citations, so it
/// is stored as one Variant and classified sensitive.
pub const OUTCOME: &str = "outcome";

impl DomainTable for ResultsTable {
    /// Lives in `vala.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `vala.verification.results` FQN.
    const NAME: &'static str = "results";
    /// The server appends and stamps the managed application `run_id`, verified `card_uid`, and subject/actor pair.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// The outcome makes projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The outcome column gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[OUTCOME];

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("execution_id", false),
            utf8("execution_status", false),
            utf8("verdict", false),
            utf8("verifier_version", false),
            utf8("owner_card_uid", true),
            utf8("verifier_card_uid", false),
            utf8("binding_id", true),
            utf8("group_id", true),
            utf8("deployment_environment", true),
            utf8("trigger_identity", true),
            ts_us_utc("window_start", true),
            ts_us_utc("window_end", true),
            ts_us_utc("started_at", false),
            ts_us_utc("ended_at", false),
            variant_field(OUTCOME, false),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on result, execution, Verifier, binding, and group lookups.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            "ended_at",
            vec![sort_desc("ended_at")],
            &[
                "run_id",
                "card_uid",
                "sub_principal_id",
                "act_principal_id",
                "result_id",
                "execution_id",
                "verifier_card_uid",
                "binding_id",
                "group_id",
            ],
        )
    }
}
