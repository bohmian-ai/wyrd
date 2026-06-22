use std::sync::Arc;

use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DfResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use iceberg::table::Table;
use iceberg_datafusion::IcebergStaticTableProvider;

use arrow::datatypes::SchemaRef;

#[derive(Debug)]
pub struct PassthroughProvider {
    inner: IcebergStaticTableProvider,
}

impl PassthroughProvider {
    pub async fn try_new(table: Table) -> DfResult<Self> {
        use datafusion::error::DataFusionError;
        let inner = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|e| DataFusionError::External(Box::new(e)))?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl TableProvider for PassthroughProvider {
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
        self.inner.supports_filters_pushdown(filters)
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
        self.inner.scan(state, projection, filters, limit).await
    }
}
