use std::sync::Arc;

use secrecy::SecretString;
use wiremock::matchers::{body_bytes, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_registry::{CardSelector, Cards};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardUid, SpaceName};
use wyrd_spec::registry::ListCardsRequest;

fn client(base_url: String) -> WyrdClient {
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.clone();
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from("test-bearer")),
    )
    .expect("auth builds");
    let transport = HttpTransport::new(
        &HttpConfig {
            base_url,
            ..HttpConfig::default()
        },
        Arc::clone(&auth),
    )
    .expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

fn uid() -> CardUid {
    CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").expect("test uid is valid")
}

#[tokio::test]
async fn list_uses_typed_query_parameters_and_an_empty_get_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/cards"))
        .and(query_param("kind", "Prompt"))
        .and(query_param("space", "prod"))
        .and(query_param("limit", "20"))
        .and(query_param("cursor", "opaque-next"))
        .and(body_bytes(Vec::<u8>::new()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [],
            "next_cursor": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    let cards = Cards::with_client(client(server.uri()));
    let page = cards
        .list(ListCardsRequest {
            kind: Some(CardKind::Prompt),
            space: Some(SpaceName::new("prod").expect("test space is valid")),
            name: None,
            version_range: None,
            status: None,
            filter: None,
            include_prerelease: false,
            limit: Some(20),
            cursor: Some("opaque-next".to_owned()),
        })
        .await
        .expect("typed list succeeds");
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn uid_delete_uses_kind_qualified_path_and_preserves_idempotence() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/v1/cards/by-uid/Prompt/01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
        ))
        .and(body_bytes(Vec::<u8>::new()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "card_uid": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
            "deleted": false
        })))
        .expect(1)
        .mount(&server)
        .await;

    let cards = Cards::with_client(client(server.uri()));
    cards
        .delete(CardSelector::uid(CardKind::Prompt, uid()))
        .await
        .expect("idempotent delete succeeds when already deleted");
}

#[tokio::test]
async fn server_problem_code_and_status_survive_registry_boundary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/v1/cards/by-uid/Prompt/01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "code": "WYRD_REGISTRY_404_CARD_NOT_FOUND",
            "detail": "card not found",
            "details": {"tenant": "redacted"}
        })))
        .mount(&server)
        .await;

    let cards = Cards::with_client(client(server.uri()));
    let error = cards
        .get(CardSelector::uid(CardKind::Prompt, uid()))
        .await
        .expect_err("missing card must be returned as a Wyrd error");
    assert_eq!(error.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    assert_eq!(error.status(), 404);
    assert_eq!(error.as_problem_json()["details"]["tenant"], "redacted");
}

#[test]
fn client_debug_redacts_constructor_secret() {
    let config = ClientConfig {
        api_key: Some(SecretString::from("wyrd_sk_private")),
        http: HttpConfig {
            base_url: "http://localhost:50050".to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let client = WyrdClient::with_config(config).expect("configured client assembles");
    let rendered = format!("{client:?}");
    assert!(!rendered.contains("wyrd_sk_private"));
}
