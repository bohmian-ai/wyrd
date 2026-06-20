//! Row mirrors for `wyrd.auth_governance_tokens`.

use sqlx::types::{Uuid, chrono};

/// SQL row for `wyrd.auth_governance_tokens`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GovernanceTokenRow {
    /// Governance token identifier.
    pub token_id: String,
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// Card UID bound to the token.
    pub card_uid: String,
    /// Issuer identifier.
    pub issuer: String,
    /// Issuance timestamp.
    pub issued_at: chrono::DateTime<chrono::Utc>,
    /// Expiration timestamp.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Revocation timestamp.
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Previous governance token identifier when rotated.
    pub rotated_from: Option<String>,
    /// Token lifecycle status.
    pub status: String,
    /// Optional workload binding.
    pub workload_binding: Option<String>,
}
