use wyrd_auth_check::guard::{GuardOutcome, GuardReason, guard_reason};
use wyrd_runtime::{CardScope, PermissionSet, Principal, PrincipalId, PrincipalKind};
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
        space: SpaceName::new("prod").expect("static space is valid"),
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
        card_scope: CardScope::default(),
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
