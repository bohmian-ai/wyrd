//! Versioned pricing selection and exact decimal cost arithmetic.
//!
//! The contract layer carries decimals as canonical text; this module owns
//! the arithmetic. Unknown pricing, an unpriced usage dimension, a unit
//! mismatch, or absent usage yields `None` — never zero.

use std::ops::Add;
use std::str::FromStr;

use bigdecimal::{BigDecimal, Zero};
use chrono::{DateTime, Utc};
use wyrd_spec::gateway::{
    CurrencyCode, GatewayContractError, GatewayDecimal, GatewayGovernancePolicy,
    GatewayModelPricing, GatewayUsageAmount, ModelRef,
};

/// Fractional digits kept in computed costs; longer fractions truncate.
const COST_SCALE: i64 = 12;

/// Exact non-negative monetary amount used for reservation and settlement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GatewayCost(BigDecimal);

impl GatewayCost {
    /// The zero amount.
    #[must_use]
    pub fn zero() -> Self {
        Self(BigDecimal::from(0))
    }

    /// Converts a contract decimal.
    #[must_use]
    pub fn from_decimal(value: &GatewayDecimal) -> Self {
        // A GatewayDecimal is always `digits[.digits]`, which BigDecimal parses.
        Self(BigDecimal::from_str(value.as_str()).unwrap_or_default())
    }

    /// Parses decimal text such as a Postgres `NUMERIC` rendered as text.
    ///
    /// # Errors
    ///
    /// Returns the contract error when `value` is not a non-negative decimal.
    pub fn parse(value: &str) -> Result<Self, GatewayContractError> {
        GatewayDecimal::new(value).map(|decimal| Self::from_decimal(&decimal))
    }

    /// True when the amount is exactly zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// `self - other`, floored at zero.
    #[must_use]
    pub fn saturating_sub(&self, other: &Self) -> Self {
        if other >= self {
            Self::zero()
        } else {
            Self(&self.0 - &other.0)
        }
    }

    /// Renders the canonical contract decimal, truncated to 12 fractional
    /// digits.
    ///
    /// # Panics
    ///
    /// Never for amounts built by this module: they are non-negative, so
    /// their plain rendering satisfies the decimal grammar.
    #[must_use]
    pub fn to_decimal(&self) -> GatewayDecimal {
        let text = self.0.with_scale(COST_SCALE).normalized().to_plain_string();
        GatewayDecimal::new(&text).expect("non-negative plain decimal text is a gateway decimal")
    }
}

impl Add for GatewayCost {
    type Output = Self;

    /// Exact decimal sum; costs never round, so settlement totals match the
    /// attempt costs they add.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::iter::Sum for GatewayCost {
    /// Adds every amount starting from zero, so an empty iterator totals zero.
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::zero(), Add::add)
    }
}

/// The pricing version that applies to one model at admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricedModel<'a> {
    /// Selected immutable pricing entry.
    pricing: &'a GatewayModelPricing,
}

impl<'a> PricedModel<'a> {
    /// Selects the active entry for `model` with the greatest `effective_at`
    /// not later than `admitted_at`.
    #[must_use]
    pub fn select(
        governance: &'a GatewayGovernancePolicy,
        model: &ModelRef,
        admitted_at: DateTime<Utc>,
    ) -> Option<Self> {
        governance
            .pricing
            .iter()
            .filter(|p| p.active && p.model == *model && p.effective_at <= admitted_at)
            .max_by_key(|p| p.effective_at)
            .map(|pricing| Self { pricing })
    }

    /// Immutable pricing version label.
    #[must_use]
    pub fn version(&self) -> &'a str {
        &self.pricing.version
    }

    /// Pricing currency.
    #[must_use]
    pub fn currency(&self) -> &'a CurrencyCode {
        &self.pricing.currency
    }

    /// Prices `usage`, or `None` when it is empty or any amount lacks a rate
    /// with the same dimension and a unit of `<n>_<usage unit>` or the usage
    /// unit itself, where `n` is `1k` or `1m`.
    #[must_use]
    pub fn cost(&self, usage: &[GatewayUsageAmount]) -> Option<GatewayCost> {
        if usage.is_empty() {
            return None;
        }
        usage
            .iter()
            .map(|amount| {
                self.pricing.rates.iter().find_map(|rate| {
                    let scale = unit_scale(&rate.unit, &amount.unit)?;
                    (rate.dimension == amount.dimension).then(|| {
                        GatewayCost(
                            GatewayCost::from_decimal(&amount.quantity).0
                                * GatewayCost::from_decimal(&rate.price).0
                                / BigDecimal::from(scale),
                        )
                    })
                })
            })
            .sum::<Option<GatewayCost>>()
    }
}

/// Divisor turning a usage quantity in `usage_unit` into billing `rate_unit`s.
fn unit_scale(rate_unit: &str, usage_unit: &str) -> Option<u64> {
    if rate_unit == usage_unit {
        return Some(1);
    }
    let (prefix, base) = rate_unit.split_once('_')?;
    (base == usage_unit).then_some(())?;
    match prefix {
        "1k" => Some(1_000),
        "1m" => Some(1_000_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Parses a governance document.
    fn governance(value: serde_json::Value) -> GatewayGovernancePolicy {
        serde_json::from_value(value).expect("governance decodes")
    }

    /// Builds usage amounts from `(dimension, unit, quantity)`.
    fn usage(items: &[(&str, &str, &str)]) -> Vec<GatewayUsageAmount> {
        items
            .iter()
            .map(|(dimension, unit, quantity)| GatewayUsageAmount {
                dimension: (*dimension).to_owned(),
                unit: (*unit).to_owned(),
                quantity: GatewayDecimal::new(quantity).expect("quantity"),
            })
            .collect()
    }

    /// The latest active effective entry prices usage exactly, and missing
    /// rates, unit mismatches, or absent usage stay unknown.
    #[test]
    fn pricing_selects_latest_active_entry_and_keeps_unknown_null() {
        let model = ModelRef::from_projection("openai/gpt-4o").expect("model");
        let policy = governance(json!({
            "limits": [], "budgets": [], "unknown_cost": "allow_unpriced",
            "pricing": [
                {"model": model, "version": "v1", "currency": "USD", "effective_at": "2026-01-01T00:00:00Z", "active": true,
                 "rates": [{"dimension": "input_tokens", "unit": "1m_tokens", "price": "2.5"}]},
                {"model": model, "version": "v2", "currency": "USD", "effective_at": "2026-06-01T00:00:00Z", "active": true,
                 "rates": [{"dimension": "input_tokens", "unit": "1m_tokens", "price": "3"},
                           {"dimension": "output_tokens", "unit": "1k_tokens", "price": "0.01"}]},
                {"model": model, "version": "v3", "currency": "USD", "effective_at": "2026-08-01T00:00:00Z", "active": false,
                 "rates": [{"dimension": "input_tokens", "unit": "1m_tokens", "price": "9"}]},
            ]
        }));
        let at = |value: &str| value.parse::<DateTime<Utc>>().expect("timestamp");

        assert_eq!(
            PricedModel::select(&policy, &model, at("2025-12-31T00:00:00Z")),
            None
        );
        let v1 = PricedModel::select(&policy, &model, at("2026-03-01T00:00:00Z")).expect("v1");
        assert_eq!(v1.version(), "v1");
        let v2 = PricedModel::select(&policy, &model, at("2026-09-01T00:00:00Z")).expect("v2");
        assert_eq!(v2.version(), "v2", "inactive v3 is never selected");

        let cost = v2
            .cost(&usage(&[
                ("input_tokens", "tokens", "1000"),
                ("output_tokens", "tokens", "500"),
            ]))
            .expect("priced");
        assert_eq!(cost.to_decimal().as_str(), "0.008");
        assert_eq!(v1.cost(&usage(&[("output_tokens", "tokens", "1")])), None);
        assert_eq!(
            v2.cost(&usage(&[("input_tokens", "characters", "1")])),
            None
        );
        assert_eq!(v2.cost(&[]), None);

        let total: GatewayCost = [cost.clone(), cost.clone()].into_iter().sum();
        assert_eq!(total.to_decimal().as_str(), "0.016");
        assert!(cost.saturating_sub(&total).is_zero());
        assert_eq!(total.saturating_sub(&cost), cost);
        assert_eq!(
            GatewayCost::parse("1.500000")
                .expect("parses")
                .to_decimal()
                .as_str(),
            "1.5"
        );
    }
}
