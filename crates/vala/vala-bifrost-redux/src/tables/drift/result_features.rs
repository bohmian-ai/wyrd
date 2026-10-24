//! `bifrost.drift.result_features` — the per-feature detail of one Drift result.
//!
//! Owns the table's authored schema and physical layout only; the server's
//! result writer writes it, and the shared verdict lives in
//! `bifrost.verification.results`.

use arrow::datatypes::Field;

use crate::tables::fields::{float64, ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_INGESTED_AT;

/// `bifrost.drift.result_features` — one row per scored feature of one drift check.
///
/// These are the per-feature details of a drift requirement in a row of
/// `bifrost.verification.results`, joined on `result_id` and `requirement_id`
/// inside the tenant's own namespace. The binding, owner, Verifier, window, and
/// method are repeated on every row so a cross-execution query over one feature
/// or one method never has to join back to the parent result. The managed
/// `run_id` is the application Run, null for scheduled drift, and the managed
/// `card_uid` is the Card verified; the Verifier Card is the
/// `verifier_card_uid` payload column.
pub struct ResultFeaturesTable;

impl DomainTable for ResultFeaturesTable {
    /// Lives in `bifrost.drift`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "drift";
    /// Table segment of the fixed `bifrost.drift.result_features` FQN.
    const NAME: &'static str = "result_features";
    /// The server appends and stamps the managed application `run_id`, verified `card_uid`, and subject/actor pair.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// Feature scores are aggregate statistics, never raw captured payload.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("result_id", false),
            utf8("requirement_id", false),
            utf8("owner_card_uid", true),
            utf8("verifier_card_uid", false),
            utf8("binding_id", true),
            ts_us_utc("window_start", false),
            ts_us_utc("window_end", false),
            utf8("method", false),
            utf8("feature", false),
            float64("score", true),
            float64("threshold", true),
            utf8("verdict", false),
        ]
    }

    /// Daily partitions sorted newest first, Bloom-pruned on `result_id` for the parent-result join.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            WYRD_INGESTED_AT,
            vec![sort_desc(WYRD_INGESTED_AT)],
            &[
                "run_id",
                "card_uid",
                "sub_principal_id",
                "act_principal_id",
                "result_id",
            ],
        )
    }
}
