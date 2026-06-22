use std::sync::Arc;

use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DfResult;
use datafusion::logical_expr::{Expr, Operator, TableProviderFilterPushDown};
use datafusion::physical_expr::expressions::{BinaryExpr, Column, Literal};
use datafusion::physical_plan::{ExecutionPlan, filter::FilterExec};
use datafusion::scalar::ScalarValue;
use iceberg::table::Table;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::DATA_TENANT_ID;

use arrow::datatypes::SchemaRef;

use crate::types::TableScope;

pub mod pushdown;

#[derive(Debug)]
pub struct WyrdTableProvider {
    inner: pushdown::PassthroughProvider,
    tenant_filter: Option<DataTenantId>,
}

impl WyrdTableProvider {
    pub async fn try_new(table: Table, scope: TableScope, tenant: DataTenantId) -> DfResult<Self> {
        let inner = pushdown::PassthroughProvider::try_new(table).await?;
        let tenant_filter = match scope {
            TableScope::SystemShared => Some(tenant),
            TableScope::TenantOwned => None,
        };
        Ok(Self {
            inner,
            tenant_filter,
        })
    }

    fn tenant_predicate(
        &self,
        schema: &SchemaRef,
    ) -> Option<Arc<dyn datafusion::physical_expr::PhysicalExpr>> {
        let tenant = self.tenant_filter?;
        let col = Arc::new(Column::new_with_schema(DATA_TENANT_ID, schema).ok()?)
            as Arc<dyn datafusion::physical_expr::PhysicalExpr>;
        let lit = Arc::new(Literal::new(ScalarValue::Utf8(Some(tenant.to_string()))))
            as Arc<dyn datafusion::physical_expr::PhysicalExpr>;
        Some(Arc::new(BinaryExpr::new(col, Operator::Eq, lit)))
    }
}

#[async_trait]
impl TableProvider for WyrdTableProvider {
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
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        let plan = self.inner.scan(state, projection, filters, limit).await?;
        let schema = plan.schema();

        if let Some(predicate) = self.tenant_predicate(&schema) {
            let filtered = FilterExec::try_new(predicate, plan)?;
            return Ok(Arc::new(filtered));
        }

        Ok(plan)
    }
}
