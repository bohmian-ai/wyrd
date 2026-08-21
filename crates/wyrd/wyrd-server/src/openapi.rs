//! Canonical generated OpenAPI document for the Wyrd server.

use utoipa::OpenApi;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, CancelRunningQueryResponse, FreshnessPolicy, ListRunningQueriesResponse,
    QueryClass, RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
    VisibilityMode,
};

/// Source-owned OpenAPI document emitted by the deterministic generator.
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::query::routes::sync_query,
        crate::query::routes::list_running_queries,
        crate::query::routes::get_running_query,
        crate::query::routes::cancel_running_query
    ),
    components(schemas(
        BifrostQueryRequest,
        CancelRunningQueryResponse,
        FreshnessPolicy,
        ListRunningQueriesResponse,
        QueryClass,
        RunningQueryLifecycleState,
        RunningQueryProgress,
        RunningQuerySummary,
        VisibilityMode
    )),
    tags((name = "Bifrost", description = "Bounded Bifrost query transport"))
)]
pub struct WyrdApiDoc;
