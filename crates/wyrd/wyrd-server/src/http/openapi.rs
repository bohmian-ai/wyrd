//! Generated OpenAPI document for the public HTTP surface.

use utoipa::OpenApi;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, CancelRunningQueryResponse, FreshnessPolicy, ListRunningQueriesResponse,
    QueryClass, RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
    VisibilityMode,
};

/// Authoritative Wyrd HTTP contract.
#[derive(OpenApi)]
#[openapi(
    info(title = "Wyrd API", version = "0.0.1", license(name = "Apache-2.0")),
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
