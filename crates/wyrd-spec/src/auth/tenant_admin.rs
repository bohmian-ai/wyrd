//! Wire contract for tenant provisioning on the platform control plane.

use serde::{Deserialize, Serialize};

use crate::DataTenantId;
use crate::auth::{PrincipalId, SecretBearer};
use crate::ids::TenantSlug;

/// Request to provision a new tenant.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateTenantRequest {
    /// URL-safe tenant identifier, unique across the deployment.
    pub slug: TenantSlug,
    /// Human-readable tenant name.
    pub display_name: String,
}

/// The tenant that was provisioned.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ProvisionedTenant {
    /// Server-assigned tenant isolation key.
    pub id: DataTenantId,
    /// URL-safe tenant identifier.
    pub slug: TenantSlug,
    /// Human-readable tenant name.
    pub display_name: String,
    /// Lifecycle state. `active` once the tenant is usable.
    pub status: String,
}

/// The tenant's administrative principal and its one-time credential.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ProvisionedTenantAdmin {
    /// Durable principal id. It survives credential rotation and loss.
    pub principal_id: PrincipalId,
    /// The credential, returned exactly once and never retrievable again.
    #[schemars(with = "String")]
    pub credential: SecretBearer,
}

/// Result of provisioning a tenant.
///
/// Carries the administrative credential exactly once. The server stores only
/// its verifier, so a caller that discards this response must recover through
/// the platform plane rather than by reading it back.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateTenantResponse {
    /// The provisioned tenant.
    pub tenant: ProvisionedTenant,
    /// Its administrative principal and one-time credential.
    pub admin: ProvisionedTenantAdmin,
}
