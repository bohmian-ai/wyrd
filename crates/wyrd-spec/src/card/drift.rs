//! DriftCard spec.
//!
//! Envelope locked per `architecture/wyrd-design.md` §Drift. `DriftProfile`
//! carries method-specific math config only; fitted baseline state lives in
//! `vala-drift`, never in `wyrd-spec`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;
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
