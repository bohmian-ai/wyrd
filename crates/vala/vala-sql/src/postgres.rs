//! Production-ready Postgres handle for Vala SQL.
//!
//! `ValaPostgres` owns its runtime connection pool rather than borrowing a
//! Wyrd-owned pool by reference.

use std::time::Duration;

use secrecy::ExposeSecret;
use sqlx::PgPool;
use wyrd_spec::DataTenantId;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::pool::build_pool;
use wyrd_sql::schema_check::verify_tenant_pool;
use wyrd_sql::{OperatorPool, PoolConfig, SqlError, TenantConn};

/// Drop-safe telemetry for one Vala runtime-pool acquisition.
struct PoolAcquireLifecycle<'a> {
    /// Pool sampled only at terminal observation.
    pool: &'a PgPool,
    /// Monotonic acquisition start.
    started: std::time::Instant,
    /// Whether success or failure was already recorded.
    finished: bool,
}

impl<'a> PoolAcquireLifecycle<'a> {
    /// Start one acquisition attempt before its first await.
    fn begin(pool: &'a PgPool) -> Self {
        metrics::counter!("vala_postgres_pool_acquire_total", "pool" => "runtime").increment(1);
        Self {
            pool,
            started: std::time::Instant::now(),
            finished: false,
        }
    }

    /// Record the unchanged acquisition result exactly once.
    fn finish<T, E>(&mut self, result: &Result<T, E>) {
        self.record(if result.is_ok() { "success" } else { "failed" });
    }

    /// Emit one bounded terminal observation and pool snapshot.
    fn record(&mut self, outcome: &'static str) {
        if self.finished {
            return;
        }
        metrics::histogram!("vala_postgres_pool_acquire_seconds", "pool" => "runtime", "outcome" => outcome).record(self.started.elapsed().as_secs_f64());
        metrics::gauge!("vala_postgres_pool_size", "pool" => "runtime")
            .set(f64::from(self.pool.size()));
        metrics::gauge!("vala_postgres_pool_idle", "pool" => "runtime")
            .set(self.pool.num_idle() as f64);
        self.finished = true;
    }
}

impl Drop for PoolAcquireLifecycle<'_> {
    /// Record cancellation when the pending acquisition future is dropped.
    fn drop(&mut self) {
        self.record("cancelled");
    }
}

/// Runtime-ready Vala Postgres handle.
///
/// Holds the dedicated Vala/Bifrost runtime pool, which connects as the tenant
/// login. Construction never migrates; serving boot calls
/// [`Self::validate_schema`] before it reports ready.
#[derive(Clone)]
pub struct ValaPostgres {
    pool: PgPool,
    /// Whether the runtime pool uses the login shared with the platform pool
    /// ([`ResolvedDsns::shares_login`]), which selects the tenant-login checks.
    shared_login: bool,
}

impl ValaPostgres {
    /// Build the Vala runtime pool from the tenant DSN ([`ResolvedDsns::app`]).
    ///
    /// No DDL runs and nothing is validated; call [`Self::validate_schema`]
    /// before serving.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when the pool cannot be built.
    pub async fn connect_from_dsns(dsns: &ResolvedDsns) -> Result<Self, SqlError> {
        let pool = build_pool(dsns.app.expose_secret(), vala_pool_config())
            .await
            .map_err(SqlError::Connect)?;
        Ok(Self {
            pool,
            shared_login: dsns.shares_login(),
        })
    }

    /// Prove the database is ready for serving Vala and Bifrost traffic.
    ///
    /// The runtime pool's login must stay bound by row-level security over the
    /// Vala schemas ([`verify_tenant_pool`]); then the Vala schema contract
    /// ([`crate::verify_schema`]) holds through the deployment's `operator`
    /// session. Read-only; no DDL runs.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] for the first failed check,
    /// [`SqlError::MigrateChecksum`] for checksum drift, and
    /// [`SqlError::Connect`] on query failure.
    pub async fn validate_schema(&self, operator: &OperatorPool) -> Result<(), SqlError> {
        verify_tenant_pool(&self.pool, !self.shared_login, crate::OWNED_SCHEMAS).await?;
        crate::verify_schema(operator).await
    }

    /// Wrap pre-built pools into a handle. Migrations assumed already applied
    /// elsewhere. Used only by DB-free unit tests; production and DB-backed tests
    /// use `connect_from_dsns`. Gated behind
    /// `testing` / `cfg(test)`.
    #[must_use]
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool,
            shared_login: false,
        }
    }

    /// Borrow the Vala/Bifrost runtime pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Open a tenant-scoped transaction through the Vala application pool.
    ///
    /// Forge uses this owner boundary for every tenant mutation. The caller
    /// owns the transaction and must commit it explicitly.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction cannot be opened or the
    /// tenant binding cannot be applied.
    pub async fn tenant_conn(
        &self,
        data_tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, SqlError> {
        let mut lifecycle = PoolAcquireLifecycle::begin(&self.pool);
        let result = TenantConn::acquire(&self.pool, data_tenant_id).await;
        lifecycle.finish(&result);
        result
    }
}

/// Default pool profile for Vala/Bifrost runtime SQL.
#[must_use]
pub fn vala_pool_config() -> PoolConfig {
    PoolConfig::from_env_with_suffix(
        PoolConfig {
            max_connections: 16,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(10),
            idle_timeout: Some(Duration::from_mins(5)),
            max_lifetime: Some(Duration::from_mins(30)),
            statement_cache_capacity: 512,
            test_before_acquire: true,
        },
        "_VALA",
    )
}

#[cfg(test)]
mod telemetry_tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};

    use metrics::{Counter, Gauge, Histogram, HistogramFn, Key, Metadata, Recorder};
    use sqlx::postgres::PgPoolOptions;

    use super::ValaPostgres;

    /// Isolated recorder for exact Vala pool owner assertions.
    #[derive(Default)]
    struct TestRecorder {
        /// Exact counter series.
        counters: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
        /// Exact histogram counts.
        histograms: Mutex<HashMap<String, Arc<Count>>>,
    }
    /// One atomic histogram observation count.
    #[derive(Default)]
    struct Count(AtomicU64);
    impl HistogramFn for Count {
        /// Count one terminal duration.
        fn record(&self, _: f64) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    impl Recorder for TestRecorder {
        fn describe_counter(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn describe_gauge(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn describe_histogram(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
            Counter::from_arc(Arc::clone(
                self.counters
                    .lock()
                    .expect("counters")
                    .entry(name(key))
                    .or_default(),
            ))
        }
        fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
            Gauge::noop()
        }
        fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
            Histogram::from_arc(Arc::clone(
                self.histograms
                    .lock()
                    .expect("histograms")
                    .entry(name(key))
                    .or_default(),
            ))
        }
    }
    /// Render a stable exact series key.
    fn name(key: &Key) -> String {
        let mut labels = key
            .labels()
            .map(|label| format!("{}={}", label.key(), label.value()))
            .collect::<Vec<_>>();
        labels.sort();
        if labels.is_empty() {
            key.name().to_owned()
        } else {
            format!("{}{{{}}}", key.name(), labels.join(","))
        }
    }
    impl TestRecorder {
        /// Read one exact counter.
        fn counter(&self, key: &str) -> u64 {
            self.counters
                .lock()
                .expect("counters")
                .get(key)
                .map_or(0, |value| value.load(Ordering::Relaxed))
        }
        /// Read one exact histogram count.
        fn histogram(&self, key: &str) -> u64 {
            self.histograms
                .lock()
                .expect("histograms")
                .get(key)
                .map_or(0, |value| value.0.load(Ordering::Relaxed))
        }
    }

    /// The real Vala owner reconciles success, closed failure, and pending cancellation.
    #[tokio::test(flavor = "current_thread")]
    async fn vala_pool_acquire_owner_reconciles_all_terminals() {
        let Some(url) = std::env::var("WYRD_DATABASE_URL").ok() else {
            return;
        };
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);

        let success_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("test pool");
        let success_owner = ValaPostgres::from_pool(success_pool);
        let success = success_owner
            .tenant_conn(wyrd_spec::DataTenantId::SYSTEM_OWNER)
            .await;
        assert!(success.is_ok(), "system tenant acquisition must succeed");
        drop(success);

        let failed_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("failure pool");
        failed_pool.close().await;
        let failure_owner = ValaPostgres::from_pool(failed_pool);
        assert!(
            failure_owner
                .tenant_conn(wyrd_spec::DataTenantId::SYSTEM_OWNER)
                .await
                .is_err()
        );

        let pending_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("pending pool");
        let held = pending_pool.acquire().await.expect("held connection");
        let owner = ValaPostgres::from_pool(pending_pool);
        let mut pending = Box::pin(owner.tenant_conn(wyrd_spec::DataTenantId::SYSTEM_OWNER));
        assert!(matches!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(std::task::Waker::noop())),
            Poll::Pending
        ));
        drop(pending);
        drop(held);

        assert_eq!(
            recorder.counter("vala_postgres_pool_acquire_total{pool=runtime}"),
            3
        );
        for outcome in ["success", "failed", "cancelled"] {
            assert_eq!(
                recorder.histogram(&format!(
                    "vala_postgres_pool_acquire_seconds{{outcome={outcome},pool=runtime}}"
                )),
                1
            );
        }
    }
}
