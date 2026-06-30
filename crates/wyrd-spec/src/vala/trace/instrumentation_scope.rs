//! OTel `InstrumentationScope` -- the SDK or library that produced the span.
//!
//! (Proto: `opentelemetry.proto.common.v1.InstrumentationScope`.)
//!
//! This is distinct from [`Resource`](crate::vala::trace::Resource). Resource
//! is "what entity is running"; scope is "what library inside that entity
//! emitted the span".

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

/// OTel-canonical instrumentation scope.
///
/// Fields:
///
/// - `name` -- instrumentation library name, such as `wyrd-tracing` or
///   `opentelemetry-instrumentation-requests`.
/// - `version` -- optional library version string.
/// - `attributes` -- rare scope-level attributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstrumentationScope {
    /// Instrumentation library name.
    pub name: String,
    /// Instrumentation library version, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Scope-level attribute bag.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

impl InstrumentationScope {
    /// Validate the scope-level invariants.
    ///
    /// Enforces:
    ///
    /// - `name` is non-empty and no longer than 256 characters.
    /// - `version` is no longer than 64 characters when present.
    /// - `attributes` contains no more than 32 entries.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the scope violates one of these
    /// invariants.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.name.is_empty() {
            return Err(WyrdError::Validation {
                message: "instrumentation_scope.name must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.name.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!(
                    "instrumentation_scope.name length must be <= 256, got {}",
                    self.name.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if let Some(v) = &self.version
            && v.len() > 64
        {
            return Err(WyrdError::Validation {
                message: format!(
                    "instrumentation_scope.version length must be <= 64, got {}",
                    v.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.attributes.len() > 32 {
            return Err(WyrdError::Validation {
                message: format!(
                    "instrumentation_scope.attributes count must be <= 32, got {}",
                    self.attributes.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        Ok(())
    }
}
