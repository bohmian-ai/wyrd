//! Shared field and tensor-shape contracts for cards that describe typed data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::ColumnName;

/// Ordered field declaration used by Wyrd data schemas and model signatures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FieldSpec {
    /// Canonical column or feature name.
    pub name: ColumnName,
    /// Canonical Arrow logical dtype string.
    pub dtype: String,
    /// Optional tensor or nested value shape; empty means scalar/tabular.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<Dim>,
    /// Whether this field may contain null values.
    #[serde(default)]
    pub nullable: bool,
    /// Additional string metadata that does not change the Wyrd field contract.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl FieldSpec {
    /// Build a scalar, non-nullable field with no extra metadata.
    #[must_use]
    pub fn new(name: ColumnName, dtype: impl Into<String>) -> Self {
        Self {
            name,
            dtype: dtype.into(),
            shape: Vec::new(),
            nullable: false,
            extra: BTreeMap::new(),
        }
    }
}

/// One dimension in a field shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "value")]
pub enum Dim {
    /// A fixed, known dimension length.
    Fixed(i64),
    /// A dynamic dimension, optionally named for documentation and signatures.
    Dynamic(Option<String>),
}
