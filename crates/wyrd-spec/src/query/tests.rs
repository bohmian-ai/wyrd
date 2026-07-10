use proptest::prelude::*;

use crate::error::WyrdError;
use crate::query::{
    FieldRef, MAX_DEPTH, MAX_PREDICATES, MetadataQuery, Operator, Predicate, QueryFieldErrorDetail,
    Value,
};

fn pred(field: FieldRef, op: Operator) -> MetadataQuery {
    MetadataQuery::Predicate(Predicate { field, op })
}

fn reserved(name: &str, value: Value) -> MetadataQuery {
    pred(FieldRef::Reserved(name.to_owned()), Operator::Eq(value))
}

fn assert_invalid_syntax(input: &str, expected: &str, found: &str) {
    let err = MetadataQuery::parse(input).expect_err("query is invalid");
    let problem = err.as_problem_json();
    assert_eq!(problem["code"], "WYRD_QUERY_400_INVALID_SYNTAX");
    assert_eq!(problem["details"]["expected"], expected);
    assert_eq!(problem["details"]["found"], found);
}

#[test]
fn parses_operators_and_fields() {
    assert_eq!(
        MetadataQuery::parse("labels.env = \"prod\"").expect("query parses"),
        pred(
            FieldRef::Label("env".to_owned()),
            Operator::Eq(Value::String("prod".to_owned()))
        )
    );
    assert_eq!(
        MetadataQuery::parse("labels.env != \"prod\"").expect("query parses"),
        pred(
            FieldRef::Label("env".to_owned()),
            Operator::Ne(Value::String("prod".to_owned()))
        )
    );
    assert_eq!(
        MetadataQuery::parse("annotations.team =~ \"platform-.*\"").expect("query parses"),
        pred(
            FieldRef::Annotation("team".to_owned()),
            Operator::Matches("platform-.*".to_owned())
        )
    );
    assert_eq!(
        MetadataQuery::parse("annotations.team !~ \"x\"").expect("query parses"),
        pred(
            FieldRef::Annotation("team".to_owned()),
            Operator::NotMatches("x".to_owned())
        )
    );
    assert_eq!(
        MetadataQuery::parse("labels.tier in [\"gold\",\"silver\"]").expect("query parses"),
        pred(
            FieldRef::Label("tier".to_owned()),
            Operator::In(vec![
                Value::String("gold".to_owned()),
                Value::String("silver".to_owned())
            ])
        )
    );
    assert_eq!(
        MetadataQuery::parse("labels.tier not in [\"gold\"]").expect("query parses"),
        pred(
            FieldRef::Label("tier".to_owned()),
            Operator::NotIn(vec![Value::String("gold".to_owned())])
        )
    );
    assert_eq!(
        MetadataQuery::parse("labels.deprecated exists").expect("query parses"),
        pred(FieldRef::Label("deprecated".to_owned()), Operator::Exists)
    );
    assert_eq!(
        MetadataQuery::parse("labels.deprecated not exists").expect("query parses"),
        pred(
            FieldRef::Label("deprecated".to_owned()),
            Operator::NotExists
        )
    );
    assert_eq!(
        MetadataQuery::parse("not labels.deprecated exists").expect("query parses"),
        MetadataQuery::Not(Box::new(pred(
            FieldRef::Label("deprecated".to_owned()),
            Operator::Exists
        )))
    );
    assert_eq!(
        MetadataQuery::parse("created_at >= \"2026-01-01T00:00:00Z\"").expect("query parses"),
        pred(
            FieldRef::Reserved("created_at".to_owned()),
            Operator::Ge(Value::String("2026-01-01T00:00:00Z".to_owned()))
        )
    );
    assert_eq!(
        MetadataQuery::parse("labels.\"acme.com/team\" = \"x\"").expect("query parses"),
        pred(
            FieldRef::Label("acme.com/team".to_owned()),
            Operator::Eq(Value::String("x".to_owned()))
        )
    );
    assert_eq!(
        MetadataQuery::parse("attributes.http.status_code >= 500").expect("query parses"),
        pred(
            FieldRef::Attribute("http.status_code".to_owned()),
            Operator::Ge(Value::Int(500))
        )
    );
    assert_eq!(
        MetadataQuery::parse("labels.a.\"b.c\" = \"x\"").expect("query parses"),
        pred(
            FieldRef::Label("a.b.c".to_owned()),
            Operator::Eq(Value::String("x".to_owned()))
        )
    );
}

#[test]
fn parses_typed_literals() {
    assert_eq!(
        MetadataQuery::parse("x = 500").expect("query parses"),
        reserved("x", Value::Int(500))
    );
    assert_eq!(
        MetadataQuery::parse("x = 1.5").expect("query parses"),
        reserved("x", Value::Float(1.5))
    );
    assert_eq!(
        MetadataQuery::parse("x = true").expect("query parses"),
        reserved("x", Value::Bool(true))
    );
    for (raw, nanos) in [
        ("250ms", 250_000_000),
        ("1s", 1_000_000_000),
        ("2m", 120_000_000_000),
        ("1h", 3_600_000_000_000),
        ("10us", 10_000),
        ("5ns", 5),
    ] {
        assert_eq!(
            MetadataQuery::parse(&format!("x = {raw}")).expect("query parses"),
            reserved("x", Value::Duration(nanos))
        );
    }
    assert_eq!(
        MetadataQuery::parse("x = \"2026-01-01T00:00:00Z\"").expect("query parses"),
        reserved("x", Value::String("2026-01-01T00:00:00Z".to_owned()))
    );
    let err = MetadataQuery::parse("x = 2026-01-01T00:00:00Z").expect_err("query is invalid");
    assert!(
        err.as_problem_json()["detail"]
            .as_str()
            .expect("detail is a string")
            .contains("quoted")
    );
    assert_eq!(
        MetadataQuery::parse("labels.release_at = \"2026-01-01T00:00:00Z\"").expect("query parses"),
        pred(
            FieldRef::Label("release_at".to_owned()),
            Operator::Eq(Value::String("2026-01-01T00:00:00Z".to_owned()))
        )
    );
}

#[test]
fn parses_precedence_and_grouping() {
    assert_eq!(
        MetadataQuery::parse("a = 1 and b = 2 or c = 3").expect("query parses"),
        MetadataQuery::Or(vec![
            MetadataQuery::And(vec![
                reserved("a", Value::Int(1)),
                reserved("b", Value::Int(2))
            ]),
            reserved("c", Value::Int(3))
        ])
    );
    assert_eq!(
        MetadataQuery::parse("a = 1 or b = 2 and c = 3").expect("query parses"),
        MetadataQuery::Or(vec![
            reserved("a", Value::Int(1)),
            MetadataQuery::And(vec![
                reserved("b", Value::Int(2)),
                reserved("c", Value::Int(3))
            ])
        ])
    );
    assert_eq!(
        MetadataQuery::parse("not a = 1 and b = 2").expect("query parses"),
        MetadataQuery::And(vec![
            MetadataQuery::Not(Box::new(reserved("a", Value::Int(1)))),
            reserved("b", Value::Int(2))
        ])
    );
    assert_eq!(
        MetadataQuery::parse("(a = 1 or b = 2) and c = 3").expect("query parses"),
        MetadataQuery::And(vec![
            MetadataQuery::Or(vec![
                reserved("a", Value::Int(1)),
                reserved("b", Value::Int(2))
            ]),
            reserved("c", Value::Int(3))
        ])
    );
    assert!(matches!(
        MetadataQuery::parse("a = 1 and b = 2").expect("query parses"),
        MetadataQuery::And(_)
    ));
}

#[test]
fn parse_errors_have_structured_details() {
    let err = MetadataQuery::parse("labels.env == \"x\"").expect_err("query is invalid");
    let problem = err.as_problem_json();
    assert_eq!(problem["code"], "WYRD_QUERY_400_INVALID_SYNTAX");
    assert_eq!(problem["details"]["offset"], 12);
    assert_eq!(problem["details"]["expected"], "value");
    assert_eq!(problem["details"]["found"], "=");

    assert_invalid_syntax("labels.env =", "value", "<eof>");
    assert_invalid_syntax("labels.env in \"x\"", "[", "\"x\"");
    assert_invalid_syntax("foo bar baz", "operator", "bar");
    assert_invalid_syntax("a = 1 xyz", "EOF", "xyz");

    let err = MetadataQuery::parse("created_at.foo = \"x\"").expect_err("query is invalid");
    assert!(
        err.as_problem_json()["detail"]
            .as_str()
            .expect("detail is a string")
            .contains("only labels/annotations/attributes take a dotted key")
    );

    let err = MetadataQuery::parse("labels.a. = \"x\"").expect_err("query is invalid");
    assert!(
        err.as_problem_json()["detail"]
            .as_str()
            .expect("detail is a string")
            .contains("expected key segment")
    );
}

#[test]
fn validates_caps_and_empty_groups() {
    let ok = MetadataQuery::And(
        (0..MAX_PREDICATES)
            .map(|i| reserved(&format!("f{i}"), Value::Int(1)))
            .collect(),
    );
    assert!(ok.validate().is_ok());

    let too_many = MetadataQuery::And(
        (0..=MAX_PREDICATES)
            .map(|i| reserved(&format!("f{i}"), Value::Int(1)))
            .collect(),
    );
    let err = too_many.validate().expect_err("query is too complex");
    assert_eq!(err.code(), "WYRD_QUERY_400_TOO_COMPLEX");
    let problem = err.as_problem_json();
    assert_eq!(problem["details"]["cap"], "predicate_count");
    assert_eq!(problem["details"]["actual"], 65);
    assert_eq!(problem["details"]["limit"], 64);

    let mut depth_ok = reserved("a", Value::Int(1));
    for _ in 1..MAX_DEPTH {
        depth_ok = MetadataQuery::Not(Box::new(depth_ok));
    }
    assert_eq!(depth_ok.depth(), 8);
    assert!(depth_ok.validate().is_ok());

    let err = MetadataQuery::Not(Box::new(depth_ok))
        .validate()
        .expect_err("query is too deep");
    assert_eq!(err.as_problem_json()["details"]["cap"], "depth");

    for query in [
        MetadataQuery::And(vec![]),
        MetadataQuery::Or(vec![]),
        MetadataQuery::And(vec![MetadataQuery::Or(vec![])]),
    ] {
        let err = query.validate().expect_err("empty group is rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_INVALID_FIELD");
        assert_eq!(err.as_problem_json()["details"]["reason"], "empty_group");
    }

    let long = (0..=MAX_PREDICATES)
        .map(|i| format!("f{i} = 1"))
        .collect::<Vec<_>>()
        .join(" and ");
    assert_eq!(
        MetadataQuery::parse(&long)
            .expect_err("over-cap parse rejects")
            .code(),
        "WYRD_QUERY_400_TOO_COMPLEX"
    );
}

#[test]
fn query_field_detail_omits_none_fields() {
    let err = WyrdError::query_invalid_field_detail(
        "bad field",
        QueryFieldErrorDetail {
            reason: "operator_type_mismatch",
            field: Some("labels.env"),
            surface: Some("cards"),
            operator: Some(">"),
            expected_type: Some("timestamp"),
            actual_type: Some("string"),
            valid_fields: &["created_at"],
        },
    );
    let details = err.as_problem_json()["details"].clone();
    assert_eq!(details["reason"], "operator_type_mismatch");
    assert_eq!(details["field"], "labels.env");
    assert_eq!(details["surface"], "cards");
    assert_eq!(details["operator"], ">");
    assert_eq!(details["expected_type"], "timestamp");
    assert_eq!(details["actual_type"], "string");
    assert_eq!(details["valid_fields"][0], "created_at");

    let err = WyrdError::query_invalid_field_detail(
        "bad field",
        QueryFieldErrorDetail {
            reason: "unknown_field",
            ..Default::default()
        },
    );
    let details = err.as_problem_json()["details"].clone();
    assert_eq!(details["reason"], "unknown_field");
    assert!(details.get("field").is_none());
}

#[test]
fn display_roundtrips_examples_and_variants() {
    let examples = [
        "labels.env = \"prod\"",
        "labels.env != \"prod\"",
        "annotations.team =~ \"platform-.*\"",
        "annotations.team !~ \"x\"",
        "labels.tier in [\"gold\", \"silver\"]",
        "labels.tier not in [\"gold\"]",
        "labels.deprecated exists",
        "labels.deprecated not exists",
        "created_at >= \"2026-01-01T00:00:00Z\"",
        "attributes.http.status_code >= 500",
        "(a = 1 or b = 2) and c = 3",
        "not (labels.a = \"1\" or labels.b = \"2\")",
        "x = 250ms",
        "x = true",
        "x = 1.5",
    ];

    for example in examples {
        let ast = MetadataQuery::parse(example).expect("query parses");
        let rendered = ast.to_string();
        let reparsed = MetadataQuery::parse(&rendered).expect("rendered query parses");
        assert_eq!(
            reparsed, ast,
            "round-trip failed for {example} -> {rendered}"
        );
    }
}

#[test]
fn serde_roundtrips_mixed_query() {
    let query = MetadataQuery::parse("labels.env = \"prod\" and x = 1").expect("query parses");
    let value = serde_json::to_value(&query).expect("query serializes");
    let roundtrip: MetadataQuery = serde_json::from_value(value).expect("query deserializes");
    assert_eq!(roundtrip, query);
}

prop_compose! {
    fn key_strategy()(key in prop_oneof![
        "[a-z][a-z0-9_/.-]{0,12}",
        "[a-z]{1,4} [a-z]{1,4}",
    ]) -> String {
        key
    }
}

fn value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        "[a-z0-9 _/-]{0,16}".prop_map(Value::String),
        (-10_000_i64..10_000).prop_map(Value::Int),
        (-10_000_i64..10_000).prop_map(|v| Value::Float(v as f64 / 10.0)),
        any::<bool>().prop_map(Value::Bool),
        (0_i64..1_000_000).prop_map(Value::Duration),
    ]
}

fn field_strategy() -> impl Strategy<Value = FieldRef> {
    prop_oneof![
        key_strategy().prop_map(FieldRef::Label),
        key_strategy().prop_map(FieldRef::Annotation),
        key_strategy().prop_map(FieldRef::Attribute),
        "[a-z_][a-z0-9_]{0,12}".prop_map(FieldRef::Reserved),
    ]
}

fn predicate_strategy() -> impl Strategy<Value = MetadataQuery> {
    (field_strategy(), value_strategy()).prop_map(|(field, value)| {
        MetadataQuery::Predicate(Predicate {
            field,
            op: Operator::Eq(value),
        })
    })
}

fn query_strategy(depth: u32) -> BoxedStrategy<MetadataQuery> {
    let leaf = predicate_strategy().boxed();
    if depth == 0 {
        return leaf;
    }
    prop_oneof![
        leaf,
        query_strategy(depth - 1).prop_map(|q| MetadataQuery::Not(Box::new(q))),
        prop::collection::vec(query_strategy(depth - 1), 2..4).prop_map(MetadataQuery::And),
        prop::collection::vec(query_strategy(depth - 1), 2..4).prop_map(MetadataQuery::Or),
    ]
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn display_parse_roundtrip_for_small_queries(query in query_strategy(3)) {
        let rendered = query.to_string();
        let reparsed = MetadataQuery::parse(&rendered).expect("rendered query parses");
        prop_assert_eq!(reparsed, query);
    }
}
