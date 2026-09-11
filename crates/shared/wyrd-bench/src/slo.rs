//! Threshold loading and hard-fail SLO evaluation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// One configured SLO threshold.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SloThreshold {
    /// Measurement name supplied by a Criterion bench.
    pub metric: String,
    /// Inclusive lower bound, when configured.
    pub min: Option<f64>,
    /// Inclusive upper bound, when configured.
    pub max: Option<f64>,
    /// Optional benchmark scope metadata.
    pub scope: Option<String>,
    /// Optional pod-count metadata for distributed measurements.
    pub pod_count: Option<u64>,
}

/// A measured Criterion value to evaluate.
#[derive(Debug, Clone, PartialEq)]
pub struct SloMeasurement {
    /// Criterion group or threshold key.
    pub group: String,
    /// Measurement name, for example `p99_us`.
    pub metric: String,
    /// Measured value in the metric's configured unit.
    pub value: f64,
}

/// Result of a passing SLO evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct SloResult {
    /// Normalized threshold key that was evaluated.
    pub group: String,
    /// Metric that was evaluated.
    pub metric: String,
    /// Measured value.
    pub value: f64,
}

/// Errors are hard failures: a missing threshold is never treated as a pass.
#[derive(Debug, Error, PartialEq)]
pub enum SloError {
    /// Threshold TOML could not be read.
    #[error("failed to read SLO thresholds {path}: {message}")]
    Read { path: String, message: String },
    /// Threshold TOML was invalid.
    #[error("failed to parse SLO thresholds: {0}")]
    Parse(String),
    /// No entry matched the measured Criterion group.
    #[error("no SLO threshold configured for benchmark group {0}")]
    MissingThreshold(String),
    /// The measurement used a different metric than the configured threshold.
    #[error("SLO metric mismatch for {group}: expected {expected}, got {actual}")]
    MetricMismatch {
        /// Threshold key.
        group: String,
        /// Configured metric.
        expected: String,
        /// Measured metric.
        actual: String,
    },
    /// The measured value violated a configured bound.
    #[error("SLO regression for {group}: {metric}={value} outside {bound}")]
    Regression {
        /// Threshold key.
        group: String,
        /// Measured metric.
        metric: String,
        /// Measured value.
        value: f64,
        /// Human-readable bound description.
        bound: String,
    },
    /// The measured value was not finite.
    #[error("SLO measurement for {group} is not finite: {value}")]
    NonFinite { group: String, value: f64 },
}

#[derive(Debug, Deserialize)]
struct ThresholdDocument {
    bench: BTreeMap<String, BTreeMap<String, SloThreshold>>,
}

/// SLO gate loaded from `benches/thresholds.toml`.
#[derive(Debug, Clone)]
pub struct SloGate {
    thresholds: BTreeMap<String, SloThreshold>,
}

impl SloGate {
    /// Load thresholds from a TOML string.
    pub fn from_toml(raw: &str) -> Result<Self, SloError> {
        let document: ThresholdDocument =
            toml::from_str(raw).map_err(|error| SloError::Parse(error.to_string()))?;
        let thresholds = document
            .bench
            .into_iter()
            .flat_map(|(domain, entries)| {
                entries
                    .into_iter()
                    .map(move |(name, threshold)| (format!("{domain}.{name}"), threshold))
            })
            .collect();
        Ok(Self { thresholds })
    }

    /// Load thresholds from a repository file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, SloError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|error| SloError::Read {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        Self::from_toml(&raw)
    }

    /// Load the repository's canonical threshold file.
    pub fn from_default_path() -> Result<Self, SloError> {
        Self::from_path(default_threshold_path())
    }

    /// Return a configured threshold by its normalized key.
    #[must_use]
    pub fn threshold(&self, group: &str) -> Option<&SloThreshold> {
        self.thresholds.get(&normalize_group(group))
    }

    /// Evaluate one measurement and return a passing result.
    pub fn check(&self, measurement: &SloMeasurement) -> Result<SloResult, SloError> {
        let group = normalize_group(&measurement.group);
        let Some(threshold) = self.thresholds.get(&group) else {
            return Err(SloError::MissingThreshold(group));
        };
        if threshold.metric != measurement.metric {
            return Err(SloError::MetricMismatch {
                group,
                expected: threshold.metric.clone(),
                actual: measurement.metric.clone(),
            });
        }
        if !measurement.value.is_finite() {
            return Err(SloError::NonFinite {
                group,
                value: measurement.value,
            });
        }
        if let Some(min) = threshold.min
            && measurement.value < min
        {
            return Err(SloError::Regression {
                group,
                metric: measurement.metric.clone(),
                value: measurement.value,
                bound: format!(">={min}"),
            });
        }
        if let Some(max) = threshold.max
            && measurement.value > max
        {
            return Err(SloError::Regression {
                group,
                metric: measurement.metric.clone(),
                value: measurement.value,
                bound: format!("<={max}"),
            });
        }
        Ok(SloResult {
            group,
            metric: measurement.metric.clone(),
            value: measurement.value,
        })
    }

    /// Evaluate a value using the threshold's configured metric.
    pub fn check_value(&self, group: &str, value: f64) -> Result<SloResult, SloError> {
        let normalized = normalize_group(group);
        let Some(threshold) = self.thresholds.get(&normalized) else {
            return Err(SloError::MissingThreshold(normalized));
        };
        self.check(&SloMeasurement {
            group: normalized,
            metric: threshold.metric.clone(),
            value,
        })
    }

    /// Return a process-style status for a measurement (`0` pass, `1` fail).
    #[must_use]
    pub fn exit_status(&self, measurement: &SloMeasurement) -> i32 {
        i32::from(self.check(measurement).is_err())
    }
}

/// Resolves the checked-in threshold file independently of process cwd.
fn default_threshold_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("benches/thresholds.toml")
}

fn normalize_group(group: &str) -> String {
    let normalized = group.replace('/', ".");
    normalized
        .strip_prefix("bench.")
        .unwrap_or(&normalized)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLDS: &str = include_str!("../../../../benches/thresholds.toml");

    /// Proves the canonical loader uses the compile-time repository location.
    #[test]
    fn default_threshold_path_loads_checked_in_thresholds() {
        let path = default_threshold_path();
        assert!(path.is_absolute());
        let gate = SloGate::from_default_path().expect("checked-in thresholds load");
        assert!(gate.threshold("bench.bifrost.forge_slo").is_some());
    }

    #[test]
    fn test_threshold_parse_from_toml() {
        let gate = SloGate::from_toml(THRESHOLDS).expect("thresholds parse");
        let threshold = gate
            .threshold("bench.bifrost.read_lookup_p99")
            .expect("lookup threshold");
        assert_eq!(threshold.metric, "p99_us");
        assert_eq!(threshold.max, Some(200_000.0));
    }

    #[test]
    fn test_slo_gate_fails_on_regression() {
        let gate = SloGate::from_toml(THRESHOLDS).expect("thresholds parse");
        let measurement = SloMeasurement {
            group: "bifrost.read_lookup_p99".to_owned(),
            metric: "p99_us".to_owned(),
            value: 200_001.0,
        };
        assert_ne!(gate.exit_status(&measurement), 0);
        assert!(matches!(
            gate.check(&measurement),
            Err(SloError::Regression { .. })
        ));
    }

    #[test]
    fn test_slo_gate_passes_at_or_below_threshold() {
        let gate = SloGate::from_toml(THRESHOLDS).expect("thresholds parse");
        let measurement = SloMeasurement {
            group: "bifrost.read_lookup_p99".to_owned(),
            metric: "p99_us".to_owned(),
            value: 200_000.0,
        };
        assert_eq!(gate.exit_status(&measurement), 0);
        assert!(gate.check(&measurement).is_ok());
    }
}
