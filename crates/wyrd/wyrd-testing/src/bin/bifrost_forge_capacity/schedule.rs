//! The leader schedule, measured in-process under open-loop pull arrivals.
//!
//! The leader's peer routes are a term check around one [`ForgeSchedule`]
//! operation, so driving the schedule directly measures the leader decision
//! itself. Ten thousand tables are tracked and one thousand are due. A fleet
//! of [`FLEET_WORKERS`] compactors each pulls [`PULL_LIMIT`] tasks once per
//! pull interval; one OS thread plays each compactor, its arrivals fixed in
//! advance and evenly staggered across the fleet, so a slow decision does not
//! slow the offered rate (open loop). Every dispatched task is reported
//! succeeded and its table immediately receives a new commit that makes it due
//! again, so the due set stays at one thousand and each arrival exercises one
//! pull, its reports, and their commit notices. The sweep offers the
//! production rate, then 10x, then increasing multiples until the leader no
//! longer keeps its p99 under the bound or the offered rate; the last step
//! that did is the knee.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use vala_bifrost_redux::forge::{
    DEFAULT_REPORT_TIMEOUT, ForgeCommitNotice, ForgeCompactionOutcome, ForgeSchedule,
    ForgeTableKey, ForgeTableSettings, ForgeWorkerConfig,
};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_spec::DataTenantId;
use wyrd_testing::capacity::Percentiles;

use crate::Result;

/// Tasks one compactor pull requests, as a worker with free room asks.
pub const PULL_LIMIT: usize = 4;

/// Compactors in the production fleet the arrival rate models.
pub const FLEET_WORKERS: usize = 32;

/// Decision p99 bound, microseconds, that the production and 10x rates must
/// meet and that ends the sweep.
pub const DECISION_P99_US: f64 = 1_000.0;

/// Share of the offered pull rate a step must achieve to count as kept.
const ACHIEVED_SHARE: f64 = 0.95;

/// Rate multiples of the production pull rate, in sweep order.
const MULTIPLIERS: [f64; 8] = [1.0, 10.0, 30.0, 100.0, 300.0, 1_000.0, 3_000.0, 10_000.0];

/// The production pull rate: every compactor pulls once per pull interval.
pub fn production_pulls_per_second() -> f64 {
    FLEET_WORKERS as f64 / ForgeWorkerConfig::default().pull_interval.as_secs_f64()
}

/// The schedule the sweep drives and how long each step runs.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RateSweep {
    /// Tables the leader tracks.
    pub tables: usize,
    /// Tracked tables kept due throughout.
    pub due: usize,
    /// Seconds the production-rate step runs.
    pub production_seconds: f64,
    /// Seconds the 10x step runs.
    pub tenfold_seconds: f64,
    /// Seconds every later step runs.
    pub step_seconds: f64,
}

/// One offered rate's measurement.
#[derive(Debug, Clone, Serialize)]
pub struct SweepStep {
    /// Offered rate as a multiple of the production rate.
    pub multiplier: f64,
    /// Offered pulls per second.
    pub offered_pulls_per_second: f64,
    /// Pulls served per second.
    pub achieved_pulls_per_second: f64,
    /// Pulls served.
    pub pulls: usize,
    /// Pulls answered with [`PULL_LIMIT`] tasks.
    pub full_pulls: usize,
    /// Commit-notice latency, microseconds.
    pub commit_us: Percentiles,
    /// Pull latency, microseconds.
    pub pull_us: Percentiles,
    /// Report latency, microseconds.
    pub report_us: Percentiles,
}

impl SweepStep {
    /// The worst p99 of the three decisions, microseconds.
    pub fn worst_p99_us(&self) -> Option<f64> {
        [self.commit_us.p99, self.pull_us.p99, self.report_us.p99]
            .into_iter()
            .try_fold(0.0_f64, |worst, p99| Some(worst.max(p99?)))
    }

    /// Whether the leader kept this step: every p99 under the bound and the
    /// offered rate achieved.
    pub fn kept(&self) -> bool {
        self.worst_p99_us().is_some_and(|us| us < DECISION_P99_US)
            && self.achieved_pulls_per_second >= ACHIEVED_SHARE * self.offered_pulls_per_second
    }
}

/// One compactor thread's raw samples, nanoseconds.
#[derive(Default)]
struct Samples {
    /// Pull latencies.
    pull: Vec<u64>,
    /// Report latencies.
    report: Vec<u64>,
    /// Commit-notice latencies.
    commit: Vec<u64>,
    /// Pulls answered in full.
    full: usize,
}

/// A schedule with `tables` tracked and `due` kept due, and the keys and
/// settings that re-arm it.
struct Loaded {
    /// The schedule under test.
    schedule: ForgeSchedule,
    /// Settings that make a table due on every commit.
    due: ForgeTableSettings,
    /// Snapshot ids, increasing across every commit notice.
    snapshot: AtomicI64,
}

impl RateSweep {
    /// Runs the sweep: the production rate, 10x, then each larger multiple
    /// until a step is not kept.
    ///
    /// # Errors
    ///
    /// Returns an invalid generated table identity.
    ///
    /// # Panics
    ///
    /// Panics when a compactor thread panics.
    pub fn run(&self) -> Result<Vec<SweepStep>> {
        let mut steps = Vec::new();
        for (index, multiplier) in MULTIPLIERS.into_iter().enumerate() {
            let seconds = match index {
                0 => self.production_seconds,
                1 => self.tenfold_seconds,
                _ => self.step_seconds,
            };
            let step = self.step(multiplier, seconds)?;
            eprintln!(
                "schedule {multiplier}x: {:.1} of {:.1} pulls/s, p99 commit/pull/report {:?}/{:?}/{:?} µs",
                step.achieved_pulls_per_second,
                step.offered_pulls_per_second,
                step.commit_us.p99,
                step.pull_us.p99,
                step.report_us.p99
            );
            let kept = step.kept();
            steps.push(step);
            if !kept && index >= 1 {
                break;
            }
        }
        Ok(steps)
    }

    /// Offers `multiplier` times the production rate for `seconds` to a
    /// fresh schedule.
    ///
    /// # Errors
    ///
    /// Returns an invalid generated table identity.
    ///
    /// # Panics
    ///
    /// Panics when a compactor thread panics.
    fn step(&self, multiplier: f64, seconds: f64) -> Result<SweepStep> {
        let loaded = self.load()?;
        let period = ForgeWorkerConfig::default()
            .pull_interval
            .div_f64(multiplier);
        let window = Duration::from_secs_f64(seconds);
        let started = Instant::now();
        let samples = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..FLEET_WORKERS)
                .map(|worker| {
                    let first = started + period.mul_f64(worker as f64 / FLEET_WORKERS as f64);
                    let loaded = &loaded;
                    scope.spawn(move || loaded.compactor(first, period, started + window))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a compactor thread does not panic"))
                .collect::<Vec<_>>()
        });
        let elapsed = started.elapsed().as_secs_f64().max(seconds);
        let pooled = |pick: fn(&Samples) -> &[u64]| {
            let all: Vec<u64> = samples
                .iter()
                .flat_map(|s| pick(s).iter().copied())
                .collect();
            Percentiles::raw(&all, 1e-3)
        };
        let pulls: usize = samples.iter().map(|s| s.pull.len()).sum();
        Ok(SweepStep {
            multiplier,
            offered_pulls_per_second: production_pulls_per_second() * multiplier,
            achieved_pulls_per_second: pulls as f64 / elapsed,
            pulls,
            full_pulls: samples.iter().map(|s| s.full).sum(),
            commit_us: pooled(|s| s.commit.as_slice()),
            pull_us: pooled(|s| s.pull.as_slice()),
            report_us: pooled(|s| s.report.as_slice()),
        })
    }

    /// Builds a schedule tracking [`Self::tables`] tables, the first
    /// [`Self::due`] of them due.
    ///
    /// # Errors
    ///
    /// Returns an invalid generated table identity.
    fn load(&self) -> Result<Loaded> {
        let tenant = DataTenantId::try_from(uuid::Uuid::now_v7())?;
        let loaded = Loaded {
            schedule: ForgeSchedule::new(DEFAULT_REPORT_TIMEOUT),
            due: ForgeTableSettings {
                compaction_enabled: true,
                trigger_snapshot_count: 1,
                ..ForgeTableSettings::default()
            },
            snapshot: AtomicI64::new(1),
        };
        let idle = ForgeTableSettings {
            compaction_enabled: true,
            ..ForgeTableSettings::default()
        };
        for index in 0..self.tables {
            let key = ForgeTableKey {
                tenant,
                table: ForgeTaskTableIdentity::new(
                    "wyrd-redux",
                    BifrostNamespace::Datasets.as_str(),
                    format!("schedule_{index}"),
                )?,
            };
            let settings = if index < self.due { &loaded.due } else { &idle };
            loaded.notify(key, settings.clone());
        }
        Ok(loaded)
    }
}

impl Loaded {
    /// Applies one commit notice for `key` with the next snapshot id.
    fn notify(&self, key: ForgeTableKey, settings: ForgeTableSettings) {
        self.schedule.notify_commit(
            ForgeCommitNotice {
                key,
                snapshot_id: self.snapshot.fetch_add(1, Ordering::Relaxed),
                settings,
            },
            Utc::now(),
        );
    }

    /// Plays one compactor: pulls at `first` and every `period` after it
    /// until `end`, reports each dispatched task succeeded, and re-arms its
    /// table with a commit, timing every decision.
    fn compactor(&self, first: Instant, period: Duration, end: Instant) -> Samples {
        let mut samples = Samples::default();
        let mut arrival = first;
        while arrival < end {
            if let Some(wait) = arrival.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            let timer = Instant::now();
            let tasks = self.schedule.pull(PULL_LIMIT, Utc::now());
            samples.pull.push(nanos(timer.elapsed()));
            samples.full += usize::from(tasks.len() == PULL_LIMIT);
            for task in tasks {
                let timer = Instant::now();
                self.schedule.report(
                    &task.key,
                    task.task_id,
                    ForgeCompactionOutcome::Succeeded,
                    Utc::now(),
                );
                samples.report.push(nanos(timer.elapsed()));
                let timer = Instant::now();
                self.notify(task.key, self.due.clone());
                samples.commit.push(nanos(timer.elapsed()));
            }
            arrival += period;
        }
        samples
    }
}

/// `elapsed` in whole nanoseconds, saturating.
fn nanos(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{PULL_LIMIT, RateSweep};

    /// A short step at a high multiple keeps every pull full, because each
    /// report re-arms its table, and times one report and one commit per
    /// dispatched task.
    ///
    /// # Panics
    ///
    /// Panics when the step fails or miscounts.
    #[test]
    fn open_loop_step_keeps_the_due_set_full() {
        let sweep = RateSweep {
            tables: 200,
            due: 40,
            production_seconds: 0.2,
            tenfold_seconds: 0.2,
            step_seconds: 0.2,
        };
        let step = sweep.step(100.0, 0.2).expect("the step runs");
        assert!(step.pulls > 0, "the fleet pulled");
        assert_eq!(step.full_pulls, step.pulls, "the due set never drained");
        assert!(step.pull_us.p99.is_some());
        assert!(step.report_us.p99.is_some() && step.commit_us.p99.is_some());
        assert_eq!(PULL_LIMIT, 4);
    }
}
