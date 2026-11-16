//! `bifrost.verification.evidence` — published deferred evidence, one row
//! per subscribed Verifier version that accepted it.
//!
//! Owns the table's authored schema and physical layout only; the server
//! writes each acceptance through Scribe under a batch id derived from its
//! source and subscriber.

use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.verification.evidence` — one row per subscriber acceptance of
/// one published deferred input.
///
/// `host`, `repository_id`, `commit`, and `input` name the source the
/// evidence is about, and `verifier_card_uid` the exact Verifier version
/// that subscribed to it. Each acceptance is written under a batch id
/// derived from those five, so the first accepted batch keeps the evidence,
/// a different value for it is refused as a batch conflict, and its durable
/// batch fence names the acceptance whether or not a query can see the row
/// yet. `sha256` is the content identity: the digest of the canonical JSON
/// of [`EVIDENCE`] without its display filename. `evidence_id` is derived from
/// the source and content, so every subscriber of the same publication
/// holds the same id. [`EVIDENCE`] is the declaration itself: inline JSON, or
/// a File named by digest whose bytes stay in verification File storage.
pub struct EvidenceTable;

/// Variant column holding the published input declaration.
pub const EVIDENCE: &str = "value";

impl DomainTable for EvidenceTable {
    /// Lives in `bifrost.verification`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "verification";
    /// Table segment of the fixed `bifrost.verification.evidence` FQN.
    const NAME: &'static str = "evidence";
    /// Published JSON values make projecting this table require the elevated
    /// payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The value column gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[EVIDENCE];
    /// An acceptance exists only once its batch fence commits.
    const FENCED_ONLY: bool = true;

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("evidence_id", false),
            utf8("host", false),
            utf8("repository_id", false),
            utf8("commit", false),
            utf8("input", false),
            utf8("verifier_card_uid", false),
            utf8("sha256", false),
            variant_field(EVIDENCE, false),
            ts_us_utc("recorded_at", false),
        ])
    }

    /// Daily partitions on `recorded_at`, attribution-sorted, Bloom-pruned on
    /// the shared attribution set, the evidence, the commit a consumer reads
    /// it for, and the subscribing Verifier.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(
            daily_layout,
            "recorded_at",
            &["evidence_id", "commit", "verifier_card_uid"],
        )
    }
}
