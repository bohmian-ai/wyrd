use utoipa::OpenApi;

fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&wyrd_server::http::openapi::WyrdApiDoc::openapi())
            .expect("OpenAPI document serializes")
    );
}
