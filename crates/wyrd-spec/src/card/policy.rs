//! Policy Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::card::common::NonSecretValue;
use crate::envelope::{Card, Spec};
use crate::reference::CardRef;
use crate::request_id::RequestId;
use crate::run::{RunKind, RunRef};
use crate::trace::TraceContext;

/// Declarative policy rule set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PolicySpec {
    /// Policy description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Policy rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PolicyRule>,
    /// Enforcement mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement: Option<String>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

/// One policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PolicyRule {
    /// Rule name.
    pub name: String,
    /// Rule expression.
    pub expression: String,
    /// Action such as `allow`, `warn`, `block`, or `approval_required`.
    pub action: String,
    /// Rule metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

/// Invocation context passed to runtime Policy Cards.
// Status: Locked (2026-05-18)
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-5
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct InvokeContext {
    /// Invoked Card reference.
    pub card: CardRef,
    /// Kind of Run being produced.
    pub run_kind: RunKind,
    /// Principal that initiated the invocation.
    pub actor: Actor,
    /// Request correlation ID.
    pub request_id: RequestId,
    /// Trace context for the invocation.
    pub trace: TraceContext,
    /// Runtime invocation inputs; runtime phases validate schemas and redact before audit.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub inputs: serde_json::Value,
}

/// Invocation outcome passed to post-invoke Policy Cards.
// Status: Locked (2026-05-18)
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-5
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct InvokeOutcome {
    /// Whether the invocation succeeded.
    pub success: bool,
    /// Run reference produced by the invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_ref: Option<RunRef>,
    /// Machine-readable error code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Runtime outcome summary; runtime phases validate schemas and redact before audit.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub summary: serde_json::Value,
}

/// Policy evaluation decision.
// Status: Locked (2026-05-18)
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-5
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Permit the operation.
    Allow,
    /// Deny the operation with a human-readable reason.
    Deny {
        /// Denial reason.
        reason: String,
    },
}

impl PolicyDecision {
    /// Construct an allow decision.
    #[must_use]
    pub fn allow() -> Self {
        Self::Allow
    }

    /// Construct a deny decision.
    #[must_use]
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }
}

/// Runtime evaluation surface for Policy Cards.
// Status: Locked (2026-05-18)
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-5
pub trait PolicyCard {
    /// Evaluate a Card envelope and spec payload.
    fn evaluate(&self, card: &Card, spec: &Spec) -> PolicyDecision;

    /// Optional pre-invoke gate.
    fn pre_invoke(&self, _ctx: &InvokeContext) -> PolicyDecision {
        PolicyDecision::allow()
    }

    /// Optional post-invoke gate.
    fn post_invoke(&self, _ctx: &InvokeContext, _outcome: &InvokeOutcome) -> PolicyDecision {
        PolicyDecision::allow()
    }
}

#[cfg(test)]
mod spec_canonicalization_tests {
    use std::str::FromStr;

    use crate::card::policy::PolicySpec;
    use crate::envelope::{Spec, SpecHash, SpecHashParseError};

    #[test]
    fn test_spec_hash_round_trips_through_text_form() {
        let bytes = b"hello world";
        let h = SpecHash::from_canonical_bytes(bytes);
        let s = h.as_str();
        assert_eq!(s.len(), 64);
        let h2 = SpecHash::from_str(s).expect("valid hex");
        assert_eq!(h, h2);
    }

    #[test]
    fn test_spec_hash_rejects_invalid_hex_length() {
        let err = SpecHash::from_str("abc").unwrap_err();
        assert!(matches!(err, SpecHashParseError::InvalidLength { len: 3 }));

        let short = "a".repeat(63);
        let err2 = SpecHash::from_str(&short).unwrap_err();
        assert!(matches!(
            err2,
            SpecHashParseError::InvalidLength { len: 63 }
        ));
    }

    #[test]
    fn test_spec_hash_rejects_uppercase_hex_chars() {
        let upper = "A".repeat(64);
        let err = SpecHash::from_str(&upper).unwrap_err();
        assert!(matches!(err, SpecHashParseError::InvalidChar { pos: 0 }));
    }

    #[test]
    fn test_spec_hash_display_matches_as_str() {
        let bytes = b"test display";
        let h = SpecHash::from_canonical_bytes(bytes);
        assert_eq!(format!("{h}"), h.as_str());
    }

    #[test]
    fn canonical_hash_is_deterministic() {
        let spec = Spec::Policy(PolicySpec::default());
        let h1 = spec
            .canonical_hash()
            .expect("canonicalization must succeed");
        let h2 = spec
            .canonical_hash()
            .expect("canonicalization must succeed");
        assert_eq!(h1, h2);
        assert_eq!(h1.as_str().len(), 64);
    }

    #[test]
    fn canonical_hash_is_field_order_invariant() {
        let json_a = r#"{"description":"test","enforcement":"warn"}"#;
        let json_b = r#"{"enforcement":"warn","description":"test"}"#;
        let spec_a: PolicySpec = serde_json::from_str(json_a).expect("valid policy spec");
        let spec_b: PolicySpec = serde_json::from_str(json_b).expect("valid policy spec");
        let h_a = Spec::Policy(spec_a)
            .canonical_hash()
            .expect("canonicalization must succeed");
        let h_b = Spec::Policy(spec_b)
            .canonical_hash()
            .expect("canonicalization must succeed");
        assert_eq!(
            h_a, h_b,
            "JCS must produce the same hash regardless of field order"
        );
    }

    #[test]
    fn canonical_hash_changes_on_spec_mutation() {
        let spec_a = Spec::Policy(PolicySpec::default());
        let spec_b = Spec::Policy(PolicySpec {
            description: Some("mutated".to_owned()),
            ..Default::default()
        });
        let h_a = spec_a
            .canonical_hash()
            .expect("canonicalization must succeed");
        let h_b = spec_b
            .canonical_hash()
            .expect("canonicalization must succeed");
        assert_ne!(h_a, h_b, "mutating a field must change the hash");
    }
}
