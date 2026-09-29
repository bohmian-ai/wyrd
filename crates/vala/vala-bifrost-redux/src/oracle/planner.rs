//! SQL planning owner for Oracle's immutable visibility cut.
//!
//! Planning validates read-only requests, pins tenant-qualified metadata under
//! one deadline, and builds session-local providers without performing
//! admission, audit, or row execution side effects. The query class is derived
//! later from the physical root alone, so nothing here classifies.

use std::sync::Arc;
use std::time::Instant;

use num_traits::ToPrimitive;

use super::*;
use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, PlannedSqlCut,
    ProtectedPlannedSqlCut, TableRef,
};
use crate::oracle::reader_pins::OracleReaderAuthority;

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

/// Why one protect-and-materialize attempt failed.
///
/// Only authoritative catalog promotion is worth restarting for; every other
/// failure would repeat identically, so it is reported as it stands.
enum AttemptFailure {
    /// Revalidation proved the catalog moved under this attempt.
    CatalogPromoted(BifrostError),
    /// A failure a restart cannot change.
    Fatal(BifrostError),
}

impl AttemptFailure {
    /// Unwraps the public error this attempt failed with.
    fn into_public(self) -> BifrostError {
        match self {
            Self::CatalogPromoted(error) | Self::Fatal(error) => error,
        }
    }
}

impl OraclePlanner {
    /// Prepares identities, takes one complete reader guard, revalidates, then
    /// materializes — restarting the whole thing once on catalog promotion.
    ///
    /// This is the only place a leader turns table references into readable
    /// cuts. The ordering is the protection contract: every identity is
    /// resolved from metadata alone, the complete resolved object set is
    /// authorized before a guard is even taken, one guard covers the whole set,
    /// every prepared table is revalidated against the authoritative catalog
    /// before any of them is materialized, and no snapshot-dependent source IO
    /// happens before that guard exists.
    ///
    /// Promotion between preparation and materialization invalidates the whole
    /// attempt, not one table: the guard covers a set of snapshots, and a set
    /// with one stale member protects nothing coherent. The guard is therefore
    /// dropped and every table is prepared, protected, revalidated, and
    /// materialized again. One restart only — a second drift is reported rather
    /// than chased.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTimeout`] when the deadline passes during
    /// preparation or materialization, [`BifrostError::QueryForbidden`] when
    /// the caller's grants do not cover every resolved table, the reader
    /// authority's refusal when the cut cannot be protected, a metadata-mismatch
    /// failure when the catalog was promoted twice under this query, and the
    /// catalog's public failure otherwise.
    async fn protect_and_materialize(
        tables: &[TableRef],
        context: &AuthorizedQueryContext,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: Option<&Arc<OracleReaderAuthority>>,
    ) -> Result<ProtectedPlannedSqlCut, BifrostError> {
        let Some(authority) = authority else {
            // A replica with no local Oracle role holds no reader epoch, so it
            // has nothing that could protect a snapshot it is about to read.
            return Err(BifrostError::OracleRoleUnavailable);
        };
        match Self::attempt_protected_cut(tables, context, deadline, catalog, authority).await {
            Err(AttemptFailure::CatalogPromoted(error)) => {
                tracing::warn!(
                    error = %error,
                    tables = tables.len(),
                    "Oracle restarted complete reader admission after catalog promotion"
                );
                Self::attempt_protected_cut(tables, context, deadline, catalog, authority)
                    .await
                    .map_err(AttemptFailure::into_public)
            }
            other => other.map_err(AttemptFailure::into_public),
        }
    }

    /// Runs one complete prepare, protect, revalidate, and materialize attempt.
    ///
    /// The object decision is taken on the prepared identities, before the
    /// reader guard: a restart cannot change who holds a table, and an
    /// out-of-scope caller must not reach protection or materialization at all.
    ///
    /// # Errors
    /// Returns [`AttemptFailure::CatalogPromoted`] when revalidation proved the
    /// authoritative catalog moved under this attempt, which the caller may
    /// restart once, and [`AttemptFailure::Fatal`] for every failure a restart
    /// cannot change, including the query-forbidden object refusal.
    async fn attempt_protected_cut(
        tables: &[TableRef],
        context: &AuthorizedQueryContext,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: &Arc<OracleReaderAuthority>,
    ) -> Result<ProtectedPlannedSqlCut, AttemptFailure> {
        let mut prepared = Vec::with_capacity(tables.len());
        for table in tables {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            prepared.push(
                tokio::time::timeout(
                    remaining,
                    catalog.prepare_reader_identity(table, context.data_tenant_id),
                )
                .await
                .map_err(|_| AttemptFailure::Fatal(BifrostError::QueryTimeout))?
                .map_err(|error| AttemptFailure::Fatal(error.into_public()))?,
            );
        }
        // The complete prepared set is the first trustworthy object list a
        // decision can be taken on: every requested table now has a tenant-bound
        // canonical binding and its stable registered UID. Authorizing here — and
        // not one step later — means an out-of-scope caller never takes a reader
        // guard, never drives revalidation, and never causes manifest or hot-cut
        // source IO it could observe.
        super::authorize_resolved_tables(context, &prepared).map_err(AttemptFailure::Fatal)?;
        let started = std::time::Instant::now();
        let guarded = authority.acquire_guard(&prepared).await;
        super::QueryPhase::ReaderGuard.record(started);
        let (guard, permit) = guarded.map_err(AttemptFailure::Fatal)?;
        // Every prepared table is revalidated before any of them materializes,
        // so a promotion is found while the whole attempt is still discardable.
        for identity in &prepared {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            let started = std::time::Instant::now();
            let revalidated =
                tokio::time::timeout(remaining, catalog.revalidate_reader_identity(identity)).await;
            super::QueryPhase::Revalidation.record(started);
            revalidated
                .map_err(|_| AttemptFailure::Fatal(BifrostError::QueryTimeout))?
                .map_err(|error| match error {
                    BifrostCatalogError::MetadataMismatch(_) => {
                        AttemptFailure::CatalogPromoted(error.into_public())
                    }
                    other => AttemptFailure::Fatal(other.into_public()),
                })?;
        }
        let mut cuts = Vec::with_capacity(prepared.len());
        for identity in prepared {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            cuts.push(
                tokio::time::timeout(remaining, catalog.materialize_reader_cut(identity, &permit))
                    .await
                    .map_err(|_| AttemptFailure::Fatal(BifrostError::QueryTimeout))?
                    .map_err(|error| AttemptFailure::Fatal(error.into_public()))?,
            );
        }
        Ok(ProtectedPlannedSqlCut {
            guard,
            permit,
            cuts,
        })
    }

    /// Drives one complete protect-and-materialize attempt from a test.
    ///
    /// Preparation, protection, revalidation, and materialization are one
    /// operation by construction, so a test that needs to observe the restart
    /// has no other entry point into the exact production sequence.
    ///
    /// Returns how many cuts the successful attempt materialized, which is the
    /// observable the caller needs; the protected cut itself stays private.
    ///
    /// # Errors
    /// Returns whatever [`Self::protect_and_materialize`] returns.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn protect_and_materialize_for_test(
        tables: &[TableRef],
        context: &AuthorizedQueryContext,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: &Arc<OracleReaderAuthority>,
    ) -> Result<usize, BifrostError> {
        Self::protect_and_materialize(tables, context, deadline, catalog, Some(authority))
            .await
            .map(|protected| protected.cuts.len())
    }

    /// Pins the immutable source cut for one SQL attempt.
    ///
    /// The planner owns parsing-adjacent metadata work and never executes rows;
    /// provider installation remains an Oracle composition concern after audit.
    /// The whole pin — identity lookup, reader guard, revalidation, and
    /// materialization — runs under the leader deadline, so a query waiting for
    /// a Postgres connection or a catalog read ends as a timeout rather than
    /// holding the request past its deadline. Cancellation or expiry drops the
    /// pending pool acquisition and any local partial cuts; no admission or
    /// audit side effect has occurred at this stage. Nothing here derives a
    /// class — the class comes from the physical root built on top of this cut.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTimeout`] when the deadline passes first,
    /// and authorization, catalog, or byte-accounting failures otherwise.
    pub(super) async fn pin_cut(
        &self,
        context: &AuthorizedQueryContext,
        tables: &[TableRef],
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: Option<&Arc<OracleReaderAuthority>>,
    ) -> Result<PlannedSqlCut, BifrostError> {
        // DEBUG, not INFO: one event per catalog pin per query is per-request
        // decision detail, not a lifecycle transition. It is the only way to
        // attribute pre-fragment query latency, which is otherwise invisible
        // between admission and the first fragment dispatch.
        let pin_started = std::time::Instant::now();
        let ProtectedPlannedSqlCut {
            guard,
            permit,
            cuts,
        } = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            Self::protect_and_materialize(tables, context, deadline, catalog, authority),
        )
        .await
        .map_err(|_| BifrostError::QueryTimeout)??;
        let hot_files = cuts.iter().map(|cut| cut.hot_files.len()).sum::<usize>();
        let iceberg_files = cuts
            .iter()
            .map(|cut| cut.iceberg_files.len())
            .sum::<usize>();
        tracing::debug!(
            tables = tables.len(),
            hot_files,
            iceberg_files,
            pin_ms = pin_started.elapsed().as_millis(),
            "Oracle pinned one sealed cut"
        );
        let local_bytes = cuts.iter().try_fold(0_u64, |total, cut| {
            cut.hot_files.iter().try_fold(total, |total, file| {
                let bytes = u64::try_from(file.file_size)
                    .map_err(|_| BifrostError::QueryAdmissionRejected)?;
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
        let local_ratio = if total_bytes == 0 {
            0.0
        } else {
            local_bytes
                .to_f64()
                .zip(total_bytes.to_f64())
                .map(|(local, total)| local / total)
                .ok_or(BifrostError::QueryAdmissionRejected)?
        };
        Ok(PlannedSqlCut {
            cuts,
            local_ratio,
            reader_pin: guard,
            reader_io_permit: permit,
        })
    }
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
