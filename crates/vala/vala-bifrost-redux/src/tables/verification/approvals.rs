//! `bifrost.verification.approvals` — human approval decisions, one row per
//! decided approval occurrence of a queued execution.
//!
//! Owns the table's authored schema and physical layout only; the server
//! writes each decision through Scribe under a batch id derived from its
//! approval id.

use arrow::datatypes::Field;

use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.approvals` — one row per decided approval
/// occurrence.
///
/// `approval_id` is the server-issued identity of one step, fanout item, and
/// repeat iteration of `execution_id`, whose exact Verifier version is
/// `verifier_card_uid`. Each decision is written under a batch id derived
/// from `approval_id`, so the first accepted decision is final, a different
/// decision for it is refused as a batch conflict, and its durable batch
/// fence names the decision whether or not a query can see the row yet.
/// `decision` is `approved` or `rejected`, `actor_id` the deciding
/// principal, and [`REASON`] the decider's optional free-text reason.
pub struct ApprovalsTable;

/// Column holding the decider's optional free-text reason.
pub const REASON: &str = "reason";

impl DomainTable for ApprovalsTable {
    /// Lives in `bifrost.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `bifrost.verification.approvals` FQN.
    const NAME: &'static str = "approvals";
    /// A free-text reason makes projecting this table require the elevated
    /// payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The reason column gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[REASON];
    /// A decision exists only once its batch fence commits.
    const FENCED_ONLY: bool = true;

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("approval_id", false),
            utf8("execution_id", false),
            utf8("verifier_card_uid", false),
            utf8("decision", false),
            utf8("actor_id", false),
            utf8(REASON, true),
            ts_us_utc("decided_at", false),
        ])
    }

    /// Daily partitions on `decided_at`, attribution-sorted, Bloom-pruned on
    /// the shared attribution set, the approval, and the execution an
    /// inspection or resumption reads it for.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(daily_layout, "decided_at", &["approval_id", "execution_id"])
    }
}
