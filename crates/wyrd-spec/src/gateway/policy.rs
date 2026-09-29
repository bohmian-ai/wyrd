//! Tenant fallback, governance, and capture policy contracts.

use std::collections::{BTreeSet, HashSet};
use std::num::NonZeroU64;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    CurrencyCode, GatewayContractError, GatewayDecimal, GatewayOperation, ModelRef, validate_text,
};
use crate::auth::PrincipalId;
use crate::ids::{ProviderId, RoleName};

/// Where one fallback rule applies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum FallbackScope {
    /// Every call without a more specific rule.
    Global,
    /// Calls for one operation.
    Operation {
        /// Operation the rule applies to.
        operation: GatewayOperation,
    },
    /// Calls for one exact requested model.
    Model {
        /// Requested model the rule applies to.
        model: ModelRef,
    },
}

/// Ordered fallback candidates for one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct FallbackRule {
    /// Scope the rule applies to.
    pub scope: FallbackScope,
    /// Non-empty, ordered, duplicate-free candidates.
    pub candidates: Vec<ModelRef>,
}

/// Tenant fallback policy; at most one rule per scope, rules never merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayFallbackPolicy {
    /// Scoped rules.
    pub rules: Vec<FallbackRule>,
}

impl GatewayFallbackPolicy {
    /// Rejects duplicate scopes, empty or duplicated candidate lists, and a
    /// model-scoped rule that lists its own model.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming `rules[i]` or
    /// `rules[i].candidates` for the first violation.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        let mut scopes = HashSet::new();
        for (index, rule) in self.rules.iter().enumerate() {
            if !scopes.insert(&rule.scope) {
                return Err(GatewayContractError::new(
                    format!("rules[{index}].scope"),
                    "at most one rule may exist for each scope",
                ));
            }
            let field = format!("rules[{index}].candidates");
            validate_candidates(&field, &rule.candidates)?;
            if let FallbackScope::Model { model } = &rule.scope
                && rule.candidates.contains(model)
            {
                return Err(GatewayContractError::new(
                    field,
                    "a model-scoped rule cannot list its own model",
                ));
            }
        }
        Ok(())
    }
}

/// Per-request or per-workflow-step ordered fallback override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayFallbackOverride {
    /// Non-empty, ordered, duplicate-free candidates.
    pub candidates: Vec<ModelRef>,
}

impl GatewayFallbackOverride {
    /// Rejects an empty or duplicated list, or one containing `requested`.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `fallback.candidates`.
    pub fn validate_for(&self, requested: &ModelRef) -> Result<(), GatewayContractError> {
        validate_candidates("fallback.candidates", &self.candidates)?;
        if self.candidates.contains(requested) {
            return Err(GatewayContractError::new(
                "fallback.candidates",
                "an override cannot list the requested model",
            ));
        }
        Ok(())
    }
}

/// Rejects an empty or duplicated fallback candidate list.
///
/// # Errors
///
/// Returns [`GatewayContractError`] naming `field`.
fn validate_candidates(field: &str, candidates: &[ModelRef]) -> Result<(), GatewayContractError> {
    if candidates.is_empty() {
        return Err(GatewayContractError::new(field, "must not be empty"));
    }
    let mut seen = HashSet::new();
    if candidates.iter().any(|candidate| !seen.insert(candidate)) {
        return Err(GatewayContractError::new(field, "must not repeat a model"));
    }
    Ok(())
}

/// Principal set a budget applies to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayPolicySubject {
    /// The whole tenant.
    Tenant,
    /// One principal.
    Principal {
        /// Principal identity.
        principal_id: PrincipalId,
    },
    /// Every principal holding one role.
    Role {
        /// Role name.
        role_name: RoleName,
    },
}

/// Principal set a limit applies to; limits have no role subject.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayLimitSubject {
    /// The whole tenant.
    Tenant,
    /// One principal.
    Principal {
        /// Principal identity.
        principal_id: PrincipalId,
    },
}

/// Calls a limit applies to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayPolicyTarget {
    /// Every provider and model.
    All,
    /// One provider.
    Provider {
        /// Provider identity.
        provider: ProviderId,
    },
    /// One exact model.
    Model {
        /// Model identity.
        model: ModelRef,
    },
}

/// Rate and concurrency limit for one subject and target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayLimit {
    /// Limited principal set.
    pub subject: GatewayLimitSubject,
    /// Limited calls.
    pub target: GatewayPolicyTarget,
    /// Maximum admitted requests per minute.
    #[cfg_attr(feature = "server", schema(value_type = Option<u64>, minimum = 1))]
    pub requests_per_minute: Option<NonZeroU64>,
    /// Maximum tokens per minute.
    #[cfg_attr(feature = "server", schema(value_type = Option<u64>, minimum = 1))]
    pub tokens_per_minute: Option<NonZeroU64>,
    /// Maximum concurrently admitted calls.
    #[cfg_attr(feature = "server", schema(value_type = Option<u64>, minimum = 1))]
    pub concurrent_calls: Option<NonZeroU64>,
}

/// UTC calendar accounting period for a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GatewayBudgetPeriod {
    /// One UTC calendar day.
    CalendarDayUtc,
    /// One UTC calendar month.
    CalendarMonthUtc,
}

/// Admission of a call whose cost cannot be bounded.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UnknownCostPolicy {
    /// Reject the call.
    Reject,
    /// Admit it with an explicit unpriced outcome when no budget applies.
    #[default]
    AllowUnpriced,
}

/// Spending budget for one subject and period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayBudget {
    /// Budgeted principal set.
    pub subject: GatewayPolicySubject,
    /// Accounting period.
    pub period: GatewayBudgetPeriod,
    /// Positive budget amount.
    pub amount: GatewayDecimal,
    /// Budget currency.
    pub currency: CurrencyCode,
}

/// Price for one provider billing dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayPriceRate {
    /// Open provider billing dimension, such as `input_tokens`.
    pub dimension: String,
    /// Billing unit, such as `1m_tokens`.
    pub unit: String,
    /// Non-negative price per unit.
    pub price: GatewayDecimal,
}

/// One immutable pricing version for a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayModelPricing {
    /// Priced model.
    pub model: ModelRef,
    /// Immutable version label.
    pub version: String,
    /// Pricing currency.
    pub currency: CurrencyCode,
    /// Admission time from which this entry applies.
    pub effective_at: DateTime<Utc>,
    /// Whether the entry is eligible for newly admitted calls.
    pub active: bool,
    /// Non-empty rates.
    pub rates: Vec<GatewayPriceRate>,
}

impl GatewayModelPricing {
    /// True when `other` is the same immutable version with identical content
    /// apart from the `active` flag, which replacement may retire.
    #[must_use]
    pub fn same_version_content(&self, other: &Self) -> bool {
        self.model == other.model
            && self.version == other.version
            && self.currency == other.currency
            && self.effective_at == other.effective_at
            && self.rates == other.rates
    }
}

/// The tenant's single gateway governance policy.
///
/// The default is the empty policy: no limits or budgets, no active pricing,
/// and `unknown_cost: allow_unpriced`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayGovernancePolicy {
    /// Rate and concurrency limits.
    pub limits: Vec<GatewayLimit>,
    /// Spending budgets.
    pub budgets: Vec<GatewayBudget>,
    /// Versioned model pricing.
    pub pricing: Vec<GatewayModelPricing>,
    /// Unknown-cost admission.
    pub unknown_cost: UnknownCostPolicy,
}

impl GatewayGovernancePolicy {
    /// Rejects duplicate keys, empty limits or rates, non-positive budgets,
    /// invalid text, and mixed budget/active-pricing currencies.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming the first offending entry.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        let mut limits = HashSet::new();
        for (index, limit) in self.limits.iter().enumerate() {
            if !limits.insert((&limit.subject, &limit.target)) {
                return Err(GatewayContractError::new(
                    format!("limits[{index}]"),
                    "duplicate subject and target",
                ));
            }
            if limit.requests_per_minute.is_none()
                && limit.tokens_per_minute.is_none()
                && limit.concurrent_calls.is_none()
            {
                return Err(GatewayContractError::new(
                    format!("limits[{index}]"),
                    "at least one limit must be set",
                ));
            }
        }
        let mut currency: Option<&CurrencyCode> = None;
        let mut budgets = HashSet::new();
        for (index, budget) in self.budgets.iter().enumerate() {
            if !budgets.insert((&budget.subject, budget.period)) {
                return Err(GatewayContractError::new(
                    format!("budgets[{index}]"),
                    "duplicate subject and period",
                ));
            }
            if budget.amount.is_zero() {
                return Err(GatewayContractError::new(
                    format!("budgets[{index}].amount"),
                    "must be positive",
                ));
            }
            require_one_currency(
                &mut currency,
                &budget.currency,
                format!("budgets[{index}].currency"),
            )?;
        }
        let mut versions = HashSet::new();
        let mut effective = HashSet::new();
        for (index, entry) in self.pricing.iter().enumerate() {
            validate_text(&format!("pricing[{index}].version"), &entry.version)?;
            if !versions.insert((&entry.model, entry.version.as_str())) {
                return Err(GatewayContractError::new(
                    format!("pricing[{index}]"),
                    "duplicate model and version",
                ));
            }
            if !effective.insert((&entry.model, entry.effective_at)) {
                return Err(GatewayContractError::new(
                    format!("pricing[{index}]"),
                    "duplicate model and effective_at",
                ));
            }
            if entry.rates.is_empty() {
                return Err(GatewayContractError::new(
                    format!("pricing[{index}].rates"),
                    "must not be empty",
                ));
            }
            for (rate_index, rate) in entry.rates.iter().enumerate() {
                let field = format!("pricing[{index}].rates[{rate_index}]");
                validate_text(&format!("{field}.dimension"), &rate.dimension)?;
                validate_text(&format!("{field}.unit"), &rate.unit)?;
            }
            if entry.active {
                require_one_currency(
                    &mut currency,
                    &entry.currency,
                    format!("pricing[{index}].currency"),
                )?;
            }
        }
        Ok(())
    }
}

/// Records the first currency seen and rejects any later different one.
///
/// # Errors
///
/// Returns [`GatewayContractError`] naming `field` when `code` differs from
/// the currency already recorded in `seen`.
fn require_one_currency<'a>(
    seen: &mut Option<&'a CurrencyCode>,
    code: &'a CurrencyCode,
    field: String,
) -> Result<(), GatewayContractError> {
    match seen {
        Some(existing) if *existing != code => Err(GatewayContractError::new(
            field,
            "budgets and active pricing must use one currency",
        )),
        _ => {
            seen.get_or_insert(code);
            Ok(())
        }
    }
}

/// Gateway-call capture mode.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GatewayCaptureMode {
    /// Publish nothing to Bifrost.
    #[default]
    Disabled,
    /// Publish call metadata without request or response content.
    Metadata,
    /// Publish metadata plus the selected redacted payload fields.
    Payload,
}

/// Payload column selected for capture.
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
pub enum GatewayPayloadField {
    /// Redacted request payload.
    Request,
    /// Redacted response payload.
    Response,
}

/// `PUT /v1/admin/gateway/capture-policy` body.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayCapturePolicyWrite {
    /// Capture mode.
    pub mode: GatewayCaptureMode,
    /// Payload fields; empty unless `mode` is `payload`.
    pub payload_fields: BTreeSet<GatewayPayloadField>,
}

impl GatewayCapturePolicyWrite {
    /// Rejects payload fields outside `payload` mode and an empty `payload` set.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `payload_fields`.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        match (self.mode, self.payload_fields.is_empty()) {
            (GatewayCaptureMode::Payload, true) => Err(GatewayContractError::new(
                "payload_fields",
                "payload mode requires at least one field",
            )),
            (GatewayCaptureMode::Disabled | GatewayCaptureMode::Metadata, false) => Err(
                GatewayContractError::new("payload_fields", "must be empty unless mode is payload"),
            ),
            _ => Ok(()),
        }
    }
}

/// Persisted capture policy with its server-assigned version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayCapturePolicy {
    /// Capture mode.
    pub mode: GatewayCaptureMode,
    /// Selected payload fields.
    pub payload_fields: BTreeSet<GatewayPayloadField>,
    /// Version that increases only when effective content changes.
    #[cfg_attr(feature = "server", schema(value_type = u64, minimum = 1))]
    pub version: NonZeroU64,
}

#[cfg(test)]
mod tests {
    use super::{
        GatewayCapturePolicyWrite, GatewayFallbackOverride, GatewayFallbackPolicy,
        GatewayGovernancePolicy,
    };
    use crate::gateway::ModelRef;
    use serde_json::json;

    /// Builds a `ModelRef` JSON document.
    fn model(provider: &str, name: &str) -> serde_json::Value {
        json!({"provider": provider, "model": name})
    }

    /// Proves fallback duplicates, empties, and self-reference fail, while the
    /// exact shape round-trips.
    #[test]
    fn fallback_policy_rules_validate() {
        let valid = json!({"rules": [
            {"scope": "global", "candidates": [model("openai", "gpt-4o")]},
            {"scope": {"operation": {"operation": "embeddings"}}, "candidates": [model("gemini", "e")]},
            {"scope": {"model": {"model": model("openai", "gpt-4o")}}, "candidates": [model("anthropic", "claude")]},
        ]});
        let policy: GatewayFallbackPolicy = serde_json::from_value(valid.clone()).expect("decodes");
        policy.validate().expect("valid");
        assert_eq!(serde_json::to_value(&policy).expect("encodes"), valid);

        for rules in [
            json!([{"scope": "global", "candidates": []}]),
            json!([{"scope": "global", "candidates": [model("a-a", "x"), model("a-a", "x")]}]),
            json!([
                {"scope": "global", "candidates": [model("a-a", "x")]},
                {"scope": "global", "candidates": [model("b-b", "y")]}
            ]),
            json!([{"scope": {"model": {"model": model("a-a", "x")}}, "candidates": [model("a-a", "x")]}]),
        ] {
            let policy: GatewayFallbackPolicy =
                serde_json::from_value(json!({"rules": rules})).expect("decodes");
            assert!(policy.validate().is_err(), "{rules}");
        }

        let requested = ModelRef::from_projection("openai/gpt-4o").expect("model");
        let override_self: GatewayFallbackOverride =
            serde_json::from_value(json!({"candidates": [model("openai", "gpt-4o")]}))
                .expect("decodes");
        assert!(override_self.validate_for(&requested).is_err());
    }

    /// Proves governance duplicates, mixed currencies, zero budgets, and empty
    /// limits fail, and the default is the empty allow-unpriced policy.
    #[test]
    fn governance_policy_rules_validate() {
        assert_eq!(
            serde_json::to_value(GatewayGovernancePolicy::default()).expect("encodes"),
            json!({"limits": [], "budgets": [], "pricing": [], "unknown_cost": "allow_unpriced"})
        );
        let pricing = |version: &str, at: &str, currency: &str, active: bool| {
            json!({
                "model": model("openai", "gpt-4o"), "version": version, "currency": currency,
                "effective_at": at, "active": active,
                "rates": [{"dimension": "input_tokens", "unit": "1m_tokens", "price": "2.50"}],
            })
        };
        let budget = |subject: serde_json::Value, currency: &str, amount: &str| json!({"subject": subject, "period": "calendar_month_utc", "amount": amount, "currency": currency});
        let valid = json!({
            "limits": [{"subject": "tenant", "target": "all", "requests_per_minute": 60, "tokens_per_minute": null, "concurrent_calls": null}],
            "budgets": [budget(json!("tenant"), "USD", "100")],
            "pricing": [pricing("v1", "2026-01-01T00:00:00Z", "USD", true), pricing("v0", "2025-01-01T00:00:00Z", "EUR", false)],
            "unknown_cost": "reject",
        });
        let policy: GatewayGovernancePolicy = serde_json::from_value(valid).expect("decodes");
        policy
            .validate()
            .expect("valid; retired pricing may keep an old currency");

        let invalid = [
            json!({"budgets": [budget(json!("tenant"), "USD", "1"), budget(json!("tenant"), "USD", "2")]}),
            json!({"budgets": [budget(json!("tenant"), "USD", "0")]}),
            json!({"budgets": [budget(json!("tenant"), "USD", "1")], "pricing": [pricing("v1", "2026-01-01T00:00:00Z", "EUR", true)]}),
            json!({"pricing": [pricing("v1", "2026-01-01T00:00:00Z", "USD", true), pricing("v1", "2026-02-01T00:00:00Z", "USD", true)]}),
            json!({"pricing": [pricing("v1", "2026-01-01T00:00:00Z", "USD", true), pricing("v2", "2026-01-01T00:00:00Z", "USD", true)]}),
            json!({"limits": [{"subject": "tenant", "target": "all", "requests_per_minute": null, "tokens_per_minute": null, "concurrent_calls": null}]}),
        ];
        for mut case in invalid {
            for key in ["limits", "budgets", "pricing"] {
                case.as_object_mut()
                    .expect("object")
                    .entry(key)
                    .or_insert(json!([]));
            }
            case["unknown_cost"] = json!("allow_unpriced");
            let policy: GatewayGovernancePolicy =
                serde_json::from_value(case.clone()).expect("decodes");
            assert!(policy.validate().is_err(), "{case}");
        }
        assert!(
            serde_json::from_value::<GatewayGovernancePolicy>(json!({
                "limits": [{"subject": {"role": {"role_name": "viewer"}}, "target": "all", "requests_per_minute": 1, "tokens_per_minute": null, "concurrent_calls": null}],
                "budgets": [], "pricing": [], "unknown_cost": "reject",
            }))
            .is_err(),
            "limits have no role subject"
        );
    }

    /// Proves capture mode and payload-field combinations validate.
    #[test]
    fn capture_policy_field_combinations_validate() {
        for (mode, fields, ok) in [
            ("disabled", json!([]), true),
            ("metadata", json!([]), true),
            ("payload", json!(["response", "request"]), true),
            ("payload", json!([]), false),
            ("metadata", json!(["request"]), false),
        ] {
            let write: GatewayCapturePolicyWrite =
                serde_json::from_value(json!({"mode": mode, "payload_fields": fields}))
                    .expect("decodes");
            assert_eq!(write.validate().is_ok(), ok, "{mode} {fields}");
        }
    }
}
