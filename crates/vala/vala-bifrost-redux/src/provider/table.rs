//! `DataFusion` provider for tenant-qualified Redux Iceberg tables.

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DfResult};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::limit::GlobalLimitExec;
use datafusion::physical_plan::projection::ProjectionExec;
use iceberg::table::Table;
use iceberg_datafusion::IcebergStaticTableProvider;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use super::tenant_filter::attach_tenant_filter;

/// Tenant-qualified Iceberg provider used by Oracle and server query paths.
///
/// Redux physical bindings isolate tables by tenant namespace. The provider also
/// applies the tenant predicate at execution time so a malformed or mixed file
/// fails closed instead of relying on catalog isolation alone.
#[derive(Debug)]
pub struct ReduxTableProvider {
    /// Published Iceberg scan provider for the tenant-qualified table.
    inner: IcebergStaticTableProvider,
    /// Tenant whose predicate is re-applied above every produced scan.
    tenant: DataTenantId,
}

#[derive(Debug)]
struct TenantProjection {
    scan: Option<Vec<usize>>,
    output: Option<Vec<usize>>,
}

impl ReduxTableProvider {
    /// Build a provider over one already tenant-qualified Iceberg table.
    ///
    /// # Errors
    /// Returns the `DataFusion` error produced while constructing the Iceberg
    /// scan provider.
    pub async fn try_new(table: Table, tenant: DataTenantId) -> DfResult<Self> {
        let inner = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        Ok(Self { inner, tenant })
    }

    /// Remove the temporary tenant column after the physical filter.
    fn project_filtered_plan(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        projection: Option<&Vec<usize>>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        let filtered = attach_tenant_filter(plan, self.tenant)?;
        let Some(columns) = projection else {
            return Ok(filtered);
        };
        let schema = filtered.schema();
        let expressions: Vec<(Arc<dyn PhysicalExpr>, String)> = columns
            .iter()
            .map(|input| {
                let name = schema.field(*input).name().clone();
                let expression = Arc::new(Column::new(&name, *input)) as Arc<dyn PhysicalExpr>;
                (expression, name)
            })
            .collect();
        Ok(Arc::new(ProjectionExec::try_new(expressions, filtered)?))
    }

    fn tenant_projection(&self, projection: Option<&Vec<usize>>) -> DfResult<TenantProjection> {
        let Some(columns) = projection else {
            return Ok(TenantProjection {
                scan: None,
                output: None,
            });
        };
        let tenant_ordinal = self
            .schema()
            .index_of(DATA_TENANT_ID)
            .map_err(|_| DataFusionError::Internal(format!("table missing {DATA_TENANT_ID}")))?;
        if columns.contains(&tenant_ordinal) {
            return Ok(TenantProjection {
                scan: Some(columns.clone()),
                output: None,
            });
        }
        let mut expanded = columns.clone();
        expanded.push(tenant_ordinal);
        Ok(TenantProjection {
            scan: Some(expanded),
            output: Some((0..columns.len()).collect()),
        })
    }

    /// Scans the published table with the tenant predicate re-applied above it.
    ///
    /// The tenant column is added to the pushed scan projection when the caller
    /// did not request it, filtered physically, and then projected back out, so
    /// a mixed or malformed file fails closed rather than relying on catalog
    /// isolation. An explicit `limit` is applied above the filter because the
    /// filter can remove rows the pushed limit would have already counted.
    ///
    /// # Errors
    /// Returns the `DataFusion` error produced by the inner Iceberg scan,
    /// projection resolution, or physical filter construction.
    async fn scan_with_tenant_filter(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        let tenant_projection = self.tenant_projection(projection)?;

        let published = self
            .inner
            .scan(state, tenant_projection.scan.as_ref(), filters, None)
            .await?;
        let projected = self.project_filtered_plan(published, tenant_projection.output.as_ref())?;

        match limit {
            Some(limit) => Ok(Arc::new(GlobalLimitExec::new(projected, 0, Some(limit)))),
            None => Ok(projected),
        }
    }
}

#[async_trait]
impl TableProvider for ReduxTableProvider {
    fn schema(&self) -> SchemaRef {
        self.inner.schema()
    }

    fn table_type(&self) -> TableType {
        self.inner.table_type()
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        self.inner.supports_filters_pushdown(filters)
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        self.scan_with_tenant_filter(state, projection, filters, limit)
            .await
    }
}
