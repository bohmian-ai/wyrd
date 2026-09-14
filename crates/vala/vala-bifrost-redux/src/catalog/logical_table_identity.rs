//! Borrowed logical table identity and role-owned fixed physical projection.

use std::fmt;

use wyrd_spec::DataTenantId;

use crate::catalog::TableRef;
use crate::catalog::table_ref::is_safe_name;

/// Number of fixed physical strings materialized for one role operation.
const PHYSICAL_COMPONENTS: usize = 2;
/// Canonical textual length of one hyphenated UUID tenant identity.
const TENANT_TEXT_BYTES: usize = 36;

/// Validated logical identity borrowed from one authenticated role operation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LogicalTableIdentity<'a> {
    /// Authenticated tenant borrowed without formatting or duplication.
    tenant: &'a DataTenantId,
    /// Stable logical table reference borrowed from the operation.
    table: &'a TableRef,
    /// Validated one-segment physical namespace suffix.
    logical_segment: &'a str,
}

impl<'a> LogicalTableIdentity<'a> {
    /// Validates and borrows one authenticated tenant/table identity.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::InvalidTenant`] for the system owner,
    /// [`LogicalTableIdentityError::TenantMismatch`] when the binding tenant is
    /// foreign to the authenticated operation,
    /// [`LogicalTableIdentityError::InvalidTableName`] for an unsafe table
    /// segment, or [`LogicalTableIdentityError::InvalidLogicalNamespace`] when
    /// the logical namespace cannot map to one physical segment.
    pub(crate) fn try_new(
        authenticated_tenant: &'a DataTenantId,
        binding_tenant: &'a DataTenantId,
        table: &'a TableRef,
    ) -> Result<Self, LogicalTableIdentityError<()>> {
        if *authenticated_tenant == DataTenantId::SYSTEM_OWNER
            || *binding_tenant == DataTenantId::SYSTEM_OWNER
        {
            return Err(LogicalTableIdentityError::InvalidTenant);
        }
        if authenticated_tenant != binding_tenant {
            return Err(LogicalTableIdentityError::TenantMismatch);
        }
        if !is_safe_name(&table.name) {
            return Err(LogicalTableIdentityError::InvalidTableName);
        }
        let logical_namespace = table.namespace.as_str();
        let logical_segment = logical_namespace
            .strip_prefix("vala.")
            .filter(|segment| !segment.is_empty() && !segment.contains('.'))
            .ok_or(LogicalTableIdentityError::InvalidLogicalNamespace)?;
        Ok(Self {
            tenant: authenticated_tenant,
            table,
            logical_segment,
        })
    }

    /// Returns the borrowed authenticated tenant.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn tenant(&self) -> &'a DataTenantId {
        self.tenant
    }

    /// Returns the borrowed logical table reference unchanged.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn table(&self) -> &'a TableRef {
        self.table
    }

    /// Rejects persisted identity drift without materializing physical strings.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::PersistedIdentityMismatch`] when
    /// either the persisted tenant or logical table differs from this operation.
    #[cfg(test)]
    pub(crate) fn validate_persisted(
        &self,
        persisted_tenant: &DataTenantId,
        persisted_table: &TableRef,
    ) -> Result<(), LogicalTableIdentityError<()>> {
        if self.tenant == persisted_tenant && self.table == persisted_table {
            Ok(())
        } else {
            Err(LogicalTableIdentityError::PersistedIdentityMismatch)
        }
    }

    /// Computes exact projection facts without allocating physical strings.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::Overflow`] when component or byte
    /// arithmetic overflows.
    pub(crate) fn projection_facts(
        &self,
    ) -> Result<PhysicalProjectionFacts, LogicalTableIdentityError<()>> {
        let namespace_bytes = checked_sum(&[
            "vala".len(),
            1,
            "tenants".len(),
            1,
            TENANT_TEXT_BYTES,
            1,
            self.logical_segment.len(),
        ])?;
        let object_prefix_bytes = checked_sum(&[
            "tenants".len(),
            1,
            TENANT_TEXT_BYTES,
            1,
            self.logical_segment.len(),
            1,
            self.table.name.len(),
        ])?;
        let material_bytes = namespace_bytes
            .checked_add(object_prefix_bytes)
            .ok_or(LogicalTableIdentityError::Overflow)?;
        Ok(PhysicalProjectionFacts {
            material_bytes,
            component_count: PHYSICAL_COMPONENTS,
            namespace_bytes,
            object_prefix_bytes,
        })
    }

    /// Reserves and constructs one fixed physical projection.
    ///
    /// The reservation closure must split the exact bytes from the calling
    /// role's already admitted owner. It runs before either physical string is
    /// allocated, and the returned guard remains inseparable from the projection.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::Overflow`] when checked planning
    /// overflows or [`LogicalTableIdentityError::Reservation`] when the existing
    /// role owner refuses the exact child before allocation.
    pub(crate) fn try_project<G, E>(
        self,
        reserve: impl FnOnce(PhysicalProjectionFacts) -> Result<G, E>,
    ) -> Result<PhysicalTableProjection<G>, LogicalTableIdentityError<E>> {
        let facts = self
            .projection_facts()
            .map_err(|_| LogicalTableIdentityError::Overflow)?;
        let guard = reserve(facts).map_err(LogicalTableIdentityError::Reservation)?;

        let mut tenant_buffer = uuid::Uuid::encode_buffer();
        let tenant_text = self
            .tenant
            .as_uuid()
            .hyphenated()
            .encode_lower(&mut tenant_buffer);
        let mut namespace = String::with_capacity(facts.namespace_bytes);
        namespace.push_str("vala.tenants.");
        namespace.push_str(tenant_text);
        namespace.push('.');
        namespace.push_str(self.logical_segment);
        let mut object_prefix = String::with_capacity(facts.object_prefix_bytes);
        object_prefix.push_str("tenants/");
        object_prefix.push_str(tenant_text);
        object_prefix.push('/');
        object_prefix.push_str(self.logical_segment);
        object_prefix.push('/');
        object_prefix.push_str(&self.table.name);

        Ok(PhysicalTableProjection {
            _guard: guard,
            namespace: namespace.into_boxed_str(),
            object_prefix: object_prefix.into_boxed_str(),
            facts,
        })
    }
}

/// Checked material and cardinality facts presented to one role authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PhysicalProjectionFacts {
    /// Exact bytes across the fixed namespace and object-prefix strings.
    pub(crate) material_bytes: usize,
    /// Exact number of independently allocated physical string components.
    pub(crate) component_count: usize,
    /// Exact tenant-qualified namespace bytes.
    namespace_bytes: usize,
    /// Exact tenant-qualified object-prefix bytes.
    object_prefix_bytes: usize,
}

/// Fixed physical projection retaining one role's admitted child guard.
pub(crate) struct PhysicalTableProjection<G> {
    /// Opaque role child retained solely for its terminal drop behavior.
    _guard: G,
    /// Fixed tenant-qualified physical namespace.
    namespace: Box<str>,
    /// Fixed tenant-qualified object-store prefix.
    object_prefix: Box<str>,
    /// Checked facts retained for materialized-capacity reconciliation.
    facts: PhysicalProjectionFacts,
}

impl<G> fmt::Debug for PhysicalTableProjection<G> {
    /// Formats safe physical identity and capacity observations without the guard.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PhysicalTableProjection")
            .field("namespace", &self.namespace)
            .field("object_prefix", &self.object_prefix)
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}

impl<G> PhysicalTableProjection<G> {
    /// Returns the tenant-qualified physical namespace.
    #[must_use]
    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the tenant-qualified contained object prefix.
    #[must_use]
    pub(crate) fn object_prefix(&self) -> &str {
        &self.object_prefix
    }

    /// Validates an object key is contained beneath this projection's prefix.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::PathEscape`] for an empty, absolute,
    /// URI, backslash, traversal, repeated-separator, or sibling-prefix path.
    #[cfg(test)]
    pub(crate) fn validate_object_path<'a>(
        &self,
        path: &'a str,
    ) -> Result<&'a str, LogicalTableIdentityError<()>> {
        let prefix = self.object_prefix.trim_end_matches('/');
        if path.is_empty()
            || path.starts_with('/')
            || path.contains("://")
            || path.contains('\\')
            || path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
            || !(path == prefix
                || path.starts_with(prefix) && path.as_bytes().get(prefix.len()) == Some(&b'/'))
        {
            return Err(LogicalTableIdentityError::PathEscape);
        }
        Ok(path)
    }
}

/// Validation, checked-planning, or role-reservation failure for table projection.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum LogicalTableIdentityError<E> {
    /// The system owner cannot own logical or physical query data.
    #[error("data tenant id must not be the system owner")]
    InvalidTenant,
    /// The binding tenant differs from the authenticated operation tenant.
    #[error("authenticated tenant does not own the table binding")]
    TenantMismatch,
    /// The logical table name is unsafe as a physical path segment.
    #[error("logical table name is not a safe physical segment")]
    InvalidTableName,
    /// The logical namespace cannot map to one physical namespace segment.
    #[error("logical namespace cannot map to one physical segment")]
    InvalidLogicalNamespace,
    /// Persisted tenant or logical table identity differs from the operation.
    #[cfg(test)]
    #[error("persisted tenant/table identity does not match the operation")]
    PersistedIdentityMismatch,
    /// Checked physical component arithmetic overflowed.
    #[error("physical table projection bound overflowed")]
    Overflow,
    /// The caller's existing role authority refused the complete projection.
    #[error("physical table projection reservation refused: {0}")]
    Reservation(E),
    /// An object key escaped or did not belong to the projected table prefix.
    #[cfg(test)]
    #[error("object path escapes the physical table prefix")]
    PathEscape,
}

/// Computes one exact component sum without allocating.
///
/// # Errors
///
/// Returns [`LogicalTableIdentityError::Overflow`] when the sum exceeds
/// `usize`.
fn checked_sum<E>(parts: &[usize]) -> Result<usize, LogicalTableIdentityError<E>> {
    parts
        .iter()
        .try_fold(0usize, |sum, part| sum.checked_add(*part))
        .ok_or(LogicalTableIdentityError::Overflow)
}

#[cfg(test)]
mod tests {
    //! Focused borrowed-identity, checked-refusal, role transfer/release,
    //! tenant validation, unsafe-name, persisted-identity, and containment proof.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::namespaces::BifrostNamespace;

    /// Builds the stable logical table used by identity tests.
    fn table() -> TableRef {
        TableRef::new(BifrostNamespace::Traces, "spans")
    }

    /// Guard that records exactly one role-owned terminal release.
    struct RoleGuard(Arc<AtomicUsize>);

    impl Drop for RoleGuard {
        /// Records release when the projection reaches its terminal owner.
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The logical identity retains pointers to its validated caller-owned values.
    #[test]
    fn logical_table_identity_borrows_validated_components() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let logical = LogicalTableIdentity::try_new(&tenant, &tenant, &table).expect("valid");
        assert!(std::ptr::eq(logical.tenant(), &raw const tenant));
        assert!(std::ptr::eq(logical.table(), &raw const table));
    }

    /// A projection receives exact facts and releases only at terminal drop.
    #[test]
    fn logical_table_identity_role_transfer_cancellation_and_release() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let releases = Arc::new(AtomicUsize::new(0));
        let logical =
            LogicalTableIdentity::try_new(&tenant, &tenant, &table).expect("logical identity");
        let projection = logical
            .try_project(|facts| {
                assert_eq!(facts.component_count, 2);
                assert!(facts.material_bytes > table.name.len());
                Ok::<_, ()>(RoleGuard(Arc::clone(&releases)))
            })
            .expect("role projection");
        let transferred = transfer(projection);
        assert_eq!(releases.load(Ordering::SeqCst), 0);
        drop(transferred);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
    }

    /// Moves the complete projection and opaque guard through a role boundary.
    fn transfer<G>(projection: PhysicalTableProjection<G>) -> PhysicalTableProjection<G> {
        projection
    }

    /// Reservation refusal happens before a physical string can be observed.
    #[test]
    fn logical_table_identity_refuses_before_physical_allocation() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let logical = LogicalTableIdentity::try_new(&tenant, &tenant, &table).expect("valid");
        let error = logical.try_project(|_| Err::<(), _>("role full"));
        assert!(matches!(
            error,
            Err(LogicalTableIdentityError::Reservation("role full"))
        ));
    }

    /// Checked addition reports overflow without invoking a reservation authority.
    #[test]
    fn logical_table_identity_checked_sum_refuses_overflow() {
        assert!(matches!(
            checked_sum::<()>(&[usize::MAX, 1]),
            Err(LogicalTableIdentityError::Overflow)
        ));
    }

    /// Foreign and system-owner tenants fail before projection planning.
    #[test]
    fn logical_table_identity_rejects_foreign_and_system_owner_tenants() {
        let tenant = DataTenantId::new_v7();
        let foreign = DataTenantId::new_v7();
        let nil = DataTenantId::SYSTEM_OWNER;
        let table = table();
        assert!(matches!(
            LogicalTableIdentity::try_new(&tenant, &foreign, &table),
            Err(LogicalTableIdentityError::TenantMismatch)
        ));
        assert!(matches!(
            LogicalTableIdentity::try_new(&nil, &nil, &table),
            Err(LogicalTableIdentityError::InvalidTenant)
        ));
    }

    /// Unsafe logical names fail without creating a physical projection.
    #[test]
    fn logical_table_identity_rejects_unsafe_name() {
        let tenant = DataTenantId::new_v7();
        let unsafe_table = TableRef::new(BifrostNamespace::Traces, "../spans");
        assert!(matches!(
            LogicalTableIdentity::try_new(&tenant, &tenant, &unsafe_table),
            Err(LogicalTableIdentityError::InvalidTableName)
        ));
    }

    /// Persisted mismatch and sibling or traversal paths fail closed.
    #[test]
    fn logical_table_identity_validates_persisted_identity_and_path_containment() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let logical = LogicalTableIdentity::try_new(&tenant, &tenant, &table).expect("valid");
        assert!(matches!(
            logical.validate_persisted(&DataTenantId::new_v7(), &table),
            Err(LogicalTableIdentityError::PersistedIdentityMismatch)
        ));
        let projection = logical.try_project(|_| Ok::<_, ()>(())).expect("projected");
        let valid = format!("{}/data/part.parquet", projection.object_prefix());
        assert_eq!(projection.validate_object_path(&valid), Ok(valid.as_str()));
        assert!(
            projection
                .validate_object_path(&format!("{}-sibling/file", projection.object_prefix()))
                .is_err()
        );
        assert!(
            projection
                .validate_object_path(&format!("{}/../other/file", projection.object_prefix()))
                .is_err()
        );
    }
}
