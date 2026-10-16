//! Trigger Card spec: the closed activation that creates a Verifier execution.
//!
//! A Trigger declares *when* a bound Verifier runs. It never names the
//! Verifier, an Operator, or a verdict threshold: the verification binding
//! that references it supplies the Verifier and its failure reactions, and the
//! Verifier's own implementation owns its thresholds. The activation is
//! flattened into the spec body so that an inline `runs_on` mapping and a
//! referenced Trigger Card's `spec` are byte-identical.

use serde::{Deserialize, Serialize};

/// Declarative activation for a bound Verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TriggerSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The single activation condition that creates an execution.
    #[serde(flatten)]
    pub activation: TriggerActivation,
}

/// The closed set of conditions that activate a bound Verifier.
///
/// `Schedule` is the only automatic activation. It is valid only for a
/// Verifier whose checks judge recorded samples; server registration enforces
/// that pairing once the effective Verifier is known. The activation itself
/// never names the Verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerActivation {
    /// Activate on each due occurrence of a cron schedule.
    Schedule {
        /// Cron expression.
        cron: String,
        /// Optional IANA timezone; absent means UTC.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    //! Trigger activation strictness: the flattened wire shape still decodes
    //! the authored forms, and every unknown sibling key is refused before the
    //! spec can reach validation or persistence.

    use super::{TriggerActivation, TriggerSpec};
    use crate::envelope::{CardKind, Spec};

    /// Decode the schedule activation and its optional description.
    ///
    /// # Panics
    /// Panics when the authored form fails to decode, or when the decoded
    /// activation or description is not the authored one.
    #[test]
    fn schedule_decodes_with_optional_description() {
        let spec: TriggerSpec = serde_json::from_value(serde_json::json!({
            "kind": "schedule",
            "cron": "0 * * * *",
            "description": "hourly",
        }))
        .expect("description decodes beside the activation");
        assert!(matches!(
            spec.activation,
            TriggerActivation::Schedule { .. }
        ));
        assert_eq!(spec.description.as_deref(), Some("hourly"));
    }

    /// Refuse the removed `observations_ready` activation.
    ///
    /// # Panics
    /// Panics when the removed activation decodes.
    #[test]
    fn observations_ready_is_not_an_activation() {
        assert!(
            serde_json::from_value::<TriggerSpec>(serde_json::json!({
                "kind": "observations_ready",
            }))
            .is_err()
        );
    }

    /// Refuse an unknown key beside the `schedule` activation's own fields.
    ///
    /// # Panics
    /// Panics when the misspelled sibling key is accepted, or when the refusal
    /// does not name it.
    #[test]
    fn schedule_rejects_unknown_fields() {
        let error = serde_json::from_value::<TriggerSpec>(serde_json::json!({
            "kind": "schedule",
            "cron": "0 * * * *",
            "threshhold": 0.2,
        }))
        .expect_err("a misspelled sibling key is refused");
        assert!(error.to_string().contains("threshhold"), "{error}");
    }

    /// Refuse the same unknown key when the activation arrives as a Trigger Card spec.
    ///
    /// Inline `runs_on` mappings and referenced Trigger Card bodies are the
    /// same wire shape, so both authoring forms must refuse identically.
    ///
    /// # Panics
    /// Panics when the unknown sibling key is accepted through the Trigger Card
    /// spec form, or when the refusal does not name it.
    #[test]
    fn trigger_card_spec_rejects_unknown_fields() {
        let error = Spec::from_kind_and_value(
            &CardKind::Trigger,
            serde_json::json!({
                "kind": "schedule",
                "cron": "0 * * * *",
                "api_token": "sk-live-not-a-real-token",
            }),
        )
        .expect_err("a Trigger Card spec refuses the same unknown key");
        assert!(error.to_string().contains("api_token"), "{error}");
    }
}
