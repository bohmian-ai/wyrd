//! Generated OpenAPI document for the public HTTP surface.

use utoipa::OpenApi;

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
        crate::components::cards::routes::list_artifacts_http,
        crate::components::storage::routes::download_init,
        crate::components::cards::routes::delete_card_http,
        crate::components::cards::routes::delete_card_by_ref_http
    )
)]
pub struct WyrdApiDoc;

#[cfg(test)]
mod tests {
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
        ] {
            assert!(document.paths.paths.contains_key(path), "missing {path}");
        }
    }
}
