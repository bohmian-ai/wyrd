//! Row mirror for `wyrd.auth_trusted_issuers`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::types::Uuid;

/// SQL row for `wyrd.auth_trusted_issuers`.
///
/// Carries the full lossless column set from the migration so commit 04's
/// resolver can reconstruct a complete `TrustedIssuer` without a second query.
/// `client_secret_enc` is returned byte-for-byte; callers must not interpret
/// or trim the BYTEA — the 12-byte AES-GCM nonce is the leading prefix.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrustedIssuerRow {
    /// Tenant isolation UUID as stored by Postgres.
    pub data_tenant_id: Uuid,
    /// OIDC issuer URL (natural composite key with `data_tenant_id`).
    pub issuer_url: String,
    /// Resolved JWKS endpoint URI.
    pub jwks_uri: String,
    /// Expected `aud` claim value for tokens from this issuer.
    pub expected_audience: String,
    /// Wyrd's OAuth 2.0 client identifier at this IdP.
    pub client_id: String,
    /// `ClientAuth` discriminant: `SecretBasic`, `SecretPost`, `PrivateKeyJwt`, or `Public`.
    pub client_auth: String,
    /// Lossless `ClaimMapping` (subject path required; email and groups optional).
    #[sqlx(json)]
    pub claim_mapping: Value,
    /// Per-issuer IdP group → Wyrd role mapping (`HashMap<String, Vec<String>>`).
    #[sqlx(json)]
    pub group_role_map: Value,
    /// Baseline roles granted to every federated principal from this issuer.
    #[sqlx(json)]
    pub default_roles: Value,
    /// Principal kind discriminant: `Human` or `Workload`.
    pub principal_kind: String,
    /// JWKS key cache TTL in seconds.
    pub jwks_ttl_secs: i64,
    /// Encrypted client secret: 12-byte AES-GCM nonce prepended to ciphertext.
    ///
    /// `NULL` for `PrivateKeyJwt` and `Public` variants. Returned byte-for-byte;
    /// decryption is 04's responsibility.
    pub client_secret_enc: Option<Vec<u8>>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Row update timestamp.
    pub updated_at: DateTime<Utc>,
}
