//! Observation correlation — the reserved server-stamped column names.
//!
//! The principal comes from the verified JWT and the card from the run's
//! server-authorized `card_ref`, so a client never forges its own identity.
//! Observation fact tables (traces, metrics, logs, genai, eval, drift) carry the
//! universal correlation set (`run_id`, `card_uid`, `principal_id`).
//!
//! [`CorrelationColumns`] fixes the server-stamped column names so the table
//! schema fingerprint stays stable.

/// Reserved observation-table column names for the server-stamped correlation block.
///
/// The identity axis is server-stamped at ingest: principal from the verified
/// JWT, card uid resolved from the run's server-authorized `card_ref`. `run_id`
/// and `data_tenant_id` are already covered by the run envelope and Bifrost
/// system columns respectively.
pub struct CorrelationColumns;

impl CorrelationColumns {
    /// Resolved card uid column (resolved by server from wire `card_ref`).
    pub const CARD_UID: &'static str = "card_uid";
    /// Resolved principal id column.
    pub const PRINCIPAL_ID: &'static str = "principal_id";
    /// Resolved experiment id column, when the run is part of an experiment.
    pub const EXPERIMENT_ID: &'static str = "experiment_id";

    /// Column names the server stamps at ingest (identity axis).
    pub const SERVER_STAMPED: &'static [&'static str] =
        &[Self::CARD_UID, Self::PRINCIPAL_ID, Self::EXPERIMENT_ID];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server stamps the resolved card uid on every correlated row.
    #[test]
    fn server_stamped_columns_include_card_uid() {
        assert!(CorrelationColumns::SERVER_STAMPED.contains(&CorrelationColumns::CARD_UID));
    }

    #[test]
    fn no_card_version_in_server_stamped() {
        assert!(
            !CorrelationColumns::SERVER_STAMPED.contains(&"card_version"),
            "card_version was removed in Stage 4 reconcile"
        );
    }
}
