//! DriftCard spec.
//!
//! Envelope locked per `architecture/wyrd-design.md` §Drift. `DriftProfile`
//! carries method-specific math config only; fitted baseline state lives in
//! `vala-drift`, never in `wyrd-spec`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::card::common::NonSecretValue;
use crate::envelope::CardKind;
use crate::error::WyrdError;
use crate::ids::FeatureName;
use crate::reference::CardRef;

/// DriftCard spec body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftSpec {
    /// Free-text description authored on the card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Drift detection method. Drives which `DriftProfile` variant is allowed.
    pub method: DriftMethod,

    /// Singular subject of the monitor: the entity drift is observed against.
    /// Allowed `subject_ref.kind`: `Model | Agent | Service | Data`.
    pub subject_ref: CardRef,

    /// How the measurement enters the monitor.
    pub signal: DriftSignal,

    /// When a sample becomes an emittable observation.
    pub condition: DriftCondition,

    /// Method-specific math configuration. Required for `Psi | Spc | Custom`,
    /// forbidden for `External`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DriftProfile>,

    /// Free-form non-secret authoring metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

/// Drift detection method.
///
/// `Agent` is intentionally absent in v1. It can return only after the signal
/// vocabulary, profile, and scoring algorithm are locked in `wyrd-design.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "PascalCase")]
pub enum DriftMethod {
    /// Population Stability Index over a baseline distribution.
    Psi,
    /// Statistical Process Control over a baseline distribution or scalar stream.
    Spc,
    /// User-defined metric with an author-supplied baseline value.
    Custom,
    /// Drift method defined externally, with no Vala-side fit or score.
    External,
}

/// How measurements enter the drift monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum DriftSignal {
    /// PSI or SPC over a baseline dataset.
    Distribution {
        /// Data card carrying the baseline artifact.
        baseline_ref: CardRef,
        /// Columns of the baseline DataCard that participate in the monitor.
        features: Vec<FeatureName>,
    },
    /// Named scalar emitted by the subject's runtime.
    Metric {
        /// Metric name, such as `p99_latency_ms`, `mae`, or `tokens_per_call`.
        name: String,
    },
    /// Score stream produced by an Eval card.
    EvalScore {
        /// Eval card whose score stream this drift consumes.
        eval_ref: CardRef,
    },
    /// Measurement from an external system, such as Prometheus or OTel.
    External {
        /// Source card describing where to read the external measurement.
        source_ref: CardRef,
    },
}

/// Fire condition for a drift observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum DriftCondition {
    /// The method's profile decides, such as PSI threshold or SPC zone.
    Statistical,
    /// Fires when sample > limit.
    Above {
        /// Upper exclusive limit.
        limit: f64,
    },
    /// Fires when sample < limit.
    Below {
        /// Lower exclusive limit.
        limit: f64,
    },
    /// Fires when sample < lower or sample > upper.
    Outside {
        /// Lower exclusive bound.
        lower: f64,
        /// Upper exclusive bound.
        upper: f64,
    },
}

/// Method-specific math configuration. Config only; fitted state lives in
/// `vala-drift::baseline`, never here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum DriftProfile {
    /// PSI profile configuration.
    Psi(PsiProfile),
    /// SPC profile configuration.
    Spc(SpcProfile),
    /// Custom profile configuration.
    Custom(CustomProfile),
}

/// PSI profile configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PsiProfile {
    /// Binning strategy applied to numeric features.
    pub binning_strategy: PsiBinningStrategy,

    /// Features treated as categorical regardless of dtype.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categorical_features: Vec<FeatureName>,

    /// PSI threshold strategy.
    pub threshold: PsiThreshold,
}

/// PSI numeric binning strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum PsiBinningStrategy {
    /// Equal-width bins across the baseline column range.
    EqualWidth {
        /// Number of bins.
        n_bins: u32,
    },
    /// Quantile bins from sorted baseline values.
    Quantile {
        /// Number of bins.
        n_bins: u32,
    },
}

/// PSI alert threshold strategy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum PsiThreshold {
    /// Chi-square approximation threshold.
    ChiSquare {
        /// Significance level.
        alpha: f64,
    },
    /// Normal approximation threshold.
    Normal {
        /// Significance level.
        alpha: f64,
    },
    /// Static threshold value.
    Fixed {
        /// Fixed PSI threshold.
        value: f64,
    },
}

/// SPC profile configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SpcProfile {
    /// Sample chunk size for baseline computation. `0` means adaptive default.
    pub sample_size: u32,

    /// WECO rule configuration.
    pub weco_rule: SpcWecoRule,

    /// Lowest zone that should produce a Drift verdict.
    pub alert_threshold: SpcAlertThreshold,
}

/// WECO rule string config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SpcWecoRule {
    /// Eight whitespace-separated positive `u32`s. Default:
    /// `"8 16 4 8 2 4 1 1"`.
    pub rule_string: String,
}

impl Default for SpcWecoRule {
    fn default() -> Self {
        Self {
            rule_string: "8 16 4 8 2 4 1 1".to_string(),
        }
    }
}

/// SPC alert threshold: the lowest zone that produces a Drift verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "PascalCase")]
pub enum SpcAlertThreshold {
    /// Zone 1.
    Zone1,
    /// Zone 2.
    Zone2,
    /// Zone 3.
    Zone3,
    /// Zone 4.
    Zone4,
}

/// Custom profile configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CustomProfile {
    /// Column or metric name read from the target record batch.
    pub metric_name: String,

    /// Author-supplied baseline value.
    pub baseline_value: f64,

    /// Threshold on `|mean(target) - baseline_value|` that triggers drift.
    pub alert_threshold: f64,
}

/// Locked DriftSpec validation error variants.
///
/// In-Rust error type, not a wire-contract type. Public boundaries receive
/// [`WyrdError`] through the [`From<DriftValidationError>`] implementation.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum DriftValidationError {
    /// `subject_ref.kind` was outside the allowed Drift subject set.
    #[error("subject_ref.kind must be Model | Agent | Service | Data, got {got}")]
    InvalidSubjectKind {
        /// Actual Card kind.
        got: String,
    },
    /// `signal` was not compatible with `method`.
    #[error("signal {signal} is not compatible with method {method}")]
    SignalMethodMismatch {
        /// Signal variant name.
        signal: String,
        /// Method variant name.
        method: String,
    },
    /// Distribution baseline did not reference a Data card.
    #[error("baseline_ref.kind must be Data, got {got}")]
    BaselineRefMustBeData {
        /// Actual Card kind.
        got: String,
    },
    /// Distribution signal had no feature columns.
    #[error("Distribution signal must declare at least one feature")]
    DistributionMissingFeatures,
    /// Distribution signal repeated a feature column.
    #[error("Distribution features contain duplicate {dup}")]
    DistributionDuplicateFeatures {
        /// Duplicated feature.
        dup: String,
    },
    /// EvalScore signal did not reference an Eval card.
    #[error("eval_ref.kind must be Eval, got {got}")]
    EvalRefMustBeEval {
        /// Actual Card kind.
        got: String,
    },
    /// External source referenced a Drift card.
    #[error("source_ref.kind must not be Drift")]
    SourceRefInvalidKind,
    /// A profile-bearing method had no profile.
    #[error("profile is required for method {method}")]
    ProfileRequired {
        /// Method variant name.
        method: String,
    },
    /// A profile variant did not match the selected method.
    #[error("profile variant {profile} does not match method {method}")]
    ProfileMethodMismatch {
        /// Profile variant name.
        profile: String,
        /// Method variant name.
        method: String,
    },
    /// External method supplied a forbidden profile.
    #[error("profile must be absent for External method")]
    ProfileForbiddenForExternal,
    /// Statistical condition was used without a profile.
    #[error("Statistical condition requires a profile; External method has none")]
    StatisticalRequiresProfile,
    /// Condition limit was not finite.
    #[error("condition limit must be finite: {field}")]
    NonFiniteLimit {
        /// Offending condition field.
        field: String,
    },
    /// Outside condition bounds were inverted.
    #[error("Outside condition requires lower < upper")]
    OutsideBoundsInverted,
    /// PSI alpha threshold was non-finite or outside `(0, 1)`.
    #[error("PSI threshold {field} must be finite and in (0, 1)")]
    PsiAlphaOutOfRange {
        /// Offending threshold field.
        field: String,
    },
    /// PSI fixed threshold was non-positive or non-finite.
    #[error("PSI fixed threshold must be positive and finite")]
    PsiFixedInvalid,
    /// PSI bin count was outside the allowed range.
    #[error("PSI bin count must be in [2, 1000]")]
    PsiBinCountOutOfRange,
    /// SPC sample size was not zero or at least two.
    #[error("SPC sample_size must be 0 (adaptive) or >= 2")]
    SpcSampleSizeOutOfRange,
    /// SPC WECO rule string was malformed.
    #[error("SPC weco rule string must be exactly eight whitespace-separated positive u32s")]
    SpcWecoMalformed,
    /// Custom profile metric name was empty.
    #[error("Custom profile metric_name must be non-empty")]
    CustomMetricNameEmpty,
    /// Custom profile numeric fields were invalid.
    #[error(
        "Custom profile baseline_value or alert_threshold not finite, or alert_threshold negative"
    )]
    CustomInvalidNumber,
}

impl DriftMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Psi => "Psi",
            Self::Spc => "Spc",
            Self::Custom => "Custom",
            Self::External => "External",
        }
    }
}

impl DriftSignal {
    fn variant_name(&self) -> &'static str {
        match self {
            Self::Distribution { .. } => "Distribution",
            Self::Metric { .. } => "Metric",
            Self::EvalScore { .. } => "EvalScore",
            Self::External { .. } => "External",
        }
    }
}

impl DriftProfile {
    fn variant_name(&self) -> &'static str {
        match self {
            Self::Psi(_) => "Psi",
            Self::Spc(_) => "Spc",
            Self::Custom(_) => "Custom",
        }
    }
}

impl DriftValidationError {
    fn details(&self) -> serde_json::Value {
        match self {
            Self::InvalidSubjectKind { got } => {
                json!({ "field": "subject_ref.kind", "got": got })
            }
            Self::SignalMethodMismatch { signal, method } => {
                json!({ "signal": signal, "method": method })
            }
            Self::BaselineRefMustBeData { got } => {
                json!({ "field": "signal.baseline_ref.kind", "got": got, "expected": "Data" })
            }
            Self::DistributionMissingFeatures => {
                json!({ "field": "signal.features", "reason": "missing" })
            }
            Self::DistributionDuplicateFeatures { dup } => {
                json!({ "field": "signal.features", "duplicate": dup })
            }
            Self::EvalRefMustBeEval { got } => {
                json!({ "field": "signal.eval_ref.kind", "got": got, "expected": "Eval" })
            }
            Self::SourceRefInvalidKind => {
                json!({ "field": "signal.source_ref.kind", "got": "Drift" })
            }
            Self::ProfileRequired { method } => json!({ "method": method }),
            Self::ProfileMethodMismatch { profile, method } => {
                json!({ "field": "profile.kind", "profile": profile, "method": method })
            }
            Self::ProfileForbiddenForExternal => {
                json!({ "field": "profile", "method": "External", "reason": "forbidden" })
            }
            Self::StatisticalRequiresProfile => {
                json!({ "field": "condition", "condition": "Statistical", "reason": "profile_required" })
            }
            Self::NonFiniteLimit { field } => json!({ "field": field }),
            Self::OutsideBoundsInverted => {
                json!({ "field": "condition", "reason": "lower_gte_upper" })
            }
            Self::PsiAlphaOutOfRange { field } => {
                json!({ "field": field, "expected": "finite value in (0, 1)" })
            }
            Self::PsiFixedInvalid => {
                json!({ "field": "profile.threshold.value", "expected": "positive finite value" })
            }
            Self::PsiBinCountOutOfRange => {
                json!({ "field": "profile.binning_strategy.n_bins", "min": 2, "max": 1000 })
            }
            Self::SpcSampleSizeOutOfRange => {
                json!({ "field": "profile.sample_size", "expected": "0 or >= 2" })
            }
            Self::SpcWecoMalformed => {
                json!({ "field": "profile.weco_rule.rule_string", "expected": "eight positive u32 values" })
            }
            Self::CustomMetricNameEmpty => {
                json!({ "field": "profile.metric_name", "reason": "empty" })
            }
            Self::CustomInvalidNumber => {
                json!({ "fields": ["profile.baseline_value", "profile.alert_threshold"], "expected": "finite values and alert_threshold >= 0" })
            }
        }
    }
}

impl From<DriftValidationError> for WyrdError {
    fn from(error: DriftValidationError) -> Self {
        let message = error.to_string();
        let details = error.details();
        match error {
            DriftValidationError::SignalMethodMismatch { .. } => {
                WyrdError::DriftSignalMethodMismatch { message, details }
            }
            DriftValidationError::ProfileRequired { .. } => {
                WyrdError::DriftProfileRequired { message, details }
            }
            _ => WyrdError::DriftValidation { message, details },
        }
    }
}

impl DriftSpec {
    /// Build a validated DriftSpec.
    ///
    /// # Errors
    /// Returns a [`DriftValidationError`] when any locked invariant fails.
    pub fn new(
        method: DriftMethod,
        subject_ref: CardRef,
        signal: DriftSignal,
        condition: DriftCondition,
        profile: Option<DriftProfile>,
        description: Option<String>,
        details: BTreeMap<String, NonSecretValue>,
    ) -> Result<Self, DriftValidationError> {
        let spec = Self {
            description,
            method,
            subject_ref,
            signal,
            condition,
            profile,
            details,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Run all locked validation invariants on this spec.
    ///
    /// # Errors
    /// Returns the first [`DriftValidationError`] discovered.
    pub fn validate(&self) -> Result<(), DriftValidationError> {
        validate_subject_ref(&self.subject_ref)?;
        validate_signal_method(&self.signal, self.method)?;
        validate_signal(&self.signal)?;
        validate_condition(&self.condition)?;
        validate_profile_presence(self.method, self.profile.as_ref())?;
        validate_statistical_requires_profile(&self.condition, self.profile.as_ref())?;
        if let Some(profile) = &self.profile {
            validate_profile(profile)?;
        }
        Ok(())
    }
}

fn validate_subject_ref(card_ref: &CardRef) -> Result<(), DriftValidationError> {
    match card_ref.kind {
        CardKind::Model | CardKind::Agent | CardKind::Service | CardKind::Data => Ok(()),
        _ => Err(DriftValidationError::InvalidSubjectKind {
            got: format!("{:?}", card_ref.kind),
        }),
    }
}

fn validate_signal_method(
    signal: &DriftSignal,
    method: DriftMethod,
) -> Result<(), DriftValidationError> {
    let allowed = match method {
        DriftMethod::Psi => matches!(signal, DriftSignal::Distribution { .. }),
        DriftMethod::Spc => matches!(
            signal,
            DriftSignal::Distribution { .. }
                | DriftSignal::Metric { .. }
                | DriftSignal::EvalScore { .. }
        ),
        DriftMethod::Custom => matches!(
            signal,
            DriftSignal::Metric { .. } | DriftSignal::EvalScore { .. }
        ),
        DriftMethod::External => matches!(signal, DriftSignal::External { .. }),
    };

    if allowed {
        Ok(())
    } else {
        Err(DriftValidationError::SignalMethodMismatch {
            signal: signal.variant_name().to_string(),
            method: method.name().to_string(),
        })
    }
}

fn validate_signal(signal: &DriftSignal) -> Result<(), DriftValidationError> {
    match signal {
        DriftSignal::Distribution {
            baseline_ref,
            features,
        } => {
            if baseline_ref.kind != CardKind::Data {
                return Err(DriftValidationError::BaselineRefMustBeData {
                    got: format!("{:?}", baseline_ref.kind),
                });
            }
            if features.is_empty() {
                return Err(DriftValidationError::DistributionMissingFeatures);
            }
            let mut seen = std::collections::BTreeSet::new();
            for feature in features {
                if !seen.insert(feature.as_str()) {
                    return Err(DriftValidationError::DistributionDuplicateFeatures {
                        dup: feature.to_string(),
                    });
                }
            }
            Ok(())
        }
        DriftSignal::Metric { .. } => Ok(()),
        DriftSignal::EvalScore { eval_ref } => {
            if eval_ref.kind != CardKind::Eval {
                return Err(DriftValidationError::EvalRefMustBeEval {
                    got: format!("{:?}", eval_ref.kind),
                });
            }
            Ok(())
        }
        DriftSignal::External { source_ref } => {
            if source_ref.kind == CardKind::Drift {
                return Err(DriftValidationError::SourceRefInvalidKind);
            }
            Ok(())
        }
    }
}

fn validate_condition(condition: &DriftCondition) -> Result<(), DriftValidationError> {
    match condition {
        DriftCondition::Statistical => Ok(()),
        DriftCondition::Above { limit } | DriftCondition::Below { limit } => {
            if limit.is_finite() {
                Ok(())
            } else {
                Err(DriftValidationError::NonFiniteLimit {
                    field: "limit".to_string(),
                })
            }
        }
        DriftCondition::Outside { lower, upper } => {
            if !lower.is_finite() {
                return Err(DriftValidationError::NonFiniteLimit {
                    field: "lower".to_string(),
                });
            }
            if !upper.is_finite() {
                return Err(DriftValidationError::NonFiniteLimit {
                    field: "upper".to_string(),
                });
            }
            if lower >= upper {
                return Err(DriftValidationError::OutsideBoundsInverted);
            }
            Ok(())
        }
    }
}

fn validate_profile_presence(
    method: DriftMethod,
    profile: Option<&DriftProfile>,
) -> Result<(), DriftValidationError> {
    match (method, profile) {
        (DriftMethod::External, Some(_)) => Err(DriftValidationError::ProfileForbiddenForExternal),
        (DriftMethod::External, None) => Ok(()),
        (method, None) => Err(DriftValidationError::ProfileRequired {
            method: method.name().to_string(),
        }),
        (method, Some(profile)) => {
            let matches = matches!(
                (method, profile),
                (DriftMethod::Psi, DriftProfile::Psi(_))
                    | (DriftMethod::Spc, DriftProfile::Spc(_))
                    | (DriftMethod::Custom, DriftProfile::Custom(_))
            );
            if matches {
                Ok(())
            } else {
                Err(DriftValidationError::ProfileMethodMismatch {
                    profile: profile.variant_name().to_string(),
                    method: method.name().to_string(),
                })
            }
        }
    }
}

fn validate_statistical_requires_profile(
    condition: &DriftCondition,
    profile: Option<&DriftProfile>,
) -> Result<(), DriftValidationError> {
    if matches!(condition, DriftCondition::Statistical) && profile.is_none() {
        Err(DriftValidationError::StatisticalRequiresProfile)
    } else {
        Ok(())
    }
}

fn validate_profile(profile: &DriftProfile) -> Result<(), DriftValidationError> {
    match profile {
        DriftProfile::Psi(profile) => validate_psi_profile(profile),
        DriftProfile::Spc(profile) => validate_spc_profile(profile),
        DriftProfile::Custom(profile) => validate_custom_profile(profile),
    }
}

fn validate_psi_profile(profile: &PsiProfile) -> Result<(), DriftValidationError> {
    match &profile.binning_strategy {
        PsiBinningStrategy::EqualWidth { n_bins } | PsiBinningStrategy::Quantile { n_bins } => {
            if !(2..=1000).contains(n_bins) {
                return Err(DriftValidationError::PsiBinCountOutOfRange);
            }
        }
    }

    match profile.threshold {
        PsiThreshold::ChiSquare { alpha } | PsiThreshold::Normal { alpha } => {
            if !alpha.is_finite() || alpha <= 0.0 || alpha >= 1.0 {
                return Err(DriftValidationError::PsiAlphaOutOfRange {
                    field: "alpha".to_string(),
                });
            }
        }
        PsiThreshold::Fixed { value } => {
            if !value.is_finite() || value <= 0.0 {
                return Err(DriftValidationError::PsiFixedInvalid);
            }
        }
    }
    Ok(())
}

fn validate_spc_profile(profile: &SpcProfile) -> Result<(), DriftValidationError> {
    if profile.sample_size == 1 {
        return Err(DriftValidationError::SpcSampleSizeOutOfRange);
    }

    if profile.weco_rule.rule_string.len() > 256 {
        return Err(DriftValidationError::SpcWecoMalformed);
    }

    let parts: Vec<&str> = profile.weco_rule.rule_string.split_whitespace().collect();
    if parts.len() != 8 {
        return Err(DriftValidationError::SpcWecoMalformed);
    }
    for part in parts {
        match part.parse::<u32>() {
            Ok(value) if value >= 1 => {}
            _ => return Err(DriftValidationError::SpcWecoMalformed),
        }
    }
    Ok(())
}

fn validate_custom_profile(profile: &CustomProfile) -> Result<(), DriftValidationError> {
    if profile.metric_name.trim().is_empty() {
        return Err(DriftValidationError::CustomMetricNameEmpty);
    }
    if !profile.baseline_value.is_finite()
        || !profile.alert_threshold.is_finite()
        || profile.alert_threshold < 0.0
    {
        return Err(DriftValidationError::CustomInvalidNumber);
    }
    Ok(())
}
