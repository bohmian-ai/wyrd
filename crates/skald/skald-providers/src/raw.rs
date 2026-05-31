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
