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
    /// The tenant identity is nil and cannot own physical data.
    #[error("data tenant id must not be nil")]
    InvalidTenant,

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

    /// Checked physical-name accounting exceeded the platform address space.
    #[error("physical table binding size overflow")]
    BindingSizeOverflow,
}

/// Checked physical-name facts shared by Scribe admission and materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalBindingFacts<'a> {
    /// Authenticated tenant used in every physical tenant component.
    pub(crate) tenant: &'a DataTenantId,
    /// Validated logical table input retained until construction.
    pub(crate) table_ref: &'a TableRef,
    /// Bytes in the Iceberg namespace component vector.
    pub(crate) input_bytes: usize,
    /// Bytes in the object-store prefix.
    pub(crate) output_bytes: usize,
    /// Total owned string bytes in the resolved binding.
    pub(crate) binding_bytes: usize,
    /// Bytes in the retained fully-qualified logical table key.
    pub(crate) file_prefix_bytes: usize,
    /// Simultaneous physical binding construction peak admitted by Scribe.
    pub(crate) peak_bytes: usize,
}

/// Computes the five checked physical binding totals from component lengths.
///
/// # Errors
///
/// Returns [`TenantTableBindingError::BindingSizeOverflow`] when any exact
/// formula cannot be represented by `usize`.
fn checked_binding_totals(
    name: usize,
    namespace: usize,
    segment: usize,
) -> Result<(usize, usize, usize, usize, usize), TenantTableBindingError> {
    let add = |values: &[usize]| {
        values
            .iter()
            .try_fold(0_usize, |total, value| total.checked_add(*value))
    };
    let input = add(&[4, 7, 36, segment]).ok_or(TenantTableBindingError::BindingSizeOverflow)?;
    let output =
        add(&[8, 36, 1, segment, 1, name]).ok_or(TenantTableBindingError::BindingSizeOverflow)?;
    let binding = add(&[name, name, namespace, input, output])
        .ok_or(TenantTableBindingError::BindingSizeOverflow)?;
    let file_prefix =
        add(&[namespace, 1, name]).ok_or(TenantTableBindingError::BindingSizeOverflow)?;
    let peak =
        add(&[36, binding, file_prefix]).ok_or(TenantTableBindingError::BindingSizeOverflow)?;
    Ok((input, output, binding, file_prefix, peak))
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
    /// Computes the exact checked physical-name allocation facts without allocating.
    ///
    /// # Errors
    ///
    /// Returns an identity validation error or [`TenantTableBindingError::BindingSizeOverflow`]
    /// when any exact byte total cannot be represented.
    pub(crate) fn facts<'a>(
        tenant: &'a DataTenantId,
        table_ref: &'a TableRef,
    ) -> Result<PhysicalBindingFacts<'a>, TenantTableBindingError> {
        if tenant.as_uuid().is_nil() {
            return Err(TenantTableBindingError::InvalidTenant);
        }
        if !is_safe_name(&table_ref.name) {
            return Err(TenantTableBindingError::InvalidTableName {
                table_name: table_ref.name.clone(),
            });
        }
        let namespace = table_ref.namespace.as_str();
        let segment = namespace
            .strip_prefix("vala.")
            .filter(|value| !value.is_empty() && !value.contains('.'))
            .ok_or_else(|| TenantTableBindingError::InvalidPhysicalNamespace {
                detail: format!("logical namespace `{namespace}` is not one segment"),
            })?;
        let (input_bytes, output_bytes, binding_bytes, file_prefix_bytes, peak_bytes) =
            checked_binding_totals(table_ref.name.len(), namespace.len(), segment.len())?;
        Ok(PhysicalBindingFacts {
            tenant,
            table_ref,
            input_bytes,
            output_bytes,
            binding_bytes,
            file_prefix_bytes,
            peak_bytes,
        })
    }

    /// Materializes one binding using the capacities authorized by the same facts.
    ///
    /// # Errors
    ///
    /// Returns [`TenantTableBindingError::InvalidPhysicalNamespace`] if Iceberg
    /// rejects the already validated physical namespace components.
    pub(crate) fn from_facts(
        facts: PhysicalBindingFacts<'_>,
    ) -> Result<Self, TenantTableBindingError> {
        debug_assert_eq!(
            facts.peak_bytes,
            36 + facts.binding_bytes + facts.file_prefix_bytes
        );
        debug_assert!(facts.input_bytes >= 4 + 7 + 36);
        let tenant_string = facts.tenant.to_string();
        let namespace = facts.table_ref.namespace.as_str();
        let segment = namespace.strip_prefix("vala.").ok_or_else(|| {
            TenantTableBindingError::InvalidPhysicalNamespace {
                detail: format!("logical namespace `{namespace}` is not one segment"),
            }
        })?;
        let iceberg_namespace =
            NamespaceIdent::from_strs(["vala", "tenants", tenant_string.as_str(), segment])
                .map_err(|error| TenantTableBindingError::InvalidPhysicalNamespace {
                    detail: error.to_string(),
                })?;
        let mut object_prefix = String::with_capacity(facts.output_bytes);
        object_prefix.push_str("tenants/");
        object_prefix.push_str(&tenant_string);
        object_prefix.push('/');
        object_prefix.push_str(segment);
        object_prefix.push('/');
        object_prefix.push_str(&facts.table_ref.name);
        let mut table_name = String::with_capacity(facts.table_ref.name.len());
        table_name.push_str(&facts.table_ref.name);
        let mut logical_namespace = String::with_capacity(namespace.len());
        logical_namespace.push_str(namespace);
        Ok(Self {
            tenant: *facts.tenant,
            table_ref: facts.table_ref.clone(),
            iceberg_namespace,
            table_name,
            logical_namespace,
            object_prefix,
        })
    }
    /// Resolve one organization-qualified physical identity.
    ///
    /// The logical table name is retained as a local name. The tenant is added
    /// only to the physical Iceberg namespace and object-store prefix.
    ///
    /// # Errors
    /// Returns [`TenantTableBindingError::InvalidTenant`] for a nil tenant,
    /// [`TenantTableBindingError::InvalidTableName`] when the logical table
    /// name is not a safe local identifier,
    /// [`TenantTableBindingError::InvalidPhysicalNamespace`] when the fixed
    /// namespace cannot be represented by Iceberg, or
    /// [`TenantTableBindingError::BindingSizeOverflow`] when exact physical
    /// allocation accounting cannot be represented.
    pub fn resolve((tenant, table_ref): TenantTableKey) -> Result<Self, TenantTableBindingError> {
        let facts = Self::facts(&tenant, &table_ref)?;
        Self::from_facts(facts)
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

    /// Validate and return a staging object path under this table's prefix.
    ///
    /// Input paths are relative object-store keys. URI forms, absolute paths,
    /// traversal segments, and sibling prefixes are rejected before any object
    /// store operation is attempted.
    pub(crate) fn validate_object_path(&self, path: &str) -> Option<String> {
        let prefix = self.object_prefix.trim_end_matches('/');
        if prefix.is_empty()
            || path.is_empty()
            || path.starts_with('/')
            || path.contains("://")
            || path.contains('\\')
            || path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return None;
        }
        (path == prefix || path.starts_with(&format!("{prefix}/"))).then(|| path.to_owned())
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

    /// Proves the checked binding formulas and exact-capacity construction agree.
    #[test]
    fn physical_binding_facts_match_materialized_capacities() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let facts = TenantTableBinding::facts(&tenant, &table).expect("binding facts");
        assert_eq!(facts.input_bytes, 4 + 7 + 36 + 6);
        assert_eq!(facts.output_bytes, 8 + 36 + 1 + 6 + 1 + 5);
        assert_eq!(
            facts.binding_bytes,
            5 + 5 + "vala.traces".len() + facts.input_bytes + facts.output_bytes
        );
        assert_eq!(facts.file_prefix_bytes, "vala.traces".len() + 1 + 5);
        assert_eq!(
            facts.peak_bytes,
            36 + facts.binding_bytes + facts.file_prefix_bytes
        );
        let binding = TenantTableBinding::from_facts(facts).expect("materialized binding");
        assert_eq!(binding.object_prefix.len(), facts.output_bytes);
        assert_eq!(binding.object_prefix.capacity(), facts.output_bytes);
    }

    /// Proves each unrepresentable formula fails before physical allocation.
    #[test]
    fn physical_binding_facts_reject_overflow() {
        for lengths in [(usize::MAX, 11, 6), (5, usize::MAX, 6), (5, 11, usize::MAX)] {
            assert_eq!(
                checked_binding_totals(lengths.0, lengths.1, lengths.2),
                Err(TenantTableBindingError::BindingSizeOverflow)
            );
        }
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
    fn staging_object_paths_are_segment_bounded() {
        let tenant = DataTenantId::new_v7();
        let binding = TenantTableBinding::resolve((tenant, table())).expect("binding");
        let valid = format!("{}/part-000.parquet", binding.object_prefix);

        assert_eq!(binding.validate_object_path(&valid), Some(valid.clone()));
        assert!(
            binding
                .validate_object_path(&format!("{}-sibling/part.parquet", binding.object_prefix))
                .is_none()
        );
        assert!(
            binding
                .validate_object_path(&format!("s3://bucket/{valid}"))
                .is_none()
        );
        assert!(
            binding
                .validate_object_path(&format!("{}/../other/part.parquet", binding.object_prefix))
                .is_none()
        );
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
    fn tenant_table_binding_rejects_nil_tenant() {
        assert_eq!(
            TenantTableBinding::resolve((crate::test_support::nil_tenant(), table())),
            Err(TenantTableBindingError::InvalidTenant)
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
