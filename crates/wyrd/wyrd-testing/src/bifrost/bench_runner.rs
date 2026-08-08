//! Fixture-driven tier planning and budget enforcement shared by every family runner.
//!
//! The live family runners (ingest, query, distributed, tenant fairness, mixed)
//! differ only in how they offer load and interpret captured telemetry. Every
//! decision that does not require a running cluster — which tier a run targets,
//! how many stages that tier executes and at what durations, how a fixture
//! control unit maps onto a [`StageControl`], which concrete topology a pod
//! count selects, and whether a lane has breached its wall-clock budget — is
//! owned here so it is deterministic and unit-testable without Postgres. The
//! runners consume this planner and add only the cluster-bound offer/capture
//! step, so no ladder constant or budget is compiled into runner code.

use std::path::PathBuf;
use std::time::Duration;

use super::BifrostTopology;
use super::bench_dataset::{
    BenchmarkSelectionError, TierBudgets, TopologyProfile, TopologyProfiles, WorkloadControl,
    WorkloadProfile, WorkloadProfiles,
};
use super::bench_report::{BenchmarkTier, StageControl};

/// Default artifact root used when `--output-root` is omitted.
///
/// Every runner writes its report tree under this path so the qualification and
/// smoke lanes share one discoverable location. The mise lanes may override it,
/// but the default keeps a bare invocation self-consistent.
pub const DEFAULT_OUTPUT_ROOT: &str = "target/bifrost-benchmarks/qualification";

/// Tier a runner invocation targets.
///
/// Only the two runner-authored tiers are representable here; the report-level
/// [`BenchmarkTier::Scale`] is not a runner input. Parsing rejects any other
/// token so an unknown `--tier` fails before a cluster starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerTier {
    /// Bounded smoke run: exactly the first ladder stage at smoke durations.
    Smoke,
    /// Authoritative qualification run: the full fixture ladder.
    Qualification,
}

impl RunnerTier {
    /// Parse a `--tier` CLI token into a runner tier.
    ///
    /// # Errors
    /// Returns [`RunnerPlanError::UnknownTier`] for any token other than
    /// `smoke` or `qualification`.
    pub fn parse(token: &str) -> Result<Self, RunnerPlanError> {
        match token {
            "smoke" => Ok(Self::Smoke),
            "qualification" => Ok(Self::Qualification),
            other => Err(RunnerPlanError::UnknownTier(other.to_owned())),
        }
    }

    /// Project this runner tier onto the persisted report tier.
    ///
    /// Every artifact a runner writes is stamped with this value, so a smoke
    /// run can never be mistaken for capacity-authoritative evidence.
    #[must_use]
    pub fn benchmark_tier(self) -> BenchmarkTier {
        match self {
            Self::Smoke => BenchmarkTier::Smoke,
            Self::Qualification => BenchmarkTier::Qualification,
        }
    }

    /// Return the per-lane wall-clock budget in seconds for this tier.
    ///
    /// Budgets are read from the locked [`TierBudgets`] fixture, never inferred.
    /// Qualification lanes are not budget-bounded by this task, so only the
    /// smoke lane budget is returned.
    #[must_use]
    pub fn lane_budget_seconds(self, budgets: &TierBudgets) -> Option<u32> {
        match self {
            Self::Smoke => Some(budgets.smoke_lane_seconds),
            Self::Qualification => None,
        }
    }
}

/// The benchmark family a workload profile belongs to.
///
/// A workload's family is a property of its fixture shape, not a separate field:
/// the presence of an ingest shape, tenant matrix, or mixed fractions is
/// mutually exclusive across the fixture, and the two query-only families are
/// disambiguated by the `distributed-` id prefix (a distributed workload reuses a
/// plain `query_id` but scales the topology instead of the offer ladder). The
/// dispatch layer classifies each resolved invocation with this enum so the two
/// benchmark binaries route a workload to exactly one runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerFamily {
    /// Scribe ingest write path (`ingest-*`).
    Ingest,
    /// Single-topology Oracle read path (`q1`..`q5`).
    Query,
    /// Weak-scaling Oracle read path across the pod ladder (`distributed-*`).
    Distributed,
    /// Multi-tenant Oracle read path under a total-volume-constant offer.
    Fairness,
    /// Concurrent Scribe/Oracle/Forge diagonal path (`mixed-*`).
    Mixed,
}

impl RunnerFamily {
    /// Classify the family a resolved workload profile belongs to.
    ///
    /// The fixture guarantees at most one of `ingest_shape`, `tenant_matrix`, and
    /// `mixed_fractions` is populated per workload, so those three shapes classify
    /// directly. The remaining query-only workloads split on the `distributed-`
    /// id prefix, which is the only fixture signal distinguishing a weak-scaling
    /// sweep from a single-topology query.
    #[must_use]
    pub fn classify(workload: &WorkloadProfile) -> Self {
        if workload.ingest_shape.is_some() {
            Self::Ingest
        } else if workload.tenant_matrix.is_some() {
            Self::Fairness
        } else if workload.mixed_fractions.is_some() {
            Self::Mixed
        } else if workload.workload_id.starts_with("distributed") {
            Self::Distributed
        } else {
            Self::Query
        }
    }
}

/// One planned stage: its ordinal, controlling ladder value, and window durations.
///
/// A plan is produced entirely from the fixture profile and the target tier
/// before any load is offered, so a runner never derives stage count or timing
/// from measured behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagePlan {
    /// Zero-based ordinal, equal to the stage index in the emitted report.
    pub ordinal: u16,
    /// Controlling ladder value offered during the measured window.
    pub control_value: u64,
    /// Warmup seconds before the measured window.
    pub warmup_seconds: u32,
    /// Measured window seconds.
    pub measure_seconds: u32,
    /// Drain seconds after the measured window.
    pub drain_seconds: u32,
}

/// Build the ordered stage plan for a workload at a tier.
///
/// A smoke run executes exactly the first ladder rung at the profile's smoke
/// durations (D69), regardless of ladder length. A qualification run executes
/// every ladder rung at the profile's full-tier durations. Ordinals are the
/// zero-based stage index so they match [`BifrostCapacityReport`] validation.
///
/// # Errors
/// Returns [`RunnerPlanError::EmptyLadder`] when the profile declares no ladder
/// rung, so a runner cannot produce a zero-stage report.
///
/// [`BifrostCapacityReport`]: super::bench_report::BifrostCapacityReport
pub fn plan_stages(
    profile: &WorkloadProfile,
    tier: RunnerTier,
) -> Result<Vec<StagePlan>, RunnerPlanError> {
    let ladder = &profile.control.ladder;
    if ladder.is_empty() {
        return Err(RunnerPlanError::EmptyLadder(profile.workload_id.clone()));
    }
    let values: &[u64] = match tier {
        RunnerTier::Smoke => &ladder[..1],
        RunnerTier::Qualification => ladder,
    };
    let (warmup, measure, drain) = match tier {
        RunnerTier::Smoke => (
            profile.smoke.warmup_seconds,
            profile.smoke.measure_seconds,
            profile.smoke.drain_seconds,
        ),
        RunnerTier::Qualification => (
            profile.stages.warmup_seconds,
            profile.stages.measure_seconds,
            profile.stages.drain_seconds,
        ),
    };
    Ok(values
        .iter()
        .enumerate()
        .map(|(index, &control_value)| StagePlan {
            ordinal: u16::try_from(index).unwrap_or(u16::MAX),
            control_value,
            warmup_seconds: warmup,
            measure_seconds: measure,
            drain_seconds: drain,
        })
        .collect())
}

/// Map a fixture control unit and offered value onto a typed [`StageControl`].
///
/// This is the single place the closed set of fixture control units is
/// translated, so a runner offers load through exactly one family-native
/// controller. The `qps` unit carries the profile's `max_in_flight` cap; every
/// other unit is a single offered value.
///
/// # Errors
/// Returns [`RunnerPlanError::UnknownControlUnit`] for a unit outside the closed
/// set, and [`RunnerPlanError::MissingMaxInFlight`] when a `qps` control omits
/// its required `max_in_flight` cap.
pub fn stage_control(
    control: &WorkloadControl,
    value: u64,
) -> Result<StageControl, RunnerPlanError> {
    match control.unit.as_str() {
        "qps" => {
            let max_in_flight = control
                .max_in_flight
                .ok_or_else(|| RunnerPlanError::MissingMaxInFlight("qps".to_owned()))?;
            Ok(StageControl::Qps {
                value,
                max_in_flight,
            })
        }
        "requests_per_sec" => Ok(StageControl::RequestsPerSec { value }),
        "concurrency" => Ok(StageControl::Concurrency { value }),
        "streams" => Ok(StageControl::Streams { value }),
        "logical_bytes_per_sec" => Ok(StageControl::LogicalBytesPerSec { value }),
        "returned_bytes_per_sec" => Ok(StageControl::ReturnedBytesPerSec { value }),
        other => Err(RunnerPlanError::UnknownControlUnit(other.to_owned())),
    }
}

/// Select the concrete cluster topology for an Oracle pod count.
///
/// The distributed ladder uses `1`, `2`, `3`, and `6` mixed pods (D70); every
/// other count is rejected so a runner never starts an unlisted topology. The
/// two-pod rung maps to [`BifrostTopology::TwoPod`], which mixes onto
/// `BifrostClusterSpec::two_mixed()`.
///
/// # Errors
/// Returns [`RunnerPlanError::UnsupportedPodCount`] for any count not on the
/// locked ladder.
pub fn topology_for_pods(pods: u16) -> Result<BifrostTopology, RunnerPlanError> {
    match pods {
        1 => Ok(BifrostTopology::OnePod),
        2 => Ok(BifrostTopology::TwoPod),
        3 => Ok(BifrostTopology::ThreePod),
        6 => Ok(BifrostTopology::SixPod),
        other => Err(RunnerPlanError::UnsupportedPodCount(other)),
    }
}

/// Verify a lane has not exceeded its wall-clock budget.
///
/// Budget enforcement is measured from lane process start and compared against
/// the fixture-locked per-lane budget. A breach is a lane failure that names the
/// budget so the artifact and exit status attribute the failure precisely.
///
/// # Errors
/// Returns [`RunnerPlanError::BudgetBreach`] when `elapsed` exceeds
/// `budget_seconds`.
pub fn enforce_budget(elapsed: Duration, budget_seconds: u32) -> Result<(), RunnerPlanError> {
    let budget = Duration::from_secs(u64::from(budget_seconds));
    if elapsed > budget {
        return Err(RunnerPlanError::BudgetBreach {
            elapsed_seconds: elapsed.as_secs(),
            budget_seconds,
        });
    }
    Ok(())
}

/// A fully parsed runner invocation, resolved against fixtures before any cluster start.
///
/// Both family binaries construct this from their process arguments so the flag
/// surface (`--workload`, `--topology`, `--tier`, `--output-root`, `--run-id`)
/// is identical across lanes. Parsing is purely syntactic; [`Self::resolve`]
/// performs the fail-closed fixture lookup, and the two steps are split so an
/// unknown selection is rejected before a cluster, storage, or database side
/// effect occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerInvocation {
    /// Requested workload id, validated against `workload-profiles.json`.
    pub workload_id: String,
    /// Requested topology id, validated against `topology-profiles.json`.
    pub topology_id: String,
    /// Target tier parsed from `--tier`.
    pub tier: RunnerTier,
    /// Artifact root the report tree is written beneath.
    pub output_root: PathBuf,
    /// Run id that groups every family artifact for this invocation.
    pub run_id: String,
}

/// A runner invocation whose workload and topology ids resolved to locked fixtures.
///
/// Holding borrows of the owning fixture entries proves resolution succeeded
/// before a cluster starts; a runner reads its ladder, control, timings, and pod
/// count entirely from these references.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedInvocation<'a> {
    /// Resolved workload profile providing the control ladder and stage timings.
    pub workload: &'a WorkloadProfile,
    /// Resolved topology profile providing the mixed-pod count.
    pub topology: &'a TopologyProfile,
    /// Target tier for this invocation.
    pub tier: RunnerTier,
}

impl RunnerInvocation {
    /// Parse the shared runner flag surface from process arguments.
    ///
    /// `args` must exclude the program name. Every flag takes one value;
    /// `--workload`, `--topology`, and `--tier` are required, `--run-id` is
    /// required so artifacts are deterministically grouped, and `--output-root`
    /// defaults to [`DEFAULT_OUTPUT_ROOT`]. Parsing is syntactic only: the
    /// workload and topology ids are not checked against fixtures here so the
    /// caller can separate argument errors from unknown-selection errors. A bare
    /// `--bench` (which Cargo appends when launching a `harness = false` bench
    /// target) is accepted and ignored.
    ///
    /// # Errors
    /// Returns [`RunnerCliError::MissingValue`] when a flag has no value,
    /// [`RunnerCliError::UnknownArg`] for an unrecognized argument,
    /// [`RunnerCliError::MissingArg`] when a required flag is absent, and
    /// [`RunnerCliError::Tier`] when `--tier` is not a runner tier.
    pub fn parse<I>(args: I) -> Result<Self, RunnerCliError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut workload_id: Option<String> = None;
        let mut topology_id: Option<String> = None;
        let mut tier: Option<RunnerTier> = None;
        let mut output_root: Option<PathBuf> = None;
        let mut run_id: Option<String> = None;

        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            let mut take_value = |flag: &str| {
                args.next()
                    .ok_or_else(|| RunnerCliError::MissingValue(flag.to_owned()))
            };
            match flag.as_str() {
                "--workload" => workload_id = Some(take_value("--workload")?),
                "--topology" => topology_id = Some(take_value("--topology")?),
                "--tier" => tier = Some(RunnerTier::parse(&take_value("--tier")?)?),
                "--output-root" => output_root = Some(PathBuf::from(take_value("--output-root")?)),
                "--run-id" => run_id = Some(take_value("--run-id")?),
                // Cargo appends `--bench` when launching a `harness = false`
                // benchmark target; it carries no value and is not a runner flag.
                "--bench" => {}
                other => return Err(RunnerCliError::UnknownArg(other.to_owned())),
            }
        }

        Ok(Self {
            workload_id: workload_id
                .ok_or_else(|| RunnerCliError::MissingArg("--workload".to_owned()))?,
            topology_id: topology_id
                .ok_or_else(|| RunnerCliError::MissingArg("--topology".to_owned()))?,
            tier: tier.ok_or_else(|| RunnerCliError::MissingArg("--tier".to_owned()))?,
            output_root: output_root.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_ROOT)),
            run_id: run_id.ok_or_else(|| RunnerCliError::MissingArg("--run-id".to_owned()))?,
        })
    }

    /// Resolve the workload and topology ids against the locked fixtures.
    ///
    /// This is the fail-closed gate a runner calls before starting a cluster: an
    /// id absent from its fixture surfaces a [`RunnerCliError::Selection`] with
    /// no side effect performed.
    ///
    /// # Errors
    /// Returns [`RunnerCliError::Selection`] when the workload or topology id is
    /// not present in the corresponding locked fixture.
    pub fn resolve<'a>(
        &self,
        workloads: &'a WorkloadProfiles,
        topologies: &'a TopologyProfiles,
    ) -> Result<ResolvedInvocation<'a>, RunnerCliError> {
        let workload = workloads.resolve(&self.workload_id)?;
        let topology = topologies.resolve(&self.topology_id)?;
        Ok(ResolvedInvocation {
            workload,
            topology,
            tier: self.tier,
        })
    }
}

/// Failure raised while parsing or resolving the shared runner CLI surface.
///
/// Every variant is a fail-closed condition surfaced before a cluster starts, so
/// a malformed or unknown invocation never consumes cluster, storage, or
/// database resources.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RunnerCliError {
    /// A recognized flag was given without a following value.
    #[error("Bifrost runner flag {0} requires a value")]
    MissingValue(String),
    /// An unrecognized argument was supplied.
    #[error("unknown Bifrost runner argument: {0}")]
    UnknownArg(String),
    /// A required flag was absent.
    #[error("Bifrost runner requires {0}")]
    MissingArg(String),
    /// The `--tier` token was not a runner tier.
    #[error(transparent)]
    Tier(#[from] RunnerPlanError),
    /// A workload or topology id was absent from its locked fixture.
    #[error(transparent)]
    Selection(#[from] BenchmarkSelectionError),
}

/// Failure raised while planning a tiered runner invocation before or during a run.
///
/// Every variant is a fail-closed condition a runner surfaces without leaving a
/// promotable artifact: an unknown selection is rejected before a cluster
/// starts, and a budget breach fails the lane after the run.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RunnerPlanError {
    /// The `--tier` token is not `smoke` or `qualification`.
    #[error("unknown Bifrost benchmark tier: {0}")]
    UnknownTier(String),
    /// The workload profile declares no ladder rung.
    #[error("Bifrost workload {0} declares an empty control ladder")]
    EmptyLadder(String),
    /// The fixture control unit is outside the closed set.
    #[error("unknown Bifrost control unit: {0}")]
    UnknownControlUnit(String),
    /// A `qps` control omitted its required `max_in_flight` cap.
    #[error("Bifrost control unit {0} requires max_in_flight")]
    MissingMaxInFlight(String),
    /// The requested Oracle pod count is not on the locked distributed ladder.
    #[error("unsupported Bifrost Oracle pod count: {0}")]
    UnsupportedPodCount(u16),
    /// The lane exceeded its fixture-locked wall-clock budget.
    #[error("Bifrost lane exceeded its {budget_seconds}s budget after {elapsed_seconds}s")]
    BudgetBreach {
        /// Measured wall-clock seconds from lane start.
        elapsed_seconds: u64,
        /// Fixture-locked budget in seconds that was breached.
        budget_seconds: u32,
    },
}

/// Pure unit tests for tier planning, control mapping, topology selection, and budgets.
#[cfg(test)]
mod tests {
    use super::super::bench_dataset::WorkloadProfiles;
    use super::*;

    /// Resolve one workload profile from the locked fixture for a planning test.
    fn profile(workload_id: &str) -> WorkloadProfile {
        WorkloadProfiles::load()
            .resolve(workload_id)
            .expect("locked workload")
            .clone()
    }

    /// Proves every locked workload classifies to the family its binary serves.
    ///
    /// The `distributed-` prefix must win over the plain `query_id` both those
    /// workloads share, and the mutually exclusive ingest/matrix/mixed shapes must
    /// each classify directly.
    #[test]
    fn family_classification_routes_every_workload() {
        assert_eq!(
            RunnerFamily::classify(&profile("ingest-small")),
            RunnerFamily::Ingest
        );
        assert_eq!(RunnerFamily::classify(&profile("q1")), RunnerFamily::Query);
        assert_eq!(
            RunnerFamily::classify(&profile("distributed-q1")),
            RunnerFamily::Distributed
        );
        assert_eq!(
            RunnerFamily::classify(&profile("mixed-25-25")),
            RunnerFamily::Mixed
        );
        assert_eq!(
            RunnerFamily::classify(&profile("tenants-2")),
            RunnerFamily::Fairness
        );
    }

    /// Proves `--tier` parsing accepts the two runner tiers and rejects others.
    #[test]
    fn tier_parse_accepts_only_runner_tiers() {
        assert_eq!(RunnerTier::parse("smoke"), Ok(RunnerTier::Smoke));
        assert_eq!(
            RunnerTier::parse("qualification"),
            Ok(RunnerTier::Qualification)
        );
        assert_eq!(
            RunnerTier::parse("scale"),
            Err(RunnerPlanError::UnknownTier("scale".to_owned()))
        );
        assert_eq!(RunnerTier::Smoke.benchmark_tier(), BenchmarkTier::Smoke);
        assert_eq!(
            RunnerTier::Qualification.benchmark_tier(),
            BenchmarkTier::Qualification
        );
    }

    /// Proves smoke runs exactly the first ladder rung at smoke durations.
    #[test]
    fn smoke_plans_exactly_the_first_stage() {
        let q1 = profile("q1");
        let stages = plan_stages(&q1, RunnerTier::Smoke).expect("smoke plan");
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].ordinal, 0);
        assert_eq!(stages[0].control_value, q1.control.ladder[0]);
        assert_eq!(stages[0].warmup_seconds, q1.smoke.warmup_seconds);
        assert_eq!(stages[0].measure_seconds, q1.smoke.measure_seconds);
        assert_eq!(stages[0].drain_seconds, q1.smoke.drain_seconds);
    }

    /// Proves qualification runs every ladder rung with contiguous ordinals at full durations.
    #[test]
    fn qualification_plans_the_full_ladder() {
        let q1 = profile("q1");
        let stages = plan_stages(&q1, RunnerTier::Qualification).expect("qualification plan");
        assert_eq!(stages.len(), q1.control.ladder.len());
        for (index, stage) in stages.iter().enumerate() {
            assert_eq!(usize::from(stage.ordinal), index);
            assert_eq!(stage.control_value, q1.control.ladder[index]);
            assert_eq!(stage.measure_seconds, q1.stages.measure_seconds);
        }
    }

    /// Proves each fixture control unit maps onto its family-native controller.
    #[test]
    fn control_mapping_covers_every_fixture_unit() {
        assert_eq!(
            stage_control(&profile("q1").control, 500),
            Ok(StageControl::Qps {
                value: 500,
                max_in_flight: 256
            })
        );
        assert_eq!(
            stage_control(&profile("ingest-small").control, 500),
            Ok(StageControl::RequestsPerSec { value: 500 })
        );
        assert_eq!(
            stage_control(&profile("q3").control, 4),
            Ok(StageControl::LogicalBytesPerSec { value: 4 })
        );
        assert_eq!(
            stage_control(&profile("q5").control, 8),
            Ok(StageControl::ReturnedBytesPerSec { value: 8 })
        );
        assert_eq!(
            stage_control(&profile("tenants-2").control, 16),
            Ok(StageControl::Concurrency { value: 16 })
        );
    }

    /// Proves an unknown control unit and a `qps` control without a cap are rejected.
    #[test]
    fn control_mapping_rejects_invalid_controls() {
        let mut unknown = profile("q1").control;
        unknown.unit = "furlongs_per_fortnight".to_owned();
        assert!(matches!(
            stage_control(&unknown, 1),
            Err(RunnerPlanError::UnknownControlUnit(_))
        ));
        let mut uncapped = profile("q1").control;
        uncapped.max_in_flight = None;
        assert!(matches!(
            stage_control(&uncapped, 1),
            Err(RunnerPlanError::MissingMaxInFlight(_))
        ));
    }

    /// Proves the distributed pod ladder maps 1/2/3/6 and rejects everything else.
    #[test]
    fn topology_selection_covers_the_distributed_ladder() {
        assert_eq!(topology_for_pods(1), Ok(BifrostTopology::OnePod));
        assert_eq!(topology_for_pods(2), Ok(BifrostTopology::TwoPod));
        assert_eq!(topology_for_pods(3), Ok(BifrostTopology::ThreePod));
        assert_eq!(topology_for_pods(6), Ok(BifrostTopology::SixPod));
        assert_eq!(
            topology_for_pods(4),
            Err(RunnerPlanError::UnsupportedPodCount(4))
        );
    }

    /// Build an argument vector from string slices for a CLI parsing test.
    fn args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_owned()).collect()
    }

    /// Proves the shared flag surface parses, applies the output-root default, and resolves fixtures.
    #[test]
    fn cli_parses_and_resolves_a_known_invocation() {
        let invocation = RunnerInvocation::parse(args(&[
            "--workload",
            "q1",
            "--topology",
            "three-pod",
            "--tier",
            "smoke",
            "--run-id",
            "run-2026",
        ]))
        .expect("known invocation parses");
        assert_eq!(invocation.workload_id, "q1");
        assert_eq!(invocation.topology_id, "three-pod");
        assert_eq!(invocation.tier, RunnerTier::Smoke);
        assert_eq!(invocation.run_id, "run-2026");
        assert_eq!(invocation.output_root, PathBuf::from(DEFAULT_OUTPUT_ROOT));

        let workloads = WorkloadProfiles::load();
        let topologies = TopologyProfiles::load();
        let resolved = invocation
            .resolve(&workloads, &topologies)
            .expect("known ids resolve");
        assert_eq!(resolved.workload.workload_id, "q1");
        assert_eq!(resolved.topology.topology_id, "three-pod");
        assert_eq!(resolved.tier, RunnerTier::Smoke);
    }

    /// Proves the Cargo-appended `--bench` flag is accepted and ignored, so the
    /// smoke lane can launch the `harness = false` bench targets directly.
    #[test]
    fn cli_ignores_the_cargo_bench_flag() {
        let invocation = RunnerInvocation::parse(args(&[
            "--bench",
            "--workload",
            "q1",
            "--topology",
            "three-pod",
            "--tier",
            "smoke",
            "--run-id",
            "run-2026",
        ]))
        .expect("invocation with --bench parses");
        assert_eq!(invocation.workload_id, "q1");
    }

    /// Proves an overridden output root is honored over the default.
    #[test]
    fn cli_honors_an_overridden_output_root() {
        let invocation = RunnerInvocation::parse(args(&[
            "--workload",
            "ingest-small",
            "--topology",
            "one-pod",
            "--tier",
            "qualification",
            "--run-id",
            "run-a",
            "--output-root",
            "/custom/root",
        ]))
        .expect("invocation parses");
        assert_eq!(invocation.output_root, PathBuf::from("/custom/root"));
    }

    /// Proves a missing required flag and a value-less flag are rejected before resolution.
    #[test]
    fn cli_rejects_malformed_arguments() {
        assert_eq!(
            RunnerInvocation::parse(args(&[
                "--topology",
                "one-pod",
                "--tier",
                "smoke",
                "--run-id",
                "r"
            ])),
            Err(RunnerCliError::MissingArg("--workload".to_owned()))
        );
        assert_eq!(
            RunnerInvocation::parse(args(&["--workload"])),
            Err(RunnerCliError::MissingValue("--workload".to_owned()))
        );
        assert!(matches!(
            RunnerInvocation::parse(args(&["--frobnicate", "x"])),
            Err(RunnerCliError::UnknownArg(_))
        ));
        assert!(matches!(
            RunnerInvocation::parse(args(&[
                "--workload",
                "q1",
                "--topology",
                "one-pod",
                "--tier",
                "scale",
                "--run-id",
                "r"
            ])),
            Err(RunnerCliError::Tier(RunnerPlanError::UnknownTier(_)))
        ));
    }

    /// Proves an unknown workload or topology id fails closed at resolution.
    #[test]
    fn cli_resolution_rejects_unknown_ids() {
        let workloads = WorkloadProfiles::load();
        let topologies = TopologyProfiles::load();
        let unknown_workload = RunnerInvocation::parse(args(&[
            "--workload",
            "nope",
            "--topology",
            "one-pod",
            "--tier",
            "smoke",
            "--run-id",
            "r",
        ]))
        .expect("parses");
        assert!(matches!(
            unknown_workload.resolve(&workloads, &topologies),
            Err(RunnerCliError::Selection(
                BenchmarkSelectionError::UnknownWorkload(_)
            ))
        ));
        let unknown_topology = RunnerInvocation::parse(args(&[
            "--workload",
            "q1",
            "--topology",
            "nope",
            "--tier",
            "smoke",
            "--run-id",
            "r",
        ]))
        .expect("parses");
        assert!(matches!(
            unknown_topology.resolve(&workloads, &topologies),
            Err(RunnerCliError::Selection(
                BenchmarkSelectionError::UnknownTopology(_)
            ))
        ));
    }

    /// Proves budget enforcement passes within budget and names the budget on breach.
    #[test]
    fn budget_enforcement_names_the_budget_on_breach() {
        let budgets = TierBudgets::load();
        assert!(enforce_budget(Duration::from_secs(1), budgets.smoke_lane_seconds).is_ok());
        assert_eq!(
            RunnerTier::Smoke.lane_budget_seconds(&budgets),
            Some(budgets.smoke_lane_seconds)
        );
        assert_eq!(
            RunnerTier::Qualification.lane_budget_seconds(&budgets),
            None
        );
        let breach = enforce_budget(
            Duration::from_secs(u64::from(budgets.smoke_lane_seconds) + 1),
            budgets.smoke_lane_seconds,
        );
        assert!(matches!(
            breach,
            Err(RunnerPlanError::BudgetBreach {
                budget_seconds,
                ..
            }) if budget_seconds == budgets.smoke_lane_seconds
        ));
    }
}
