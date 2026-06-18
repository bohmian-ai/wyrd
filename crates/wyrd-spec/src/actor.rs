//! Actor identity types.

use serde::{Deserialize, Serialize};

use crate::ids::CardUid;

/// Principal making a Wyrd request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Actor {
    /// Human user identity.
    User {
        /// User record UID.
        id: CardUid,
        /// Email address from identity provider.
        email: String,
    },
    /// Service-account identity.
    Service {
        /// Service account name (human-readable label).
        name: String,
        /// Stable opaque client identifier from verified claims — never a
        /// credential. Populated from the JWT `sub` or equivalent verified
        /// identity claim.
        client_id: String,
    },
    /// Agent identity.
    Agent {
        /// Agent card UID.
        id: CardUid,
        /// Principal that initiated this agent.
        parent_actor: Option<Box<Actor>>,
    },
}
