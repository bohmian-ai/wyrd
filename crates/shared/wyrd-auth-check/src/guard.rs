//! Delegated-token guard for authz-check.

use wyrd_runtime::{PrincipalKind, PrincipalRef};

/// Delegated-token guard outcome.
#[derive(Debug, PartialEq, Eq)]
pub enum GuardOutcome {
    /// The token is eligible for authz-check policy evaluation.
    Allow,
    /// The token is not eligible, with a stable reason.
    Reject(GuardReason),
}

/// Stable delegated-token guard rejection reason.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GuardReason {
    /// The current actor's kind is neither Service nor Agent.
    KindNotEligible,
    /// The current actor lacks a card reference.
    CardRefMissing,
    /// The token is direct and does not carry an actor chain.
    ChainEmpty,
}

impl GuardReason {
    /// Stable reason slug for problem JSON details.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::KindNotEligible => "kind_not_eligible",
            Self::CardRefMissing => "card_ref_missing",
            Self::ChainEmpty => "chain_empty",
        }
    }
}

/// Authoritative delegated-token guard for `/v1/authz/check`.
///
/// `actor` is the token's current actor — the last verified `act` layer, or
/// `None` for a direct token. Only a Card-bound Service or Agent actor may ask
/// whether its call on the subject's behalf is permitted.
#[must_use]
pub fn guard_reason(actor: Option<&PrincipalRef>) -> GuardOutcome {
    let Some(actor) = actor else {
        return GuardOutcome::Reject(GuardReason::ChainEmpty);
    };
    if !matches!(
        actor.kind,
        PrincipalKind::Service { .. } | PrincipalKind::Agent { .. }
    ) {
        return GuardOutcome::Reject(GuardReason::KindNotEligible);
    }
    if actor.card_ref().is_none() {
        return GuardOutcome::Reject(GuardReason::CardRefMissing);
    }
    GuardOutcome::Allow
}

#[cfg(test)]
mod tests {
    use wyrd_runtime::{PrincipalId, PrincipalKind, PrincipalRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::{GuardOutcome, GuardReason, guard_reason};

    /// A direct token has no actor.
    #[test]
    fn direct_token_reports_chain_empty() {
        assert_eq!(
            guard_reason(None),
            GuardOutcome::Reject(GuardReason::ChainEmpty)
        );
    }

    /// A human actor is not eligible.
    #[test]
    fn user_actor_is_not_eligible() {
        assert_eq!(
            guard_reason(Some(&actor(PrincipalKind::User))),
            GuardOutcome::Reject(GuardReason::KindNotEligible)
        );
    }

    /// A Card-free Service actor has no Card to evaluate policy against.
    #[test]
    fn card_free_service_actor_reports_card_ref_missing() {
        let kind = PrincipalKind::Service {
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        };
        assert_eq!(
            guard_reason(Some(&actor(kind))),
            GuardOutcome::Reject(GuardReason::CardRefMissing)
        );
    }

    /// Card-bound Service and Agent actors are allowed.
    #[test]
    fn card_bound_service_and_agent_actors_are_allowed() {
        for card_kind in [CardKind::Service, CardKind::Agent] {
            let card_ref = card_ref(card_kind.clone());
            let kind = if card_kind == CardKind::Service {
                PrincipalKind::Service {
                    card_ref: Some(card_ref.clone()),
                    card_ref_scope: CardRefScope::own(&card_ref),
                }
            } else {
                PrincipalKind::Agent {
                    card_ref: card_ref.clone(),
                    card_ref_scope: CardRefScope::own(&card_ref),
                }
            };
            assert_eq!(guard_reason(Some(&actor(kind))), GuardOutcome::Allow);
        }
    }

    /// Reason slugs are a stable wire projection.
    #[test]
    fn reason_slugs_are_locked() {
        assert_eq!(GuardReason::KindNotEligible.as_str(), "kind_not_eligible");
        assert_eq!(GuardReason::CardRefMissing.as_str(), "card_ref_missing");
        assert_eq!(GuardReason::ChainEmpty.as_str(), "chain_empty");
    }

    /// Build an actor reference of `kind`.
    fn actor(kind: PrincipalKind) -> PrincipalRef {
        PrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind,
        }
    }

    /// Build a static Card reference of `kind`.
    fn card_ref(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("actor").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }
}
