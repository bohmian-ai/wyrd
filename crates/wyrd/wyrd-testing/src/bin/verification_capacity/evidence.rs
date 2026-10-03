//! Server-side evidence of one step: exporter histogram deltas summed across
//! replicas, and the durable run rows read by the database owner.
//!
//! Histogram percentiles are bucket estimates: the upper bound of the
//! cumulative bucket covering the quantile, computed from the step's bucket
//! deltas after summing every replica's buckets, never by averaging replica
//! percentiles or subtracting one percentile from another. Raw percentiles
//! come from per-request samples.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
pub use wyrd_testing::capacity::Percentiles;
use wyrd_testing::capacity::{deltas, merged};
use wyrd_testing::release_server::Metrics;

use crate::Result;
use crate::fixture::Kind;
use crate::load::Mode;

/// Phases of `wyrd_verification_phase_duration_seconds`, in waterfall order.
pub const PHASES: [&str; 6] = [
    "load",
    "input_read",
    "prepare",
    "engine",
    "publication",
    "settlement",
];

/// Engine-overhead bucket bound under which a sample is below the AC-040
/// 10 ms objective: the shared bucket set's `0.009`.
pub const OVERHEAD_BOUND: f64 = 0.009;

/// Every replica's `/metrics` scrapes at both step boundaries, in ordinal
/// order.
pub struct Scrapes {
    /// At the step start.
    pub before: Vec<Metrics>,
    /// After the step drained.
    pub after: Vec<Metrics>,
}

/// Exporter evidence for one workload on one path over one step.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ServerStats {
    /// Attempts started, summed across replicas.
    pub attempts: f64,
    /// Attempts started on each replica.
    pub attempts_per_replica: Vec<f64>,
    /// Attempt or direct-runtime duration, bucket estimates in seconds.
    pub run_duration: Percentiles,
    /// p95 of each [`PHASES`] entry that recorded samples, bucket estimates
    /// in seconds; phases overlap and never add up.
    pub phase_p95: BTreeMap<String, Option<f64>>,
    /// Paired engine overhead, bucket estimates in seconds.
    pub overhead: Percentiles,
    /// Engine-overhead samples.
    pub overhead_samples: f64,
    /// Share of engine-overhead samples at or below [`OVERHEAD_BOUND`].
    pub overhead_within_bound: Option<f64>,
    /// Queued claimable-to-claim wait, bucket estimates in seconds.
    pub queue_wait: Percentiles,
    /// Queued creation-to-terminal latency, bucket estimates in seconds.
    pub trigger_to_terminal: Percentiles,
}

impl Scrapes {
    /// Exporter evidence for `kind` on `mode` between the two scrapes.
    pub fn stats(&self, kind: Kind, mode: Mode) -> ServerStats {
        let kind_label = format!("kind=\"{}\"", kind.label());
        let mode_label = format!("mode=\"{}\"", mode.label());
        let labels = [kind_label.as_str(), mode_label.as_str()];
        let histogram = |family: &str, labels: &[&str]| {
            Percentiles::buckets(
                &merged(&self.before, family, labels),
                &merged(&self.after, family, labels),
            )
        };
        let attempts_per_replica: Vec<f64> = self
            .before
            .iter()
            .zip(&self.after)
            .map(|(before, after)| {
                after.sum("wyrd_verification_run_attempts_total", &labels)
                    - before.sum("wyrd_verification_run_attempts_total", &labels)
            })
            .collect();
        let phase_p95 = PHASES
            .iter()
            .map(|phase| {
                let phase_label = format!("phase=\"{phase}\"");
                let mut phase_labels = labels.to_vec();
                phase_labels.push(&phase_label);
                let p95 = histogram("wyrd_verification_phase_duration_seconds", &phase_labels).p95;
                ((*phase).to_owned(), p95)
            })
            .filter(|(_, p95)| p95.is_some())
            .collect();
        let overhead_before = merged(
            &self.before,
            "wyrd_verification_engine_overhead_seconds",
            &labels,
        );
        let overhead_after = merged(
            &self.after,
            "wyrd_verification_engine_overhead_seconds",
            &labels,
        );
        let (within, total) = share_within(&overhead_before, &overhead_after, OVERHEAD_BOUND);
        let queued = [kind_label.as_str()];
        ServerStats {
            attempts: attempts_per_replica.iter().sum(),
            attempts_per_replica,
            run_duration: histogram("wyrd_verification_run_duration_seconds", &labels),
            phase_p95,
            overhead: Percentiles::buckets(&overhead_before, &overhead_after),
            overhead_samples: total,
            overhead_within_bound: (total > 0.0).then(|| within / total),
            queue_wait: if mode == Mode::Queued {
                histogram("wyrd_verification_queue_wait_seconds", &queued)
            } else {
                Percentiles::default()
            },
            trigger_to_terminal: if mode == Mode::Queued {
                histogram("wyrd_verification_trigger_to_terminal_seconds", &queued)
            } else {
                Percentiles::default()
            },
        }
    }
}

/// Observations at or below `bound`, and all observations, between two
/// readings of one histogram.
fn share_within(before: &[(f64, f64)], after: &[(f64, f64)], bound: f64) -> (f64, f64) {
    let deltas = deltas(before, after);
    let total = deltas.last().map_or(0.0, |(_, count)| *count);
    let within = deltas
        .iter()
        .find(|(b, _)| b.to_bits() == bound.to_bits())
        .map_or(0.0, |(_, count)| *count);
    (within, total)
}

/// The durable runs of one Verifier created during a step.
#[derive(Debug, Default, Clone, Serialize)]
pub struct RunTally {
    /// Rows created.
    pub created: u64,
    /// Rows by status.
    pub statuses: BTreeMap<String, u64>,
    /// Rows that took more than one attempt.
    pub retried: u64,
    /// Creation-to-settlement latency of each settled row, microseconds of
    /// PostgreSQL time.
    #[serde(skip)]
    pub latency_us: Vec<u64>,
    /// When each settled row settled, from the step start.
    #[serde(skip)]
    pub settled_at: Vec<Duration>,
}

/// One run row: Verifier UID, status, attempts, creation-to-settlement
/// microseconds, and settlement microseconds after the step start.
type RunRow = (String, String, i32, Option<i64>, Option<i64>);

/// The database owner's view of the durable run queue.
pub struct Queue {
    /// Owner pool, which reads every tenant's rows.
    owner: PgPool,
}

impl Queue {
    /// Connects as the database owner the Postgres wrapper exports.
    ///
    /// # Errors
    ///
    /// Returns the missing variable or connection failure.
    pub async fn connect() -> Result<Self> {
        Ok(Self {
            owner: PgPool::connect(&std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")?).await?,
        })
    }

    /// PostgreSQL's clock, which stamps every run.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    pub async fn now(&self) -> Result<DateTime<Utc>> {
        Ok(sqlx::query_scalar("SELECT statement_timestamp()")
            .fetch_one(&self.owner)
            .await?)
    }

    /// Rows created since `since` still pending, running, or retrying, and
    /// all rows created since `since`.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    pub async fn progress(&self, since: DateTime<Utc>) -> Result<(u64, u64)> {
        let (outstanding, created): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*) FILTER (WHERE status IN ('pending','running','retrying')), \
                    COUNT(*) FROM wyrd.verifier_runs WHERE created_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.owner)
        .await?;
        Ok((u64::try_from(outstanding)?, u64::try_from(created)?))
    }

    /// Every run created since `since`, tallied by Verifier UID.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    pub async fn runs(&self, since: DateTime<Utc>) -> Result<BTreeMap<String, RunTally>> {
        let rows: Vec<RunRow> = sqlx::query_as(
            "SELECT verifier_uid::text, status, attempts, \
                    (EXTRACT(EPOCH FROM settled_at - created_at) * 1e6)::bigint, \
                    (EXTRACT(EPOCH FROM settled_at - $1) * 1e6)::bigint \
               FROM wyrd.verifier_runs WHERE created_at >= $1",
        )
        .bind(since)
        .fetch_all(&self.owner)
        .await?;
        let mut tallies: BTreeMap<String, RunTally> = BTreeMap::new();
        for (verifier, status, attempts, latency, settled) in rows {
            let tally = tallies.entry(verifier).or_default();
            tally.created += 1;
            *tally.statuses.entry(status).or_default() += 1;
            tally.retried += u64::from(attempts > 1);
            if let (Some(latency), Some(settled)) = (latency, settled) {
                tally.latency_us.push(u64::try_from(latency).unwrap_or(0));
                tally
                    .settled_at
                    .push(Duration::from_micros(u64::try_from(settled).unwrap_or(0)));
            }
        }
        Ok(tallies)
    }
}

#[cfg(test)]
mod tests {
    use super::share_within;

    /// The share under a bound counts only the interval's observations.
    ///
    /// # Panics
    ///
    /// Panics when the share is wrong.
    #[test]
    fn share_within_reads_the_bound_delta() {
        let before = [(0.009, 90.0), (0.01, 95.0), (f64::INFINITY, 100.0)];
        let after = [(0.009, 185.0), (0.01, 195.0), (f64::INFINITY, 200.0)];
        assert_eq!(share_within(&before, &after, 0.009), (95.0, 100.0));
    }
}
