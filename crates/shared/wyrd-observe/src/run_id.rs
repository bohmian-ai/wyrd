//! Run and observation identifiers for Wyrd-originated Vala records.

use std::fmt;

use serde::{Deserialize, Serialize};

pub use wyrd_spec::vala::ids::RunId;

/// Source of the run id for observer construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunIdSource {
    /// Caller already holds the run id.
    FromRequest(RunId),
    /// Generate a fresh id when the observer is constructed.
    NewSession,
}

impl RunIdSource {
    /// Resolve the source into a concrete run id.
    #[must_use]
    pub fn resolve(self) -> RunId {
        match self {
            Self::FromRequest(id) => id,
            Self::NewSession => RunId::new(),
        }
    }
}

/// Observation identifier generated per emitted Vala record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservationId(String);

impl ObservationId {
    /// Generate a fresh UUIDv7 observation identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(wyrd_utils::uuid7())
    }

    /// Borrow the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ObservationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ObservationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
