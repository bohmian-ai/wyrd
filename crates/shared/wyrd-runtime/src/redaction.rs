//! Redaction sink shell.

use wyrd_spec::redaction::{Redactable, RedactionPolicy};

/// Redaction sink installation plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactionSinkPlan {
    /// Number of field-pattern rules configured on the policy.
    pub field_pattern_count: usize,
    /// Number of JSON-path rules configured on the policy.
    pub json_path_count: usize,
}

/// Prepare redaction sink wiring for the supplied policy.
#[must_use]
pub fn sink(policy: &RedactionPolicy) -> RedactionSinkPlan {
    RedactionSinkPlan {
        field_pattern_count: policy.field_patterns.len(),
        json_path_count: policy.json_paths.len(),
    }
}

/// Apply redaction to a value.
pub fn redact<T: Redactable>(value: &mut T, policy: &RedactionPolicy) {
    value.redact(policy);
}
