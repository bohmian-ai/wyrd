use std::sync::Arc;

use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::common::{Column, Result as DfResult};
use datafusion::config::ConfigOptions;
use datafusion::datasource::default_table_source::DefaultTableSource;
use datafusion::logical_expr::{Expr, Filter, LogicalPlan};
use datafusion::optimizer::analyzer::AnalyzerRule;
use datafusion::scalar::ScalarValue;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::DATA_TENANT_ID;

use crate::provider::WyrdTableProvider;

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

    /// True only for a `TableScan` backed by a [`WyrdTableProvider`]. The scan's
    /// `source` is an `Arc<dyn TableSource>` wrapping a `DefaultTableSource`, which
    /// in turn wraps the real provider — hence the two-hop downcast (N-M8). A
    /// single-hop `source.as_any().downcast_ref::<WyrdTableProvider>()` always
    /// returns `None` and would silently disable tenant injection everywhere.
    /// Scoping keeps foreign providers (`MemTable` / CTAS targets) pass-through; the
    /// provider `FilterExec` remains the authoritative boundary.
    fn is_wyrd_scan(plan: &LogicalPlan) -> bool {
        let LogicalPlan::TableScan(scan) = plan else {
            return false;
        };
        scan.source
            .as_any()
            .downcast_ref::<DefaultTableSource>()
            .and_then(|s| {
                s.table_provider
                    .as_any()
                    .downcast_ref::<WyrdTableProvider>()
            })
            .is_some()
    }
}

impl AnalyzerRule for TenantPredicateRule {
    fn name(&self) -> &str {
        "wyrd_tenant_predicate"
    }

    fn analyze(&self, plan: LogicalPlan, _config: &ConfigOptions) -> DfResult<LogicalPlan> {
        let predicate = self.tenant_predicate();

        let result = plan.transform(|node| {
            let inject = Self::is_wyrd_scan(&node) && Self::has_tenant_column(&node);
            if inject {
                let filter = Filter::try_new(predicate.clone(), Arc::new(node))?;
                Ok(Transformed::yes(LogicalPlan::Filter(filter)))
            } else {
                Ok(Transformed::no(node))
            }
        })?;

        Ok(result.data)
    }
}
