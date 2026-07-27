//! Real Forge benchmark execution over the shared Bifrost harness.

use std::time::Instant;

use crate::bifrost::{BifrostHarness, seed_forge_group_for_tenant};
use wyrd_bench::{BifrostLane, BifrostScenario, NegativeFlowReport};

type BenchError = Box<dyn std::error::Error + Send + Sync>;

/// Run one typed Forge maintenance scenario.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    if scenario.lane != BifrostLane::Forge {
        return Err("Forge adapter received a non-Forge scenario".into());
    }
    let harness = BifrostHarness::start(
        usize::try_from(scenario.pods)?,
        usize::try_from(scenario.tenants)?,
    )
    .await?;
    let started = Instant::now();
    let tenant = *harness.tenants().first().ok_or("Forge needs one tenant")?;
    let server = harness
        .cluster()
        .servers()
        .first()
        .ok_or("Forge needs one server")?;
    let fixture = seed_forge_group_for_tenant(server, tenant, "bifrost_bench_forge").await;
    let outcome = fixture.forge.run_once().await?;
    let negative_flows = if scenario.require_negative_flows {
        let retry = fixture.forge.run_once().await?;
        NegativeFlowReport::executed(
            ["completed_forge_tick_is_idempotent"],
            retry.bins_committed == 0 && retry.tables_failed == 0,
        )
    } else {
        NegativeFlowReport::skipped()
    };
    let verified =
        (outcome.tables_succeeded > 0 || outcome.tables_skipped > 0) && negative_flows.passed;
    let mut envelope =
        wyrd_bench::BifrostReportEnvelope::new(scenario, super::bench_report::readiness(verified));
    envelope.negative_flows = negative_flows;
    let report = super::bench_report::LaneExecutionReport {
        envelope,
        elapsed_us: u64::try_from(started.elapsed().as_micros())?,
        verified,
        rows: 1,
    };
    super::bench_report::emit_report(&report, "forge")?;
    harness.shutdown().await?;
    if !verified {
        return Err("Forge benchmark verification failed".into());
    }
    Ok(())
}
