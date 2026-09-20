//! Generated OpenAPI document for the public HTTP surface.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme};
use utoipa::{Modify, OpenApi};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, CancelRunningQueryResponse, FreshnessPolicy, ListRunningQueriesResponse,
    QueryClass, RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
    VisibilityMode,
};

/// Name the contract gives the one Wyrd authentication scheme.
const WYRD_ACCESS_TOKEN_SCHEME: &str = "wyrdAccessToken";

/// Paths that authenticate no caller, because a caller reaching them has no
/// session yet.
///
/// Everything else inherits the document-level requirement, so a route added
/// tomorrow is documented as authenticated without anyone remembering to say so.
/// A new anonymous route must be listed here; until it is, the contract
/// overstates its protection rather than understating it.
const ANONYMOUS_PATHS: [&str; 6] = [
    "/auth/platform/token",
    "/auth/platform/login",
    "/auth/platform/callback",
    "/auth/login",
    "/auth/callback",
    "/auth/token",
];

/// Declares how every Wyrd surface authenticates.
///
/// One scheme, because there is one header: `X-Wyrd-Access-Token` carries the
/// token on every plane, and the caller's own `Authorization` header is never
/// read by any Wyrd route. The scheme is applied document-wide and lifted from
/// [`ANONYMOUS_PATHS`], which is why a generated client cannot mistake an
/// authenticated route for an open one.
///
/// Written as a modifier rather than repeated on each `#[utoipa::path]` so the
/// requirement cannot drift route by route.
struct SecurityAddon;

impl Modify for SecurityAddon {
    /// Register the scheme, require it globally, and clear it where no session
    /// exists yet.
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
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

        for path in ANONYMOUS_PATHS {
            if let Some(item) = openapi.paths.paths.get_mut(path) {
                // Anonymous routes are all POSTs today; clearing every verb the
                // item could carry keeps this correct if one gains a GET.
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
                    operation.security = Some(Vec::new());
                }
            }
        }
    }
}

/// Media type RFC 9457 problem bodies are actually served with.
const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";

/// Serves every documented problem body under its real media type.
///
/// Handlers return `application/problem+json`, but `#[utoipa::path]` has no way
/// to say so per response without repeating a content-type on every error on
/// every route — which is exactly the kind of repetition that drifts. Rewriting
/// it once here means a generated client's error branch matches what the server
/// sends, for every route that names [`WyrdProblem`] and every route added
/// later.
struct ProblemMediaAddon;

impl Modify for ProblemMediaAddon {
    /// Rename the `application/json` content of every problem-bodied response.
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
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
fn is_problem(content: Option<&utoipa::openapi::Content>) -> bool {
    matches!(
        content.and_then(|content| content.schema.as_ref()),
        Some(utoipa::openapi::RefOr::Ref(reference))
            if reference.ref_location.ends_with("/WyrdProblem")
    )
}

/// Authoritative Wyrd HTTP contract.
#[derive(OpenApi)]
#[openapi(
    info(title = "Wyrd API", version = "0.0.1", license(name = "Apache-2.0")),
    modifiers(&SecurityAddon, &ProblemMediaAddon),
    paths(
        crate::components::cards::routes::register_card_http,
        crate::components::cards::routes::get_card_http,
        crate::components::cards::routes::get_card_by_ref_http,
        crate::components::cards::routes::get_latest_card_http,
        crate::components::cards::routes::list_versions_http,
        crate::components::cards::routes::list_cards_http,
        crate::components::cards::routes::complete_card_http,
        crate::components::cards::routes::list_artifacts_http,
        crate::components::storage::routes::download_init,
        crate::components::cards::routes::delete_card_http,
        crate::components::cards::routes::delete_card_by_ref_http,
        crate::bifrost::routes::register,
        crate::bifrost::routes::list,
        crate::bifrost::routes::describe,
        crate::query::routes::sync_query,
        crate::query::routes::list_running_queries,
        crate::query::routes::get_running_query,
        crate::query::routes::cancel_running_query,
        crate::components::principals::routes::create_service_principal,
        crate::components::principals::routes::issue_credential,
        crate::components::principals::routes::list_credentials,
        crate::components::principals::routes::revoke_credential,
        crate::auth::revoke::revoke_principal,
        crate::components::platform::routes::platform_token,
        crate::components::platform::routes::create_tenant,
        crate::components::platform::routes::recover_tenant_admin,
        crate::components::platform::routes::list_tenants,
        crate::components::platform::routes::inspect_tenant,
        crate::components::platform::routes::set_tenant_status,
        crate::components::platform::identity::configure_connection,
        crate::components::platform::identity::read_connection,
        crate::components::platform::identity::remove_connection,
        crate::components::platform::identity::register_admin,
        crate::components::platform::identity::list_platform_admins,
        crate::components::platform::identity::set_admin_status,
        crate::components::platform::identity::begin_login,
        crate::components::platform::identity::complete_login,
        crate::components::platform::credentials::issue_credential,
        crate::components::platform::credentials::list_credentials,
        crate::components::platform::credentials::revoke_credential,
        crate::auth::login::login,
        crate::components::auth::routes::callback,
        crate::components::auth::routes::token,
        crate::components::auth::routes::issue_key,
        crate::components::admin::routes::create_trusted_issuer,
        crate::components::admin::routes::list_trusted_issuers,
        crate::components::admin::routes::delete_trusted_issuer_route,
        crate::components::admin::routes::create_workload_binding,
        crate::components::admin::routes::list_workload_bindings,
        crate::components::admin::routes::delete_workload_binding_route
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
            name = "Platform",
            description = "Platform control plane: tenant lifecycle, platform identity, and platform credentials"
        )
    )
)]
pub struct WyrdApiDoc;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::WyrdApiDoc;
    use utoipa::OpenApi;
    use wyrd_spec::error::WyrdError;

    #[test]
    fn registration_route_is_published() {
        let document = WyrdApiDoc::openapi();
        assert!(document.paths.paths.contains_key("/v1/cards"));
        for path in [
            "/v1/cards/by-uid/{kind}/{card_uid}",
            "/v1/cards/by-ref",
            "/v1/cards/{kind}/{space}/{name}/latest",
            "/v1/cards/{kind}/{space}/{name}/versions",
            "/v1/cards/{card_uid}/artifacts",
            "/v1/cards/download/init",
            "/v1/bifrost/tables",
            "/v1/bifrost/tables/{namespace}/{name}",
        ] {
            assert!(document.paths.paths.contains_key(path), "missing {path}");
        }
    }

    /// Every auth and admin route the server actually serves is documented.
    ///
    /// The comparison reads the route tables themselves rather than a list
    /// maintained beside them: a second list would drift in exactly the way
    /// that left `/auth/token` and the admin CRUD undocumented while they were
    /// live. Adding a route to either router without annotating it fails here.
    ///
    /// # Panics
    ///
    /// Panics when a served path is absent from the contract.
    #[test]
    fn every_served_auth_and_admin_route_is_documented() {
        let document = WyrdApiDoc::openapi();
        let served = [
            (
                include_str!("../components/auth/routes.rs"),
                "",
                "auth router",
            ),
            (
                include_str!("../components/admin/routes.rs"),
                "/v1",
                "admin router",
            ),
        ];

        let mut checked = BTreeSet::new();
        for (source, prefix, name) in served {
            for path in registered_routes(source) {
                let path = format!("{prefix}{path}");
                assert!(
                    document.paths.paths.contains_key(&path),
                    "the {name} serves {path}, which the OpenAPI contract does not declare"
                );
                checked.insert(path);
            }
        }
        assert_eq!(
            checked.len(),
            6,
            "the route tables no longer register the expected surfaces: {checked:?}"
        );
    }

    /// Extract the paths a router module registers with `.route("...")`.
    ///
    /// Reading the source is what makes this a comparison against the served
    /// surface rather than against a restatement of it.
    fn registered_routes(source: &str) -> Vec<String> {
        source
            .split(".route(")
            .skip(1)
            .filter_map(|rest| rest.split_once('"'))
            .filter_map(|(_, rest)| rest.split_once('"'))
            .map(|(path, _)| path.to_owned())
            .collect()
    }

    /// Tags whose operations a tenant or platform operator drives directly.
    ///
    /// These are the surfaces whose refusals an operator or a generated admin
    /// client has to branch on, so their documented codes are the ones worth
    /// holding to the catalog.
    const ADMINISTRATIVE_TAGS: [&str; 4] = ["Auth", "Admin", "Platform", "Principals"];

    /// Pull every `WYRD_…` stable code named in a response description.
    ///
    /// Descriptions are prose with codes in parentheses rather than a
    /// structured field, so the codes are recovered by scanning for the one
    /// prefix the catalog uses and taking the identifier that follows.
    fn stable_codes(description: &str) -> Vec<String> {
        description
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|token| token.starts_with("WYRD_"))
            .map(str::to_owned)
            .collect()
    }

    /// Every documented problem body is served as `application/problem+json`,
    /// and every administrative problem names real catalog codes for its status.
    ///
    /// A generated client branches on the media type and on the code; declaring
    /// a problem as plain `application/json`, or naming a code that disagrees
    /// with the status it is documented under, breaks that branch silently. The
    /// media-type half holds document-wide because the modifier applies it
    /// there.
    ///
    /// The code half used to accept any description containing `_404_`, which
    /// passes for a code that no longer exists and for a typo in its tail. Each
    /// named code is now reconstructed through [`WyrdError::from_code`] — the
    /// same lookup every client and boundary uses — and its own declared status
    /// must equal the response it is documented under. It covers the four
    /// administrative tags a tenant or platform operator drives.
    ///
    /// # Panics
    ///
    /// Panics when a problem response uses the wrong media type, names no code,
    /// or names one that is absent from the catalog or belongs to another
    /// status.
    #[test]
    fn every_problem_response_declares_its_media_type_and_stable_code() {
        let document = serde_json::to_value(WyrdApiDoc::openapi()).expect("OpenAPI is JSON");
        let mut problems = 0_usize;

        for (path, item) in document["paths"]
            .as_object()
            .expect("paths is an object")
            .iter()
        {
            for (method, operation) in item.as_object().expect("path item is an object") {
                let responses = operation["responses"]
                    .as_object()
                    .expect("an operation declares responses");
                for (status, response) in responses {
                    let content = &response["content"];
                    if content["application/json"]["schema"]["$ref"]
                        .as_str()
                        .is_some_and(|reference| reference.ends_with("/WyrdProblem"))
                    {
                        panic!("{method} {path} {status} declares a problem as application/json");
                    }
                    if !content[super::PROBLEM_MEDIA_TYPE].is_object() {
                        continue;
                    }
                    problems += 1;
                    let tags = operation["tags"].to_string();
                    if !ADMINISTRATIVE_TAGS
                        .iter()
                        .any(|tag| tags.contains(&format!("\"{tag}\"")))
                    {
                        continue;
                    }
                    let description = response["description"].as_str().unwrap_or_default();
                    let codes = stable_codes(description);
                    assert!(
                        !codes.is_empty(),
                        "{method} {path} {status} names no stable code: {description}"
                    );
                    let expected: u16 = status.parse().expect("a response key is a status code");
                    for code in codes {
                        let error = WyrdError::from_code(
                            &code,
                            "documented refusal".to_owned(),
                            serde_json::json!({}),
                        )
                        .unwrap_or_else(|| {
                            panic!("{method} {path} {status} names {code}, absent from the catalog")
                        });
                        assert_eq!(
                            error.status(),
                            expected,
                            "{method} {path} documents {code} under {status}, but the catalog \
                             gives it {}",
                            error.status()
                        );
                    }
                }
            }
        }

        assert!(
            problems > 0,
            "the contract declares no problem responses at all"
        );
    }

    /// The contract names one authentication scheme, requires it everywhere, and
    /// lifts it only where a caller cannot yet have a session.
    ///
    /// # Panics
    ///
    /// Panics when the scheme is missing or misdescribed, when the document
    /// carries no global requirement, when an anonymous path still requires one,
    /// or when an authenticated path was accidentally exempted.
    #[test]
    fn every_authenticated_path_declares_the_one_wyrd_scheme() {
        let document = serde_json::to_value(WyrdApiDoc::openapi()).expect("OpenAPI is JSON");
        let scheme = &document["components"]["securitySchemes"][super::WYRD_ACCESS_TOKEN_SCHEME];
        assert_eq!(scheme["type"], "apiKey");
        assert_eq!(scheme["in"], "header");
        assert_eq!(scheme["name"], "X-Wyrd-Access-Token");
        assert_eq!(
            document["security"],
            serde_json::json!([{ super::WYRD_ACCESS_TOKEN_SCHEME: [] }]),
            "the document requires the scheme by default"
        );

        for (path, item) in document["paths"]
            .as_object()
            .expect("paths is an object")
            .iter()
        {
            let anonymous = super::ANONYMOUS_PATHS.contains(&path.as_str());
            for (method, operation) in item.as_object().expect("path item is an object") {
                let overridden = operation.get("security");
                if anonymous {
                    assert_eq!(
                        overridden,
                        Some(&serde_json::json!([])),
                        "{method} {path} is anonymous and must clear the requirement"
                    );
                } else {
                    assert!(
                        overridden.is_none(),
                        "{method} {path} authenticates and must inherit the requirement, \
                         not override it"
                    );
                }
            }
        }
    }

    /// Every public Bifrost table, query, and lifecycle operation publishes its
    /// pre-stream refusals as typed `WyrdProblem` bodies.
    ///
    /// # Panics
    ///
    /// Panics when an operation omits a common or route-specific refusal, or
    /// publishes one without the shared problem schema.
    #[test]
    fn bifrost_operations_publish_typed_problem_refusals() {
        let document = serde_json::to_value(WyrdApiDoc::openapi()).expect("OpenAPI is JSON");
        let problem_ref = "#/components/schemas/WyrdProblem";
        let operations: [(&str, &str, &[&str]); 7] = [
            ("/v1/bifrost/tables", "post", &["400", "409", "503"]),
            ("/v1/bifrost/tables", "get", &["503"]),
            (
                "/v1/bifrost/tables/{namespace}/{name}",
                "get",
                &["400", "404", "503"],
            ),
            ("/v1/query", "post", &["400", "503"]),
            ("/v1/query/running", "get", &["409", "503"]),
            (
                "/v1/query/{request_id}",
                "get",
                &["400", "404", "409", "503"],
            ),
            (
                "/v1/query/{request_id}",
                "delete",
                &["400", "404", "409", "503"],
            ),
        ];
        for (path, method, specific) in operations {
            let responses = &document["paths"][path][method]["responses"];
            for status in ["401", "403", "default"].iter().chain(specific) {
                assert_eq!(
                    responses[*status]["content"]["application/problem+json"]["schema"]["$ref"],
                    problem_ref,
                    "{method} {path} must publish {status} as WyrdProblem problem+json"
                );
                assert!(
                    responses[*status]["content"]["application/json"].is_null(),
                    "{method} {path} must not publish {status} as plain JSON"
                );
            }
        }
    }

    #[test]
    fn card_contract_publishes_typed_lifecycle_and_problem_shapes() {
        let document = serde_json::to_value(WyrdApiDoc::openapi()).expect("OpenAPI is JSON");
        let paths = document["paths"].as_object().expect("paths object");

        assert!(paths.contains_key("/v1/cards/{card_uid}/complete"));
        assert!(!paths.contains_key("/v1/cards/{card_uid}/abort"));

        let parameters = paths["/v1/cards"]["get"]["parameters"]
            .as_array()
            .expect("list parameters");
        let parameter_names: BTreeSet<&str> = parameters
            .iter()
            .filter_map(|parameter| parameter["name"].as_str())
            .collect();
        let expected_names = BTreeSet::from([
            "kind",
            "space",
            "name",
            "version_range",
            "status",
            "filter",
            "include_prerelease",
            "limit",
            "cursor",
        ]);
        assert_eq!(parameter_names, expected_names);

        let card = &document["components"]["schemas"]["Card"];
        let required = card["required"].as_array().expect("Card required fields");
        assert!(required.iter().any(|field| field == "apiVersion"));
        assert_eq!(
            document["components"]["schemas"]["Spec"]["oneOf"]
                .as_array()
                .expect("typed spec alternatives")
                .len(),
            16
        );

        let problem = &document["components"]["schemas"]["WyrdProblem"];
        let problem_required: BTreeSet<&str> = problem["required"]
            .as_array()
            .expect("problem required fields")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert_eq!(
            problem_required,
            BTreeSet::from([
                "type",
                "title",
                "status",
                "detail",
                "code",
                "details",
                "remediation",
            ])
        );
    }
}
