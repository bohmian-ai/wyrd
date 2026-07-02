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
    Expr, ObjectName, ObjectNamePart, Statement as SqlStatement, Visit, Visitor,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError as ValaError;

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
    "count", "sum", "avg", "min", "max", "median", "mode", "variance", "var_pop", "var_samp",
    "stddev", "stddev_pop", "stddev_samp", "corr", "covar_pop", "covar_samp", "array_agg",
    "string_agg", "bit_and", "bit_or", "bit_xor", "bool_and", "bool_or", "first_value",
    "last_value", "approx_distinct", "approx_median", "approx_percentile_cont",
    "approx_percentile_cont_with_weight", "grouping",
    // window
    "row_number", "rank", "dense_rank", "percent_rank", "cume_dist", "ntile", "lag", "lead",
    "nth_value",
    // math
    "abs", "acos", "acosh", "asin", "asinh", "atan", "atan2", "atanh", "cbrt", "ceil", "ceiling",
    "cos", "cosh", "cot", "degrees", "exp", "factorial", "floor", "gcd", "isnan", "iszero", "lcm",
    "ln", "log", "log10", "log2", "nanvl", "pi", "power", "pow", "radians", "random", "round",
    "signum", "sign", "sin", "sinh", "sqrt", "tan", "tanh", "trunc", "mod",
    // string
    "ascii", "bit_length", "btrim", "char_length", "character_length", "chr", "concat",
    "concat_ws", "ends_with", "initcap", "instr", "left", "length", "levenshtein", "lower", "lpad",
    "ltrim", "md5", "octet_length", "overlay", "position", "repeat", "replace", "reverse", "right",
    "rpad", "rtrim", "sha224", "sha256", "sha384", "sha512", "split_part", "starts_with", "strpos",
    "substr", "substring", "to_hex", "translate", "trim", "upper", "uuid", "regexp_like",
    "regexp_match", "regexp_replace",
    // conditional / null
    "coalesce", "greatest", "ifnull", "least", "nullif", "nvl", "nvl2",
    // date / time
    "current_date", "current_time", "current_timestamp", "date_bin", "date_part", "date_trunc",
    "datepart", "datetrunc", "extract", "from_unixtime", "make_date", "now", "to_char", "to_date",
    "to_local_time", "to_timestamp", "to_timestamp_micros", "to_timestamp_millis",
    "to_timestamp_nanos", "to_timestamp_seconds", "to_unixtime",
    // conversion / typing
    "arrow_cast", "arrow_typeof", "cast", "try_cast",
    // array / struct / map
    "array_element", "array_length", "array_position", "array_slice", "cardinality", "get_field",
    "make_array", "named_struct", "struct", "unnest", "range",
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

    #[test]
    fn query_floor_accepts_select_with_allowlisted_functions() {
        validate_query_sql("SELECT count(*), sum(n) FROM \"vala.bifrost.t\" GROUP BY model")
            .expect("select with count/sum passes the floor");
    }

    #[test]
    fn query_floor_rejects_non_select() {
        for sql in [
            "DELETE FROM \"vala.bifrost.t\"",
            "INSERT INTO \"vala.bifrost.t\" VALUES (1)",
            "DROP TABLE \"vala.bifrost.t\"",
            "UPDATE \"vala.bifrost.t\" SET n = 1",
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

    #[test]
    fn query_floor_rejects_disallowed_function() {
        let err = validate_query_sql("SELECT evil_udf(payload) FROM \"vala.bifrost.t\"")
            .expect_err("unknown function rejected");
        assert_eq!(err.status(), 400);
        assert_eq!(err.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");
    }

    #[test]
    fn query_floor_rejects_unparseable_sql() {
        let err = validate_query_sql("not valid sql at all").expect_err("garbage rejected");
        assert_eq!(err.status(), 400);
    }

    #[test]
    fn query_referenced_tables_extracts_relation_names() {
        let tables = referenced_tables("SELECT payload FROM \"vala.bifrost.events\"");
        assert!(
            tables.contains(&"vala.bifrost.events".to_owned()),
            "extracted: {tables:?}"
        );
    }

    #[test]
    fn query_referenced_tables_empty_for_tableless_select() {
        assert!(referenced_tables("SELECT 1").is_empty());
    }
}
