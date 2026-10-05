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

/// Source of truth for Wyrd's per-tenant builtin roles.
pub const BUILTIN_ROLES: &[BuiltinRole] = &[
    BuiltinRole {
        name: "admin",
        permissions: &[Permission::wildcard()],
    },
    BuiltinRole {
        name: "writer",
        permissions: &[
            Permission::card_read(),
            Permission::card_write(),
            Permission::artifact_read(),
            Permission::artifact_write(),
            Permission::operator_invoke(),
            Permission::eval_run(),
            Permission::trigger_write(),
            Permission::workflow_run(),
        ],
    },
    BuiltinRole {
        name: "reader",
        permissions: &[
            Permission::card_read(),
            Permission::artifact_read(),
            Permission::audit_read(),
        ],
    },
    BuiltinRole {
        name: "agent",
        permissions: &[
            Permission::card_read(),
            Permission::card_write(),
            Permission::artifact_read(),
            Permission::artifact_write(),
            Permission::operator_invoke(),
            Permission::eval_run(),
            Permission::workflow_run(),
        ],
    },
    BuiltinRole {
        name: "runtime_admin",
        permissions: &[Permission::service_accounts_write()],
    },
    BuiltinRole {
        name: WORKLOAD_ROLE,
        permissions: &[
            Permission::bifrost_table_read(),
            Permission::bifrost_record_write(),
            Permission::bifrost_query_read(),
        ],
    },
];

/// Built-in Role a Card-bound Service or Agent principal receives at its first
/// projection.
///
/// It lets the workload emit and read back its own evidence: Bifrost table
/// read (the describe every writer performs before admission), record write,
/// and query read, nothing else. The principal's Card scope
/// still bounds which Cards it may emit for, and an administrator who revokes
/// the Role is not overridden by a later re-registration.
pub const WORKLOAD_ROLE: &str = "workload";

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

    use super::{BUILTIN_ROLES, builtin_role_uuid};
    use crate::Permission;

    #[test]
    fn all_seeds_are_valid_permissions() {
        for role in BUILTIN_ROLES {
            let value = serde_json::to_value(role.permissions).expect("permissions serialize");
            let round_trip: Vec<Permission> =
                serde_json::from_value(value).expect("permissions deserialize");
            assert_eq!(round_trip, role.permissions);
        }
    }

    /// Human SSO administration is reachable only through the tenant `admin`
    /// wildcard: `runtime_admin` (credential administration) and every other
    /// built-in role must not confer `identity_connections:write`.
    #[test]
    fn only_admin_grants_identity_connection_administration() {
        let required = Permission::identity_connections_write();
        let granting = BUILTIN_ROLES
            .iter()
            .filter(|role| role.permissions.iter().any(|held| held.covers(&required)))
            .map(|role| role.name)
            .collect::<Vec<_>>();
        assert_eq!(granting, vec!["admin"]);
        assert!(!Permission::service_accounts_write().covers(&required));
    }

    /// Proves Workflow run authority is granted to `writer` and `agent`,
    /// covered by `admin`'s wildcard, and withheld from `reader` and
    /// `runtime_admin`.
    ///
    /// # Panics
    ///
    /// Panics when a role's Workflow run grant differs from that split.
    #[test]
    fn workflow_run_is_granted_to_writing_roles_only() {
        for role in BUILTIN_ROLES {
            let granted = role
                .permissions
                .iter()
                .any(|permission| permission.covers(&Permission::workflow_run()));
            let expected = matches!(role.name, "admin" | "writer" | "agent");
            assert_eq!(granted, expected, "role {}", role.name);
        }
    }

    #[test]
    fn names_are_unique() {
        let names = BUILTIN_ROLES
            .iter()
            .map(|role| role.name)
            .collect::<BTreeSet<_>>();

        assert_eq!(names.len(), BUILTIN_ROLES.len());
    }

    #[test]
    fn role_uuid_is_stable_per_tenant_and_name() {
        let tenant = wyrd_spec::DataTenantId::new_v7();

        assert_eq!(
            builtin_role_uuid(tenant, "admin"),
            builtin_role_uuid(tenant, "admin")
        );
        assert_ne!(
            builtin_role_uuid(tenant, "admin"),
            builtin_role_uuid(tenant, "reader")
        );
    }
}
