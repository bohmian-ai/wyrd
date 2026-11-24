//! Builtin RBAC roles shared across Wyrd server surfaces.
//!
//! A role is an operation bundle: it says what an account can do, never
//! where. A role binding supplies the scope, and
//! [`wyrd_spec::auth::compile_binding`] applies the bundle there.

use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Operation;

use crate::permission::{Action, Resource};

/// Fixed namespace for deterministic per-tenant builtin role IDs.
pub const NS_BUILTIN_ROLE: Uuid = Uuid::from_u128(0x6ad8_2377_3a8f_5f42_9d17_a9f5_5b1c_5c63);

/// A builtin role definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinRole {
    /// Stable, read-only role name.
    pub name: &'static str,
    /// Server-set, read-only one-line summary seeded into
    /// `wyrd.auth_roles.description`.
    pub description: &'static str,
    /// Operations seeded into `wyrd.auth_roles.permissions`.
    pub operations: &'static [Operation],
}

const fn op(resource: Resource, action: Action) -> Operation {
    Operation { resource, action }
}

/// Resource and derived-record reads plus authorized queries. No audit
/// history and no captured gateway payloads.
const VIEWER: &[Operation] = &[
    op(Resource::Cards, Action::Read),
    op(Resource::Executions, Action::Read),
    op(Resource::Artifacts, Action::Read),
    op(Resource::Operators, Action::Read),
    op(Resource::Gateway, Action::Read),
    op(Resource::BifrostTable, Action::Read),
    op(Resource::BifrostQuery, Action::Read),
];

/// A running Service: read its granted runtime resources and their execution
/// context, execute verification, append authorized data, and invoke granted
/// gateways. No authoring, approval, audit, or captured payload read.
const WORKLOAD: &[Operation] = &[
    op(Resource::Cards, Action::Read),
    op(Resource::Executions, Action::Read),
    op(Resource::Artifacts, Action::Read),
    op(Resource::Operators, Action::Read),
    op(Resource::Gateway, Action::Read),
    op(Resource::BifrostTable, Action::Read),
    op(Resource::BifrostQuery, Action::Read),
    op(Resource::BifrostTable, Action::Write),
    op(Resource::BifrostRecord, Action::Write),
    op(Resource::Verifier, Action::Run),
    op(Resource::Triggers, Action::Write),
    op(Resource::Operators, Action::Invoke),
    op(Resource::Gateway, Action::Invoke),
];

/// [`VIEWER`] plus authoring, verification binding management, execution,
/// and cancellation. No approvals and no access administration.
const EDITOR: &[Operation] = &[
    op(Resource::Cards, Action::Read),
    op(Resource::Executions, Action::Read),
    op(Resource::Artifacts, Action::Read),
    op(Resource::Operators, Action::Read),
    op(Resource::Gateway, Action::Read),
    op(Resource::BifrostTable, Action::Read),
    op(Resource::BifrostQuery, Action::Read),
    op(Resource::BifrostTable, Action::Write),
    op(Resource::BifrostRecord, Action::Write),
    op(Resource::Verifier, Action::Run),
    op(Resource::Triggers, Action::Write),
    op(Resource::Operators, Action::Invoke),
    op(Resource::Gateway, Action::Invoke),
    op(Resource::Cards, Action::Write),
    op(Resource::Cards, Action::Delete),
    op(Resource::Artifacts, Action::Write),
    op(Resource::Policy, Action::Lock),
    op(Resource::Services, Action::Install),
];

/// Read and decide approvals on the bound Verifiers. Deciding also needs the
/// execution's parent to be readable, which a Repository-owned execution
/// requires a separate Repository binding for.
const APPROVER: &[Operation] = &[
    op(Resource::Cards, Action::Read),
    op(Resource::Verifier, Action::Approve),
];

/// Explicit tenant audit-history read; nothing else.
const AUDITOR: &[Operation] = &[op(Resource::Audit, Action::Read)];

/// Source of truth for Wyrd's per-tenant builtin roles.
///
/// Bundles, not a ladder. Only `admin` (through `*`, tenant scope only)
/// holds credential, user, identity-connection, GitHub-connection,
/// Operator-secret, and gateway-configuration administration. Platform-plane
/// and engine-peer permissions belong to no tenant Role.
pub const BUILTIN_ROLES: &[BuiltinRole] = &[
    BuiltinRole {
        name: "admin",
        description: "Everything, including people, sign-in and secrets",
        operations: &[op(Resource::Wildcard, Action::Wildcard)],
    },
    BuiltinRole {
        name: "editor",
        description: "Write Cards and tables, run Verifiers, invoke the gateway and Operators",
        operations: EDITOR,
    },
    BuiltinRole {
        name: "workload",
        description: "What a service account needs to run verification",
        operations: WORKLOAD,
    },
    BuiltinRole {
        name: "viewer",
        description: "Read Cards, executions, artifacts and tables",
        operations: VIEWER,
    },
    BuiltinRole {
        name: "approver",
        description: "Read Cards and approve Verifiers",
        operations: APPROVER,
    },
    BuiltinRole {
        name: "auditor",
        description: "Read the audit log",
        operations: AUDITOR,
    },
];

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

    use wyrd_spec::auth::{CardPermissionScope, Operation, PermissionScope, compile_binding};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::CardName;

    use super::{BUILTIN_ROLES, builtin_role_uuid};
    use crate::{Permission, PermissionSet};

    fn operations(name: &str) -> &'static [Operation] {
        BUILTIN_ROLES
            .iter()
            .find(|role| role.name == name)
            .expect("role is built in")
            .operations
    }

    fn at(name: &str, scope: &PermissionScope) -> PermissionSet {
        compile_binding(operations(name), scope)
            .map(PermissionSet::from_iter)
            .unwrap_or_default()
    }

    fn family(kind: CardKind, name: &str) -> PermissionScope {
        PermissionScope::Card(CardPermissionScope::family(
            kind,
            CardName::new(name).expect("name"),
        ))
    }

    /// Proves the six agreed bundles exist and every operation round-trips.
    ///
    /// # Panics
    /// Panics when the name set differs or an operation fails to decode.
    #[test]
    fn the_agreed_bundles_are_built_in() {
        let names = BUILTIN_ROLES
            .iter()
            .map(|role| role.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            names,
            BTreeSet::from([
                "admin", "approver", "auditor", "editor", "viewer", "workload"
            ])
        );
        for role in BUILTIN_ROLES {
            let value = serde_json::to_value(role.operations).expect("serializes");
            let round_trip: Vec<Operation> = serde_json::from_value(value).expect("decodes");
            assert_eq!(round_trip, role.operations);
        }
    }

    /// Proves each tenant-scoped bundle's boundary: no audit or captured
    /// payload outside auditor/admin, no approval outside approver/admin, no
    /// administration outside admin, and no authoring for workload.
    ///
    /// # Panics
    /// Panics when a bundle holds or lacks a boundary permission.
    #[test]
    fn bundles_hold_their_agreed_boundaries() {
        let all = PermissionScope::All;
        for name in ["viewer", "workload", "editor", "approver"] {
            let held = at(name, &all);
            assert!(
                !held.contains(&Permission::audit_read()),
                "{name} reads audit"
            );
            assert!(
                !held.contains(&Permission::gateway_payload_read()),
                "{name} reads payloads"
            );
        }
        for name in ["viewer", "workload", "editor", "auditor"] {
            assert!(
                !at(name, &all).contains(&Permission::verifier_approve_any()),
                "{name} approves"
            );
        }
        assert!(at("auditor", &all).contains(&Permission::audit_read()));
        assert!(!at("auditor", &all).contains(&Permission::card_read()));
        assert!(!at("approver", &all).contains(&Permission::verifier_run_any()));
        assert!(!at("approver", &all).contains(&Permission::card_write()));
        assert!(!at("workload", &all).contains(&Permission::card_write()));
        assert!(at("editor", &all).contains(&Permission::card_write()));
        assert!(at("viewer", &all).contains(&Permission::executions_read_any()));
        for required in [
            Permission::service_accounts_write(),
            Permission::identity_connections_write(),
            Permission::github_connections_write(),
            Permission::users_manage(),
            Permission::operators_write(),
            Permission::gateway_write(),
            Permission::bifrost_peer_invoke(),
            Permission::tenant_create(),
        ] {
            let granting = BUILTIN_ROLES
                .iter()
                .filter(|role| at(role.name, &all).contains(&required))
                .map(|role| role.name)
                .collect::<Vec<_>>();
            assert_eq!(granting, vec!["admin"], "{required}");
        }
    }

    /// Proves resource-scoped bundles: viewer at a Repository family reads
    /// that family's Cards and executions only; approver at a Verifier
    /// decides that Verifier only; admin and auditor have no resource scope.
    ///
    /// # Panics
    /// Panics when a scoped bundle compiles to the wrong authority.
    #[test]
    fn bundles_compile_at_resource_scope() {
        let payments = family(CardKind::Repository, "payments");
        let viewer = at("viewer", &payments);
        assert!(viewer.contains(&Permission::card_read().at(payments.clone())));
        assert!(viewer.contains(&Permission::executions_read_any().at(payments.clone())));
        assert!(!viewer.contains(&Permission::card_read()));
        assert!(
            !viewer.contains(&Permission::card_read().at(family(CardKind::Repository, "ledger")))
        );
        let lint = family(CardKind::Verifier, "lint");
        assert!(
            at("approver", &lint).contains(&Permission::verifier_approve_any().at(lint.clone()))
        );
        assert!(
            compile_binding(operations("approver"), &payments)
                .expect("reads apply")
                .iter()
                .all(|permission| permission.action != wyrd_spec::auth::Action::Approve),
            "approving is Verifier-scoped"
        );
        for name in ["admin", "auditor"] {
            assert!(
                compile_binding(operations(name), &payments).is_err(),
                "{name}"
            );
        }
    }

    /// Proves deterministic role ids are stable per tenant and name.
    ///
    /// # Panics
    /// Panics when the same input yields different ids or names collide.
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
