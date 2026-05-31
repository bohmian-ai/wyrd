use reqwest::StatusCode;
use skald_providers::ProviderError;

#[test]
fn status_errors_map_to_stable_codes() {
    assert_eq!(
        ProviderError::from_status("openai", StatusCode::UNAUTHORIZED, "", None).code(),
        "SKALD_PROVIDERS_401_AUTH"
    );
    assert_eq!(
        ProviderError::from_status("openai", StatusCode::TOO_MANY_REQUESTS, "", Some(1000)).code(),
        "SKALD_PROVIDERS_429_RATE_LIMIT"
    );
    assert_eq!(
        ProviderError::from_status(
            "anthropic",
            StatusCode::from_u16(529).unwrap(),
            "overloaded",
            None
        )
        .code(),
        "SKALD_PROVIDERS_5XX_UPSTREAM"
    );
    assert_eq!(
        ProviderError::from_status("google", StatusCode::BAD_REQUEST, "bad", None).code(),
        "SKALD_PROVIDERS_400_BAD_REQUEST"
    );
    assert_eq!(
        ProviderError::decode("openai", "bad json").code(),
        "SKALD_PROVIDERS_502_DECODE"
    );
}
