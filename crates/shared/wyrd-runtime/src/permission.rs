//! Runtime RBAC permission model.
//!
//! The vocabulary and permission values are pure contracts owned by
//! `wyrd_spec::auth`; runtime re-exports them beside the checker and the
//! request-time effective-permission view.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use wyrd_spec::auth::{
    Action, BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope, CardPermissionScope,
    GatewayAccess, Permission, PermissionParseError, PermissionScope, PermissionScopeError,
    PermissionSet, Resource, UnrepresentableIntersection,
};

/// The authority one request holds: the account's current grants, bounded by
/// every mandatory restriction layer.
///
/// `grants` is the server's cached view of the account's direct, group, and
/// provider bindings, shared by reference rather than copied per request.
/// Each ceiling — a signed token ceiling, a credential's current ceiling, a
/// delegation participant's current grants, an accepted execution's frozen
/// authority — is additive internally, and every layer must allow. A layer
/// can only remove authority; none can add what `grants` lacks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectivePermissions {
    grants: Arc<PermissionSet>,
    ceilings: Vec<Arc<PermissionSet>>,
}

impl EffectivePermissions {
    /// Authority of exactly `grants`, with no further restriction.
    #[must_use]
    pub fn new(grants: Arc<PermissionSet>) -> Self {
        Self {
            grants,
            ceilings: Vec::new(),
        }
    }

    /// The same authority bounded additionally by `ceiling`.
    #[must_use]
    pub fn restricted_by(mut self, ceiling: Arc<PermissionSet>) -> Self {
        self.ceilings.push(ceiling);
        self
    }

    /// The account's current grants, before any restriction.
    #[must_use]
    pub fn grants(&self) -> &PermissionSet {
        &self.grants
    }

    /// Every restriction layer, in the order it was attached.
    #[must_use]
    pub fn ceilings(&self) -> &[Arc<PermissionSet>] {
        &self.ceilings
    }

    /// True when the grants and every ceiling cover `required`.
    #[must_use]
    pub fn contains(&self, required: &Permission) -> bool {
        self.grants.contains(required) && self.ceilings.iter().all(|set| set.contains(required))
    }

    /// True when a grant naming specific objects covers `required` and every
    /// ceiling allows it. See [`PermissionSet::contains_object_scoped`].
    #[must_use]
    pub fn contains_object_scoped(&self, required: &Permission) -> bool {
        self.grants.contains_object_scoped(required)
            && self
                .ceilings
                .iter()
                .all(|ceiling| ceiling.contains(required))
    }

    /// True when every layer admits the operation on some object; coarse
    /// admission only, never an object decision. See
    /// [`PermissionSet::covers_operation`].
    #[must_use]
    pub fn covers_operation(&self, resource: &Resource, action: &Action) -> bool {
        self.grants.covers_operation(resource, action)
            && self
                .ceilings
                .iter()
                .all(|set| set.covers_operation(resource, action))
    }

    /// True when no layer combination could allow anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty() || self.ceilings.iter().any(|set| set.is_empty())
    }

    /// The single permission set this authority equals, for freezing it as a
    /// durable ceiling or signing it into a restricted token.
    ///
    /// # Errors
    ///
    /// Returns [`UnrepresentableIntersection`] when two layers share
    /// authority that no single set can express, so the caller refuses
    /// instead of widening.
    pub fn flatten(&self) -> Result<PermissionSet, UnrepresentableIntersection> {
        let mut flat = PermissionSet::clone(&self.grants);
        for ceiling in &self.ceilings {
            flat = flat.intersection(ceiling)?;
        }
        Ok(flat)
    }
}

impl From<PermissionSet> for EffectivePermissions {
    fn from(grants: PermissionSet) -> Self {
        Self::new(Arc::new(grants))
    }
}

/// Serialized layers of an [`EffectivePermissions`]: plain sets, never cache
/// handles.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectivePermissionsWire {
    grants: PermissionSet,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ceilings: Vec<PermissionSet>,
}

impl Serialize for EffectivePermissions {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EffectivePermissionsWire {
            grants: PermissionSet::clone(&self.grants),
            ceilings: self
                .ceilings
                .iter()
                .map(|set| PermissionSet::clone(set))
                .collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EffectivePermissions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = EffectivePermissionsWire::deserialize(deserializer)?;
        Ok(Self {
            grants: Arc::new(wire.grants),
            ceilings: wire.ceilings.into_iter().map(Arc::new).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wyrd_spec::auth::{CardPermissionScope, PermissionScope};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid};

    use super::{EffectivePermissions, Permission, PermissionSet};

    fn payments() -> PermissionScope {
        PermissionScope::Card(CardPermissionScope::family(
            CardKind::Repository,
            CardName::new("payments").expect("name"),
        ))
    }

    fn payments_v1() -> PermissionScope {
        PermissionScope::Card(CardPermissionScope::version(
            CardKind::Repository,
            CardName::new("payments").expect("name"),
            CardUid::new("01927b4c-0000-7000-8000-000000000001").expect("uid"),
        ))
    }

    /// Proves layers are conjunctive: a ceiling narrows a family grant to an
    /// exact version, an empty ceiling permits nothing, a ceiling never adds
    /// what grants lack, and flattening yields the same narrowed set.
    ///
    /// # Panics
    /// Panics when a layer widens or fails to narrow authority.
    #[test]
    fn ceilings_only_restrict_current_grants() {
        let grants: PermissionSet = [
            Permission::card_read().at(payments()),
            Permission::card_write().at(payments()),
        ]
        .into_iter()
        .collect();
        let unrestricted = EffectivePermissions::from(grants);
        assert!(unrestricted.contains(&Permission::card_write().at(payments_v1())));

        let read_v1: PermissionSet = [Permission::card_read().at(payments_v1())]
            .into_iter()
            .collect();
        let restricted = unrestricted.clone().restricted_by(Arc::new(read_v1));
        assert!(restricted.contains(&Permission::card_read().at(payments_v1())));
        assert!(!restricted.contains(&Permission::card_write().at(payments_v1())));
        assert!(!restricted.contains(&Permission::card_read().at(payments())));

        let empty = unrestricted
            .clone()
            .restricted_by(Arc::new(PermissionSet::new()));
        assert!(empty.is_empty());
        assert!(!empty.contains(&Permission::card_read().at(payments_v1())));

        let wide: PermissionSet = [Permission::wildcard()].into_iter().collect();
        let capped = EffectivePermissions::from(PermissionSet::new()).restricted_by(Arc::new(wide));
        assert!(
            !capped.contains(&Permission::card_read()),
            "a ceiling grants nothing"
        );

        assert_eq!(
            restricted.flatten().expect("representable"),
            [Permission::card_read().at(payments_v1())]
                .into_iter()
                .collect()
        );
        let json = serde_json::to_value(&restricted).expect("serializes");
        let back: EffectivePermissions = serde_json::from_value(json).expect("decodes");
        assert_eq!(back, restricted);
    }
}
