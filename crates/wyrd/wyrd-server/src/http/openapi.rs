//! Generated OpenAPI document for the public HTTP surface.

use utoipa::OpenApi;

/// Authoritative Wyrd HTTP contract.
#[derive(OpenApi)]
#[openapi(
    info(title = "Wyrd API", version = "0.0.1", license(name = "Apache-2.0")),
    paths(
        crate::components::cards::routes::register_card_http,
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
    }
}
