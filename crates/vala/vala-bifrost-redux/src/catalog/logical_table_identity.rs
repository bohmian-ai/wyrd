//! Borrowed logical table identity and role-owned fixed physical projection.

use std::fmt;

use wyrd_spec::DataTenantId;

use crate::catalog::TableRef;
use crate::catalog::table_ref::is_safe_name;

/// Number of fixed physical strings materialized for one role operation.
const PHYSICAL_COMPONENTS: usize = 2;
/// Canonical textual length of one hyphenated UUID tenant identity.
const TENANT_TEXT_BYTES: usize = 36;

/// Validated logical identity borrowed from an authenticated operation.
#[derive(Clone, Copy, Debug)]
pub struct LogicalTableIdentity<'a> {
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
    /// Returns [`LogicalTableIdentityError::InvalidTenant`] for a nil tenant,
    /// [`LogicalTableIdentityError::TenantMismatch`] when the binding tenant is
    /// foreign to the authenticated operation,
    /// [`LogicalTableIdentityError::InvalidTableName`] for an unsafe table
    /// segment, or [`LogicalTableIdentityError::InvalidLogicalNamespace`] when
    /// the logical namespace cannot map to one physical segment.
    pub fn try_new(
        authenticated_tenant: &'a DataTenantId,
        binding_tenant: &'a DataTenantId,
        table: &'a TableRef,
    ) -> Result<Self, LogicalTableIdentityError<()>> {
        if authenticated_tenant.as_uuid().is_nil() || binding_tenant.as_uuid().is_nil() {
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
    #[must_use]
    pub const fn tenant(&self) -> &'a DataTenantId {
        self.tenant
    }

    /// Returns the borrowed logical table reference unchanged.
    #[must_use]
    pub const fn table(&self) -> &'a TableRef {
        self.table
    }

    /// Rejects persisted identity drift without materializing physical strings.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::PersistedIdentityMismatch`] when
    /// either the persisted tenant or logical table differs from this operation.
    pub fn validate_persisted(
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

    /// Computes the complete fixed projection and reserves it before allocation.
    ///
    /// The caller supplies its existing Gate, Scribe, Forge, or Oracle
    /// reservation authority. The returned projection retains that opaque guard
    /// and can only transfer it by moving the complete projection.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::Overflow`] when component or byte
    /// arithmetic overflows. Returns [`LogicalTableIdentityError::Reservation`]
    /// when the role authority refuses the checked facts, before any physical
    /// projection allocation.
    pub fn try_project<G, E>(
        self,
        role: PhysicalProjectionRole,
        reserve: impl FnOnce(PhysicalProjectionFacts) -> Result<G, E>,
    ) -> Result<PhysicalTableProjection<'a, G>, LogicalTableIdentityError<E>> {
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
        let facts = PhysicalProjectionFacts {
            role,
            material_bytes,
            component_count: PHYSICAL_COMPONENTS,
        };
        let guard = reserve(facts).map_err(LogicalTableIdentityError::Reservation)?;

        let mut tenant_buffer = uuid::Uuid::encode_buffer();
        let tenant_text = self
            .tenant
            .as_uuid()
            .hyphenated()
            .encode_lower(&mut tenant_buffer);
        let mut namespace = String::with_capacity(namespace_bytes);
        namespace.push_str("vala.tenants.");
        namespace.push_str(tenant_text);
        namespace.push('.');
        namespace.push_str(self.logical_segment);
        let mut object_prefix = String::with_capacity(object_prefix_bytes);
        object_prefix.push_str("tenants/");
        object_prefix.push_str(tenant_text);
        object_prefix.push('/');
        object_prefix.push_str(self.logical_segment);
        object_prefix.push('/');
        object_prefix.push_str(&self.table.name);

        Ok(PhysicalTableProjection {
            logical: self,
            _guard: guard,
            namespace: namespace.into_boxed_str(),
            object_prefix: object_prefix.into_boxed_str(),
            facts,
        })
    }
}

/// Checked material and cardinality facts presented to one role authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalProjectionFacts {
    /// Runtime role whose existing T5A authority must reserve this projection.
    pub role: PhysicalProjectionRole,
    /// Exact bytes across the fixed namespace and object-prefix strings.
    pub material_bytes: usize,
    /// Exact number of independently allocated physical string components.
    pub component_count: usize,
}

/// Closed runtime roles that may own one operation-scoped physical projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalProjectionRole {
    /// Ingress Gate routing and validation.
    Gate,
    /// Scribe buffering, WAL, and sealing.
    Scribe,
    /// Forge maintenance and publication.
    Forge,
    /// Oracle planning and query execution.
    Oracle,
}

/// Fixed physical projection retaining one role's opaque reservation guard.
pub struct PhysicalTableProjection<'a, G> {
    /// Borrowed logical identity preserved unchanged through the operation.
    logical: LogicalTableIdentity<'a>,
    /// Opaque role authority retained solely for its terminal drop behavior.
    _guard: G,
    /// Fixed tenant-qualified physical namespace.
    namespace: Box<str>,
    /// Fixed tenant-qualified object-store prefix.
    object_prefix: Box<str>,
    /// Checked facts retained for materialized-capacity reconciliation.
    facts: PhysicalProjectionFacts,
}

impl<G> fmt::Debug for PhysicalTableProjection<'_, G> {
    /// Formats safe identity and capacity observations without exposing the guard.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PhysicalTableProjection")
            .field("logical_table", &self.logical.table)
            .field("namespace", &self.namespace)
            .field("object_prefix", &self.object_prefix)
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}

impl<'a, G> PhysicalTableProjection<'a, G> {
    /// Returns the original borrowed logical identity.
    #[must_use]
    pub const fn logical(&self) -> LogicalTableIdentity<'a> {
        self.logical
    }

    /// Returns the tenant-qualified physical namespace.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the tenant-qualified contained object prefix.
    #[must_use]
    pub fn object_prefix(&self) -> &str {
        &self.object_prefix
    }

    /// Returns exact materialized bytes and component cardinality.
    #[must_use]
    pub const fn materialized_facts(&self) -> PhysicalProjectionFacts {
        self.facts
    }

    /// Validates an object key is contained beneath this projection's prefix.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalTableIdentityError::PathEscape`] for an empty, absolute,
    /// URI, backslash, traversal, repeated-separator, or sibling-prefix path.
    pub fn validate_object_path<'b>(
        &self,
        path: &'b str,
    ) -> Result<&'b str, LogicalTableIdentityError<()>> {
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
#[non_exhaustive]
pub enum LogicalTableIdentityError<E> {
    /// A nil tenant cannot own logical or physical data.
    #[error("data tenant id must not be nil")]
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
    #[error("persisted tenant/table identity does not match the operation")]
    PersistedIdentityMismatch,
    /// Checked physical component arithmetic overflowed.
    #[error("physical table projection bound overflowed")]
    Overflow,
    /// The caller's existing role authority refused the complete projection.
    #[error("physical table projection reservation refused: {0}")]
    Reservation(E),
    /// An object key escaped or did not belong to the projected table prefix.
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
        assert!(std::ptr::eq(logical.tenant(), &tenant));
        assert!(std::ptr::eq(logical.table(), &table));
    }

    /// All four runtime roles receive exact facts and release only at terminal drop.
    #[test]
    fn logical_table_identity_role_transfer_cancellation_and_release() {
        for role in [
            PhysicalProjectionRole::Gate,
            PhysicalProjectionRole::Scribe,
            PhysicalProjectionRole::Forge,
            PhysicalProjectionRole::Oracle,
        ] {
            let tenant = DataTenantId::new_v7();
            let table = table();
            let releases = Arc::new(AtomicUsize::new(0));
            let logical =
                LogicalTableIdentity::try_new(&tenant, &tenant, &table).expect("logical identity");
            let projection = logical
                .try_project(role, |facts| {
                    assert_eq!(facts.role, role);
                    assert_eq!(facts.component_count, 2);
                    assert!(facts.material_bytes > table.name.len());
                    Ok::<_, ()>(RoleGuard(Arc::clone(&releases)))
                })
                .expect("role projection");
            let transferred = transfer(projection);
            assert_eq!(releases.load(Ordering::SeqCst), 0, "{role:?}");
            drop(transferred);
            assert_eq!(releases.load(Ordering::SeqCst), 1, "{role:?}");
        }
    }

    /// Moves the complete projection and opaque guard through a role boundary.
    fn transfer<'a, G>(
        projection: PhysicalTableProjection<'a, G>,
    ) -> PhysicalTableProjection<'a, G> {
        projection
    }

    /// Reservation refusal happens before a physical string can be observed.
    #[test]
    fn logical_table_identity_refuses_before_physical_allocation() {
        let tenant = DataTenantId::new_v7();
        let table = table();
        let logical = LogicalTableIdentity::try_new(&tenant, &tenant, &table).expect("valid");
        let error =
            logical.try_project(PhysicalProjectionRole::Gate, |_| Err::<(), _>("role full"));
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

    /// Foreign and nil tenants fail before projection planning.
    #[test]
    fn logical_table_identity_rejects_foreign_and_nil_tenants() {
        let tenant = DataTenantId::new_v7();
        let foreign = DataTenantId::new_v7();
        let nil = crate::test_support::nil_tenant();
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
        let projection = logical
            .try_project(PhysicalProjectionRole::Forge, |_| Ok::<_, ()>(()))
            .expect("projected");
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
