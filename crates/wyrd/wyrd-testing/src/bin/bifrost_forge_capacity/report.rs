//! The benchmark's report: host and its load, settings, the resource plans
//! the processes resolved, the in-process rate sweep, the live leader probe,
//! every measured window, per-step summaries, the bounding resource, and the
//! gating checks.
//!
//! A check fails the run when its measurement misses the threshold or was not
//! measured at all. Thresholds are the Forge capacity scenario's and are never
//! relaxed here.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use wyrd_testing::capacity::Percentiles;

use crate::Result;
use crate::deployment::{PullEvidence, WindowRecord};
use crate::host::{Host, HostLoad};
use crate::probe::ProbeStep;
use crate::schedule::{DECISION_P99_US, SweepStep};

/// Required 2-worker over 1-worker completion-rate ratio.
const TWO_WORKER_SCALING: f64 = 1.7;
/// Required 4-worker over 1-worker completion-rate ratio.
const FOUR_WORKER_SCALING: f64 = 3.0;
/// Leader CPU share of its envelope that must not be reached.
const LEADER_CPU_SHARE: f64 = 0.70;
/// Multiples of the production rate whose p99 decisions are gated.
const GATED_MULTIPLIERS: [f64; 2] = [1.0, 10.0];

/// Workload settings the report was measured under.
#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// Independent tables committing and compacting.
    pub tables: usize,
    /// Tenants owning them.
    pub tenants: usize,
    /// Scribe seal age, seconds (`WYRD_MAX_FILE_RETENTION_TIME`).
    pub seal_seconds: u64,
    /// Interval between one table's writes, milliseconds.
    pub write_interval_ms: u64,
    /// Rows per write.
    pub rows_per_write: usize,
    /// Promotion commits that make a table due.
    pub trigger_snapshot_count: usize,
    /// Leader scope: CPU percent and memory bytes.
    pub leader_envelope: (u64, u64),
    /// Each worker's scope: CPU percent and memory bytes.
    pub worker_envelope: (u64, u64),
    /// Warmup before a step's first window, seconds.
    pub warmup_seconds: f64,
    /// Measured window length, seconds.
    pub window_seconds: f64,
    /// Windows measured per step.
    pub windows: usize,
}

/// What each process resolved from its scope, read from its start log.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResourcePlans {
    /// The leader's effective CPUs, from its Oracle admission line.
    pub leader_effective_cpu: Option<u64>,
    /// The leader's managed memory, bytes.
    pub leader_managed_memory_bytes: Option<u64>,
    /// Each worker's local task parallelism, from its start line.
    pub worker_max_task_parallelism: Vec<Option<u64>>,
}

impl ResourcePlans {
    /// Reads `field=<integer>` from the first line of `log` containing
    /// `message`.
    pub fn field(log: &str, message: &str, field: &str) -> Option<u64> {
        log.lines()
            .find(|line| line.contains(message))?
            .split_whitespace()
            .find_map(|token| token.strip_prefix(field)?.strip_prefix('='))?
            .parse()
            .ok()
    }
}

/// One worker count's windows, summarized.
#[derive(Debug, Clone, Serialize)]
pub struct StepSummary {
    /// Workers serving.
    pub workers: usize,
    /// Median window completion rate, rewrites per second.
    pub rewrites_per_second: f64,
    /// Lowest and highest window completion rates.
    pub rewrites_per_second_range: (f64, f64),
    /// Median window promotion commit rate.
    pub promotion_commits_per_second: f64,
    /// Highest window leader CPU share.
    pub leader_cpu_share: f64,
    /// Mean worker CPU share of its envelope.
    pub worker_cpu_share: f64,
    /// Median completion rate over the workers' pull ceiling.
    pub pull_cadence_share: f64,
    /// Mean RustFS cores.
    pub rustfs_cores: f64,
    /// Mean Postgres cores.
    pub postgres_cores: f64,
    /// Every pull the leader served in the step's windows.
    pub pulls: PullEvidence,
}

impl StepSummary {
    /// Summarizes the windows measured with `workers` workers; `None` when
    /// none were.
    fn of(windows: &[WindowRecord], workers: usize) -> Option<Self> {
        let step: Vec<&WindowRecord> = windows.iter().filter(|w| w.workers == workers).collect();
        if step.is_empty() {
            return None;
        }
        let median = |pick: fn(&WindowRecord) -> f64| {
            let mut values: Vec<f64> = step.iter().map(|w| pick(w)).collect();
            values.sort_by(f64::total_cmp);
            values[(values.len() - 1) / 2]
        };
        let mean = |values: Vec<f64>| values.iter().sum::<f64>() / values.len().max(1) as f64;
        let mut rates: Vec<f64> = step.iter().map(|w| w.rewrites_per_second).collect();
        rates.sort_by(f64::total_cmp);
        let mut pulls = PullEvidence::default();
        for window in &step {
            pulls.add(window.pulls);
        }
        Some(Self {
            workers,
            rewrites_per_second: rates[(rates.len() - 1) / 2],
            rewrites_per_second_range: (rates[0], rates[rates.len() - 1]),
            promotion_commits_per_second: median(|w| w.promotion_commits_per_second),
            leader_cpu_share: step.iter().map(|w| w.leader_cpu_share).fold(0.0, f64::max),
            worker_cpu_share: mean(
                step.iter()
                    .flat_map(|w| w.per_worker.iter().map(|worker| worker.cpu_share))
                    .collect(),
            ),
            pull_cadence_share: median(|w| {
                w.rewrites_per_second / w.pull_ceiling_per_second.max(f64::EPSILON)
            }),
            rustfs_cores: mean(
                step.iter()
                    .map(|w| w.dependencies.rustfs_cores.unwrap_or(0.0))
                    .collect(),
            ),
            postgres_cores: mean(
                step.iter()
                    .map(|w| w.dependencies.postgres_cores.unwrap_or(0.0))
                    .collect(),
            ),
            pulls,
        })
    }

    /// Every candidate bound's utilization, highest first: compactor CPU of
    /// its envelope, RustFS and Postgres cores of the CPUs left outside the
    /// Wyrd envelope, the workers' pull cadence, and the due-table supply
    /// (the share of pulls the leader answered short because nothing else
    /// was due).
    fn bounds(&self, spare_cpus: f64) -> Vec<(&'static str, f64)> {
        let spare = spare_cpus.max(1.0);
        let mut bounds = vec![
            ("compactor CPU", self.worker_cpu_share),
            ("RustFS", self.rustfs_cores / spare),
            ("Postgres", self.postgres_cores / spare),
            ("worker pull cadence", self.pull_cadence_share),
            (
                "due-table supply",
                self.pulls.short as f64 / self.pulls.pulls.max(1) as f64,
            ),
        ];
        bounds.sort_by(|a, b| b.1.total_cmp(&a.1));
        bounds
    }
}

/// One gating check.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// What is checked.
    pub name: String,
    /// The measurement, when there was one.
    pub measured: Option<f64>,
    /// The bound, in words.
    pub threshold: String,
    /// Whether the measurement met the bound.
    pub pass: bool,
}

impl Check {
    /// A check that passes when `measured` is present and `within` holds.
    fn new(
        name: &str,
        measured: Option<f64>,
        threshold: &str,
        within: impl Fn(f64) -> bool,
    ) -> Self {
        Self {
            name: name.to_owned(),
            pass: measured.is_some_and(within),
            measured,
            threshold: threshold.to_owned(),
        }
    }
}

/// The complete report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The machine.
    pub host: Host,
    /// Host load sampled before the run.
    pub load_before: HostLoad,
    /// Host load sampled after the run.
    pub load_after: HostLoad,
    /// The measured server binary's identity.
    pub binary: serde_json::Value,
    /// Workload settings.
    pub settings: Settings,
    /// What each process resolved from its scope.
    pub resource_plans: ResourcePlans,
    /// The in-process open-loop rate sweep.
    pub sweep: Vec<SweepStep>,
    /// Offered pulls per second at the knee: the last swept step whose p99
    /// stayed under the bound at the offered rate.
    pub knee_pulls_per_second: Option<f64>,
    /// The live leader under open-loop peer pulls.
    pub probe: Vec<ProbeStep>,
    /// Every measured window, in order.
    pub windows: Vec<WindowRecord>,
    /// One summary per worker count.
    pub steps: Vec<StepSummary>,
    /// The largest step's candidate bounds, highest utilization first.
    pub bounds: Vec<(&'static str, f64)>,
    /// Every gating check.
    pub checks: Vec<Check>,
}

/// Everything the report is built from.
pub struct Evidence {
    /// The machine.
    pub host: Host,
    /// Host load before the run.
    pub load_before: HostLoad,
    /// Host load after the run.
    pub load_after: HostLoad,
    /// The binary's identity.
    pub binary: serde_json::Value,
    /// Workload settings.
    pub settings: Settings,
    /// Resolved resource plans.
    pub resource_plans: ResourcePlans,
    /// The in-process sweep.
    pub sweep: Vec<SweepStep>,
    /// The live probe.
    pub probe: Vec<ProbeStep>,
    /// Every window.
    pub windows: Vec<WindowRecord>,
}

impl Report {
    /// Summarizes `evidence` for each worker count of `ladder` and evaluates
    /// every check.
    pub fn new(evidence: Evidence, ladder: &[usize]) -> Self {
        let Evidence {
            host,
            load_before,
            load_after,
            binary,
            settings,
            resource_plans,
            sweep,
            probe,
            windows,
        } = evidence;
        let steps: Vec<StepSummary> = ladder
            .iter()
            .filter_map(|workers| StepSummary::of(&windows, *workers))
            .collect();
        let knee_pulls_per_second = sweep
            .iter()
            .take_while(|step| step.kept())
            .last()
            .map(|step| step.offered_pulls_per_second);
        let envelope_cpus = (settings.leader_envelope.0
            + settings.worker_envelope.0 * ladder.iter().copied().max().unwrap_or(0) as u64)
            as f64
            / 100.0;
        let bounds = steps
            .last()
            .map(|step| step.bounds(host.cpus as f64 - envelope_cpus))
            .unwrap_or_default();
        let checks = Self::checks(&sweep, &probe, &steps);
        Self {
            host,
            load_before,
            load_after,
            binary,
            settings,
            resource_plans,
            sweep,
            knee_pulls_per_second,
            probe,
            windows,
            steps,
            bounds,
            checks,
        }
    }

    /// Evaluates every gate.
    fn checks(sweep: &[SweepStep], probe: &[ProbeStep], steps: &[StepSummary]) -> Vec<Check> {
        let mut checks = Vec::new();
        let p99 = |name: String, measured: Option<f64>| {
            Check::new(&name, measured, "< 1000", |us| us < DECISION_P99_US)
        };
        for multiplier in GATED_MULTIPLIERS {
            let swept = sweep.iter().find(|step| step.multiplier == multiplier);
            for (operation, pick) in [
                (
                    "commit",
                    (|s: &SweepStep| s.commit_us.p99) as fn(&SweepStep) -> Option<f64>,
                ),
                ("pull", |s: &SweepStep| s.pull_us.p99),
                ("report", |s: &SweepStep| s.report_us.p99),
            ] {
                checks.push(p99(
                    format!("in-process p99 {operation} at {multiplier}x (µs)"),
                    swept.and_then(pick),
                ));
            }
            let live = probe.iter().find(|step| step.multiplier == multiplier);
            for operation in ["commit", "pull", "report"] {
                let measured = live.and_then(|step| match operation {
                    "commit" => step.leader.commit.us.p99,
                    "pull" => step.leader.pull.us.p99,
                    _ => step.leader.report.us.p99,
                });
                checks.push(p99(
                    format!("live leader p99 {operation} at {multiplier}x (µs)"),
                    measured,
                ));
            }
        }
        let rate = |workers: usize| {
            steps
                .iter()
                .find(|step| step.workers == workers)
                .map(|step| step.rewrites_per_second)
        };
        let ratio = |workers: usize| {
            let base = rate(1).filter(|base| *base > 0.0)?;
            Some(rate(workers)? / base)
        };
        checks.push(Check::new(
            "2-worker / 1-worker completion rate",
            ratio(2),
            ">= 1.7",
            |ratio| ratio >= TWO_WORKER_SCALING,
        ));
        checks.push(Check::new(
            "4-worker / 1-worker completion rate",
            ratio(4),
            ">= 3.0",
            |ratio| ratio >= FOUR_WORKER_SCALING,
        ));
        let mut pulls = PullEvidence::default();
        for step in steps {
            pulls.add(step.pulls);
        }
        checks.push(Check::new(
            "pulls answered short while a table stayed due",
            (pulls.pulls_with_due > 0).then_some(pulls.short_while_due as f64),
            "= 0 over at least one pull that found due tables",
            |short| short == 0.0,
        ));
        checks.push(Check::new(
            "highest leader CPU share",
            (!steps.is_empty()).then(|| {
                steps
                    .iter()
                    .map(|step| step.leader_cpu_share)
                    .fold(0.0, f64::max)
            }),
            "< 0.70",
            |share| share < LEADER_CPU_SHARE,
        ));
        checks
    }

    /// Whether every check passed.
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.pass)
    }

    /// Writes `report.json` and `report.txt` into `directory`.
    ///
    /// # Errors
    ///
    /// Returns a serialization or write failure.
    pub fn write(&self, directory: &Path) -> Result<()> {
        std::fs::write(
            directory.join("report.json"),
            serde_json::to_vec_pretty(self)?,
        )?;
        std::fs::write(directory.join("report.txt"), self.text())?;
        Ok(())
    }

    /// The human-readable report.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let host = &self.host;
        let _ = writeln!(out, "Forge capacity benchmark");
        let _ = writeln!(
            out,
            "host: {} | {} CPUs | {:.1} GiB | kernel {}",
            host.cpu_model,
            host.cpus,
            host.memory_bytes as f64 / f64::from(1 << 30),
            host.kernel
        );
        let _ = writeln!(out, "host load before: {}", self.load_before.describe());
        let _ = writeln!(out, "host load after:  {}", self.load_after.describe());
        let settings = &self.settings;
        let gib = |bytes: u64| bytes as f64 / f64::from(1 << 30);
        let _ = writeln!(
            out,
            "workload: {} tables, {} tenant, seal {} s, one {}-row write per table every {} ms, \
             compaction on at the default type and target, due every {} promotion commits; \
             leader {:.1} CPU / {:.0} GiB, each worker {:.1} CPU / {:.0} GiB; \
             warmup {} s, {} windows of {} s per step",
            settings.tables,
            settings.tenants,
            settings.seal_seconds,
            settings.rows_per_write,
            settings.write_interval_ms,
            settings.trigger_snapshot_count,
            settings.leader_envelope.0 as f64 / 100.0,
            gib(settings.leader_envelope.1),
            settings.worker_envelope.0 as f64 / 100.0,
            gib(settings.worker_envelope.1),
            settings.warmup_seconds,
            settings.windows,
            settings.window_seconds
        );
        let plans = &self.resource_plans;
        let _ = writeln!(
            out,
            "resolved plans: leader effective_cpu {} managed memory {} | worker max_task_parallelism {:?}",
            optional_int(plans.leader_effective_cpu),
            optional_int(plans.leader_managed_memory_bytes),
            plans.worker_max_task_parallelism
        );

        let _ = writeln!(
            out,
            "\nin-process schedule, open loop (10,000 tracked / 1,000 due, 32 compactors x 4 tasks):"
        );
        for step in &self.sweep {
            let _ = writeln!(
                out,
                "  {:>7}x {:>9.1}/{:<9.1} pulls/s | p50/p99 µs commit {} pull {} report {} | {} {}",
                step.multiplier,
                step.achieved_pulls_per_second,
                step.offered_pulls_per_second,
                pair(step.commit_us),
                pair(step.pull_us),
                pair(step.report_us),
                format_args!("full {}/{}", step.full_pulls, step.pulls),
                if step.kept() { "kept" } else { "BENT" }
            );
        }
        let _ = writeln!(
            out,
            "  knee: {} pulls/s",
            optional(self.knee_pulls_per_second)
        );

        let _ = writeln!(out, "\nlive leader, open-loop peer pulls (no compactors):");
        for step in &self.probe {
            let leader = &step.leader;
            let _ = writeln!(
                out,
                "  {:>4}x {:.2}/{:.2} pulls/s, full {}/{}, {} errors | leader p50/p99 µs commit {} ({:.1}/s) pull {} report {} | client round trip {}",
                step.multiplier,
                step.achieved_pulls_per_second,
                step.offered_pulls_per_second,
                step.full_pulls,
                step.pulls,
                step.errors,
                pair(leader.commit.us),
                leader.commit.per_second,
                pair(leader.pull.us),
                pair(leader.report.us),
                pair(step.round_trip_us)
            );
        }

        let _ = writeln!(out, "\nwindows:");
        for window in &self.windows {
            let deps = &window.dependencies;
            let pulls = &window.pulls;
            let _ = writeln!(
                out,
                "  {} worker(s), {:.0} s: {:.2} rewrites/s (in {:.1} files/s, {:.0} KiB/s; out {:.1} files/s; {} unsuccessful) \
                 rewrite p50/p99 {} s | promotions {:.2}/s p50/p99 {} s | pull ceiling {:.2}/s | writes {:.0}/s ({} refused)",
                window.workers,
                window.seconds,
                window.rewrites_per_second,
                window.input_files_per_second,
                window.input_bytes_per_second / 1024.0,
                window.output_files_per_second,
                window.unsuccessful_attempts,
                pair(window.rewrite_seconds),
                window.promotion_commits_per_second,
                pair(window.promotion_seconds),
                window.pull_ceiling_per_second,
                window.writes_per_second,
                window.refused_writes
            );
            let decisions = &window.leader_decisions;
            let _ = writeln!(
                out,
                "    leader decisions p50/p99 µs: commit {} ({:.1}/s) | pull {} ({:.2}/s) | report {} ({:.2}/s)",
                pair(decisions.commit.us),
                decisions.commit.per_second,
                pair(decisions.pull.us),
                decisions.pull.per_second,
                pair(decisions.report.us),
                decisions.report.per_second
            );
            let _ = writeln!(
                out,
                "    pulls served {}: {} found due, {} short, {} short while due; tasks {}/{}",
                pulls.pulls,
                pulls.pulls_with_due,
                pulls.short,
                pulls.short_while_due,
                pulls.returned,
                pulls.requested
            );
            let workers: Vec<String> = window
                .per_worker
                .iter()
                .map(|worker| {
                    format!(
                        "#{} {:.2}/s cpu {:.0}% active {:.2}",
                        worker.replica,
                        worker.rewrites_per_second,
                        worker.cpu_share * 100.0,
                        worker.mean_active_tasks
                    )
                })
                .collect();
            let memory: Vec<String> = window
                .resources
                .iter()
                .map(|resource| {
                    format!(
                        "#{} {:.0} MiB",
                        resource.replica,
                        resource.peak_memory as f64 / f64::from(1 << 20)
                    )
                })
                .collect();
            let _ = writeln!(
                out,
                "    leader CPU {:.1}% | workers [{}] | peak memory [{}]",
                window.leader_cpu_share * 100.0,
                workers.join(", "),
                memory.join(", ")
            );
            let _ = writeln!(
                out,
                "    postgres {:.0} xact/s, {:.0} blk read/s, {:.0} tup written/s, {:.1} active backends, {} cores | rustfs {} cores",
                deps.postgres_transactions_per_second,
                deps.postgres_blocks_read_per_second,
                deps.postgres_tuples_written_per_second,
                deps.postgres_active_backends,
                optional(deps.postgres_cores),
                optional(deps.rustfs_cores)
            );
        }

        let _ = writeln!(out, "\nsteps:");
        for step in &self.steps {
            let _ = writeln!(
                out,
                "  {} worker(s): median {:.2} rewrites/s (range {:.2}-{:.2}), promotions {:.2}/s, leader CPU max {:.1}%, \
                 worker CPU {:.0}%, pull cadence {:.0}%, pulls {} ({} short, {} short while due), rustfs {:.2} / postgres {:.2} cores",
                step.workers,
                step.rewrites_per_second,
                step.rewrites_per_second_range.0,
                step.rewrites_per_second_range.1,
                step.promotion_commits_per_second,
                step.leader_cpu_share * 100.0,
                step.worker_cpu_share * 100.0,
                step.pull_cadence_share * 100.0,
                step.pulls.pulls,
                step.pulls.short,
                step.pulls.short_while_due,
                step.rustfs_cores,
                step.postgres_cores
            );
        }
        let bounds: Vec<String> = self
            .bounds
            .iter()
            .map(|(name, share)| format!("{name} {:.0}%", share * 100.0))
            .collect();
        let _ = writeln!(
            out,
            "largest step bound: {} (utilizations: {})",
            self.bounds.first().map_or("-", |(name, _)| name),
            bounds.join(", ")
        );

        let _ = writeln!(out, "\nchecks:");
        for check in &self.checks {
            let _ = writeln!(
                out,
                "  {} {}: {} (needs {})",
                if check.pass { "PASS" } else { "FAIL" },
                check.name,
                optional(check.measured),
                check.threshold
            );
        }
        out
    }
}

/// `p50/p99` of `percentiles`, `-` where absent.
fn pair(percentiles: Percentiles) -> String {
    let one = |value: Option<f64>| value.map_or("-".to_owned(), |v| format!("{v:.3}"));
    format!("{}/{}", one(percentiles.p50), one(percentiles.p99))
}

/// A measurement to three decimals, `-` when absent.
fn optional(value: Option<f64>) -> String {
    value.map_or("-".to_owned(), |v| format!("{v:.3}"))
}

/// An integer measurement, `-` when absent.
fn optional_int(value: Option<u64>) -> String {
    value.map_or("-".to_owned(), |v| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::ResourcePlans;

    /// A start line's integer field is read from the first matching line
    /// only.
    ///
    /// # Panics
    ///
    /// Panics when the field is misread.
    #[test]
    fn resource_plan_field_reads_the_first_matching_line() {
        let log = "a: other effective_cpu=9\n\
                   b: Oracle admission capacity resolved running_slots=2 effective_cpu=1 managed_memory_bytes=1610612736\n";
        let read = |field| ResourcePlans::field(log, "Oracle admission capacity resolved", field);
        assert_eq!(read("effective_cpu"), Some(1));
        assert_eq!(read("managed_memory_bytes"), Some(1_610_612_736));
        assert_eq!(read("absent"), None);
    }
}
