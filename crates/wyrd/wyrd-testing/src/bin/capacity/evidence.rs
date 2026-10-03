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

/// Scribe work still owned on every replica in `scrapes`: generations
/// waiting for persistence, immutable generations not yet persisted, and
/// durable staging members, ready or claimed, not yet published. Persistence
/// hands work to staging, so the first two can read zero while staging
/// still holds it.
fn scribe_backlog(scrapes: &[Metrics]) -> u64 {
    scrapes
        .iter()
        .map(|metrics| {
            metrics.sum("bifrost_scribe_persistence_queue_depth", &[])
                + metrics.sum("bifrost_scribe_immutable_generation_count", &[])
                + metrics.sum("bifrost_scribe_staging_live_members", &[])
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
    /// Scribe work not yet published, every replica together: see
    /// [`scribe_backlog`].
    pub scribe: u64,
    /// Audit decisions still owned: pending in any replica's outbox writer,
    /// or staged above their tenant's publication watermark.
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

    /// Adds what the replicas in `scrapes` still own in process to this
    /// durable reading from [`Queue::backlog`]: Scribe's backlog, and each
    /// replica's `audit_outbox_pending` decisions on top of the staged audit
    /// rows. A decision is pending until its commit lands in staging, so the
    /// sum covers it from the request's return until its publication,
    /// provided `scrapes` were taken before the durable reading: a decision
    /// committing between the two is then counted twice, never missed.
    pub fn with_replicas(self, scrapes: &[Metrics]) -> Self {
        let pending = scrapes
            .iter()
            .map(|metrics| metrics.sum("audit_outbox_pending", &[]))
            .sum::<f64>()
            .max(0.0) as u64;
        Self {
            scribe: scribe_backlog(scrapes),
            audit: self.audit + pending,
            ..self
        }
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
    ///
    /// # Cancellation
    ///
    /// Read-only; a dropped connect leaves nothing behind.
    pub async fn connect() -> Result<Self> {
        Ok(Self {
            owner: PgPool::connect(&std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")?).await?,
        })
    }

    /// A queue whose pool never connects until first used, for proofs of
    /// deployment cleanup that read no backlog.
    ///
    /// # Errors
    ///
    /// Returns the pool construction failure.
    #[cfg(test)]
    pub fn unconnected() -> Result<Self> {
        Ok(Self {
            owner: PgPool::connect_lazy("postgres://unused")?,
        })
    }

    /// PostgreSQL's clock, which stamps every run, audit row, and demand.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    ///
    /// # Cancellation
    ///
    /// Read-only.
    pub async fn now(&self) -> Result<DateTime<Utc>> {
        Ok(sqlx::query_scalar("SELECT statement_timestamp()")
            .fetch_one(&self.owner)
            .await?)
    }

    /// The Postgres-held backlogs: runs created since `since` that are not
    /// terminal with `activations` accepted queued requests expected to have
    /// created one each, every audit row staged above its tenant's
    /// publication watermark, and Forge demands first requested by
    /// `stopped`. Audit rows are not cut at `stopped`: a decision a request
    /// left pending commits later, and its row is still the step's backlog.
    /// A coalesced demand keeps its first request time, so one re-requested
    /// but never settled stays counted. Replica-held work is read from
    /// metrics instead and added by [`Backlog::with_replicas`].
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    ///
    /// # Cancellation
    ///
    /// Read-only.
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
                 WHERE s.seq > h.published_seq), \
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
    ///
    /// # Cancellation
    ///
    /// Read-only.
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
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use metrics_exporter_prometheus::PrometheusHandle;
    use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{GrpcConfig, HttpConfig};
    use wyrd_client::{Bifrost, WyrdClient};
    use wyrd_server::audit::publication::{AuditPublisher, PublishOutcome};
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::release_server::Metrics;

    use super::{Backlog, Percentiles, Queue, deltas, quantile, scribe_backlog};
    use crate::Result;

    /// How long the held-commit proof waits for any one server move.
    const WAIT: Duration = Duration::from_secs(60);

    /// The backlog a drain poll would read: the replica's `/metrics`
    /// snapshot first, then the durable backlog at `stopped`, combined in
    /// that order exactly as the capacity drain combines them.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    async fn drain_read(
        queue: &Queue,
        metrics: &PrometheusHandle,
        stopped: DateTime<Utc>,
    ) -> Result<Backlog> {
        let scrapes = [Metrics::parse(&metrics.render())];
        Ok(queue
            .backlog(stopped, stopped, 0)
            .await?
            .with_replicas(&scrapes))
    }

    /// The replica's pending audit decisions in its `/metrics` snapshot.
    fn pending(metrics: &PrometheusHandle) -> f64 {
        Metrics::parse(&metrics.render()).sum("audit_outbox_pending", &[])
    }

    /// Waits until Postgres shows at least one backend blocked on another's
    /// lock, which here can only be the audit writer behind the held chain
    /// head.
    ///
    /// # Errors
    ///
    /// Returns the query failure, or a timeout.
    async fn await_blocked_writer(queue: &Queue) -> Result<()> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let blocked: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity \
                  WHERE cardinality(pg_blocking_pids(pid)) > 0",
            )
            .fetch_one(&queue.owner)
            .await?;
            if blocked > 0 {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("the audit writer never blocked on the held chain head".into());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Publishes `tenant`'s staged audit rows until none is owed.
    ///
    /// # Errors
    ///
    /// Returns the publication failure.
    async fn publish_all(
        publisher: &AuditPublisher,
        tenant: wyrd_spec::DataTenantId,
    ) -> Result<()> {
        while publisher.publish_tenant(tenant).await? != PublishOutcome::Idle {}
        Ok(())
    }

    /// The capacity audit cell covers a decision for its whole life after
    /// its request returned: pending in the replica while the tenant's
    /// audit-chain commit is held, then staged above the publication
    /// watermark with a `created_at` later than the captured stop, and empty
    /// only once publication advances past it.
    ///
    /// Publication is disabled in the server and driven by hand, so every
    /// window is held rather than raced. Two public Oracle reads run while
    /// the chain head is locked: the first's decision blocks the writer
    /// inside its commit, the second's waits behind it, and the stop is
    /// captured after both requests return. Releasing the lock commits the
    /// first and then starts the second's transaction, so the second row is
    /// stamped after the stop.
    ///
    /// # Errors
    ///
    /// Returns the server, client, Postgres, or publication failure.
    ///
    /// # Panics
    ///
    /// Panics when the audit cell reads empty while the replica or staging
    /// still owns a decision, or stays nonzero after publication.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "starts an in-process server; needs the repository Postgres wrapper and migrations"]
    async fn the_audit_backlog_holds_from_a_pending_decision_until_its_publication() -> Result<()> {
        let metrics = wyrd_server::app::metrics::install_recorder()?;
        let server = WyrdTestServer::builder()
            .without_audit_publication_for_test()
            .start_bound()
            .await?;
        let tenant = server.data_tenant_id();
        let table = format!("capacity_audit_{}", uuid::Uuid::now_v7().simple());
        server
            .create_bifrost_table_for_test(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table),
                user_fields: vec![arrow::datatypes::Field::new(
                    "value",
                    arrow::datatypes::DataType::Int64,
                    false,
                )],
                tenant,
                physical_layout: None,
                audit: None,
            })
            .await?;
        let bootstrap = server
            .bootstrap_service_in_tenant(tenant, "capacity-audit", &["admin"])
            .await?;
        let client = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server.base_url().ok_or("missing HTTP URL")?.to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(bootstrap.api_key().ok_or("a machine key")?.clone()),
            ..ClientConfig::default()
        })?;
        let oracle = Bifrost::query_only(&client);
        let sql = format!("SELECT value FROM vala.bifrost.{table}");
        // The in-process server owns its own fixture database; the queue
        // reads it as that database's owner, as the benchmark reads its own.
        let queue = Queue {
            owner: server.pg_fixture().superuser_pool().await?,
        };
        let publisher =
            AuditPublisher::from_state(server.state()).ok_or("the server owns a local Scribe")?;

        // A first read authenticates the client and creates the chain head;
        // publishing it leaves nothing owed.
        oracle.sql(&sql).await?;
        let deadline = tokio::time::Instant::now() + WAIT;
        while pending(&metrics) > 0.0 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the first decision never committed"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        publish_all(&publisher, tenant).await?;
        let before = queue.now().await?;
        assert_eq!(
            drain_read(&queue, &metrics, before).await?.audit,
            0,
            "nothing owed yet"
        );

        let mut fence = server.tenant_conn_for(tenant).await?;
        sqlx::query("SELECT last_seq FROM vala.audit_chain_head FOR UPDATE")
            .fetch_all(&mut **fence.transaction())
            .await?;
        oracle.sql(&sql).await?;
        await_blocked_writer(&queue).await?;
        oracle.sql(&sql).await?;
        let stopped = queue.now().await?;

        let held = drain_read(&queue, &metrics, stopped).await?;
        assert_eq!(
            pending(&metrics),
            2.0,
            "both returned reads still own a decision"
        );
        assert_eq!(
            held.audit, 2,
            "the drain counts the replica's pending decisions"
        );

        fence.commit().await?;
        let deadline = tokio::time::Instant::now() + WAIT;
        while pending(&metrics) > 0.0 {
            let audit = drain_read(&queue, &metrics, stopped).await?.audit;
            assert!(
                audit >= 2,
                "the handoff from pending to staged read {audit}"
            );
            assert!(
                tokio::time::Instant::now() < deadline,
                "held decisions never committed"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let late: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging WHERE data_tenant_id = $1 AND created_at > $2",
        )
        .bind(tenant.as_uuid())
        .bind(stopped)
        .fetch_one(&queue.owner)
        .await?;
        assert!(late >= 1, "a held decision's row is stamped after the stop");
        assert_eq!(
            drain_read(&queue, &metrics, stopped).await?.audit,
            2,
            "committed rows above the watermark stay owed, late ones included"
        );

        publish_all(&publisher, tenant).await?;
        assert_eq!(
            drain_read(&queue, &metrics, stopped).await?.audit,
            0,
            "publication past every row empties the audit cell"
        );
        server.shutdown().await?;
        Ok(())
    }

    /// The replica's pending audit decisions add to the staged rows in the
    /// audit cell, beside an unchanged Scribe backlog.
    ///
    /// # Panics
    ///
    /// Panics when the sum is wrong.
    #[test]
    fn pending_decisions_add_to_staged_audit_rows() {
        let scrape = |pending: u32| {
            Metrics::parse(&format!(
                "audit_outbox_pending {pending}\nbifrost_scribe_staging_live_members 1\n"
            ))
        };
        let durable = Backlog {
            audit: 3,
            ..Backlog::default()
        };
        let combined = durable.with_replicas(&[scrape(2), scrape(0)]);
        assert_eq!((combined.audit, combined.scribe), (5, 2));
        assert!(Backlog::default().with_replicas(&[scrape(0)]).audit == 0);
    }

    /// One staged live member keeps Scribe's backlog nonzero while the
    /// persistence queue and immutable generations read zero, and the
    /// backlog clears with it.
    ///
    /// # Panics
    ///
    /// Panics when the backlog is wrong.
    #[test]
    fn staged_members_hold_the_scribe_backlog() {
        let scrape = |staged: u32| {
            Metrics::parse(&format!(
                "bifrost_scribe_persistence_queue_depth 0\n\
                 bifrost_scribe_immutable_generation_count 0\n\
                 bifrost_scribe_staging_live_members {staged}\n"
            ))
        };
        assert_eq!(scribe_backlog(&[scrape(0), scrape(1)]), 1);
        assert_eq!(scribe_backlog(&[scrape(0), scrape(0)]), 0);
    }

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
