//! Fail-closed conversion from durable SQL identity to Redux table identity.

use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_spec::DataTenantId;

use super::error::ForgeError;
use crate::catalog::{TableRef, TenantTableBinding};

/// Converts a durable identity before any catalog load and validates tenant binding.
///
/// # Errors
/// Returns an invariant error for unsupported catalog/namespace, unsafe table
/// names, or a task tenant different from the scheduler binding.
pub(crate) fn task_table_binding(
    task_tenant: DataTenantId,
    scheduler_tenant: DataTenantId,
    identity: &ForgeTaskTableIdentity,
) -> Result<TenantTableBinding, ForgeError> {
    if task_tenant != scheduler_tenant {
        return Err(ForgeError::Invariant {
            detail: "Forge task tenant differs from scheduler binding".to_owned(),
        });
    }
    if identity.catalog != "wyrd-redux" {
        return Err(ForgeError::Invariant {
            detail: "unsupported Forge task catalog".to_owned(),
        });
    }
    let table_ref = TableRef::parse_fqn(&format!("{}.{}", identity.namespace, identity.table))
        .ok_or_else(|| ForgeError::Invariant {
            detail: "unsupported or unsafe Forge task table identity".to_owned(),
        })?;
    TenantTableBinding::resolve((scheduler_tenant, table_ref)).map_err(|error| {
        ForgeError::Invariant {
            detail: error.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Invalid durable identity is rejected by conversion before callers can load a catalog.
    #[test]
    fn durable_identity_conversion_fails_closed() {
        let tenant = DataTenantId::new_v7();
        let identity = |catalog: &str, namespace: &str, table: &str| ForgeTaskTableIdentity {
            catalog: catalog.to_owned(),
            namespace: namespace.to_owned(),
            table: table.to_owned(),
        };
        assert!(
            task_table_binding(tenant, tenant, &identity("other", "vala.bifrost", "events"))
                .is_err()
        );
        assert!(
            task_table_binding(tenant, tenant, &identity("wyrd-redux", "unknown", "events"))
                .is_err()
        );
        assert!(
            task_table_binding(
                tenant,
                tenant,
                &identity("wyrd-redux", "vala.bifrost", "../events")
            )
            .is_err()
        );
        assert!(
            task_table_binding(
                DataTenantId::new_v7(),
                tenant,
                &identity("wyrd-redux", "vala.bifrost", "events")
            )
            .is_err()
        );
    }
}
