//! Builtin RBAC roles shared across Wyrd server surfaces.

use uuid::Uuid;
use wyrd_spec::DataTenantId;

use crate::permission::Permission;

/// Fixed namespace for deterministic per-tenant builtin role IDs.
pub const NS_BUILTIN_ROLE: Uuid = Uuid::from_u128(0x6ad8_2377_3a8f_5f42_9d17_a9f5_5b1c_5c63);

/// A builtin role definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinRole {
    /// Stable role name referenced by JWT role claims.
    pub name: &'static str,
    /// Permissions seeded into `wyrd.auth_roles.permissions`.
    pub permissions: &'static [Permission],
}

/// Every read a tenant principal can be granted: Cards, artifacts, audit,
/// Operators, gateway configuration and captured payloads, Bifrost table
/// definitions, and Bifrost queries.
const VIEWER: &[Permission] = &[
    Permission::card_read(),
    Permission::artifact_read(),
    Permission::audit_read(),
    Permission::operators_read(),
    Permission::gateway_read(),
    Permission::gateway_payload_read(),
    Permission::bifrost_table_read(),
    Permission::bifrost_query_read(),
];

/// [`VIEWER`] plus every runtime write a running Service or Agent performs:
/// Bifrost table and record writes, evaluation, Workflow execution, Trigger
/// writes, Operator invocation, and gateway invocation on every model.
const WORKLOAD: &[Permission] = &[
    Permission::card_read(),
    Permission::artifact_read(),
    Permission::audit_read(),
    Permission::operators_read(),
    Permission::gateway_read(),
    Permission::gateway_payload_read(),
    Permission::bifrost_table_read(),
    Permission::bifrost_query_read(),
    Permission::bifrost_table_write(),
    Permission::bifrost_record_write(),
    Permission::eval_run(),
    Permission::workflow_run(),
    Permission::trigger_write(),
    Permission::operator_invoke(),
    Permission::gateway_invoke_any(),
];

/// [`WORKLOAD`] plus authoring: Card and artifact writes, Card deletion, policy
/// lock, and Service installation. Changing what a workload is verified
/// against belongs to people, not to the workload itself.
const EDITOR: &[Permission] = &[
    Permission::card_read(),
    Permission::artifact_read(),
    Permission::audit_read(),
    Permission::operators_read(),
    Permission::gateway_read(),
    Permission::gateway_payload_read(),
    Permission::bifrost_table_read(),
    Permission::bifrost_query_read(),
    Permission::bifrost_table_write(),
    Permission::bifrost_record_write(),
    Permission::eval_run(),
    Permission::workflow_run(),
    Permission::trigger_write(),
    Permission::operator_invoke(),
    Permission::gateway_invoke_any(),
    Permission::card_write(),
    Permission::card_delete(),
    Permission::artifact_write(),
    Permission::policy_lock(),
    Permission::service_install(),
];

/// Source of truth for Wyrd's per-tenant builtin roles.
///
/// Exactly four Roles ordered `viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`.
/// Only `admin` (through `*`) holds credential, user, identity-connection,
/// Operator-secret, and gateway-configuration administration. Platform-plane
/// and engine-peer permissions belong to no tenant Role. Narrower Roles are a
/// tenant administrator's choice.
pub const BUILTIN_ROLES: &[BuiltinRole] = &[
    BuiltinRole {
        name: "admin",
        permissions: &[Permission::wildcard()],
    },
    BuiltinRole {
        name: "editor",
        permissions: EDITOR,
    },
    BuiltinRole {
        name: DEFAULT_CARD_ROLE,
        permissions: WORKLOAD,
    },
    BuiltinRole {
        name: "viewer",
        permissions: VIEWER,
    },
];

/// Built-in Role a Card-bound Service or Agent principal receives at its first
/// projection.
///
/// The grant is a direct assignment made once: an administrator who revokes
/// it is not overridden by a later re-registration. The principal's Card
/// scope still bounds which Cards it may attribute evidence to.
pub const DEFAULT_CARD_ROLE: &str = "workload";

/// Deterministic UUID for a tenant-scoped builtin role row.
#[must_use]
pub fn builtin_role_uuid(data_tenant_id: DataTenantId, role_name: &str) -> Uuid {
    Uuid::new_v5(
        &NS_BUILTIN_ROLE,
        format!("{data_tenant_id}/{role_name}").as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{BUILTIN_ROLES, DEFAULT_CARD_ROLE, builtin_role_uuid};
    use crate::Permission;

    /// Returns the named built-in Role's permissions.
    ///
    /// # Panics
    ///
    /// Panics when `name` is not built in.
    fn permissions(name: &str) -> &'static [Permission] {
        BUILTIN_ROLES
            .iter()
            .find(|role| role.name == name)
            .expect("role is built in")
            .permissions
    }

    /// True when the named Role covers `required`.
    fn holds(name: &str, required: &Permission) -> bool {
        permissions(name)
            .iter()
            .any(|permission| permission.covers(required))
    }

    /// Proves every seeded permission survives its JSON round trip.
    ///
    /// # Panics
    ///
    /// Panics when a permission fails to serialize or decode identically.
    #[test]
    fn all_seeds_are_valid_permissions() {
        for role in BUILTIN_ROLES {
            let value = serde_json::to_value(role.permissions).expect("permissions serialize");
            let round_trip: Vec<Permission> =
                serde_json::from_value(value).expect("permissions deserialize");
            assert_eq!(round_trip, role.permissions);
        }
    }

    /// Proves Wyrd ships exactly the four persona Roles and no retired name.
    ///
    /// # Panics
    ///
    /// Panics when the built-in name set differs.
    #[test]
    fn exactly_four_roles_are_built_in() {
        let names = BUILTIN_ROLES
            .iter()
            .map(|role| role.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            names,
            BTreeSet::from(["admin", "editor", "viewer", "workload"])
        );
        assert_eq!(DEFAULT_CARD_ROLE, "workload");
    }

    /// Proves `viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`, each strictly.
    ///
    /// # Panics
    ///
    /// Panics when a lower Role holds a permission its successor does not, or
    /// when two adjacent Roles are equal.
    #[test]
    fn roles_are_strictly_nested() {
        for (lower, upper) in [
            ("viewer", "workload"),
            ("workload", "editor"),
            ("editor", "admin"),
        ] {
            for permission in permissions(lower) {
                assert!(holds(upper, permission), "{upper} lacks {permission}");
            }
            assert!(
                permissions(upper)
                    .iter()
                    .any(|permission| !holds(lower, permission)),
                "{upper} adds nothing to {lower}"
            );
        }
    }

    /// Proves each non-admin Role's exact permission set.
    ///
    /// # Panics
    ///
    /// Panics when a Role's seeded permissions differ from the specified set.
    #[test]
    fn each_role_holds_its_exact_permissions() {
        let viewer = vec![
            Permission::card_read(),
            Permission::artifact_read(),
            Permission::audit_read(),
            Permission::operators_read(),
            Permission::gateway_read(),
            Permission::gateway_payload_read(),
            Permission::bifrost_table_read(),
            Permission::bifrost_query_read(),
        ];
        let mut workload = viewer.clone();
        workload.extend([
            Permission::bifrost_table_write(),
            Permission::bifrost_record_write(),
            Permission::eval_run(),
            Permission::workflow_run(),
            Permission::trigger_write(),
            Permission::operator_invoke(),
            Permission::gateway_invoke_any(),
        ]);
        let mut editor = workload.clone();
        editor.extend([
            Permission::card_write(),
            Permission::card_delete(),
            Permission::artifact_write(),
            Permission::policy_lock(),
            Permission::service_install(),
        ]);
        assert_eq!(permissions("viewer"), viewer.as_slice());
        assert_eq!(permissions("workload"), workload.as_slice());
        assert_eq!(permissions("editor"), editor.as_slice());
        assert_eq!(permissions("admin"), [Permission::wildcard()].as_slice());
    }

    /// Proves tenant administration, platform-plane, and engine-peer
    /// permissions are reachable only through `admin`'s wildcard, and that
    /// `workload` cannot author Cards or artifacts.
    ///
    /// # Panics
    ///
    /// Panics when a non-admin Role covers an administrative permission.
    #[test]
    fn only_admin_holds_administration() {
        for required in [
            Permission::service_accounts_write(),
            Permission::identity_connections_write(),
            Permission::users_manage(),
            Permission::operators_write(),
            Permission::gateway_write(),
            Permission::gateway_delete(),
            Permission::bifrost_peer_invoke(),
            Permission::bifrost_table_install(),
            Permission::tenant_create(),
        ] {
            let granting = BUILTIN_ROLES
                .iter()
                .filter(|role| holds(role.name, &required))
                .map(|role| role.name)
                .collect::<Vec<_>>();
            assert_eq!(granting, vec!["admin"], "{required}");
        }
        assert!(!holds("workload", &Permission::card_write()));
        assert!(!holds("workload", &Permission::artifact_write()));
    }

    /// Proves deterministic role ids are stable per tenant and name.
    ///
    /// # Panics
    ///
    /// Panics when the same input yields different ids or different names
    /// collide.
    #[test]
    fn role_uuid_is_stable_per_tenant_and_name() {
        let tenant = wyrd_spec::DataTenantId::new_v7();

        assert_eq!(
            builtin_role_uuid(tenant, "admin"),
            builtin_role_uuid(tenant, "admin")
        );
        assert_ne!(
            builtin_role_uuid(tenant, "admin"),
            builtin_role_uuid(tenant, "viewer")
        );
    }
}
