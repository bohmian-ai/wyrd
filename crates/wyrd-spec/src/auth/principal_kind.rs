//! Principal-kind discriminator contract.
//!
//! [`PrincipalKindTag`] is the canonical **bare discriminator** — the kind label
//! (`global_admin` / `tenant_admin` / `user` / `service` / `agent` /
//! `system`) without a bound card. It is the single wire
//! encoding for every site that identifies a principal kind: the revoke-by-id
//! request body, access- and refresh-token claims, the audit event, and the
//! CLI.
//!
//! The **data-bearing** identity (the kind plus its bound `card_ref` and
//! transitive `card_ref_scope`) is a runtime concept and lives on
//! `wyrd_runtime::principal::PrincipalKind`, which projects to this tag via its
//! `tag()` method. It is intentionally not duplicated here: `wyrd-spec` owns the
//! wire discriminator, not the in-memory authorization payload.

use serde::{Deserialize, Serialize};

/// Principal-kind discriminator without a bound card.
///
/// Serializes as a bare snake_case string (`"global_admin"`, `"tenant_admin"`,
/// `"user"`, `"service"`, `"agent"`, `"system"`); this is a stable wire
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
    /// provisioning. It is the tenant's headless root of trust and is not bound
    /// to a Card.
    TenantAdmin,
    /// Human user principal.
    User,
    /// Service principal.
    Service,
    /// Agent principal.
    Agent,
    /// Internal tenant verification-result writer.
    ///
    /// Exactly one credentialless principal of this kind exists per tenant,
    /// created by tenant provisioning. It binds no Card, holds no role, and is
    /// absent from every public principal-management and credential path; the
    /// server alone mints its short-lived token, scoped to one exact Verifier
    /// Card, to publish canonical verification results. It stays representable
    /// here so token claims and audit records can attribute those writes.
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
            Self::Agent => "agent",
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
        assert_eq!(PrincipalKindTag::Agent.as_str(), "agent");
        assert_eq!(PrincipalKindTag::System.as_str(), "system");
    }

    /// The internal verification-result writer is the sixth wire value and
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
        assert_eq!(
            serde_json::from_value::<PrincipalKindTag>(serde_json::json!("agent"))
                .expect("deserializes"),
            PrincipalKindTag::Agent
        );
        assert_eq!(
            serde_json::to_value(PrincipalKindTag::GlobalAdmin).expect("serializes"),
            serde_json::json!("global_admin")
        );
    }
}
