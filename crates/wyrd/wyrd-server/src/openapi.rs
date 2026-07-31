//! Canonical generated OpenAPI document for the Wyrd server.

use utoipa::OpenApi;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};

/// Source-owned OpenAPI document emitted by the deterministic generator.
#[derive(OpenApi)]
#[openapi(
    paths(crate::query::routes::sync_query),
    components(schemas(BifrostQueryRequest, FreshnessPolicy, VisibilityMode)),
    tags((name = "Bifrost", description = "Bounded Bifrost query transport"))
)]
pub struct WyrdApiDoc;
