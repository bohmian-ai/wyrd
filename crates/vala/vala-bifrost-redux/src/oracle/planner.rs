//! SQL planning owner for Oracle's immutable visibility cut.

use std::sync::Arc;
use std::time::Instant;

use arrow::datatypes::Schema;
use datafusion::catalog::{CatalogProvider, MemoryCatalogProvider, MemorySchemaProvider};
use datafusion::datasource::{MemTable, TableProvider};
use datafusion::execution::context::SessionContext;
use wyrd_spec::vala::api::ClusterCapabilities;

use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, OraclePlanner,
    OracleTelemetry, PinnedSealedTable, PlannedSqlCut, TableRef, map_datafusion_error,
    optimized_plan_is_complex, query_class_label,
};

impl OraclePlanner {
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
