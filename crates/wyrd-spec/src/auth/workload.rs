//! Workload restriction and Card-access contracts.
//!
//! A Service account is a stable identity that outlives every release. Which
//! exact Service version a request runs as is execution context, not
//! account identity: the account's durable [`WorkloadRestriction`] limits which
//! roots it may select, and every tenant access token signs the resulting
//! [`CardAccess`] state beside — never inside — the account reference.

use serde::{Deserialize, Serialize};

use crate::envelope::CardKind;
use crate::ids::{CardName, CardUid};

/// The request header and gRPC metadata key that selects workload context for
/// a rootless bearer.
pub const WORKLOAD_UID_HEADER: &str = "x-wyrd-workload-uid";

/// The optional request header and gRPC metadata key that carries a configured
/// workload's deployment environment.
///
/// Descriptive invocation context beside [`WORKLOAD_UID_HEADER`], never
/// permission: it selects no principal, tenant, or grant.
pub const DEPLOYMENT_ENVIRONMENT_HEADER: &str = "x-wyrd-deployment-environment";

/// Parse a request's optional workload-context selector from every value it
/// sent under [`WORKLOAD_UID_HEADER`].
///
/// HTTP headers and gRPC metadata on every surface route through here, so a
/// selector is refused identically everywhere. It selects context only,
/// never a principal or tenant.
///
/// # Errors
/// Returns [`crate::error::WyrdError::WorkloadUidRefused`] when the selector is repeated or
/// is not a Card UID, the same refusal an unusable root earns.
pub fn workload_uid_selector<'a>(
    values: impl IntoIterator<Item = &'a [u8]>,
) -> Result<Option<CardUid>, crate::error::WyrdError> {
    let refused = || crate::error::WyrdError::WorkloadUidRefused {
        message: format!("{WORKLOAD_UID_HEADER} must be sent at most once as one Card UID"),
        details: serde_json::json!({ "header": WORKLOAD_UID_HEADER }),
    };
    let mut values = values.into_iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(refused());
    }
    std::str::from_utf8(value)
        .ok()
        .and_then(|value| CardUid::new(value).ok())
        .map(Some)
        .ok_or_else(refused)
}

/// Parse a request's optional deployment environment from every value it sent
/// under [`DEPLOYMENT_ENVIRONMENT_HEADER`].
///
/// HTTP headers and gRPC metadata on every surface route through here, so an
/// environment is refused identically everywhere. It is descriptive context
/// stamped on the request's rows, never permission.
///
/// # Errors
/// Returns [`crate::error::WyrdError::Validation`] naming the header when it
/// is repeated, empty, or not UTF-8.
pub fn deployment_environment_selector<'a>(
    values: impl IntoIterator<Item = &'a [u8]>,
) -> Result<Option<String>, crate::error::WyrdError> {
    let refused = || crate::error::WyrdError::Validation {
        message: format!(
            "{DEPLOYMENT_ENVIRONMENT_HEADER} must be sent at most once as one non-empty UTF-8 value"
        ),
        details: serde_json::json!({ "header": DEPLOYMENT_ENVIRONMENT_HEADER }),
    };
    let mut values = values.into_iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(refused());
    }
    std::str::from_utf8(value)
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| Some(value.to_owned()))
        .ok_or_else(refused)
}

/// Most Card UIDs one signed scope may carry.
///
/// Matches the graph resolver's membership bound, so every resolvable root
/// graph is representable and nothing larger is admitted.
pub const MAX_SCOPED_CARDS: usize = 32;

/// Card kind a workload family or root may name.
///
/// Workloads are Services only: an Agent is a registered subject inside a
/// Service graph, never a root or a restricting family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum WorkloadKind {
    /// A Service Card family.
    Service,
}

impl WorkloadKind {
    /// The Card kind this workload kind names.
    #[must_use]
    pub const fn card_kind(self) -> CardKind {
        match self {
            Self::Service => CardKind::Service,
        }
    }

    /// The workload kind of `kind`, when it may be a workload root.
    #[must_use]
    pub const fn from_card_kind(kind: &CardKind) -> Option<Self> {
        match kind {
            CardKind::Service => Some(Self::Service),
            _ => None,
        }
    }

    /// Stable wire label, matching the serde representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Service => "Service",
        }
    }
}

/// One tenant workload family: every version of one Service name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkloadFamily {
    /// Family Card kind.
    pub kind: WorkloadKind,
    /// Family Card name, resolved in the account's tenant.
    pub name: CardName,
}

impl WorkloadFamily {
    /// Whether the exact root `(kind, name)` belongs to this family.
    #[must_use]
    pub fn contains(&self, kind: &CardKind, name: &CardName) -> bool {
        self.kind.card_kind() == *kind && self.name == *name
    }
}

/// A Service account's durable workload restriction.
///
/// `unrestricted` adds no workload-family limit; it is not an operation grant
/// and never crosses tenants. `family` limits Card-attributed emission and
/// verification to roots of one family and is that family's unique scheduled
/// runtime account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum WorkloadRestriction {
    /// No workload-family limit.
    Unrestricted,
    /// Limited to roots of exactly one family.
    Family(WorkloadFamily),
}

impl WorkloadRestriction {
    /// The restricting family, when there is one.
    #[must_use]
    pub const fn family(&self) -> Option<&WorkloadFamily> {
        match self {
            Self::Unrestricted => None,
            Self::Family(family) => Some(family),
        }
    }

    /// Whether a root `(kind, name)` is allowed under this restriction.
    #[must_use]
    pub fn allows(&self, kind: &CardKind, name: &CardName) -> bool {
        self.family()
            .is_none_or(|family| family.contains(kind, name))
    }
}

impl From<Option<WorkloadFamily>> for WorkloadRestriction {
    /// A stored restricting family, or its absence, as the explicit
    /// restriction every Service account reports.
    fn from(family: Option<WorkloadFamily>) -> Self {
        family.map_or(Self::Unrestricted, Self::Family)
    }
}

/// The family restriction carried by a `root_required` Card-access state.
///
/// A one-variant union so the signed claim keeps the exact
/// `{"mode":"family",…}` restriction spelling while being unable to name
/// `unrestricted`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum FamilyRestriction {
    /// The account's restricting family.
    Family(WorkloadFamily),
}

impl FamilyRestriction {
    /// The restricting family.
    #[must_use]
    pub const fn family(&self) -> &WorkloadFamily {
        let Self::Family(family) = self;
        family
    }
}

/// The signed limit on which Cards a token may attribute evidence to.
///
/// Independent of `permissions`: an operation needs its permission and, when
/// it is Card-attributed, this limit. Neither dimension widens the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum CardAccess {
    /// Any registered observation-target Card in the tenant; the registry still
    /// validates each Card. Rootless Users, administrators, and unrestricted
    /// Service accounts.
    AnyRegistered,
    /// A family-restricted account that has selected no root: generic
    /// authorized work only, no Card-attributed authority until a root of the
    /// family is validated.
    RootRequired {
        /// The account's restriction snapshot.
        restriction: FamilyRestriction,
    },
    /// Exactly these Card UIDs. Direct scopes include their root; a delegated
    /// intersection may omit roots or be empty, and then authorizes no Card.
    Scoped {
        /// Authoritative graph UIDs from the server's graph resolver.
        cards: Vec<CardUid>,
    },
}

impl CardAccess {
    /// The intersection of two Card limits, for delegation.
    ///
    /// `any_registered` is the identity; two scopes intersect by UID in `self`
    /// order. Returns `None` when either side is `root_required`: an
    /// unresolved restricted account cannot delegate Card authority it has not
    /// established.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::RootRequired { .. }, _) | (_, Self::RootRequired { .. }) => None,
            (Self::AnyRegistered, any) | (any, Self::AnyRegistered) => Some(any.clone()),
            (Self::Scoped { cards }, Self::Scoped { cards: theirs }) => Some(Self::Scoped {
                cards: cards
                    .iter()
                    .filter(|card| theirs.contains(card))
                    .cloned()
                    .collect(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CardAccess, FamilyRestriction, WorkloadFamily, WorkloadKind, WorkloadRestriction};
    use crate::envelope::CardKind;
    use crate::ids::{CardName, CardUid};

    fn family() -> WorkloadFamily {
        WorkloadFamily {
            kind: WorkloadKind::Service,
            name: CardName::new("checkout").expect("static name"),
        }
    }

    fn uid() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7")
    }

    /// The wire spellings are exactly the supplement's closed unions.
    ///
    /// # Panics
    /// Panics when a shape serializes or parses differently.
    #[test]
    fn restriction_and_access_use_closed_wire_unions() {
        assert_eq!(
            serde_json::to_value(WorkloadRestriction::Unrestricted).expect("serializes"),
            json!({"mode": "unrestricted"})
        );
        assert_eq!(
            serde_json::to_value(WorkloadRestriction::Family(family())).expect("serializes"),
            json!({"mode": "family", "kind": "Service", "name": "checkout"})
        );
        assert_eq!(
            serde_json::to_value(CardAccess::RootRequired {
                restriction: FamilyRestriction::Family(family()),
            })
            .expect("serializes"),
            json!({"mode": "root_required", "restriction": {"mode": "family", "kind": "Service", "name": "checkout"}})
        );
        let card = uid();
        assert_eq!(
            serde_json::to_value(CardAccess::Scoped {
                cards: vec![card.clone()]
            })
            .expect("serializes"),
            json!({"mode": "scoped", "cards": [card.as_str()]})
        );
        for refused in [
            json!({"mode": "root_required", "restriction": {"mode": "unrestricted"}}),
            json!({"mode": "scoped"}),
            json!({"mode": "unrestricted"}),
        ] {
            assert!(
                serde_json::from_value::<CardAccess>(refused.clone()).is_err(),
                "{refused} must not parse as Card access"
            );
        }
        for kind in ["Model", "Agent"] {
            assert!(
                serde_json::from_value::<WorkloadRestriction>(
                    json!({"mode": "family", "kind": kind, "name": "m"})
                )
                .is_err(),
                "{kind} is not a workload family kind"
            );
        }
        assert_eq!(WorkloadKind::from_card_kind(&CardKind::Agent), None);
    }

    /// Delegation intersects scopes by UID, treats any-registered as identity,
    /// and refuses an unresolved restricted side.
    ///
    /// # Panics
    /// Panics when an intersection widens, drops shared members, or accepts
    /// `root_required`.
    #[test]
    fn intersection_is_exact_and_refuses_unresolved_roots() {
        let (a, b, c) = (uid(), uid(), uid());
        let left = CardAccess::Scoped {
            cards: vec![a.clone(), b.clone()],
        };
        let right = CardAccess::Scoped {
            cards: vec![b.clone(), c],
        };
        assert_eq!(
            left.intersection(&right),
            Some(CardAccess::Scoped { cards: vec![b] })
        );
        assert_eq!(
            CardAccess::AnyRegistered.intersection(&left),
            Some(left.clone())
        );
        assert_eq!(
            left.intersection(&CardAccess::Scoped { cards: vec![] }),
            Some(CardAccess::Scoped { cards: vec![] })
        );
        let unresolved = CardAccess::RootRequired {
            restriction: FamilyRestriction::Family(family()),
        };
        assert_eq!(unresolved.intersection(&left), None);
        assert_eq!(left.intersection(&unresolved), None);
        assert!(WorkloadRestriction::Family(family()).allows(
            &CardKind::Service,
            &CardName::new("checkout").expect("name")
        ));
        assert!(
            !WorkloadRestriction::Family(family())
                .allows(&CardKind::Agent, &CardName::new("checkout").expect("name"))
        );
        assert!(
            WorkloadRestriction::Unrestricted
                .allows(&CardKind::Agent, &CardName::new("anything").expect("name"))
        );
    }

    /// No selector is rootless; one Card UID selects it; a repeated or
    /// malformed selector is refused like an unusable root.
    ///
    /// # Panics
    /// Panics when an absent selector is not rootless, one UID is not selected
    /// verbatim, or a repeated, non-UID, or non-UTF-8 selector is not refused
    /// as `WorkloadUidRefused`.
    #[test]
    fn workload_uid_selector_accepts_one_card_uid() {
        let uid: &[u8] = b"01890a5d-ac96-774b-bcce-b302099a8057";
        assert_eq!(super::workload_uid_selector([]).expect("absent"), None);
        assert_eq!(
            super::workload_uid_selector([uid])
                .expect("one uid")
                .map(|uid| uid.to_string()),
            Some("01890a5d-ac96-774b-bcce-b302099a8057".to_owned())
        );
        for values in [
            vec![uid, uid],
            vec![b"not-a-uid".as_slice()],
            vec![b"\xff".as_slice()],
        ] {
            assert!(matches!(
                super::workload_uid_selector(values),
                Err(crate::error::WyrdError::WorkloadUidRefused { .. })
            ));
        }
    }

    /// No environment is unknown; one non-empty UTF-8 value is kept verbatim;
    /// a repeated, empty, or non-UTF-8 value refuses the request.
    ///
    /// # Panics
    /// Panics when an absent environment is not `None`, one value is not kept
    /// verbatim, or a malformed value is not refused as `Validation`.
    #[test]
    fn deployment_environment_selector_accepts_one_value() {
        assert_eq!(
            super::deployment_environment_selector([]).expect("absent"),
            None
        );
        assert_eq!(
            super::deployment_environment_selector([b"prod".as_slice()]).expect("one value"),
            Some("prod".to_owned())
        );
        for values in [
            vec![b"prod".as_slice(), b"prod".as_slice()],
            vec![b"".as_slice()],
            vec![b"\xff".as_slice()],
        ] {
            assert!(matches!(
                super::deployment_environment_selector(values),
                Err(crate::error::WyrdError::Validation { .. })
            ));
        }
    }
}
