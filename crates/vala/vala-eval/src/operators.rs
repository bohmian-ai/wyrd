//! Runtime semantics for every frozen variant of
//! [`wyrd_spec::vala::eval::operator::ComparisonOperator`].
//!
//! This module is the deterministic core of `vala-eval`: one pure function over
//! `(left, op, right)` with typed failures and no side effects.
//!
//! ## Inputs
//!
//! - `left: &serde_json::Value` - the observed value extracted from a record,
//!   trace, workflow record, or judge response.
//! - `op: &ComparisonOperator` - the operator declared by the spec task.
//! - `right: &serde_json::Value` - the expected payload. For parameterless
//!   operators (`IsNull`, `IsPositive`, `IsTruthy`, etc.) the caller passes
//!   [`serde_json::Value::Null`].
//!
//! ## Output
//!
//! [`OperatorVerdict`] always carries the boolean pass/fail. `observed` and
//! `expected` mirror the spec's assertion-result fields and are populated with
//! the values the operator actually compared against.
//!
//! ## Errors
//!
//! Type mismatches produce [`EvalExecError::OperatorMismatch`]. Operator
//! parameters that are well-formed at the wire layer but nonsense at runtime
//! produce [`EvalExecError::OperatorInvalidConfig`]. Regex compile failure
//! produces [`EvalExecError::OperatorRegexInvalid`]. Operators never panic.
//!
//! ## Determinism
//!
//! No clocks, no env, no RNG. NaN comparisons fall through as `passed = false`.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use regex::Regex;
use serde_json::Value;
use uuid::Uuid;

use wyrd_spec::vala::eval::operator::{ComparisonOperator, DivergenceMetric, JsonValueType};

use crate::error::EvalExecError;

/// Verdict produced by one operator evaluation.
///
/// `observed` and `expected` are populated with the values the operator
/// actually compared, so result rendering can treat every operator family
/// consistently.
#[derive(Debug, Clone, PartialEq)]
pub struct OperatorVerdict {
    pub passed: bool,
    pub observed: Option<Value>,
    pub expected: Option<Value>,
}

impl OperatorVerdict {
    fn outcome(passed: bool, observed: Value, expected: Value) -> Self {
        Self {
            passed,
            observed: Some(observed),
            expected: Some(expected),
        }
    }

    fn type_test(passed: bool, observed: Value) -> Self {
        Self {
            passed,
            observed: Some(observed),
            expected: None,
        }
    }
}

/// Evaluate `op` against `(left, right)`.
///
/// See module docs for the semantic contract.
pub fn evaluate_operator(
    left: &Value,
    op: &ComparisonOperator,
    right: &Value,
) -> Result<OperatorVerdict, EvalExecError> {
    use ComparisonOperator::*;

    match op {
        Equals => Ok(OperatorVerdict::outcome(
            json_equals(left, right),
            left.clone(),
            right.clone(),
        )),
        NotEquals => Ok(OperatorVerdict::outcome(
            !json_equals(left, right),
            left.clone(),
            right.clone(),
        )),
        GreaterThan => {
            let (l, r) = require_numbers(op, left, right)?;
            Ok(OperatorVerdict::outcome(l > r, json_num(l), json_num(r)))
        }
        GreaterThanOrEquals => {
            let (l, r) = require_numbers(op, left, right)?;
            Ok(OperatorVerdict::outcome(l >= r, json_num(l), json_num(r)))
        }
        LessThan => {
            let (l, r) = require_numbers(op, left, right)?;
            Ok(OperatorVerdict::outcome(l < r, json_num(l), json_num(r)))
        }
        LessThanOrEquals => {
            let (l, r) = require_numbers(op, left, right)?;
            Ok(OperatorVerdict::outcome(l <= r, json_num(l), json_num(r)))
        }
        InRange {
            min,
            max,
            inclusive,
        } => {
            check_range_bounds(op, *min, *max)?;
            let l = require_number_left(op, left)?;
            let passed = if *inclusive {
                l >= *min && l <= *max
            } else {
                l > *min && l < *max
            };
            Ok(OperatorVerdict::outcome(
                passed,
                json_num(l),
                serde_json::json!({ "min": *min, "max": *max, "inclusive": *inclusive }),
            ))
        }
        NotInRange {
            min,
            max,
            inclusive,
        } => {
            check_range_bounds(op, *min, *max)?;
            let l = require_number_left(op, left)?;
            let inside = if *inclusive {
                l >= *min && l <= *max
            } else {
                l > *min && l < *max
            };
            Ok(OperatorVerdict::outcome(
                !inside,
                json_num(l),
                serde_json::json!({ "min": *min, "max": *max, "inclusive": *inclusive }),
            ))
        }
        ApproximatelyEquals { tolerance } => {
            let (l, r) = require_numbers(op, left, right)?;
            let passed = (l - r).abs() <= *tolerance;
            Ok(OperatorVerdict::outcome(passed, json_num(l), json_num(r)))
        }
        IsPositive => {
            let l = require_number_left(op, left)?;
            Ok(OperatorVerdict::type_test(l > 0.0, json_num(l)))
        }
        IsNegative => {
            let l = require_number_left(op, left)?;
            Ok(OperatorVerdict::type_test(l < 0.0, json_num(l)))
        }
        IsZero => {
            let l = require_number_left(op, left)?;
            Ok(OperatorVerdict::type_test(l == 0.0, json_num(l)))
        }
        Contains => {
            let (l, r) = require_strings(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                l.contains(r),
                str_val(l),
                str_val(r),
            ))
        }
        NotContains => {
            let (l, r) = require_strings(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                !l.contains(r),
                str_val(l),
                str_val(r),
            ))
        }
        ContainsIgnoreCase => {
            let (l, r) = require_strings(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                l.to_lowercase().contains(&r.to_lowercase()),
                str_val(l),
                str_val(r),
            ))
        }
        StartsWith => {
            let (l, r) = require_strings(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                l.starts_with(r),
                str_val(l),
                str_val(r),
            ))
        }
        EndsWith => {
            let (l, r) = require_strings(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                l.ends_with(r),
                str_val(l),
                str_val(r),
            ))
        }
        MatchesRegex { pattern } => {
            let l = require_string_left(op, left)?;
            let re = compile_regex(op, pattern)?;
            Ok(OperatorVerdict::outcome(
                re.is_match(l),
                str_val(l),
                Value::String(pattern.clone()),
            ))
        }
        NotMatchesRegex { pattern } => {
            let l = require_string_left(op, left)?;
            let re = compile_regex(op, pattern)?;
            Ok(OperatorVerdict::outcome(
                !re.is_match(l),
                str_val(l),
                Value::String(pattern.clone()),
            ))
        }
        IsEmail => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::type_test(
                validate_email_lite(l),
                str_val(l),
            ))
        }
        IsUrl => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::type_test(validate_url_lite(l), str_val(l)))
        }
        IsUuid => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::type_test(
                Uuid::parse_str(l).is_ok(),
                str_val(l),
            ))
        }
        IsIpv4 => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::type_test(
                Ipv4Addr::from_str(l).is_ok(),
                str_val(l),
            ))
        }
        IsIpv6 => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::type_test(
                Ipv6Addr::from_str(l).is_ok(),
                str_val(l),
            ))
        }
        HasMinLength { min } => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::outcome(
                l.chars().count() >= *min,
                str_val(l),
                serde_json::json!({ "min": *min }),
            ))
        }
        HasMaxLength { max } => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::outcome(
                l.chars().count() <= *max,
                str_val(l),
                serde_json::json!({ "max": *max }),
            ))
        }
        IsJson => {
            let l = require_string_left(op, left)?;
            Ok(OperatorVerdict::type_test(
                serde_json::from_str::<Value>(l).is_ok(),
                str_val(l),
            ))
        }
        In => {
            let arr = require_array_right(op, right)?;
            Ok(OperatorVerdict::outcome(
                arr.iter().any(|v| json_equals(left, v)),
                left.clone(),
                right.clone(),
            ))
        }
        NotIn => {
            let arr = require_array_right(op, right)?;
            Ok(OperatorVerdict::outcome(
                !arr.iter().any(|v| json_equals(left, v)),
                left.clone(),
                right.clone(),
            ))
        }
        IsSubset => {
            let (l, r) = require_arrays(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                l.iter()
                    .all(|item| r.iter().any(|other| json_equals(item, other))),
                left.clone(),
                right.clone(),
            ))
        }
        IsSuperset => {
            let (l, r) = require_arrays(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                r.iter()
                    .all(|item| l.iter().any(|other| json_equals(item, other))),
                left.clone(),
                right.clone(),
            ))
        }
        IsDisjoint => {
            let (l, r) = require_arrays(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                !l.iter()
                    .any(|item| r.iter().any(|other| json_equals(item, other))),
                left.clone(),
                right.clone(),
            ))
        }
        AllOf => {
            let (l, r) = require_arrays(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                r.iter()
                    .all(|item| l.iter().any(|other| json_equals(item, other))),
                left.clone(),
                right.clone(),
            ))
        }
        AnyOf => {
            let (l, r) = require_arrays(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                r.iter()
                    .any(|item| l.iter().any(|other| json_equals(item, other))),
                left.clone(),
                right.clone(),
            ))
        }
        NoneOf => {
            let (l, r) = require_arrays(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                !r.iter()
                    .any(|item| l.iter().any(|other| json_equals(item, other))),
                left.clone(),
                right.clone(),
            ))
        }
        IsEmpty => {
            let len = container_length(op, left)?;
            Ok(OperatorVerdict::type_test(len == 0, left.clone()))
        }
        IsNonEmpty => {
            let len = container_length(op, left)?;
            Ok(OperatorVerdict::type_test(len > 0, left.clone()))
        }
        Length { expected } => {
            let len = container_length(op, left)?;
            Ok(OperatorVerdict::outcome(
                len == *expected,
                serde_json::json!(len),
                serde_json::json!(*expected),
            ))
        }
        LengthGreaterThan { min } => {
            let len = container_length(op, left)?;
            Ok(OperatorVerdict::outcome(
                len > *min,
                serde_json::json!(len),
                serde_json::json!({ "min": *min }),
            ))
        }
        LengthLessThan { max } => {
            let len = container_length(op, left)?;
            Ok(OperatorVerdict::outcome(
                len < *max,
                serde_json::json!(len),
                serde_json::json!({ "max": *max }),
            ))
        }
        UniqueValues => {
            let arr = require_array_left(op, left)?;
            let mut seen: Vec<&Value> = Vec::with_capacity(arr.len());
            let mut unique = true;
            for v in arr {
                if seen.iter().any(|other| json_equals(v, other)) {
                    unique = false;
                    break;
                }
                seen.push(v);
            }
            Ok(OperatorVerdict::type_test(unique, left.clone()))
        }
        IsTruthy => Ok(OperatorVerdict::type_test(is_truthy(left), left.clone())),
        IsFalsy => Ok(OperatorVerdict::type_test(!is_truthy(left), left.clone())),
        IsNull => Ok(OperatorVerdict::type_test(left.is_null(), left.clone())),
        IsNotNull => Ok(OperatorVerdict::type_test(!left.is_null(), left.clone())),
        IsType { expected } => Ok(OperatorVerdict::outcome(
            json_kind_matches(left, *expected),
            left.clone(),
            serde_json::json!(json_value_type_str(*expected)),
        )),
        IsString => Ok(OperatorVerdict::type_test(left.is_string(), left.clone())),
        IsNumber => Ok(OperatorVerdict::type_test(left.is_number(), left.clone())),
        IsBoolean => Ok(OperatorVerdict::type_test(left.is_boolean(), left.clone())),
        IsObject => Ok(OperatorVerdict::type_test(left.is_object(), left.clone())),
        WithinAbsTolerance { tolerance } => {
            let (l, r) = require_numbers(op, left, right)?;
            Ok(OperatorVerdict::outcome(
                (l - r).abs() <= *tolerance,
                json_num(l),
                json_num(r),
            ))
        }
        WithinPctTolerance { pct } => {
            let (l, r) = require_numbers(op, left, right)?;
            let passed = if r == 0.0 {
                l == 0.0
            } else {
                (l - r).abs() <= r.abs() * *pct
            };
            Ok(OperatorVerdict::outcome(passed, json_num(l), json_num(r)))
        }
        WithinStdDev {
            sigma,
            mean,
            std_dev,
        } => {
            let l = require_number_left(op, left)?;
            if *std_dev < 0.0 {
                return Err(EvalExecError::OperatorInvalidConfig {
                    op: op.discriminator().to_string(),
                    message: format!("std_dev must be non-negative, got {std_dev}"),
                });
            }
            let passed = (l - *mean).abs() <= *sigma * *std_dev;
            Ok(OperatorVerdict::outcome(
                passed,
                json_num(l),
                serde_json::json!({ "sigma": *sigma, "mean": *mean, "std_dev": *std_dev }),
            ))
        }
        BetweenPercentiles {
            lower_pct,
            upper_pct,
        } => {
            if !(0.0..=1.0).contains(lower_pct)
                || !(0.0..=1.0).contains(upper_pct)
                || lower_pct > upper_pct
            {
                return Err(EvalExecError::OperatorInvalidConfig {
                    op: op.discriminator().to_string(),
                    message: format!(
                        "percentile bounds must satisfy 0 <= lower_pct ({lower_pct}) \
                         <= upper_pct ({upper_pct}) <= 1"
                    ),
                });
            }
            let l = require_number_left(op, left)?;
            let passed = l >= *lower_pct && l <= *upper_pct;
            Ok(OperatorVerdict::outcome(
                passed,
                json_num(l),
                serde_json::json!({ "lower_pct": *lower_pct, "upper_pct": *upper_pct }),
            ))
        }
        DivergenceLessThan { metric, threshold } => {
            let (p, q) = require_distribution_pair(op, left, right)?;
            let value = match metric {
                DivergenceMetric::Kl => kl_divergence(&p, &q),
                DivergenceMetric::Js => js_divergence(&p, &q),
                DivergenceMetric::Wasserstein => wasserstein_1d(&p, &q),
            };
            Ok(OperatorVerdict::outcome(
                value < *threshold,
                json_num(value),
                serde_json::json!({ "metric": metric_str(*metric), "threshold": *threshold }),
            ))
        }
        CosineSimilarityAtLeast { threshold } => {
            let (l, r) = require_vector_pair(op, left, right)?;
            let value = cosine_similarity(&l, &r);
            Ok(OperatorVerdict::outcome(
                value >= *threshold,
                json_num(value),
                serde_json::json!({ "threshold": *threshold }),
            ))
        }
    }
}

fn mismatch(op: &ComparisonOperator, left: &Value, right: &Value) -> EvalExecError {
    EvalExecError::OperatorMismatch {
        op: op.discriminator().to_string(),
        left_kind: json_kind(left),
        right_kind: json_kind(right),
    }
}

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn json_num(n: f64) -> Value {
    serde_json::Number::from_f64(n)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn str_val(s: &str) -> Value {
    Value::String(s.to_string())
}

fn json_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x == y,
            _ => a == b,
        },
        _ => a == b,
    }
}

fn require_numbers(
    op: &ComparisonOperator,
    left: &Value,
    right: &Value,
) -> Result<(f64, f64), EvalExecError> {
    match (left.as_f64(), right.as_f64()) {
        (Some(l), Some(r)) => Ok((l, r)),
        _ => Err(mismatch(op, left, right)),
    }
}

fn require_number_left(op: &ComparisonOperator, left: &Value) -> Result<f64, EvalExecError> {
    left.as_f64()
        .ok_or_else(|| EvalExecError::OperatorMismatch {
            op: op.discriminator().to_string(),
            left_kind: json_kind(left),
            right_kind: "null",
        })
}

fn check_range_bounds(op: &ComparisonOperator, min: f64, max: f64) -> Result<(), EvalExecError> {
    if min > max {
        Err(EvalExecError::OperatorInvalidConfig {
            op: op.discriminator().to_string(),
            message: format!("range bounds invalid: min ({min}) > max ({max})"),
        })
    } else {
        Ok(())
    }
}

fn require_strings<'a>(
    op: &ComparisonOperator,
    left: &'a Value,
    right: &'a Value,
) -> Result<(&'a str, &'a str), EvalExecError> {
    match (left.as_str(), right.as_str()) {
        (Some(l), Some(r)) => Ok((l, r)),
        _ => Err(mismatch(op, left, right)),
    }
}

fn require_string_left<'a>(
    op: &ComparisonOperator,
    left: &'a Value,
) -> Result<&'a str, EvalExecError> {
    left.as_str()
        .ok_or_else(|| EvalExecError::OperatorMismatch {
            op: op.discriminator().to_string(),
            left_kind: json_kind(left),
            right_kind: "null",
        })
}

fn compile_regex(op: &ComparisonOperator, pattern: &str) -> Result<Regex, EvalExecError> {
    Regex::new(pattern).map_err(|source| EvalExecError::OperatorRegexInvalid {
        op: op.discriminator().to_string(),
        pattern: pattern.to_string(),
        source,
    })
}

fn validate_email_lite(s: &str) -> bool {
    let mut parts = s.splitn(2, '@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.contains(char::is_whitespace) || domain.contains(char::is_whitespace) {
        return false;
    }
    domain.contains('.')
}

fn validate_url_lite(s: &str) -> bool {
    let Some((scheme, rest)) = s.split_once("://") else {
        return false;
    };
    if scheme.is_empty() || rest.is_empty() {
        return false;
    }
    scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
}

fn require_array_left<'a>(
    op: &ComparisonOperator,
    left: &'a Value,
) -> Result<&'a Vec<Value>, EvalExecError> {
    left.as_array()
        .ok_or_else(|| EvalExecError::OperatorMismatch {
            op: op.discriminator().to_string(),
            left_kind: json_kind(left),
            right_kind: "null",
        })
}

fn require_array_right<'a>(
    op: &ComparisonOperator,
    right: &'a Value,
) -> Result<&'a Vec<Value>, EvalExecError> {
    right
        .as_array()
        .ok_or_else(|| EvalExecError::OperatorMismatch {
            op: op.discriminator().to_string(),
            left_kind: "null",
            right_kind: json_kind(right),
        })
}

fn require_arrays<'a>(
    op: &ComparisonOperator,
    left: &'a Value,
    right: &'a Value,
) -> Result<(&'a Vec<Value>, &'a Vec<Value>), EvalExecError> {
    match (left.as_array(), right.as_array()) {
        (Some(l), Some(r)) => Ok((l, r)),
        _ => Err(mismatch(op, left, right)),
    }
}

fn container_length(op: &ComparisonOperator, left: &Value) -> Result<usize, EvalExecError> {
    match left {
        Value::Array(a) => Ok(a.len()),
        Value::Object(o) => Ok(o.len()),
        Value::String(s) => Ok(s.chars().count()),
        _ => Err(EvalExecError::OperatorMismatch {
            op: op.discriminator().to_string(),
            left_kind: json_kind(left),
            right_kind: "null",
        }),
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn json_kind_matches(v: &Value, expected: JsonValueType) -> bool {
    matches!(
        (v, expected),
        (Value::Null, JsonValueType::Null)
            | (Value::Bool(_), JsonValueType::Bool)
            | (Value::Number(_), JsonValueType::Number)
            | (Value::String(_), JsonValueType::String)
            | (Value::Array(_), JsonValueType::Array)
            | (Value::Object(_), JsonValueType::Object)
    )
}

fn json_value_type_str(t: JsonValueType) -> &'static str {
    match t {
        JsonValueType::Null => "null",
        JsonValueType::Bool => "bool",
        JsonValueType::Number => "number",
        JsonValueType::String => "string",
        JsonValueType::Array => "array",
        JsonValueType::Object => "object",
    }
}

fn require_vector_pair(
    op: &ComparisonOperator,
    left: &Value,
    right: &Value,
) -> Result<(Vec<f64>, Vec<f64>), EvalExecError> {
    let (la, ra) = require_arrays(op, left, right)?;
    if la.len() != ra.len() {
        return Err(EvalExecError::OperatorInvalidConfig {
            op: op.discriminator().to_string(),
            message: format!(
                "vector lengths differ: observed {}, expected {}",
                la.len(),
                ra.len()
            ),
        });
    }
    let l = to_float_vec(op, la, "observed")?;
    let r = to_float_vec(op, ra, "expected")?;
    Ok((l, r))
}

fn require_distribution_pair(
    op: &ComparisonOperator,
    left: &Value,
    right: &Value,
) -> Result<(Vec<f64>, Vec<f64>), EvalExecError> {
    let (l, r) = require_vector_pair(op, left, right)?;
    if l.is_empty() {
        return Err(EvalExecError::OperatorInvalidConfig {
            op: op.discriminator().to_string(),
            message: "distribution vectors must be non-empty".to_string(),
        });
    }
    Ok((l, r))
}

fn to_float_vec(
    op: &ComparisonOperator,
    values: &[Value],
    side: &str,
) -> Result<Vec<f64>, EvalExecError> {
    values
        .iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| EvalExecError::OperatorInvalidConfig {
                    op: op.discriminator().to_string(),
                    message: format!("{side} vector contains non-numeric entry: {v}"),
                })
        })
        .collect()
}

const DIVERGENCE_EPSILON: f64 = 1e-12;

fn kl_divergence(p: &[f64], q: &[f64]) -> f64 {
    p.iter()
        .zip(q.iter())
        .map(|(pi, qi)| {
            let pi = pi + DIVERGENCE_EPSILON;
            let qi = qi + DIVERGENCE_EPSILON;
            pi * (pi / qi).ln()
        })
        .sum()
}

fn js_divergence(p: &[f64], q: &[f64]) -> f64 {
    let m: Vec<f64> = p
        .iter()
        .zip(q.iter())
        .map(|(pi, qi)| 0.5 * (pi + qi))
        .collect();
    0.5 * kl_divergence(p, &m) + 0.5 * kl_divergence(q, &m)
}

fn wasserstein_1d(p: &[f64], q: &[f64]) -> f64 {
    let mut cp = 0.0f64;
    let mut cq = 0.0f64;
    let mut total = 0.0f64;
    for (pi, qi) in p.iter().zip(q.iter()) {
        cp += pi;
        cq += qi;
        total += (cp - cq).abs();
    }
    total
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}

fn metric_str(m: DivergenceMetric) -> &'static str {
    match m {
        DivergenceMetric::Kl => "kl",
        DivergenceMetric::Js => "js",
        DivergenceMetric::Wasserstein => "wasserstein",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wyrd_spec::vala::eval::operator::ComparisonOperator as Op;

    fn run(left: Value, op: Op, right: Value) -> OperatorVerdict {
        evaluate_operator(&left, &op, &right).expect("operator evaluation should succeed")
    }

    #[test]
    fn equals_matches_across_numeric_types() {
        assert!(run(json!(1), Op::Equals, json!(1.0)).passed);
        assert!(!run(json!(1), Op::NotEquals, json!(1.0)).passed);
    }

    #[test]
    fn greater_than_rejects_non_numeric_with_typed_error() {
        let err = evaluate_operator(&json!("seven"), &Op::GreaterThan, &json!(3)).unwrap_err();
        assert!(matches!(
            err,
            EvalExecError::OperatorMismatch {
                op,
                left_kind: "string",
                right_kind: "number",
            } if op == "greater_than"
        ));
    }

    #[test]
    fn nan_operator_parameter_never_satisfies_threshold() {
        let v = run(
            json!(0.5),
            Op::InRange {
                min: f64::NAN,
                max: 1.0,
                inclusive: true,
            },
            Value::Null,
        );
        assert!(!v.passed);
    }

    #[test]
    fn in_range_rejects_inverted_bounds() {
        let err = evaluate_operator(
            &json!(0.5),
            &Op::InRange {
                min: 1.0,
                max: 0.0,
                inclusive: true,
            },
            &Value::Null,
        )
        .unwrap_err();
        assert!(matches!(err, EvalExecError::OperatorInvalidConfig { .. }));
    }

    #[test]
    fn approximately_equals_respects_tolerance() {
        let op = Op::ApproximatelyEquals { tolerance: 0.05 };
        assert!(run(json!(1.02), op.clone(), json!(1.0)).passed);
        assert!(!run(json!(1.10), op, json!(1.0)).passed);
    }

    #[test]
    fn contains_ignore_case_matches_lower_case_needle() {
        let v = run(json!("Hello World"), Op::ContainsIgnoreCase, json!("world"));
        assert!(v.passed);
    }

    #[test]
    fn matches_regex_compile_failure_is_typed_error() {
        let err = evaluate_operator(
            &json!("abc"),
            &Op::MatchesRegex {
                pattern: "[".to_string(),
            },
            &Value::Null,
        )
        .unwrap_err();
        assert!(matches!(err, EvalExecError::OperatorRegexInvalid { .. }));
    }

    #[test]
    fn matches_regex_passes_on_match() {
        let v = run(
            json!("user-1234"),
            Op::MatchesRegex {
                pattern: r"^user-\d+$".to_string(),
            },
            Value::Null,
        );
        assert!(v.passed);
    }

    #[test]
    fn is_uuid_validates_canonical_form() {
        assert!(
            run(
                json!("550e8400-e29b-41d4-a716-446655440000"),
                Op::IsUuid,
                Value::Null
            )
            .passed
        );
        assert!(!run(json!("not-a-uuid"), Op::IsUuid, Value::Null).passed);
    }

    #[test]
    fn in_membership_uses_json_equality() {
        let v = run(json!(2), Op::In, json!([1, 2.0, 3]));
        assert!(v.passed);
    }

    #[test]
    fn is_subset_handles_empty_left() {
        let v = run(json!([]), Op::IsSubset, json!([1, 2, 3]));
        assert!(v.passed);
    }

    #[test]
    fn unique_values_detects_duplicates() {
        assert!(run(json!([1, 2, 3]), Op::UniqueValues, Value::Null).passed);
        assert!(!run(json!([1, 2, 1]), Op::UniqueValues, Value::Null).passed);
    }

    #[test]
    fn empty_array_is_empty() {
        assert!(run(json!([]), Op::IsEmpty, Value::Null).passed);
        assert!(!run(json!([1]), Op::IsEmpty, Value::Null).passed);
    }

    #[test]
    fn is_type_matches_declared_kind() {
        let v = run(
            json!({ "a": 1 }),
            Op::IsType {
                expected: JsonValueType::Object,
            },
            Value::Null,
        );
        assert!(v.passed);
    }

    #[test]
    fn is_truthy_matches_runtime_semantics() {
        assert!(run(json!("non-empty"), Op::IsTruthy, Value::Null).passed);
        assert!(!run(json!(""), Op::IsTruthy, Value::Null).passed);
        assert!(!run(json!(0), Op::IsTruthy, Value::Null).passed);
        assert!(!run(Value::Null, Op::IsTruthy, Value::Null).passed);
    }

    #[test]
    fn within_pct_tolerance_handles_zero_baseline() {
        let op = Op::WithinPctTolerance { pct: 0.10 };
        assert!(run(json!(0.0), op.clone(), json!(0.0)).passed);
        assert!(!run(json!(0.01), op, json!(0.0)).passed);
    }

    #[test]
    fn between_percentiles_rejects_invalid_bounds() {
        let err = evaluate_operator(
            &json!(0.5),
            &Op::BetweenPercentiles {
                lower_pct: 0.8,
                upper_pct: 0.2,
            },
            &Value::Null,
        )
        .unwrap_err();
        assert!(matches!(err, EvalExecError::OperatorInvalidConfig { .. }));
    }

    #[test]
    fn divergence_kl_identical_distributions_below_threshold() {
        let v = run(
            json!([0.25, 0.25, 0.25, 0.25]),
            Op::DivergenceLessThan {
                metric: DivergenceMetric::Kl,
                threshold: 1e-6,
            },
            json!([0.25, 0.25, 0.25, 0.25]),
        );
        assert!(v.passed);
    }

    #[test]
    fn cosine_similarity_at_least_one_for_identical_vectors() {
        let v = run(
            json!([1.0, 0.0, 0.0]),
            Op::CosineSimilarityAtLeast { threshold: 0.99 },
            json!([1.0, 0.0, 0.0]),
        );
        assert!(v.passed);
    }
}
