//! Frozen comparison operator catalog. PR4.4 implements each variant in
//! `vala-eval::operators` against this surface.
//!
//! ## Variant stability
//!
//! Adding a variant in a minor release is allowed. Renaming or removing a
//! variant is a breaking change. Parameterless variants serialize as scalar
//! `snake_case` strings (`"equals"`). Parameterized variants serialize as
//! objects with a `kind` discriminator and named parameters
//! (`{"kind":"in_range","min":0.0,"max":1.0,"inclusive":true}`).
//!
//! ## Catalog families
//!
//! | Family | Count | Indexes |
//! |---|---|---|
//! | Numeric | 12 | 1..=12 in declaration order |
//! | String | 15 | 13..=27 |
//! | Collection | 14 | 28..=41 |
//! | Type | 9 | 42..=50 |
//! | Tolerance + advanced | 6 | 51..=56 |

use std::str::FromStr;

use regex::Regex;
use serde::de::Error as DeError;
use serde::ser::Serializer;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::WyrdError;

/// Maximum byte length of a user-supplied regex pattern stored by
/// [`ComparisonOperator::MatchesRegex`] or [`ComparisonOperator::NotMatchesRegex`].
///
/// Patterns longer than this are rejected at spec validation time to bound
/// regex-compilation cost in eval workers.
pub const MAX_REGEX_PATTERN_LEN: usize = 4096;

pub(super) fn validate_regex_pattern(pattern: &str) -> Result<(), WyrdError> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(WyrdError::Validation {
            message: format!(
                "regex pattern length {} exceeds MAX_REGEX_PATTERN_LEN {MAX_REGEX_PATTERN_LEN}",
                pattern.len()
            ),
            details: serde_json::Value::Null,
        });
    }
    Regex::new(pattern).map_err(|e| WyrdError::Validation {
        message: format!("regex pattern is invalid: {e}"),
        details: serde_json::Value::Null,
    })?;
    Ok(())
}

/// One comparison operator selected by an assertion, judge, trace, agent,
/// condition, or scenario task.
///
/// Runtime semantics are owned by `vala-eval::operators` (PR4.4). The shape
/// here is the wire contract.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ComparisonOperator {
    /// Requires exact equality.
    Equals,
    /// Requires values to differ.
    NotEquals,
    /// Requires the observed value to be strictly greater than the expected
    /// value.
    GreaterThan,
    /// Requires the observed value to be greater than or equal to the expected
    /// value.
    GreaterThanOrEquals,
    /// Requires the observed value to be strictly less than the expected
    /// value.
    LessThan,
    /// Requires the observed value to be less than or equal to the expected
    /// value.
    LessThanOrEquals,
    /// Requires the observed value to fall within the given range.
    ///
    /// Runtime rejects `min > max`.
    InRange {
        /// Lower bound of the accepted range.
        min: f64,
        /// Upper bound of the accepted range.
        max: f64,
        /// Whether the range endpoints are inclusive.
        inclusive: bool,
    },
    /// Requires the observed value to fall outside the given range.
    ///
    /// Runtime rejects `min > max`.
    NotInRange {
        /// Lower bound of the excluded range.
        min: f64,
        /// Upper bound of the excluded range.
        max: f64,
        /// Whether the range endpoints are inclusive.
        inclusive: bool,
    },
    /// Requires the observed value to be within the given absolute tolerance
    /// of the expected value.
    ApproximatelyEquals {
        /// Absolute tolerance accepted by the comparison.
        tolerance: f64,
    },
    /// Requires the observed numeric value to be positive.
    IsPositive,
    /// Requires the observed numeric value to be negative.
    IsNegative,
    /// Requires the observed numeric value to be zero.
    IsZero,

    /// Requires the observed string to contain the expected substring.
    Contains,
    /// Requires the observed string not to contain the expected substring.
    NotContains,
    /// Requires the observed string to contain the expected substring,
    /// ignoring case.
    ContainsIgnoreCase,
    /// Requires the observed string to start with the expected prefix.
    StartsWith,
    /// Requires the observed string to end with the expected suffix.
    EndsWith,
    /// Requires the observed string to match the given regex pattern.
    MatchesRegex {
        /// Regex pattern used for matching.
        pattern: String,
    },
    /// Requires the observed string not to match the given regex pattern.
    NotMatchesRegex {
        /// Regex pattern used for matching.
        pattern: String,
    },
    /// Requires the observed string to be a valid email address.
    IsEmail,
    /// Requires the observed string to be a valid URL.
    IsUrl,
    /// Requires the observed string to be a valid UUID.
    IsUuid,
    /// Requires the observed string to be a valid IPv4 address.
    IsIpv4,
    /// Requires the observed string to be a valid IPv6 address.
    IsIpv6,
    /// Requires the observed string length to be at least `min`.
    HasMinLength {
        /// Minimum accepted string length.
        min: usize,
    },
    /// Requires the observed string length to be at most `max`.
    HasMaxLength {
        /// Maximum accepted string length.
        max: usize,
    },
    /// Requires the observed string to parse as JSON.
    IsJson,

    /// Requires the observed value to be contained in the expected collection.
    In,
    /// Requires the observed value not to be contained in the expected
    /// collection.
    NotIn,
    /// Requires the observed collection to be a subset of the expected
    /// collection.
    IsSubset,
    /// Requires the observed collection to be a superset of the expected
    /// collection.
    IsSuperset,
    /// Requires the observed and expected collections to be disjoint.
    IsDisjoint,
    /// Requires all expected predicates or members to match.
    AllOf,
    /// Requires at least one expected predicate or member to match.
    AnyOf,
    /// Requires no expected predicates or members to match.
    NoneOf,
    /// Requires the observed collection to be empty.
    IsEmpty,
    /// Requires the observed collection to be non-empty.
    IsNonEmpty,
    /// Requires the observed collection length to match `expected`.
    Length {
        /// Exact required collection length.
        expected: usize,
    },
    /// Requires the observed collection length to be greater than `min`.
    LengthGreaterThan {
        /// Strict lower bound for collection length.
        min: usize,
    },
    /// Requires the observed collection length to be less than `max`.
    LengthLessThan {
        /// Strict upper bound for collection length.
        max: usize,
    },
    /// Requires all observed collection values to be unique.
    UniqueValues,

    /// Requires the observed value to be truthy under runtime semantics.
    IsTruthy,
    /// Requires the observed value to be falsy under runtime semantics.
    IsFalsy,
    /// Requires the observed value to be JSON null.
    IsNull,
    /// Requires the observed value not to be JSON null.
    IsNotNull,
    /// Requires the observed value to match the declared JSON type.
    IsType {
        /// Expected JSON value type.
        expected: JsonValueType,
    },
    /// Requires the observed value to be a JSON string.
    IsString,
    /// Requires the observed value to be a JSON number.
    IsNumber,
    /// Requires the observed value to be a JSON boolean.
    IsBoolean,
    /// Requires the observed value to be a JSON object.
    IsObject,

    /// Requires the observed and expected values to be within the given
    /// absolute tolerance.
    WithinAbsTolerance {
        /// Absolute tolerance accepted by the comparison.
        tolerance: f64,
    },
    /// Requires the observed and expected values to be within the given
    /// percentage tolerance.
    WithinPctTolerance {
        /// Percentage tolerance accepted by the comparison.
        pct: f64,
    },
    /// Requires the observed value to be within `sigma` standard deviations of
    /// `mean`.
    WithinStdDev {
        /// Number of standard deviations allowed.
        sigma: f64,
        /// Distribution mean used by the comparison.
        mean: f64,
        /// Distribution standard deviation used by the comparison.
        std_dev: f64,
    },
    /// Requires the observed value to fall between the given percentiles.
    ///
    /// Runtime enforces `0.0 <= lower_pct <= upper_pct <= 1.0`.
    BetweenPercentiles {
        /// Lower percentile bound.
        lower_pct: f64,
        /// Upper percentile bound.
        upper_pct: f64,
    },
    /// Requires the measured divergence to remain below `threshold`.
    DivergenceLessThan {
        /// Divergence metric used by the comparison.
        metric: DivergenceMetric,
        /// Maximum allowed divergence.
        threshold: f64,
    },
    /// Requires cosine similarity to be at least `threshold`.
    CosineSimilarityAtLeast {
        /// Minimum accepted cosine similarity.
        threshold: f64,
    },
}

impl Serialize for ComparisonOperator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(parameterized) = ParameterizedComparisonOperator::from_operator(self) {
            parameterized.serialize(serializer)
        } else {
            serializer.serialize_str(self.discriminator())
        }
    }
}

impl<'de> Deserialize<'de> for ComparisonOperator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(discriminator) => {
                Self::from_discriminator_parameterless(&discriminator).map_err(D::Error::custom)
            }
            serde_json::Value::Object(_) => {
                let parameterized = ParameterizedComparisonOperator::deserialize(value)
                    .map_err(D::Error::custom)?;
                Ok(parameterized.into())
            }
            other => Err(D::Error::custom(format!(
                "operator must be a scalar discriminator or parameterized object; got {other}"
            ))),
        }
    }
}

impl schemars::JsonSchema for ComparisonOperator {
    fn schema_name() -> String {
        "ComparisonOperator".to_string()
    }

    fn json_schema(schema_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        <ComparisonOperatorWireSchema as schemars::JsonSchema>::json_schema(schema_gen)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ParameterizedComparisonOperator {
    InRange {
        min: f64,
        max: f64,
        inclusive: bool,
    },
    NotInRange {
        min: f64,
        max: f64,
        inclusive: bool,
    },
    ApproximatelyEquals {
        tolerance: f64,
    },
    MatchesRegex {
        pattern: String,
    },
    NotMatchesRegex {
        pattern: String,
    },
    HasMinLength {
        min: usize,
    },
    HasMaxLength {
        max: usize,
    },
    Length {
        expected: usize,
    },
    LengthGreaterThan {
        min: usize,
    },
    LengthLessThan {
        max: usize,
    },
    IsType {
        expected: JsonValueType,
    },
    WithinAbsTolerance {
        tolerance: f64,
    },
    WithinPctTolerance {
        pct: f64,
    },
    WithinStdDev {
        sigma: f64,
        mean: f64,
        std_dev: f64,
    },
    BetweenPercentiles {
        lower_pct: f64,
        upper_pct: f64,
    },
    DivergenceLessThan {
        metric: DivergenceMetric,
        threshold: f64,
    },
    CosineSimilarityAtLeast {
        threshold: f64,
    },
}

impl ParameterizedComparisonOperator {
    fn from_operator(operator: &ComparisonOperator) -> Option<Self> {
        use ComparisonOperator::*;

        Some(match operator {
            InRange {
                min,
                max,
                inclusive,
            } => Self::InRange {
                min: *min,
                max: *max,
                inclusive: *inclusive,
            },
            NotInRange {
                min,
                max,
                inclusive,
            } => Self::NotInRange {
                min: *min,
                max: *max,
                inclusive: *inclusive,
            },
            ApproximatelyEquals { tolerance } => Self::ApproximatelyEquals {
                tolerance: *tolerance,
            },
            MatchesRegex { pattern } => Self::MatchesRegex {
                pattern: pattern.clone(),
            },
            NotMatchesRegex { pattern } => Self::NotMatchesRegex {
                pattern: pattern.clone(),
            },
            HasMinLength { min } => Self::HasMinLength { min: *min },
            HasMaxLength { max } => Self::HasMaxLength { max: *max },
            Length { expected } => Self::Length {
                expected: *expected,
            },
            LengthGreaterThan { min } => Self::LengthGreaterThan { min: *min },
            LengthLessThan { max } => Self::LengthLessThan { max: *max },
            IsType { expected } => Self::IsType {
                expected: *expected,
            },
            WithinAbsTolerance { tolerance } => Self::WithinAbsTolerance {
                tolerance: *tolerance,
            },
            WithinPctTolerance { pct } => Self::WithinPctTolerance { pct: *pct },
            WithinStdDev {
                sigma,
                mean,
                std_dev,
            } => Self::WithinStdDev {
                sigma: *sigma,
                mean: *mean,
                std_dev: *std_dev,
            },
            BetweenPercentiles {
                lower_pct,
                upper_pct,
            } => Self::BetweenPercentiles {
                lower_pct: *lower_pct,
                upper_pct: *upper_pct,
            },
            DivergenceLessThan { metric, threshold } => Self::DivergenceLessThan {
                metric: *metric,
                threshold: *threshold,
            },
            CosineSimilarityAtLeast { threshold } => Self::CosineSimilarityAtLeast {
                threshold: *threshold,
            },
            Equals | NotEquals | GreaterThan | GreaterThanOrEquals | LessThan
            | LessThanOrEquals | IsPositive | IsNegative | IsZero | Contains | NotContains
            | ContainsIgnoreCase | StartsWith | EndsWith | IsEmail | IsUrl | IsUuid | IsIpv4
            | IsIpv6 | IsJson | In | NotIn | IsSubset | IsSuperset | IsDisjoint | AllOf | AnyOf
            | NoneOf | IsEmpty | IsNonEmpty | UniqueValues | IsTruthy | IsFalsy | IsNull
            | IsNotNull | IsString | IsNumber | IsBoolean | IsObject => return None,
        })
    }
}

impl From<ParameterizedComparisonOperator> for ComparisonOperator {
    fn from(operator: ParameterizedComparisonOperator) -> Self {
        match operator {
            ParameterizedComparisonOperator::InRange {
                min,
                max,
                inclusive,
            } => Self::InRange {
                min,
                max,
                inclusive,
            },
            ParameterizedComparisonOperator::NotInRange {
                min,
                max,
                inclusive,
            } => Self::NotInRange {
                min,
                max,
                inclusive,
            },
            ParameterizedComparisonOperator::ApproximatelyEquals { tolerance } => {
                Self::ApproximatelyEquals { tolerance }
            }
            ParameterizedComparisonOperator::MatchesRegex { pattern } => {
                Self::MatchesRegex { pattern }
            }
            ParameterizedComparisonOperator::NotMatchesRegex { pattern } => {
                Self::NotMatchesRegex { pattern }
            }
            ParameterizedComparisonOperator::HasMinLength { min } => Self::HasMinLength { min },
            ParameterizedComparisonOperator::HasMaxLength { max } => Self::HasMaxLength { max },
            ParameterizedComparisonOperator::Length { expected } => Self::Length { expected },
            ParameterizedComparisonOperator::LengthGreaterThan { min } => {
                Self::LengthGreaterThan { min }
            }
            ParameterizedComparisonOperator::LengthLessThan { max } => Self::LengthLessThan { max },
            ParameterizedComparisonOperator::IsType { expected } => Self::IsType { expected },
            ParameterizedComparisonOperator::WithinAbsTolerance { tolerance } => {
                Self::WithinAbsTolerance { tolerance }
            }
            ParameterizedComparisonOperator::WithinPctTolerance { pct } => {
                Self::WithinPctTolerance { pct }
            }
            ParameterizedComparisonOperator::WithinStdDev {
                sigma,
                mean,
                std_dev,
            } => Self::WithinStdDev {
                sigma,
                mean,
                std_dev,
            },
            ParameterizedComparisonOperator::BetweenPercentiles {
                lower_pct,
                upper_pct,
            } => Self::BetweenPercentiles {
                lower_pct,
                upper_pct,
            },
            ParameterizedComparisonOperator::DivergenceLessThan { metric, threshold } => {
                Self::DivergenceLessThan { metric, threshold }
            }
            ParameterizedComparisonOperator::CosineSimilarityAtLeast { threshold } => {
                Self::CosineSimilarityAtLeast { threshold }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ParameterlessComparisonOperatorSchema {
    Equals,
    NotEquals,
    GreaterThan,
    GreaterThanOrEquals,
    LessThan,
    LessThanOrEquals,
    IsPositive,
    IsNegative,
    IsZero,
    Contains,
    NotContains,
    ContainsIgnoreCase,
    StartsWith,
    EndsWith,
    IsEmail,
    IsUrl,
    IsUuid,
    IsIpv4,
    IsIpv6,
    IsJson,
    In,
    NotIn,
    IsSubset,
    IsSuperset,
    IsDisjoint,
    AllOf,
    AnyOf,
    NoneOf,
    IsEmpty,
    IsNonEmpty,
    UniqueValues,
    IsTruthy,
    IsFalsy,
    IsNull,
    IsNotNull,
    IsString,
    IsNumber,
    IsBoolean,
    IsObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
enum ComparisonOperatorWireSchema {
    Parameterless(ParameterlessComparisonOperatorSchema),
    Parameterized(ParameterizedComparisonOperator),
}

impl ComparisonOperator {
    /// Stable `snake_case` discriminator for this variant.
    ///
    /// This matches the `operator` tag in the serde wire form without
    /// requiring a serialization round-trip.
    #[must_use]
    pub fn discriminator(&self) -> &'static str {
        use ComparisonOperator::*;

        match self {
            Equals => "equals",
            NotEquals => "not_equals",
            GreaterThan => "greater_than",
            GreaterThanOrEquals => "greater_than_or_equals",
            LessThan => "less_than",
            LessThanOrEquals => "less_than_or_equals",
            InRange { .. } => "in_range",
            NotInRange { .. } => "not_in_range",
            ApproximatelyEquals { .. } => "approximately_equals",
            IsPositive => "is_positive",
            IsNegative => "is_negative",
            IsZero => "is_zero",
            Contains => "contains",
            NotContains => "not_contains",
            ContainsIgnoreCase => "contains_ignore_case",
            StartsWith => "starts_with",
            EndsWith => "ends_with",
            MatchesRegex { .. } => "matches_regex",
            NotMatchesRegex { .. } => "not_matches_regex",
            IsEmail => "is_email",
            IsUrl => "is_url",
            IsUuid => "is_uuid",
            IsIpv4 => "is_ipv4",
            IsIpv6 => "is_ipv6",
            HasMinLength { .. } => "has_min_length",
            HasMaxLength { .. } => "has_max_length",
            IsJson => "is_json",
            In => "in",
            NotIn => "not_in",
            IsSubset => "is_subset",
            IsSuperset => "is_superset",
            IsDisjoint => "is_disjoint",
            AllOf => "all_of",
            AnyOf => "any_of",
            NoneOf => "none_of",
            IsEmpty => "is_empty",
            IsNonEmpty => "is_non_empty",
            Length { .. } => "length",
            LengthGreaterThan { .. } => "length_greater_than",
            LengthLessThan { .. } => "length_less_than",
            UniqueValues => "unique_values",
            IsTruthy => "is_truthy",
            IsFalsy => "is_falsy",
            IsNull => "is_null",
            IsNotNull => "is_not_null",
            IsType { .. } => "is_type",
            IsString => "is_string",
            IsNumber => "is_number",
            IsBoolean => "is_boolean",
            IsObject => "is_object",
            WithinAbsTolerance { .. } => "within_abs_tolerance",
            WithinPctTolerance { .. } => "within_pct_tolerance",
            WithinStdDev { .. } => "within_std_dev",
            BetweenPercentiles { .. } => "between_percentiles",
            DivergenceLessThan { .. } => "divergence_less_than",
            CosineSimilarityAtLeast { .. } => "cosine_similarity_at_least",
        }
    }

    /// Stable `snake_case` name for this variant.
    ///
    /// This is an alias for [`ComparisonOperator::discriminator`].
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        self.discriminator()
    }

    /// Parses a parameterless operator from its discriminator.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the discriminator is unknown or
    /// names a parameterized variant that requires the full serde-tagged form.
    pub fn from_discriminator_parameterless(s: &str) -> Result<Self, WyrdError> {
        use ComparisonOperator::*;

        Ok(match s {
            "equals" => Equals,
            "not_equals" => NotEquals,
            "greater_than" => GreaterThan,
            "greater_than_or_equals" => GreaterThanOrEquals,
            "less_than" => LessThan,
            "less_than_or_equals" => LessThanOrEquals,
            "is_positive" => IsPositive,
            "is_negative" => IsNegative,
            "is_zero" => IsZero,
            "contains" => Contains,
            "not_contains" => NotContains,
            "contains_ignore_case" => ContainsIgnoreCase,
            "starts_with" => StartsWith,
            "ends_with" => EndsWith,
            "is_email" => IsEmail,
            "is_url" => IsUrl,
            "is_uuid" => IsUuid,
            "is_ipv4" => IsIpv4,
            "is_ipv6" => IsIpv6,
            "is_json" => IsJson,
            "in" => In,
            "not_in" => NotIn,
            "is_subset" => IsSubset,
            "is_superset" => IsSuperset,
            "is_disjoint" => IsDisjoint,
            "all_of" => AllOf,
            "any_of" => AnyOf,
            "none_of" => NoneOf,
            "is_empty" => IsEmpty,
            "is_non_empty" => IsNonEmpty,
            "unique_values" => UniqueValues,
            "is_truthy" => IsTruthy,
            "is_falsy" => IsFalsy,
            "is_null" => IsNull,
            "is_not_null" => IsNotNull,
            "is_string" => IsString,
            "is_number" => IsNumber,
            "is_boolean" => IsBoolean,
            "is_object" => IsObject,
            "in_range"
            | "not_in_range"
            | "approximately_equals"
            | "matches_regex"
            | "not_matches_regex"
            | "has_min_length"
            | "has_max_length"
            | "length"
            | "length_greater_than"
            | "length_less_than"
            | "is_type"
            | "within_abs_tolerance"
            | "within_pct_tolerance"
            | "within_std_dev"
            | "between_percentiles"
            | "divergence_less_than"
            | "cosine_similarity_at_least" => {
                return Err(WyrdError::Validation {
                    message: format!("operator {s:?} is parameterized; use the full tagged form"),
                    details: serde_json::Value::Null,
                });
            }
            other => {
                return Err(WyrdError::Validation {
                    message: format!("unknown operator discriminator: {other:?}"),
                    details: serde_json::Value::Null,
                });
            }
        })
    }
}

impl FromStr for ComparisonOperator {
    type Err = WyrdError;

    /// Parses a parameterless operator discriminator.
    ///
    /// Parameterized operators must round-trip through the full serde-tagged
    /// representation instead.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_discriminator_parameterless(s)
    }
}

/// JSON value-type set used by [`ComparisonOperator::IsType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum JsonValueType {
    /// JSON null.
    Null,
    /// JSON boolean.
    Bool,
    /// JSON number.
    Number,
    /// JSON string.
    String,
    /// JSON array.
    Array,
    /// JSON object.
    Object,
}

/// Probability-divergence metric used by
/// [`ComparisonOperator::DivergenceLessThan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DivergenceMetric {
    /// Kullback-Leibler divergence.
    Kl,
    /// Jensen-Shannon divergence.
    Js,
    /// Wasserstein distance.
    Wasserstein,
}
