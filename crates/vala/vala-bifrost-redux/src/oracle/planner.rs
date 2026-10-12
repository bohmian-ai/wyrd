//! SQL planning owner for Oracle's immutable visibility cut.
//!
//! Planning validates read-only requests, reads the query's cut and pins
//! tenant-qualified metadata under one deadline, and builds
//! session-local providers without performing admission, audit, or row
//! execution side effects. The query class is derived
//! later from the physical root alone, so nothing here classifies.

use std::sync::Arc;
use std::time::Instant;

use num_traits::ToPrimitive;

use super::*;
use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, PlannedSqlCut,
    TableRef,
};
use crate::catalog::{PinnedSealedTable, TableUid, TenantTableBinding};

/// Query floor and logical-plan preparation owner.
///
/// Snapshot preparation has no admission bound of its own: concurrent pins
/// wait for a connection from the bounded runtime Postgres pool inside their
/// leader deadline, and execution is bounded later by Oracle's query
/// admission.
#[derive(Debug, Clone)]
pub struct OraclePlanner {
    /// Synchronous floor, deadline, and capacity configuration.
    pub(super) config: OracleConfig,
}

impl OraclePlanner {
    /// Creates a planner with bounded synchronous validation settings.
    #[must_use]
    pub fn new(config: OracleConfig) -> Self {
        Self { config }
    }

    /// Validates one non-empty, single-statement `SELECT` request.
    ///
    /// Validation already parses the statement, so it returns the distinct
    /// canonical table references that parse produced.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryInvalidSql`] for floor violations.
    pub fn validate_query(
        &self,
        request: &BifrostQueryRequest,
    ) -> Result<Vec<TableRef>, BifrostError> {
        request
            .validate()
            .map_err(|error| BifrostError::QueryInvalidSql {
                detail: error.to_string(),
            })?;
        if request.sql.len() > self.config.max_sql_bytes {
            return Err(BifrostError::QueryInvalidSql {
                detail: "query exceeds configured SQL byte limit".to_owned(),
            });
        }
        parse_select_tables(&request.sql)
    }

    /// Projects one request's `deadline_ms` to the total budget it runs under.
    ///
    /// A positive value is honored up to this planner's maximum deadline and
    /// clamped to it above. An omitted, zero, or negative value takes the
    /// configured default, so every entry that owns a planner built from the
    /// boot-resolved configuration — local Oracle entry and public forwarding
    /// alike — resolves the same budget. The maximum is what Forge's snapshot
    /// retention is sized against, so no query outlives the snapshot it read.
    #[must_use]
    pub fn request_deadline(&self, requested_ms: Option<i64>) -> Duration {
        requested_ms
            .and_then(|deadline_ms| u64::try_from(deadline_ms).ok())
            .filter(|deadline_ms| *deadline_ms != 0)
            .map_or(self.config.default_deadline, Duration::from_millis)
            .min(self.config.max_deadline)
    }
}

impl OraclePlanner {
    /// Reads the complete cut and materializes every table from it, rereading
    /// once if a selected metadata document is already gone.
    ///
    /// One read-only SQL statement returns every pointer and hot candidate.
    /// The resolved tables are authorized next, still before any object IO,
    /// and then every table's metadata and manifests are read concurrently.
    /// Only an object-store `NotFound` on a selected document — a catalog move
    /// that already deleted it — rereads, and only once. The first read may
    /// come from the node's cut cache, bounded by `not_before`; the reread
    /// always reads Postgres.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryForbidden`] when the caller's grants do not
    /// cover every resolved table, the public catalog error for an
    /// unregistered table or a failed read or materialization, and the second
    /// `NotFound` as the catalog's public failure.
    async fn read_and_materialize(
        tables: &[TableRef],
        context: &AuthorizedQueryContext,
        catalog: &BifrostCatalog,
        profile: &super::profile::QueryProfile,
        not_before: Option<Instant>,
    ) -> Result<Vec<PinnedSealedTable>, BifrostError> {
        let tenant = context.data_tenant_id;
        let mut reread = false;
        loop {
            let read = profile.cut_read.timer();
            let cut = if reread {
                catalog.read_cut(tenant, tables).await
            } else {
                catalog.read_cut_cached(tenant, tables, not_before).await
            };
            let cut = cut.map_err(BifrostCatalogError::into_public)?;
            read.done();
            let identities = cut
                .iter()
                .zip(tables)
                .map(|(table_cut, table)| {
                    TenantTableBinding::resolve((tenant, table.clone()))
                        .map(|binding| {
                            (binding, TableUid::from_bytes(table_cut.identity.table_uid))
                        })
                        .map_err(|error| {
                            BifrostCatalogError::InvalidBinding(error.to_string()).into_public()
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            // Authorized on the resolved identities, before any metadata,
            // manifest, or data object is opened: the first point at which
            // every table has its registered UID.
            super::authorize_resolved_tables(context, &identities)?;
            let materialize = profile.cut_materialize.timer();
            let materialized = futures_util::future::try_join_all(
                cut.into_iter()
                    .zip(tables)
                    .map(|(table_cut, table)| catalog.materialize_cut(tenant, table, table_cut)),
            )
            .await;
            materialize.done();
            match materialized {
                Ok(cuts) => return Ok(cuts),
                Err(error) if error.is_missing_object() && !reread => {
                    tracing::warn!(
                        tables = tables.len(),
                        "Oracle reread its cut after a selected metadata document was removed"
                    );
                    reread = true;
                }
                Err(error) => return Err(error.into_public()),
            }
        }
    }

    /// Pins the immutable source cut for one SQL attempt.
    ///
    /// The planner owns parsing-adjacent metadata work and never executes rows;
    /// provider installation remains an Oracle composition concern after audit.
    /// The whole pin — cut read, authorization, and materialization — runs
    /// under the leader deadline. Nothing durable is recorded, so a failed or
    /// dropped pin leaves nothing to release. Nothing here derives a class —
    /// the class comes from the physical root built on top of this cut.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTimeout`] when the deadline passes first,
    /// and authorization, catalog, or byte-accounting failures otherwise.
    pub(super) async fn pin_cut(
        &self,
        context: &AuthorizedQueryContext,
        tables: &[TableRef],
        deadline: Instant,
        catalog: &Arc<BifrostCatalog>,
    ) -> Result<PlannedSqlCut, BifrostError> {
        let profile = Arc::new(super::profile::QueryProfile::new());
        let cuts = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            Self::read_and_materialize(
                tables,
                context,
                catalog,
                &profile,
                deadline.checked_sub(self.config.max_deadline),
            ),
        )
        .await
        .map_err(|_| BifrostError::QueryTimeout)??;
        let warehouse_local = matches!(
            catalog.storage().backend(),
            wyrd_storage::BackendConfig::Local { .. }
        );
        let local_ratio = local_ratio(&cuts, warehouse_local)?;
        tracing::debug!(
            tables = tables.len(),
            hot_files = cuts.iter().map(|cut| cut.hot_files.len()).sum::<usize>(),
            iceberg_files = cuts
                .iter()
                .map(|cut| cut.iceberg_files.len())
                .sum::<usize>(),
            "Oracle pinned one sealed cut"
        );
        Ok(PlannedSqlCut {
            cuts,
            local_ratio,
            profile,
        })
    }
}

/// Derives the fraction of a cut's pinned bytes this node reads from local disk.
///
/// Hot (staged) files are always local. Published and compacted Iceberg files
/// are local only when the warehouse itself is the local filesystem, and
/// remote on S3, GCS, or Azure. A cut with no sealed bytes reads nothing
/// remote, so it is fully local: extra partitions would only hide IO latency
/// that does not exist.
///
/// # Errors
/// Returns [`BifrostError::QueryAdmissionRejected`] when a persisted size is
/// negative or the byte totals overflow.
fn local_ratio(cuts: &[PinnedSealedTable], warehouse_local: bool) -> Result<f64, BifrostError> {
    let local_bytes = cuts.iter().try_fold(0_u64, |total, cut| {
        cut.hot_files.iter().try_fold(total, |total, file| {
            let bytes =
                u64::try_from(file.file_size).map_err(|_| BifrostError::QueryAdmissionRejected)?;
            total
                .checked_add(bytes)
                .ok_or(BifrostError::QueryAdmissionRejected)
        })
    })?;
    let remote_bytes = cuts.iter().try_fold(0_u64, |total, cut| {
        cut.iceberg_files.iter().try_fold(total, |total, file| {
            total
                .checked_add(file.file_size)
                .ok_or(BifrostError::QueryAdmissionRejected)
        })
    })?;
    let total_bytes = local_bytes
        .checked_add(remote_bytes)
        .ok_or(BifrostError::QueryAdmissionRejected)?;
    if warehouse_local || total_bytes == 0 {
        return Ok(1.0);
    }
    local_bytes
        .to_f64()
        .zip(total_bytes.to_f64())
        .map(|(local, total)| local / total)
        .ok_or(BifrostError::QueryAdmissionRejected)
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::MemTable;
    use datafusion::execution::context::SessionContext;
    use std::sync::Arc;

    use super::{local_ratio, register_session_table};
    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use wyrd_spec::DataTenantId;

    /// A cut with no sealed bytes reads nothing remote, so it is fully local
    /// on every warehouse backend.
    #[test]
    fn empty_cut_is_fully_local() {
        assert!((local_ratio(&[], false).expect("empty remote cut") - 1.0).abs() < f64::EPSILON);
        assert!((local_ratio(&[], true).expect("empty local cut") - 1.0).abs() < f64::EPSILON);
    }

    /// Both canonical hierarchy and quoted dotted-FQN SQL resolve identically.
    #[tokio::test]
    async fn quoted_dotted_identifier_resolves_alongside_canonical_name() {
        let session = SessionContext::new();
        let binding = TenantTableBinding::resolve((
            DataTenantId::new(uuid::Uuid::now_v7()).expect("test tenant identity"),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("test table binding");
        let provider = MemTable::try_new(
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
            vec![Vec::new()],
        )
        .expect("schema-only provider");
        register_session_table(&session, &binding, Arc::new(provider))
            .expect("register canonical and quoted aliases");

        for sql in [
            "SELECT * FROM vala.traces.spans",
            "SELECT * FROM \"vala.traces.spans\"",
        ] {
            session
                .sql(sql)
                .await
                .unwrap_or_else(|error| panic!("{sql} must resolve: {error}"));
        }
    }
}
