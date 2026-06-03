use std::str::FromStr;

use wyrd_spec::vala::eval::operator::{ComparisonOperator, DivergenceMetric, JsonValueType};

#[test]
fn parameterless_round_trip_equals() {
    let op = ComparisonOperator::Equals;
    let json = serde_json::to_string(&op).expect("operator serializes");
    assert_eq!(json, r#""equals""#);
    let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
    assert_eq!(op, back);
}

#[test]
fn parameterized_round_trip_in_range() {
    let op = ComparisonOperator::InRange {
        min: 0.0,
        max: 1.0,
        inclusive: true,
    };
    let json = serde_json::to_string(&op).expect("operator serializes");
    assert_eq!(
        json,
        r#"{"kind":"in_range","min":0.0,"max":1.0,"inclusive":true}"#,
    );
    let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
    assert_eq!(op, back);
}

#[test]
fn parameterized_round_trip_matches_regex() {
    let op = ComparisonOperator::MatchesRegex {
        pattern: "^foo".into(),
    };
    let json = serde_json::to_string(&op).expect("operator serializes");
    let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
    assert_eq!(op, back);
}

#[test]
fn parameterized_round_trip_is_type() {
    let op = ComparisonOperator::IsType {
        expected: JsonValueType::String,
    };
    let json = serde_json::to_string(&op).expect("operator serializes");
    assert_eq!(json, r#"{"kind":"is_type","expected":"string"}"#);
    let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
    assert_eq!(op, back);
}

#[test]
fn redundant_object_shape_is_rejected() {
    let err = serde_json::from_str::<ComparisonOperator>(r#"{"operator":"is_not_null"}"#)
        .expect_err("redundant object shape must be rejected");

    assert!(
        err.to_string().contains("kind"),
        "error should point callers to the parameterized object discriminator: {err}"
    );
}

#[test]
fn parameterless_object_shape_is_rejected() {
    let err = serde_json::from_str::<ComparisonOperator>(r#"{"kind":"is_not_null"}"#)
        .expect_err("parameterless operators must use scalar strings");

    assert!(
        err.to_string().contains("is_not_null"),
        "error should name the bad discriminator: {err}"
    );
}

#[test]
fn parameterized_round_trip_divergence() {
    let op = ComparisonOperator::DivergenceLessThan {
        metric: DivergenceMetric::Kl,
        threshold: 0.05,
    };
    let json = serde_json::to_string(&op).expect("operator serializes");
    let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
    assert_eq!(op, back);
}

#[test]
fn discriminator_matches_serde_tag() {
    use ComparisonOperator::*;

    let pairs: &[(ComparisonOperator, &str)] = &[
        (Equals, "equals"),
        (NotEquals, "not_equals"),
        (
            WithinAbsTolerance { tolerance: 0.0 },
            "within_abs_tolerance",
        ),
        (
            IsType {
                expected: JsonValueType::Null,
            },
            "is_type",
        ),
        (
            DivergenceLessThan {
                metric: DivergenceMetric::Js,
                threshold: 0.0,
            },
            "divergence_less_than",
        ),
    ];

    for (op, want) in pairs {
        assert_eq!(op.discriminator(), *want);
        assert_eq!(op.as_str(), *want);
        let value = serde_json::to_value(op).expect("operator converts to json");
        if value.is_string() {
            assert_eq!(value.as_str(), Some(*want));
        } else {
            assert_eq!(value["kind"].as_str(), Some(*want));
        }
    }
}

#[test]
fn from_str_parses_parameterless() {
    assert_eq!(
        ComparisonOperator::from_str("equals").expect("equals parses"),
        ComparisonOperator::Equals,
    );
    assert_eq!(
        ComparisonOperator::from_discriminator("equals").expect("equals parses"),
        ComparisonOperator::Equals,
    );
    assert_eq!(
        ComparisonOperator::from_str("is_uuid").expect("is_uuid parses"),
        ComparisonOperator::IsUuid,
    );
}

#[test]
fn from_str_rejects_parameterized_without_params() {
    assert!(ComparisonOperator::from_str("in_range").is_err());
    assert!(ComparisonOperator::from_str("matches_regex").is_err());
    assert!(ComparisonOperator::from_str("divergence_less_than").is_err());
}

#[test]
fn from_str_rejects_unknown() {
    assert!(ComparisonOperator::from_str("not_a_real_operator").is_err());
}

#[test]
fn catalog_count_locked_at_56() {
    let all = [
        ComparisonOperator::Equals,
        ComparisonOperator::NotEquals,
        ComparisonOperator::GreaterThan,
        ComparisonOperator::GreaterThanOrEquals,
        ComparisonOperator::LessThan,
        ComparisonOperator::LessThanOrEquals,
        ComparisonOperator::InRange {
            min: 0.0,
            max: 1.0,
            inclusive: true,
        },
        ComparisonOperator::NotInRange {
            min: 0.0,
            max: 1.0,
            inclusive: false,
        },
        ComparisonOperator::ApproximatelyEquals { tolerance: 0.0 },
        ComparisonOperator::IsPositive,
        ComparisonOperator::IsNegative,
        ComparisonOperator::IsZero,
        ComparisonOperator::Contains,
        ComparisonOperator::NotContains,
        ComparisonOperator::ContainsIgnoreCase,
        ComparisonOperator::StartsWith,
        ComparisonOperator::EndsWith,
        ComparisonOperator::MatchesRegex {
            pattern: "x".into(),
        },
        ComparisonOperator::NotMatchesRegex {
            pattern: "x".into(),
        },
        ComparisonOperator::IsEmail,
        ComparisonOperator::IsUrl,
        ComparisonOperator::IsUuid,
        ComparisonOperator::IsIpv4,
        ComparisonOperator::IsIpv6,
        ComparisonOperator::HasMinLength { min: 1 },
        ComparisonOperator::HasMaxLength { max: 100 },
        ComparisonOperator::IsJson,
        ComparisonOperator::In,
        ComparisonOperator::NotIn,
        ComparisonOperator::IsSubset,
        ComparisonOperator::IsSuperset,
        ComparisonOperator::IsDisjoint,
        ComparisonOperator::AllOf,
        ComparisonOperator::AnyOf,
        ComparisonOperator::NoneOf,
        ComparisonOperator::IsEmpty,
        ComparisonOperator::IsNonEmpty,
        ComparisonOperator::Length { expected: 1 },
        ComparisonOperator::LengthGreaterThan { min: 1 },
        ComparisonOperator::LengthLessThan { max: 1 },
        ComparisonOperator::UniqueValues,
        ComparisonOperator::IsTruthy,
        ComparisonOperator::IsFalsy,
        ComparisonOperator::IsNull,
        ComparisonOperator::IsNotNull,
        ComparisonOperator::IsType {
            expected: JsonValueType::Null,
        },
        ComparisonOperator::IsString,
        ComparisonOperator::IsNumber,
        ComparisonOperator::IsBoolean,
        ComparisonOperator::IsObject,
        ComparisonOperator::WithinAbsTolerance { tolerance: 0.0 },
        ComparisonOperator::WithinPctTolerance { pct: 0.0 },
        ComparisonOperator::WithinStdDev {
            sigma: 0.0,
            mean: 0.0,
            std_dev: 1.0,
        },
        ComparisonOperator::BetweenPercentiles {
            lower_pct: 0.0,
            upper_pct: 1.0,
        },
        ComparisonOperator::DivergenceLessThan {
            metric: DivergenceMetric::Kl,
            threshold: 0.0,
        },
        ComparisonOperator::CosineSimilarityAtLeast { threshold: 0.0 },
    ];

    assert_eq!(all.len(), 56, "catalog count locked at 56");

    for op in &all {
        let json = serde_json::to_string(op).expect("operator serializes");
        let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
        assert_eq!(*op, back, "round-trip failed for {}", op.discriminator());
    }
}

#[test]
fn discriminators_are_unique() {
    use std::collections::HashSet;

    let names: HashSet<&str> = [
        "equals",
        "not_equals",
        "greater_than",
        "greater_than_or_equals",
        "less_than",
        "less_than_or_equals",
        "in_range",
        "not_in_range",
        "approximately_equals",
        "is_positive",
        "is_negative",
        "is_zero",
        "contains",
        "not_contains",
        "contains_ignore_case",
        "starts_with",
        "ends_with",
        "matches_regex",
        "not_matches_regex",
        "is_email",
        "is_url",
        "is_uuid",
        "is_ipv4",
        "is_ipv6",
        "has_min_length",
        "has_max_length",
        "is_json",
        "in",
        "not_in",
        "is_subset",
        "is_superset",
        "is_disjoint",
        "all_of",
        "any_of",
        "none_of",
        "is_empty",
        "is_non_empty",
        "length",
        "length_greater_than",
        "length_less_than",
        "unique_values",
        "is_truthy",
        "is_falsy",
        "is_null",
        "is_not_null",
        "is_type",
        "is_string",
        "is_number",
        "is_boolean",
        "is_object",
        "within_abs_tolerance",
        "within_pct_tolerance",
        "within_std_dev",
        "between_percentiles",
        "divergence_less_than",
        "cosine_similarity_at_least",
    ]
    .into_iter()
    .collect();

    assert_eq!(names.len(), 56);
}
