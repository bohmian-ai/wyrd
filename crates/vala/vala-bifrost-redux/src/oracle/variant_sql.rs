//! The Oracle Variant SQL surface: `->`/`->>` lowering and the Variant functions.
//!
//! [`OracleVariantSql`] is the one owner every production Oracle session
//! installs before it plans, decodes, or executes anything. It holds:
//!
//! - `variant_get(v, path...)`, the semantic Variant access every literal
//!   `->` chain lowers to. A fully literal path runs Arrow-rs `variant_get`
//!   over the whole column; a path element that is not a literal is evaluated
//!   per row from the full root value.
//! - `variant_as_text(v)`, the text conversion `->>` adds after
//!   `variant_get`: a string as itself, any other value as its JSON text, and
//!   SQL null for an absent or JSON-null value.
//! - `parse_json(text)`, `try_parse_json(text)`, and `to_json(v)`.
//! - the expression planner that rewrites `->` and `->>` on a Variant operand.
//!
//! Struct access is not handled here: `s['field']` stays `DataFusion`'s exact
//! `get_field`. Every Variant result is the canonical unshredded storage under
//! the `arrow.parquet.variant` extension, so results keep the extension on the
//! wire and a chained `->` sees a Variant operand.
//!
//! The functions travel in physical plans by name; a peer rebuilds them from
//! its own session registry, which is why every session must install this
//! owner and why [`ORACLE_VARIANT_SQL_VERSION`] is bound into the plan and
//! stage digests peers verify before decoding.

use std::sync::{Arc, LazyLock};

use arrow::array::{Array, ArrayRef, AsArray, StringBuilder, StructArray};
use arrow::compute::cast;
use arrow::datatypes::{DataType, Field, FieldRef};
use datafusion::common::{DFSchema, ScalarValue, exec_err, plan_err};
use datafusion::error::{DataFusionError, Result};
use datafusion::execution::SessionStateBuilder;
use datafusion::logical_expr::expr::ScalarFunction;
use datafusion::logical_expr::planner::{ExprPlanner, PlannerResult, RawBinaryExpr};
use datafusion::logical_expr::{
    ColumnarValue, Expr, ExprSchemable, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF,
    ScalarUDFImpl, Signature, Volatility,
};
use datafusion::sql::sqlparser::ast::BinaryOperator;
use parquet_variant::{Variant, VariantPath, VariantPathElement};
use parquet_variant_compute::{
    GetOptions, VariantArray, VariantArrayBuilder, unshred_variant, variant_get,
};
use parquet_variant_json::VariantToJson;
use wyrd_queue::variant::{
    EncodedVariant, VariantColumnBuilder, VariantViolation, is_variant, mask_placeholders,
    variant_field, variant_storage_type,
};

use super::QueryCatalogError;

/// Version of the Oracle Variant SQL contract every peer must share.
///
/// Bound into the follower plan fingerprint and the Analytical stage body
/// digest, so a peer built against a different function set refuses a plan
/// before decoding it rather than resolving a function by name to different
/// semantics.
pub const ORACLE_VARIANT_SQL_VERSION: u32 = 1;

/// Name of the semantic Variant access function `->` lowers to.
const VARIANT_GET: &str = "variant_get";
/// Name of the text conversion `->>` applies after [`VARIANT_GET`].
const VARIANT_AS_TEXT: &str = "variant_as_text";
/// Name of the strict JSON-text parser.
const PARSE_JSON: &str = "parse_json";
/// Name of the null-on-invalid JSON-text parser.
const TRY_PARSE_JSON: &str = "try_parse_json";
/// Name of the Variant-to-JSON-text renderer.
const TO_JSON: &str = "to_json";

/// The process-wide Variant SQL owner; its functions are immutable.
static SHARED: LazyLock<OracleVariantSql> = LazyLock::new(OracleVariantSql::new);

/// The Variant SQL functions and operator planner one Oracle session installs.
///
/// One instance is shared by every session in the process: the functions are
/// stateless and immutable, so sessions differ only in which builder they
/// install into.
#[derive(Debug)]
pub struct OracleVariantSql {
    /// `variant_get`, `variant_as_text`, `parse_json`, `try_parse_json`, and
    /// `to_json`, in that order.
    functions: Vec<Arc<ScalarUDF>>,
    /// Rewrites `->` and `->>` on a Variant operand into the functions above.
    planner: Arc<dyn ExprPlanner>,
}

impl OracleVariantSql {
    /// Build the functions and the planner that refers to them.
    fn new() -> Self {
        let get = Arc::new(ScalarUDF::new_from_impl(VariantGet::new()));
        let as_text = Arc::new(ScalarUDF::new_from_impl(VariantAsText::new()));
        let planner = Arc::new(VariantOperatorPlanner {
            get: Arc::clone(&get),
            as_text: Arc::clone(&as_text),
        });
        Self {
            functions: vec![
                get,
                as_text,
                Arc::new(ScalarUDF::new_from_impl(ParseJson::new(false))),
                Arc::new(ScalarUDF::new_from_impl(ParseJson::new(true))),
                Arc::new(ScalarUDF::new_from_impl(ToJson::new())),
            ],
            planner,
        }
    }

    /// Return the process-wide owner every production session installs.
    #[must_use]
    pub fn shared() -> &'static Self {
        &SHARED
    }

    /// Install the Variant functions and operator planner into `builder`.
    ///
    /// Call it after any default features are applied: it appends to the
    /// builder's existing function and planner lists rather than replacing
    /// them, so the session keeps `DataFusion`'s built-ins (including
    /// `get_field` for Struct access) and gains the Variant surface.
    #[must_use]
    pub fn install(&self, mut builder: SessionStateBuilder) -> SessionStateBuilder {
        builder
            .scalar_functions()
            .get_or_insert_with(Vec::new)
            .extend(self.functions.iter().map(Arc::clone));
        builder
            .expr_planners()
            .get_or_insert_with(Vec::new)
            .push(Arc::clone(&self.planner));
        builder
    }
}

/// Lowers `->` and `->>` on a Variant operand to `variant_get`.
///
/// A chain `v -> 'a' -> 'b'` becomes one `variant_get(v, 'a', 'b')`, so the
/// whole literal path is one Arrow-rs path lookup. `->>` wraps that call in
/// `variant_as_text`. An operand that is not a Variant is left alone, so
/// `DataFusion` reports its ordinary type error.
#[derive(Debug)]
struct VariantOperatorPlanner {
    /// The registered `variant_get` function.
    get: Arc<ScalarUDF>,
    /// The registered `variant_as_text` function.
    as_text: Arc<ScalarUDF>,
}

impl ExprPlanner for VariantOperatorPlanner {
    /// Rewrite one `->`/`->>` whose left operand resolves to a Variant field.
    ///
    /// # Errors
    ///
    /// Returns the schema error raised while resolving the left operand's
    /// field.
    fn plan_binary_op(
        &self,
        expr: RawBinaryExpr,
        schema: &DFSchema,
    ) -> Result<PlannerResult<RawBinaryExpr>> {
        let as_text = match expr.op {
            BinaryOperator::Arrow => false,
            BinaryOperator::LongArrow => true,
            _ => return Ok(PlannerResult::Original(expr)),
        };
        let (_, field) = expr.left.to_field(schema)?;
        if !is_variant(&field) {
            return Ok(PlannerResult::Original(expr));
        }
        let mut args = match expr.left {
            Expr::ScalarFunction(call) if call.func.name() == VARIANT_GET => call.args,
            left => vec![left],
        };
        args.push(expr.right);
        let get = Expr::ScalarFunction(ScalarFunction::new_udf(Arc::clone(&self.get), args));
        Ok(PlannerResult::Planned(if as_text {
            self.as_text.call(vec![get])
        } else {
            get
        }))
    }
}

/// `variant_get(v, path...)`: the Variant value at a key/index path.
#[derive(Debug, PartialEq, Eq, Hash)]
struct VariantGet {
    /// Variadic: one Variant followed by one or more path elements.
    signature: Signature,
}

impl VariantGet {
    /// Declare the variadic signature; argument types are checked in
    /// [`ScalarUDFImpl::return_field_from_args`].
    fn new() -> Self {
        Self {
            signature: Signature::variadic_any(Volatility::Immutable),
        }
    }
}

impl ScalarUDFImpl for VariantGet {
    /// The SQL name `->` and `variant_get` calls resolve to.
    fn name(&self) -> &str {
        VARIANT_GET
    }

    /// The signature declared by the constructor; `DataFusion` coerces and
    /// checks call arguments against it before planning the return field.
    fn signature(&self) -> &Signature {
        &self.signature
    }

    /// The unshredded Variant storage type every path lookup returns.
    ///
    /// # Errors
    ///
    /// Never fails; the argument checks run in `return_field_from_args`.
    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(variant_storage_type())
    }

    /// Require a Variant root and string-key or integer-index path elements.
    ///
    /// # Errors
    ///
    /// Returns a plan error for a missing path, a non-Variant root, or a path
    /// element that is neither a string nor an integer.
    fn return_field_from_args(&self, args: ReturnFieldArgs) -> Result<FieldRef> {
        let [root, path @ ..] = args.arg_fields else {
            return plan_err!("variant_get requires a Variant and a path");
        };
        if path.is_empty() || !is_variant(root) {
            return plan_err!("variant_get requires a Variant and a path");
        }
        if let Some(bad) = path.iter().find(|field| !is_path_type(field.data_type())) {
            return plan_err!(
                "variant_get path elements are string keys or integer indexes, not {}",
                bad.data_type()
            );
        }
        Ok(Arc::new(variant_field(VARIANT_GET, true)))
    }

    /// Extract the path, with one Arrow-rs kernel call when it is all literal.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error raised while decoding the Variant column or
    /// normalizing the result to canonical storage.
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let rows = args.number_rows;
        let mut values = args.args.into_iter();
        let root = values
            .next()
            .ok_or_else(|| DataFusionError::Internal("variant_get has no root".to_owned()))?
            .into_array(rows)?;
        let root = mask_placeholders(root)?;
        let elements: Vec<ColumnarValue> = values.collect();
        let literal: Option<Vec<Option<VariantPathElement<'_>>>> = elements
            .iter()
            .map(|element| match element {
                ColumnarValue::Scalar(scalar) => Some(scalar_path_element(scalar)),
                ColumnarValue::Array(_) => None,
            })
            .collect();
        let result = match literal {
            // A literal null key matches nothing.
            Some(path) if path.iter().any(Option::is_none) => {
                let mut builder = VariantArrayBuilder::new(root.len());
                builder.append_nulls(root.len());
                builder.build()
            }
            Some(path) => {
                let path = VariantPath::new(path.into_iter().flatten().collect());
                VariantArray::try_new(&variant_get(&root, GetOptions::new_with_path(path))?)?
            }
            None => per_row_get(&root, &elements, rows)?,
        };
        Ok(ColumnarValue::Array(canonical_storage(&result)?))
    }
}

/// Report whether a path element type is a string key or an integer index.
fn is_path_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
    ) || data_type.is_integer()
}

/// Convert one literal path element; `None` for SQL null or a negative index.
fn scalar_path_element(scalar: &ScalarValue) -> Option<VariantPathElement<'static>> {
    if scalar.is_null() {
        return None;
    }
    match scalar {
        ScalarValue::Utf8(Some(key))
        | ScalarValue::LargeUtf8(Some(key))
        | ScalarValue::Utf8View(Some(key)) => Some(VariantPathElement::field(key.clone())),
        other => other
            .cast_to(&DataType::Int64)
            .ok()
            .and_then(|value| match value {
                ScalarValue::Int64(Some(index)) => usize::try_from(index).ok(),
                _ => None,
            })
            .map(VariantPathElement::index),
    }
}

/// Evaluate a path that has at least one non-literal element, row by row.
///
/// Each row resolves its own path from the full root value; a null key, a
/// negative index, or a missing step yields a null row.
///
/// # Errors
///
/// Returns the Arrow error raised while decoding the root or a path column.
fn per_row_get(root: &ArrayRef, elements: &[ColumnarValue], rows: usize) -> Result<VariantArray> {
    let root = VariantArray::try_new(root)?;
    let columns = elements
        .iter()
        .map(|element| element.to_array(rows))
        .collect::<Result<Vec<_>>>()?;
    let mut builder = VariantArrayBuilder::new(rows);
    for row in 0..rows {
        let path: Option<Vec<VariantPathElement<'static>>> = columns
            .iter()
            .map(|column| scalar_path_element(&ScalarValue::try_from_array(column, row).ok()?))
            .collect();
        let value = match path {
            Some(path) if root.is_valid(row) => {
                Some((root.try_value(row)?, VariantPath::new(path)))
            }
            _ => None,
        };
        match value
            .as_ref()
            .and_then(|(value, path)| value.get_path(path))
        {
            Some(found) => builder.append_variant(found),
            None => builder.append_null(),
        }
    }
    Ok(builder.build())
}

/// Rebuild a Variant result as the canonical unshredded `Binary` storage.
///
/// Arrow-rs kernels may return view or dictionary children, or a shredded
/// layout; every Oracle Variant result is normalized so the declared return
/// field and the produced array always agree.
///
/// # Errors
///
/// Returns the Arrow error raised while unshredding, casting a child, or
/// assembling the struct.
fn canonical_storage(variant: &VariantArray) -> Result<ArrayRef> {
    let variant = unshred_variant(variant)?;
    let DataType::Struct(fields) = variant_storage_type() else {
        return exec_err!("canonical Variant storage is a struct");
    };
    let metadata = cast(variant.metadata_column(), &DataType::Binary)?;
    let value = cast(variant.value_column(), &DataType::Binary)?;
    Ok(Arc::new(StructArray::try_new(
        fields,
        vec![metadata, value],
        variant.nulls().cloned(),
    )?))
}

/// Decode a Variant argument as a column of `rows` values.
///
/// # Errors
///
/// Returns the Arrow error raised when the argument is not Variant storage.
fn decode_rows(argument: &ColumnarValue, rows: usize) -> Result<VariantArray> {
    Ok(VariantArray::try_new(&mask_placeholders(
        argument.to_array(rows)?,
    )?)?)
}

/// `variant_as_text(v)`: the text `->>` returns.
#[derive(Debug, PartialEq, Eq, Hash)]
struct VariantAsText {
    /// One Variant argument.
    signature: Signature,
}

impl VariantAsText {
    /// Declare the one-argument signature; the argument is checked when the
    /// return field is resolved.
    fn new() -> Self {
        Self {
            signature: Signature::any(1, Volatility::Immutable),
        }
    }
}

impl ScalarUDFImpl for VariantAsText {
    /// The internal name the `->>` operator plans to.
    fn name(&self) -> &str {
        VARIANT_AS_TEXT
    }

    /// The signature declared by the constructor; `DataFusion` coerces and
    /// checks call arguments against it before planning the return field.
    fn signature(&self) -> &Signature {
        &self.signature
    }

    /// `Utf8`, the text every `->>` lookup returns.
    ///
    /// # Errors
    ///
    /// Never fails; the argument checks run in `return_field_from_args`.
    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(DataType::Utf8)
    }

    /// Require one Variant argument and return a nullable `Utf8` field.
    ///
    /// # Errors
    ///
    /// Returns a plan error when the argument is not a Variant.
    fn return_field_from_args(&self, args: ReturnFieldArgs) -> Result<FieldRef> {
        require_variant(VARIANT_AS_TEXT, args.arg_fields)?;
        Ok(Arc::new(Field::new(VARIANT_AS_TEXT, DataType::Utf8, true)))
    }

    /// Render each row: strings as themselves, other values as JSON text.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error raised while decoding a row or rendering JSON.
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let variant = decode_rows(&args.args[0], args.number_rows)?;
        let mut text = StringBuilder::with_capacity(variant.len(), 0);
        for row in 0..variant.len() {
            if variant.is_null(row) {
                text.append_null();
                continue;
            }
            match variant.try_value(row)? {
                Variant::Null => text.append_null(),
                value => match value.as_string() {
                    Some(string) => text.append_value(string),
                    None => text.append_value(value.to_json_value()?.to_string()),
                },
            }
        }
        Ok(ColumnarValue::Array(Arc::new(text.finish())))
    }
}

/// `to_json(v)`: the JSON text of a Variant value.
#[derive(Debug, PartialEq, Eq, Hash)]
struct ToJson {
    /// One Variant argument.
    signature: Signature,
}

impl ToJson {
    /// Declare the one-argument signature.
    fn new() -> Self {
        Self {
            signature: Signature::any(1, Volatility::Immutable),
        }
    }
}

impl ScalarUDFImpl for ToJson {
    /// The SQL name `to_json`.
    fn name(&self) -> &str {
        TO_JSON
    }

    /// The signature declared by the constructor; `DataFusion` coerces and
    /// checks call arguments against it before planning the return field.
    fn signature(&self) -> &Signature {
        &self.signature
    }

    /// `Utf8`, the JSON text of each Variant.
    ///
    /// # Errors
    ///
    /// Never fails; the argument checks run in `return_field_from_args`.
    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(DataType::Utf8)
    }

    /// Require one Variant argument and return a nullable `Utf8` field.
    ///
    /// # Errors
    ///
    /// Returns a plan error when the argument is not a Variant.
    fn return_field_from_args(&self, args: ReturnFieldArgs) -> Result<FieldRef> {
        require_variant(TO_JSON, args.arg_fields)?;
        Ok(Arc::new(Field::new(TO_JSON, DataType::Utf8, true)))
    }

    /// Render each non-null row as JSON text; a JSON null renders as `null`.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error raised while decoding a row or rendering JSON.
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let variant = decode_rows(&args.args[0], args.number_rows)?;
        let mut text = StringBuilder::with_capacity(variant.len(), 0);
        for row in 0..variant.len() {
            if variant.is_null(row) {
                text.append_null();
            } else {
                text.append_value(variant.try_value(row)?.to_json_value()?.to_string());
            }
        }
        Ok(ColumnarValue::Array(Arc::new(text.finish())))
    }
}

/// Require exactly one Variant argument.
///
/// # Errors
///
/// Returns a plan error naming `function` otherwise.
fn require_variant(function: &str, fields: &[FieldRef]) -> Result<()> {
    match fields {
        [field] if is_variant(field) => Ok(()),
        _ => plan_err!("{function} requires one Variant argument"),
    }
}

/// `parse_json(text)` and `try_parse_json(text)`.
///
/// Both encode through the shared Bifrost Variant rules, so a parsed value
/// obeys the same depth, size, and numeric limits as a written one.
#[derive(Debug, PartialEq, Eq, Hash)]
struct ParseJson {
    /// One string argument, coerced to `Utf8`.
    signature: Signature,
    /// Whether invalid JSON yields SQL null instead of an error.
    lenient: bool,
}

impl ParseJson {
    /// Declare the strict (`parse_json`) or lenient (`try_parse_json`) parser.
    fn new(lenient: bool) -> Self {
        Self {
            signature: Signature::uniform(1, vec![DataType::Utf8], Volatility::Immutable),
            lenient,
        }
    }
}

impl ScalarUDFImpl for ParseJson {
    /// `try_parse_json` for the lenient parser, otherwise `parse_json`.
    fn name(&self) -> &str {
        if self.lenient {
            TRY_PARSE_JSON
        } else {
            PARSE_JSON
        }
    }

    /// The signature declared by the constructor; `DataFusion` coerces and
    /// checks call arguments against it before planning the return field.
    fn signature(&self) -> &Signature {
        &self.signature
    }

    /// The unshredded Variant storage type of each parsed value.
    ///
    /// # Errors
    ///
    /// Never fails; the argument checks run in `return_field_from_args`.
    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(variant_storage_type())
    }

    /// A nullable Variant field named after the function, so the parsed
    /// column carries the `arrow.parquet.variant` extension downstream.
    ///
    /// # Errors
    ///
    /// Never fails; the signature already coerced the argument to `Utf8`.
    fn return_field_from_args(&self, _args: ReturnFieldArgs) -> Result<FieldRef> {
        Ok(Arc::new(variant_field(self.name(), true)))
    }

    /// Parse each non-null row into a Variant.
    ///
    /// # Errors
    ///
    /// Returns the catalogued Variant error, as an external `DataFusion`
    /// error, for the first row that is not storable — except that the lenient
    /// parser maps invalid JSON to SQL null. `row` in the error is the
    /// zero-based row of the evaluated batch.
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let array = args.args[0].to_array(args.number_rows)?;
        let texts = array.as_string::<i32>();
        let mut column = VariantColumnBuilder::with_capacity(texts.len());
        for (row, text) in texts.iter().enumerate() {
            let Some(text) = text else {
                column.append_null();
                continue;
            };
            match EncodedVariant::from_json_text(text) {
                Ok(value) => column.append(&value),
                Err(VariantViolation::InvalidJson { .. }) if self.lenient => column.append_null(),
                Err(violation) => {
                    let row = u64::try_from(row).unwrap_or(u64::MAX);
                    return Err(QueryCatalogError::external(
                        violation.into_error(self.name(), row),
                    ));
                }
            }
        }
        Ok(ColumnarValue::Array(column.finish()))
    }
}

/// Contract tests for the Variant SQL surface over an in-memory session.
#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{BinaryArray, Int64Array, RecordBatch, StringArray};
    use arrow::datatypes::Schema;
    use arrow::util::display::{ArrayFormatter, FormatOptions};
    use datafusion::datasource::MemTable;
    use datafusion::prelude::SessionContext;
    use std::error::Error;
    use wyrd_spec::vala::BifrostError;

    /// Builds a session with the Variant surface over one table `t` holding
    /// a Variant `v`, a Struct `s` with a Variant child, and a JSON text `j`.
    /// A `None` value makes that row's `v`, `j`, and `s` null.
    ///
    /// # Panics
    ///
    /// Panics when a fixture value does not encode or the table does not
    /// register.
    fn session(values: &[Option<&str>]) -> SessionContext {
        let mut variant = VariantColumnBuilder::with_capacity(values.len());
        for value in values {
            match value {
                Some(json) => variant.append(&EncodedVariant::from_json_text(json).expect("json")),
                None => variant.append_null(),
            }
        }
        let child = variant_field("features", false);
        let mut features = VariantColumnBuilder::with_capacity(values.len());
        for value in values {
            match value {
                Some(_) => {
                    features.append(&EncodedVariant::from_json_text(r#"{"k":1}"#).expect("json"));
                }
                None => features.append_null(),
            }
        }
        // A null parent leaves its child slot as a valid empty-bytes
        // placeholder, as the Parquet reader does.
        let (feature_fields, feature_children, _) =
            features.finish().as_struct().clone().into_parts();
        let features = StructArray::new(feature_fields, feature_children, None);
        let structs = StructArray::new(
            vec![
                Arc::new(Field::new("method", DataType::Utf8, false)),
                Arc::new(child.clone()),
            ]
            .into(),
            vec![
                Arc::new(StringArray::from(vec!["psi"; values.len()])) as ArrayRef,
                Arc::new(features),
            ],
            Some(values.iter().map(Option::is_some).collect()),
        );
        let schema = Arc::new(Schema::new(vec![
            variant_field("v", true),
            Field::new("s", structs.data_type().clone(), true),
            Field::new("j", DataType::Utf8, true),
            Field::new("n", DataType::Int64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                variant.finish(),
                Arc::new(structs),
                Arc::new(StringArray::from(
                    values
                        .iter()
                        .map(|value| value.map(|_| r#"{"a":"x"}"#))
                        .collect::<Vec<_>>(),
                )),
                Arc::new(Int64Array::from_iter_values((0..).take(values.len()))),
            ],
        )
        .expect("batch");
        let state = OracleVariantSql::shared()
            .install(SessionStateBuilder::new().with_default_features())
            .build();
        let context = SessionContext::new_with_state(state);
        context
            .register_table(
                "t",
                Arc::new(MemTable::try_new(schema, vec![vec![batch]]).expect("table")),
            )
            .expect("register");
        context
    }

    /// Runs `sql` and returns its first column rendered as display strings.
    ///
    /// # Panics
    ///
    /// Panics when the query fails to plan, execute, or render.
    async fn column(context: &SessionContext, sql: &str) -> Vec<String> {
        let batches = context
            .sql(sql)
            .await
            .expect(sql)
            .collect()
            .await
            .expect(sql);
        let mut out = Vec::new();
        for batch in batches {
            let rendered = ArrayFormatter::try_new(
                batch.column(0).as_ref(),
                &FormatOptions::default().with_null("NULL"),
            )
            .expect("formatter");
            for row in 0..batch.num_rows() {
                out.push(rendered.value(row).to_string());
            }
        }
        out
    }

    /// `->`/`->>` lower to `variant_get`, Struct access stays `get_field`,
    /// and the Variant functions keep the locked semantics.
    ///
    /// # Panics
    ///
    /// Panics when any operator or function result drifts.
    #[tokio::test]
    async fn variant_operators_and_functions_follow_the_contract() {
        let context = session(&[
            Some(r#"{"a":"x","n":7,"o":{"b":true},"z":null,"arr":[10,20]}"#),
            Some(r#"{"a":1.5}"#),
            None,
        ]);
        assert_eq!(
            column(&context, "SELECT v ->> 'a' FROM t ORDER BY n").await,
            ["x", "1.5", "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT v -> 'o' ->> 'b' FROM t ORDER BY n").await,
            ["true", "NULL", "NULL"]
        );
        assert_eq!(
            column(
                &context,
                "SELECT CAST(v ->> 'n' AS BIGINT) + 1 FROM t ORDER BY n"
            )
            .await,
            ["8", "NULL", "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT (v ->> 'z') IS NULL FROM t ORDER BY n").await,
            ["true", "true", "true"]
        );
        assert_eq!(
            column(&context, "SELECT to_json(v -> 'z') FROM t ORDER BY n").await,
            ["null", "NULL", "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT v -> 'arr' ->> 1 FROM t ORDER BY n").await,
            ["20", "NULL", "NULL"]
        );
        assert_eq!(
            column(
                &context,
                "SELECT v ->> (CASE WHEN n = 0 THEN 'a' END) FROM t ORDER BY n"
            )
            .await,
            ["x", "NULL", "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT to_json(v -> 'o') FROM t ORDER BY n").await,
            [r#"{"b":true}"#, "NULL", "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT parse_json(j) ->> 'a' FROM t ORDER BY n").await,
            ["x", "x", "NULL"]
        );
        assert_eq!(
            column(
                &context,
                "SELECT s['method'] FROM t WHERE s IS NOT NULL ORDER BY n"
            )
            .await,
            ["psi", "psi"]
        );
        assert_eq!(
            column(&context, "SELECT s['features'] ->> 'k' FROM t ORDER BY n").await,
            ["1", "1", "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT to_json(s['features']) FROM t ORDER BY n").await,
            [r#"{"k":1}"#, r#"{"k":1}"#, "NULL"]
        );
        assert_eq!(
            column(&context, "SELECT try_parse_json('{nope') IS NULL").await,
            ["true"]
        );

        let plan = context
            .sql("SELECT v -> 'o' ->> 'b', s['method'] FROM t")
            .await
            .expect("plans")
            .logical_plan()
            .display_indent()
            .to_string();
        assert!(
            plan.contains("variant_as_text(variant_get(t.v, Utf8(\"o\"), Utf8(\"b\")))"),
            "{plan}"
        );
        assert!(plan.contains("get_field(t.s, Utf8(\"method\"))"), "{plan}");
    }

    /// `parse_json` keeps every 64-bit integer exact and refuses with typed
    /// errors: invalid text, and an integer outside both 64-bit ranges.
    ///
    /// # Panics
    ///
    /// Panics when an integer loses digits or a refusal changes identity.
    #[tokio::test]
    async fn parse_json_keeps_exact_integers_and_refuses_the_rest() {
        let context = session(&[None]);
        assert_eq!(
            refusal(&context, "SELECT parse_json('{nope')").await.code(),
            "WYRD_VALA_400_VARIANT_INVALID_JSON"
        );

        let exact = format!(
            "[{},{},{},{}]",
            i64::MAX,
            i64::MAX.unsigned_abs() + 1,
            u64::MAX,
            i64::MIN
        );
        assert_eq!(
            column(&context, &format!("SELECT to_json(parse_json('{exact}'))")).await,
            [exact]
        );
        for token in ["18446744073709551616", "-9223372036854775809"] {
            assert_eq!(
                refusal(&context, &format!("SELECT parse_json('{token}')")).await,
                BifrostError::VariantNumericOutOfRange {
                    field: "parse_json".to_owned(),
                    row: 0,
                    path: String::new(),
                    numeric_kind: "integer".to_owned(),
                },
                "{token}"
            );
        }
    }

    /// Malformed stored Variant bytes fail every Variant function, never panic.
    ///
    /// Upstream decoding validates shallowly and panics on such bytes, so this
    /// proves each reader path validates the stored column first.
    ///
    /// # Panics
    ///
    /// Panics when a query over the malformed cell succeeds or panics.
    #[tokio::test]
    async fn malformed_stored_variant_fails_every_function_without_panicking() {
        let malformed = || Arc::new(BinaryArray::from(vec![&[0xff_u8][..]])) as ArrayRef;
        let DataType::Struct(storage) = variant_field("v", true).data_type().clone() else {
            panic!("Variant storage is a struct");
        };
        let schema = Arc::new(Schema::new(vec![
            variant_field("v", true),
            Field::new("k", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StructArray::new(
                    storage,
                    vec![malformed(), malformed()],
                    None,
                )),
                Arc::new(StringArray::from(vec![Some("a")])),
            ],
        )
        .expect("batch");
        let state = OracleVariantSql::shared()
            .install(SessionStateBuilder::new().with_default_features())
            .build();
        let context = SessionContext::new_with_state(state);
        context
            .register_table(
                "m",
                Arc::new(MemTable::try_new(schema, vec![vec![batch]]).expect("table")),
            )
            .expect("register");
        for sql in [
            "SELECT v -> 'a' FROM m",
            "SELECT v -> k FROM m",
            "SELECT v ->> 'a' FROM m",
            "SELECT to_json(v) FROM m",
        ] {
            let error = context
                .sql(sql)
                .await
                .expect(sql)
                .collect()
                .await
                .expect_err(sql);
            assert!(
                error.to_string().contains("is not a valid stored Variant"),
                "{sql} refuses the malformed cell instead of panicking: {error}"
            );
        }
    }

    /// Run a query expected to fail and return its typed Variant error.
    ///
    /// # Panics
    ///
    /// Panics when the query fails to plan, succeeds, or fails without a
    /// [`BifrostError`] in its source chain.
    async fn refusal(context: &SessionContext, sql: &str) -> BifrostError {
        let error = context
            .sql(sql)
            .await
            .expect(sql)
            .collect()
            .await
            .expect_err(sql);
        let mut source: Option<&(dyn Error + 'static)> = Some(&error);
        let mut found = None;
        while let Some(current) = source {
            if let Some(bifrost) = current.downcast_ref::<BifrostError>() {
                found = Some(bifrost.clone());
            }
            source = current.source();
        }
        found.expect("a typed Variant error")
    }
}
