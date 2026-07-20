//! Server-owned, in-memory eval run/lease/session state.
//!
//! This is ephemeral transport state — **not** the doctrinal Card→Run→
//! Observation run. It creates no run registry and persists no run table. The
//! map lives in one process's memory, so eval is single-replica / sticky-session
//! routed for now (a documented deployment constraint; a shared run-store is a
//! deferred decision).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vala_eval::orchestrator::SharedRun;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::ids::{LeaseToken, RunId};

/// Maximum concurrently open eval runs **per tenant**. New opens beyond this are
/// rejected with 429. A process-global cap would be a cross-tenant DoS — one
/// tenant's abandoned runs would `429` every other tenant for up to `RUN_TTL`.
pub const MAX_CONCURRENT_RUNS: usize = 100;

/// Runs older than this are eligible for lazy eviction on the next `open`.
pub const RUN_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Composite run-map key. The tenant is part of the key so lookups are
/// tenant-scoped by construction — a foreign `run_id` is indistinguishable from
/// a nonexistent one, closing the existence-oracle.
pub type RunKey = (DataTenantId, RunId);

/// Tenant-keyed in-memory run map.
pub type EvalRuns = Arc<Mutex<HashMap<RunKey, Arc<RunEntry>>>>;

/// Build an empty run map.
#[must_use]
pub fn new_run_map() -> EvalRuns {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Per-run shared state owned by the server.
pub struct RunEntry {
    /// The engine state machine.
    pub state: SharedRun,
    /// Bearer lease minted at open; a per-run capability beneath principal identity.
    pub lease: LeaseToken,
    /// Principal that opened the run. Enforced on every subsequent call.
    pub owner: PrincipalId,
    /// Wall-clock instant when this run was opened, used for TTL eviction.
    pub opened_at: Instant,
}

/// Evict TTL-expired runs, then count the caller tenant's live runs.
///
/// The per-tenant cap is derived by filtering the single composite-keyed map
/// **after** eviction — never a side counter, which would drift against lazy
/// eviction (over-count → false 429s; under-count → a cap bypass). Eviction and
/// the count share one pass so the count can never reflect stale entries.
pub fn sweep_and_count_tenant(
    runs: &mut HashMap<RunKey, Arc<RunEntry>>,
    tenant: DataTenantId,
) -> usize {
    runs.retain(|_, entry| entry.opened_at.elapsed() < RUN_TTL);
    runs.keys()
        .filter(|(run_tenant, _)| *run_tenant == tenant)
        .count()
}

#[cfg(test)]
mod tests {
    use super::{RUN_TTL, RunEntry, sweep_and_count_tenant};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use vala_eval::orchestrator::RunState;
    use wyrd_runtime::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::protocol::SimulatedUserMode;
    use wyrd_spec::vala::ids::{LeaseToken, RunId};

    fn entry(opened_at: Instant) -> Arc<RunEntry> {
        let card_ref = CardRef {
            kind: CardKind::Eval,
            name: CardName::new("rubric").expect("name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: Some(SpaceName::new("prod").expect("space")),
            uid: None,
        };
        let run_state = RunState::open(card_ref, SimulatedUserMode::Client, Vec::new());
        Arc::new(RunEntry {
            state: Arc::new(tokio::sync::Mutex::new(run_state)),
            lease: LeaseToken::new("lease-abc").expect("lease"),
            owner: PrincipalId::new(uuid::Uuid::now_v7()),
            opened_at,
        })
    }

    #[test]
    fn cap_count_is_per_tenant_and_survives_eviction() {
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let mut runs = HashMap::new();

        // Two live runs for A, one for B, plus one expired A run.
        runs.insert((tenant_a, RunId::new()), entry(Instant::now()));
        runs.insert((tenant_a, RunId::new()), entry(Instant::now()));
        runs.insert((tenant_b, RunId::new()), entry(Instant::now()));
        let expired = Instant::now()
            .checked_sub(RUN_TTL + Duration::from_secs(1))
            .expect("instant subtract");
        runs.insert((tenant_a, RunId::new()), entry(expired));

        // A's count excludes B and drops the expired entry (no drift).
        assert_eq!(sweep_and_count_tenant(&mut runs, tenant_a), 2);
        // The expired entry is gone from the map after the sweep.
        assert_eq!(runs.len(), 3);
        // B's count is isolated from A's load.
        assert_eq!(sweep_and_count_tenant(&mut runs, tenant_b), 1);
    }
}
