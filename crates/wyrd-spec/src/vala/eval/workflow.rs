//! Optional typed workflow declaration carried by the eval primitive surface.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Author-declared workflow field map for eval task inputs and outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    /// Map from workflow field name to the expected JSON shape at that key.
    pub fields: BTreeMap<String, WorkflowFieldType>,
}

/// Declared JSON shape for one workflow field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFieldType {
    /// String-shaped field.
    String,
    /// Integer-shaped field.
    Integer,
    /// Floating-point field.
    Float,
    /// Boolean-shaped field.
    Boolean,
    /// Object-shaped field.
    Object,
    /// Array-shaped field.
    Array,
    /// Field with no declared shape restriction.
    Any,
}
