//! Authz-check response payload.

use serde::{Deserialize, Serialize};

/// Successful authz-check response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthzCheckResponse {
    /// RBAC decision for the requested action.
    pub decision: AuthzCheckDecision,
    /// Denial reason slug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Required permission string when RBAC denies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<String>,
}

/// Authz-check decision slug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthzCheckDecision {
    /// The checked action is allowed.
    Allow,
    /// The checked action is denied.
    Deny,
}

impl AuthzCheckResponse {
    /// Build an allow response.
    #[must_use]
    pub fn allow() -> Self {
        Self {
            decision: AuthzCheckDecision::Allow,
            reason: None,
            required: None,
        }
    }

    /// Build a missing-permission deny response.
    #[must_use]
    pub fn missing_permission(required: String) -> Self {
        Self {
            decision: AuthzCheckDecision::Deny,
            reason: Some("missing_permission".to_owned()),
            required: Some(required),
        }
    }
}
