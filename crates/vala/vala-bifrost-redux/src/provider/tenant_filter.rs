//! Tenant filter attachment — copied from vala-bifrost.

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use datafusion::error::{DataFusionError, Result as DfResult};
use datafusion::logical_expr::Operator;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::{BinaryExpr, Column, Literal};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::filter::FilterExec;
use datafusion::scalar::ScalarValue;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

/// Build a tenant predicate: `data_tenant_id = <tenant>`.
///
/// Returns `None` if the schema does not contain a `data_tenant_id` column.
fn tenant_predicate(schema: &SchemaRef, tenant: DataTenantId) -> Option<Arc<dyn PhysicalExpr>> {
    let col =
        Arc::new(Column::new_with_schema(DATA_TENANT_ID, schema).ok()?) as Arc<dyn PhysicalExpr>;
    let lit = Arc::new(Literal::new(ScalarValue::Utf8(Some(tenant.to_string()))))
        as Arc<dyn PhysicalExpr>;
    Some(Arc::new(BinaryExpr::new(col, Operator::Eq, lit)))
}

/// Wrap `plan` (a scan whose output schema includes `data_tenant_id`) in the
/// authoritative tenant `FilterExec`. This is the PRIMARY, non-removable
/// boundary (N-M1/N-M2): it must isolate even with no analyzer registered.
/// If a physical scan ever reaches here without a bindable tenant column,
/// fail closed (`vala.tenant.predicate_missing`) rather than return unfiltered
/// rows — finding 2F, cheap insurance, never a substitute for the filter.
///
/// # Errors
/// Returns an error if the plan's schema is missing `data_tenant_id` or if
/// the `FilterExec` construction fails.
pub fn attach_tenant_filter(
    plan: Arc<dyn ExecutionPlan>,
    tenant: DataTenantId,
) -> DfResult<Arc<dyn ExecutionPlan>> {
    let schema = plan.schema();
    match tenant_predicate(&schema, tenant) {
        Some(predicate) => Ok(Arc::new(FilterExec::try_new(predicate, plan)?)),
        None => Err(DataFusionError::Internal(
            "vala.tenant.predicate_missing: tenant-scoped scan reached execution \
 without a bindable data_tenant_id filter"
                .to_string(),
        )),
    }
}
