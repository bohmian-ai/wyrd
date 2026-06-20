//! API-key issuance contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::SecretBearer;
use crate::reference::CardRef;

/// Body of `POST /auth/issue-key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IssueKeyRequest {
    /// Service or Agent card the key binds to.
    pub card_ref: CardRef,
    /// Optional operator label stored with the key row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional TTL override in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u32>,
}

/// Response from `POST /auth/issue-key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IssueKeyResponse {
    /// API key row id.
    pub key_id: String,
    /// Plaintext API key, returned exactly once.
    pub key: SecretBearer,
    /// Log-safe prefix used for lookup.
    pub prefix: String,
    /// Confirmed card binding.
    pub card_ref: CardRef,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Expiry timestamp.
    pub expires_at: DateTime<Utc>,
}
