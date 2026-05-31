use skald_providers::auth::GoogleOAuth;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Tests the pre-obtained access_token path: JSON contains access_token directly,
// no token_uri exchange or JWT signing needed.
#[tokio::test]
async fn inline_access_token_grants_and_cache_reuses_it() {
    let json = r#"{"access_token":"pre-obtained","expires_in":3600}"#;
    let oauth = GoogleOAuth::from_account_json(json).expect("oauth builds");

    let first = oauth.token().await.expect("token grants");
    let second = oauth.token().await.expect("token cached");

    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&first.access_token),
        "pre-obtained"
    );
    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&second.access_token),
        secrecy::ExposeSecret::expose_secret(&first.access_token)
    );
}

// Tests that service account JSON without client_email returns a clear auth error
// rather than posting a mock assertion to token_uri.
#[tokio::test]
async fn service_account_json_missing_client_email_returns_auth_error() {
    // token_uri is on an allowlisted host so URL validation passes; the error
    // is that client_email is missing before any network call is made.
    let json = r#"{"token_uri":"https://oauth2.googleapis.com/token","private_key":"pk"}"#;
    let oauth = GoogleOAuth::from_account_json(json).expect("oauth builds");

    let err = oauth.token().await.expect_err("missing client_email");

    assert_eq!(err.code(), "SKALD_PROVIDERS_401_AUTH");
    assert!(err.to_string().contains("client_email"));
}

// Tests that token_uri with a non-allowlisted host is rejected before any network call.
#[tokio::test]
async fn service_account_json_invalid_token_uri_host_returns_auth_error() {
    let json = r#"{"token_uri":"https://evil.example.com/token"}"#;
    let oauth = GoogleOAuth::from_account_json(json).expect("oauth builds");

    let err = oauth.token().await.expect_err("disallowed host");

    assert_eq!(err.code(), "SKALD_PROVIDERS_401_AUTH");
    assert!(err.to_string().contains("not allowed"));
}

// Tests that token_uri using http (not https) is rejected.
#[tokio::test]
async fn service_account_json_http_token_uri_returns_auth_error() {
    let json = r#"{"token_uri":"http://oauth2.googleapis.com/token"}"#;
    let oauth = GoogleOAuth::from_account_json(json).expect("oauth builds");

    let err = oauth.token().await.expect_err("http scheme rejected");

    assert_eq!(err.code(), "SKALD_PROVIDERS_401_AUTH");
    assert!(err.to_string().contains("https"));
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
