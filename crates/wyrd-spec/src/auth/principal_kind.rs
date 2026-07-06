//! Principal-kind discriminator contract.
//!
//! [`PrincipalKindTag`] is the canonical **bare discriminator** — the kind label
//! (`user` / `service` / `agent`) without a bound card. It is the single wire
//! encoding for every site that identifies a principal kind: the revoke-by-id
//! request body, access- and refresh-token claims, the revocation NOTIFY
//! channel, revocation-epoch lookups, the audit event, and the CLI.
//!
//! The **data-bearing** identity (the kind plus its bound `card_ref` and
//! transitive `card_ref_scope`) is a runtime concept and lives on
//! `wyrd_runtime::principal::PrincipalKind`, which projects to this tag via its
//! `tag()` method. It is intentionally not duplicated here: `wyrd-spec` owns the
//! wire discriminator, not the in-memory authorization payload.

use serde::{Deserialize, Serialize};

/// Principal-kind discriminator without a bound card.
///
/// Serializes as a bare snake_case string (`"user"`, `"service"`, `"agent"`);
/// this is a stable wire contract for the revoke request body, token claims, and
/// the revocation NOTIFY channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKindTag {
    /// Human user principal.
    User,
    /// Service principal.
    Service,
    /// Agent principal.
    Agent,
}

impl PrincipalKindTag {
    /// Stable lowercase label, matching the serde representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Service => "service",
            Self::Agent => "agent",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrincipalKindTag;

    #[test]
    fn tag_labels_match_serde() {
        assert_eq!(PrincipalKindTag::User.as_str(), "user");
        assert_eq!(PrincipalKindTag::Service.as_str(), "service");
        assert_eq!(PrincipalKindTag::Agent.as_str(), "agent");
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
    }
}
