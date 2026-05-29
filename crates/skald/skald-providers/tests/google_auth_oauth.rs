use skald_providers::auth::GoogleOAuth;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn service_account_json_grants_token_and_cache_reuses_it() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"granted-token","expires_in":3600}"#),
        )
        .expect(1)
        .mount(&server)
        .await;
    let json = format!(r#"{{"token_uri":"{}/token"}}"#, server.uri());
    let oauth = GoogleOAuth::from_account_json(json).expect("oauth builds");

    let first = oauth.token().await.expect("token grants");
    let second = oauth.token().await.expect("token cached");

    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&first.access_token),
        secrecy::ExposeSecret::expose_secret(&second.access_token)
    );
}

#[tokio::test]
async fn metadata_server_grants_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/computeMetadata/v1/instance/service-accounts/default/token",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"metadata-token","expires_in":3600}"#),
        )
        .mount(&server)
        .await;
    let oauth = GoogleOAuth::from_metadata_server(server.uri()).expect("oauth builds");

    let token = oauth.token().await.expect("token grants");

    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&token.access_token),
        "metadata-token"
    );
}
