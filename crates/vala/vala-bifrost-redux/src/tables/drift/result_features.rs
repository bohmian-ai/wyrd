//! `bifrost.drift.result_features` — the per-feature detail of one Drift result.
//!
//! Owns the table's authored schema and physical layout only; the server's
//! result writer writes it, and the shared verdict lives in
//! `bifrost.verification.results`.

use arrow::datatypes::Field;

use crate::tables::fields::{float64, ts_us_utc, utf8};
use crate::tables::{
    DomainTable, PayloadClass, attributed_layout, daily_layout, with_shared_dimensions,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_INGESTED_AT;

/// `bifrost.drift.result_features` — one row per scored feature of one drift check.
///
/// These are the per-feature details of a drift requirement in a row of
/// `bifrost.verification.results`, joined on `result_id` and `requirement_id`
/// inside the tenant's own namespace. The binding, Verifier, window, and
/// method are repeated on every row so a cross-execution query over one feature
/// or one method never has to join back to the parent result. The managed
/// `workload_card_uid` is the binding owner, the managed `run_id` the
/// application Run, null for scheduled drift, and the managed
/// `subject_card_uid` the Card verified; the Verifier Card is the
/// `verifier_card_uid` payload column.
pub struct ResultFeaturesTable;

impl DomainTable for ResultFeaturesTable {
    /// Lives in `bifrost.drift`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "drift";
    /// Table segment of the fixed `bifrost.drift.result_features` FQN.
    const NAME: &'static str = "result_features";
    /// Feature scores are aggregate statistics, never raw captured payload.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("result_id", false),
            utf8("requirement_id", false),
            utf8("verifier_card_uid", false),
            utf8("binding_id", true),
            ts_us_utc("window_start", false),
            ts_us_utc("window_end", false),
            utf8("method", false),
            utf8("feature", false),
            float64("score", true),
            float64("threshold", true),
            utf8("verdict", false),
        ])
    }

    /// Daily partitions on `wyrd_ingested_at`, attribution-sorted, Bloom-pruned
    /// on the shared attribution set and `result_id` for the parent-result join.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(daily_layout, WYRD_INGESTED_AT, &["result_id"])
    }
}
