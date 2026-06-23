use std::sync::Arc;

use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DfResult};
use datafusion::logical_expr::{Expr, Operator, TableProviderFilterPushDown};
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::{BinaryExpr, Column, Literal};
use datafusion::physical_plan::{
    ExecutionPlan, filter::FilterExec, limit::GlobalLimitExec, projection::ProjectionExec,
};
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

    fn tenant_predicate(&self, schema: &SchemaRef) -> Option<Arc<dyn PhysicalExpr>> {
        let tenant = self.tenant_filter?;
        let col = Arc::new(Column::new_with_schema(DATA_TENANT_ID, schema).ok()?)
            as Arc<dyn PhysicalExpr>;
        let lit = Arc::new(Literal::new(ScalarValue::Utf8(Some(tenant.to_string()))))
            as Arc<dyn PhysicalExpr>;
        Some(Arc::new(BinaryExpr::new(col, Operator::Eq, lit)))
    }

    /// Wrap `plan` (a scan whose output schema includes `data_tenant_id`) in the
    /// authoritative tenant `FilterExec`. This is the PRIMARY, non-removable
    /// boundary (N-M1/N-M2): it must isolate even with no analyzer registered.
    /// If a `SystemShared` scan ever reaches here without a bindable tenant column,
    /// fail closed (`vala.tenant.predicate_missing`) rather than return unfiltered
    /// rows — finding 2F, cheap insurance, never a substitute for the filter.
    fn attach_tenant_filter(
        &self,
        plan: Arc<dyn ExecutionPlan>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        let schema = plan.schema();
        match self.tenant_predicate(&schema) {
            Some(predicate) => Ok(Arc::new(FilterExec::try_new(predicate, plan)?)),
            None => Err(DataFusionError::Internal(
                "vala.tenant.predicate_missing: SystemShared scan reached execution \
                 without a bindable data_tenant_id filter"
                    .to_string(),
            )),
        }
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
        // TenantOwned tables carry no data_tenant_id column; isolation is by
        // catalog namespace. Pass the caller's scan through unchanged.
        if self.tenant_filter.is_none() {
            return self.inner.scan(state, projection, filters, limit).await;
        }

        // SystemShared: the provider is self-sufficient for isolation. Guarantee
        // data_tenant_id is in the scanned schema (expanding the caller's
        // projection if they pruned it), attach the FilterExec, then drop any
        // artificially-appended column so the caller sees its requested schema.
        let full_schema = self.schema();
        let tenant_ordinal = full_schema.index_of(DATA_TENANT_ID).map_err(|_| {
            DataFusionError::Internal(format!(
                "SystemShared table missing {DATA_TENANT_ID} column in schema"
            ))
        })?;

        // The caller's `limit` must NOT be pushed below the tenant filter: a
        // mixed-tenant inner scan limited first could truncate the bound tenant's
        // rows before isolation and under-count. Scan unlimited, filter, then cap.
        let filtered = match projection {
            // All columns present, including data_tenant_id.
            None => {
                let plan = self.inner.scan(state, None, filters, None).await?;
                self.attach_tenant_filter(plan)?
            }
            // Caller already projects the tenant column — no expansion/drop needed.
            Some(proj) if proj.contains(&tenant_ordinal) => {
                let plan = self.inner.scan(state, Some(proj), filters, None).await?;
                self.attach_tenant_filter(plan)?
            }
            // Tenant column pruned: append it (keeps drop indices stable), filter,
            // then re-project to the caller's original columns.
            Some(proj) => {
                let mut expanded = proj.clone();
                expanded.push(tenant_ordinal);
                let plan = self
                    .inner
                    .scan(state, Some(&expanded), filters, None)
                    .await?;
                let filtered = self.attach_tenant_filter(plan)?;

                let kept_schema = filtered.schema();
                let exprs = (0..proj.len())
                    .map(|i| {
                        let name = kept_schema.field(i).name();
                        let expr = Arc::new(Column::new(name, i)) as Arc<dyn PhysicalExpr>;
                        (expr, name.clone())
                    })
                    .collect::<Vec<_>>();
                Arc::new(ProjectionExec::try_new(exprs, filtered)?) as Arc<dyn ExecutionPlan>
            }
        };

        match limit {
            Some(n) => Ok(Arc::new(GlobalLimitExec::new(filtered, 0, Some(n)))),
            None => Ok(filtered),
        }
    }
}
