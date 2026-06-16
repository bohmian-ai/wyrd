//! Authorization capability contracts.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::ids::RoleName;

/// A single control-plane capability.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum Scope {
    /// Read cards and specs.
    #[serde(rename = "card:read")]
    CardRead,
    /// Create and update cards.
    #[serde(rename = "card:write")]
    CardWrite,
    /// Register cards from a spec or lock.
    #[serde(rename = "card:register")]
    CardRegister,
    /// Author org-global policy.
    #[serde(rename = "policy:author:org")]
    PolicyAuthorOrg,
    /// Author service-local policy.
    #[serde(rename = "policy:author:service")]
    PolicyAuthorService,
    /// Invoke the deploy gate.
    #[serde(rename = "gate:run")]
    GateRun,
    /// Mint governance tokens.
    #[serde(rename = "token:issue")]
    TokenIssue,
    /// Read audit history and compliance reports.
    #[serde(rename = "audit:read")]
    AuditRead,
    /// Sign off an assessment as passing.
    #[serde(rename = "audit:signoff")]
    AuditSignoff,
    /// Query observations.
    #[serde(rename = "observation:read")]
    ObservationRead,
    /// Manage users and role assignments.
    #[serde(rename = "user:manage")]
    UserManage,
    /// Manage service accounts and API keys.
    #[serde(rename = "service_account:manage")]
    ServiceAccountManage,
}

const ALL_SCOPES: &[Scope] = &[
    Scope::CardRead,
    Scope::CardWrite,
    Scope::CardRegister,
    Scope::PolicyAuthorOrg,
    Scope::PolicyAuthorService,
    Scope::GateRun,
    Scope::TokenIssue,
    Scope::AuditRead,
    Scope::AuditSignoff,
    Scope::ObservationRead,
    Scope::UserManage,
    Scope::ServiceAccountManage,
];

impl Scope {
    /// Every scope in declaration order.
    ///
    /// This is seed data, not a wildcard authorization token.
    #[must_use]
    pub fn all() -> Vec<Scope> {
        ALL_SCOPES.to_vec()
    }

    /// Stable wire string for this scope.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::CardRead => "card:read",
            Scope::CardWrite => "card:write",
            Scope::CardRegister => "card:register",
            Scope::PolicyAuthorOrg => "policy:author:org",
            Scope::PolicyAuthorService => "policy:author:service",
            Scope::GateRun => "gate:run",
            Scope::TokenIssue => "token:issue",
            Scope::AuditRead => "audit:read",
            Scope::AuditSignoff => "audit:signoff",
            Scope::ObservationRead => "observation:read",
            Scope::UserManage => "user:manage",
            Scope::ServiceAccountManage => "service_account:manage",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failure parsing a scope token.
#[derive(Debug, thiserror::Error)]
#[error("unknown scope token: {0}")]
pub struct ScopeParseError(String);

impl FromStr for Scope {
    type Err = ScopeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "card:read" => Scope::CardRead,
            "card:write" => Scope::CardWrite,
            "card:register" => Scope::CardRegister,
            "policy:author:org" => Scope::PolicyAuthorOrg,
            "policy:author:service" => Scope::PolicyAuthorService,
            "gate:run" => Scope::GateRun,
            "token:issue" => Scope::TokenIssue,
            "audit:read" => Scope::AuditRead,
            "audit:signoff" => Scope::AuditSignoff,
            "observation:read" => Scope::ObservationRead,
            "user:manage" => Scope::UserManage,
            "service_account:manage" => Scope::ServiceAccountManage,
            other => return Err(ScopeParseError(other.to_owned())),
        })
    }
}

/// A named bundle of scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Role {
    /// Lowercase role identifier.
    pub name: RoleName,
    /// Scopes this role grants.
    pub scopes: BTreeSet<Scope>,
}

/// An authenticated subject with its resolved capability set.
///
/// `scopes` is already flattened from the subject's roles at authentication
/// time. It is never derived from `actor` at check time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Principal {
    /// Who the subject is.
    pub actor: Actor,
    /// Capabilities the subject holds for this session.
    pub scopes: BTreeSet<Scope>,
}

impl Principal {
    /// Construct from an actor and an already-resolved scope set.
    #[must_use]
    pub fn new(actor: Actor, scopes: BTreeSet<Scope>) -> Self {
        Self { actor, scopes }
    }

    /// Flatten a set of roles into a single scope set for `actor`.
    ///
    /// This is the canonical resolution step: assigned roles collapse to the
    /// union of their scopes.
    #[must_use]
    pub fn from_roles<'a>(actor: Actor, roles: impl IntoIterator<Item = &'a Role>) -> Self {
        let scopes = roles
            .into_iter()
            .flat_map(|role| role.scopes.iter().copied())
            .collect();
        Self { actor, scopes }
    }

    /// True if the principal holds `scope`.
    #[must_use]
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    /// True only if the principal holds every scope in `required`.
    #[must_use]
    pub fn has_all(&self, required: impl IntoIterator<Item = Scope>) -> bool {
        required
            .into_iter()
            .all(|scope| self.scopes.contains(&scope))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::actor::Actor;
    use crate::ids::RoleName;

    use super::{Principal, Role, Scope};

    #[test]
    fn scope_roundtrips_through_wire_string() {
        for scope in Scope::all() {
            assert_eq!(
                scope.as_str().parse::<Scope>().expect("scope parses"),
                scope
            );

            let json = serde_json::to_string(&scope).expect("scope serializes to JSON");
            assert_eq!(json, format!("\"{}\"", scope.as_str()));
            assert_eq!(
                serde_json::from_str::<Scope>(&json).expect("scope deserializes from JSON"),
                scope
            );
        }
    }

    #[test]
    fn unknown_scope_token_errors() {
        assert!("card:nuke".parse::<Scope>().is_err());
    }

    #[test]
    fn from_roles_unions_scopes() {
        let writer = Role {
            name: RoleName::new("writer").expect("role name is valid"),
            scopes: BTreeSet::from([Scope::CardRead, Scope::CardWrite]),
        };
        let auditor = Role {
            name: RoleName::new("auditor").expect("role name is valid"),
            scopes: BTreeSet::from([Scope::AuditRead]),
        };
        let principal = Principal::from_roles(test_actor(), [&writer, &auditor]);
        let expected = writer
            .scopes
            .iter()
            .chain(auditor.scopes.iter())
            .copied()
            .collect::<BTreeSet<_>>();

        assert_eq!(principal.scopes, expected);
    }

    #[test]
    fn from_roles_empty_is_no_scopes() {
        let roles = Vec::<Role>::new();
        let principal = Principal::from_roles(test_actor(), roles.iter());

        assert!(principal.scopes.is_empty());
        assert!(!principal.has_scope(Scope::CardRead));
    }

    #[test]
    fn has_scope_reflects_membership() {
        let principal = Principal::new(test_actor(), BTreeSet::from([Scope::CardRead]));

        assert!(principal.has_scope(Scope::CardRead));
        assert!(!principal.has_scope(Scope::CardWrite));
    }

    #[test]
    fn has_all_requires_every_scope() {
        let principal = Principal::new(
            test_actor(),
            BTreeSet::from([Scope::CardRead, Scope::CardWrite]),
        );

        assert!(principal.has_all([Scope::CardRead]));
        assert!(!principal.has_all([Scope::CardRead, Scope::GateRun]));
    }

    fn test_actor() -> Actor {
        Actor::Service {
            name: "test-service".to_owned(),
            client_id: "test-service".to_owned(),
        }
    }
}
