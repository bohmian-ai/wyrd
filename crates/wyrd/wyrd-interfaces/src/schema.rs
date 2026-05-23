//! Python/Rust wrappers for Wyrd data schema value objects.

use std::collections::BTreeMap;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyAny, PyModule};
#[cfg(feature = "python")]
use serde_json::Value;
#[cfg(feature = "python")]
use wyrd_utils::py::{json_to_pyobject, pyobject_to_json};

#[cfg(feature = "python")]
use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::DataSchema;
use wyrd_spec::card::field::{Dim, FieldSpec};
use wyrd_spec::ids::ColumnName;

/// Python-facing wrapper for a Wyrd `FieldSpec`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.data", name = "FieldSpec", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyFieldSpec {
    inner: FieldSpec,
}

impl PyFieldSpec {
    /// Build a wrapper from a Wyrd field spec.
    #[must_use]
    pub const fn from_inner(inner: FieldSpec) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped Wyrd field spec.
    #[must_use]
    pub const fn inner(&self) -> &FieldSpec {
        &self.inner
    }

    /// Unwrap the Wyrd field spec.
    #[must_use]
    pub fn into_inner(self) -> FieldSpec {
        self.inner
    }
}

impl From<FieldSpec> for PyFieldSpec {
    fn from(inner: FieldSpec) -> Self {
        Self::from_inner(inner)
    }
}

impl From<PyFieldSpec> for FieldSpec {
    fn from(value: PyFieldSpec) -> Self {
        value.into_inner()
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PyFieldSpec {
    #[new]
    #[pyo3(signature = (name, dtype, shape=None, nullable=false, extra=None))]
    fn py_new(
        name: &str,
        dtype: String,
        shape: Option<&Bound<'_, PyAny>>,
        nullable: bool,
        extra: Option<BTreeMap<String, String>>,
    ) -> CardPyResult<Self> {
        let name = ColumnName::new(name).map_err(|error| {
            WyrdPyError::validation_with_details(
                format!("invalid schema field name: {name}"),
                serde_json::json!({
                    "field": "name",
                    "value": name,
                    "source": error.to_string(),
                }),
            )
        })?;
        Ok(Self::from_inner(FieldSpec {
            name,
            dtype,
            shape: parse_dims(shape)?,
            nullable,
            extra: extra.unwrap_or_default(),
        }))
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.as_str().to_string()
    }

    #[getter]
    fn dtype(&self) -> String {
        self.inner.dtype.clone()
    }

    #[getter]
    fn shape(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(&self.inner.shape)?,
        )?)
    }

    #[getter]
    fn dims(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(&self.inner.shape)?,
        )?)
    }

    #[getter]
    fn nullable(&self) -> bool {
        self.inner.nullable
    }

    #[getter]
    fn extra(&self) -> BTreeMap<String, String> {
        self.inner.extra.clone()
    }

    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }
}

/// Python-facing wrapper for a Wyrd `DataSchema`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.data", name = "DataSchema", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyDataSchema {
    inner: DataSchema,
}

impl PyDataSchema {
    /// Build a wrapper from a Wyrd data schema.
    #[must_use]
    pub const fn from_inner(inner: DataSchema) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped Wyrd data schema.
    #[must_use]
    pub const fn inner(&self) -> &DataSchema {
        &self.inner
    }

    /// Unwrap the Wyrd data schema.
    #[must_use]
    pub fn into_inner(self) -> DataSchema {
        self.inner
    }
}

impl From<DataSchema> for PyDataSchema {
    fn from(inner: DataSchema) -> Self {
        Self::from_inner(inner)
    }
}

impl From<PyDataSchema> for DataSchema {
    fn from(value: PyDataSchema) -> Self {
        value.into_inner()
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PyDataSchema {
    #[new]
    #[pyo3(signature = (columns=None))]
    fn py_new(columns: Option<&Bound<'_, PyAny>>) -> CardPyResult<Self> {
        Ok(Self::from_inner(DataSchema::new(parse_columns(columns)?)))
    }

    #[getter]
    fn columns(&self) -> Vec<PyFieldSpec> {
        self.inner
            .columns
            .iter()
            .cloned()
            .map(PyFieldSpec::from)
            .collect()
    }

    #[getter]
    fn fields(&self) -> Vec<PyFieldSpec> {
        self.columns()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn contains_column(&self, name: &str) -> CardPyResult<bool> {
        let name = column_name(name)?;
        Ok(self.inner.contains_column(&name))
    }

    fn column(&self, name: &str) -> CardPyResult<Option<PyFieldSpec>> {
        let name = column_name(name)?;
        Ok(self.inner.column(&name).cloned().map(PyFieldSpec::from))
    }

    fn column_names(&self) -> Vec<String> {
        self.inner
            .column_names()
            .map(|name| name.as_str().to_string())
            .collect()
    }

    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }
}

/// Register data schema wrapper classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyFieldSpec>()?;
    module.add_class::<PyDataSchema>()?;
    Ok(())
}

#[cfg(feature = "python")]
fn column_name(value: &str) -> CardPyResult<ColumnName> {
    ColumnName::new(value).map_err(|error| {
        WyrdPyError::validation_with_details(
            format!("invalid schema column name: {value}"),
            serde_json::json!({
                "field": "name",
                "value": value,
                "source": error.to_string(),
            }),
        )
    })
}

#[cfg(feature = "python")]
fn parse_columns(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<Vec<FieldSpec>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(Vec::new());
    };

    let mut columns = Vec::new();
    for item in value.try_iter()? {
        let item = item?;
        if let Ok(field) = item.extract::<PyRef<'_, PyFieldSpec>>() {
            columns.push(field.inner.clone());
        } else {
            columns.push(serde_json::from_value(pyobject_to_json(&item)?)?);
        }
    }
    Ok(columns)
}

#[cfg(feature = "python")]
fn parse_dims(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<Vec<Dim>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(Vec::new());
    };
    dims_from_json(pyobject_to_json(value)?)
}

#[cfg(feature = "python")]
fn dims_from_json(value: Value) -> CardPyResult<Vec<Dim>> {
    match value {
        Value::Array(values) if values.iter().all(Value::is_i64) => values
            .into_iter()
            .map(|value| {
                value.as_i64().map(Dim::Fixed).ok_or_else(|| {
                    WyrdPyError::validation("schema dimension values must be signed integers")
                })
            })
            .collect(),
        value => Ok(serde_json::from_value(value)?),
    }
}

#[cfg(test)]
mod tests {
    use super::{PyDataSchema, PyFieldSpec};
    use wyrd_spec::card::data::DataSchema;
    use wyrd_spec::card::field::{Dim, FieldSpec};
    use wyrd_spec::ids::ColumnName;

    #[test]
    fn data_schema_round_trips_inner() {
        let field = FieldSpec {
            name: ColumnName::new("feature").expect("valid test column name"),
            dtype: "int64".to_string(),
            shape: vec![Dim::Fixed(3)],
            nullable: false,
            extra: Default::default(),
        };
        let schema = DataSchema::new(vec![field]);

        assert_eq!(PyDataSchema::from(schema.clone()).into_inner(), schema);
    }

    #[test]
    fn field_spec_round_trips_inner() {
        let field = FieldSpec {
            name: ColumnName::new("feature").expect("valid test column name"),
            dtype: "float64".to_string(),
            shape: Vec::new(),
            nullable: true,
            extra: Default::default(),
        };

        assert_eq!(PyFieldSpec::from(field.clone()).into_inner(), field);
    }
}
