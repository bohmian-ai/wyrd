//! Background cleanup for expired multipart upload control-plane rows.
//!
//! The sweeper is a server-tier worker. It uses the platform-admin pool to
//! discover cross-tenant expired rows, then audits each row-level mutation under
//! that row's tenant through a tenant-scoped connection.

use crate::env_parse::{parse_bool_optional, parse_clamped_i64, parse_u64_optional};
use crate::error::StorageError;
use crate::{StorageHandle, tenant_path};
use sqlx::PgPool;
use sqlx::pool::PoolConnection;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::instrument;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, WireProtocol};

/// Session-level Postgres advisory lock key for sweeper leadership.
pub const SWEEPER_LEADER_LOCK_KEY: i64 = 0x5759_7264_5374_6f72_i64;

/// Default interval between sweeper ticks.
pub const DEFAULT_TICK: Duration = Duration::from_mins(1);
/// Default maximum multipart rows processed per tick.
pub const DEFAULT_BATCH_SIZE: u64 = 100;
/// Default age after which `initiating` rows are treated as orphaned.
pub const DEFAULT_INIT_GRACE: Duration = Duration::from_secs(30);
/// Default maximum idempotency rows deleted per tick.
pub const DEFAULT_IDEMPOTENCY_BATCH_SIZE: u64 = 500;

const ENV_ENABLED: &str = "WYRD_STORAGE_SWEEPER_ENABLED";
const ENV_TICK_SECS: &str = "WYRD_STORAGE_SWEEPER_TICK_SECS";
const ENV_BATCH_SIZE: &str = "WYRD_STORAGE_SWEEPER_BATCH_SIZE";
const ENV_INIT_GRACE_SECS: &str = "WYRD_STORAGE_SWEEPER_INIT_GRACE_SECS";
const ENV_IDEMPOTENCY_BATCH_SIZE: &str = "WYRD_STORAGE_SWEEPER_IDEMPOTENCY_BATCH_SIZE";

/// Runtime configuration for the storage sweeper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweeperConfig {
    /// Whether the worker should run.
    pub enabled: bool,
    /// Interval between ticks.
    pub tick: Duration,
    /// Maximum multipart upload rows swept per tick.
    pub batch_size: i64,
    /// Age after which an `initiating` row is treated as orphaned.
    pub init_grace: Duration,
    /// Maximum idempotency rows deleted per tick.
    pub idempotency_batch_size: i64,
}

impl SweeperConfig {
    /// Load sweeper configuration from process environment.
    ///
    /// # Errors
    /// Returns [`StorageError::ConfigParse`] with the exact variable name when
    /// an environment value cannot be parsed.
    pub fn from_env() -> Result<Self, StorageError> {
        Ok(Self {
            enabled: parse_bool_optional(ENV_ENABLED)?.unwrap_or(true),
            tick: Duration::from_secs(
                parse_u64_optional(ENV_TICK_SECS)?
                    .unwrap_or(DEFAULT_TICK.as_secs())
                    .clamp(10, 3600),
            ),
            batch_size: parse_clamped_i64(
                ENV_BATCH_SIZE,
                DEFAULT_BATCH_SIZE,
                1,
                1000,
            )?,
            init_grace: Duration::from_secs(
                parse_u64_optional(ENV_INIT_GRACE_SECS)?
                    .unwrap_or(DEFAULT_INIT_GRACE.as_secs())
                    .clamp(5, 600),
            ),
            idempotency_batch_size: parse_clamped_i64(
                ENV_IDEMPOTENCY_BATCH_SIZE,
                DEFAULT_IDEMPOTENCY_BATCH_SIZE,
                1,
                5000,
            )?,
        })
    }
}

impl Default for SweeperConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tick: DEFAULT_TICK,
            batch_size: DEFAULT_BATCH_SIZE as i64,
            init_grace: DEFAULT_INIT_GRACE,
            idempotency_batch_size: DEFAULT_IDEMPOTENCY_BATCH_SIZE as i64,
        }
    }
}

/// Background storage sweeper.
pub struct Sweeper {
    handle: Arc<StorageHandle>,
    admin_pool: PgPool,
    cfg: SweeperConfig,
    shutdown: watch::Receiver<bool>,
}

impl Sweeper {
    /// Build a sweeper from already-constructed process state.
    #[must_use]
    pub fn new(
        handle: Arc<StorageHandle>,
        admin_pool: PgPool,
        cfg: SweeperConfig,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        Self {
            handle,
            admin_pool,
            cfg,
            shutdown,
        }
    }

    /// Run the sweeper loop until shutdown is signaled.
    pub async fn run(mut self) {
        if !self.cfg.enabled {
            tracing::info!("storage sweeper disabled");
            return;
        }

        tracing::info!(
            tick_secs = self.cfg.tick.as_secs(),
            batch_size = self.cfg.batch_size,
            init_grace_secs = self.cfg.init_grace.as_secs(),
            idempotency_batch_size = self.cfg.idempotency_batch_size,
            "storage sweeper starting"
        );

        let mut ticker = tokio::time::interval(self.cfg.tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(error) = self.tick().await {
                        tracing::error!(error = %error, "storage sweeper tick failed");
                    }
                }
                changed = self.shutdown.changed() => {
                    if changed.is_err() || *self.shutdown.borrow() {
                        tracing::info!("storage sweeper shutting down");
                        return;
                    }
                }
            }
        }
    }

    /// Execute one leader-elected cleanup pass.
    ///
    /// # Errors
    /// Returns a storage error when leader election or batch discovery fails.
    #[instrument(skip(self), fields(batch_size = self.cfg.batch_size))]
    pub async fn tick(&self) -> Result<(), StorageError> {
        let Some(_leader_conn) = self.try_acquire_leader().await? else {
            tracing::debug!("storage sweeper skipped tick because another leader is active");
            return Ok(());
        };

        self.sweep_expired_uploads_batch().await?;
        self.reap_expired_idempotency_keys().await;
        Ok(())
    }

    /// Attempt to hold the sweeper leader advisory lock for this tick.
    ///
    /// # Errors
    /// Returns a storage error when the admin pool cannot provide a connection
    /// or Postgres rejects the advisory-lock query.
    pub async fn try_acquire_leader(
        &self,
    ) -> Result<Option<PoolConnection<sqlx::Postgres>>, StorageError> {
        let mut conn = self
            .admin_pool
            .acquire()
            .await
            .map_err(|source| StorageError::AdminPool { source })?;
        let acquired =
            wyrd_sql::queries::storage::admin::multipart_uploads::try_acquire_leader_lock(
                conn.as_mut(),
                SWEEPER_LEADER_LOCK_KEY,
            )
            .await?;

        if acquired {
            conn.close_on_drop();
            Ok(Some(conn))
        } else {
            Ok(None)
        }
    }

    async fn sweep_expired_uploads_batch(&self) -> Result<(), StorageError> {
        let rows = wyrd_sql::queries::storage::admin::multipart_uploads::expired_uploads_batch(
            &self.admin_pool,
            self.cfg.batch_size,
            self.cfg.init_grace,
        )
        .await?;

        if rows.is_empty() {
            tracing::debug!("storage sweeper found no expired multipart uploads");
            return Ok(());
        }

        tracing::info!(
            count = rows.len(),
            "storage sweeper found expired multipart uploads"
        );

        for row in rows {
            if let Err(error) = self.abort_one(&row).await {
                tracing::error!(
                    upload_id = %row.id,
                    data_tenant_id = %row.data_tenant_id,
                    backend = %row.backend,
                    error_class = classify_error(&error),
                    error = %error,
                    "storage sweeper row failed"
                );
            }
        }

        Ok(())
    }

    async fn reap_expired_idempotency_keys(&self) {
        match wyrd_sql::queries::storage::admin::idempotency::reap_idempotency_keys(
            &self.admin_pool,
            self.cfg.idempotency_batch_size,
        )
        .await
        {
            Ok(0) => tracing::debug!("storage sweeper found no expired idempotency keys"),
            Ok(count) => tracing::info!(count, "storage sweeper reaped expired idempotency keys"),
            Err(error) => {
                tracing::error!(
                    error_class = classify_error(&StorageError::Sql(error)),
                    "storage sweeper idempotency reaper failed"
                );
            }
        }
    }

    async fn abort_one(
        &self,
        row: &wyrd_sql::queries::storage::admin::multipart_uploads::ExpiredUpload,
    ) -> Result<(), StorageError> {
        let data_tenant_id = DataTenantId::try_from(row.data_tenant_id).map_err(|error| {
            StorageError::TenantPathMismatch(format!(
                "row {} has invalid data_tenant_id: {error}",
                row.id
            ))
        })?;
        let validated = tenant_path::validate(&row.storage_path, data_tenant_id)
            .map_err(|error| StorageError::TenantPathMismatch(error.to_string()))?;
        let backend =
            StorageBackendKind::from_str(&row.backend).map_err(|error| StorageError::Backend {
                backend: self.handle.backend(),
                op: "sweeper_parse_backend",
                message: error.to_string(),
            })?;
        let wire_protocol =
            WireProtocol::from_str(&row.wire_protocol).map_err(|error| StorageError::Backend {
                backend,
                op: "sweeper_parse_wire_protocol",
                message: error.to_string(),
            })?;

        let abort_result = if row.status == "initiating" && row.backend_upload_id.is_none() {
            tracing::info!(
                upload_id = %row.id,
                "storage sweeper found orphaned initiating row without backend upload id"
            );
            Ok(())
        } else if matches!(
            wire_protocol,
            WireProtocol::SinglePutV1
                | WireProtocol::GcsResumableV1
                | WireProtocol::AzureBlockBlobV1
                | WireProtocol::LocalFsV1
        ) && row.backend_upload_id.is_none()
        {
            tracing::info!(
                upload_id = %row.id,
                wire_protocol = %wire_protocol,
                "storage sweeper found upload with no backend abort handle"
            );
            Ok(())
        } else if let Some(backend_upload_id) = row.backend_upload_id.as_deref() {
            self.handle
                .signer()
                .abort_multipart(&validated, backend_upload_id)
                .await
        } else {
            Ok(())
        };

        let (status_code, error_code) = match &abort_result {
            Ok(()) => (200, None),
            Err(error) => {
                tracing::warn!(
                    upload_id = %row.id,
                    error_class = classify_error(error),
                    error = %error,
                    "storage sweeper backend abort failed; marking control-plane row aborted"
                );
                (500, Some(classify_error(error)))
            }
        };

        let rows_updated = wyrd_sql::queries::storage::admin::multipart_uploads::mark_aborted_admin(
            &self.admin_pool,
            row.id,
            "sweeper-ttl-expired",
        )
        .await?;

        if rows_updated == 0 {
            tracing::debug!(
                upload_id = %row.id,
                "sweeper race: upload completed before abort, skipping audit"
            );
            return Ok(());
        }

        self.audit_abort(data_tenant_id, row, backend, status_code, error_code)
            .await
    }

    async fn audit_abort(
        &self,
        data_tenant_id: DataTenantId,
        row: &wyrd_sql::queries::storage::admin::multipart_uploads::ExpiredUpload,
        backend: StorageBackendKind,
        status_code: i32,
        error_code: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut conn = wyrd_sql::TenantConn::acquire(&self.admin_pool, data_tenant_id)
            .await
            .map_err(StorageError::Sql)?;
        let request_id = format!("storage-sweeper-{}", row.id);

        wyrd_sql::queries::platform::audit_log::write_storage_event(
            &mut conn,
            wyrd_sql::queries::platform::audit_log::StorageAuditEvent {
                subject_id: "storage-sweeper",
                operation: "sweeper_abort",
                storage_path: row.storage_path.as_str(),
                status_code,
                error_code,
                request_id: request_id.as_str(),
                backend,
                upload_id: Some(row.id),
            },
        )
        .await?;
        conn.commit().await?;

        Ok(())
    }
}

fn classify_error(error: &StorageError) -> &'static str {
    match error {
        StorageError::AdminPool { .. } => "admin_pool",
        StorageError::BackendCapabilityMismatch { .. } => "capability_mismatch",
        StorageError::ConfigParse { .. } => "config_parse",
        StorageError::Sql(_) => "sql",
        StorageError::TenantPathMismatch(_) => "tenant_path",
        _ => "backend",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_BATCH_SIZE, DEFAULT_IDEMPOTENCY_BATCH_SIZE, DEFAULT_INIT_GRACE, DEFAULT_TICK,
        ENV_BATCH_SIZE, ENV_ENABLED, ENV_IDEMPOTENCY_BATCH_SIZE, ENV_INIT_GRACE_SECS,
        ENV_TICK_SECS, SweeperConfig,
    };
    use crate::StorageError;

    const ENV_KEYS: &[&str] = &[
        ENV_ENABLED,
        ENV_TICK_SECS,
        ENV_BATCH_SIZE,
        ENV_INIT_GRACE_SECS,
        ENV_IDEMPOTENCY_BATCH_SIZE,
    ];

    #[test]
    fn config_defaults_match_contract() {
        with_clean_env(Vec::new(), || {
            let cfg = SweeperConfig::from_env().expect("config parses");

            assert!(cfg.enabled);
            assert_eq!(cfg.tick, DEFAULT_TICK);
            assert_eq!(cfg.batch_size, DEFAULT_BATCH_SIZE as i64);
            assert_eq!(cfg.init_grace, DEFAULT_INIT_GRACE);
            assert_eq!(cfg.idempotency_batch_size, DEFAULT_IDEMPOTENCY_BATCH_SIZE as i64);
        });
    }

    #[test]
    fn config_parses_disabled_and_clamps_bounds() {
        with_clean_env(
            vec![
                (ENV_ENABLED, Some("false".to_owned())),
                (ENV_TICK_SECS, Some("1".to_owned())),
                (ENV_BATCH_SIZE, Some("0".to_owned())),
                (ENV_INIT_GRACE_SECS, Some("9999".to_owned())),
                (ENV_IDEMPOTENCY_BATCH_SIZE, Some("999999".to_owned())),
            ],
            || {
                let cfg = SweeperConfig::from_env().expect("config parses");

                assert!(!cfg.enabled);
                assert_eq!(cfg.tick.as_secs(), 10);
                assert_eq!(cfg.batch_size, 1);
                assert_eq!(cfg.init_grace.as_secs(), 600);
                assert_eq!(cfg.idempotency_batch_size, 5000);
            },
        );
    }

    #[test]
    fn config_reports_exact_var_for_bad_values() {
        with_clean_env(vec![(ENV_BATCH_SIZE, Some("abc".to_owned()))], || {
            let error = SweeperConfig::from_env().expect_err("bad integer");

            assert_config_var(&error, ENV_BATCH_SIZE);
        });

        with_clean_env(vec![(ENV_ENABLED, Some("maybe".to_owned()))], || {
            let error = SweeperConfig::from_env().expect_err("bad bool");

            assert_config_var(&error, ENV_ENABLED);
        });
    }

    fn with_clean_env(vars: Vec<(&'static str, Option<String>)>, f: impl FnOnce()) {
        let provided = vars.iter().map(|(key, _)| *key).collect::<Vec<_>>();
        let mut all = ENV_KEYS
            .iter()
            .filter(|key| !provided.contains(key))
            .map(|key| (*key, None))
            .collect::<Vec<(&'static str, Option<String>)>>();
        all.extend(vars);
        temp_env::with_vars(all, f);
    }

    fn assert_config_var(error: &StorageError, expected: &'static str) {
        match error {
            StorageError::ConfigParse { var, .. } => assert_eq!(*var, expected),
            other => panic!("expected ConfigParse for {expected}, got {other:?}"),
        }
    }
}
