//! The OTLP-only API-key authentication entrance.
//!
//! A stock OpenTelemetry exporter is configured with static headers only, so
//! it can neither run the token-endpoint exchange nor renew an access token
//! when one lapses. On the OTLP routes alone, a request that carries
//! `x-wyrd-api-key` and no access token is exchanged through the token
//! endpoint's own API-key grant on every request, and the minted access token
//! replaces the key in the request headers. Ordinary access-token
//! verification then yields exactly the principal a bearer caller gets, before
//! any payload is decoded, so authorization, audit, limits, and tenant scope
//! are the bearer path's own.

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use wyrd_spec::auth::SecretBearer;
use wyrd_spec::error::WyrdError;

use crate::auth::exchange_api_key::api_key_invalid;
use crate::components::auth::routes::TokenGrants;
use crate::components::auth::token_extract::WYRD_ACCESS_TOKEN_HEADER;
use crate::state::AppState;

/// Header a stock OTLP exporter carries its Wyrd API key in.
pub(crate) const WYRD_API_KEY_HEADER: HeaderName = HeaderName::from_static("x-wyrd-api-key");

/// Exchanges an OTLP request's API key for an access token, once per request.
///
/// Shared by the OTLP/HTTP and OTLP/gRPC adapters so both transports verify a
/// key through the one API-key grant the token endpoint serves. It is mounted
/// only in front of OTLP routes, which is what keeps every other surface
/// bearer-only.
#[derive(Clone)]
pub(crate) struct OtlpApiKeyExchange {
    /// Server state holding the auth owners, the runtime store, and the audit
    /// outbox the API-key grant runs against.
    state: AppState,
}

impl OtlpApiKeyExchange {
    /// Bind the exchange to the server state its grant runs against.
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Replace an `x-wyrd-api-key` header with a freshly minted access token.
    ///
    /// A request that already carries `x-wyrd-access-token` is left untouched,
    /// so existing bearer callers keep their exact behavior, and a request
    /// with neither header is left for bearer verification to refuse. A key
    /// is removed from the headers, verified and exchanged through
    /// [`TokenGrants::api_key`] — which stages the grant's audit decision under
    /// `request_id` — and its access token is inserted as a sensitive header
    /// value. Nothing is cached: every request re-verifies its key, so a
    /// revoked key stops working at its next request.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for a key that is not ASCII or is
    /// not usable, the grant's store or issuance errors, and
    /// [`WyrdError::Internal`] when the minted token is not a valid header
    /// value.
    pub(crate) async fn authorize(
        &self,
        headers: &mut HeaderMap,
        request_id: &str,
    ) -> Result<(), WyrdError> {
        if headers.contains_key(WYRD_ACCESS_TOKEN_HEADER) {
            return Ok(());
        }
        let Some(presented) = headers.remove(WYRD_API_KEY_HEADER) else {
            return Ok(());
        };
        let api_key = SecretBearer::new(
            presented
                .to_str()
                .map_err(|_| api_key_invalid())?
                .to_owned(),
        );
        let token = TokenGrants {
            state: &self.state,
            request_id,
        }
        .api_key(&api_key)
        .await?;
        let mut bearer = HeaderValue::try_from(format!("Bearer {}", token.access_token.expose()))
            .map_err(|_| WyrdError::Internal {
            message: "minted access token is not a valid header value".to_owned(),
            details: serde_json::json!({}),
        })?;
        bearer.set_sensitive(true);
        headers.insert(WYRD_ACCESS_TOKEN_HEADER, bearer);
        Ok(())
    }
}
