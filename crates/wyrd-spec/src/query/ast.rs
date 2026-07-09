//! Typed metadata query AST.

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::query::value::Value;

/// Maximum number of leaf predicates in a single query.
pub const MAX_PREDICATES: usize = 64;
/// Maximum boolean nesting depth.
pub const MAX_DEPTH: usize = 8;

/// A boolean tree of metadata predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MetadataQuery {
    /// All sub-queries must match.
    And(Vec<MetadataQuery>),
    /// At least one sub-query must match.
    Or(Vec<MetadataQuery>),
    /// Negation.
    Not(Box<MetadataQuery>),
    /// A single leaf predicate.
    Predicate(Predicate),
}

/// A single field/operator predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Predicate {
    /// Field being tested.
    pub field: FieldRef,
    /// Operator and operand.
    pub op: Operator,
}

/// A reference to a queryable field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FieldRef {
    /// `labels.<key>`.
    Label(String),
    /// `annotations.<key>`.
    Annotation(String),
    /// `attributes.<key>`.
    Attribute(String),
    /// A bare reserved column name.
    Reserved(String),
}

/// Operator and operand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// `=`.
    Eq(Value),
    /// `!=`.
    Ne(Value),
    /// `>`.
    Gt(Value),
    /// `>=`.
    Ge(Value),
    /// `<`.
    Lt(Value),
    /// `<=`.
    Le(Value),
    /// `=~`.
    Matches(String),
    /// `!~`.
    NotMatches(String),
    /// `in [..]`.
    In(Vec<Value>),
    /// `not in [..]`.
    NotIn(Vec<Value>),
    /// `exists`.
    Exists,
    /// `not exists`.
    NotExists,
}

impl MetadataQuery {
    /// Parse the canonical string form into an AST, then validate guardrails.
    ///
    /// # Errors
    /// Returns `WYRD_QUERY_400_INVALID_SYNTAX` for lex/parse failures and
    /// `WYRD_QUERY_400_TOO_COMPLEX` when the parsed tree exceeds guardrails.
    pub fn parse(input: &str) -> Result<Self, WyrdError> {
        let query = crate::query::parse::parse_query(input)?;
        query.validate()?;
        Ok(query)
    }

    /// Validate structural guardrails.
    ///
    /// # Errors
    /// Returns a query error when the tree has an empty boolean group or exceeds
    /// the predicate-count or nesting-depth caps.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.has_empty_group() {
            return Err(WyrdError::query_empty_group());
        }
        let count = self.predicate_count();
        if count > MAX_PREDICATES {
            return Err(WyrdError::query_too_complex(
                format!("query has {count} predicates; max is {MAX_PREDICATES}"),
                "predicate_count",
                count,
                MAX_PREDICATES,
            ));
        }
        let depth = self.depth();
        if depth > MAX_DEPTH {
            return Err(WyrdError::query_too_complex(
                format!("query nesting depth {depth} exceeds max {MAX_DEPTH}"),
                "depth",
                depth,
                MAX_DEPTH,
            ));
        }
        Ok(())
    }

    fn has_empty_group(&self) -> bool {
        match self {
            Self::Predicate(_) => false,
            Self::Not(inner) => inner.has_empty_group(),
            Self::And(children) | Self::Or(children) => {
                children.is_empty() || children.iter().any(Self::has_empty_group)
            }
        }
    }

    /// Count leaf predicates.
    #[must_use]
    pub fn predicate_count(&self) -> usize {
        match self {
            Self::Predicate(_) => 1,
            Self::Not(inner) => inner.predicate_count(),
            Self::And(children) | Self::Or(children) => {
                children.iter().map(Self::predicate_count).sum()
            }
        }
    }

    /// Maximum boolean nesting depth. A bare predicate is depth 1.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Predicate(_) => 1,
            Self::Not(inner) => 1 + inner.depth(),
            Self::And(children) | Self::Or(children) => {
                1 + children.iter().map(Self::depth).max().unwrap_or(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::query::{FieldRef, MetadataQuery, Operator, Predicate, Value};

    fn pred(name: &str) -> MetadataQuery {
        MetadataQuery::Predicate(Predicate {
            field: FieldRef::Reserved(name.to_owned()),
            op: Operator::Eq(Value::String("x".to_owned())),
        })
    }

    #[test]
    fn counts_predicates_and_depth() {
        assert_eq!(pred("a").predicate_count(), 1);
        assert_eq!(pred("a").depth(), 1);

        let query = MetadataQuery::And(vec![
            pred("a"),
            MetadataQuery::Or(vec![pred("b"), MetadataQuery::Not(Box::new(pred("c")))]),
        ]);

        assert_eq!(query.predicate_count(), 3);
        assert_eq!(query.depth(), 4);
    }

    #[test]
    fn validates_predicate_count_cap() {
        let ok = MetadataQuery::And((0..64).map(|i| pred(&format!("f{i}"))).collect());
        assert!(ok.validate().is_ok());

        let too_many = MetadataQuery::And((0..65).map(|i| pred(&format!("f{i}"))).collect());
        let err = too_many.validate().expect_err("65 predicates is rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_TOO_COMPLEX");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["cap"], "predicate_count");
        assert_eq!(details["actual"], 65);
        assert_eq!(details["limit"], 64);
    }

    #[test]
    fn validates_depth_cap() {
        let mut ok = pred("a");
        for _ in 0..7 {
            ok = MetadataQuery::Not(Box::new(ok));
        }
        assert_eq!(ok.depth(), 8);
        assert!(ok.validate().is_ok());

        let too_deep = MetadataQuery::Not(Box::new(ok));
        let err = too_deep.validate().expect_err("depth 9 is rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_TOO_COMPLEX");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["cap"], "depth");
        assert_eq!(details["actual"], 9);
        assert_eq!(details["limit"], 8);
    }

    #[test]
    fn rejects_empty_boolean_groups() {
        for query in [
            MetadataQuery::And(vec![]),
            MetadataQuery::Or(vec![]),
            MetadataQuery::And(vec![MetadataQuery::Or(vec![])]),
        ] {
            let err = query.validate().expect_err("empty group is rejected");
            assert_eq!(err.code(), "WYRD_QUERY_400_INVALID_FIELD");
            assert_eq!(
                err.as_problem_json()["details"]["reason"],
                serde_json::json!("empty_group")
            );
        }
    }

    #[test]
    fn serde_roundtrips_all_operators_and_values() {
        let timestamp = chrono::Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp");
        let query = MetadataQuery::And(vec![
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Label("env".to_owned()),
                op: Operator::Eq(Value::String("prod".to_owned())),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Reserved("a".to_owned()),
                op: Operator::Ne(Value::Int(1)),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Reserved("b".to_owned()),
                op: Operator::Gt(Value::Float(1.5)),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Reserved("c".to_owned()),
                op: Operator::Ge(Value::Bool(true)),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Reserved("d".to_owned()),
                op: Operator::Lt(Value::Duration(10)),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Reserved("e".to_owned()),
                op: Operator::Le(Value::Timestamp(timestamp)),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Annotation("team".to_owned()),
                op: Operator::Matches("platform.*".to_owned()),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Annotation("team".to_owned()),
                op: Operator::NotMatches("x".to_owned()),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Label("tier".to_owned()),
                op: Operator::In(vec![Value::String("gold".to_owned())]),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Label("tier".to_owned()),
                op: Operator::NotIn(vec![Value::String("silver".to_owned())]),
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Label("deprecated".to_owned()),
                op: Operator::Exists,
            }),
            MetadataQuery::Predicate(Predicate {
                field: FieldRef::Label("deprecated".to_owned()),
                op: Operator::NotExists,
            }),
        ]);

        let value = serde_json::to_value(&query).expect("query serializes");
        let roundtrip: MetadataQuery = serde_json::from_value(value).expect("query deserializes");
        assert_eq!(roundtrip, query);
    }
}
