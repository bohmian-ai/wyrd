//! Principal kind identity contract.
//!
//! Two related-but-distinct types live here so the rest of the platform never
//! reinvents them:
//!
//! - [`PrincipalKind`] is the **data-bearing identity**. A Service or Agent
//!   principal is inseparable from the card it is bound to, so `card_ref` is a
//!   required field of the variant — never an `Option`. This is the shape stored
//!   on the runtime `Principal`, projected into `PrincipalRef`, and embedded
//!   (nested) in JWT access-token claims.
//! - [`PrincipalKindTag`] is the **bare discriminator** — the kind label without
//!   a card. It exists for the sites that identify a principal kind but have no
//!   card to name: the revoke-by-id request body, refresh-token claims, the
//!   revocation NOTIFY channel, and revocation-epoch lookups.

use serde::{Deserialize, Serialize};

use crate::envelope::CardKind;
use crate::reference::CardRef;

/// Kind of authenticated identity, carrying the bound card for card-backed kinds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PrincipalKind {
    /// Human user identity.
    User,
    /// Card-bound service identity.
    Service {
        /// Bound Service card.
        card_ref: CardRef,
    },
    /// Card-bound agent identity.
    Agent {
        /// Bound Agent card.
        card_ref: CardRef,
    },
}

impl PrincipalKind {
    /// Returns the bound card ref for Service and Agent principals.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Service { card_ref } | Self::Agent { card_ref } => Some(card_ref),
            Self::User => None,
        }
    }

    /// Projects to the card-free discriminator.
    #[must_use]
    pub fn tag(&self) -> PrincipalKindTag {
        match self {
            Self::User => PrincipalKindTag::User,
            Self::Service { .. } => PrincipalKindTag::Service,
            Self::Agent { .. } => PrincipalKindTag::Agent,
        }
    }

    /// Validates that the bound card's kind matches the principal kind.
    ///
    /// A signed token is trusted, but this enforces defense-in-depth: a Service
    /// principal must carry a Service card and an Agent principal an Agent card.
    ///
    /// # Errors
    /// Returns [`CardKindMismatch`] when the bound card kind does not match.
    pub fn validate_card_kind(&self) -> Result<(), CardKindMismatch> {
        let ok = match self {
            Self::User => true,
            Self::Service { card_ref } => card_ref.kind == CardKind::Service,
            Self::Agent { card_ref } => card_ref.kind == CardKind::Agent,
        };
        if ok {
            Ok(())
        } else {
            Err(CardKindMismatch)
        }
    }
}

/// The bound card's kind does not match the principal kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("bound card kind does not match principal kind")]
pub struct CardKindMismatch;

/// Principal-kind discriminator without a bound card.
///
/// Serializes as a bare snake_case string (`"user"`, `"service"`, `"agent"`);
/// this is a stable wire contract for the revoke request body and the
/// revocation NOTIFY channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

impl From<&PrincipalKind> for PrincipalKindTag {
    fn from(kind: &PrincipalKind) -> Self {
        kind.tag()
    }
}

#[cfg(test)]
mod tests {
    use super::{CardKindMismatch, PrincipalKind, PrincipalKindTag};
    use crate::envelope::CardKind;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn card(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("billing").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    #[test]
    fn service_kind_nests_card_ref_on_the_wire() {
        let kind = PrincipalKind::Service {
            card_ref: card(CardKind::Service),
        };

        let value = serde_json::to_value(&kind).expect("serializes");

        assert_eq!(value["kind"], "service");
        assert!(value["card_ref"].is_object());
        assert_eq!(
            serde_json::from_value::<PrincipalKind>(value).expect("deserializes"),
            kind
        );
    }

    #[test]
    fn user_kind_has_no_card() {
        let value = serde_json::to_value(PrincipalKind::User).expect("serializes");

        assert_eq!(value["kind"], "user");
        assert!(value.get("card_ref").is_none());
    }

    #[test]
    fn unknown_fields_in_card_variant_are_rejected() {
        let mut value = serde_json::to_value(PrincipalKind::Service {
            card_ref: card(CardKind::Service),
        })
        .expect("serializes");
        value["extra"] = serde_json::json!(true);

        assert!(serde_json::from_value::<PrincipalKind>(value).is_err());
    }

    #[test]
    fn tag_projects_and_labels() {
        let kind = PrincipalKind::Agent {
            card_ref: card(CardKind::Agent),
        };

        assert_eq!(kind.tag(), PrincipalKindTag::Agent);
        assert_eq!(PrincipalKindTag::from(&kind).as_str(), "agent");
    }

    #[test]
    fn tag_serializes_as_bare_string() {
        assert_eq!(
            serde_json::to_value(PrincipalKindTag::Service).expect("serializes"),
            serde_json::json!("service")
        );
    }

    #[test]
    fn validate_card_kind_rejects_mismatch() {
        let mismatched = PrincipalKind::Service {
            card_ref: card(CardKind::Agent),
        };

        assert_eq!(mismatched.validate_card_kind(), Err(CardKindMismatch));
        assert!(
            PrincipalKind::Service {
                card_ref: card(CardKind::Service),
            }
            .validate_card_kind()
            .is_ok()
        );
    }
}
