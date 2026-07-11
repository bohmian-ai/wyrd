//! Compile a `MetadataQuery` into a parameterized Postgres predicate fragment.

use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};
use wyrd_spec::error::WyrdError;
use wyrd_spec::query::{
    FieldRef, MetadataQuery, Operator, Predicate, QueryFieldErrorDetail, Value, ValueType,
};

/// Max regex length accepted for `=~`/`!~`.
pub const MAX_REGEX_LEN: usize = 256;

/// How a resolved field maps onto physical SQL.
#[derive(Debug, Clone)]
pub enum FieldColumn {
    /// JSONB map column addressed by text extraction and existence.
    JsonbText {
        /// Physical column name.
        column: &'static str,
        /// The map key.
        key: String,
    },
    /// A first-class typed column.
    Typed {
        /// Physical column name.
        column: &'static str,
        /// The column's value type.
        ty: ValueType,
    },
}

/// Resolves an AST `FieldRef` to a physical column for a specific surface.
pub trait FieldResolver {
    /// Surface identifier used in structured error details.
    fn surface(&self) -> &'static str;

    /// Reserved columns this surface accepts.
    fn valid_fields(&self) -> &'static [&'static str];

    /// Resolve a field reference, or reject it.
    ///
    /// # Errors
    /// Returns `WYRD_QUERY_400_INVALID_FIELD` for fields this surface rejects.
    fn resolve(&self, field: &FieldRef) -> Result<FieldColumn, WyrdError>;
}

/// Append `query` as a boolean SQL predicate to `qb`.
///
/// The caller is responsible for the surrounding `WHERE`/`AND` and tenant
/// scoping. This only emits the predicate body.
///
/// # Errors
/// Returns query validation or field/operator errors.
pub fn compile_query(
    query: &MetadataQuery,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    query.validate()?;
    compile_query_inner(query, resolver, qb)
}

fn compile_query_inner(
    query: &MetadataQuery,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    match query {
        MetadataQuery::And(children) => compile_junction(children, " AND ", resolver, qb),
        MetadataQuery::Or(children) => compile_junction(children, " OR ", resolver, qb),
        MetadataQuery::Not(inner) => {
            qb.push("NOT (");
            compile_query_inner(inner, resolver, qb)?;
            qb.push(")");
            Ok(())
        }
        MetadataQuery::Predicate(predicate) => compile_predicate(predicate, resolver, qb),
    }
}

fn compile_junction(
    children: &[MetadataQuery],
    sep: &str,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    debug_assert!(
        !children.is_empty(),
        "empty group must be rejected by validate()"
    );
    qb.push("(");
    for (idx, child) in children.iter().enumerate() {
        if idx > 0 {
            qb.push(sep);
        }
        compile_query_inner(child, resolver, qb)?;
    }
    qb.push(")");
    Ok(())
}

fn compile_predicate(
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    let col = resolve_field(&predicate.field, resolver)?;
    match col {
        FieldColumn::JsonbText { column, key } => {
            compile_jsonb_predicate(column, &key, predicate, resolver, qb)
        }
        FieldColumn::Typed { column, ty } => {
            compile_typed_predicate(column, ty, predicate, resolver, qb)
        }
    }
}

fn resolve_field(field: &FieldRef, resolver: &dyn FieldResolver) -> Result<FieldColumn, WyrdError> {
    match field {
        FieldRef::Label(key) => Ok(FieldColumn::JsonbText {
            column: "labels",
            key: key.clone(),
        }),
        FieldRef::Annotation(key) => Ok(FieldColumn::JsonbText {
            column: "annotations",
            key: key.clone(),
        }),
        FieldRef::Reserved(_) | FieldRef::Attribute(_) => resolver.resolve(field),
    }
}

fn compile_jsonb_predicate(
    column: &'static str,
    key: &str,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    match &predicate.op {
        Operator::Eq(value) => {
            let value = expect_string(value, predicate, resolver, "=")?;
            push_jsonb_text(qb, column, key);
            qb.push(" = ");
            qb.push_bind(value.to_owned());
        }
        Operator::Ne(value) => {
            let value = expect_string(value, predicate, resolver, "!=")?;
            qb.push("(NOT (");
            push_jsonb_exists(qb, column, key);
            qb.push(") OR ");
            push_jsonb_text(qb, column, key);
            qb.push(" <> ");
            qb.push_bind(value.to_owned());
            qb.push(")");
        }
        Operator::Matches(regex) => {
            admit_regex(regex, predicate, resolver, "=~")?;
            push_jsonb_text(qb, column, key);
            qb.push(" ~ ");
            qb.push_bind(regex.clone());
        }
        Operator::NotMatches(regex) => {
            admit_regex(regex, predicate, resolver, "!~")?;
            qb.push("(NOT (");
            push_jsonb_exists(qb, column, key);
            qb.push(") OR ");
            push_jsonb_text(qb, column, key);
            qb.push(" !~ ");
            qb.push_bind(regex.clone());
            qb.push(")");
        }
        Operator::In(values) => {
            let values = expect_string_values(values, predicate, resolver, "in")?;
            push_jsonb_text(qb, column, key);
            qb.push(" = ANY(");
            qb.push_bind(values);
            qb.push(")");
        }
        Operator::NotIn(values) => {
            let values = expect_string_values(values, predicate, resolver, "not in")?;
            qb.push("(NOT (");
            push_jsonb_exists(qb, column, key);
            qb.push(") OR ");
            push_jsonb_text(qb, column, key);
            qb.push(" <> ALL(");
            qb.push_bind(values);
            qb.push("))");
        }
        Operator::Exists => push_jsonb_exists(qb, column, key),
        Operator::NotExists => {
            qb.push("NOT (");
            push_jsonb_exists(qb, column, key);
            qb.push(")");
        }
        Operator::Gt(_) | Operator::Ge(_) | Operator::Lt(_) | Operator::Le(_) => {
            return Err(ordering_requires_typed(
                predicate,
                resolver,
                operator_symbol(&predicate.op),
            ));
        }
    }
    Ok(())
}

fn compile_typed_predicate(
    column: &'static str,
    ty: ValueType,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    match &predicate.op {
        Operator::Eq(value) => {
            push_typed_comparison(column, "=", ty, value, predicate, resolver, qb)
        }
        Operator::Ne(value) => {
            // NULL-inclusive: match absent/NULL rows the same way JSONB Ne does.
            qb.push("(");
            qb.push(column);
            qb.push(" IS NULL OR ");
            push_typed_comparison(column, "<>", ty, value, predicate, resolver, qb)?;
            qb.push(")");
            Ok(())
        }
        Operator::Gt(value) => {
            ensure_orderable(ty, predicate, resolver, ">")?;
            push_typed_comparison(column, ">", ty, value, predicate, resolver, qb)
        }
        Operator::Ge(value) => {
            ensure_orderable(ty, predicate, resolver, ">=")?;
            push_typed_comparison(column, ">=", ty, value, predicate, resolver, qb)
        }
        Operator::Lt(value) => {
            ensure_orderable(ty, predicate, resolver, "<")?;
            push_typed_comparison(column, "<", ty, value, predicate, resolver, qb)
        }
        Operator::Le(value) => {
            ensure_orderable(ty, predicate, resolver, "<=")?;
            push_typed_comparison(column, "<=", ty, value, predicate, resolver, qb)
        }
        Operator::Matches(regex) => {
            if ty != ValueType::Str {
                return Err(regex_on_non_string(predicate, resolver, "=~", ty));
            }
            admit_regex(regex, predicate, resolver, "=~")?;
            qb.push(column);
            qb.push(" ~ ");
            qb.push_bind(regex.clone());
            Ok(())
        }
        Operator::NotMatches(regex) => {
            if ty != ValueType::Str {
                return Err(regex_on_non_string(predicate, resolver, "!~", ty));
            }
            admit_regex(regex, predicate, resolver, "!~")?;
            // NULL-inclusive: match absent/NULL rows the same way JSONB NotMatches does.
            qb.push("(");
            qb.push(column);
            qb.push(" IS NULL OR ");
            qb.push(column);
            qb.push(" !~ ");
            qb.push_bind(regex.clone());
            qb.push(")");
            Ok(())
        }
        Operator::In(values) => {
            push_typed_array(column, " = ANY", ty, values, predicate, resolver, qb)
        }
        Operator::NotIn(values) => {
            // NULL-inclusive: match absent/NULL rows the same way JSONB NotIn does.
            qb.push("(");
            qb.push(column);
            qb.push(" IS NULL OR ");
            push_typed_array(column, " <> ALL", ty, values, predicate, resolver, qb)?;
            qb.push(")");
            Ok(())
        }
        Operator::Exists => {
            qb.push(column);
            qb.push(" IS NOT NULL");
            Ok(())
        }
        Operator::NotExists => {
            qb.push(column);
            qb.push(" IS NULL");
            Ok(())
        }
    }
}

fn push_typed_comparison(
    column: &'static str,
    op: &'static str,
    ty: ValueType,
    value: &Value,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    qb.push(column);
    qb.push(" ");
    qb.push(op);
    qb.push(" ");
    push_typed_value(ty, value, predicate, resolver, op, qb)
}

fn push_typed_array(
    column: &'static str,
    op: &'static str,
    ty: ValueType,
    values: &[Value],
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    if ty == ValueType::Timestamp {
        return Err(type_mismatch(
            predicate,
            resolver,
            op.trim(),
            "non-timestamp",
            "timestamp",
        ));
    }
    qb.push(column);
    qb.push(op);
    qb.push("(");
    let operator = op.trim();
    match ty {
        ValueType::Str => {
            qb.push_bind(expect_string_values(values, predicate, resolver, operator)?)
        }
        ValueType::Int => qb.push_bind(collect_typed(
            values,
            predicate,
            resolver,
            operator,
            "int",
            |v| {
                if let Value::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            },
        )?),
        ValueType::Float => qb.push_bind(collect_typed(
            values,
            predicate,
            resolver,
            operator,
            "float",
            |v| {
                if let Value::Float(f) = v {
                    Some(*f)
                } else {
                    None
                }
            },
        )?),
        ValueType::Bool => qb.push_bind(collect_typed(
            values,
            predicate,
            resolver,
            operator,
            "bool",
            |v| {
                if let Value::Bool(b) = v {
                    Some(*b)
                } else {
                    None
                }
            },
        )?),
        ValueType::Duration => qb.push_bind(collect_typed(
            values,
            predicate,
            resolver,
            operator,
            "duration",
            |v| {
                if let Value::Duration(d) = v {
                    Some(*d)
                } else {
                    None
                }
            },
        )?),
        ValueType::Timestamp => unreachable!("timestamp arrays rejected above"),
    };
    qb.push(")");
    Ok(())
}

fn push_typed_value(
    ty: ValueType,
    value: &Value,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
    qb: &mut QueryBuilder<Postgres>,
) -> Result<(), WyrdError> {
    match (ty, value) {
        (ValueType::Str, Value::String(value)) => qb.push_bind(value.clone()),
        (ValueType::Int, Value::Int(value)) => qb.push_bind(*value),
        (ValueType::Float, Value::Float(value)) => qb.push_bind(*value),
        (ValueType::Bool, Value::Bool(value)) => qb.push_bind(*value),
        (ValueType::Duration, Value::Duration(value)) => qb.push_bind(*value),
        (ValueType::Timestamp, Value::Timestamp(value)) => qb.push_bind(*value),
        (ValueType::Timestamp, Value::String(value)) => {
            let parsed = parse_timestamp(value, predicate, resolver, operator)?;
            qb.push_bind(parsed)
        }
        _ => {
            return Err(type_mismatch(
                predicate,
                resolver,
                operator,
                ty.as_str(),
                value.value_type().as_str(),
            ));
        }
    };
    Ok(())
}

fn collect_typed<T>(
    values: &[Value],
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
    expected: &'static str,
    extract: impl Fn(&Value) -> Option<T>,
) -> Result<Vec<T>, WyrdError> {
    values
        .iter()
        .map(|v| {
            extract(v).ok_or_else(|| {
                type_mismatch(
                    predicate,
                    resolver,
                    operator,
                    expected,
                    v.value_type().as_str(),
                )
            })
        })
        .collect()
}

fn push_jsonb_text(qb: &mut QueryBuilder<Postgres>, column: &'static str, key: &str) {
    qb.push(column);
    qb.push(" ->> ");
    qb.push_bind(key.to_owned());
}

fn push_jsonb_exists(qb: &mut QueryBuilder<Postgres>, column: &'static str, key: &str) {
    qb.push(column);
    qb.push(" ? ");
    qb.push_bind(key.to_owned());
}

fn expect_string<'a>(
    value: &'a Value,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
) -> Result<&'a str, WyrdError> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(type_mismatch(
            predicate,
            resolver,
            operator,
            "string",
            other.value_type().as_str(),
        )),
    }
}

fn expect_string_values(
    values: &[Value],
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
) -> Result<Vec<String>, WyrdError> {
    values
        .iter()
        .map(|value| expect_string(value, predicate, resolver, operator).map(str::to_owned))
        .collect()
}

fn parse_timestamp(
    value: &str,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
) -> Result<DateTime<Utc>, WyrdError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| type_mismatch(predicate, resolver, operator, "timestamp", "string"))
}

fn ensure_orderable(
    ty: ValueType,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
) -> Result<(), WyrdError> {
    if ty.is_orderable() {
        Ok(())
    } else {
        Err(ordering_requires_typed(predicate, resolver, operator))
    }
}

fn admit_regex(
    regex: &str,
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
) -> Result<(), WyrdError> {
    if regex.len() > MAX_REGEX_LEN || regex::Regex::new(regex).is_err() {
        return Err(WyrdError::query_invalid_field_detail(
            "regex rejected by query prefilter",
            QueryFieldErrorDetail {
                reason: "regex_rejected",
                field: Some(&predicate.field.to_string()),
                surface: Some(resolver.surface()),
                operator: Some(operator),
                valid_fields: resolver.valid_fields(),
                ..Default::default()
            },
        ));
    }
    Ok(())
}

fn type_mismatch(
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
    expected_type: &'static str,
    actual_type: &'static str,
) -> WyrdError {
    WyrdError::query_invalid_field_detail(
        "query operator is not valid for the supplied field/value type",
        QueryFieldErrorDetail {
            reason: "operator_type_mismatch",
            field: Some(&predicate.field.to_string()),
            surface: Some(resolver.surface()),
            operator: Some(operator),
            expected_type: Some(expected_type),
            actual_type: Some(actual_type),
            valid_fields: resolver.valid_fields(),
        },
    )
}

fn ordering_requires_typed(
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
) -> WyrdError {
    WyrdError::query_invalid_field_detail(
        "ordering operators require a typed numeric, duration, or timestamp field",
        QueryFieldErrorDetail {
            reason: "ordering_requires_typed",
            field: Some(&predicate.field.to_string()),
            surface: Some(resolver.surface()),
            operator: Some(operator),
            valid_fields: resolver.valid_fields(),
            ..Default::default()
        },
    )
}

fn regex_on_non_string(
    predicate: &Predicate,
    resolver: &dyn FieldResolver,
    operator: &'static str,
    ty: ValueType,
) -> WyrdError {
    WyrdError::query_invalid_field_detail(
        "regex operators require a string field",
        QueryFieldErrorDetail {
            reason: "regex_on_non_string",
            field: Some(&predicate.field.to_string()),
            surface: Some(resolver.surface()),
            operator: Some(operator),
            expected_type: Some("string"),
            actual_type: Some(ty.as_str()),
            valid_fields: resolver.valid_fields(),
        },
    )
}

fn operator_symbol(op: &Operator) -> &'static str {
    match op {
        Operator::Eq(_) => "=",
        Operator::Ne(_) => "!=",
        Operator::Gt(_) => ">",
        Operator::Ge(_) => ">=",
        Operator::Lt(_) => "<",
        Operator::Le(_) => "<=",
        Operator::Matches(_) => "=~",
        Operator::NotMatches(_) => "!~",
        Operator::In(_) => "in",
        Operator::NotIn(_) => "not in",
        Operator::Exists => "exists",
        Operator::NotExists => "not exists",
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{Execute, Postgres, QueryBuilder};
    use wyrd_spec::query::{FieldRef, MetadataQuery, QueryFieldErrorDetail, ValueType};

    use super::{FieldColumn, FieldResolver, MAX_REGEX_LEN, compile_query};
    use wyrd_spec::error::WyrdError;

    struct TestResolver;

    impl FieldResolver for TestResolver {
        fn surface(&self) -> &'static str {
            "test"
        }

        fn valid_fields(&self) -> &'static [&'static str] {
            &["created_at", "status", "score"]
        }

        fn resolve(&self, field: &FieldRef) -> Result<FieldColumn, WyrdError> {
            match field {
                FieldRef::Reserved(column) if column == "created_at" => Ok(FieldColumn::Typed {
                    column: "created_at",
                    ty: ValueType::Timestamp,
                }),
                FieldRef::Reserved(column) if column == "status" => Ok(FieldColumn::Typed {
                    column: "status",
                    ty: ValueType::Str,
                }),
                FieldRef::Reserved(column) if column == "score" => Ok(FieldColumn::Typed {
                    column: "score",
                    ty: ValueType::Int,
                }),
                FieldRef::Reserved(column) => Err(WyrdError::query_invalid_field_detail(
                    format!("unknown field {column}"),
                    QueryFieldErrorDetail {
                        reason: "unknown_field",
                        field: Some(column),
                        surface: Some("test"),
                        valid_fields: self.valid_fields(),
                        ..Default::default()
                    },
                )),
                FieldRef::Attribute(_) => Err(WyrdError::query_invalid_field_detail(
                    "attributes are not queryable on this surface",
                    QueryFieldErrorDetail {
                        reason: "unknown_field",
                        surface: Some("test"),
                        valid_fields: self.valid_fields(),
                        ..Default::default()
                    },
                )),
                FieldRef::Label(_) | FieldRef::Annotation(_) => {
                    unreachable!("labels/annotations handled by compiler")
                }
            }
        }
    }

    fn compile(input: &str) -> Result<String, WyrdError> {
        let query = MetadataQuery::parse(input)?;
        compile_ast(&query)
    }

    fn compile_ast(query: &MetadataQuery) -> Result<String, WyrdError> {
        let mut qb = QueryBuilder::<Postgres>::new("");
        compile_query(query, &TestResolver, &mut qb)?;
        Ok(qb.build().sql().as_str().to_owned())
    }

    fn placeholder_count(sql: &str) -> usize {
        sql.matches('$').count()
    }

    #[test]
    fn compiles_jsonb_string_predicates() {
        let sql = compile("labels.env = \"prod\"").expect("query compiles");
        assert!(sql.contains("labels ->> $1 = $2"));
        assert_eq!(placeholder_count(&sql), 2);

        let sql = compile("not labels.deprecated exists").expect("query compiles");
        assert!(sql.contains("NOT (labels ? $1)"));

        let sql = compile("annotations.team =~ \"platform-.*\"").expect("query compiles");
        assert!(sql.contains("annotations ->> $1 ~ $2"));

        let sql = compile("labels.tier in [\"gold\",\"silver\"]").expect("query compiles");
        assert!(sql.contains("labels ->> $1 = ANY($2)"));
        assert_eq!(placeholder_count(&sql), 2);
    }

    #[test]
    fn compiles_timestamp_and_mixed_junction() {
        let sql = compile("created_at >= \"2026-01-01T00:00:00Z\"").expect("query compiles");
        assert!(sql.contains("created_at >= $1"));
        assert_eq!(placeholder_count(&sql), 1);

        let sql = compile("labels.release_at = \"2026-01-01T00:00:00Z\"").expect("query compiles");
        assert!(sql.contains("labels ->> $1 = $2"));

        let sql = compile("status = \"active\" and labels.env = \"prod\"").expect("query compiles");
        assert!(sql.contains("(status = $1 AND labels ->> $2 = $3)"));
        assert_eq!(placeholder_count(&sql), 3);
    }

    #[test]
    fn rejects_invalid_fields_and_operators() {
        let err = compile("labels.env > \"prod\"").expect_err("ordering rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_INVALID_FIELD");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["reason"], "ordering_requires_typed");
        assert_eq!(details["field"], "labels.env");
        assert_eq!(details["operator"], ">");

        let err = compile("created_at =~ \"x\"").expect_err("regex rejected");
        assert_eq!(
            err.as_problem_json()["details"]["reason"],
            "regex_on_non_string"
        );

        let err = compile("nonesuch = \"x\"").expect_err("unknown field rejected");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["reason"], "unknown_field");
        assert!(
            details["valid_fields"]
                .as_array()
                .expect("valid_fields is an array")
                .contains(&serde_json::json!("created_at"))
        );

        let err = compile("attributes.foo = \"x\"").expect_err("attributes rejected");
        assert_eq!(err.as_problem_json()["details"]["reason"], "unknown_field");
    }

    #[test]
    fn never_interpolates_user_values_into_sql() {
        let sql = compile("labels.env = \"' OR 1=1 --\"").expect("query compiles");
        assert!(!sql.contains("OR 1=1"));
        assert!(sql.contains("labels ->> $1 = $2"));
    }

    #[test]
    fn compiles_nested_negation() {
        let sql = compile("not (labels.a = \"1\" or labels.b = \"2\")").expect("query compiles");
        assert!(sql.contains("NOT ((labels ->> $1 = $2 OR labels ->> $3 = $4))"));
    }

    #[test]
    fn regex_admission_rejects_invalid_patterns() {
        let long = "a".repeat(MAX_REGEX_LEN + 1);
        let err = compile(&format!("labels.env =~ \"{long}\"")).expect_err("regex too long");
        assert_eq!(err.as_problem_json()["details"]["reason"], "regex_rejected");

        let err = compile("labels.env =~ \"(\"").expect_err("regex invalid");
        assert_eq!(err.as_problem_json()["details"]["reason"], "regex_rejected");
    }

    #[test]
    fn public_compiler_validates_direct_ast() {
        let err = compile_ast(&MetadataQuery::And(vec![])).expect_err("empty group rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_INVALID_FIELD");
        assert_eq!(err.as_problem_json()["details"]["reason"], "empty_group");
    }

    #[test]
    fn typed_negation_is_null_inclusive() {
        let sql = compile("status != \"active\"").expect("query compiles");
        assert!(
            sql.contains("status IS NULL OR status <> $"),
            "Ne must include NULL rows: {sql}"
        );

        let sql = compile("status !~ \"act.*\"").expect("query compiles");
        assert!(
            sql.contains("status IS NULL OR status !~ $"),
            "NotMatches must include NULL rows: {sql}"
        );

        let sql = compile("status not in [\"a\",\"b\"]").expect("query compiles");
        assert!(
            sql.contains("status IS NULL OR status <> ALL($"),
            "NotIn must include NULL rows: {sql}"
        );
    }

    #[test]
    fn typed_numeric_coercion_is_strict() {
        let err = compile("score = 1.5").expect_err("float value rejected for int column");
        assert_eq!(
            err.as_problem_json()["details"]["reason"],
            "operator_type_mismatch"
        );

        let err = compile("score in [1, 2.0]").expect_err("mixed list rejected for int column");
        assert_eq!(
            err.as_problem_json()["details"]["reason"],
            "operator_type_mismatch"
        );
    }
}
