//! Principal-kind discriminator contract.
//!
//! [`PrincipalKindTag`] is the canonical **bare discriminator** — the kind label
//! (`global_admin` / `tenant_admin` / `user` / `service` / `system`). It is the single wire
//! encoding for every site that identifies a principal kind: the revoke-by-id
//! request body, access- and refresh-token claims, the audit event, and the
//! CLI.
//!
//! A principal kind names an account, never an execution root: the exact
//! Service version a request runs as is separate workload context.
//! The tenant-plane runtime projection is
//! `wyrd_runtime::principal::PrincipalKind`, which maps to this tag through
//! its `tag()` method.

use serde::{Deserialize, Serialize};

/// Principal-kind discriminator.
///
/// Serializes as a bare snake_case string (`"global_admin"`, `"tenant_admin"`,
/// `"user"`, `"service"`, `"system"`); this is a stable wire
/// contract for the revoke request body, token claims, and the durable
/// `principal_kind` columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKindTag {
    /// Platform-scope administrative principal. Carries no tenant and operates
    /// the platform control plane: tenant lifecycle and tenant-administration
    /// recovery. It is never implicitly authorized over a tenant's resources.
    GlobalAdmin,
    /// Tenant-scope administrative principal created during tenant
    /// provisioning. It is the tenant's headless root of trust.
    TenantAdmin,
    /// Human user principal.
    User,
    /// Stable nonhuman account for Services, Agents, CI, and automation.
    Service,
    /// Internal tenant verification-result writer.
    ///
    /// Exactly one credentialless principal of this kind exists per tenant,
    /// created by tenant provisioning. It holds no role, is absent from every
    /// public principal-management and credential path, and never appears in
    /// a token. It stays representable here so audit records can attribute
    /// internal execution to it.
    System,
}

impl PrincipalKindTag {
    /// Stable lowercase label, matching the serde representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GlobalAdmin => "global_admin",
            Self::TenantAdmin => "tenant_admin",
            Self::User => "user",
            Self::Service => "service",
            Self::System => "system",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrincipalKindTag;

    #[test]
    fn tag_labels_match_serde() {
        assert_eq!(PrincipalKindTag::GlobalAdmin.as_str(), "global_admin");
        assert_eq!(PrincipalKindTag::TenantAdmin.as_str(), "tenant_admin");
        assert_eq!(PrincipalKindTag::User.as_str(), "user");
        assert_eq!(PrincipalKindTag::Service.as_str(), "service");
        assert_eq!(PrincipalKindTag::System.as_str(), "system");
    }

    /// The internal verification-result writer is the fifth wire value and
    /// round-trips as the bare `system` label in token and audit contracts.
    ///
    /// # Panics
    /// Panics when the tag does not serialize or deserialize as `"system"`.
    #[test]
    fn system_tag_round_trips_as_bare_string() {
        assert_eq!(
            serde_json::to_value(PrincipalKindTag::System).expect("serializes"),
            serde_json::json!("system")
        );
        assert_eq!(
            serde_json::from_value::<PrincipalKindTag>(serde_json::json!("system"))
                .expect("deserializes"),
            PrincipalKindTag::System
        );
    }

    #[test]
    fn tag_serializes_as_bare_string() {
        assert_eq!(
            serde_json::to_value(PrincipalKindTag::Service).expect("serializes"),
            serde_json::json!("service")
        );
        assert!(
            serde_json::from_value::<PrincipalKindTag>(serde_json::json!("agent")).is_err(),
            "the removed Agent account kind is not a wire value"
        );
        assert_eq!(
            serde_json::to_value(PrincipalKindTag::GlobalAdmin).expect("serializes"),
            serde_json::json!("global_admin")
        );
    }
}
