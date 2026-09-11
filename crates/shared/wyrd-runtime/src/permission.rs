//! Runtime RBAC permission model.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
pub use wyrd_spec::auth::{
    BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope, PermissionScope,
    PermissionScopeError,
};

/// One operation/object authorization triple.
///
/// Wyrd RBAC is `(resource, action, scope)`: the first two axes name the
/// operation and the third names the objects it reaches. Scope is required, so
/// a grant that targets no particular object states [`PermissionScope::All`]
/// explicitly rather than leaving the object axis unsaid.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "PermissionWire")]
pub struct Permission {
    /// Resource the permission applies to.
    pub resource: Resource,
    /// Action allowed on the resource.
    pub action: Action,
    /// Objects of that resource the permission reaches.
    pub scope: PermissionScope,
}

/// Exact persisted and wire projection of one permission, before validation.
///
/// [`Permission`] deserializes through this shape so every decode — role JSON
/// in `wyrd.auth_roles.permissions`, a request body, a signed forwarding
/// envelope — runs the same validation. Scope has no default: a two-field
/// object is rejected rather than silently promoted to
/// [`PermissionScope::All`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionWire {
    /// Resource the permission applies to.
    resource: Resource,
    /// Action allowed on the resource.
    action: Action,
    /// Objects of that resource the permission reaches.
    scope: PermissionScope,
}

impl TryFrom<PermissionWire> for Permission {
    type Error = PermissionScopeError;

    /// Validates one decoded permission before it becomes an effective grant.
    ///
    /// # Errors
    ///
    /// Returns the scope's own identity failure, or
    /// [`PermissionScopeError::InvalidIdentifier`] with field `resource` when a
    /// Bifrost object scope is attached to a resource that owns no Bifrost
    /// object.
    fn try_from(wire: PermissionWire) -> Result<Self, Self::Error> {
        let permission = Self {
            resource: wire.resource,
            action: wire.action,
            scope: wire.scope,
        };
        permission.validate()?;
        Ok(permission)
    }
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
    /// Bifrost table definitions (DDL: create/list/describe).
    BifrostTable,
    /// Bifrost record ingest (the streaming write surface).
    BifrostRecord,
    /// Bifrost table reads (SQL/scan).
    BifrostQuery,
    /// Private Bifrost peer plane: reservation, execution, tail, and lifecycle.
    ///
    /// Role-neutral on purpose. Every peer-bearing target answers the same
    /// private services, so a Scribe and an Oracle are authorized by the same
    /// permission rather than by role-specific resources.
    BifrostPeer,
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

    /// True when this resource owns Bifrost objects a grant may be scoped to.
    ///
    /// Only the Bifrost query surface names a table. `AnyOf` and `Wildcard` are
    /// deliberately excluded: a multi-resource or wildcard grant reaches
    /// objects only through [`PermissionScope::All`].
    #[must_use]
    pub const fn accepts_bifrost_scope(&self) -> bool {
        matches!(self, Self::BifrostQuery)
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
            Self::BifrostTable => "bifrost_table",
            Self::BifrostRecord => "bifrost_record",
            Self::BifrostQuery => "bifrost_query",
            Self::BifrostPeer => "bifrost_peer",
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
        self.resource.covers(&required.resource)
            && self.action.covers(&required.action)
            && self.scope.covers(&required.scope)
    }

    /// Rejects a permission whose object scope cannot apply to its operation.
    ///
    /// Bifrost object scope is meaningful only for Bifrost query reads.
    /// A wildcard or multi-resource grant on either axis keeps object-wide
    /// reach only with [`PermissionScope::All`], which is what stops a
    /// wildcard from inheriting one table's narrow authority.
    ///
    /// # Errors
    ///
    /// Returns the scope's own identity failure, or
    /// [`PermissionScopeError::InvalidIdentifier`] with field `resource` when a
    /// Bifrost scope is attached to a resource that owns no Bifrost object, or
    /// with field `action` when it is attached to any action other than
    /// [`Action::Read`].
    pub fn validate(&self) -> Result<(), PermissionScopeError> {
        self.scope.validate()?;
        if !self.scope.is_bifrost() {
            return Ok(());
        }
        if !self.resource.accepts_bifrost_scope() {
            return Err(PermissionScopeError::InvalidIdentifier {
                field: "resource",
                value: self.resource.as_str().unwrap_or("any_of").to_owned(),
            });
        }
        if self.action != Action::Read {
            return Err(PermissionScopeError::InvalidIdentifier {
                field: "action",
                value: self.action.as_str().unwrap_or("any_of").to_owned(),
            });
        }
        Ok(())
    }

    /// Read cards.
    #[must_use]
    pub const fn card_read() -> Self {
        Self {
            resource: Resource::Cards,
            action: Action::Read,
            scope: PermissionScope::All,
        }
    }

    /// Write cards.
    #[must_use]
    pub const fn card_write() -> Self {
        Self {
            resource: Resource::Cards,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Delete cards.
    #[must_use]
    pub const fn card_delete() -> Self {
        Self {
            resource: Resource::Cards,
            action: Action::Delete,
            scope: PermissionScope::All,
        }
    }

    /// Read artifacts.
    #[must_use]
    pub const fn artifact_read() -> Self {
        Self {
            resource: Resource::Artifacts,
            action: Action::Read,
            scope: PermissionScope::All,
        }
    }

    /// Write artifacts.
    #[must_use]
    pub const fn artifact_write() -> Self {
        Self {
            resource: Resource::Artifacts,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Install services.
    #[must_use]
    pub const fn service_install() -> Self {
        Self {
            resource: Resource::Services,
            action: Action::Install,
            scope: PermissionScope::All,
        }
    }

    /// Write service accounts.
    #[must_use]
    pub const fn service_accounts_write() -> Self {
        Self {
            resource: Resource::ServiceAccounts,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Invoke operators.
    #[must_use]
    pub const fn operator_invoke() -> Self {
        Self {
            resource: Resource::Operators,
            action: Action::Invoke,
            scope: PermissionScope::All,
        }
    }

    /// Run evals.
    #[must_use]
    pub const fn eval_run() -> Self {
        Self {
            resource: Resource::Evals,
            action: Action::Run,
            scope: PermissionScope::All,
        }
    }

    /// Write triggers.
    #[must_use]
    pub const fn trigger_write() -> Self {
        Self {
            resource: Resource::Triggers,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Read audit.
    #[must_use]
    pub const fn audit_read() -> Self {
        Self {
            resource: Resource::Audit,
            action: Action::Read,
            scope: PermissionScope::All,
        }
    }

    /// Lock policy.
    #[must_use]
    pub const fn policy_lock() -> Self {
        Self {
            resource: Resource::Policy,
            action: Action::Lock,
            scope: PermissionScope::All,
        }
    }

    /// Manage users.
    #[must_use]
    pub const fn users_manage() -> Self {
        Self {
            resource: Resource::Users,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Issue delegated tokens.
    #[must_use]
    pub const fn delegation_issue() -> Self {
        Self {
            resource: Resource::Delegation,
            action: Action::Issue,
            scope: PermissionScope::All,
        }
    }

    /// Write Bifrost records (the ingest surface).
    #[must_use]
    pub const fn bifrost_record_write() -> Self {
        Self {
            resource: Resource::BifrostRecord,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Invoke the private Bifrost peer protocol.
    #[must_use]
    pub const fn bifrost_peer_invoke() -> Self {
        Self {
            resource: Resource::BifrostPeer,
            action: Action::Invoke,
            scope: PermissionScope::All,
        }
    }

    /// Read Bifrost table definitions.
    #[must_use]
    pub const fn bifrost_table_read() -> Self {
        Self {
            resource: Resource::BifrostTable,
            action: Action::Read,
            scope: PermissionScope::All,
        }
    }

    /// Write Bifrost table definitions (DDL).
    #[must_use]
    pub const fn bifrost_table_write() -> Self {
        Self {
            resource: Resource::BifrostTable,
            action: Action::Write,
            scope: PermissionScope::All,
        }
    }

    /// Install a `SystemShared` Bifrost table (cross-tenant DDL).
    #[must_use]
    pub const fn bifrost_table_install() -> Self {
        Self {
            resource: Resource::BifrostTable,
            action: Action::Install,
            scope: PermissionScope::All,
        }
    }

    /// Read Bifrost tables (SQL/scan).
    #[must_use]
    pub const fn bifrost_query_read() -> Self {
        Self {
            resource: Resource::BifrostQuery,
            action: Action::Read,
            scope: PermissionScope::All,
        }
    }

    /// All permissions.
    #[must_use]
    pub const fn wildcard() -> Self {
        Self {
            resource: Resource::Wildcard,
            action: Action::Wildcard,
            scope: PermissionScope::All,
        }
    }
}

/// Renders the permission's *operation* as `resource:action`.
///
/// The object axis is deliberately absent: this string names which operation
/// was exercised for audit text and diagnostics, and is never the authorization
/// decision. Scoped authority travels as the typed [`Permission`] and, for
/// distributed execution, inside the scoped permission digest.
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

/// Parses one `resource:action` operation token into an [`PermissionScope::All`]
/// permission.
///
/// The token language carries no object, so a parsed permission is the
/// object-wide form. Scoped grants are expressed only through the typed JSON
/// projection; there is no second string spelling for an object.
impl FromStr for Permission {
    type Err = PermissionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((resource, action)) = value.split_once(':') else {
            return Err(PermissionParseError);
        };
        Ok(Self {
            resource: parse_resource(resource)?,
            action: parse_action(action)?,
            scope: PermissionScope::All,
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
        "bifrost_table" => Resource::BifrostTable,
        "bifrost_record" => Resource::BifrostRecord,
        "bifrost_peer" => Resource::BifrostPeer,
        "bifrost_query" => Resource::BifrostQuery,
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

    /// True when the set grants `action` on `resource` under *any* object scope.
    ///
    /// This is coarse admission, not authorization: it answers "does this
    /// principal hold this capability at all", which is what a public route
    /// needs before it can resolve the objects a request actually touches. The
    /// authoritative decision is [`Self::contains`] against the resolved
    /// object, and no caller may substitute this for it.
    #[must_use]
    pub fn covers_operation(&self, resource: &Resource, action: &Action) -> bool {
        self.0.iter().any(|permission| {
            permission.resource.covers(resource) && permission.action.covers(action)
        })
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
    use super::{Action, Permission, PermissionScope, PermissionSet, Resource};
    use serde_json::json;
    use wyrd_spec::auth::{BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope};

    #[test]
    fn serde_round_trip_every_variant() {
        let resources = [
            Resource::Cards,
            Resource::Services,
            Resource::Operators,
            Resource::Evals,
            Resource::Drift,
            Resource::Artifacts,
            Resource::Audit,
            Resource::Policy,
            Resource::Triggers,
            Resource::ServiceAccounts,
            Resource::Users,
            Resource::Delegation,
            Resource::AnyOf(vec![Resource::Operators, Resource::Evals]),
            Resource::Wildcard,
        ];
        for resource in resources {
            let value = serde_json::to_value(&resource).expect("resource serializes");
            let round_trip: Resource =
                serde_json::from_value(value).expect("resource deserializes");
            assert_eq!(round_trip, resource);
        }

        let actions = [
            Action::Read,
            Action::Write,
            Action::Delete,
            Action::Invoke,
            Action::Install,
            Action::Lock,
            Action::Run,
            Action::Issue,
            Action::AnyOf(vec![Action::Read, Action::Write]),
            Action::Wildcard,
        ];
        for action in actions {
            let value = serde_json::to_value(&action).expect("action serializes");
            let round_trip: Action = serde_json::from_value(value).expect("action deserializes");
            assert_eq!(round_trip, action);
        }
    }

    #[test]
    fn bifrost_permissions_round_trip_through_wire_strings() {
        for (permission, wire) in [
            (Permission::bifrost_record_write(), "bifrost_record:write"),
            (Permission::bifrost_table_read(), "bifrost_table:read"),
            (Permission::bifrost_table_write(), "bifrost_table:write"),
            (Permission::bifrost_query_read(), "bifrost_query:read"),
            (Permission::bifrost_peer_invoke(), "bifrost_peer:invoke"),
        ] {
            assert_eq!(permission.to_string(), wire);
            assert_eq!(wire.parse::<Permission>().expect("wire parses"), permission);
            let json = serde_json::to_value(&permission).expect("serializes");
            assert_eq!(
                serde_json::from_value::<Permission>(json).expect("deserializes"),
                permission
            );
        }
    }

    #[test]
    fn bifrost_record_write_is_distinct_from_table_write() {
        assert!(!Permission::bifrost_table_write().covers(&Permission::bifrost_record_write()));
        assert!(!Permission::bifrost_record_write().covers(&Permission::bifrost_table_write()));
        assert!(Permission::wildcard().covers(&Permission::bifrost_record_write()));
    }

    #[test]
    fn covers_wildcard_resource() {
        let permission = Permission {
            resource: Resource::Wildcard,
            action: Action::Read,
            scope: PermissionScope::All,
        };

        assert!(permission.covers(&Permission::card_read()));
        assert!(!permission.covers(&Permission::card_write()));
    }

    #[test]
    fn covers_wildcard_action() {
        let permission = Permission {
            resource: Resource::Cards,
            action: Action::Wildcard,
            scope: PermissionScope::All,
        };

        assert!(permission.covers(&Permission::card_read()));
        assert!(permission.covers(&Permission::card_write()));
        assert!(!permission.covers(&Permission::artifact_write()));
    }

    #[test]
    fn covers_double_wildcard() {
        let permission = Permission::wildcard();

        assert!(permission.covers(&Permission::card_read()));
        assert!(permission.covers(&Permission::card_write()));
        assert!(permission.covers(&Permission::delegation_issue()));
    }

    #[test]
    fn covers_anyof_resource() {
        let permission = Permission {
            resource: Resource::AnyOf(vec![Resource::Operators, Resource::Evals]),
            action: Action::Invoke,
            scope: PermissionScope::All,
        };

        assert!(permission.covers(&Permission::operator_invoke()));
        assert!(permission.covers(&Permission {
            resource: Resource::Evals,
            action: Action::Invoke,
            scope: PermissionScope::All,
        }));
        assert!(!permission.covers(&Permission {
            resource: Resource::Cards,
            action: Action::Invoke,
            scope: PermissionScope::All,
        }));
    }

    #[test]
    fn delegation_issue_const_fn_round_trips() {
        let permission = Permission::delegation_issue();
        let value = serde_json::to_value(&permission).expect("permission serializes");

        assert_eq!(
            value,
            json!({"resource": "delegation", "action": "issue", "scope": "all"})
        );

        let round_trip: Permission =
            serde_json::from_value(value).expect("permission deserializes");
        assert_eq!(round_trip, permission);
    }

    #[test]
    fn permission_set_subsumption_drops_redundant() {
        let mut set = PermissionSet::from_iter([Permission::card_read()]);
        set.insert(Permission::wildcard());

        assert_eq!(set.len(), 1);
        assert_eq!(set.iter().next(), Some(&Permission::wildcard()));
    }

    #[test]
    fn permission_set_subsumption_skips_covered() {
        let mut set = PermissionSet::from_iter([Permission::wildcard()]);
        set.insert(Permission::card_read());

        assert_eq!(set.len(), 1);
        assert_eq!(set.iter().next(), Some(&Permission::wildcard()));
    }

    #[test]
    fn permission_set_contains_resolves_through_wildcard() {
        let set = PermissionSet::from_iter([Permission::wildcard()]);

        assert!(set.contains(&Permission::card_read()));
        assert!(set.contains(&Permission::delegation_issue()));
    }

    #[test]
    fn permission_set_jsonb_round_trip() {
        let permissions = vec![
            Permission::card_write(),
            Permission::card_read(),
            Permission {
                resource: Resource::AnyOf(vec![Resource::Operators, Resource::Evals]),
                action: Action::Invoke,
                scope: PermissionScope::All,
            },
            Permission::delegation_issue(),
            Permission::wildcard(),
        ];
        let value = serde_json::to_value(&permissions).expect("permissions serialize");

        assert_eq!(
            value,
            json!([
                {"resource": "cards", "action": "write", "scope": "all"},
                {"resource": "cards", "action": "read", "scope": "all"},
                {"resource": {"any_of": ["operators", "evals"]}, "action": "invoke", "scope": "all"},
                {"resource": "delegation", "action": "issue", "scope": "all"},
                {"resource": "wildcard", "action": "wildcard", "scope": "all"}
            ])
        );

        let round_trip: Vec<Permission> =
            serde_json::from_value(value).expect("permissions deserialize");
        assert_eq!(round_trip, permissions);
    }

    /// Builds the schema-scoped Bifrost query-read grant the journey seeds.
    fn logs_schema_grant() -> Permission {
        Permission {
            resource: Resource::BifrostQuery,
            action: Action::Read,
            scope: PermissionScope::Bifrost(BifrostPermissionScope::Schema(BifrostSchemaScope {
                catalog: "vala".to_owned(),
                schema: "logs".to_owned(),
            })),
        }
    }

    /// Builds the exact table-scoped Bifrost query-read requirement for one UID.
    fn table_requirement(schema: &str, uid: uuid::Uuid) -> Permission {
        Permission {
            resource: Resource::BifrostQuery,
            action: Action::Read,
            scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "vala".to_owned(),
                schema: schema.to_owned(),
                table_uid: uid,
            })),
        }
    }

    /// Proves the persisted permission JSON is exactly the approved three-field
    /// projection for `all`, schema, and table scope, in both directions.
    #[test]
    fn scoped_permission_json_matches_the_approved_projection() {
        let uid = uuid::Uuid::from_u128(0x99);
        let cases = [
            (
                Permission::bifrost_query_read(),
                json!({"resource": "bifrost_query", "action": "read", "scope": "all"}),
            ),
            (
                logs_schema_grant(),
                json!({
                    "resource": "bifrost_query",
                    "action": "read",
                    "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}
                }),
            ),
            (
                table_requirement("traces", uid),
                json!({
                    "resource": "bifrost_query",
                    "action": "read",
                    "scope": {"bifrost": {"table": {
                        "catalog": "vala",
                        "schema": "traces",
                        "table_uid": uid.to_string(),
                    }}}
                }),
            ),
        ];

        for (permission, wire) in cases {
            assert_eq!(
                serde_json::to_value(&permission).expect("permission serializes"),
                wire
            );
            assert_eq!(
                serde_json::from_value::<Permission>(wire).expect("permission deserializes"),
                permission
            );
        }
    }

    /// Proves `scope` is mandatory: the pre-scope two-field form is rejected
    /// rather than defaulted, so no compatibility decoder exists.
    #[test]
    fn scope_is_required_and_has_no_compatibility_decoder() {
        let error = serde_json::from_value::<Permission>(json!({
            "resource": "bifrost_query",
            "action": "read"
        }))
        .expect_err("the unscoped two-field form is not accepted");

        assert!(error.to_string().contains("scope"), "{error}");
    }

    /// Proves a Bifrost object scope only attaches to a resource that owns a
    /// Bifrost object; `cards` and `wildcard` both fail closed at decode.
    #[test]
    fn bifrost_scope_is_rejected_on_an_unrelated_resource() {
        for resource in ["cards", "wildcard"] {
            let error = serde_json::from_value::<Permission>(json!({
                "resource": resource,
                "action": "read",
                "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}
            }))
            .expect_err("a Bifrost object scope needs a Bifrost object resource");

            assert!(error.to_string().contains("resource"), "{error}");
        }
    }

    /// Proves Bifrost object scope is valid only for the exact `read` action.
    ///
    /// Without this, persisted or signed JSON could carry a Bifrost scope on
    /// `wildcard`, which then covers the required read and turns corrupt role
    /// JSON into effective read authority instead of a decode failure.
    #[test]
    fn bifrost_scope_is_rejected_on_a_non_read_action() {
        for action in [
            json!("write"),
            json!("wildcard"),
            json!({"any_of": ["read", "write"]}),
        ] {
            let error = serde_json::from_value::<Permission>(json!({
                "resource": "bifrost_query",
                "action": action,
                "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}
            }))
            .expect_err("a Bifrost object scope only applies to an exact read");

            assert!(error.to_string().contains("action"), "{error}");
        }

        serde_json::from_value::<Permission>(json!({
            "resource": "bifrost_query",
            "action": "read",
            "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}
        }))
        .expect("exact query read stays valid for Bifrost scope");
    }

    /// Proves a structurally valid but empty object identity is not an object:
    /// malformed identities fail at decode, never at check time.
    #[test]
    fn malformed_scope_identity_is_rejected_at_decode() {
        assert!(
            serde_json::from_value::<Permission>(json!({
                "resource": "bifrost_query",
                "action": "read",
                "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": ""}}}
            }))
            .is_err(),
            "an empty schema segment is not an object identity"
        );
    }

    /// Proves coverage requires resource, action, and scope together, and that
    /// wildcard authority stays object-wide only because its scope is `All`.
    #[test]
    fn coverage_is_three_axis() {
        let logs = uuid::Uuid::from_u128(1);
        let traces = uuid::Uuid::from_u128(2);

        // Schema scope reaches current and future tables in that exact schema.
        assert!(logs_schema_grant().covers(&table_requirement("logs", logs)));
        assert!(logs_schema_grant().covers(&table_requirement("logs", traces)));
        assert!(!logs_schema_grant().covers(&table_requirement("traces", traces)));

        // Exact table scope reaches only its own UID.
        let exact = table_requirement("traces", traces);
        assert!(exact.covers(&table_requirement("traces", traces)));
        assert!(!exact.covers(&table_requirement("traces", logs)));

        // `All` reaches every object, but only for its own resource and action.
        assert!(Permission::bifrost_query_read().covers(&table_requirement("logs", logs)));
        assert!(!Permission::bifrost_table_read().covers(&table_requirement("logs", logs)));

        // A wildcard grant is object-wide only because its scope is `All`.
        assert!(Permission::wildcard().covers(&table_requirement("logs", logs)));

        // A scoped grant never satisfies an object-wide requirement.
        assert!(!logs_schema_grant().covers(&Permission::bifrost_query_read()));
    }

    /// Proves coarse operation admission and authoritative object coverage are
    /// distinct: a scoped grant admits the operation but authorizes only its object.
    #[test]
    fn covers_operation_admits_a_scoped_grant_without_authorizing_an_object() {
        let set = PermissionSet::from_iter([logs_schema_grant()]);

        assert!(set.covers_operation(&Resource::BifrostQuery, &Action::Read));
        assert!(!set.covers_operation(&Resource::BifrostTable, &Action::Read));
        assert!(!set.contains(&Permission::bifrost_query_read()));
        assert!(set.contains(&table_requirement("logs", uuid::Uuid::from_u128(7))));
        assert!(!set.contains(&table_requirement("traces", uuid::Uuid::from_u128(7))));
    }

    /// Proves set subsumption runs on all three axes: disjoint scopes are both
    /// retained, and an `All`-scoped grant absorbs every narrower scope.
    #[test]
    fn permission_set_keeps_scopes_that_do_not_subsume_each_other() {
        let mut set = PermissionSet::from_iter([logs_schema_grant()]);
        set.insert(table_requirement("traces", uuid::Uuid::from_u128(3)));

        assert_eq!(set.len(), 2);

        set.insert(Permission::bifrost_query_read());

        assert_eq!(set.len(), 1);
        assert_eq!(set.iter().next(), Some(&Permission::bifrost_query_read()));
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
