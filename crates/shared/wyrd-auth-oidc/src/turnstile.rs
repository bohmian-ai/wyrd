//! Cloudflare Turnstile server-side validation (Siteverify).
//!
//! A browser widget produces a one-use token; only this server-side check
//! decides whether it counts. The secret and token are posted as a form to
//! the Siteverify endpoint through the screened transport, and the caller
//! judges the decoded answer.

use std::net::IpAddr;

use openidconnect::http::{HeaderValue, Method, StatusCode, header};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use url::Url;

use crate::relying_party::ProviderHttpError;
use crate::screening::ScreenedHttp;

/// Cloudflare's Siteverify endpoint.
pub const TURNSTILE_SITEVERIFY: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

/// The Siteverify client.
#[derive(Debug, Clone)]
pub struct Turnstile {
    /// Screened transport the validation goes through.
    http: ScreenedHttp,
    /// Where Siteverify is.
    siteverify: Url,
}

/// Siteverify's answer to one token.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Siteverify {
    /// Whether the token is valid, unexpired and unspent.
    pub success: bool,
    /// The hostname the widget was solved on.
    #[serde(default)]
    pub hostname: Option<String>,
    /// The action the widget was rendered with.
    #[serde(default)]
    pub action: Option<String>,
    /// Why the token was refused, such as `timeout-or-duplicate`.
    #[serde(default, rename = "error-codes")]
    pub error_codes: Vec<String>,
}

impl Turnstile {
    /// A client for Siteverify at `siteverify`.
    #[must_use]
    pub const fn new(http: ScreenedHttp, siteverify: Url) -> Self {
        Self { http, siteverify }
    }

    /// Ask Siteverify whether `token` is valid for `secret`, naming the
    /// visitor's `remote_ip` when known.
    ///
    /// # Errors
    /// Returns [`ProviderHttpError`] when the endpoint is refused by
    /// screening, unreachable, answers other than `200`, or answers
    /// something that is not a Siteverify result.
    pub async fn verify(
        &self,
        secret: &SecretString,
        token: &str,
        remote_ip: Option<IpAddr>,
    ) -> Result<Siteverify, ProviderHttpError> {
        // The serializer is not `Send`, so it ends before the request is sent.
        let body = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("secret", secret.expose_secret());
            form.append_pair("response", token);
            if let Some(ip) = remote_ip {
                form.append_pair("remoteip", &ip.to_string());
            }
            form.finish()
        };
        let mut request = openidconnect::http::Request::new(body.into_bytes());
        *request.method_mut() = Method::POST;
        *request.uri_mut() = self
            .siteverify
            .as_str()
            .parse()
            .map_err(|_| ProviderHttpError::Transport("siteverify URL".to_owned()))?;
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let response = self.http.send(request).await?;
        if response.status() != StatusCode::OK {
            return Err(ProviderHttpError::Transport(format!(
                "siteverify answered {}",
                response.status()
            )));
        }
        serde_json::from_slice(response.body())
            .map_err(|error| ProviderHttpError::Transport(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The secret, token and visitor address are posted as a form, and a
    /// refusal decodes with its error codes.
    #[tokio::test]
    async fn siteverify_receives_the_form_and_decodes_a_refusal() {
        let cloudflare = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::body_string(
                "secret=s3cret&response=tok&remoteip=198.51.100.7",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "success": false, "error-codes": ["timeout-or-duplicate"] }),
            ))
            .expect(1)
            .mount(&cloudflare)
            .await;
        let turnstile = Turnstile::new(
            ScreenedHttp::allowing_internal(),
            Url::parse(&cloudflare.uri()).expect("mock URL"),
        );
        let answer = turnstile
            .verify(
                &SecretString::from("s3cret"),
                "tok",
                Some("198.51.100.7".parse().expect("address")),
            )
            .await
            .expect("siteverify answers");
        assert!(!answer.success);
        assert_eq!(answer.error_codes, ["timeout-or-duplicate"]);
    }

    /// A server error is an outage, not a refusal.
    #[tokio::test]
    async fn an_unavailable_siteverify_is_an_error() {
        let cloudflare = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(503))
            .mount(&cloudflare)
            .await;
        let turnstile = Turnstile::new(
            ScreenedHttp::allowing_internal(),
            Url::parse(&cloudflare.uri()).expect("mock URL"),
        );
        let answer = turnstile
            .verify(&SecretString::from("s3cret"), "tok", None)
            .await;
        assert!(matches!(answer, Err(ProviderHttpError::Unavailable(_))));
    }
}
