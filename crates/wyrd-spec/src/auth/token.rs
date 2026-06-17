//! Auth token request and response contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{PrincipalId, SecretBearer};
use crate::reference::CardRef;

/// Body of `POST /auth/token`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "grant_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TokenRequest {
    /// Wyrd-native API key exchange.
    WyrdApiKey {
        /// API key from the deployment secret store.
        api_key: SecretBearer,
    },
    /// RFC 8693 token exchange for agent/service delegation.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:token-exchange")]
    TokenExchange {
        /// Existing access token to delegate from.
        subject_token: SecretBearer,
        /// Type of `subject_token`.
        subject_token_type: SubjectTokenType,
        /// Requested non-human target principal.
        requested_subject: RequestedSubject,
    },
}

/// Token-exchange target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RequestedSubject {
    /// Target by principal id.
    PrincipalId {
        /// Stable principal id.
        id: PrincipalId,
    },
    /// Target by card reference.
    CardRef {
        /// Service or Agent card reference.
        card_ref: CardRef,
    },
}

/// Supported token type for RFC 8693 subject token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum SubjectTokenType {
    /// OAuth access token.
    #[serde(rename = "urn:ietf:params:oauth:token-type:access_token")]
    AccessToken,
}

/// Response from `POST /auth/token`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct TokenResponse {
    /// Signed Wyrd access token.
    pub access_token: SecretBearer,
    /// Refresh token.
    pub refresh_token: SecretBearer,
    /// Token type.
    pub token_type: TokenType,
    /// Access-token expiry timestamp.
    pub expires_at: DateTime<Utc>,
}

/// Bearer token marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "PascalCase")]
pub enum TokenType {
    /// Bearer token.
    Bearer,
}

#[cfg(test)]
mod tests {
    use super::{RequestedSubject, TokenRequest, TokenResponse, TokenType};
    use crate::auth::SecretBearer;
    use crate::envelope::CardKind;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use crate::version::VersionBlock;
    use chrono::Utc;

    #[test]
    fn token_response_has_no_card_ref_wire_field() {
        let response = TokenResponse {
            access_token: SecretBearer::new("access".to_owned()),
            refresh_token: SecretBearer::new("refresh".to_owned()),
            token_type: TokenType::Bearer,
            expires_at: Utc::now(),
        };

        let json = serde_json::to_value(response).expect("serializes");

        assert!(json.get("card_ref").is_none());
    }

    #[test]
    fn token_exchange_uses_rfc_8693_grant_type() {
        let request = TokenRequest::TokenExchange {
            subject_token: SecretBearer::new("access".to_owned()),
            subject_token_type: super::SubjectTokenType::AccessToken,
            requested_subject: RequestedSubject::CardRef {
                card_ref: card_ref(CardKind::Agent),
            },
        };

        let json = serde_json::to_value(request).expect("serializes");

        assert_eq!(
            json["grant_type"],
            "urn:ietf:params:oauth:grant-type:token-exchange"
        );
    }

    #[test]
    fn serde_grant_type_discriminator() {
        let api_key_request = TokenRequest::WyrdApiKey {
            api_key: SecretBearer::new("key".to_owned()),
        };
        let exchange_request = TokenRequest::TokenExchange {
            subject_token: SecretBearer::new("access".to_owned()),
            subject_token_type: super::SubjectTokenType::AccessToken,
            requested_subject: RequestedSubject::CardRef {
                card_ref: card_ref(CardKind::Agent),
            },
        };

        let api_key_json = serde_json::to_value(&api_key_request).expect("serializes");
        let exchange_json = serde_json::to_value(&exchange_request).expect("serializes");

        assert_eq!(api_key_json["grant_type"], "wyrd_api_key");
        assert_eq!(
            exchange_json["grant_type"],
            "urn:ietf:params:oauth:grant-type:token-exchange"
        );
        assert_eq!(
            serde_json::from_value::<TokenRequest>(api_key_json).expect("deserializes"),
            api_key_request
        );
        assert_eq!(
            serde_json::from_value::<TokenRequest>(exchange_json).expect("deserializes"),
            exchange_request
        );
    }

    fn card_ref(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("runtime").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }
}
