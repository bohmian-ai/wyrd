//! Typed key for `vala.maintenance_leases` rows.
//!
//! A maintenance lease is named by its free-form `lease_key` text column. The
//! lease query fns ([`renew_lease_fenced`], [`try_acquire_lease`]) and the
//! bifrost `LeaseHeartbeat` handle all consume the key as a string
//! (`&str` / `impl Into<String>`). [`MaintenanceLeaseKey`] gives callers one
//! obvious way to build the per-table-writer key shape `table_writer:{uid}`
//! instead of hand-formatting the literal at each call site.
//!
//! Only the per-writer key shape lives here today. Global maintenance keys
//! (snapshot expiry, orphan GC) and election work land in later slices with
//! their own constructors; this module deliberately does not model a work-kind
//! enum yet.
//!
//! [`renew_lease_fenced`]: crate::queries::maintenance_leases::renew_lease_fenced
//! [`try_acquire_lease`]: crate::queries::maintenance_leases::try_acquire_lease

use std::fmt;

use uuid::Uuid;

/// Prefix for the per-table-writer maintenance lease key.
const TABLE_WRITER_PREFIX: &str = "table_writer";

/// Prefix for the per-derivation cross-table-derivation maintenance lease key.
const CROSS_TABLE_DERIVATION_PREFIX: &str = "cross_table_derivation";

/// The key that names a maintenance lease row.
///
/// Renders to the `&str` shape the lease query fns and `LeaseHeartbeat` accept.
/// Use [`as_str`](Self::as_str) or the [`fmt::Display`] impl to obtain the
/// rendered key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaintenanceLeaseKey(String);

impl MaintenanceLeaseKey {
    /// Build the lease key for a single table writer: `table_writer:{uid}`.
    ///
    /// `uid` is the table identity as a UUID; it renders to the same string a
    /// bifrost `TableUid` renders to, so the produced key matches the literal
    /// bifrost callers previously hand-formatted.
    #[must_use]
    pub fn table_writer(uid: &Uuid) -> Self {
        Self(format!("{TABLE_WRITER_PREFIX}:{uid}"))
    }

    /// Build the lease key for one cross-table derivation:
    /// `cross_table_derivation:{data_tenant_id}:{derivation_uid}`.
    ///
    /// The tenant identifies the derivation registration's isolation boundary
    /// and the UID identifies the derivation registration, so two tenants can
    /// hold distinct leases for otherwise identical derivation IDs. The
    /// `cross_table_derivation` namespace is admitted by the
    /// `maintenance_leases_lease_key_namespace_check` CHECK.
    #[must_use]
    pub fn cross_table_derivation(data_tenant_id: &Uuid, derivation_uid: &Uuid) -> Self {
        Self(format!(
            "{CROSS_TABLE_DERIVATION_PREFIX}:{data_tenant_id}:{derivation_uid}"
        ))
    }

    /// Borrow the rendered key as the `&str` the lease fns accept.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MaintenanceLeaseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_writer_key() {
        let uid = Uuid::parse_str("018f3c1e-2a4b-7c8d-9e0f-1a2b3c4d5e6f").expect("valid uuid");
        let key = MaintenanceLeaseKey::table_writer(&uid);

        assert_eq!(key.as_str(), format!("table_writer:{uid}"));
        assert_eq!(key.to_string(), format!("table_writer:{uid}"));
    }

    #[test]
    fn cross_table_derivation_key() {
        let uid = Uuid::parse_str("018f3c1e-2a4b-7c8d-9e0f-1a2b3c4d5e6f").expect("valid uuid");
        let data_tenant_id =
            Uuid::parse_str("00000000-0000-0000-0000-000000000000").expect("valid uuid");
        let key = MaintenanceLeaseKey::cross_table_derivation(&data_tenant_id, &uid);

        assert_eq!(
            key.as_str(),
            format!("cross_table_derivation:{data_tenant_id}:{uid}")
        );
        // The namespace (substring before the first ':') is what the
        // maintenance_leases CHECK whitelists.
        assert_eq!(
            key.as_str().split(':').next(),
            Some("cross_table_derivation")
        );
    }
}
