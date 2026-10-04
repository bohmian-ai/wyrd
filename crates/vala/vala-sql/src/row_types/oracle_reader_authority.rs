//! Validated identity behind Oracle's durable reader authority.
//!
//! One thing is durable here: which registered table a cut acquisition or
//! destructive maintenance decision serializes on. This module owns its pure
//! validation so the query owners stay statements.

use wyrd_spec::DataTenantId;

use crate::SqlError;

/// Constructs a fail-closed invariant error without leaking row payloads.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}

/// The durable identity every active-read and maintenance statement is keyed by.
///
/// `table_uid` is the identity; the catalog, namespace, and table names are
/// checked payload that makes registry drift visible rather than silent. A
/// consumer that reconstructs a namespace string and finds it disagreeing with
/// the registered table has contradictory evidence, not a naming preference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableAuthorityIdentity {
    /// Tenant owning both the table and its active reads.
    pub tenant: DataTenantId,
    /// Durable 16-byte table UID from `vala.bifrost_tables`.
    pub table_uid: [u8; 16],
    /// Exact catalog wire name; only the single Bifrost catalog is valid.
    pub catalog_name: String,
    /// Logical Bifrost namespace of the registered table.
    pub namespace_name: String,
    /// Physical table name inside that namespace.
    pub table_name: String,
}

impl TableAuthorityIdentity {
    /// Validates the checked payload before any statement binds it.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the catalog is not the
    /// single Bifrost catalog or when a name segment is blank.
    pub fn validate(&self, expected_catalog: &str) -> Result<(), SqlError> {
        if self.catalog_name != expected_catalog {
            return Err(invariant(
                "table authority names a catalog other than the Bifrost catalog",
            ));
        }
        if self.namespace_name.trim().is_empty() || self.table_name.trim().is_empty() {
            return Err(invariant("table authority names a blank table identity"));
        }
        Ok(())
    }

    /// Canonical ordering key used to lock several tables without deadlock.
    ///
    /// Multi-table admission sorts by `(tenant UUID bytes, table UID bytes)`
    /// and never waits on a lower key while holding a higher one, so two
    /// queries requesting the same pair in opposite orders still serialize.
    #[must_use]
    pub fn order_key(&self) -> ([u8; 16], [u8; 16]) {
        (*uuid::Uuid::from(self.tenant).as_bytes(), self.table_uid)
    }
}
