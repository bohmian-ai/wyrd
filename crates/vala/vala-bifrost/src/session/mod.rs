use std::sync::Arc;

use datafusion::execution::SessionStateBuilder;
use datafusion::prelude::SessionContext;
use wyrd_spec::ids::DataTenantId;

pub mod analyzer;

pub use analyzer::TenantPredicateRule;

pub fn wyrd_session_context(tenant: DataTenantId) -> SessionContext {
    let state = SessionStateBuilder::new()
        .with_analyzer_rule(Arc::new(TenantPredicateRule::new(tenant)))
        .build();
    SessionContext::new_with_state(state)
}
