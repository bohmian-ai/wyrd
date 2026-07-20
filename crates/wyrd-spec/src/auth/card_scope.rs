//! Principal card scope: the set of cards a principal may tag data with.

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// The set of [`CardRef`]s an authenticated principal is authorized to tag data
/// with (the `card_ref` correlation column on Bifrost tables).
///
/// Resolved once at token-issue time and carried as a signed JWT claim, so the
/// hot ingest path validates a client-supplied `card_ref` against membership
/// without a registry read. Membership is exact [`CardRef`] equality — a
/// concrete `(kind, name, version, space)` reference with the same optional
/// `uid` convention on both sides — never a version range or partial-key match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct CardScope(Vec<CardRef>);

impl CardScope {
    /// Build a scope from an iterator of card references, preserving order and
    /// dropping exact duplicates.
    #[must_use]
    pub fn new(cards: impl IntoIterator<Item = CardRef>) -> Self {
        let mut ordered: Vec<CardRef> = Vec::new();
        for card in cards {
            if !ordered.contains(&card) {
                ordered.push(card);
            }
        }
        Self(ordered)
    }

    /// Returns `true` when `card_ref` is a member of this scope (exact equality).
    #[must_use]
    pub fn contains(&self, card_ref: &CardRef) -> bool {
        self.0.contains(card_ref)
    }

    /// Returns `true` when the scope authorizes no cards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of cards in the scope.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Iterate the scope's card references in order.
    pub fn iter(&self) -> impl Iterator<Item = &CardRef> {
        self.0.iter()
    }

    /// Return the most restrictive scope authorizing only cards present in both
    /// `self` and `other` (the fail-safe rule for a delegated credential).
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter(|card| other.contains(card))
                .cloned()
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::CardScope;
    use crate::envelope::CardKind;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn card(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    #[test]
    fn contains_is_exact_card_ref_equality() {
        let member = card(CardKind::Service, "billing");
        let scope = CardScope::new([member.clone()]);

        assert!(scope.contains(&member));
        assert!(!scope.contains(&card(CardKind::Service, "shipping")));
    }

    #[test]
    fn build_dedups_and_preserves_order() {
        let a = card(CardKind::Service, "billing");
        let b = card(CardKind::Model, "churn");
        let scope = CardScope::new([a.clone(), b.clone(), a.clone()]);

        assert_eq!(scope.len(), 2);
        assert_eq!(scope.iter().collect::<Vec<_>>(), vec![&a, &b]);
    }

    #[test]
    fn empty_scope_contains_nothing() {
        let scope = CardScope::default();

        assert!(scope.is_empty());
        assert!(!scope.contains(&card(CardKind::Service, "billing")));
    }

    #[test]
    fn intersection_keeps_only_shared_members() {
        let shared = card(CardKind::Service, "billing");
        let caller = CardScope::new([shared.clone(), card(CardKind::Model, "churn")]);
        let callee = CardScope::new([shared.clone(), card(CardKind::Agent, "helper")]);

        let intersection = caller.intersection(&callee);

        assert_eq!(intersection.len(), 1);
        assert!(intersection.contains(&shared));
    }

    #[test]
    fn card_scope_round_trips_through_json() {
        let scope = CardScope::new([card(CardKind::Service, "billing")]);
        let json = serde_json::to_string(&scope).expect("serializes");
        let decoded: CardScope = serde_json::from_str(&json).expect("deserializes");

        assert_eq!(decoded, scope);
    }
}
