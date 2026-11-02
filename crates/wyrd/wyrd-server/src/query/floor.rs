//! Pre-DataFusion SQL safety floor.
//!
//! Runs before any provider is built or any durable job row is written: a purely
//! syntactic gate over the SQL text that rejects anything that is not a single
//! `SELECT` and any function call outside the allowlist. It maps violations to the
//! stable public `WYRD_VALA_*` query codes through the one `WyrdError::Vala`
//! delegate — it never touches the database, the catalog, or a tenant.

use std::ops::ControlFlow;
use std::time::Duration;

use datafusion::sql::parser::{DFParser, Statement as DfStatement};
use datafusion::sql::sqlparser::ast::{
    BinaryOperator, Expr, Ident, ObjectName, ObjectNamePart, Statement as SqlStatement,
    UnaryOperator, Value, Visit, Visitor,
};
use datafusion::sql::sqlparser::dialect::GenericDialect;
use datafusion::sql::sqlparser::parser::Parser;
use datafusion::sql::sqlparser::tokenizer::{Token, Tokenizer, Whitespace};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::MAX_PREDICATE_BYTES;

/// Wall-clock budget for a sync query before the floor rejects it with 504.
pub const SYNC_QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum rows a sync query may return before the floor rejects it with 413.
pub const MAX_SYNC_RESULT_ROWS: usize = 1_000_000;

/// Maximum serialized Arrow IPC bytes a sync query may return before 413.
pub const MAX_SYNC_RESULT_BYTES: usize = 256 * 1024 * 1024;

/// Scalar, aggregate, and window functions the floor permits. Any function whose
/// bare (last-segment) name is absent is rejected as unsupported SQL. Kept
/// deliberately broad for analytics while excluding arbitrary/side-effecting UDFs.
const ALLOWED_FUNCTIONS: &[&str] = &[
    // aggregates
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "median",
    "mode",
    "variance",
    "var_pop",
    "var_samp",
    "stddev",
    "stddev_pop",
    "stddev_samp",
    "corr",
    "covar_pop",
    "covar_samp",
    "array_agg",
    "string_agg",
    "bit_and",
    "bit_or",
    "bit_xor",
    "bool_and",
    "bool_or",
    "first_value",
    "last_value",
    "approx_distinct",
    "approx_median",
    "approx_percentile_cont",
    "approx_percentile_cont_with_weight",
    "grouping",
    // window
    "row_number",
    "rank",
    "dense_rank",
    "percent_rank",
    "cume_dist",
    "ntile",
    "lag",
    "lead",
    "nth_value",
    // math
    "abs",
    "acos",
    "acosh",
    "asin",
    "asinh",
    "atan",
    "atan2",
    "atanh",
    "cbrt",
    "ceil",
    "ceiling",
    "cos",
    "cosh",
    "cot",
    "degrees",
    "exp",
    "factorial",
    "floor",
    "gcd",
    "isnan",
    "iszero",
    "lcm",
    "ln",
    "log",
    "log10",
    "log2",
    "nanvl",
    "pi",
    "power",
    "pow",
    "radians",
    "random",
    "round",
    "signum",
    "sign",
    "sin",
    "sinh",
    "sqrt",
    "tan",
    "tanh",
    "trunc",
    "mod",
    // string
    "ascii",
    "bit_length",
    "btrim",
    "char_length",
    "character_length",
    "chr",
    "concat",
    "concat_ws",
    "ends_with",
    "initcap",
    "instr",
    "left",
    "length",
    "levenshtein",
    "lower",
    "lpad",
    "ltrim",
    "md5",
    "octet_length",
    "overlay",
    "position",
    "repeat",
    "replace",
    "reverse",
    "right",
    "rpad",
    "rtrim",
    "sha224",
    "sha256",
    "sha384",
    "sha512",
    "split_part",
    "starts_with",
    "strpos",
    "substr",
    "substring",
    "to_hex",
    "translate",
    "trim",
    "upper",
    "uuid",
    "regexp_like",
    "regexp_match",
    "regexp_replace",
    // conditional / null
    "coalesce",
    "greatest",
    "ifnull",
    "least",
    "nullif",
    "nvl",
    "nvl2",
    // date / time
    "current_date",
    "current_time",
    "current_timestamp",
    "date_bin",
    "date_part",
    "date_trunc",
    "datepart",
    "datetrunc",
    "extract",
    "from_unixtime",
    "make_date",
    "now",
    "to_char",
    "to_date",
    "to_local_time",
    "to_timestamp",
    "to_timestamp_micros",
    "to_timestamp_millis",
    "to_timestamp_nanos",
    "to_timestamp_seconds",
    "to_unixtime",
    // conversion / typing
    "arrow_cast",
    "arrow_typeof",
    "cast",
    "try_cast",
    // array / struct / map
    "array_element",
    "array_length",
    "array_position",
    "array_slice",
    "cardinality",
    "get_field",
    "make_array",
    "named_struct",
    "struct",
    "unnest",
    "range",
];

/// Enforce the syntactic query floor: exactly one statement, a `SELECT`, and only
/// allowlisted functions.
///
/// # Errors
/// Returns [`WyrdError::Vala`] carrying `WYRD_VALA_400_QUERY_INVALID_SQL` when the
/// SQL fails to parse, is not a single `SELECT`, or calls a non-allowlisted
/// function.
pub fn validate_query_sql(sql: &str) -> Result<(), WyrdError> {
    let statement = parse_single_select(sql)?;
    let mut visitor = FunctionFloor { violation: None };
    let _ = statement.visit(&mut visitor);
    if let Some(function) = visitor.violation {
        return Err(invalid_sql(format!(
            "function `{function}` is not permitted by the query floor"
        )));
    }
    Ok(())
}

/// Validate one `WHERE` predicate and return it in canonical SQL.
///
/// The predicate is parsed by the same SQL parser queries use, as exactly one
/// expression ending the input. It may hold only column references `column`
/// admits, literals, comparisons, `AND`/`OR`/`NOT`, `IS [NOT] NULL` and the
/// boolean `IS` tests, literal `IN` lists, `BETWEEN`, and `[NOT] LIKE`/`ILIKE`.
/// Comments, placeholders, statements, subqueries, functions, casts, and every
/// other construct are refused, so the predicate can neither reach another
/// relation nor carry its own parameters. Blank input is no predicate and
/// normalizes to the empty string.
///
/// `column` receives each referenced name as the engine resolves it: unquoted
/// names lowercased, quoted names exact.
///
/// # Errors
/// Returns [`WyrdError::Vala`] carrying `WYRD_VALA_400_QUERY_INVALID_SQL` when
/// the predicate exceeds [`MAX_PREDICATE_BYTES`], does not parse as one
/// expression, uses a refused construct, or names a column `column` refuses.
pub fn normalize_predicate(
    expression: &str,
    column: impl Fn(&str) -> bool,
) -> Result<String, WyrdError> {
    if expression.len() > MAX_PREDICATE_BYTES {
        return Err(invalid_sql(format!(
            "the predicate is {} bytes; the limit is {MAX_PREDICATE_BYTES}",
            expression.len()
        )));
    }
    if expression.trim().is_empty() {
        return Ok(String::new());
    }
    let dialect = GenericDialect {};
    let tokens = Tokenizer::new(&dialect, expression)
        .tokenize()
        .map_err(|error| invalid_sql(format!("parse error: {error}")))?;
    for token in &tokens {
        match token {
            Token::Whitespace(
                Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_),
            ) => {
                return Err(invalid_sql(
                    "a predicate cannot contain comments".to_owned(),
                ));
            }
            Token::Placeholder(_) => {
                return Err(invalid_sql(
                    "a predicate cannot contain parameters".to_owned(),
                ));
            }
            Token::SemiColon => {
                return Err(invalid_sql(
                    "a predicate is one expression, not a statement".to_owned(),
                ));
            }
            _ => {}
        }
    }
    let mut parser = Parser::new(&dialect).with_tokens(tokens);
    let parsed = parser
        .parse_expr()
        .map_err(|error| invalid_sql(format!("parse error: {error}")))?;
    let rest = parser.peek_token();
    if rest.token != Token::EOF {
        return Err(invalid_sql(format!(
            "unexpected `{}` after the predicate",
            rest.token
        )));
    }
    check_predicate(&parsed, &column)?;
    Ok(parsed.to_string())
}

/// Refuse every construct a predicate may not hold, and unknown columns.
fn check_predicate(expr: &Expr, column: &impl Fn(&str) -> bool) -> Result<(), WyrdError> {
    match expr {
        Expr::Identifier(ident) => check_column(ident, column),
        Expr::Value(value) if matches!(value.value, Value::Placeholder(_)) => Err(invalid_sql(
            "a predicate cannot contain parameters".to_owned(),
        )),
        Expr::Value(_) | Expr::TypedString(_) => Ok(()),
        Expr::BinaryOp { left, op, right } => {
            if !matches!(
                op,
                BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Lt
                    | BinaryOperator::LtEq
                    | BinaryOperator::Gt
                    | BinaryOperator::GtEq
                    | BinaryOperator::And
                    | BinaryOperator::Or
            ) {
                return Err(refused(expr));
            }
            check_predicate(left, column)?;
            check_predicate(right, column)
        }
        Expr::UnaryOp { op, expr: operand } => {
            if !matches!(
                op,
                UnaryOperator::Not | UnaryOperator::Minus | UnaryOperator::Plus
            ) {
                return Err(refused(expr));
            }
            check_predicate(operand, column)
        }
        Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner) => check_predicate(inner, column),
        Expr::InList {
            expr: tested, list, ..
        } => {
            check_predicate(tested, column)?;
            for item in list {
                if !is_literal(item) {
                    return Err(invalid_sql(format!(
                        "`IN` lists hold literals only, not `{item}`"
                    )));
                }
            }
            Ok(())
        }
        Expr::Between {
            expr: tested,
            low,
            high,
            ..
        } => {
            check_predicate(tested, column)?;
            check_predicate(low, column)?;
            check_predicate(high, column)
        }
        Expr::Like {
            any: false,
            expr: tested,
            pattern,
            escape_char,
            ..
        }
        | Expr::ILike {
            any: false,
            expr: tested,
            pattern,
            escape_char,
            ..
        } => {
            check_predicate(tested, column)?;
            check_predicate(pattern, column)?;
            escape_char
                .as_deref()
                .map_or(Ok(()), |escape| check_predicate(escape, column))
        }
        _ => Err(refused(expr)),
    }
}

/// Whether `expr` is a literal, optionally signed.
fn is_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Value(value) => !matches!(value.value, Value::Placeholder(_)),
        Expr::TypedString(_) => true,
        Expr::UnaryOp {
            op: UnaryOperator::Minus | UnaryOperator::Plus,
            expr,
        } => is_literal(expr),
        _ => false,
    }
}

/// Refuse a column reference the table does not describe.
fn check_column(ident: &Ident, column: &impl Fn(&str) -> bool) -> Result<(), WyrdError> {
    let name = if ident.quote_style.is_some() {
        ident.value.clone()
    } else {
        ident.value.to_ascii_lowercase()
    };
    if column(&name) {
        Ok(())
    } else {
        Err(invalid_sql(format!(
            "`{name}` is not a column of this table"
        )))
    }
}

/// The refusal for a construct a predicate may not hold.
fn refused(expr: &Expr) -> WyrdError {
    invalid_sql(format!("`{expr}` is not permitted in a predicate"))
}

/// Best-effort list of the fully-qualified table names referenced by an
/// already-validated query, used by the sync handler to build only the providers
/// the query needs. Names that are not resolvable Bifrost tables (CTEs, aliases,
/// `information_schema`) are handled by the caller: it skips any it cannot resolve
/// and lets DataFusion planning surface a genuine unknown-table error.
#[must_use]
pub fn referenced_tables(sql: &str) -> Vec<String> {
    let Ok(statement) = parse_single_select(sql) else {
        return Vec::new();
    };
    let mut visitor = RelationCollector {
        relations: Vec::new(),
    };
    let _ = statement.visit(&mut visitor);
    visitor.relations
}

/// Public constructor for the invalid-SQL floor error, reused by the sync handler
/// when DataFusion planning rejects the (floor-valid) statement.
#[must_use]
pub fn invalid_sql(detail: String) -> WyrdError {
    ValaError::QueryInvalidSql { detail }.into()
}

/// Public constructor for the 413 result-too-large floor error.
#[must_use]
pub fn result_too_large() -> WyrdError {
    ValaError::QueryResultTooLarge.into()
}

/// Public constructor for the 504 query-timeout floor error.
#[must_use]
pub fn query_timeout() -> WyrdError {
    ValaError::QueryTimeout.into()
}

/// Parse `sql` and require it to be exactly one `SELECT` statement.
fn parse_single_select(sql: &str) -> Result<SqlStatement, WyrdError> {
    let statements =
        DFParser::parse_sql(sql).map_err(|error| invalid_sql(format!("parse error: {error}")))?;
    if statements.len() != 1 {
        return Err(invalid_sql(format!(
            "exactly one statement is required; found {}",
            statements.len()
        )));
    }
    match statements.into_iter().next() {
        Some(DfStatement::Statement(inner)) if matches!(*inner, SqlStatement::Query(_)) => {
            Ok(*inner)
        }
        _ => Err(invalid_sql(
            "only a single SELECT statement is supported".to_owned(),
        )),
    }
}

/// Reconstruct the dot-joined, unquoted name of a relation `ObjectName`.
fn object_name_to_string(name: &ObjectName) -> String {
    name.0
        .iter()
        .filter_map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
            ObjectNamePart::Function(_) => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Rejects the first function call whose bare name is not allowlisted.
struct FunctionFloor {
    violation: Option<String>,
}

impl Visitor for FunctionFloor {
    type Break = ();

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        if let Expr::Function(function) = expr {
            let full = object_name_to_string(&function.name).to_ascii_lowercase();
            let bare = full.rsplit('.').next().unwrap_or(full.as_str());
            if !ALLOWED_FUNCTIONS.contains(&bare) {
                self.violation = Some(bare.to_owned());
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }
}

/// Collects every relation name that appears in the statement.
struct RelationCollector {
    relations: Vec<String>,
}

impl Visitor for RelationCollector {
    type Break = ();

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        self.relations.push(object_name_to_string(relation));
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the query floor admits a grouped `SELECT` that calls only
    /// allowlisted aggregates such as `count` and `sum`.
    ///
    /// # Panics
    ///
    /// Panics when the floor refuses the allowlisted `SELECT`.
    #[test]
    fn query_floor_accepts_select_with_allowlisted_functions() {
        validate_query_sql("SELECT count(*), sum(n) FROM \"bifrost.bifrost.t\" GROUP BY model")
            .expect("select with count/sum passes the floor");
    }

    /// Proves the read-only query floor refuses every mutating or DDL statement
    /// before it can reach the engine.
    ///
    /// # Panics
    ///
    /// Panics when the floor admits a `DELETE`, `INSERT`, `DROP`, or `UPDATE`, or
    /// refuses one with anything but 400 `WYRD_VALA_400_QUERY_INVALID_SQL`.
    #[test]
    fn query_floor_rejects_non_select() {
        for sql in [
            "DELETE FROM \"bifrost.bifrost.t\"",
            "INSERT INTO \"bifrost.bifrost.t\" VALUES (1)",
            "DROP TABLE \"bifrost.bifrost.t\"",
            "UPDATE \"bifrost.bifrost.t\" SET n = 1",
        ] {
            let err = validate_query_sql(sql).expect_err("non-SELECT rejected");
            assert_eq!(err.status(), 400);
            assert_eq!(err.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");
        }
    }

    #[test]
    fn query_floor_rejects_multiple_statements() {
        let err = validate_query_sql("SELECT 1; SELECT 2").expect_err("multi-statement rejected");
        assert_eq!(err.status(), 400);
        assert_eq!(err.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");
    }

    /// Proves a function outside the allowlist, such as an unknown UDF, is
    /// refused even inside an otherwise valid `SELECT`.
    ///
    /// # Panics
    ///
    /// Panics when the floor admits `evil_udf` or refuses it with anything but
    /// 400 `WYRD_VALA_400_QUERY_INVALID_SQL`.
    #[test]
    fn query_floor_rejects_disallowed_function() {
        let err = validate_query_sql("SELECT evil_udf(payload) FROM \"bifrost.bifrost.t\"")
            .expect_err("unknown function rejected");
        assert_eq!(err.status(), 400);
        assert_eq!(err.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");
    }

    #[test]
    fn query_floor_rejects_unparseable_sql() {
        let err = validate_query_sql("not valid sql at all").expect_err("garbage rejected");
        assert_eq!(err.status(), 400);
    }

    /// Proves table extraction returns the quoted Bifrost relation a query
    /// reads, which authorization scopes are checked against.
    ///
    /// # Panics
    ///
    /// Panics when `bifrost.bifrost.events` is missing from the extracted tables.
    #[test]
    fn query_referenced_tables_extracts_relation_names() {
        let tables = referenced_tables("SELECT payload FROM \"bifrost.bifrost.events\"");
        assert!(
            tables.contains(&"bifrost.bifrost.events".to_owned()),
            "extracted: {tables:?}"
        );
    }

    #[test]
    fn query_referenced_tables_empty_for_tableless_select() {
        assert!(referenced_tables("SELECT 1").is_empty());
    }

    /// The columns the predicate tests below may reference.
    fn span_column(name: &str) -> bool {
        [
            "name",
            "status_code",
            "duration_nano",
            "service_name",
            "Mixed",
        ]
        .contains(&name)
    }

    /// Proves every permitted construct is accepted and comes back in the
    /// parser's canonical spelling.
    ///
    /// # Panics
    ///
    /// Panics when a permitted predicate is refused or normalizes differently.
    #[test]
    fn predicate_accepts_and_normalizes_permitted_constructs() {
        for (expression, canonical) in [
            ("name='a'", "name = 'a'"),
            (
                "(status_code = 2 OR duration_nano >= 1e6) and not service_name is null",
                "(status_code = 2 OR duration_nano >= 1e6) AND NOT service_name IS NULL",
            ),
            ("NAME in ('a', 'b') ", "NAME IN ('a', 'b')"),
            ("status_code not in (-1, 0)", "status_code NOT IN (-1, 0)"),
            (
                "duration_nano between 1 and 2",
                "duration_nano BETWEEN 1 AND 2",
            ),
            ("name ilike 'GET %'", "name ILIKE 'GET %'"),
            (
                "name not like 'a\\_%' escape '\\'",
                "name NOT LIKE 'a\\_%' ESCAPE '\\'",
            ),
            ("\"Mixed\" is not null", "\"Mixed\" IS NOT NULL"),
            (
                "(service_name = 'x') is true",
                "(service_name = 'x') IS TRUE",
            ),
        ] {
            assert_eq!(
                normalize_predicate(expression, span_column).expect(expression),
                canonical
            );
        }
        assert_eq!(normalize_predicate("  \n", span_column).expect("blank"), "");
    }

    /// Proves statements, injection, comments, parameters, subqueries,
    /// functions, unknown columns, and oversized input are each refused with the
    /// invalid-SQL code.
    ///
    /// # Panics
    ///
    /// Panics when any refused predicate is accepted or refused with another code.
    #[test]
    fn predicate_refuses_everything_else() {
        let oversized = format!("name = '{}'", "x".repeat(MAX_PREDICATE_BYTES));
        for expression in [
            "name = 'a'; DROP TABLE t",
            "name = 'a' -- trailing",
            "name = 'a' /* hidden */",
            "name = $1",
            "name = ?",
            "name IN (SELECT name FROM \"bifrost.traces.spans\")",
            "EXISTS (SELECT 1)",
            "lower(name) = 'a'",
            "CAST(status_code AS VARCHAR) = '2'",
            "status_code + 1 = 2",
            "name IN (service_name)",
            "secret = 1",
            "\"mixed\" = 1",
            "spans.name = 'a'",
            "name = 'a' ORDER BY name",
            "name = 'a') OR (1 = 1",
            "SELECT 1",
            oversized.as_str(),
        ] {
            let error = normalize_predicate(expression, span_column)
                .expect_err(&expression[..expression.len().min(60)]);
            assert_eq!(
                error.code(),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
                "{expression}"
            );
        }
    }
}
