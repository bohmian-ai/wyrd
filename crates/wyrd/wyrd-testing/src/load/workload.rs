//! Tenant workload descriptors and query shapes.

use std::sync::Arc;

use serde_json::Value;
use wyrd_spec::ids::DataTenantId;

/// How a returned row identifies its owning tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantIdentity {
    /// Compare a row field directly with the authenticated tenant UUID.
    DataTenantId,
    /// Compare a namespace/path field for the authenticated tenant UUID.
    NamespacePath,
}

/// Expected row fields used by load assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedRowShape {
    /// Field containing the row identifier used for reconciliation.
    pub row_id_field: String,
    /// Field containing the tenant ID or tenant-owned namespace path.
    pub tenant_field: String,
    /// Isolation comparison to use for the tenant field.
    pub tenant_identity: TenantIdentity,
}

impl Default for ExpectedRowShape {
    fn default() -> Self {
        Self {
            row_id_field: "row_id".to_owned(),
            tenant_field: "data_tenant_id".to_owned(),
            tenant_identity: TenantIdentity::DataTenantId,
        }
    }
}

/// Build a query string for a tenant query sequence number.
pub type QueryGenerator = Arc<dyn Fn(u64) -> String + Send + Sync + 'static>;

/// Request passed to a sustained-load query callback.
#[derive(Debug, Clone)]
pub struct QueryRequest {
    /// Principal tenant whose rows must be returned.
    pub tenant: DataTenantId,
    /// Monotonic sequence local to the workload's query loop.
    pub sequence: u64,
    /// Query text or a caller-defined query token.
    pub query: String,
}

/// One tenant's sustained-load descriptor.
#[derive(Clone)]
pub struct TenantWorkload {
    /// Tenant whose ingest and query loops are driven.
    pub tenant: DataTenantId,
    /// Target number of rows submitted per second.
    pub rows_per_sec: u64,
    /// Number of concurrent query loops for this tenant.
    pub concurrent_query_count: usize,
    /// Builds a query for each query-loop sequence number.
    pub query_generator: QueryGenerator,
    /// Expected shape used for row reconciliation and leak assertions.
    pub expected_row_shape: ExpectedRowShape,
}

impl TenantWorkload {
    /// Create a tenant workload descriptor.
    #[must_use]
    pub fn new(
        tenant: DataTenantId,
        rows_per_sec: u64,
        concurrent_query_count: usize,
        query_generator: QueryGenerator,
        expected_row_shape: ExpectedRowShape,
    ) -> Self {
        Self {
            tenant,
            rows_per_sec,
            concurrent_query_count: concurrent_query_count.max(1),
            query_generator,
            expected_row_shape,
        }
    }

    /// Create a workload with a constant query token and the default row shape.
    #[must_use]
    pub fn simple(
        tenant: DataTenantId,
        rows_per_sec: u64,
        concurrent_query_count: usize,
        query: impl Into<String>,
    ) -> Self {
        let query = query.into();
        Self::new(
            tenant,
            rows_per_sec,
            concurrent_query_count,
            Arc::new(move |_| query.clone()),
            ExpectedRowShape::default(),
        )
    }

    /// Build a JSON row shape for a generated row ID and tenant.
    #[must_use]
    pub fn default_row(row_id: u64, tenant: DataTenantId) -> Value {
        serde_json::json!({
            "row_id": row_id.to_string(),
            "data_tenant_id": tenant.to_string(),
        })
    }
}
