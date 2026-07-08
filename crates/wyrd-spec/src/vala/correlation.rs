//! Observation correlation — the code axis carried on a run.
//!
//! Wyrd links the agent lifecycle (develop → review → deploy → observe) on three
//! axes: **code** (repo + commit, bridged before a commit by a dev-session id),
//! **actor** (developer principal → service principal), and **card** (uid). The
//! principal comes from the verified JWT and the card from the run's
//! server-authorized `card_ref`, so a client never forges its own identity. It
//! asserts only the **code axis** — which only the local environment knows — via
//! [`CorrelationContext`].
//!
//! Code context is a property of the `agent_traces` table only; observation fact
//! tables (traces, metrics, logs, genai, eval, drift) carry only the universal
//! correlation set (`run_id`, `card_uid`, `principal_id`) not the code axis.
//!
//! [`CorrelationColumns`] reserves the observation-table column names for both
//! the client-asserted code axis and the server-stamped identity axis. The names
//! are fixed here so the table schema fingerprint stays stable when the warehouse
//! lands.

use serde::{Deserialize, Serialize};

use crate::origin::CommitSha;
use crate::vala::ids::DevSessionId;

/// Client-asserted code axis context for dev-capture (`agent_traces`).
///
/// Stamped at run start from the local environment. The server stamps principal
/// and tenant from the verified JWT, and card from the run's authorized `card_ref`;
/// those are never carried here.
///
/// This is the dev-capture code axis — it lives only in `vala.dev.agent_traces` rows,
/// never on observation fact rows (traces, metrics, logs, genai, eval, drift).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CorrelationContext {
    /// Normalized repository identifier, e.g. `github.com/org/repo`.
    pub repo: String,
    /// Advisory base commit the run executed against. Absent before the first commit.
    /// Reconciled to the resulting commit at push/PR time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitSha>,
    /// Branch the run executed on, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Local dev-session id bridging activity that predates a commit or card.
    pub dev_session_id: DevSessionId,
}

/// Reserved observation-table column names for the correlation block.
///
/// Split by trust boundary: the code axis is client-asserted (from
/// [`CorrelationContext`]); the identity axis is server-stamped at ingest:
/// principal from the verified JWT, card uid resolved from the run's server-authorized
/// `card_ref`. `run_id` and `data_tenant_id` are already covered by the run envelope
/// and Bifrost system columns respectively.
pub struct CorrelationColumns;

impl CorrelationColumns {
    // --- client-asserted code axis (agent_traces only) ---
    /// Repository identifier column.
    pub const REPO: &'static str = "repo";
    /// Commit sha column.
    pub const COMMIT_SHA: &'static str = "commit_sha";
    /// Branch column.
    pub const BRANCH: &'static str = "branch";
    /// Dev-session id column.
    pub const DEV_SESSION_ID: &'static str = "dev_session_id";

    // --- server-stamped identity axis ---
    /// Resolved card uid column (resolved by server from wire `card_ref`).
    pub const CARD_UID: &'static str = "card_uid";
    /// Resolved principal id column.
    pub const PRINCIPAL_ID: &'static str = "principal_id";
    /// Resolved experiment id column, when the run is part of an experiment.
    pub const EXPERIMENT_ID: &'static str = "experiment_id";

    /// Column names the client asserts (code axis — agent_traces only).
    pub const CLIENT_ASSERTED: &'static [&'static str] = &[
        Self::REPO,
        Self::COMMIT_SHA,
        Self::BRANCH,
        Self::DEV_SESSION_ID,
    ];

    /// Column names the server stamps at ingest (identity axis).
    pub const SERVER_STAMPED: &'static [&'static str] =
        &[Self::CARD_UID, Self::PRINCIPAL_ID, Self::EXPERIMENT_ID];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_context_round_trips_and_omits_absent_fields() {
        let ctx = CorrelationContext {
            repo: "github.com/org/repo".to_string(),
            commit: None,
            branch: None,
            dev_session_id: DevSessionId::from_string(
                "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b".to_string(),
            ),
        };
        let json = serde_json::to_value(&ctx).expect("serialize");
        assert_eq!(json.get("branch"), None, "absent branch is omitted");
        assert_eq!(json.get("commit"), None, "absent commit is omitted");
        let back: CorrelationContext = serde_json::from_value(json).expect("deserialize");
        assert_eq!(ctx, back);
    }

    #[test]
    fn correlation_context_round_trips_with_commit() {
        let ctx = CorrelationContext {
            repo: "github.com/org/repo".to_string(),
            commit: Some(CommitSha::new("deadbeef").expect("valid commit")),
            branch: Some("main".to_string()),
            dev_session_id: DevSessionId::from_string(
                "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b".to_string(),
            ),
        };
        let json = serde_json::to_value(&ctx).expect("serialize");
        let back: CorrelationContext = serde_json::from_value(json).expect("deserialize");
        assert_eq!(ctx, back);
    }

    #[test]
    fn correlation_columns_split_by_trust_boundary() {
        assert!(CorrelationColumns::CLIENT_ASSERTED.contains(&CorrelationColumns::COMMIT_SHA));
        assert!(CorrelationColumns::SERVER_STAMPED.contains(&CorrelationColumns::CARD_UID));
        for c in CorrelationColumns::CLIENT_ASSERTED {
            assert!(!CorrelationColumns::SERVER_STAMPED.contains(c));
        }
    }

    #[test]
    fn no_card_version_in_server_stamped() {
        assert!(
            !CorrelationColumns::SERVER_STAMPED.contains(&"card_version"),
            "card_version was removed in Stage 4 reconcile"
        );
    }
}
