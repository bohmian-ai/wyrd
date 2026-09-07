//! Deterministic, IO-free Forge plan construction.
//!
//! Planning binds discovered candidates to the snapshot they were observed on
//! and derives their durable identity. It carries no execution capacity: what
//! a rewrite may run is decided by the worker's own admission queue against a
//! per-plan estimate, so nothing here classifies, clamps, or refuses work.

use sha2::{Digest, Sha256};
use vala_sql::row_types::forge_tasks::{
    FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEstimates, ForgeTaskPlan, ForgeTaskStrategy,
};

use super::error::ForgeError;

/// One exact candidate derived from a single stable table snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgePlanCandidate {
    /// Closed maintenance strategy.
    pub strategy: ForgeTaskStrategy,
    /// Sorted, duplicate-free immutable input identities.
    pub inputs: Vec<String>,
    /// Per-input bytes aligned with `inputs`.
    pub input_bytes: Vec<u64>,
    /// Estimated total input bytes.
    pub bytes: u64,
    /// Stable strategy parameters.
    pub parameters: serde_json::Value,
}

/// Computes the durable identity digest of one exact task plan.
///
/// The digest is a column in the `forge_tasks` idempotency index, so it is what
/// makes replanning the same work recognize its own already-enqueued task
/// instead of creating a second one. Publication recomputes it from the claimed
/// plan to bind a committed snapshot back to the durable row that authorized
/// it, which only works while both sides derive it here.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the plan's parameters do not
/// serialize, which no validated plan can do.
pub(super) fn plan_hash(plan: &ForgeTaskPlan) -> Result<[u8; 32], ForgeError> {
    let canonical = serde_json::to_vec(&serde_json::json!({
        "inputs": plan.inputs,
        "parameters": plan.parameters,
        "version": plan.version,
    }))
    .map_err(|error| ForgeError::Invariant {
        detail: error.to_string(),
    })?;
    Ok(Sha256::digest(canonical).into())
}

/// Every deterministic candidate derived from one immutable table snapshot.
///
/// The snapshot identity travels with the candidates so a plan can never be
/// persisted against a base the discovery pass did not actually observe.
#[derive(Debug, Clone)]
pub struct ForgeTableSnapshot {
    /// Exact Iceberg snapshot identity shared by every emitted plan.
    pub snapshot_id: i64,
    /// Deterministically ordered candidates from that snapshot.
    pub candidates: Vec<ForgePlanCandidate>,
}

/// Exact versioned plan bound to the snapshot that produced it.
#[derive(Debug, Clone)]
pub struct PlannedForgeTask {
    /// Closed strategy persisted with the task.
    pub strategy: ForgeTaskStrategy,
    /// Stable snapshot used for discovery and planning.
    pub base_snapshot_id: i64,
    /// Exact versioned payload.
    pub plan: ForgeTaskPlan,
    /// SHA-256 of the canonical payload bytes.
    pub plan_hash: [u8; 32],
    /// Persisted input observations retained for telemetry.
    pub estimates: ForgeTaskEstimates,
}

/// Plans every candidate discovered from one immutable snapshot.
///
/// Candidates are already the exact work a discovery pass selected, so planning
/// neither regroups nor splits them: it binds each one to the snapshot identity
/// and hashes its canonical payload.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when a candidate's inputs or byte
/// observations are empty, unsorted, duplicated, or misaligned.
pub fn plan_table(snapshot: &ForgeTableSnapshot) -> Result<Vec<PlannedForgeTask>, ForgeError> {
    snapshot
        .candidates
        .iter()
        .map(|candidate| plan_candidate(snapshot.snapshot_id, candidate))
        .collect()
}

/// Binds one candidate to its snapshot and derives its durable identity.
///
/// The canonical hash covers exactly the persisted payload — inputs,
/// parameters, and payload version — so two discovery passes that select the
/// same work produce the same durable identity and the idempotent enqueue
/// collapses them.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the input count exceeds `u32`, the
/// byte total is zero, the per-input byte terms do not align with the inputs,
/// the inputs are not strictly sorted, or the payload cannot be canonicalized.
fn plan_candidate(
    snapshot_id: i64,
    candidate: &ForgePlanCandidate,
) -> Result<PlannedForgeTask, ForgeError> {
    let files = u32::try_from(candidate.inputs.len()).map_err(|_| ForgeError::Invariant {
        detail: "Forge plan input count exceeds u32".to_owned(),
    })?;
    if files == 0
        || candidate.bytes == 0
        || candidate.inputs.len() != candidate.input_bytes.len()
        || candidate.inputs.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(ForgeError::Invariant {
            detail: "Forge candidate inputs and estimates must be positive, sorted, and unique"
                .to_owned(),
        });
    }
    let plan = ForgeTaskPlan {
        version: FORGE_TASK_PAYLOAD_VERSION,
        inputs: candidate.inputs.clone(),
        parameters: candidate.parameters.clone(),
    };
    let plan_hash = plan_hash(&plan)?;
    Ok(PlannedForgeTask {
        strategy: candidate.strategy,
        base_snapshot_id: snapshot_id,
        plan,
        plan_hash,
        estimates: ForgeTaskEstimates {
            files,
            bytes: candidate.bytes,
        },
    })
}
