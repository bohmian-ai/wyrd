//! Generated OpenAPI document for the public HTTP surface.

use std::collections::BTreeSet;

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme};
use utoipa::openapi::{Content, OpenApi as OpenApiDocument};
use utoipa::{Modify, OpenApi};
use wyrd_spec::card::trigger::{TriggerActivation, TriggerSpec};
use wyrd_spec::card::verifier::{VerificationBinding, VerifierSpec};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, CancelRunningQueryResponse, ListRunningQueriesResponse, QueryClass,
    RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
};

/// Name the contract gives the one Wyrd authentication scheme.
pub(crate) const WYRD_ACCESS_TOKEN_SCHEME: &str = "wyrdAccessToken";

/// Declares how every Wyrd surface authenticates.
///
/// One caller scheme, because there is one header: `X-Wyrd-Access-Token` carries the
/// token on every plane. The public gateway inference ingress additionally
/// accepts the same Wyrd access token, or a Wyrd API key it exchanges for one,
/// in the header its provider SDK sends (`Authorization`, `x-api-key`, or
/// `x-goog-api-key`); no other route reads the
/// caller's own `Authorization` header. The scheme is applied document-wide, so a route added
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
    /// Register the caller scheme and require it document-wide.
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

/// Name the contract gives the tenant browser-session cookie scheme.
pub(crate) const BROWSER_SESSION_SCHEME: &str = "wyrdBrowserSession";

/// Declares the browser-session cookie as the alternative to the access token
/// on exactly the routes whose authentication layer accepts it.
///
/// Only the protected tenant `/v1` HTTP routes authenticate a browser session
/// (`require_tenant_authenticated` and the Bifrost layer); MCP, OTLP, the
/// platform plane and gateway inference stay bearer-only. The router collects
/// those routes' paths from the same registrations that mount them, so this
/// modifier never keeps a second list. Each such operation that inherits the
/// document requirement is given two alternatives — the access token, or the
/// session cookie — and an operation that clears its requirement is left alone.
pub(crate) struct BrowserSessionAddon {
    /// Served paths of the operations that accept a browser session.
    pub(crate) paths: BTreeSet<String>,
}

impl Modify for BrowserSessionAddon {
    /// Register the cookie scheme and offer it beside the access token on
    /// every collected operation that has no requirement of its own.
    fn modify(&self, openapi: &mut OpenApiDocument) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BROWSER_SESSION_SCHEME,
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "wyrd_session_{tenant}",
                "Opaque tenant browser session set by `/auth/callback` or \
                 `POST /auth/browser/api-key`, named by the tenant's route key. Used only \
                 when no `X-Wyrd-Access-Token` is sent: an explicit token always wins and \
                 never falls back to the cookie. Exactly one session cookie may be sent \
                 (`400 WYRD_SPEC_400_VALIDATION` otherwise). Any method but GET, HEAD and \
                 OPTIONS must carry this deployment's exact `Origin` \
                 (`403 WYRD_AUTH_403_ORIGIN_REFUSED` otherwise). A dead session is \
                 `401 WYRD_AUTH_401_INVALID_TOKEN`.",
            ))),
        );
        let alternatives = vec![
            SecurityRequirement::new(WYRD_ACCESS_TOKEN_SCHEME, Vec::<String>::new()),
            SecurityRequirement::new(BROWSER_SESSION_SCHEME, Vec::<String>::new()),
        ];
        for (path, item) in &mut openapi.paths.paths {
            if !self.paths.contains(path) {
                continue;
            }
            for operation in [
                item.get.as_mut(),
                item.put.as_mut(),
                item.post.as_mut(),
                item.delete.as_mut(),
                item.patch.as_mut(),
                item.head.as_mut(),
                item.options.as_mut(),
            ]
            .into_iter()
            .flatten()
            {
                if operation.security.is_none() {
                    operation.security = Some(alternatives.clone());
                }
            }
        }
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
/// sends, for every route that names
/// [`WyrdProblem`](wyrd_spec::error::WyrdProblem) and every route added later.
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
    info(title = "Wyrd API", version = env!("CARGO_PKG_VERSION"), license(name = "Apache-2.0")),
    components(schemas(
        BifrostQueryRequest,
        CancelRunningQueryResponse,
        ListRunningQueriesResponse,
        QueryClass,
        RunningQueryLifecycleState,
        RunningQueryProgress,
        RunningQuerySummary,
        TriggerActivation,
        TriggerSpec,
        VerificationBinding,
        VerifierSpec
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
            name = "Identity",
            description = "Tenant human OIDC login connection administration"
        ),
        (
            name = "Cards",
            description = "Card registration, resolution, listing, and deletion"
        ),
        (
            name = "Storage",
            description = "Card artifact upload and download plans"
        ),
        (
            name = "Verification",
            description = "Verification binding status, manual Verifier runs, and run status"
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
            name = "Platform",
            description = "Platform control plane: tenant lifecycle, platform identity, and platform credentials"
        ),
        (
            name = "Gateway",
            description = "Governed model inference and tenant gateway administration"
        )
    )
)]
pub struct WyrdApiDoc;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::WyrdApiDoc;
    use utoipa::OpenApi;

    /// Collect component references nested under one schema.
    fn collect_refs(value: &serde_json::Value, found: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(serde_json::Value::String(target)) = object.get("$ref")
                    && let Some(name) = target.strip_prefix("#/components/schemas/")
                {
                    found.insert(name.to_owned());
                }
                for nested in object.values() {
                    collect_refs(nested, found);
                }
            }
            serde_json::Value::Array(items) => {
                for nested in items {
                    collect_refs(nested, found);
                }
            }
            _ => {}
        }
    }

    /// Every component reachable from the Verifier contract resolves.
    #[test]
    fn verifier_contract_component_references_all_resolve() {
        let document = serde_json::to_value(WyrdApiDoc::openapi()).expect("OpenAPI is JSON");
        let schemas = document["components"]["schemas"]
            .as_object()
            .expect("the document defines components.schemas");
        let mut pending: Vec<String> = [
            "VerifierSpec",
            "VerificationBinding",
            "TriggerSpec",
            "TriggerActivation",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let mut visited = BTreeSet::new();

        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            let schema = schemas.get(&name).unwrap_or_else(|| {
                panic!("{name} is reachable from the Verifier contract but is undefined")
            });
            let mut referenced = BTreeSet::new();
            collect_refs(schema, &mut referenced);
            pending.extend(referenced);
        }
    }
}
