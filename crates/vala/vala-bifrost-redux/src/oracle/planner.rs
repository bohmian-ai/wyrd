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
    TableRef, map_datafusion_error,
};

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

impl OraclePlanner {
    /// Pins every table scan in a typed plan against one authenticated tenant.
    ///
    /// Tables are pinned sequentially under one absolute deadline. Cancellation
    /// drops the active catalog future; cuts already returned are immutable local
    /// values and are discarded with the incomplete result.
    ///
    /// # Errors
    /// Returns invalid SQL for non-canonical scans or timeout/catalog failures
    /// while materializing the immutable table cuts.
    pub(super) async fn prepare_typed_cuts(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        tenant: wyrd_spec::DataTenantId,
        deadline: Instant,
        catalog: &BifrostCatalog,
    ) -> Result<Vec<PinnedSealedTable>, BifrostError> {
        let tables = collect_plan_table_refs(plan)?;
        let mut cuts = Vec::with_capacity(tables.len());
        for table in tables {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryTimeout)?;
            cuts.push(
                tokio::time::timeout(remaining, catalog.pin_sealed_table(&table, tenant))
                    .await
                    .map_err(|_| BifrostError::QueryTimeout)?
                    .map_err(BifrostCatalogError::into_public)?,
            );
        }
        Ok(cuts)
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
            let provider = OracleTableProvider::try_new(OracleTableInputs {
                table: cut.iceberg_table,
                storage: Arc::clone(catalog.storage()),
                hot_files,
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
    /// # Errors
    /// Returns a typed execution failure when any scan lacks a cut provider or
    /// when `DataFusion` rejects the transformed logical plan.
    pub(super) fn replace_typed_sources(
        plan: datafusion::logical_expr::LogicalPlan,
        providers: &std::collections::HashMap<String, Arc<dyn TableProvider>>,
    ) -> Result<datafusion::logical_expr::LogicalPlan, BifrostError> {
        plan.transform(|node| {
            let datafusion::logical_expr::LogicalPlan::TableScan(scan) = &node else {
                return Ok(Transformed::no(node));
            };
            let table_name = scan.table_name.to_string();
            let Some(provider) = providers.get(&table_name) else {
                return Err(datafusion::error::DataFusionError::Plan(
                    "typed plan scan has no authenticated Oracle provider".to_owned(),
                ));
            };
            let mut replacement = scan.clone();
            replacement.source = Arc::new(DefaultTableSource::new(Arc::clone(provider)));
            Ok(Transformed::yes(
                datafusion::logical_expr::LogicalPlan::TableScan(replacement),
            ))
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
    /// Returns timeout, catalog, or byte-accounting failures.
    pub(super) async fn pin_cut(
        &self,
        context: &AuthorizedQueryContext,
        tables: &[TableRef],
        deadline: Instant,
        catalog: &BifrostCatalog,
    ) -> Result<PlannedSqlCut, BifrostError> {
        let planning = self.try_planning()?;
        // DEBUG, not INFO: one event per catalog pin per query is per-request
        // decision detail, not a lifecycle transition. It is the only way to
        // attribute pre-fragment query latency, which is otherwise invisible
        // between admission and the first fragment dispatch.
        let pin_started = std::time::Instant::now();
        let mut cuts = Vec::with_capacity(tables.len());
        for table in tables {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryTimeout)?;
            let cut = tokio::time::timeout(
                remaining,
                catalog.pin_sealed_table(table, context.data_tenant_id),
            )
            .await
            .map_err(|_| BifrostError::QueryTimeout)?
            .map_err(BifrostCatalogError::into_public)?;
            cuts.push(cut);
        }
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
        Ok(PlannedSqlCut { cuts, local_ratio })
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
}
