//! Python/Rust wrappers for Wyrd data split declarations.

use serde_json::json;

use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::{Inequality, SplitStrategy};

#[cfg(feature = "python")]
use {
    chrono::{DateTime, Utc},
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyBool, PyFloat, PyInt, PyList, PyModule, PyString},
    wyrd_spec::card::data::ColValue,
    wyrd_spec::envelope::CardKind,
    wyrd_spec::ids::ColumnName,
    wyrd_spec::reference::CardRef,
    wyrd_utils::py::{json_to_pyobject, pyobject_to_json},
};

/// Python-facing builder for a Wyrd `SplitStrategy`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.data", name = "Split", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PySplit {
    inner: SplitStrategy,
}

impl PySplit {
    /// Build a wrapper from a Wyrd split strategy.
    #[must_use]
    pub const fn from_inner(inner: SplitStrategy) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped Wyrd split strategy.
    #[must_use]
    pub const fn inner(&self) -> &SplitStrategy {
        &self.inner
    }

    /// Unwrap the Wyrd split strategy.
    #[must_use]
    pub fn into_inner(self) -> SplitStrategy {
        self.inner
    }
}

impl From<SplitStrategy> for PySplit {
    fn from(inner: SplitStrategy) -> Self {
        Self::from_inner(inner)
    }
}

impl From<PySplit> for SplitStrategy {
    fn from(value: PySplit) -> Self {
        value.into_inner()
    }
}

/// Convert a Python split operator string into the locked Wyrd predicate enum.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_SPLIT_RULE` when `op` is not supported.
pub fn parse_inequality(op: &str) -> CardPyResult<Inequality> {
    match op {
        "==" => Ok(Inequality::Eq),
        "!=" => Ok(Inequality::Ne),
        "<" => Ok(Inequality::Lt),
        "<=" => Ok(Inequality::Le),
        ">" => Ok(Inequality::Gt),
        ">=" => Ok(Inequality::Ge),
        "in" => Ok(Inequality::In),
        _ => Err(invalid_split_rule(
            format!("invalid split operator {op:?}"),
            json!({
                "field": "op",
                "got": op,
                "accepted": ["==", "!=", "<", "<=", ">", ">=", "in"],
            }),
        )),
    }
}

/// Validate a Python slice-style index range split.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_SPLIT_RULE` when either bound is negative or
/// when `start > stop`.
pub fn validate_index_range(start: i64, stop: i64) -> CardPyResult<()> {
    if start < 0 || stop < 0 || start > stop {
        return Err(invalid_split_rule(
            "index range splits require non-negative start <= stop",
            json!({
                "start": start,
                "stop": stop,
            }),
        ));
    }
    Ok(())
}

/// Validate explicit split indices.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_SPLIT_RULE` when the split is empty or any
/// index is negative. Duplicate values are preserved for `DataSpec` validation.
pub fn validate_indices(values: &[i64]) -> CardPyResult<()> {
    let mut seen = std::collections::HashSet::new();
    if values.is_empty() {
        return Err(invalid_split_rule(
            "indices splits require at least one value",
            json!({ "values": values }),
        ));
    }
    if values
        .iter()
        .any(|value| *value < 0 || !seen.insert(*value))
    {
        return Err(invalid_split_rule(
            "indices splits require non-negative unique values",
            json!({ "values": values }),
        ));
    }
    Ok(())
}

#[cfg(feature = "python")]
#[pymethods]
impl PySplit {
    #[staticmethod]
    fn column(col: &str, op: &str, value: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        let name = ColumnName::new(col).map_err(|error| {
            invalid_split_rule(
                format!("invalid split column name {col:?}"),
                json!({
                    "field": "col",
                    "value": col,
                    "source": error.to_string(),
                }),
            )
        })?;
        Ok(Self::from_inner(SplitStrategy::Column {
            name,
            op: parse_inequality(op)?,
            value: col_value_from_py(value)?,
        }))
    }

    #[staticmethod]
    fn materialized(artifact_ref: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        Ok(Self::from_inner(SplitStrategy::Materialized(
            artifact_ref_from_py(artifact_ref)?,
        )))
    }

    #[staticmethod]
    fn index_range(start: i64, stop: i64) -> CardPyResult<Self> {
        validate_index_range(start, stop)?;
        Ok(Self::from_inner(SplitStrategy::IndexRange { start, stop }))
    }

    #[staticmethod]
    fn indices(values: Vec<i64>) -> CardPyResult<Self> {
        validate_indices(&values)?;
        Ok(Self::from_inner(SplitStrategy::Indices(values)))
    }

    #[getter]
    fn strategy(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }

    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }
}

/// Register the data split builder class on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PySplit>()?;
    Ok(())
}

fn invalid_split_rule(message: impl Into<String>, details: serde_json::Value) -> WyrdPyError {
    WyrdPyError::invalid_split_rule(message, details)
}

#[cfg(feature = "python")]
fn col_value_from_py(value: &Bound<'_, PyAny>) -> CardPyResult<ColValue> {
    if value.is_instance_of::<PyBool>() {
        return Ok(ColValue::Bool(value.extract()?));
    }
    if value.is_instance_of::<PyInt>() {
        return Ok(ColValue::Int(value.extract()?));
    }
    if value.is_instance_of::<PyFloat>() {
        return Ok(ColValue::Float(value.extract()?));
    }
    if value.is_instance_of::<PyString>() {
        return Ok(ColValue::Str(value.extract()?));
    }
    if let Ok(timestamp) = value.extract::<DateTime<Utc>>() {
        return Ok(ColValue::Timestamp(timestamp));
    }
    if value.is_instance_of::<PyList>() {
        let list = value.cast::<PyList>().map_err(|error| {
            invalid_split_rule(
                "failed to read split list value",
                json!({ "source": error.to_string() }),
            )
        })?;
        return list
            .iter()
            .map(|item| col_value_from_py(&item))
            .collect::<CardPyResult<Vec<_>>>()
            .map(ColValue::List);
    }
    let python_type = match value.get_type().name() {
        Ok(name) => name.to_string(),
        Err(error) => format!("<unknown: {error}>"),
    };
    Err(invalid_split_rule(
        "unsupported split column value type",
        json!({
            "python_type": python_type,
            "accepted": ["bool", "int", "float", "str", "datetime", "list"],
        }),
    ))
}

#[cfg(feature = "python")]
fn artifact_ref_from_py(value: &Bound<'_, PyAny>) -> CardPyResult<CardRef> {
    let raw = pyobject_to_json(value).map_err(|error| {
        invalid_split_rule(
            "materialized split artifact_ref must be a CardRef mapping",
            json!({ "source": error.to_string() }),
        )
    })?;
    let artifact_ref: CardRef = serde_json::from_value(raw.clone()).map_err(|error| {
        invalid_split_rule(
            "materialized split artifact_ref must be a valid CardRef",
            json!({
                "source": error.to_string(),
                "value": raw,
            }),
        )
    })?;
    if artifact_ref.kind != CardKind::Artifact {
        return Err(invalid_split_rule(
            "materialized split references must target Artifact cards",
            json!({
                "kind": artifact_ref.kind.wire_name(),
            }),
        ));
    }
    Ok(artifact_ref)
}

#[cfg(test)]
mod tests {
    use super::{parse_inequality, validate_index_range, validate_indices};
    use crate::error::WyrdPyError;
    use wyrd_spec::card::data::Inequality;

    #[test]
    fn parse_inequality_maps_locked_ops() {
        let cases = [
            ("==", Inequality::Eq),
            ("!=", Inequality::Ne),
            ("<", Inequality::Lt),
            ("<=", Inequality::Le),
            (">", Inequality::Gt),
            (">=", Inequality::Ge),
            ("in", Inequality::In),
        ];

        for (op, expected) in cases {
            assert_eq!(parse_inequality(op).expect("valid split op"), expected);
        }
    }

    #[test]
    fn parse_inequality_rejects_invalid_op_with_wyrd_code() {
        assert_invalid_split_rule(parse_inequality("contains").expect_err("invalid op"));
    }

    #[test]
    fn validate_index_range_allows_empty_slice() {
        validate_index_range(3, 3).expect("start == stop is a valid empty slice");
    }

    #[test]
    fn validate_index_range_rejects_invalid_bounds() {
        assert_invalid_split_rule(validate_index_range(4, 3).expect_err("start > stop"));
        assert_invalid_split_rule(validate_index_range(-1, 3).expect_err("negative start"));
        assert_invalid_split_rule(validate_index_range(0, -1).expect_err("negative stop"));
    }

    #[test]
    fn validate_indices_rejects_empty_negative_and_duplicate_values() {
        assert_invalid_split_rule(validate_indices(&[]).expect_err("empty indices"));
        assert_invalid_split_rule(validate_indices(&[0, -1]).expect_err("negative index"));
        assert_invalid_split_rule(validate_indices(&[1, 1]).expect_err("duplicate index"));
    }

    fn assert_invalid_split_rule(error: WyrdPyError) {
        match error {
            WyrdPyError::Spec(error) => {
                assert_eq!(error.code(), "WYRD_DATA_400_INVALID_SPLIT_RULE");
            }
            other => panic!("expected Wyrd spec error, got {other:?}"),
        }
    }
}
