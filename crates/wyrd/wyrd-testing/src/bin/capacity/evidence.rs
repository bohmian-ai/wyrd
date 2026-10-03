//! Server-side evidence of one step: exporter histogram deltas summed across
//! replicas, durable run rows, and every backlog the server owns.
//!
//! Histogram percentiles are bucket estimates: the upper bound of the
//! cumulative bucket covering the quantile, computed from the step's bucket
//! deltas after summing every replica's buckets, never by averaging replica
//! percentiles. Raw percentiles come from per-request samples.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use wyrd_testing::release_server::Metrics;

use crate::Result;
use crate::fixture::Kind;

/// p50, p95, and p99 of something.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Percentiles {
    /// Median.
    pub p50: Option<f64>,
    /// 95th percentile.
    pub p95: Option<f64>,
    /// 99th percentile.
    pub p99: Option<f64>,
}

impl Percentiles {
    /// Nearest-rank percentiles of raw `samples`, scaled by `scale`.
    pub fn raw(samples: &[u64], scale: f64) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let rank = |q: f64| -> Option<f64> {
            if sorted.is_empty() {
                return None;
            }
            let index = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
            Some(sorted[index] as f64 * scale)
        };
        Self {
            p50: rank(0.5),
            p95: rank(0.95),
            p99: rank(0.99),
        }
    }
}

/// Paired direct engine overhead of one kind over one step (AC-040).
#[derive(Debug, Clone, Serialize)]
pub struct Overhead {
    /// The workload's metric label.
    pub kind: &'static str,
    /// Samples recorded.
    pub samples: f64,
    /// p95 bucket bound, seconds; `None` without samples.
    pub p95: Option<f64>,
}

/// Every replica's `/metrics` scrapes at both step boundaries, in ordinal
/// order.
pub struct Scrapes {
    /// At the step start.
    pub before: Vec<Metrics>,
    /// After the step drained.
    pub after: Vec<Metrics>,
}

impl Scrapes {
    /// Direct-mode paired engine overhead of `kind` between the two scrapes.
    pub fn overhead(&self, kind: Kind) -> Overhead {
        let kind_label = format!("kind=\"{}\"", kind.label());
        let labels = [kind_label.as_str(), "mode=\"direct\""];
        let family = "wyrd_verification_engine_overhead_seconds";
        let deltas = deltas(
            &merged(&self.before, family, &labels),
            &merged(&self.after, family, &labels),
        );
        Overhead {
            kind: kind.label(),
            samples: deltas.last().map_or(0.0, |(_, count)| *count),
            p95: quantile(&deltas, 0.95),
        }
    }
}

/// Scribe work still queued on every replica in `scrapes`: generations
/// waiting for persistence plus immutable generations not yet persisted.
pub fn scribe_backlog(scrapes: &[Metrics]) -> u64 {
    scrapes
        .iter()
        .map(|metrics| {
            metrics.sum("bifrost_scribe_persistence_queue_depth", &[])
                + metrics.sum("bifrost_scribe_immutable_generation_count", &[])
        })
        .sum::<f64>()
        .max(0.0) as u64
}

/// The cumulative buckets of `family` matching `labels`, summed across
/// every replica's scrape.
fn merged(scrapes: &[Metrics], family: &str, labels: &[&str]) -> Vec<(f64, f64)> {
    let mut buckets: BTreeMap<u64, (f64, f64)> = BTreeMap::new();
    for scrape in scrapes {
        for (bound, count) in scrape.buckets(family, labels) {
            buckets.entry(bound.to_bits()).or_insert((bound, 0.0)).1 += count;
        }
    }
    let mut sorted: Vec<(f64, f64)> = buckets.into_values().collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    sorted
}

/// Per-bound increase between two cumulative bucket readings.
fn deltas(before: &[(f64, f64)], after: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let prior = |bound: f64| {
        before
            .iter()
            .find(|(b, _)| b.to_bits() == bound.to_bits())
            .map_or(0.0, |(_, count)| *count)
    };
    after
        .iter()
        .map(|(bound, count)| (*bound, count - prior(*bound)))
        .collect()
}

/// The bound of the cumulative bucket holding quantile `q` of cumulative
/// bucket `deltas`, or `None` when none were recorded.
fn quantile(deltas: &[(f64, f64)], q: f64) -> Option<f64> {
    let total = deltas.last().map_or(0.0, |(_, count)| *count);
    if total <= 0.0 {
        return None;
    }
    deltas
        .iter()
        .find(|(_, count)| *count >= q * total)
        .map(|(bound, _)| *bound)
}

/// The durable runs of one Verifier created during a step.
#[derive(Debug, Default, Clone, Serialize)]
pub struct RunTally {
    /// Rows created.
    pub created: u64,
    /// Rows by status.
    pub statuses: BTreeMap<String, u64>,
    /// When each settled row settled, from the step start.
    #[serde(skip)]
    pub settled_at: Vec<Duration>,
}

/// Work every server-owned backlog still holds.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Backlog {
    /// Runs created this step still pending, running, or retrying, plus
    /// accepted queued requests whose run does not exist yet.
    pub runs: u64,
    /// Scribe generations not yet persisted, every replica together.
    pub scribe: u64,
    /// Audit decisions staged before load stopped and not yet published.
    pub audit: u64,
    /// Forge planning demands requested before load stopped and not yet
    /// settled.
    pub forge: u64,
}

impl Backlog {
    /// Whether every backlog is empty.
    pub fn is_empty(&self) -> bool {
        self.runs + self.scribe + self.audit + self.forge == 0
    }
}

/// One run row: Verifier UID, status, and settlement microseconds after the
/// step start.
type RunRow = (String, String, Option<i64>);

/// The database owner's view of every durable queue.
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

    /// PostgreSQL's clock, which stamps every run, audit row, and demand.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    pub async fn now(&self) -> Result<DateTime<Utc>> {
        Ok(sqlx::query_scalar("SELECT statement_timestamp()")
            .fetch_one(&self.owner)
            .await?)
    }

    /// The Postgres-held backlogs: runs created since `since` that are not
    /// terminal with `activations` accepted queued requests expected to have
    /// created one each, audit rows staged by `stopped` above their tenant's
    /// publication watermark, and Forge demands first requested by
    /// `stopped`. A coalesced demand keeps its first request time, so one
    /// re-requested but never settled stays counted. Scribe is read from
    /// metrics instead and left zero here.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    pub async fn backlog(
        &self,
        since: DateTime<Utc>,
        stopped: DateTime<Utc>,
        activations: u64,
    ) -> Result<Backlog> {
        let (outstanding, created, audit, forge): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM wyrd.verifier_runs \
                 WHERE created_at >= $1 AND status IN ('pending','running','retrying')), \
               (SELECT COUNT(*) FROM wyrd.verifier_runs WHERE created_at >= $1), \
               (SELECT COUNT(*) FROM vala.audit_staging s \
                  JOIN vala.audit_chain_head h USING (data_tenant_id) \
                 WHERE s.seq > h.published_seq AND s.created_at <= $2), \
               (SELECT COUNT(*) FROM vala.forge_planning_demands \
                 WHERE first_requested_at <= $2)",
        )
        .bind(since)
        .bind(stopped)
        .fetch_one(&self.owner)
        .await?;
        Ok(Backlog {
            runs: u64::try_from(outstanding)? + activations.saturating_sub(u64::try_from(created)?),
            scribe: 0,
            audit: u64::try_from(audit)?,
            forge: u64::try_from(forge)?,
        })
    }

    /// Every run created since `since`, tallied by Verifier UID.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    pub async fn runs(&self, since: DateTime<Utc>) -> Result<BTreeMap<String, RunTally>> {
        let rows: Vec<RunRow> = sqlx::query_as(
            "SELECT verifier_uid::text, status, \
                    (EXTRACT(EPOCH FROM settled_at - $1) * 1e6)::bigint \
               FROM wyrd.verifier_runs WHERE created_at >= $1",
        )
        .bind(since)
        .fetch_all(&self.owner)
        .await?;
        let mut tallies: BTreeMap<String, RunTally> = BTreeMap::new();
        for (verifier, status, settled) in rows {
            let tally = tallies.entry(verifier).or_default();
            tally.created += 1;
            *tally.statuses.entry(status).or_default() += 1;
            if let Some(settled) = settled {
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
    use super::{Percentiles, deltas, quantile};

    /// A quantile is the first cumulative bucket bound covering it among the
    /// observations between two readings, and an empty interval has none.
    ///
    /// # Panics
    ///
    /// Panics when a bound is wrong.
    #[test]
    fn quantile_reads_bucket_deltas() {
        let before = [(0.1, 10.0), (1.0, 10.0), (f64::INFINITY, 10.0)];
        let after = [(0.1, 15.0), (1.0, 105.0), (f64::INFINITY, 110.0)];
        let step = deltas(&before, &after);
        assert_eq!(quantile(&step, 0.05), Some(0.1));
        assert_eq!(quantile(&step, 0.5), Some(1.0));
        assert_eq!(quantile(&step, 0.99), Some(f64::INFINITY));
        assert_eq!(quantile(&deltas(&after, &after), 0.5), None);
    }

    /// Raw percentiles use the nearest rank and an empty sample has none.
    ///
    /// # Panics
    ///
    /// Panics when a percentile is wrong.
    #[test]
    fn raw_percentiles_use_nearest_rank() {
        let samples: Vec<u64> = (1..=100).collect();
        let percentiles = Percentiles::raw(&samples, 1.0);
        assert_eq!(
            (percentiles.p50, percentiles.p95, percentiles.p99),
            (Some(50.0), Some(95.0), Some(99.0))
        );
        assert_eq!(Percentiles::raw(&[], 1.0).p50, None);
    }
}
