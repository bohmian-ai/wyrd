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
const ANONYMOUS_PATHS: [&str; 3] = [
    "/auth/platform/token",
    "/auth/platform/login",
    "/auth/platform/callback",
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

/// Authoritative Wyrd HTTP contract.
#[derive(OpenApi)]
#[openapi(
    info(title = "Wyrd API", version = "0.0.1", license(name = "Apache-2.0")),
    modifiers(&SecurityAddon),
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
        crate::components::platform::identity::configure_connection,
        crate::components::platform::identity::read_connection,
        crate::components::platform::identity::remove_connection,
        crate::components::platform::identity::register_admin,
        crate::components::platform::identity::list_platform_admins,
        crate::components::platform::identity::set_admin_status,
        crate::components::platform::identity::begin_login,
        crate::components::platform::identity::complete_login
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
            name = "Platform",
            description = "Platform control plane: tenant lifecycle and platform identity"
        )
    )
)]
pub struct WyrdApiDoc;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::WyrdApiDoc;
    use utoipa::OpenApi;

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
