//! Run and observation identifiers for Wyrd-originated Vala records.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Run identifier carried by every observation from one agent invocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(String);

impl RunId {
    /// Generate a fresh UUIDv7 run identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(wyrd_utils::uuid7())
    }

    /// Adopt a caller-supplied run identifier.
    #[must_use]
    pub fn from_string(value: String) -> Self {
        Self(value)
    }

    /// Borrow the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Deterministic bucket assignment for stable sampling decisions.
    ///
    /// The algorithm is stable across versions and platforms: compute Sha-256
    /// over the run id bytes, read the first eight digest bytes as a
    /// big-endian `u64`, then take modulo `buckets`. A pinned-value test locks
    /// this choice so sampling assignments do not drift.
    #[must_use]
    pub fn hash_bucket(&self, buckets: u32) -> u32 {
        if buckets == 0 {
            return 0;
        }
        let digest = Sha256::digest(self.0.as_bytes());
        let mut prefix = [0_u8; 8];
        prefix.copy_from_slice(&digest[..8]);
        let value = u64::from_be_bytes(prefix);
        (value % u64::from(buckets)) as u32
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

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
