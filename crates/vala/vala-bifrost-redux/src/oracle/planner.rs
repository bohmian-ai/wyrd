//! SQL planning owner for Oracle's immutable visibility cut.
//!
//! Planning validates read-only requests, pins tenant-qualified metadata under
//! one deadline, and builds session-local providers without performing
//! admission, audit, or row execution side effects. The query class is derived
//! later from the physical root alone, so nothing here classifies.

use std::sync::Arc;
use std::time::Instant;

use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::datasource::TableProvider;
use datafusion::datasource::default_table_source::DefaultTableSource;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::ExecutionPlan;
use num_traits::ToPrimitive;

use super::*;
use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, HotFileSource,
    OracleAudit, OracleTableInputs, OracleTableProvider, PinnedSealedTable, PlannedSqlCut,
    ProtectedPlannedSqlCut, TableRef, map_datafusion_error,
};
use crate::oracle::reader_pins::OracleReaderAuthority;

/// Query floor and logical-plan preparation owner.
#[derive(Debug, Clone)]
pub struct OraclePlanner {
    /// Synchronous floor, deadline, and capacity configuration.
    pub(super) config: OracleConfig,
    /// Bounded permits covering sealed metadata planning.
    pub(super) planning: Arc<Semaphore>,
}

impl OraclePlanner {
    /// Creates a planner with bounded synchronous validation settings.
    #[must_use]
    pub fn new(config: OracleConfig) -> Self {
        Self {
            planning: Arc::new(Semaphore::new(config.planning_permits)),
            config,
        }
    }

    /// Validates one non-empty, single-statement `SELECT` request.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryInvalidSql`] for floor violations.
    pub fn validate_query(&self, request: &BifrostQueryRequest) -> Result<(), BifrostError> {
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
        parse_select_tables(&request.sql)?;
        Ok(())
    }

    /// Tries to reserve one bounded planning slot without queuing unbounded work.
    ///
    /// # Errors
    ///
    /// Returns admission rejection while the planning bound is saturated.
    fn try_planning(&self) -> Result<OwnedSemaphorePermit, BifrostError> {
        Arc::clone(&self.planning)
            .try_acquire_owned()
            .map_err(|_| BifrostError::QueryAdmissionRejected)
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
    /// Protects and materializes every table scan in a typed plan.
    ///
    /// Tables are prepared sequentially under one absolute deadline, the whole
    /// prepared set is authorized against the caller's object grants, the whole
    /// set is protected by one durable guard, and only then is any snapshot
    /// materialized. Cancellation drops the active catalog future; a partially
    /// materialized result is discarded together with its guard, which narrows
    /// the protection it took.
    ///
    /// # Errors
    /// Returns invalid SQL for non-canonical scans, the query-forbidden refusal
    /// when the caller does not hold every resolved table, reader-authority
    /// refusal, or timeout/catalog failures while materializing the cuts.
    pub(super) async fn prepare_typed_cuts(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        context: &AuthorizedQueryContext,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: &Arc<OracleReaderAuthority>,
    ) -> Result<ProtectedPlannedSqlCut, BifrostError> {
        let tables = collect_plan_table_refs(plan)?;
        Self::protect_and_materialize(&tables, context, deadline, catalog, Some(authority)).await
    }

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
        let (guard, permit) = authority
            .acquire_guard(&prepared)
            .await
            .map_err(AttemptFailure::Fatal)?;
        // Every prepared table is revalidated before any of them materializes,
        // so a promotion is found while the whole attempt is still discardable.
        for identity in &prepared {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            tokio::time::timeout(remaining, catalog.revalidate_reader_identity(identity))
                .await
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

    /// Builds executable providers from authenticated cuts and drained tails.
    ///
    /// Provider construction consumes each cut and transfers its matching drained
    /// batches into the provider map. Cancellation can leave only local partially
    /// constructed providers, which are dropped because no map is returned.
    ///
    /// # Errors
    /// Returns metadata, storage, or provider-construction failures.
    pub(super) async fn build_typed_providers(
        &self,
        inputs: TypedProviderInputs<'_>,
    ) -> Result<std::collections::HashMap<String, Arc<dyn TableProvider>>, BifrostError> {
        let TypedProviderInputs {
            context,
            cuts,
            catalog,
            audit,
        } = inputs;
        let mut providers = std::collections::HashMap::with_capacity(cuts.len());
        for cut in cuts {
            let table_name = cut.binding.table_ref.fqn();
            let hot_files = local_hot_sources(catalog, &cut)?;
            let iceberg_event_times = cut
                .iceberg_files
                .iter()
                .map(|file| file.event_time)
                .collect();
            let provider = OracleTableProvider::try_new(OracleTableInputs {
                table: cut.iceberg_table,
                storage: Arc::clone(catalog.storage()),
                hot_files,
                iceberg_event_times,
                context: context.clone(),
                table_name: table_name.clone(),
                audit: Arc::clone(&audit),
                remote: None,
            })
            .await
            .map_err(|error| map_datafusion_error(&error))?;
            providers.insert(table_name, Arc::new(provider) as Arc<dyn TableProvider>);
        }
        Ok(providers)
    }

    /// Replaces schema-only scans with providers from the authenticated cut.
    ///
    /// Each scan is rebuilt over its provider so its projected schema is the
    /// provider's physical schema, and every ancestor recomputes its schema
    /// bottom-up. A typed plan is authored against the catalog schema, which
    /// may spell a type differently from the physical tier (Iceberg projects
    /// UTC as `+00:00`); recomputing lets the analyzer's type coercion
    /// reconcile authored literals with the physical columns during physical
    /// planning instead of failing at execution.
    ///
    /// # Errors
    /// Returns a typed execution failure when any scan lacks a cut provider or
    /// when `DataFusion` rejects the transformed logical plan.
    pub(super) fn replace_typed_sources(
        plan: datafusion::logical_expr::LogicalPlan,
        providers: &std::collections::HashMap<String, Arc<dyn TableProvider>>,
    ) -> Result<datafusion::logical_expr::LogicalPlan, BifrostError> {
        use datafusion::logical_expr::{LogicalPlan, TableScanBuilder};

        plan.transform_up(|node| {
            let LogicalPlan::TableScan(scan) = node else {
                return node.recompute_schema().map(Transformed::yes);
            };
            let Some(provider) = providers.get(&scan.table_name.to_string()) else {
                return Err(datafusion::error::DataFusionError::Plan(
                    "typed plan scan has no authenticated Oracle provider".to_owned(),
                ));
            };
            TableScanBuilder::new(
                scan.table_name,
                Arc::new(DefaultTableSource::new(Arc::clone(provider))),
            )
            .with_projection(scan.projection)
            .with_filters(scan.filters)
            .with_fetch(scan.fetch)
            .with_statistics_requests(scan.statistics_requests)
            .build()
            .map(|scan| Transformed::yes(LogicalPlan::TableScan(scan)))
        })
        .map(|transformed| transformed.data)
        .map_err(|error| map_datafusion_error(&error))
    }

    /// Creates one physical plan in the admitted query's governed session.
    ///
    /// The caller supplies the same query-owned memory and disk runtime used by
    /// SQL execution. Cancellation drops the `DataFusion` planning future and
    /// returns no session or executable plan, so no partially prepared
    /// execution escapes.
    ///
    /// # Errors
    /// Returns query execution failure when `DataFusion` cannot lower the plan
    /// in the supplied governed session.
    pub(super) async fn create_physical_plan(
        plan: &datafusion::logical_expr::LogicalPlan,
        session: SessionContext,
    ) -> Result<(SessionContext, Arc<dyn ExecutionPlan>), BifrostError> {
        let physical = session
            .state()
            .create_physical_plan(plan)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        Ok((session, physical))
    }

    /// Pins the immutable source cut for one SQL attempt.
    ///
    /// The planner owns parsing-adjacent metadata work and never executes rows;
    /// provider installation remains an Oracle composition concern after audit.
    /// The planning permit spans every catalog pin. Cancellation drops that
    /// permit and any local partial cuts; no admission or audit side effect has
    /// occurred at this stage. Nothing here derives a class — the class comes
    /// from the physical root built on top of this cut.
    ///
    /// # Errors
    /// Returns timeout, authorization, catalog, or byte-accounting failures.
    pub(super) async fn pin_cut(
        &self,
        context: &AuthorizedQueryContext,
        tables: &[TableRef],
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: Option<&Arc<OracleReaderAuthority>>,
    ) -> Result<PlannedSqlCut, BifrostError> {
        let planning = self.try_planning()?;
        // DEBUG, not INFO: one event per catalog pin per query is per-request
        // decision detail, not a lifecycle transition. It is the only way to
        // attribute pre-fragment query latency, which is otherwise invisible
        // between admission and the first fragment dispatch.
        let pin_started = std::time::Instant::now();
        let ProtectedPlannedSqlCut {
            guard,
            permit,
            cuts,
        } = Self::protect_and_materialize(tables, context, deadline, catalog, authority).await?;
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
        drop(planning);
        Ok(PlannedSqlCut {
            cuts,
            local_ratio,
            reader_pin: guard,
            reader_io_permit: permit,
        })
    }
}

/// Dependencies required to build providers for one typed immutable cut.
pub(super) struct TypedProviderInputs<'a> {
    /// Authenticated tenant context copied into each provider.
    pub(super) context: &'a AuthorizedQueryContext,
    /// Pinned table metadata selected for this query.
    pub(super) cuts: Vec<PinnedSealedTable>,
    /// Catalog used to resolve hot object locations.
    pub(super) catalog: &'a BifrostCatalog,
    /// Durable audit collaborator retained by each source operator.
    pub(super) audit: Arc<dyn OracleAudit>,
}

/// Collects distinct canonical table identities from a typed plan.
///
/// # Errors
/// Returns invalid SQL when a scan is not a canonical Bifrost table reference
/// or when the plan contains no table scans.
fn collect_plan_table_refs(
    plan: &datafusion::logical_expr::LogicalPlan,
) -> Result<Vec<TableRef>, BifrostError> {
    /// Visits one logical subtree and records distinct canonical table refs.
    ///
    /// # Errors
    /// Returns invalid SQL when a table scan does not use a canonical Bifrost
    /// fully-qualified table reference.
    fn visit(
        plan: &datafusion::logical_expr::LogicalPlan,
        refs: &mut Vec<TableRef>,
    ) -> Result<(), BifrostError> {
        if let datafusion::logical_expr::LogicalPlan::TableScan(scan) = plan {
            let table = TableRef::parse_fqn(&scan.table_name.to_string()).ok_or(
                BifrostError::QueryInvalidSql {
                    detail: "typed query plan contains an invalid Bifrost table reference"
                        .to_owned(),
                },
            )?;
            if !refs.contains(&table) {
                refs.push(table);
            }
        }
        for input in plan.inputs() {
            visit(input, refs)?;
        }
        Ok(())
    }
    let mut refs = Vec::new();
    visit(plan, &mut refs)?;
    if refs.is_empty() {
        return Err(BifrostError::QueryInvalidSql {
            detail: "typed query plans must contain at least one table scan".to_owned(),
        });
    }
    Ok(refs)
}

/// Resolves leader-local hot file locations for one pinned cut.
///
/// # Errors
/// Returns metadata mismatch when a file exceeds process bounds, catalog
/// errors when an object location cannot be resolved, or an aggregate mapping
/// error from either source.
fn local_hot_sources(
    catalog: &BifrostCatalog,
    cut: &PinnedSealedTable,
) -> Result<Vec<HotFileSource>, BifrostError> {
    cut.hot_files
        .iter()
        .map(|file| {
            let size_bytes =
                usize::try_from(file.file_size).map_err(|_| BifrostError::MetadataMismatch {
                    detail: "hot file size exceeds process bounds".to_owned(),
                })?;
            let location = catalog
                .object_location(&cut.binding, &file.file_path)
                .map_err(BifrostCatalogError::into_public)?;
            Ok(HotFileSource {
                metadata_key: super::exec::hot_metadata_key(file, size_bytes)?,
                location,
                size_bytes,
                event_time:
                    crate::catalog::event_time::EventTimeStatistics::from_catalog_timestamps(
                        file.min_event_time,
                        file.max_event_time,
                    ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::MemTable;
    use datafusion::execution::context::SessionContext;
    use std::sync::Arc;

    use super::{OraclePlanner, register_session_table};
    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use crate::oracle::BifrostError;
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

    /// Rejects a typed plan whose scan was not replaced by an authenticated provider.
    #[tokio::test]
    async fn typed_source_replacement_fails_closed_for_unmatched_scan() {
        let session = SessionContext::new();
        let provider = MemTable::try_new(
            std::sync::Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, true)])),
            vec![Vec::new()],
        )
        .expect("schema-only provider");
        session
            .register_table("spans", std::sync::Arc::new(provider))
            .expect("register typed table");
        let plan = session
            .sql("SELECT name FROM spans")
            .await
            .expect("lower typed query")
            .into_optimized_plan()
            .expect("optimize typed query");
        assert!(matches!(
            OraclePlanner::replace_typed_sources(plan, &HashMap::new()),
            Err(BifrostError::QueryExecutionFailed)
        ));
    }

    /// A typed plan authored against a `UTC` catalog schema executes over a
    /// provider that projects the same instant as `+00:00`: replacement
    /// recomputes the scan and filter schemas so coercion reconciles the
    /// authored literal with the physical column.
    #[tokio::test]
    async fn typed_source_replacement_reconciles_the_physical_timezone_spelling() {
        use arrow::array::TimestampMicrosecondArray;
        use arrow::datatypes::TimeUnit;
        use arrow::record_batch::RecordBatch;
        use datafusion::logical_expr::{
            LogicalPlanBuilder, col, lit, logical_plan::builder::LogicalTableSource,
        };
        use datafusion::scalar::ScalarValue;

        let authored = Arc::new(Schema::new(vec![Field::new(
            "at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )]));
        let physical = Arc::new(Schema::new(vec![Field::new(
            "at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&physical),
            vec![Arc::new(
                TimestampMicrosecondArray::from(vec![1_i64, 5, 9]).with_timezone("+00:00"),
            )],
        )
        .expect("physical batch");
        let provider = MemTable::try_new(physical, vec![vec![batch]]).expect("physical provider");
        let plan = LogicalPlanBuilder::scan(
            "vala.drift.observations",
            Arc::new(LogicalTableSource::new(authored)),
            None,
        )
        .and_then(|builder| {
            builder.filter(col("at").gt_eq(lit(ScalarValue::TimestampMicrosecond(
                Some(5),
                Some("UTC".into()),
            ))))
        })
        .and_then(LogicalPlanBuilder::build)
        .expect("typed plan");
        let providers = HashMap::from([(
            "vala.drift.observations".to_owned(),
            Arc::new(provider) as Arc<dyn datafusion::datasource::TableProvider>,
        )]);

        let plan = OraclePlanner::replace_typed_sources(plan, &providers).expect("replaces");
        let batches = SessionContext::new()
            .execute_logical_plan(plan)
            .await
            .expect("plans")
            .collect()
            .await
            .expect("executes without a timezone comparison error");
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    }
}
