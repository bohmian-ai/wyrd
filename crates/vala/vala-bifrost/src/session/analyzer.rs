use std::sync::Arc;

use datafusion::common::{Column, Result as DfResult};
use datafusion::config::ConfigOptions;
use datafusion::logical_expr::{Expr, Filter, LogicalPlan};
use datafusion::optimizer::analyzer::AnalyzerRule;
use datafusion::scalar::ScalarValue;
use datafusion::common::tree_node::{Transformed, TreeNode};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::DATA_TENANT_ID;

#[derive(Debug)]
pub struct TenantPredicateRule {
    tenant: DataTenantId,
}

impl TenantPredicateRule {
    pub fn new(tenant: DataTenantId) -> Self {
        Self { tenant }
    }

    fn tenant_predicate(&self) -> Expr {
        let col = Expr::Column(Column::new_unqualified(DATA_TENANT_ID));
        let val = Expr::Literal(ScalarValue::Utf8(Some(self.tenant.to_string())), None);
        col.eq(val)
    }

    fn has_tenant_column(plan: &LogicalPlan) -> bool {
        plan.schema()
            .field_with_unqualified_name(DATA_TENANT_ID)
            .is_ok()
    }
}

impl AnalyzerRule for TenantPredicateRule {
    fn name(&self) -> &str {
        "wyrd_tenant_predicate"
    }

    fn analyze(&self, plan: LogicalPlan, _config: &ConfigOptions) -> DfResult<LogicalPlan> {
        let predicate = self.tenant_predicate();

        let result = plan.transform(|node| {
            if matches!(node, LogicalPlan::TableScan(_)) && Self::has_tenant_column(&node) {
                let filter = Filter::try_new(predicate.clone(), Arc::new(node))?;
                Ok(Transformed::yes(LogicalPlan::Filter(filter)))
            } else {
                Ok(Transformed::no(node))
            }
        })?;

        Ok(result.data)
    }
}
