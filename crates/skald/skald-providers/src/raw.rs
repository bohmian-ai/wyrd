//! RawV1 byte passthrough helpers.

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::value::RawValue;

use crate::error::{ProviderError, ProviderResult};
use crate::retry::RetryPolicy;
use crate::transport::HttpTransport;

/// Sends a raw JSON body without reserializing it and returns raw JSON response.
pub async fn send_raw(
    transport: &HttpTransport,
    provider: &str,
    url: &str,
    mut headers: HeaderMap,
    body: &RawValue,
    retry: &RetryPolicy,
) -> ProviderResult<Box<RawValue>> {
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let text = super::clients::send_bytes_with_retry(
        transport,
        provider,
        reqwest::Method::POST,
        url,
        headers,
        body.get().as_bytes().to_vec(),
        retry,
    )
    .await?;

    RawValue::from_string(text).map_err(|error| ProviderError::decode(provider, error))
}

#[cfg(test)]
mod raw_passthrough {
    use crate::ProviderClient;
    use crate::common;
    use serde_json::value::RawValue;
    use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn raw_v1_writes_body_bytes_directly_and_returns_raw_response() {
        let server = MockServer::start().await;
        let request_body = common::fixture("raw_v1/unknown_provider_request.json");
        let response_body = common::fixture("raw_v1/unknown_provider_response.json");
        Mock::given(method("POST"))
            .and(path("/raw"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body.clone()))
            .mount(&server)
            .await;
        let body = RawValue::from_string(request_body.clone()).expect("raw request parses");

        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::RawV1 {
                provider: ProviderName::OpenAi,
                body,
            })
            .await
            .expect("request succeeds");

        let ProviderResponse::RawV1(body) = response else {
            panic!("expected raw response");
        };
        assert_eq!(body.get(), response_body);
        common::assert_received_body(&server, &request_body).await;
    }
}
