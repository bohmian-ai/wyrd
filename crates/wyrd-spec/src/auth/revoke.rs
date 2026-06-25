//! Principal revocation contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::PrincipalId;

/// Principal-kind discriminator for auth administration requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// Human user principal.
    User,
    /// Service principal.
    Service,
    /// Agent principal.
    Agent,
}

/// `POST /v1/principals/{id}/revoke` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RevokePrincipalRequest {
    /// Required kind discriminator; principal ids are only unique with their table.
    pub principal_kind: PrincipalKind,
    /// Required audit reason for the revocation.
    pub reason: String,
}

/// `POST /v1/principals/{id}/revoke` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RevokePrincipalResponse {
    /// Revoked principal id.
    pub principal_id: PrincipalId,
    /// Revoked principal kind.
    pub principal_kind: PrincipalKind,
    /// New tokens-not-before epoch.
    pub revoked_at: DateTime<Utc>,
    /// Number of refresh-token family rows revoked.
    pub refresh_tokens_revoked: u64,
}

#[cfg(test)]
mod tests {
    use super::{PrincipalKind, RevokePrincipalRequest};

    #[test]
    fn revoke_principal_request_roundtrips() {
        let request = RevokePrincipalRequest {
            principal_kind: PrincipalKind::Service,
            reason: "operator requested rotation".to_owned(),
        };

        let value = serde_json::to_value(&request).expect("serializes");

        assert_eq!(value["principal_kind"], "service");
        assert_eq!(
            serde_json::from_value::<RevokePrincipalRequest>(value).expect("deserializes"),
            request
        );
    }

    #[test]
    fn revoke_principal_request_requires_kind_and_reason() {
        let missing_kind = serde_json::json!({ "reason": "incident response" });
        let missing_reason = serde_json::json!({ "principal_kind": "user" });

        assert!(serde_json::from_value::<RevokePrincipalRequest>(missing_kind).is_err());
        assert!(serde_json::from_value::<RevokePrincipalRequest>(missing_reason).is_err());
    }

    #[test]
    fn revoke_principal_request_rejects_unknown_fields() {
        let request = serde_json::json!({
            "principal_kind": "agent",
            "reason": "incident response",
            "extra": true
        });

        assert!(serde_json::from_value::<RevokePrincipalRequest>(request).is_err());
    }
}
