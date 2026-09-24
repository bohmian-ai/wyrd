//! The typed spec body of a `drift` Verifier implementation.
//!
//! Envelope locked per `architecture/wyrd-design.md` §Drift. `DriftProfile`
//! carries method-specific math config only; fitted baseline state lives in
//! `vala-drift`, never in `wyrd-spec`.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::envelope::CardKind;
use crate::error::WyrdError;
use crate::ids::FeatureName;
use crate::reference::Ref;

/// Drift Verifier implementation spec body.
///
/// Reached only as `implementation.spec` on a `Verifier` Card; `drift` is an
/// implementation discriminator, never an envelope `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DriftSpec {
    /// Free-text description authored on the card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Drift detection method. Drives which `DriftProfile` variant is allowed.
    pub method: DriftMethod,

    /// How the measurement enters the monitor.
    pub signal: DriftSignal,

    /// When a sample becomes an emittable observation.
    pub condition: DriftCondition,

    /// Method-specific math configuration. Required for every executable
    /// method/signal pair and must match `method`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DriftProfile>,
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
}

/// How measurements enter the drift monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum DriftSignal {
    /// PSI or SPC over a baseline dataset.
    Distribution {
        /// Data card carrying the baseline artifact.
        baseline_ref: Ref,
        /// Columns of the baseline DataCard that participate in the monitor.
        features: Vec<FeatureName>,
    },
    /// Named scalar emitted by the subject's runtime.
    Metric {
        /// Metric name, such as `p99_latency_ms`, `mae`, or `tokens_per_call`.
        name: String,
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

/// SPC profile configuration: a two-sided, three-sigma Shewhart X-bar/S chart.
///
/// Consecutive rows, ordered by `created_at` then `record_id`, form rational
/// subgroups of exactly `sample_size` rows. The author is responsible for
/// supplying baseline rows in process order from a stable process and for a
/// size whose consecutive rows form meaningful subgroups; Wyrd infers neither
/// process context nor subgroup boundaries. The chart owns its control limits,
/// so no rule or threshold is authored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SpcProfile {
    /// Fixed rational subgroup size; at least two.
    pub sample_size: u32,
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
    /// A non-`Statistical` condition was authored.
    ///
    /// `Above`, `Below`, and `Outside` remain typed vocabulary, but the three
    /// executable method/signal pairs already own their thresholds through
    /// their profile, so registration rejects them.
    #[error("condition {condition} is not supported; use Statistical")]
    ConditionNotStatistical {
        /// Rejected condition variant name.
        condition: String,
    },
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
    /// SPC subgroup size was below two.
    #[error("SPC sample_size must be >= 2")]
    SpcSampleSizeOutOfRange,
    /// Custom profile metric name was empty.
    #[error("Custom profile metric_name must be non-empty")]
    CustomMetricNameEmpty,
    /// Custom profile numeric fields were invalid.
    #[error(
        "Custom profile baseline_value or alert_threshold not finite, or alert_threshold negative"
    )]
    CustomInvalidNumber,
    /// Custom profile `metric_name` did not name the `Metric` signal.
    ///
    /// The run reads exactly one observed `series`; two differing names would
    /// leave it ambiguous which one the Verifier judges.
    #[error("Custom profile metric_name {profile} does not match signal name {signal}")]
    CustomMetricNameMismatch {
        /// Authored `signal.name`.
        signal: String,
        /// Authored `profile.metric_name`.
        profile: String,
    },
    /// Custom metric name is not a valid feature name.
    ///
    /// Observations store each metric as a `FeatureName` series, so a name
    /// outside that grammar could never match a stored row.
    #[error("Custom metric name {name} is not a valid feature name")]
    CustomMetricNameInvalid {
        /// Rejected metric name.
        name: String,
    },
    /// A PSI categorical feature is not one of the signal's features.
    #[error("PSI categorical feature {feature} is not a signal feature")]
    PsiCategoricalFeatureUnknown {
        /// Categorical feature absent from `signal.features`.
        feature: String,
    },
}

impl DriftMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Psi => "Psi",
            Self::Spc => "Spc",
            Self::Custom => "Custom",
        }
    }
}

impl DriftSignal {
    fn variant_name(&self) -> &'static str {
        match self {
            Self::Distribution { .. } => "Distribution",
            Self::Metric { .. } => "Metric",
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
            Self::ProfileRequired { method } => json!({ "method": method }),
            Self::ProfileMethodMismatch { profile, method } => {
                json!({ "field": "profile.kind", "profile": profile, "method": method })
            }
            Self::ConditionNotStatistical { condition } => {
                json!({ "field": "condition", "got": condition, "expected": "Statistical" })
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
                json!({ "field": "profile.sample_size", "expected": ">= 2" })
            }
            Self::CustomMetricNameEmpty => {
                json!({ "field": "profile.metric_name", "reason": "empty" })
            }
            Self::CustomInvalidNumber => {
                json!({ "fields": ["profile.baseline_value", "profile.alert_threshold"], "expected": "finite values and alert_threshold >= 0" })
            }
            Self::CustomMetricNameMismatch { signal, profile } => {
                json!({ "fields": ["signal.name", "profile.metric_name"], "signal": signal, "profile": profile })
            }
            Self::CustomMetricNameInvalid { name } => {
                json!({ "field": "profile.metric_name", "got": name, "expected": "feature name" })
            }
            Self::PsiCategoricalFeatureUnknown { feature } => {
                json!({ "field": "profile.categorical_features", "unknown": feature })
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
        signal: DriftSignal,
        condition: DriftCondition,
        profile: Option<DriftProfile>,
        description: Option<String>,
    ) -> Result<Self, DriftValidationError> {
        let spec = Self {
            description,
            method,
            signal,
            condition,
            profile,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Run all locked validation invariants on this spec.
    ///
    /// # Errors
    /// Returns the first [`DriftValidationError`] discovered.
    pub fn validate(&self) -> Result<(), DriftValidationError> {
        validate_signal_method(&self.signal, self.method)?;
        validate_signal(&self.signal)?;
        validate_condition(&self.condition)?;
        validate_profile_presence(self.method, self.profile.as_ref())?;
        if let Some(profile) = &self.profile {
            validate_profile(profile)?;
            validate_signal_profile(&self.signal, profile)?;
        }
        Ok(())
    }
}

/// Enforce the invariants that span the signal and its profile.
///
/// A Custom run reads the single series its `Metric` signal names, so the
/// profile must name the same metric and that name must be a storable
/// `FeatureName`. A PSI categorical feature must be one of the fitted
/// Distribution features; otherwise the profile would declare a column the
/// fitter never reads.
///
/// # Errors
/// Returns [`DriftValidationError::CustomMetricNameMismatch`],
/// [`DriftValidationError::CustomMetricNameInvalid`], or
/// [`DriftValidationError::PsiCategoricalFeatureUnknown`].
fn validate_signal_profile(
    signal: &DriftSignal,
    profile: &DriftProfile,
) -> Result<(), DriftValidationError> {
    match (signal, profile) {
        (DriftSignal::Metric { name }, DriftProfile::Custom(custom)) => {
            if *name != custom.metric_name {
                return Err(DriftValidationError::CustomMetricNameMismatch {
                    signal: name.clone(),
                    profile: custom.metric_name.clone(),
                });
            }
            FeatureName::new(name.as_str()).map_err(|_| {
                DriftValidationError::CustomMetricNameInvalid { name: name.clone() }
            })?;
            Ok(())
        }
        (DriftSignal::Distribution { features, .. }, DriftProfile::Psi(psi)) => {
            match psi
                .categorical_features
                .iter()
                .find(|feature| !features.contains(feature))
            {
                Some(feature) => Err(DriftValidationError::PsiCategoricalFeatureUnknown {
                    feature: feature.to_string(),
                }),
                None => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

/// Reject a signal shape the chosen method cannot consume.
///
/// PSI and SPC both fit a baseline distribution and therefore require a
/// `Distribution` signal; `Custom` scores an already-reduced number and
/// therefore requires a `Metric` signal. This is the first check `validate`
/// runs, so later profile checks can assume a coherent method/signal pair.
///
/// # Errors
/// Returns [`DriftValidationError::SignalMethodMismatch`] naming both sides.
fn validate_signal_method(
    signal: &DriftSignal,
    method: DriftMethod,
) -> Result<(), DriftValidationError> {
    let allowed = match method {
        DriftMethod::Psi | DriftMethod::Spc => matches!(signal, DriftSignal::Distribution { .. }),
        DriftMethod::Custom => matches!(signal, DriftSignal::Metric { .. }),
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

/// Enforce the invariants a `Distribution` signal owns.
///
/// A resolved baseline must name a `Data` Card, and the feature list must be
/// non-empty and free of duplicates so each fitted feature maps to exactly one
/// baseline column. An unresolved baseline path is left to the loader, and a
/// `Metric` signal carries no reference or feature list to check.
///
/// # Errors
/// Returns [`DriftValidationError::BaselineRefMustBeData`],
/// [`DriftValidationError::DistributionMissingFeatures`], or
/// [`DriftValidationError::DistributionDuplicateFeatures`].
fn validate_signal(signal: &DriftSignal) -> Result<(), DriftValidationError> {
    match signal {
        DriftSignal::Distribution {
            baseline_ref,
            features,
        } => {
            let Some(baseline_ref) = baseline_ref.as_card_ref() else {
                return Ok(());
            };
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
    }
}

/// Require the statistical condition; a drift verdict is never a bare threshold.
///
/// A bound threshold belongs to the method's own profile, so accepting
/// `Above`, `Below`, or `Outside` here would create a second, competing place
/// to express the same decision.
///
/// # Errors
/// Returns [`DriftValidationError::ConditionNotStatistical`] naming the
/// authored variant.
fn validate_condition(condition: &DriftCondition) -> Result<(), DriftValidationError> {
    match condition {
        DriftCondition::Statistical => Ok(()),
        DriftCondition::Above { .. } => Err(DriftValidationError::ConditionNotStatistical {
            condition: "Above".to_string(),
        }),
        DriftCondition::Below { .. } => Err(DriftValidationError::ConditionNotStatistical {
            condition: "Below".to_string(),
        }),
        DriftCondition::Outside { .. } => Err(DriftValidationError::ConditionNotStatistical {
            condition: "Outside".to_string(),
        }),
    }
}

/// Require a profile and require it to match the method.
///
/// Each method reads its own math configuration, so a missing or mismatched
/// profile would leave the fit unparameterized at run time rather than at
/// registration.
///
/// # Errors
/// Returns [`DriftValidationError::ProfileRequired`] when no profile is
/// authored and [`DriftValidationError::ProfileMethodMismatch`] when the
/// authored profile belongs to a different method.
fn validate_profile_presence(
    method: DriftMethod,
    profile: Option<&DriftProfile>,
) -> Result<(), DriftValidationError> {
    let Some(profile) = profile else {
        return Err(DriftValidationError::ProfileRequired {
            method: method.name().to_string(),
        });
    };
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

/// Require a fixed subgroup of at least two rows.
///
/// A subgroup's sample standard deviation divides by `n - 1`, so a size of
/// zero or one could never fit the S chart.
///
/// # Errors
/// Returns [`DriftValidationError::SpcSampleSizeOutOfRange`] below two.
fn validate_spc_profile(profile: &SpcProfile) -> Result<(), DriftValidationError> {
    if profile.sample_size < 2 {
        return Err(DriftValidationError::SpcSampleSizeOutOfRange);
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
