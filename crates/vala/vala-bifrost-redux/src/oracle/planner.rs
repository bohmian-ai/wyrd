//! SQL planning owner for Oracle's immutable visibility cut.
//!
//! Planning validates read-only requests, records the query's active table
//! reads and pins tenant-qualified metadata under one deadline, and builds
//! session-local providers without performing admission, audit, or row
//! execution side effects. The query class is derived
//! later from the physical root alone, so nothing here classifies.

use std::sync::Arc;
use std::time::Instant;

use num_traits::ToPrimitive;

use super::*;
use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, ClaimedSqlCut,
    PlannedSqlCut, TableRef,
};
use crate::catalog::{PinnedSealedTable, TableUid, TenantTableBinding};
use vala_sql::queries::oracle_reader_authority::ActiveReadOwner;

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
}

/// One query's committed active table reads.
///
/// Created only by [`OraclePlanner::pin_cut`] after its acquisition statement
/// commits, and carried inside [`ClaimedSqlCut`] and then the query's terminal
/// owner. Release is explicit and asynchronous; dropping a claim performs no
/// SQL and leaves its rows to PostgreSQL-time abandonment once this node's
/// Oracle fence is no longer live.
pub(crate) struct ActiveReadClaim {
    /// Catalog owning the tenant-scoped active-read statements.
    catalog: Arc<BifrostCatalog>,
    /// Tenant whose RLS scopes the rows.
    tenant: wyrd_spec::DataTenantId,
    /// Durable query identity the rows are keyed by.
    query_id: uuid::Uuid,
}

impl std::fmt::Debug for ActiveReadClaim {
    /// Prints the claim's identity without its catalog handle.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActiveReadClaim")
            .field("query_id", &self.query_id)
            .finish_non_exhaustive()
    }
}

impl ActiveReadClaim {
    /// Deletes every active read this query holds.
    ///
    /// Called only once nothing descended from the query can read its cut
    /// again. A failed release is logged and counted rather than surfaced:
    /// the query already has its terminal outcome, and the rows stay
    /// protective until PostgreSQL-time abandonment reclaims them.
    pub(crate) async fn release(self) {
        if let Err(error) = self
            .catalog
            .release_active_reads(self.tenant, self.query_id)
            .await
        {
            metrics::counter!("bifrost_oracle_active_read_release_failures_total").increment(1);
            tracing::warn!(
                error = %error,
                query_id = %self.query_id,
                "Oracle could not release active table reads; rows remain until abandonment"
            );
        }
    }
}

impl OraclePlanner {
    /// Acquires the complete active cut and materializes every table from it,
    /// reacquiring once if a selected metadata document is already gone.
    ///
    /// One SQL statement records the active reads and returns every pointer
    /// and hot candidate; it commits before any object IO. The resolved
    /// tables are authorized next, still before any object IO, and then every
    /// table's metadata and manifests are read concurrently. Only an
    /// object-store `NotFound` on a selected document — a catalog move that
    /// already deleted it — reacquires, and only once; the replay refreshes
    /// the same rows.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryForbidden`] when the caller's grants do not
    /// cover every resolved table, the public catalog error for an
    /// unregistered table or a failed acquisition or materialization, and the
    /// second `NotFound` as the catalog's public failure.
    async fn acquire_and_materialize(
        tables: &[TableRef],
        context: &AuthorizedQueryContext,
        catalog: &BifrostCatalog,
        owner: ActiveReadOwner,
    ) -> Result<Vec<PinnedSealedTable>, BifrostError> {
        let tenant = context.data_tenant_id;
        let mut reacquired = false;
        loop {
            let acquired = catalog
                .acquire_active_cut(tenant, owner, tables)
                .await
                .map_err(BifrostCatalogError::into_public)?;
            let identities = acquired
                .iter()
                .zip(tables)
                .map(|(cut, table)| {
                    TenantTableBinding::resolve((tenant, table.clone()))
                        .map(|binding| (binding, TableUid::from_bytes(cut.identity.table_uid)))
                        .map_err(|error| {
                            BifrostCatalogError::InvalidBinding(error.to_string()).into_public()
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            // Authorized on the acquired identities, before any metadata,
            // manifest, or data object is opened: the first point at which
            // every table has its registered UID.
            super::authorize_resolved_tables(context, &identities)?;
            let materialized = futures_util::future::try_join_all(
                acquired
                    .into_iter()
                    .zip(tables)
                    .map(|(cut, table)| catalog.materialize_acquired_cut(tenant, table, cut)),
            )
            .await;
            match materialized {
                Ok(cuts) => return Ok(cuts),
                Err(error) if error.is_missing_object() && !reacquired => {
                    tracing::warn!(
                        tables = tables.len(),
                        "Oracle reacquired its active cut after a selected metadata document was removed"
                    );
                    reacquired = true;
                }
                Err(error) => return Err(error.into_public()),
            }
        }
    }

    /// Pins the immutable source cut for one SQL attempt.
    ///
    /// The planner owns parsing-adjacent metadata work and never executes rows;
    /// provider installation remains an Oracle composition concern after audit.
    /// The whole pin — acquisition, authorization, and materialization — runs
    /// under the leader deadline. Every failure after this call starts,
    /// including the deadline, releases the query's active reads before it
    /// returns, so only a successful pin hands a claim onward. Nothing here
    /// derives a class — the class comes from the physical root built on top
    /// of this cut.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTimeout`] when the deadline passes first,
    /// and authorization, catalog, or byte-accounting failures otherwise.
    ///
    /// # Cancellation
    /// Dropping the future after acquisition committed leaves the rows to
    /// PostgreSQL-time abandonment.
    pub(super) async fn pin_cut(
        &self,
        context: &AuthorizedQueryContext,
        tables: &[TableRef],
        deadline: Instant,
        catalog: &Arc<BifrostCatalog>,
        owner: ActiveReadOwner,
    ) -> Result<ClaimedSqlCut, BifrostError> {
        // DEBUG, not INFO: one event per catalog pin per query is per-request
        // decision detail, not a lifecycle transition. It is the only way to
        // attribute pre-fragment query latency, which is otherwise invisible
        // between admission and the first fragment dispatch.
        let pin_started = std::time::Instant::now();
        let claim = ActiveReadClaim {
            catalog: Arc::clone(catalog),
            tenant: context.data_tenant_id,
            query_id: owner.query_id,
        };
        let planned = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            Self::acquire_and_materialize(tables, context, catalog, owner),
        )
        .await
        .map_err(|_| BifrostError::QueryTimeout)
        .and_then(|cuts| cuts)
        .and_then(|cuts| {
            let ratio = local_ratio(&cuts)?;
            Ok((cuts, ratio))
        });
        let (cuts, local_ratio) = match planned {
            Ok(planned) => planned,
            Err(error) => {
                claim.release().await;
                return Err(error);
            }
        };
        tracing::debug!(
            tables = tables.len(),
            hot_files = cuts.iter().map(|cut| cut.hot_files.len()).sum::<usize>(),
            iceberg_files = cuts
                .iter()
                .map(|cut| cut.iceberg_files.len())
                .sum::<usize>(),
            pin_ms = pin_started.elapsed().as_millis(),
            "Oracle pinned one sealed cut"
        );
        Ok(ClaimedSqlCut {
            cut: PlannedSqlCut { cuts, local_ratio },
            claim,
        })
    }
}

/// Derives the fraction of a cut's pinned bytes held in the local hot tier.
///
/// # Errors
/// Returns [`BifrostError::QueryAdmissionRejected`] when a persisted size is
/// negative or the byte totals overflow.
fn local_ratio(cuts: &[PinnedSealedTable]) -> Result<f64, BifrostError> {
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
    if total_bytes == 0 {
        return Ok(0.0);
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

    use super::register_session_table;
    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use wyrd_spec::DataTenantId;

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
