//! Tenant-admin CRUD contracts for trusted OIDC issuers and workload bindings.
//!
//! These are the public HTTP request/response shapes for
//! `POST/GET/DELETE /v1/admin/trusted-issuers` and `/v1/admin/workload-bindings`.
//! They are plain data: every mapping to or from the server-side domain and
//! row types lives in `wyrd-server`, so this module stays pure contract and is
//! reusable by the CLI and SDK without depending on the server.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::IssuerUrl;
use crate::reference::CardRef;

/// Serde mirror of the domain claim mapping, whose claim paths are not directly
/// (de)serializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ClaimMappingPayload {
    /// Claim path that yields the principal subject.
    pub subject: String,
    /// Optional claim path that yields the principal email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Optional claim path that yields the principal groups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<String>,
}

/// How Wyrd authenticates to the IdP token endpoint, as authored by the admin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ClientAuthKind {
    /// HTTP Basic with the client secret.
    SecretBasic,
    /// Form-post body with the client secret.
    SecretPost,
    /// Private-key JWT client assertion.
    PrivateKeyJwt,
    /// Public client with no credential.
    Public,
}

/// Whether an issuer's tokens represent human users or machine workloads.
///
/// This is the issuer token policy — distinct from [`super::PrincipalKindTag`], which
/// discriminates the resulting principal identity. One canonical type backs the
/// admin wire body, the OIDC domain registry, and the server config DTO.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssuerTokenPolicy {
    /// Tokens represent human users.
    #[default]
    Human,
    /// Tokens represent machine workloads.
    Workload,
}

/// `POST /v1/admin/trusted-issuers` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateTrustedIssuerRequest {
    /// Trusted OIDC issuer URL.
    pub issuer: IssuerUrl,
    /// Expected audience for tokens from this issuer.
    pub expected_audience: String,
    /// Client id Wyrd presents to the issuer.
    pub client_id: String,
    /// How Wyrd authenticates to the issuer token endpoint.
    pub client_auth: ClientAuthKind,
    /// Client secret, required for the secret-bearing client-auth variants.
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Claim-to-principal mapping.
    pub claim_mapping: ClaimMappingPayload,
    /// Mapping from issuer group to Wyrd roles.
    #[serde(default)]
    pub group_role_map: HashMap<String, Vec<String>>,
    /// Roles granted to every principal from this issuer.
    #[serde(default)]
    pub default_roles: Vec<String>,
    /// Whether tokens represent humans or workloads.
    pub principal_kind: IssuerTokenPolicy,
    /// Optional JWKS key-cache TTL override in seconds.
    #[serde(default)]
    pub jwks_ttl_secs: Option<u64>,
}

/// Redacted issuer projection. Never carries the client secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct TrustedIssuerView {
    /// Normalized issuer URL.
    pub issuer: String,
    /// Resolved JWKS URI.
    pub jwks_uri: String,
    /// Expected audience for tokens from this issuer.
    pub expected_audience: String,
    /// Client id Wyrd presents to the issuer.
    pub client_id: String,
    /// Client-auth method, rendered as a string.
    pub client_auth: String,
    /// Principal kind, rendered as a string.
    pub principal_kind: String,
    /// JWKS key-cache TTL in seconds.
    pub jwks_ttl_secs: i64,
    /// Claim-to-principal mapping, as stored JSON.
    pub claim_mapping: Value,
    /// Group-to-roles mapping, as stored JSON.
    pub group_role_map: Value,
    /// Default roles, as stored JSON.
    pub default_roles: Value,
}

/// `POST /v1/admin/workload-bindings` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateWorkloadBindingRequest {
    /// Trusted issuer the binding belongs to.
    pub issuer: IssuerUrl,
    /// Token subject the binding matches.
    pub subject: String,
    /// Optional audience constraint.
    #[serde(default)]
    pub audience: Option<String>,
    /// Card the bound workload acts as.
    pub card_ref: CardRef,
}

/// Workload-binding projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkloadBindingView {
    /// Normalized issuer URL.
    pub issuer: String,
    /// Token subject the binding matches.
    pub subject: String,
    /// Optional audience constraint.
    pub audience: Option<String>,
    /// Card the bound workload acts as, as stored JSON.
    pub card_ref: Value,
}
