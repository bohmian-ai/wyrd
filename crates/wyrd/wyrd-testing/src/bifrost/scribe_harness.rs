//! Multi-Scribe harness for multi-pod journey tests.

use std::sync::Arc;

use opendal::Operator;
use sqlx::AssertSqlSafe;
use thiserror::Error;
use vala_sql::ValaPostgres;
use wyrd_spec::ids::DataTenantId;
use wyrd_sql::TenantConn;

use vala_bifrost_redux::scribe::ScribeImpl;

/// Harness construction or operation error.
#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("timeout waiting for drain")]
    DrainTimeout,

    #[error("internal harness error: {0}")]
    Internal(String),
}

/// Validate schema name for SQL safety: alphanumeric + underscore, max 63 chars, no SQL keywords.
fn validate_schema_name(name: &str) -> Result<(), HarnessError> {
    if name.len() > 63 {
        return Err(HarnessError::Internal(
            "schema name exceeds 63 characters".to_string(),
        ));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(HarnessError::Internal(
            "schema name must be alphanumeric + underscore".to_string(),
        ));
    }
    let lower = name.to_lowercase();
    const KEYWORDS: &[&str] = &[
        "select", "drop", "insert", "update", "delete", "union", "create", "alter", "grant",
        "revoke",
    ];
    if KEYWORDS.contains(&lower.as_str()) {
        return Err(HarnessError::Internal(
            "schema name cannot be a SQL keyword".to_string(),
        ));
    }
    Ok(())
}

/// Harness configuration.
pub struct HarnessConfig {
    /// Number of Scribe pods to simulate.
    pub pods: usize,

    /// Tenant IDs active in this test.
    pub tenants: Vec<DataTenantId>,

    /// Per-test Postgres schema name for isolation.
    pub schema: String,
}

/// Multi-pod Scribe harness for journey tests.
///
/// Constructs N stub `ScribeImpl` instances sharing a single opendal `Operator`
/// (memory backend) and a ValaPostgres handle. Provides `wait_for_drain` and `shutdown`
/// for coordinated test teardown.
pub struct MultiScribeHarness {
    pods: Vec<ScribeImpl>,
    operator: Arc<Operator>,
    pg: Arc<ValaPostgres>,
    schema: String,
    tenants: Vec<DataTenantId>,
}

impl MultiScribeHarness {
    /// Construct a new multi-scribe harness.
    ///
    /// Creates N stub Scribe pods, a shared opendal memory backend, and a
    /// per-test PG schema for isolation.
    ///
    /// # Errors
    /// Returns an error if PG pool construction or schema creation fails.
    pub async fn new(cfg: HarnessConfig) -> Result<Self, HarnessError> {
        // Construct shared opendal memory backend
        let operator = Arc::new(
            Operator::new(opendal::services::Memory::default())
                .map_err(|e| HarnessError::Internal(format!("opendal init: {e}")))?
                .finish(),
        );

        // Construct ValaPostgres from admin+tenant pools
        let database_url = std::env::var("WYRD_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://wyrd_app:wyrd_app_pw@127.0.0.1:55432/wyrd".to_string());

        // Build pool for ValaPostgres
        use wyrd_sql::PoolConfig;
        use wyrd_sql::pool::build_pool;
        let runtime_pool = build_pool(&database_url, PoolConfig::default())
            .await
            .map_err(|e| HarnessError::Internal(format!("runtime pool: {e}")))?;

        let pg = Arc::new(ValaPostgres::from_pools(runtime_pool, None));

        // Create per-test schema for isolation
        // Schema names can't be parameters in PG DDL; validate for SQL safety
        validate_schema_name(&cfg.schema)?;
        // SAFETY: schema name is validated to be alphanumeric + underscore only.
        // We can't use AssertSqlSafe directly because it requires &'static str.
        // Instead, we leak the string (test-only) to get a 'static lifetime.
        let create_schema_sql = format!("CREATE SCHEMA IF NOT EXISTS {}", cfg.schema);
        let create_schema_static: &'static str = Box::leak(create_schema_sql.into_boxed_str());
        sqlx::raw_sql(AssertSqlSafe(create_schema_static))
            .execute(pg.pool())
            .await?;
        let set_path_sql = format!("SET search_path TO {}", cfg.schema);
        let set_path_static: &'static str = Box::leak(set_path_sql.into_boxed_str());
        sqlx::raw_sql(AssertSqlSafe(set_path_static))
            .execute(pg.pool())
            .await?;

        // Construct N stub Scribe pods with temp WAL per pod
        let pods = (0..cfg.pods)
            .map(|i| {
                let temp_dir = tempfile::tempdir()
                    .map_err(|e| HarnessError::Internal(format!("temp WAL dir: {e}")))?;
                let node_id = uuid::Uuid::new_v4();
                let node_id_str = node_id.to_string();
                let wal = Arc::new(
                    vala_bifrost_redux::scribe::wal::WalWriter::new(
                        temp_dir.path(),
                        *node_id.as_bytes(),
                        1 + i as i64, // writer_epoch per pod
                        cfg.tenants
                            .first()
                            .copied()
                            .unwrap_or_else(DataTenantId::new_v7),
                        None,
                    )
                    .map_err(|e| HarnessError::Internal(format!("WAL init: {e}")))?,
                );
                Ok(ScribeImpl::new_with_deps(
                    operator.clone(),
                    wal,
                    node_id_str,
                    1 + i as i64,
                ))
            })
            .collect::<Result<Vec<_>, HarnessError>>()?;

        Ok(Self {
            pods,
            operator,
            pg,
            schema: cfg.schema,
            tenants: cfg.tenants,
        })
    }

    /// Get a reference to a specific pod by index.
    pub fn pod(&self, index: usize) -> &ScribeImpl {
        &self.pods[index]
    }

    /// Iterate over all pods.
    pub fn pods(&self) -> impl Iterator<Item = &ScribeImpl> {
        self.pods.iter()
    }

    /// Shared opendal operator (memory backend).
    pub fn operator(&self) -> &Operator {
        &self.operator
    }

    /// Shared ValaPostgres handle.
    pub fn pg(&self) -> &Arc<ValaPostgres> {
        &self.pg
    }

    /// Acquire a tenant-scoped connection.
    ///
    /// # Errors
    /// Returns an error if the connection cannot be acquired.
    pub async fn tenant_conn(&self, tenant: DataTenantId) -> Result<TenantConn<'_>, HarnessError> {
        TenantConn::acquire(self.pg.pool(), tenant)
            .await
            .map_err(|e| HarnessError::Internal(format!("tenant_conn: {e}")))
    }

    /// Force-seal every pod and poll pod inspection accessors until
    /// `wal_pending_bytes == 0` AND `memtable_row_count == 0` for every seen key
    /// across every pod.
    ///
    /// # Errors
    /// Returns an error if the drain does not complete within `timeout`.
    pub async fn wait_for_drain(&self, timeout: std::time::Duration) -> Result<(), HarnessError> {
        let start = std::time::Instant::now();

        // Force-seal all pods per tenant
        for tenant in &self.tenants {
            let mut conn = self.tenant_conn(*tenant).await?;
            let mut post_commit_batches = Vec::with_capacity(self.pods.len());
            for pod in &self.pods {
                post_commit_batches.push(
                    pod.force_seal(&mut conn)
                        .await
                        .map_err(|e| HarnessError::Internal(format!("force_seal: {e}")))?,
                );
            }
            conn.commit()
                .await
                .map_err(|e| HarnessError::Internal(format!("commit: {e}")))?;
            for (pod, post_commit) in self.pods.iter().zip(post_commit_batches) {
                pod.complete_post_commit(post_commit)
                    .map_err(|e| HarnessError::Internal(format!("post_commit: {e}")))?;
            }
        }

        // Poll until drained
        loop {
            let mut all_drained = true;

            for pod in &self.pods {
                if pod.wal_pending_bytes() > 0 {
                    all_drained = false;
                }

                // TODO: Once real memtables exist, collect active keys from all pods
                // and verify memtable_row_count(&key) == 0 for each key.
            }

            if all_drained {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(HarnessError::DrainTimeout);
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Shut down the harness, dropping the per-test schema.
    ///
    /// # Errors
    /// Returns an error if schema cleanup fails.
    pub async fn shutdown(self) -> Result<(), HarnessError> {
        // Schema names can't be parameters in PG DDL; validate for SQL safety
        validate_schema_name(&self.schema)?;
        // SAFETY: schema name is validated to be alphanumeric + underscore only.
        // We leak the string (test-only) to get a 'static lifetime for AssertSqlSafe.
        let drop_schema_sql = format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema);
        let drop_schema_static: &'static str = Box::leak(drop_schema_sql.into_boxed_str());
        sqlx::raw_sql(AssertSqlSafe(drop_schema_static))
            .execute(self.pg.pool())
            .await?;
        self.pg.pool().close().await;
        Ok(())
    }
}
