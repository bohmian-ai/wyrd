//! Generated OpenAPI document for the public HTTP surface.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme};
use utoipa::openapi::{Content, OpenApi as OpenApiDocument};
use utoipa::{Modify, OpenApi};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, CancelRunningQueryResponse, FreshnessPolicy, ListRunningQueriesResponse,
    QueryClass, RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
    VisibilityMode,
};

/// Name the contract gives the one Wyrd authentication scheme.
pub(crate) const WYRD_ACCESS_TOKEN_SCHEME: &str = "wyrdAccessToken";

/// Declares how every Wyrd surface authenticates.
///
/// One scheme, because there is one header: `X-Wyrd-Access-Token` carries the
/// token on every plane, and the caller's own `Authorization` header is never
/// read by any Wyrd route. The scheme is applied document-wide, so a route added
/// tomorrow is documented as authenticated without anyone remembering to say so;
/// the handful of operations a caller reaches before it has a session clear the
/// requirement themselves with `security(())`, beside the handler, where the
/// fact is checkable against the code rather than against a second list.
///
/// Written as a modifier rather than repeated on each `#[utoipa::path]` so the
/// requirement cannot drift route by route. It is applied by
/// [`crate::http::router::build_router`] after the routers have been composed,
/// because the document only has paths once they are.
pub(crate) struct SecurityAddon;

impl Modify for SecurityAddon {
    /// Register the scheme and require it document-wide.
    fn modify(&self, openapi: &mut OpenApiDocument) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            WYRD_ACCESS_TOKEN_SCHEME,
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-Wyrd-Access-Token",
                "Wyrd access token as `Bearer <token>`. A tenant access token and a \
                 platform session travel on this same header; which plane a token \
                 reaches is decided by its scope and by the route, never by the header.",
            ))),
        );
        openapi.security = Some(vec![SecurityRequirement::new(
            WYRD_ACCESS_TOKEN_SCHEME,
            Vec::<String>::new(),
        )]);
    }
}

/// Media type RFC 9457 problem bodies are actually served with.
pub(crate) const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";

/// Serves every documented problem body under its real media type.
///
/// Handlers return `application/problem+json`, but `#[utoipa::path]` has no way
/// to say so per response without repeating a content-type on every error on
/// every route — which is exactly the kind of repetition that drifts. Rewriting
/// it once here means a generated client's error branch matches what the server
/// sends, for every route that names [`WyrdProblem`] and every route added
/// later.
pub(crate) struct ProblemMediaAddon;

impl Modify for ProblemMediaAddon {
    /// Rename the `application/json` content of every problem-bodied response.
    fn modify(&self, openapi: &mut OpenApiDocument) {
        for item in openapi.paths.paths.values_mut() {
            for operation in [
                item.get.as_mut(),
                item.put.as_mut(),
                item.post.as_mut(),
                item.delete.as_mut(),
                item.patch.as_mut(),
            ]
            .into_iter()
            .flatten()
            {
                for response in operation.responses.responses.values_mut() {
                    let utoipa::openapi::RefOr::T(response) = response else {
                        continue;
                    };
                    if !is_problem(response.content.get("application/json")) {
                        continue;
                    }
                    let Some(content) = response.content.shift_remove("application/json") else {
                        continue;
                    };
                    response
                        .content
                        .insert(PROBLEM_MEDIA_TYPE.to_owned(), content);
                }
            }
        }
    }
}

/// Whether a response body is the problem document rather than a success shape.
///
/// A problem response is declared as `body = WyrdProblem`, which utoipa emits as
/// a reference to that component; matching on the reference is what keeps the
/// media-type rewrite off success bodies, which really are `application/json`.
fn is_problem(content: Option<&Content>) -> bool {
    matches!(
        content.and_then(|content| content.schema.as_ref()),
        Some(utoipa::openapi::RefOr::Ref(reference))
            if reference.ref_location.ends_with("/WyrdProblem")
    )
}

/// The document-level half of the Wyrd HTTP contract.
///
/// Info, shared components, and tags live here; operations do not. Every path
/// is contributed by the router module that mounts it, through `utoipa-axum`
/// co-registration, so a served method cannot exist without being documented
/// and a documented operation cannot exist without being served.
/// [`crate::http::router::build_router`] composes this with those routers and
/// applies the two document-wide modifiers to the result.
#[derive(OpenApi)]
#[openapi(
    info(title = "Wyrd API", version = "0.0.1", license(name = "Apache-2.0")),
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
    tags(
        (name = "Bifrost", description = "Bounded Bifrost query transport"),
        (
            name = "Principals",
            description = "Tenant principal and credential administration"
        ),
        (
            name = "Auth",
            description = "Tenant-plane sign-in, credential exchange, and credential issuance"
        ),
        (
            name = "Admin",
            description = "Tenant administration of trusted OIDC issuers and workload bindings"
        ),
        (
            name = "Cards",
            description = "Card registration, resolution, listing, and deletion"
        ),
        (
            name = "Storage",
            description = "Card artifact upload and download plans, and the local development \
                           blob transport"
        ),
        (
            name = "Authz",
            description = "Delegated invoke authorization checks"
        ),
        (
            name = "Observability",
            description = "OTLP/HTTP ingest for traces, metrics, and logs"
        ),
        (
            name = "Eval",
            description = "Evaluation pull protocol: open a run and drive its turns"
        ),
        (
            name = "Platform",
            description = "Platform control plane: tenant lifecycle, platform identity, and platform credentials"
        )
    )
)]
pub struct WyrdApiDoc;
