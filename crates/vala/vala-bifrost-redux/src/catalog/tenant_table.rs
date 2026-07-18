//! Organization-qualified physical identity for one logical Bifrost table.

use iceberg::{NamespaceIdent, TableIdent};
use thiserror::Error;
use wyrd_spec::DataTenantId;

use crate::catalog::partition_spec::PartitionTransform;
use crate::catalog::table_ref::{TableRef, is_safe_name};

/// The logical table plus the authenticated organization that owns its data.
pub type TenantTableKey = (DataTenantId, TableRef);

/// The only errors produced while resolving or validating a physical table binding.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TenantTableBindingError {
    /// The logical table name cannot safely be used as a catalog or path segment.
    #[error("invalid logical table name `{table_name}`")]
    InvalidTableName { table_name: String },

    /// The authenticated organization does not match the binding's organization.
    #[error(
        "tenant-table binding mismatch: binding tenant `{binding_tenant}` does not match authenticated tenant `{authenticated_tenant}`"
    )]
    TenantMismatch {
        binding_tenant: DataTenantId,
        authenticated_tenant: DataTenantId,
    },

    /// A fixed namespace component could not be represented by Iceberg.
    #[error("invalid physical Iceberg namespace: {detail}")]
    InvalidPhysicalNamespace { detail: String },
}

/// Canonical physical identity derived from `(DataTenantId, TableRef)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantTableBinding {
    /// Authenticated organization that owns the physical table.
    pub tenant: DataTenantId,
    /// Tenant-free logical table reference.
    pub table_ref: TableRef,
    /// Organization-qualified Iceberg namespace.
    pub iceberg_namespace: NamespaceIdent,
    /// Local Iceberg table name.
    pub table_name: String,
    /// Logical namespace stored in `vala.file_list`.
    pub logical_namespace: String,
    /// Object-store prefix for this physical table.
    pub object_prefix: String,
}

impl TenantTableBinding {
    /// Resolve one organization-qualified physical identity.
    ///
    /// The logical table name is retained as a local name. The tenant is added
    /// only to the physical Iceberg namespace and object-store prefix.
    ///
    /// # Errors
    /// Returns [`TenantTableBindingError::InvalidTableName`] when the logical
    /// table name is not a safe local identifier, or
    /// [`TenantTableBindingError::InvalidPhysicalNamespace`] when the fixed
    /// namespace cannot be represented by Iceberg.
    pub fn resolve((tenant, table_ref): TenantTableKey) -> Result<Self, TenantTableBindingError> {
        if !is_safe_name(&table_ref.name) {
            return Err(TenantTableBindingError::InvalidTableName {
                table_name: table_ref.name,
            });
        }

        let logical_namespace = table_ref.namespace.as_str().to_owned();
        let logical_segment = logical_namespace
            .strip_prefix("vala.")
            .filter(|segment| !segment.is_empty() && !segment.contains('.'))
            .ok_or_else(|| TenantTableBindingError::InvalidPhysicalNamespace {
                detail: format!("logical namespace `{logical_namespace}` is not one segment"),
            })?;
        let tenant_string = tenant.to_string();
        let iceberg_namespace =
            NamespaceIdent::from_strs(["vala", "tenants", tenant_string.as_str(), logical_segment])
                .map_err(|error| TenantTableBindingError::InvalidPhysicalNamespace {
                    detail: error.to_string(),
                })?;

        Ok(Self {
            tenant,
            table_name: table_ref.name.clone(),
            object_prefix: format!(
                "tenants/{tenant_string}/{logical_segment}/{}",
                table_ref.name
            ),
            logical_namespace,
            iceberg_namespace,
            table_ref,
        })
    }

    /// Verify that a binding is used with the authenticated organization that created it.
    pub fn validate_authenticated_tenant(
        &self,
        authenticated_tenant: DataTenantId,
    ) -> Result<(), TenantTableBindingError> {
        if self.tenant == authenticated_tenant {
            Ok(())
        } else {
            Err(TenantTableBindingError::TenantMismatch {
                binding_tenant: self.tenant,
                authenticated_tenant,
            })
        }
    }

    /// Return the physical Iceberg table identity.
    #[must_use]
    pub fn table_ident(&self) -> TableIdent {
        TableIdent::new(self.iceberg_namespace.clone(), self.table_name.clone())
    }

    /// Return the physical namespace under its descriptive name.
    #[must_use]
    pub fn physical_namespace(&self) -> &NamespaceIdent {
        &self.iceberg_namespace
    }

    /// Return the single partition transform used by every physical table.
    #[must_use]
    pub const fn partition_transform(&self) -> PartitionTransform {
        PartitionTransform::Day
    }

    /// Return the canonical partition source and transform.
    #[must_use]
    pub fn partition_columns(&self) -> [(String, PartitionTransform); 1] {
        [("wyrd_event_time".to_owned(), self.partition_transform())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

    fn table() -> TableRef {
        TableRef::new(BifrostNamespace::Traces, "spans")
    }

    #[test]
    fn tenant_table_binding_uses_authenticated_organization() {
        let table = table();
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let binding_a = TenantTableBinding::resolve((tenant_a, table.clone())).expect("binding A");
        let binding_b = TenantTableBinding::resolve((tenant_b, table)).expect("binding B");

        assert_ne!(binding_a.iceberg_namespace, binding_b.iceberg_namespace);
        assert_ne!(binding_a.object_prefix, binding_b.object_prefix);
        assert_eq!(binding_a.logical_namespace, "vala.traces");
        assert_eq!(binding_b.logical_namespace, "vala.traces");
        assert_eq!(
            binding_a.iceberg_namespace.to_string(),
            format!("vala.tenants.{tenant_a}.traces")
        );
        assert_eq!(
            binding_a.object_prefix,
            format!("tenants/{tenant_a}/traces/spans")
        );
    }

    #[test]
    fn tenant_table_binding_keeps_logical_table_name() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");

        assert_eq!(binding.table_ref, table);
        assert_eq!(binding.table_name, "spans");
        assert!(!binding.table_name.contains(&tenant.to_string()));
    }

    #[test]
    fn tenant_table_binding_rejects_foreign_binding() {
        let binding_tenant = DataTenantId::new_v7();
        let authenticated_tenant = DataTenantId::new_v7();
        let binding = TenantTableBinding::resolve((binding_tenant, table())).expect("binding");

        assert_eq!(
            binding.validate_authenticated_tenant(authenticated_tenant),
            Err(TenantTableBindingError::TenantMismatch {
                binding_tenant,
                authenticated_tenant,
            })
        );
    }

    #[test]
    fn tenant_table_binding_uses_day_partition_only() {
        let binding =
            TenantTableBinding::resolve((DataTenantId::new_v7(), table())).expect("binding");

        assert_eq!(binding.partition_transform(), PartitionTransform::Day);
        assert_eq!(
            binding.partition_columns(),
            [("wyrd_event_time".to_owned(), PartitionTransform::Day)]
        );
    }

    #[test]
    fn tenant_table_binding_rejects_tenant_in_logical_name() {
        let tenant = DataTenantId::new_v7();
        let table = TableRef::new(BifrostNamespace::Traces, format!("{tenant}_spans"));
        let binding = TenantTableBinding::resolve((tenant, table)).expect("binding");

        assert_eq!(binding.table_name, format!("{tenant}_spans"));
        assert_eq!(binding.logical_namespace, "vala.traces");
    }
}
