//! Typed rows and inputs for role-fenced cluster membership.

use chrono::{DateTime, Utc};
use wyrd_spec::vala::api::{ClusterCapabilities, ClusterNodeKey, ClusterRoleLease};

/// Registration input for one independently fenced node role.
pub struct RoleRegistration {
    /// Composite node/role identity.
    pub key: ClusterNodeKey,
    /// Non-empty private service address.
    pub address: String,
    /// Typed v1 role capability.
    pub capabilities: ClusterCapabilities,
    /// Role boot timestamp.
    pub started_at: DateTime<Utc>,
}

/// Result of registering one role and advancing its fence.
pub struct RegisteredRoleRow {
    /// Projected registered lease.
    pub lease: ClusterRoleLease,
}

/// Result of one fenced role mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleMutation {
    /// The matching role fence was updated or removed.
    Applied,
    /// No row matched the supplied role fence.
    StaleFence,
}
