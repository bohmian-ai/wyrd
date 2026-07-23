//! Delegated-token guard for authz-check.

use wyrd_runtime::{Principal, PrincipalKind};

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
    /// The callee principal kind is neither Service nor Agent.
    KindNotEligible,
    /// The callee principal lacks a card reference.
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
#[must_use]
pub fn guard_reason(callee: &Principal, chain_len: usize) -> GuardOutcome {
    if !matches!(
        callee.kind,
        PrincipalKind::Service { .. } | PrincipalKind::Agent { .. }
    ) {
        return GuardOutcome::Reject(GuardReason::KindNotEligible);
    }
    if callee.card_ref().is_none() {
        return GuardOutcome::Reject(GuardReason::CardRefMissing);
    }
    if chain_len == 0 {
        return GuardOutcome::Reject(GuardReason::ChainEmpty);
    }
    GuardOutcome::Allow
}

#[cfg(test)]
mod tests {
    use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::{GuardOutcome, GuardReason, guard_reason};

    #[test]
    fn user_kind_is_not_eligible_before_chain_check() {
        assert_eq!(
            guard_reason(&principal(PrincipalKind::User), 0),
            GuardOutcome::Reject(GuardReason::KindNotEligible)
        );
    }

    #[test]
    fn direct_service_token_reports_chain_empty() {
        assert_eq!(
            guard_reason(&principal(service_kind("service")), 0),
            GuardOutcome::Reject(GuardReason::ChainEmpty)
        );
    }

    #[test]
    fn delegated_agent_token_is_allowed() {
        assert_eq!(
            guard_reason(&principal(agent_kind("agent")), 1),
            GuardOutcome::Allow
        );
    }

    #[test]
    fn reason_slugs_are_locked() {
        assert_eq!(GuardReason::KindNotEligible.as_str(), "kind_not_eligible");
        assert_eq!(GuardReason::CardRefMissing.as_str(), "card_ref_missing");
        assert_eq!(GuardReason::ChainEmpty.as_str(), "chain_empty");
    }

    fn principal(kind: PrincipalKind) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind,
            tenant_id: DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        }
    }

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    /// Build a scoped service principal kind for guard tests.
    fn service_kind(name: &str) -> PrincipalKind {
        let card_ref = card_ref(CardKind::Service, name);
        PrincipalKind::Service {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }

    /// Build a scoped agent principal kind for guard tests.
    fn agent_kind(name: &str) -> PrincipalKind {
        let card_ref = card_ref(CardKind::Agent, name);
        PrincipalKind::Agent {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }
}

#[cfg(test)]
mod properties {
    use crate::guard::{GuardOutcome, GuardReason, guard_reason};
    use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn principal(kind: PrincipalKind) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind,
            tenant_id: DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        }
    }

    fn user() -> Principal {
        principal(PrincipalKind::User)
    }

    fn service() -> Principal {
        let card_ref = card_ref(CardKind::Service, "service");
        principal(PrincipalKind::Service {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        })
    }

    fn agent() -> Principal {
        let card_ref = card_ref(CardKind::Agent, "agent");
        principal(PrincipalKind::Agent {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        })
    }

    fn assert_reject(outcome: GuardOutcome, expected: GuardReason) {
        match outcome {
            GuardOutcome::Reject(actual) => assert_eq!(actual, expected),
            GuardOutcome::Allow => panic!("expected rejection {expected:?}, got allow"),
        }
    }

    fn assert_allow(outcome: GuardOutcome) {
        assert_eq!(outcome, GuardOutcome::Allow);
    }

    #[test]
    fn user_kind_rejects_regardless_of_chain() {
        for chain_len in [0, 1, 3] {
            assert_reject(
                guard_reason(&user(), chain_len),
                GuardReason::KindNotEligible,
            );
        }
    }

    #[test]
    fn service_kind_rejects_when_chain_empty() {
        assert_reject(guard_reason(&service(), 0), GuardReason::ChainEmpty);
    }

    #[test]
    fn service_kind_with_chain_allows() {
        assert_allow(guard_reason(&service(), 1));
        assert_allow(guard_reason(&service(), 5));
    }

    #[test]
    fn agent_kind_rejects_when_chain_empty() {
        assert_reject(guard_reason(&agent(), 0), GuardReason::ChainEmpty);
    }

    #[test]
    fn agent_kind_with_chain_allows() {
        assert_allow(guard_reason(&agent(), 1));
        assert_allow(guard_reason(&agent(), 5));
    }

    #[test]
    fn card_ref_missing_slug_is_card_ref_missing() {
        assert_eq!(GuardReason::CardRefMissing.as_str(), "card_ref_missing");
    }
}
