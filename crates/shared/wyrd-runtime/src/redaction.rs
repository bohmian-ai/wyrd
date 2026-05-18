//! Redaction sink shell.

use wyrd_spec::redaction::{Redactable, RedactionPolicy};

/// Apply redaction to a value.
pub fn redact<T: Redactable>(value: &mut T, policy: &RedactionPolicy) {
    value.redact(policy);
}
