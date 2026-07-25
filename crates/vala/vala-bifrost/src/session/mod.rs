use std::sync::Arc;

use datafusion::error::DataFusionError;
use datafusion::execution::SessionStateBuilder;
use datafusion::execution::memory_pool::{GreedyMemoryPool, MemoryPool};
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use datafusion::prelude::SessionContext;
use wyrd_spec::ids::DataTenantId;

pub mod analyzer;

pub use analyzer::TenantPredicateRule;

pub fn wyrd_session_context(tenant: DataTenantId) -> SessionContext {
    let state = SessionStateBuilder::new()
        .with_default_features()
        .with_analyzer_rule(Arc::new(TenantPredicateRule::new(tenant)))
        .build();
    SessionContext::new_with_state(state)
}

/// Build a tenant-scoped `DataFusion` context with an explicit bounded memory
/// pool. Server boot derives the limit from the shared Bifrost parent ceiling.
pub fn wyrd_session_context_with_memory(
    tenant: DataTenantId,
    memory_limit_bytes: usize,
) -> Result<SessionContext, DataFusionError> {
    wyrd_session_context_with_pool(
        tenant,
        Arc::new(GreedyMemoryPool::new(memory_limit_bytes.max(1))),
    )
}

/// Build a tenant-scoped context over a caller-owned `DataFusion` memory pool.
///
/// Server boot shares one bounded pool across all query contexts so concurrent
/// Oracle requests draw from the same parent ceiling instead of receiving one
/// independent per-request allowance.
pub fn wyrd_session_context_with_pool(
    tenant: DataTenantId,
    pool: Arc<dyn MemoryPool>,
) -> Result<SessionContext, DataFusionError> {
    let runtime = RuntimeEnvBuilder::new().with_memory_pool(pool).build()?;
    let state = SessionStateBuilder::new()
        .with_default_features()
        .with_runtime_env(Arc::new(runtime))
        .with_analyzer_rule(Arc::new(TenantPredicateRule::new(tenant)))
        .build();
    Ok(SessionContext::new_with_state(state))
}
