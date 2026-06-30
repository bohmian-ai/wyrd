//! Public Bifrost wire contracts — table management, query, and ingest types.
//!
//! This module is the C2 landing zone for the Bifrost HTTP register/insert wire
//! types. At Stage-3 C0 it holds only [`BifrostTableName`], which is also reused
//! by the observation contract in [`crate::vala::observation`].

use serde::{Deserialize, Serialize};

/// Bifrost table-identifier newtype.
///
/// The canonical table name as it appears in the Iceberg catalog and on the
/// wire. Reused by the observation contract and the C2 register/insert wire
/// types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct BifrostTableName(String);

impl BifrostTableName {
    /// Wraps a string as a [`BifrostTableName`].
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the table name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BifrostTableName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
