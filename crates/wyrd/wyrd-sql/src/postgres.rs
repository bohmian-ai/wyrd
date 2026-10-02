//! Production-ready Postgres handle for Wyrd control-plane SQL.

use secrecy::ExposeSecret;
use sqlx::PgPool;
use sqlx::types::Uuid;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::{DataTenantId, TenantSlug};

use crate::dsn::ResolvedDsns;
use crate::dsn::WYRD_APP_ROLE;
use crate::operator_pool::OperatorPool;
use crate::pool::{PoolConfig, build_pool};
use crate::schema_check::verify_login_name;
use crate::{SqlError, TenantConn};

/// Drop-safe telemetry for one Wyrd application-pool acquisition.
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
        metrics::counter!("wyrd_postgres_pool_acquire_total", "pool" => "app").increment(1);
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
        metrics::histogram!("wyrd_postgres_pool_acquire_seconds", "pool" => "app", "outcome" => outcome).record(self.started.elapsed().as_secs_f64());
        metrics::gauge!("wyrd_postgres_pool_size", "pool" => "app")
            .set(f64::from(self.pool.size()));
        metrics::gauge!("wyrd_postgres_pool_idle", "pool" => "app")
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

/// Runtime-ready Wyrd Postgres handle.
///
/// Holds only the two serving pools. Construction never migrates; serving
/// boot calls [`Self::validate_schema`] before it reports ready, and the
/// one-off `wyrd-server migrate` process applies migrations with the owner
/// login through [`crate::migrate`].
#[derive(Clone)]
pub struct WyrdPostgres {
    app: PgPool,
    platform_admin: Option<PgPool>,
}

impl WyrdPostgres {
    /// Build the serving `wyrd_app` and `wyrd_platform_admin` pools.
    ///
    /// No DDL runs and nothing is validated; call [`Self::validate_schema`]
    /// before serving.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when either pool cannot be built.
    pub async fn connect_from_dsns(dsns: &ResolvedDsns) -> Result<Self, SqlError> {
        let app = build_pool(dsns.app.expose_secret(), PoolConfig::app_from_env())
            .await
            .map_err(SqlError::Connect)?;
        let platform_admin = build_pool(
            dsns.platform_admin.expose_secret(),
            PoolConfig::platform_admin_from_env(),
        )
        .await
        .map_err(SqlError::Connect)?;
        Ok(Self {
            app,
            platform_admin: Some(platform_admin),
        })
    }

    /// Prove the database is ready for serving Wyrd control-plane traffic.
    ///
    /// Checks, in order: the app pool logs in as exactly `wyrd_app`; the
    /// platform pool logs in as exactly `wyrd_platform_admin`; and the Wyrd
    /// schema contract ([`crate::verify_schema`]) — role attributes, every
    /// embedded migration and checksum, schema privileges, and tenant
    /// isolation policies. Read-only; no DDL runs.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] for the first failed check (including
    /// a handle without a platform pool), [`SqlError::MigrateChecksum`] for
    /// checksum drift, and [`SqlError::Connect`] on query failure.
    pub async fn validate_schema(&self) -> Result<(), SqlError> {
        let operator = self
            .operator_pool()
            .ok_or_else(|| SqlError::SchemaNotReady {
                detail: "no wyrd_platform_admin pool is configured".to_owned(),
            })?;
        self.verify_app_login().await?;
        operator.verify_platform_login().await?;
        crate::verify_schema(&operator).await
    }

    /// Prove the app pool logs in, and acts, as exactly `wyrd_app`.
    ///
    /// # Errors
    /// Returns [`SqlError::SchemaNotReady`] naming the observed role when it
    /// differs, and [`SqlError::Connect`] on query failure.
    async fn verify_app_login(&self) -> Result<(), SqlError> {
        let (session, current): (String, String) =
            sqlx::query_as("SELECT session_user::text, current_user::text")
                .fetch_one(&self.app)
                .await
                .map_err(SqlError::Connect)?;
        verify_login_name(WYRD_APP_ROLE, &session, &current)
    }

    /// Wrap pre-built pools into a handle.
    ///
    /// **Migrations are assumed already applied elsewhere.** Production and
    /// DB-backed tests use `connect_from_dsns`. This seam exists
    /// only for DB-free unit tests that construct lazy pools and never issue a
    /// query. Gated behind `testing` / `cfg(test)` so it cannot be reached from a
    /// production build.
    #[must_use]
    pub fn from_pools(app: PgPool, platform_admin: Option<PgPool>) -> Self {
        Self {
            app,
            platform_admin,
        }
    }

    /// Borrow the RLS-enforced runtime app pool.
    #[must_use]
    pub fn app_pool(&self) -> &PgPool {
        &self.app
    }

    /// Build an `OperatorPool` from the optional platform-admin pool.
    ///
    /// Returns `None` when no cross-tenant role is configured. Production boot
    /// that requires cross-tenant maintenance must fail fast when this is `None`.
    #[must_use]
    pub fn operator_pool(&self) -> Option<OperatorPool> {
        self.platform_admin.clone().map(OperatorPool::from)
    }

    /// Open a tenant-scoped transaction on the app pool.
    ///
    /// # Errors
    /// Returns [`SqlError`] when acquiring or binding the transaction fails.
    pub async fn tenant_conn(
        &self,
        data_tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, SqlError> {
        let mut lifecycle = PoolAcquireLifecycle::begin(&self.app);
        let result = TenantConn::acquire(&self.app, data_tenant_id).await;
        lifecycle.finish(&result);
        result
    }

    /// Resolve a URL tenant slug to its tenant id.
    ///
    /// This is the one slug resolver for tenant login, workload exchange, and
    /// boot. A slug names no tenant yet, so the lookup runs on the audited
    /// operator pool through
    /// [`crate::queries::platform::tenant_resolver::resolve_by_slug`]; the
    /// RLS app pool is never a fallback. Returns `None` when the slug is
    /// unknown, suspended, or deleted.
    ///
    /// # Errors
    /// Returns [`SqlError::InsufficientPrivilege`] when no platform-admin
    /// operator pool is configured, so pre-tenant resolution fails closed, and
    /// otherwise the errors of
    /// [`crate::queries::platform::tenant_resolver::resolve_by_slug`].
    pub async fn resolve_tenant_slug(
        &self,
        slug: &TenantSlug,
    ) -> Result<Option<DataTenantId>, SqlError> {
        let operator = self
            .operator_pool()
            .ok_or_else(|| SqlError::InsufficientPrivilege {
                detail: "tenant slug resolution requires the wyrd_platform_admin operator pool"
                    .to_owned(),
            })?;
        crate::queries::platform::tenant_resolver::resolve_by_slug(&operator, slug).await
    }

    /// Resolve the tenant owning an unconsumed, unexpired login state.
    ///
    /// The common OIDC callback carries only the provider's `state`; it has no
    /// tenant selector and never trusts `Host` or forwarded headers. The
    /// SECURITY DEFINER function `wyrd.auth_login_state_tenant`, granted only
    /// to the runtime `wyrd_app` role this handle's app pool connects as,
    /// answers this one question across tenant RLS: the tenant id of the row
    /// whose SHA-256 state hash is `state_hash`, or `None` when the state is
    /// unknown, consumed, or expired. It exposes no other column, so the
    /// caller learns only which tenant transaction to open.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when Postgres rejects the lookup and
    /// [`SqlError::InvalidDataTenantId`] when the stored tenant id violates the
    /// Wyrd tenant-id contract.
    pub async fn login_state_tenant(
        &self,
        state_hash: &Sha256Hex,
    ) -> Result<Option<DataTenantId>, SqlError> {
        self.definer_tenant("SELECT wyrd.auth_login_state_tenant($1)", state_hash)
            .await
    }

    /// Resolve the tenant owning a completed, unexpired browser login.
    ///
    /// BFF login completion presents only the SHA-256 of its `HttpOnly` flow
    /// id. The SECURITY DEFINER function `wyrd.auth_login_completion_tenant`
    /// answers which tenant holds a completed login bound to exactly that
    /// flow hash, or `None`; redemption then runs under that tenant's RLS.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when Postgres rejects the lookup and
    /// [`SqlError::InvalidDataTenantId`] when the stored tenant id violates the
    /// Wyrd tenant-id contract.
    pub async fn login_completion_tenant(
        &self,
        browser_flow_hash: &Sha256Hex,
    ) -> Result<Option<DataTenantId>, SqlError> {
        self.definer_tenant(
            "SELECT wyrd.auth_login_completion_tenant($1)",
            browser_flow_hash,
        )
        .await
    }

    /// Resolve the tenant owning a live browser session.
    ///
    /// The BFF presents only the raw session id; its SHA-256 is all the
    /// SECURITY DEFINER function `wyrd.auth_browser_session_tenant` accepts.
    /// It answers which tenant holds an unrevoked, unexpired session with that
    /// hash, or `None`, and exposes no other column.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when Postgres rejects the lookup and
    /// [`SqlError::InvalidDataTenantId`] when the stored tenant id violates the
    /// Wyrd tenant-id contract.
    pub async fn browser_session_tenant(
        &self,
        id_hash: &Sha256Hex,
    ) -> Result<Option<DataTenantId>, SqlError> {
        self.definer_tenant("SELECT wyrd.auth_browser_session_tenant($1)", id_hash)
            .await
    }

    /// Read a tenant's route key and display name from the tenant directory.
    ///
    /// The directory is not tenant data, so the read runs on the audited
    /// operator pool through
    /// [`crate::queries::platform::tenants::tenant_by_id`]; the RLS app pool is
    /// never a fallback. Returns `None` for a deleted or unknown tenant.
    ///
    /// # Errors
    /// Returns [`SqlError::InsufficientPrivilege`] when no platform-admin
    /// operator pool is configured, and the errors of
    /// [`crate::queries::platform::tenants::tenant_by_id`].
    pub async fn tenant_directory_entry(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<(TenantSlug, String)>, SqlError> {
        let operator = self
            .operator_pool()
            .ok_or_else(|| SqlError::InsufficientPrivilege {
                detail: "tenant directory reads require the wyrd_platform_admin operator pool"
                    .to_owned(),
            })?;
        let Some(row) = crate::queries::platform::tenants::tenant_by_id(&operator, tenant).await?
        else {
            return Ok(None);
        };
        let slug = TenantSlug::new(row.slug).map_err(|error| SqlError::SchemaNotReady {
            detail: format!("tenant {tenant} has an invalid stored slug: {error}"),
        })?;
        Ok(Some((slug, row.display_name)))
    }

    /// Run one SECURITY DEFINER hash-to-tenant lookup on the app pool.
    ///
    /// Each lookup function is granted only to `wyrd_app` and returns one
    /// nullable tenant id for a SHA-256 key.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when Postgres rejects the lookup and
    /// [`SqlError::InvalidDataTenantId`] when the stored tenant id violates the
    /// Wyrd tenant-id contract.
    async fn definer_tenant(
        &self,
        sql: &'static str,
        hash: &Sha256Hex,
    ) -> Result<Option<DataTenantId>, SqlError> {
        // Dynamic query is intentional: the definer functions post-date the
        // SQLx offline bundle.
        let tenant_uuid = sqlx::query_scalar::<_, Option<Uuid>>(sql)
            .bind(hash.as_bytes().as_slice())
            .fetch_one(&self.app)
            .await
            .map_err(SqlError::from)?;

        tenant_uuid
            .map(DataTenantId::new)
            .transpose()
            .map_err(SqlError::InvalidDataTenantId)
    }
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

    use super::WyrdPostgres;

    /// Isolated exact recorder for the SQL owner lifecycle.
    #[derive(Default)]
    struct TestRecorder {
        /// Exact counter series.
        counters: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
        /// Exact gauge series.
        gauges: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
        /// Exact histogram observation counts.
        histograms: Mutex<HashMap<String, Arc<Count>>>,
    }

    /// Atomic histogram observation count.
    #[derive(Default)]
    struct Count(AtomicU64);

    impl HistogramFn for Count {
        /// Count one terminal duration observation.
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
        fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
            Gauge::from_arc(Arc::clone(
                self.gauges
                    .lock()
                    .expect("gauges")
                    .entry(name(key))
                    .or_default(),
            ))
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

    /// Render one stable exact metric key.
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

    /// The real Wyrd owner reconciles success, closed failure, and pending cancellation.
    #[tokio::test(flavor = "current_thread")]
    async fn wyrd_pool_acquire_owner_reconciles_all_terminals() {
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
        let success_owner = WyrdPostgres::from_pools(success_pool.clone(), None);
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
        let failure_owner = WyrdPostgres::from_pools(failed_pool, None);
        let failure = failure_owner
            .tenant_conn(wyrd_spec::DataTenantId::SYSTEM_OWNER)
            .await;
        assert!(failure.is_err());

        let pending_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("pending pool");
        let held = pending_pool.acquire().await.expect("held connection");
        let owner = WyrdPostgres::from_pools(pending_pool, None);
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
            recorder.counter("wyrd_postgres_pool_acquire_total{pool=app}"),
            3
        );
        for outcome in ["success", "failed", "cancelled"] {
            assert_eq!(
                recorder.histogram(&format!(
                    "wyrd_postgres_pool_acquire_seconds{{outcome={outcome},pool=app}}"
                )),
                1
            );
        }
    }

    /// Without a platform-admin operator pool, slug resolution fails closed
    /// before any IO rather than falling back to the RLS app pool.
    #[tokio::test(flavor = "current_thread")]
    async fn resolve_tenant_slug_without_operator_pool_fails_closed() {
        let app = PgPoolOptions::new()
            .connect_lazy("postgres://unused.invalid/none")
            .expect("lazy pool builds without connecting");
        let app_only = WyrdPostgres::from_pools(app, None);
        let slug = wyrd_spec::TenantSlug::new("acme").expect("slug is valid");

        let error = app_only
            .resolve_tenant_slug(&slug)
            .await
            .expect_err("resolution must not fall back to the app pool");

        assert!(matches!(
            error,
            crate::SqlError::InsufficientPrivilege { .. }
        ));
    }
}
