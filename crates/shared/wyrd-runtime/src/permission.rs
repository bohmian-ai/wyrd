//! Runtime RBAC permission model.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A single resource/action authorization tuple.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permission {
    /// Resource the permission applies to.
    pub resource: Resource,
    /// Action allowed on the resource.
    pub action: Action,
}

/// Resource the permission applies to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// Cards and specs.
    Cards,
    /// Service cards and deployment-facing service operations.
    Services,
    /// Operator cards and invocations.
    Operators,
    /// Eval cards and eval runs.
    Evals,
    /// Drift cards and drift observations.
    Drift,
    /// Artifact bytes and metadata.
    Artifacts,
    /// Audit records and audit cards.
    Audit,
    /// Policy cards and policy administration.
    Policy,
    /// Trigger cards.
    Triggers,
    /// Service and agent API-key principals.
    ServiceAccounts,
    /// Human user administration.
    Users,
    /// RFC 8693 token exchange and delegation.
    Delegation,
    /// One of several resources.
    AnyOf(Vec<Resource>),
    /// All resources.
    Wildcard,
}

/// Action performed against a resource.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Read.
    Read,
    /// Create or update.
    Write,
    /// Delete.
    Delete,
    /// Invoke runtime behavior.
    Invoke,
    /// Install or deploy.
    Install,
    /// Lock.
    Lock,
    /// Run.
    Run,
    /// Issue a token or delegated credential.
    Issue,
    /// One of several actions.
    AnyOf(Vec<Action>),
    /// All actions.
    Wildcard,
}

impl Resource {
    /// True if this resource covers `other`.
    #[must_use]
    pub fn covers(&self, other: &Resource) -> bool {
        match (self, other) {
            (Self::Wildcard, _) => true,
            (Self::AnyOf(resources), other) => {
                resources.iter().any(|resource| resource.covers(other))
            }
            (self_resource, other_resource) => self_resource == other_resource,
        }
    }

    fn as_str(&self) -> Option<&'static str> {
        Some(match self {
            Self::Cards => "cards",
            Self::Services => "services",
            Self::Operators => "operators",
            Self::Evals => "evals",
            Self::Drift => "drift",
            Self::Artifacts => "artifacts",
            Self::Audit => "audit",
            Self::Policy => "policy",
            Self::Triggers => "triggers",
            Self::ServiceAccounts => "service_accounts",
            Self::Users => "users",
            Self::Delegation => "delegation",
            Self::Wildcard => "wildcard",
            Self::AnyOf(_) => return None,
        })
    }
}

impl Action {
    /// True if this action covers `other`.
    #[must_use]
    pub fn covers(&self, other: &Action) -> bool {
        match (self, other) {
            (Self::Wildcard, _) => true,
            (Self::AnyOf(actions), other) => actions.iter().any(|action| action.covers(other)),
            (self_action, other_action) => self_action == other_action,
        }
    }

    fn as_str(&self) -> Option<&'static str> {
        Some(match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
            Self::Invoke => "invoke",
            Self::Install => "install",
            Self::Lock => "lock",
            Self::Run => "run",
            Self::Issue => "issue",
            Self::Wildcard => "wildcard",
            Self::AnyOf(_) => return None,
        })
    }
}

impl Permission {
    /// True if this permission covers `required`.
    #[must_use]
    pub fn covers(&self, required: &Permission) -> bool {
        self.resource.covers(&required.resource) && self.action.covers(&required.action)
    }

    /// Read cards.
    #[must_use]
    pub const fn card_read() -> Self {
        Self {
            resource: Resource::Cards,
            action: Action::Read,
        }
    }

    /// Write cards.
    #[must_use]
    pub const fn card_write() -> Self {
        Self {
            resource: Resource::Cards,
            action: Action::Write,
        }
    }

    /// Delete cards.
    #[must_use]
    pub const fn card_delete() -> Self {
        Self {
            resource: Resource::Cards,
            action: Action::Delete,
        }
    }

    /// Read artifacts.
    #[must_use]
    pub const fn artifact_read() -> Self {
        Self {
            resource: Resource::Artifacts,
            action: Action::Read,
        }
    }

    /// Write artifacts.
    #[must_use]
    pub const fn artifact_write() -> Self {
        Self {
            resource: Resource::Artifacts,
            action: Action::Write,
        }
    }

    /// Install services.
    #[must_use]
    pub const fn service_install() -> Self {
        Self {
            resource: Resource::Services,
            action: Action::Install,
        }
    }

    /// Write service accounts.
    #[must_use]
    pub const fn service_accounts_write() -> Self {
        Self {
            resource: Resource::ServiceAccounts,
            action: Action::Write,
        }
    }

    /// Invoke operators.
    #[must_use]
    pub const fn operator_invoke() -> Self {
        Self {
            resource: Resource::Operators,
            action: Action::Invoke,
        }
    }

    /// Run evals.
    #[must_use]
    pub const fn eval_run() -> Self {
        Self {
            resource: Resource::Evals,
            action: Action::Run,
        }
    }

    /// Write triggers.
    #[must_use]
    pub const fn trigger_write() -> Self {
        Self {
            resource: Resource::Triggers,
            action: Action::Write,
        }
    }

    /// Read audit.
    #[must_use]
    pub const fn audit_read() -> Self {
        Self {
            resource: Resource::Audit,
            action: Action::Read,
        }
    }

    /// Lock policy.
    #[must_use]
    pub const fn policy_lock() -> Self {
        Self {
            resource: Resource::Policy,
            action: Action::Lock,
        }
    }

    /// Manage users.
    #[must_use]
    pub const fn users_manage() -> Self {
        Self {
            resource: Resource::Users,
            action: Action::Write,
        }
    }

    /// Issue delegated tokens.
    #[must_use]
    pub const fn delegation_issue() -> Self {
        Self {
            resource: Resource::Delegation,
            action: Action::Issue,
        }
    }

    /// All permissions.
    #[must_use]
    pub const fn wildcard() -> Self {
        Self {
            resource: Resource::Wildcard,
            action: Action::Wildcard,
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resource = self.resource.as_str().ok_or(fmt::Error)?;
        let action = self.action.as_str().ok_or(fmt::Error)?;
        write!(f, "{resource}:{action}")
    }
}

/// Permission parse failure.
#[derive(Debug, thiserror::Error)]
#[error("permission must be formatted as resource:action")]
pub struct PermissionParseError;

impl FromStr for Permission {
    type Err = PermissionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((resource, action)) = value.split_once(':') else {
            return Err(PermissionParseError);
        };
        Ok(Self {
            resource: parse_resource(resource)?,
            action: parse_action(action)?,
        })
    }
}

fn parse_resource(value: &str) -> Result<Resource, PermissionParseError> {
    Ok(match value {
        "cards" => Resource::Cards,
        "services" => Resource::Services,
        "operators" => Resource::Operators,
        "evals" => Resource::Evals,
        "drift" => Resource::Drift,
        "artifacts" => Resource::Artifacts,
        "audit" => Resource::Audit,
        "policy" => Resource::Policy,
        "triggers" => Resource::Triggers,
        "service_accounts" => Resource::ServiceAccounts,
        "users" => Resource::Users,
        "delegation" => Resource::Delegation,
        "wildcard" => Resource::Wildcard,
        _ => return Err(PermissionParseError),
    })
}

fn parse_action(value: &str) -> Result<Action, PermissionParseError> {
    Ok(match value {
        "read" => Action::Read,
        "write" => Action::Write,
        "delete" => Action::Delete,
        "invoke" => Action::Invoke,
        "install" => Action::Install,
        "lock" => Action::Lock,
        "run" => Action::Run,
        "issue" => Action::Issue,
        "wildcard" => Action::Wildcard,
        _ => return Err(PermissionParseError),
    })
}

/// Subsumption-aware permission collection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PermissionSet(Vec<Permission>);

impl PermissionSet {
    /// Construct an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Insert one permission while keeping the set minimal.
    pub fn insert(&mut self, permission: Permission) {
        if self.0.iter().any(|existing| existing.covers(&permission)) {
            return;
        }
        self.0.retain(|existing| !permission.covers(existing));
        self.0.push(permission);
    }

    /// True if the set covers `required`.
    #[must_use]
    pub fn contains(&self, required: &Permission) -> bool {
        self.0.iter().any(|permission| permission.covers(required))
    }

    /// Iterate over stored permissions.
    pub fn iter(&self) -> impl Iterator<Item = &Permission> {
        self.0.iter()
    }

    /// Number of stored permissions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<Permission> for PermissionSet {
    fn from_iter<T: IntoIterator<Item = Permission>>(iter: T) -> Self {
        let mut set = Self::new();
        for permission in iter {
            set.insert(permission);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Permission, PermissionSet, Resource};

    #[test]
    fn wildcard_covers_specific_permission() {
        let set = PermissionSet::from_iter([Permission::wildcard()]);

        assert!(set.contains(&Permission::card_write()));
    }

    #[test]
    fn insert_drops_permission_covered_by_existing_wildcard() {
        let mut set = PermissionSet::from_iter([Permission::wildcard()]);
        set.insert(Permission::card_write());

        assert_eq!(set.len(), 1);
        assert!(set.contains(&Permission::card_write()));
    }

    #[test]
    fn insert_removes_entries_covered_by_new_wildcard() {
        let mut set = PermissionSet::from_iter([Permission::card_read(), Permission::card_write()]);
        set.insert(Permission::wildcard());

        assert_eq!(set.len(), 1);
        assert_eq!(set.iter().next(), Some(&Permission::wildcard()));
    }

    #[test]
    fn any_of_covers_member_resource() {
        let permission = Permission {
            resource: Resource::AnyOf(vec![Resource::Operators, Resource::Evals]),
            action: Action::Invoke,
        };

        assert!(permission.covers(&Permission::operator_invoke()));
        assert!(!permission.covers(&Permission::card_write()));
    }

    #[test]
    fn permission_wire_token_roundtrips() {
        let parsed = "cards:write"
            .parse::<Permission>()
            .expect("permission parses");

        assert_eq!(parsed, Permission::card_write());
        assert_eq!(parsed.to_string(), "cards:write");
    }
}
