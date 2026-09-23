//! Trigger Card spec: the closed activation that creates a Verifier run.
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
    /// The single activation condition that creates a run.
    #[serde(flatten)]
    pub activation: TriggerActivation,
}

/// The closed set of conditions that activate a bound Verifier.
///
/// `Schedule` is valid only for a Drift-backed Verifier and
/// `ObservationsReady` only for an Eval-backed Verifier. Server registration
/// enforces that pairing once the effective Verifier's implementation is
/// known; the activation itself never names the Verifier.
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
    /// Activate on each successfully committed observation for the subject.
    ///
    /// Declared as a fieldless struct variant so the enum-level
    /// `deny_unknown_fields` applies: a unit variant's internally tagged
    /// visitor would silently drain every sibling key, admitting typos and
    /// pasted secrets. The authored wire shape stays `{kind: observations_ready}`.
    ObservationsReady {},
}

#[cfg(test)]
mod tests {
    //! Trigger activation strictness: the flattened wire shape still decodes
    //! the authored forms, and every unknown sibling key is refused before the
    //! spec can reach validation or persistence.

    use super::{TriggerActivation, TriggerSpec};
    use crate::envelope::{CardKind, Spec};

    /// Decode the fieldless activation and its optional description.
    ///
    /// # Panics
    /// Panics when either authored form fails to decode, or when the decoded
    /// activation or description is not the authored one.
    #[test]
    fn observations_ready_decodes_with_optional_description() {
        let spec: TriggerSpec =
            serde_json::from_value(serde_json::json!({ "kind": "observations_ready" }))
                .expect("the bare activation decodes");
        assert!(matches!(
            spec.activation,
            TriggerActivation::ObservationsReady { .. }
        ));
        assert_eq!(spec.description, None);

        let described: TriggerSpec = serde_json::from_value(serde_json::json!({
            "kind": "observations_ready",
            "description": "after each committed observation",
        }))
        .expect("description decodes beside the activation");
        assert_eq!(
            described.description.as_deref(),
            Some("after each committed observation")
        );
    }

    /// Refuse an unknown or secret-shaped key beside `observations_ready`.
    ///
    /// A unit variant's internally tagged visitor would drain these silently,
    /// so the server would re-serialize the typed spec with the extra content
    /// dropped and no diagnostic.
    ///
    /// # Panics
    /// Panics when the unknown sibling key is accepted, or when the refusal
    /// does not name it.
    #[test]
    fn observations_ready_rejects_unknown_fields() {
        let error = serde_json::from_value::<TriggerSpec>(serde_json::json!({
            "kind": "observations_ready",
            "api_token": "sk-live-not-a-real-token",
        }))
        .expect_err("an unknown sibling key is refused");
        assert!(error.to_string().contains("api_token"), "{error}");
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
                "kind": "observations_ready",
                "api_token": "sk-live-not-a-real-token",
            }),
        )
        .expect_err("a Trigger Card spec refuses the same unknown key");
        assert!(error.to_string().contains("api_token"), "{error}");
    }
}
