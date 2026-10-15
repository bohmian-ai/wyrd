//! The subject/actor pair every Wyrd work record carries.

use super::principal_id::{PLATFORM_AUDIT_PRINCIPAL, PrincipalId};

/// Who a unit of work was for and who performed it.
///
/// Follows RFC 8693's subject/current-actor split: `sub_principal_id` is the
/// principal the work serves, whose accepted authority and cost it carries;
/// `act_principal_id` is the principal that did it. A principal doing its own
/// work is both; a delegated token names its top-level principal as subject
/// and its outermost `act` as actor; Wyrd-owned execution names the tenant
/// SYSTEM principal as actor. The pair is attribution, never authority: it is
/// stamped by the server from verified identity and grants nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Attribution {
    /// The principal the work was for.
    pub sub_principal_id: PrincipalId,
    /// The principal that performed the work.
    pub act_principal_id: PrincipalId,
}

impl Attribution {
    /// The pair of `sub` served by `act`.
    #[must_use]
    pub const fn new(sub_principal_id: PrincipalId, act_principal_id: PrincipalId) -> Self {
        Self {
            sub_principal_id,
            act_principal_id,
        }
    }

    /// The pair of a principal doing its own work.
    #[must_use]
    pub const fn direct(principal: PrincipalId) -> Self {
        Self::new(principal, principal)
    }

    /// The no-verified-caller pair of a record no verified principal caused —
    /// an audited credential-exchange refusal that verified no identity, or a
    /// creation with no caller such as startup built-in reconciliation or
    /// SYSTEM bootstrap: [`PLATFORM_AUDIT_PRINCIPAL`] as both subject and
    /// actor. It is a marker, never SYSTEM and never a principal whose
    /// permissions authorize work.
    #[must_use]
    pub const fn unresolved() -> Self {
        Self::direct(PLATFORM_AUDIT_PRINCIPAL)
    }

    /// The same subject served by `actor`, such as the tenant SYSTEM
    /// principal executing work a request caused.
    #[must_use]
    pub const fn executed_by(self, actor: PrincipalId) -> Self {
        Self::new(self.sub_principal_id, actor)
    }
}
