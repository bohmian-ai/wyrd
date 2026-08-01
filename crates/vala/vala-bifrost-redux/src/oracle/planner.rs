//! SQL planning owner for Oracle's immutable visibility cut.

use std::sync::Arc;
use std::time::Instant;

use arrow::datatypes::Schema;
use datafusion::catalog::{CatalogProvider, MemoryCatalogProvider, MemorySchemaProvider};
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::datasource::default_table_source::DefaultTableSource;
use datafusion::datasource::{MemTable, TableProvider};
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::ExecutionPlan;
use wyrd_spec::vala::api::ClusterCapabilities;

use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, DrainedTails,
    HotFileSource, OracleAudit, OracleMemoryResources, OraclePlanner, OracleTableInputs,
    OracleTableProvider, OracleTelemetry, PinnedSealedTable, PlannedSqlCut, QueryClass, TableRef,
    map_datafusion_error, optimized_plan_is_complex, query_class_label,
};

impl OraclePlanner {
    /// Pins every table scan in a typed plan against one authenticated tenant.
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
    /// # Errors
    /// Returns metadata, storage, or provider-construction failures.
    pub(super) async fn build_typed_providers(
        &self,
        inputs: TypedProviderInputs<'_>,
    ) -> Result<std::collections::HashMap<String, Arc<dyn TableProvider>>, BifrostError> {
        let TypedProviderInputs {
            context,
            class,
            cuts,
            drained,
            catalog,
            audit,
            memory,
            telemetry,
        } = inputs;
        let mut providers = std::collections::HashMap::with_capacity(cuts.len());
        for cut in cuts {
            let table_name = cut.binding.table_ref.fqn();
            let hot_files = local_hot_sources(catalog, &cut)?;
            let provider = OracleTableProvider::try_new(OracleTableInputs {
                table: cut.iceberg_table,
                distributed_iceberg_batches: None,
                hot_files,
                distributed_hot_batches: Vec::new(),
                live_batches: drained.batches.remove(&table_name).unwrap_or_default(),
                context: context.clone(),
                table_name: table_name.clone(),
                audit: Arc::clone(&audit),
                memory: memory.clone(),
                telemetry: Arc::clone(&telemetry),
                query_class: class,
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

    /// Creates one physical plan and retains its execution session context.
    ///
    /// # Errors
    /// Returns query execution failure when `DataFusion` cannot lower the plan.
    pub(super) async fn create_physical_plan(
        plan: &datafusion::logical_expr::LogicalPlan,
    ) -> Result<(SessionContext, Arc<dyn ExecutionPlan>), BifrostError> {
        let session = SessionContext::new();
        let physical = session
            .state()
            .create_physical_plan(plan)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        Ok((session, physical))
    }

    /// Pins metadata and derives the immutable class for one SQL retry attempt.
    ///
    /// The planner owns parsing-adjacent metadata work and never executes rows;
    /// provider installation remains an Oracle composition concern after audit.
    ///
    /// # Errors
    /// Returns timeout, catalog, planning, or byte-accounting failures.
    pub(super) async fn pin_and_classify(
        &self,
        context: &AuthorizedQueryContext,
        sql: &str,
        tables: &[TableRef],
        deadline: Instant,
        catalog: &BifrostCatalog,
        cluster: &super::ClusterRegistry,
    ) -> Result<PlannedSqlCut, BifrostError> {
        let planning = self.try_planning()?;
        let mut cuts = Vec::with_capacity(tables.len());
        let mut estimated_bytes = 0_u64;
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
            estimated_bytes = estimated_bytes
                .checked_add(cut.estimated_bytes)
                .ok_or(BifrostError::QueryAdmissionRejected)?;
            cuts.push(cut);
        }
        let optimized_plan = self.prepare_optimized_sql_plan(sql, &cuts).await?;
        let live_cpu = cluster
            .snapshot()
            .live_oracles()
            .iter()
            .filter_map(|role| match &role.capabilities {
                ClusterCapabilities::OracleV1(capabilities) => Some(capabilities.cpu_cores),
                ClusterCapabilities::ScribeV1(_) => None,
            })
            .sum::<f64>();
        let classification = Self::classification(
            estimated_bytes,
            live_cpu,
            optimized_plan_is_complex(&optimized_plan),
        );
        tracing::Span::current()
            .record("query_class", query_class_label(classification.query_class));
        OracleTelemetry::record_classification(classification);
        drop(planning);
        Ok(PlannedSqlCut {
            cuts,
            query_class: classification.query_class,
        })
    }

    /// Lowers and optimizes SQL against schema-only providers from one cut.
    ///
    /// # Errors
    /// Returns a stable planning failure when schema projection, registration,
    /// SQL lowering, or logical optimization fails.
    #[tracing::instrument(
        name = "bifrost.oracle.plan",
        skip_all,
        fields(table_count = cuts.len())
    )]
    async fn prepare_optimized_sql_plan(
        &self,
        sql: &str,
        cuts: &[PinnedSealedTable],
    ) -> Result<datafusion::logical_expr::LogicalPlan, BifrostError> {
        let session = SessionContext::new();
        for cut in cuts {
            let physical = iceberg::arrow::schema_to_arrow_schema(
                cut.iceberg_table.metadata().current_schema(),
            )
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
            let fields = physical
                .fields()
                .iter()
                .filter(|field| field.name() != "data_tenant_id")
                .cloned()
                .collect::<Vec<_>>();
            let public = Arc::new(Schema::new(fields));
            let provider = MemTable::try_new(public, vec![Vec::new()])
                .map_err(|error| map_datafusion_error(&error))?;
            register_session_table(&session, &cut.binding, Arc::new(provider))?;
        }
        session
            .sql(sql)
            .await
            .map_err(|error| map_datafusion_error(&error))?
            .into_optimized_plan()
            .map_err(|error| map_datafusion_error(&error))
    }
}

/// Dependencies required to build providers for one typed immutable cut.
pub(super) struct TypedProviderInputs<'a> {
    /// Authenticated tenant context copied into each provider.
    pub(super) context: &'a AuthorizedQueryContext,
    /// Admission class charged by each provider.
    pub(super) class: QueryClass,
    /// Pinned table metadata selected for this query.
    pub(super) cuts: Vec<PinnedSealedTable>,
    /// Drained live batches and reservations for the same cuts.
    pub(super) drained: &'a mut DrainedTails,
    /// Catalog used to resolve hot object locations.
    pub(super) catalog: &'a BifrostCatalog,
    /// Durable audit collaborator retained by each source operator.
    pub(super) audit: Arc<dyn OracleAudit>,
    /// Parent memory governor and reconciliation ceiling.
    pub(super) memory: OracleMemoryResources,
    /// Production source/reconciliation telemetry owner.
    pub(super) telemetry: Arc<OracleTelemetry>,
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
            Ok(HotFileSource {
                location: catalog
                    .object_location(&cut.binding, &file.file_path)
                    .map_err(BifrostCatalogError::into_public)?,
                size_bytes,
            })
        })
        .collect()
}

/// Registers one schema-only table beneath the Oracle catalog hierarchy.
///
/// The helper mirrors the logical namespace used by SQL lowering while keeping
/// the registered provider empty; physical source access is installed only
/// after the immutable cut and audit decision are complete.
///
/// # Errors
/// Returns query execution failure when the binding namespace is invalid or
/// `DataFusion` rejects catalog/schema/table registration.
fn register_session_table(
    session: &SessionContext,
    binding: &crate::catalog::TenantTableBinding,
    provider: Arc<dyn TableProvider>,
) -> Result<(), BifrostError> {
    let schema_name = binding
        .logical_namespace
        .strip_prefix("vala.")
        .filter(|name| !name.is_empty())
        .ok_or(BifrostError::QueryExecutionFailed)?;
    let catalog = session.catalog("vala").unwrap_or_else(|| {
        let catalog: Arc<dyn CatalogProvider> = Arc::new(MemoryCatalogProvider::new());
        session.register_catalog("vala", Arc::clone(&catalog));
        catalog
    });
    let schema = if let Some(schema) = catalog.schema(schema_name) {
        schema
    } else {
        let schema: Arc<dyn datafusion::catalog::SchemaProvider> =
            Arc::new(MemorySchemaProvider::new());
        catalog
            .register_schema(schema_name, Arc::clone(&schema))
            .map_err(|error| map_datafusion_error(&error))?;
        schema
    };
    schema
        .register_table(binding.table_name.clone(), provider)
        .map_err(|error| map_datafusion_error(&error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::MemTable;
    use datafusion::execution::context::SessionContext;

    use super::OraclePlanner;
    use crate::oracle::BifrostError;

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
