//! Authz-check request description.

use http::HeaderMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wyrd_spec::reference::CardRef;

/// Request body for an authz check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthzCheckRequest {
    /// Target card for the simulated permission check.
    pub target: CardRef,
    /// Action to check against the current actor. Well-known value: `"card_write"`.
    /// Custom actions must be valid `Permission` identifiers.
    pub action: String,
    /// Additional policy context.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Request metadata projected by the service mesh for an authz check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzCheckRequestMetadata {
    /// HTTP verb of the original request.
    pub method: String,
    /// Path of the original request.
    pub path: String,
    /// Host of the original request.
    pub host: String,
}

/// Header parsing failures for [`AuthzCheckRequestMetadata`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthzCheckRequestError {
    /// A required projected request header was missing.
    #[error("missing required header: {header}")]
    MissingHeader {
        /// Missing header name.
        header: &'static str,
    },
    /// A required projected request header was not valid UTF-8.
    #[error("header is not valid UTF-8: {header}")]
    InvalidUtf8 {
        /// Invalid header name.
        header: &'static str,
    },
}

impl AuthzCheckRequestMetadata {
    /// Read `X-Original-Method`, `X-Original-Path`, and `X-Original-Host`.
    ///
    /// # Errors
    /// Returns an error when any required header is missing or not valid UTF-8.
    pub fn from_headers(headers: &HeaderMap) -> Result<Self, AuthzCheckRequestError> {
        let pull = |header: &'static str| -> Result<String, AuthzCheckRequestError> {
            headers
                .get(header)
                .ok_or(AuthzCheckRequestError::MissingHeader { header })?
                .to_str()
                .map(str::to_owned)
                .map_err(|_| AuthzCheckRequestError::InvalidUtf8 { header })
        };

        Ok(Self {
            method: pull("x-original-method")?,
            path: pull("x-original-path")?,
            host: pull("x-original-host")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, HeaderValue};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    use super::{AuthzCheckRequest, AuthzCheckRequestError, AuthzCheckRequestMetadata};

    fn card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("callee").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("test").expect("static space is valid")),
            uid: None,
        }
    }

    #[test]
    fn request_body_serializes_target_action_context() {
        let request = AuthzCheckRequest {
            target: card_ref(),
            action: "card_write".to_owned(),
            context: serde_json::json!({ "source": "test" }),
        };

        let value = serde_json::to_value(&request).expect("request serializes");

        assert_eq!(value["target"]["kind"], "Service");
        assert_eq!(value["action"], "card_write");
        assert_eq!(value["context"]["source"], "test");
    }

    #[test]
    fn parses_mesh_projected_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-original-method", HeaderValue::from_static("POST"));
        headers.insert("x-original-path", HeaderValue::from_static("/v1/cards"));
        headers.insert("x-original-host", HeaderValue::from_static("service.wyrd"));

        let request = AuthzCheckRequestMetadata::from_headers(&headers).expect("headers parse");

        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/cards");
        assert_eq!(request.host, "service.wyrd");
    }

    #[test]
    fn missing_projected_header_is_reported_by_name() {
        let headers = HeaderMap::new();

        let error =
            AuthzCheckRequestMetadata::from_headers(&headers).expect_err("method is required");

        assert_eq!(
            error,
            AuthzCheckRequestError::MissingHeader {
                header: "x-original-method"
            }
        );
    }

    #[test]
    fn invalid_utf8_projected_header_is_reported_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-original-method",
            HeaderValue::from_bytes(&[0xff]).expect("invalid UTF-8 header bytes are accepted"),
        );
        headers.insert("x-original-path", HeaderValue::from_static("/v1/cards"));
        headers.insert("x-original-host", HeaderValue::from_static("service.wyrd"));

        let error =
            AuthzCheckRequestMetadata::from_headers(&headers).expect_err("method is invalid");

        assert_eq!(
            error,
            AuthzCheckRequestError::InvalidUtf8 {
                header: "x-original-method"
            }
        );
    }
}
