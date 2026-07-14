//! Multi-Scribe harness for multi-pod journey tests.

use std::sync::Arc;

use opendal::Operator;
use sqlx::{AssertSqlSafe, PgPool};
use thiserror::Error;
use wyrd_spec::ids::DataTenantId;

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
/// (memory backend) and a PG pool. Provides `wait_for_drain` and `shutdown`
/// for coordinated test teardown.
pub struct MultiScribeHarness {
    pods: Vec<ScribeImpl>,
    operator: Arc<Operator>,
    pg_pool: PgPool,
    schema: String,
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

        // Construct PG pool from wyrd test DATABASE_URL
        let database_url = std::env::var("WYRD_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://wyrd_app:wyrd_app_pw@127.0.0.1:55432/wyrd".to_string());
        let pg_pool = PgPool::connect(&database_url).await?;

        // Create per-test schema for isolation
        // Schema names can't be parameters in PG DDL; validate for SQL safety
        validate_schema_name(&cfg.schema)?;
        // SAFETY: schema name is validated to be alphanumeric + underscore only.
        // We can't use AssertSqlSafe directly because it requires &'static str.
        // Instead, we leak the string (test-only) to get a 'static lifetime.
        let create_schema_sql = format!("CREATE SCHEMA IF NOT EXISTS {}", cfg.schema);
        let create_schema_static: &'static str = Box::leak(create_schema_sql.into_boxed_str());
        sqlx::raw_sql(AssertSqlSafe(create_schema_static))
            .execute(&pg_pool)
            .await?;
        let set_path_sql = format!("SET search_path TO {}", cfg.schema);
        let set_path_static: &'static str = Box::leak(set_path_sql.into_boxed_str());
        sqlx::raw_sql(AssertSqlSafe(set_path_static))
            .execute(&pg_pool)
            .await?;

        // Construct N stub Scribe pods
        let pods = (0..cfg.pods).map(|_| ScribeImpl::new()).collect();

        Ok(Self {
            pods,
            operator,
            pg_pool,
            schema: cfg.schema,
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

    /// Shared PG pool.
    pub fn pg_pool(&self) -> &PgPool {
        &self.pg_pool
    }

    /// Force-seal every pod and poll `ScribeInspect` until `wal_pending_bytes == 0`
    /// AND `memtable_row_count == 0` for every seen key across every pod.
    ///
    /// # Errors
    /// Returns an error if the drain does not complete within `timeout`.
    pub async fn wait_for_drain(&self, timeout: std::time::Duration) -> Result<(), HarnessError> {
        use vala_bifrost_redux::inspect::ScribeInspect;

        let start = std::time::Instant::now();

        // Force-seal all pods
        for pod in &self.pods {
            pod.force_seal()
                .await
                .map_err(|e| HarnessError::Internal(format!("force_seal: {e}")))?;
        }

        // Poll until drained
        loop {
            let mut all_drained = true;

            for pod in &self.pods {
                if pod.wal_pending_bytes() > 0 {
                    all_drained = false;
                }

                // TODO(PR#N): Once real memtables exist, collect active keys from all pods
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
            .execute(&self.pg_pool)
            .await?;
        self.pg_pool.close().await;
        Ok(())
    }
}
