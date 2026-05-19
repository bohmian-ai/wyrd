//! Declarative redaction policy.

use serde::{Deserialize, Serialize};

/// Configuration for redacting sensitive values before logging or auditing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RedactionPolicy {
    /// Case-insensitive field-name patterns.
    #[serde(default)]
    pub field_patterns: Vec<String>,
    /// JSON path rules applied to serialized payloads.
    #[serde(default)]
    pub json_paths: Vec<String>,
}

/// Type that can redact itself according to a policy.
pub trait Redactable {
    /// Apply redaction in place.
    fn redact(&mut self, policy: &RedactionPolicy);
}
