//! `DataFusion` provider for tenant-qualified Redux Iceberg tables.

use std::sync::Arc;

use arrow::compute::cast;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DfResult};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::limit::GlobalLimitExec;
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::union::UnionExec;
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
    inner: IcebergStaticTableProvider,
    tenant: DataTenantId,
    hot_batches: Vec<arrow::record_batch::RecordBatch>,
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
        Self::try_new_with_hot_batches(table, tenant, Vec::new()).await
    }

    /// Build a provider over an Iceberg table plus the current Scribe hot tail.
    ///
    /// Hot batches are shallow Arrow handles returned by the owning Scribe
    /// shard. The provider unions them with the published scan at execution
    /// time, so a durable ACK is immediately queryable without waiting for
    /// Parquet publication.
    pub async fn try_new_with_hot_batches(
        table: Table,
        tenant: DataTenantId,
        hot_batches: Vec<arrow::record_batch::RecordBatch>,
    ) -> DfResult<Self> {
        let inner = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let schema = inner.schema();
        let hot_batches = hot_batches
            .into_iter()
            .map(|batch| project_hot_batch(&batch, &schema))
            .collect::<DfResult<Vec<_>>>()?;
        Ok(Self {
            inner,
            tenant,
            hot_batches,
        })
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
        let mut inputs =
            vec![self.project_filtered_plan(published, tenant_projection.output.as_ref())?];

        if !self.hot_batches.is_empty() {
            let hot = MemorySourceConfig::try_new_exec(
                std::slice::from_ref(&self.hot_batches),
                self.schema(),
                tenant_projection.scan.clone(),
            )?;
            inputs.push(self.project_filtered_plan(hot, tenant_projection.output.as_ref())?);
        }

        let projected = UnionExec::try_new(inputs)?;

        match limit {
            Some(limit) => Ok(Arc::new(GlobalLimitExec::new(projected, 0, Some(limit)))),
            None => Ok(projected),
        }
    }
}

/// Align a Scribe snapshot to the published table schema by field name.
///
/// Scribe may carry server-owned columns that are not part of an older
/// published table definition. Name-based projection drops those columns and
/// preserves the exact Arrow order expected by the Iceberg scan; compatible
/// type changes are cast explicitly instead of relying on positional arrays.
fn project_hot_batch(batch: &RecordBatch, target: &SchemaRef) -> DfResult<RecordBatch> {
    let columns = target
        .fields()
        .iter()
        .map(|field| {
            let index = batch.schema().index_of(field.name()).map_err(|_| {
                DataFusionError::Plan(format!(
                    "Scribe hot batch is missing table column `{}`",
                    field.name()
                ))
            })?;
            let column = batch.column(index);
            if column.data_type() == field.data_type() {
                Ok(Arc::clone(column))
            } else {
                cast(column, field.data_type()).map_err(|error| {
                    DataFusionError::Plan(format!(
                        "Scribe hot column `{}` cannot be cast to {:?}: {error}",
                        field.name(),
                        field.data_type()
                    ))
                })
            }
        })
        .collect::<DfResult<Vec<_>>>()?;
    RecordBatch::try_new(Arc::clone(target), columns)
        .map_err(|error| DataFusionError::Plan(error.to_string()))
}

#[async_trait]
impl TableProvider for ReduxTableProvider {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

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
        if self.hot_batches.is_empty() {
            return self.inner.supports_filters_pushdown(filters);
        }
        Ok(filters
            .iter()
            .map(|_| TableProviderFilterPushDown::Inexact)
            .collect())
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
