use std::sync::Arc;

use dashmap::DashMap;
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;
use crate::types::TableUid;

/// Cache key — control-plane owner first, then table UID.
///
/// `table_uid` is NOT globally unique (PK is `(data_tenant_id, table_uid)`),
/// so a UID-only key could serve one tenant's metadata to another. `owner` is
/// `scope.control_bind(data_tenant)` (`SystemShared` → `SYSTEM_OWNER`).
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct RegistryKey {
    pub owner: DataTenantId,
    pub table_uid: TableUid,
}

pub(crate) struct CachedMeta {
    pub row: vala_sql::row_types::olap_catalog::BifrostTableRow,
    pub iceberg_table: Arc<iceberg::table::Table>,
    pub refresh_epoch: u64,
}

pub(crate) struct Registry {
    pool: Arc<PgPool>,
    by_key: DashMap<RegistryKey, Arc<CachedMeta>>,
}

impl Registry {
    pub(crate) fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool,
            by_key: DashMap::new(),
        }
    }

    /// Resolve a `vala.bifrost_tables` row by FQN under a specific control-plane bind.
    ///
    /// Returns `(key, row)` on success; `TableNotFound` when no row exists.
    #[allow(dead_code)]
    pub(crate) async fn resolve(
        &self,
        fqn: &str,
        owner: DataTenantId,
    ) -> Result<
        (
            RegistryKey,
            vala_sql::row_types::olap_catalog::BifrostTableRow,
        ),
        BifrostError,
    > {
        let mut conn = vala_sql::TenantConn::acquire(&self.pool, owner)
            .await
            .map_err(BifrostError::Sql)?;
        let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, fqn)
            .await
            .map_err(BifrostError::Sql)?
            .ok_or_else(|| BifrostError::TableNotFound(fqn.to_string()))?;
        conn.commit().await.map_err(BifrostError::Sql)?;

        let table_uid = TableUid(
            row.table_uid
                .as_slice()
                .try_into()
                .map_err(|_| BifrostError::Internal("table_uid length mismatch".into()))?,
        );
        Ok((RegistryKey { owner, table_uid }, row))
    }

    /// One narrow PK-indexed `vala.refresh_epochs` read, bound to `key.owner`.
    pub(crate) async fn current_epoch(&self, key: &RegistryKey) -> Result<u64, BifrostError> {
        let mut conn = vala_sql::TenantConn::acquire(&self.pool, key.owner)
            .await
            .map_err(BifrostError::Sql)?;
        let epoch =
            vala_sql::queries::olap_catalog::current_epoch(&mut conn, key.table_uid.as_bytes())
                .await
                .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        // `current_epoch` returns 0 when no row exists; the counter is never negative.
        Ok(u64::try_from(epoch).unwrap_or(0))
    }

    /// Cache hit iff an entry exists at `key` AND its epoch matches.
    pub(crate) fn lookup_cached(&self, key: &RegistryKey, epoch: u64) -> Option<Arc<CachedMeta>> {
        self.by_key.get(key).and_then(|e| {
            if e.refresh_epoch == epoch {
                Some(Arc::clone(&*e))
            } else {
                None
            }
        })
    }

    pub(crate) fn store(&self, key: RegistryKey, meta: Arc<CachedMeta>) {
        self.by_key.insert(key, meta);
    }

    /// List all `vala.bifrost_tables` rows visible to `tenant` (`TenantOwned`) plus
    /// all `SystemShared` rows (visible under `SYSTEM_OWNER`).
    #[allow(dead_code)]
    pub(crate) async fn list_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<vala_sql::row_types::olap_catalog::BifrostTableRow>, BifrostError> {
        let mut rows: Vec<vala_sql::row_types::olap_catalog::BifrostTableRow> = Vec::new();

        let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let tenant_rows = vala_sql::queries::olap_catalog::list_tables_for_tenant(&mut conn)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        rows.extend(tenant_rows);

        if tenant != DataTenantId::SYSTEM_OWNER {
            let mut sys_conn =
                vala_sql::TenantConn::acquire(&self.pool, DataTenantId::SYSTEM_OWNER)
                    .await
                    .map_err(BifrostError::Sql)?;
            let sys_rows = vala_sql::queries::olap_catalog::list_tables_for_tenant(&mut sys_conn)
                .await
                .map_err(BifrostError::Sql)?;
            sys_conn.commit().await.map_err(BifrostError::Sql)?;
            rows.extend(sys_rows);
        }

        Ok(rows)
    }
}
