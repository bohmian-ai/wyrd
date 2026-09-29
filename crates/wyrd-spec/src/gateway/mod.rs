//! Wyrd gateway V1 public contracts.
//!
//! These are the PyO3-free wire shapes shared by `wyrd-server`, `wyrd-client`,
//! the first-class SDKs, the CLI, MCP, and generated schemas for tenant gateway
//! administration, accounting, and capture. Provider and model identities and
//! billing dimensions are open validated values; operations, adapters,
//! upstream authentication, credential sources and states, fallback scopes,
//! budget periods, unknown-cost handling, capture modes, and payload fields
//! are closed enums that reject unknown variants at decode.
//!
//! Types here are declarative. Validation that spans several fields lives in
//! inherent `validate` methods so every surface rejects the same shapes;
//! persistence, authorization, and lifecycle stay in `wyrd-server`.

mod credential;
mod deployment;
pub mod native;
pub mod openai;
mod policy;
mod record;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

pub use credential::{
    ExternalSecretReference, ProviderCredentialSourceView, ProviderCredentialState,
    ProviderCredentialView, ProviderCredentialWrite, ProviderCredentialWriteSource,
};
pub use deployment::{
    ProviderAdapter, ProviderAuth, ProviderAuthHeader, ProviderDeployment, VertexLocation,
};
pub use policy::{
    FallbackRule, FallbackScope, GatewayBudget, GatewayBudgetPeriod, GatewayCaptureMode,
    GatewayCapturePolicy, GatewayCapturePolicyWrite, GatewayFallbackOverride,
    GatewayFallbackPolicy, GatewayGovernancePolicy, GatewayLimit, GatewayLimitSubject,
    GatewayModelPricing, GatewayPayloadField, GatewayPolicySubject, GatewayPolicyTarget,
    GatewayPriceRate, UnknownCostPolicy,
};
pub use record::{
    GATEWAY_JSON_MAX_BYTES, GatewayAccountingEntryId, GatewayAccountingEntryV1,
    GatewayAttemptSpanFieldsV1, GatewayBudgetReservationId, GatewayCallId, GatewayCallOutcome,
    GatewayCallPayloadV1, GatewayPayloadObjectRefV1, GatewayUsageAmount,
};

use crate::ids::{ModelId, ProviderId};

/// Longest accepted bounded gateway text value, such as a pricing version.
const MAX_TEXT_LEN: usize = 128;

/// Provider identifiers reserved for the built-in adapters.
pub const BUILTIN_PROVIDER_IDS: [&str; 4] = ["openai", "anthropic", "gemini", "vertex"];

/// Why one gateway contract value is not acceptable.
///
/// `field` is a JSON-pointer-like path (`rules[1].candidates`) naming the
/// offending value so every surface can report the same location; `reason`
/// is a closed, human-readable explanation that never echoes secret input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("gateway field {field} is invalid: {reason}")]
pub struct GatewayContractError {
    /// Path of the rejected value inside the submitted document.
    pub field: String,
    /// Stable explanation of the violated rule.
    pub reason: &'static str,
}

impl GatewayContractError {
    /// Builds one contract error for `field`.
    #[must_use]
    pub fn new(field: impl Into<String>, reason: &'static str) -> Self {
        Self {
            field: field.into(),
            reason,
        }
    }
}

/// Rejects an empty, over-long, or control-character-bearing text value.
///
/// # Errors
///
/// Returns [`GatewayContractError`] naming `field` when `value` is empty,
/// longer than 128 bytes, or contains a control character.
fn validate_text(field: &str, value: &str) -> Result<(), GatewayContractError> {
    if value.is_empty() || value.len() > MAX_TEXT_LEN || value.chars().any(char::is_control) {
        return Err(GatewayContractError::new(
            field,
            "must be 1..=128 bytes without control characters",
        ));
    }
    Ok(())
}

/// One exact provider-native model selected by a caller.
///
/// Typed surfaces carry the pair; the OpenAI-compatible `model` string is the
/// canonical `<provider>/<model>` projection, split on the first `/` so a
/// native model identifier may itself contain slashes.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    /// Provider that serves the model.
    pub provider: ProviderId,
    /// Provider-native model identifier.
    pub model: ModelId,
}

impl ModelRef {
    /// Parses the canonical `<provider>/<model>` projection.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `model` when the value has no
    /// `/`, or when either half is not a valid identity.
    pub fn from_projection(value: &str) -> Result<Self, GatewayContractError> {
        let (provider, model) = value
            .split_once('/')
            .ok_or_else(|| GatewayContractError::new("model", "must be <provider>/<model>"))?;
        Ok(Self {
            provider: ProviderId::new(provider)
                .map_err(|_| GatewayContractError::new("model", "provider is not a valid name"))?,
            model: ModelId::new(model)
                .map_err(|_| GatewayContractError::new("model", "model is not a valid model id"))?,
        })
    }
}

impl fmt::Display for ModelRef {
    /// Renders the canonical `<provider>/<model>` projection.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)
    }
}

/// Typed capability a caller requests from the gateway.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GatewayOperation {
    /// Chat Completions.
    ChatCompletions,
    /// Responses.
    Responses,
    /// Embeddings.
    Embeddings,
    /// Image generation, editing, and variations.
    Images,
    /// Speech, transcription, and translation.
    Audio,
    /// Batches, including the file operations batches require.
    Batches,
}

impl GatewayOperation {
    /// Whether a call of this operation may ask for an incremental
    /// server-sent event answer; every other operation answers buffered.
    #[must_use]
    pub const fn streams(self) -> bool {
        matches!(self, Self::ChatCompletions | Self::Responses)
    }
}

/// Non-negative decimal amount carried as canonical text.
///
/// V1 performs no arithmetic in the contract layer, so the value stays an
/// exact decimal string rather than a lossy float. Decoding normalizes
/// redundant leading integer zeros and trailing fractional zeros, so `01.50`
/// and `1.5` are the same amount and compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct GatewayDecimal(String);

impl GatewayDecimal {
    /// Builds a normalized non-negative decimal.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `decimal` when `value` is not
    /// `digits[.digits]` or exceeds 64 bytes.
    pub fn new(value: &str) -> Result<Self, GatewayContractError> {
        let invalid = || GatewayContractError::new("decimal", "must be a non-negative decimal");
        if value.is_empty() || value.len() > 64 {
            return Err(invalid());
        }
        let (integer, fraction) = value.split_once('.').unwrap_or((value, ""));
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if integer.is_empty() || !digits(integer) || !digits(fraction) {
            return Err(invalid());
        }
        if value.contains('.') && fraction.is_empty() {
            return Err(invalid());
        }
        let integer = integer.trim_start_matches('0');
        let fraction = fraction.trim_end_matches('0');
        let integer = if integer.is_empty() { "0" } else { integer };
        Ok(Self(if fraction.is_empty() {
            integer.to_owned()
        } else {
            format!("{integer}.{fraction}")
        }))
    }

    /// True when the amount is exactly zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == "0"
    }

    /// Borrows the normalized decimal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for GatewayDecimal {
    type Err = GatewayContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for GatewayDecimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

/// ISO-4217 alphabetic currency code, such as `USD`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    /// Builds a validated currency code.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `currency` unless `value` is
    /// exactly three uppercase ASCII letters.
    pub fn new(value: &str) -> Result<Self, GatewayContractError> {
        if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            Ok(Self(value.to_owned()))
        } else {
            Err(GatewayContractError::new(
                "currency",
                "must be an ISO-4217 code of three uppercase letters",
            ))
        }
    }

    /// Borrows the currency code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{CurrencyCode, GatewayDecimal, GatewayOperation, ModelRef};
    use serde_json::json;

    /// Proves the OpenAI-compatible projection splits on the first slash only.
    #[test]
    fn model_projection_splits_on_first_slash() {
        let model = ModelRef::from_projection("deepseek/org/model-v3").expect("projection parses");
        assert_eq!(model.provider.as_str(), "deepseek");
        assert_eq!(model.model.as_str(), "org/model-v3");
        assert_eq!(model.to_string(), "deepseek/org/model-v3");
        assert_eq!(
            serde_json::to_value(&model).expect("model serializes"),
            json!({"provider": "deepseek", "model": "org/model-v3"})
        );
        assert!(ModelRef::from_projection("no-slash").is_err());
        assert!(ModelRef::from_projection("Bad/model").is_err());
        assert!(ModelRef::from_projection("openai/").is_err());
    }

    /// Proves operations are a closed snake_case set.
    #[test]
    fn operations_are_closed() {
        assert_eq!(
            serde_json::to_value(GatewayOperation::ChatCompletions).expect("serializes"),
            json!("chat_completions")
        );
        assert!(serde_json::from_value::<GatewayOperation>(json!("rerank")).is_err());
    }

    /// Proves decimals normalize and reject non-decimal text.
    #[test]
    fn decimals_normalize_and_reject_invalid_text() {
        assert_eq!(
            GatewayDecimal::new("001.500").expect("valid").as_str(),
            "1.5"
        );
        assert_eq!(GatewayDecimal::new("0.000").expect("valid").as_str(), "0");
        assert!(GatewayDecimal::new("0").expect("valid").is_zero());
        for invalid in ["", "-1", "1.", ".5", "1e3", "1,5", "NaN"] {
            assert!(GatewayDecimal::new(invalid).is_err(), "{invalid}");
        }
        assert!(CurrencyCode::new("USD").is_ok());
        assert!(CurrencyCode::new("usd").is_err());
    }
}
