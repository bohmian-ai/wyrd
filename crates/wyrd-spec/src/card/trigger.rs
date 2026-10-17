//! Trigger Card spec: the closed activation that creates a Verifier execution.
//!
//! `schedule` runs a bound Verifier on cron boundaries; `group_end` runs a
//! group Verifier once a `group_id` group of source executions goes idle or
//! reaches its maximum age, under the Trigger's own [`TriggerDuration`]s.
//!
//! A Trigger declares *when* a bound Verifier runs. It never names the
//! Verifier, an Operator, or a verdict threshold: the verification binding
//! that references it supplies the Verifier and its failure reactions, and the
//! Verifier's own implementation owns its thresholds. The activation is
//! flattened into the spec body so that an inline `runs_on` mapping and a
//! referenced Trigger Card's `spec` are byte-identical.

use std::fmt;
use std::time::Duration;

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
/// `Schedule` is valid only for a Verifier whose checks judge recorded
/// samples, and `GroupEnd` only for a Verifier declaring source slots; server
/// registration enforces each pairing once the effective Verifier is known.
/// The activation itself never names the Verifier.
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
    /// Activate once per `group_id` group of source executions that has gone
    /// quiet or grown old, re-judging it serially as new evidence arrives.
    GroupEnd {
        /// The group ends once its newest source input or result row is
        /// older than this.
        idle: TriggerDuration,
        /// The group ends once its first source row is older than this, even
        /// while activity continues.
        max_age: TriggerDuration,
        /// Rows timed at or after the group's first source row plus this no
        /// longer reopen or expand the group; nothing is deleted.
        finality: TriggerDuration,
    },
}

/// A positive whole duration authored as `<n><unit>`, such as `30m`.
///
/// The unit is `s`, `m`, `h`, or `d` (seconds, minutes, hours, or 24-hour
/// days) and `n` is a positive integer without a leading zero. The authored
/// text is kept verbatim so a Trigger spec reads back and digests exactly as
/// written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub struct TriggerDuration {
    /// The authored text.
    text: String,
    /// The duration in whole seconds.
    seconds: u64,
}

/// Generated-schema pattern of a [`TriggerDuration`].
const DURATION_PATTERN: &str = "^[1-9][0-9]{0,8}[smhd]$";

/// Generated-schema description of a [`TriggerDuration`].
const DURATION_DESCRIPTION: &str =
    "Positive whole duration <n><unit> with unit s, m, h, or d, such as 30m or 24h.";

impl TriggerDuration {
    /// Parse authored `<n><unit>` text.
    ///
    /// # Errors
    /// Returns a message naming the accepted form when the text has no known
    /// unit, its count is zero, has a leading zero or sign, or exceeds nine
    /// digits.
    pub fn parse(text: &str) -> Result<Self, String> {
        let invalid = || format!("{text:?} is not a duration <n><s|m|h|d>, such as 30m");
        let split = text.len().checked_sub(1).ok_or_else(invalid)?;
        let (count, unit) = text.split_at_checked(split).ok_or_else(invalid)?;
        let scale = match unit {
            "s" => 1,
            "m" => 60,
            "h" => 3_600,
            "d" => 86_400,
            _ => return Err(invalid()),
        };
        if count.is_empty()
            || count.len() > 9
            || count.starts_with('0')
            || !count.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(invalid());
        }
        let count: u64 = count.parse().map_err(|_| invalid())?;
        Ok(Self {
            text: text.to_owned(),
            seconds: count * scale,
        })
    }

    /// The authored text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The duration in whole seconds.
    #[must_use]
    pub const fn seconds(&self) -> u64 {
        self.seconds
    }

    /// The duration as a standard [`Duration`].
    #[must_use]
    pub const fn as_duration(&self) -> Duration {
        Duration::from_secs(self.seconds)
    }
}

impl fmt::Display for TriggerDuration {
    /// Writes the authored text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl From<TriggerDuration> for String {
    /// Returns the authored text.
    fn from(value: TriggerDuration) -> Self {
        value.text
    }
}

impl<'de> Deserialize<'de> for TriggerDuration {
    /// Decodes and validates authored `<n><unit>` text.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for TriggerDuration {
    /// Names the duration schema in generated definitions.
    fn schema_name() -> String {
        "TriggerDuration".to_owned()
    }

    /// Describes a duration as a patterned string.
    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, Metadata, SchemaObject, StringValidation};

        SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some(DURATION_DESCRIPTION.to_owned()),
                ..Metadata::default()
            })),
            instance_type: Some(InstanceType::String.into()),
            string: Some(Box::new(StringValidation {
                pattern: Some(DURATION_PATTERN.to_owned()),
                ..StringValidation::default()
            })),
            ..SchemaObject::default()
        }
        .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for TriggerDuration {
    /// Describes a duration as a patterned string.
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .pattern(Some(DURATION_PATTERN))
            .description(Some(DURATION_DESCRIPTION))
            .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for TriggerDuration {}

#[cfg(test)]
mod tests {
    //! Trigger activation strictness: the flattened wire shape still decodes
    //! the authored forms, and every unknown sibling key is refused before the
    //! spec can reach validation or persistence.

    use super::{TriggerActivation, TriggerDuration, TriggerSpec};
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

    /// Decode a group-end activation with its three durations, read back
    /// verbatim, and refuse a missing finality or a malformed duration.
    ///
    /// # Panics
    /// Panics when the authored form fails to decode, reads back differently,
    /// or a malformed form decodes.
    #[test]
    fn group_end_decodes_its_durations() {
        let authored = serde_json::json!({
            "kind": "group_end",
            "idle": "30m",
            "max_age": "24h",
            "finality": "30d",
        });
        let spec: TriggerSpec =
            serde_json::from_value(authored.clone()).expect("group_end decodes");
        let TriggerActivation::GroupEnd {
            idle,
            max_age,
            finality,
        } = &spec.activation
        else {
            panic!("decoded {:?}", spec.activation);
        };
        assert_eq!(
            (idle.seconds(), max_age.seconds(), finality.seconds()),
            (1_800, 86_400, 2_592_000)
        );
        assert_eq!(serde_json::to_value(&spec).expect("encodes"), authored);
        assert!(
            serde_json::from_value::<TriggerSpec>(
                serde_json::json!({"kind": "group_end", "idle": "30m", "max_age": "24h"})
            )
            .is_err()
        );
        for text in [
            "",
            "m",
            "0m",
            "07m",
            "30",
            "30w",
            "-5s",
            "+5s",
            "1.5h",
            "1234567890s",
        ] {
            assert!(TriggerDuration::parse(text).is_err(), "{text:?}");
        }
        assert_eq!(
            TriggerDuration::parse("45s").expect("seconds").seconds(),
            45
        );
    }
}
