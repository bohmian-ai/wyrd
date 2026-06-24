//! Observation correlation — the code axis carried on a run.
//!
//! Wyrd links the agent lifecycle (develop → review → deploy → observe) on three
//! axes: **code** (repo + commit, bridged before a commit by a dev-session id),
//! **actor** (developer principal → service principal), and **card** (uid +
//! version). Identity is server-resolved from the emit-plane token, so a client
//! never stamps its own card/principal. It asserts only the **code axis** —
//! which only the local environment knows — via [`CorrelationContext`].
//!
//! Code context is a property of a run, not of each record, so it rides the
//! agent-start observation once; every other record correlates by `run_id`.
//!
//! [`CorrelationColumns`] reserves the observation-table column names for both
//! the client-asserted code axis and the server-stamped identity axis. The
//! names are fixed here, before observation tables are persisted, so the table
//! schema fingerprint stays stable when the warehouse lands.

use serde::{Deserialize, Serialize};

use crate::origin::CommitSha;
use crate::vala::ids::DevSessionId;

/// Client-asserted code context for one run.
///
/// Stamped at run start from the local environment. The server resolves card,
/// principal, tenant, and experiment from the authenticated emit-plane context;
/// those are never carried here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CorrelationContext {
    /// Normalized repository identifier, e.g. `github.com/org/repo`.
    pub repo: String,
    /// Commit the run executed against. When the tree is dirty this is the base
    /// commit, reconciled to the resulting commit at push/PR time.
    pub commit: CommitSha,
    /// Branch the run executed on, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Local dev-session id bridging activity that predates a commit or card.
    pub dev_session_id: DevSessionId,
}

/// Reserved observation-table column names for the correlation block.
///
/// Split by trust boundary: the code axis is client-asserted (from
/// [`CorrelationContext`]); the identity axis is server-stamped at ingest from
/// the emit-plane token and run registry. `run_id` and `data_tenant_id` are
/// already covered by the run envelope and Bifrost system columns respectively.
pub struct CorrelationColumns;

impl CorrelationColumns {
    // --- client-asserted code axis ---
    /// Repository identifier column.
    pub const REPO: &'static str = "repo";
    /// Commit sha column.
    pub const COMMIT_SHA: &'static str = "commit_sha";
    /// Branch column.
    pub const BRANCH: &'static str = "branch";
    /// Dev-session id column.
    pub const DEV_SESSION_ID: &'static str = "dev_session_id";

    // --- server-stamped identity axis ---
    /// Resolved card uid column.
    pub const CARD_UID: &'static str = "card_uid";
    /// Resolved card version column.
    pub const CARD_VERSION: &'static str = "card_version";
    /// Resolved principal id column.
    pub const PRINCIPAL_ID: &'static str = "principal_id";
    /// Resolved experiment id column, when the run is part of an experiment.
    pub const EXPERIMENT_ID: &'static str = "experiment_id";

    /// Column names the client asserts (code axis).
    pub const CLIENT_ASSERTED: &'static [&'static str] = &[
        Self::REPO,
        Self::COMMIT_SHA,
        Self::BRANCH,
        Self::DEV_SESSION_ID,
    ];

    /// Column names the server stamps at ingest (identity axis).
    pub const SERVER_STAMPED: &'static [&'static str] = &[
        Self::CARD_UID,
        Self::CARD_VERSION,
        Self::PRINCIPAL_ID,
        Self::EXPERIMENT_ID,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_context_round_trips_and_omits_absent_branch() {
        let ctx = CorrelationContext {
            repo: "github.com/org/repo".to_string(),
            commit: CommitSha::new("deadbeef").expect("valid commit"),
            branch: None,
            dev_session_id: DevSessionId::from_string(
                "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b".to_string(),
            ),
        };
        let json = serde_json::to_value(&ctx).expect("serialize");
        assert_eq!(json.get("branch"), None, "absent branch is omitted");
        let back: CorrelationContext = serde_json::from_value(json).expect("deserialize");
        assert_eq!(ctx, back);
    }

    #[test]
    fn correlation_columns_split_by_trust_boundary() {
        assert!(CorrelationColumns::CLIENT_ASSERTED.contains(&CorrelationColumns::COMMIT_SHA));
        assert!(CorrelationColumns::SERVER_STAMPED.contains(&CorrelationColumns::CARD_UID));
        // No column appears on both sides of the line.
        for c in CorrelationColumns::CLIENT_ASSERTED {
            assert!(!CorrelationColumns::SERVER_STAMPED.contains(c));
        }
    }
}
