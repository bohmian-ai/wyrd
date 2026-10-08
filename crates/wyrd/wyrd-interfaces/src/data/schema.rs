//! Python/Rust wrappers for Wyrd data schema value objects.

use wyrd_spec::card::data::DataSchema;
use wyrd_spec::card::field::FieldSpec;

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyModule},
    serde_json::Value,
    std::collections::BTreeMap,
    wyrd_spec::card::field::Dim,
    wyrd_spec::error::WyrdError,
    wyrd_spec::ids::ColumnName,
    wyrd_utils::py::WyrdPyResult,
    wyrd_utils::py::{json_to_pyobject, pyobject_to_json},
};

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
    /// Declare a field; only `name` is validated here.
    ///
    /// `dtype` is stored as given; `ModelSignature` validation rejects
    /// non-canonical Arrow dtype names. `shape` accepts serialized `Dim`
    /// values or plain integers (each a fixed dimension); omitted means no
    /// shape. Omitted `extra` is empty.
    ///
    /// # Errors
    /// Returns `WYRD_DATA_400_VALIDATION` when `name` is not a valid column
    /// name (1 to 64 characters, a lowercase letter then lowercase letters,
    /// digits, `_`, or `-`), or a Wyrd error when `shape` cannot be parsed.
    #[new]
    #[pyo3(signature = (name, dtype, shape=None, nullable=false, extra=None))]
    fn __new__(
        name: &str,
        dtype: String,
        shape: Option<&Bound<'_, PyAny>>,
        nullable: bool,
        extra: Option<BTreeMap<String, String>>,
    ) -> WyrdPyResult<Self> {
        let name = ColumnName::new(name).map_err(|error| {
            crate::error::validation_with_details(
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

    /// Field name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.as_str().to_string()
    }

    /// Declared Arrow logical dtype name.
    #[getter]
    fn dtype(&self) -> String {
        self.inner.dtype.clone()
    }

    /// Return the field shape as typed dimensions.
    #[getter]
    fn shape(&self) -> Vec<PyDim> {
        py_dims(&self.inner.shape)
    }

    /// Return the field shape as typed dimensions; alias of `shape`.
    #[getter]
    fn dims(&self) -> Vec<PyDim> {
        py_dims(&self.inner.shape)
    }

    /// Whether the field may hold nulls.
    #[getter]
    fn nullable(&self) -> bool {
        self.inner.nullable
    }

    /// String metadata stored with the field.
    #[getter]
    fn extra(&self) -> BTreeMap<String, String> {
        self.inner.extra.clone()
    }

    /// Return this field as a JSON-compatible spec dictionary.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON conversion fails.
    fn to_dict(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }
}

/// Python-facing projection of one Wyrd shape [`Dim`].
///
/// Shapes read back as typed dimensions (`Dim.fixed(3)`,
/// `Dim.dynamic("batch")`) instead of serialized `{"kind", "value"}` maps, and
/// `FieldSpec(shape=[...])` accepts them directly. Shape invariants such as
/// positive fixed lengths stay with the owning spec validator.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.data", name = "Dim", frozen, eq, skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyDim {
    inner: wyrd_spec::card::field::Dim,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyDim {
    /// Declare a fixed, known dimension length.
    #[staticmethod]
    fn fixed(length: i64) -> Self {
        Self {
            inner: Dim::Fixed(length),
        }
    }

    /// Declare a dynamic dimension, optionally named such as `"batch"`.
    #[staticmethod]
    #[pyo3(signature = (name=None))]
    fn dynamic(name: Option<String>) -> Self {
        Self {
            inner: Dim::Dynamic(name),
        }
    }

    /// Return `"Fixed"` or `"Dynamic"`.
    #[getter]
    const fn kind(&self) -> &'static str {
        match self.inner {
            Dim::Fixed(_) => "Fixed",
            Dim::Dynamic(_) => "Dynamic",
        }
    }

    /// Return the fixed length, or `None` for a dynamic dimension.
    #[getter]
    const fn length(&self) -> Option<i64> {
        match self.inner {
            Dim::Fixed(length) => Some(length),
            Dim::Dynamic(_) => None,
        }
    }

    /// Return the dynamic dimension name, or `None` when fixed or unnamed.
    #[getter]
    fn name(&self) -> Option<&str> {
        match &self.inner {
            Dim::Fixed(_) => None,
            Dim::Dynamic(name) => name.as_deref(),
        }
    }

    /// Return a constructor-shaped representation.
    fn __repr__(&self) -> String {
        match &self.inner {
            Dim::Fixed(length) => format!("Dim.fixed({length})"),
            Dim::Dynamic(Some(name)) => format!("Dim.dynamic({name:?})"),
            Dim::Dynamic(None) => "Dim.dynamic()".to_owned(),
        }
    }
}

/// Project a native shape into typed Python dimensions.
#[cfg(feature = "python")]
fn py_dims(shape: &[Dim]) -> Vec<PyDim> {
    shape.iter().cloned().map(|inner| PyDim { inner }).collect()
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
    /// Create a schema from ordered `FieldSpec` objects or serialized field
    /// dictionaries; omitted means an empty schema.
    ///
    /// # Errors
    /// Returns a Wyrd error when a serialized field cannot be parsed.
    #[new]
    #[pyo3(signature = (columns=None))]
    fn __new__(columns: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<Self> {
        Ok(Self::from_inner(DataSchema::new(parse_columns(columns)?)))
    }

    /// Fields in schema order.
    #[getter]
    fn columns(&self) -> Vec<PyFieldSpec> {
        self.inner
            .columns
            .iter()
            .cloned()
            .map(PyFieldSpec::from)
            .collect()
    }

    /// Alias of `columns`.
    #[getter]
    fn fields(&self) -> Vec<PyFieldSpec> {
        self.columns()
    }

    /// Return `true` when the schema has no fields.
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return whether a field named `name` exists.
    ///
    /// # Errors
    /// Returns `WYRD_DATA_400_VALIDATION` when `name` is not a valid column
    /// name.
    fn contains_column(&self, name: &str) -> WyrdPyResult<bool> {
        let name = column_name(name)?;
        Ok(self.inner.contains_column(&name))
    }

    /// Return the field named `name`, or `None` when it is absent.
    ///
    /// # Errors
    /// Returns `WYRD_DATA_400_VALIDATION` when `name` is not a valid column
    /// name.
    fn column(&self, name: &str) -> WyrdPyResult<Option<PyFieldSpec>> {
        let name = column_name(name)?;
        Ok(self.inner.column(&name).cloned().map(PyFieldSpec::from))
    }

    /// Return field names in schema order.
    fn column_names(&self) -> Vec<String> {
        self.inner
            .column_names()
            .map(|name| name.as_str().to_string())
            .collect()
    }

    /// Return this schema as a JSON-compatible spec dictionary.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON conversion fails.
    fn to_dict(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }
}

/// Register data schema wrapper classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDim>()?;
    module.add_class::<PyFieldSpec>()?;
    module.add_class::<PyDataSchema>()?;
    Ok(())
}

#[cfg(feature = "python")]
fn column_name(value: &str) -> WyrdPyResult<ColumnName> {
    ColumnName::new(value)
        .map_err(|error| {
            crate::error::validation_with_details(
                format!("invalid schema column name: {value}"),
                serde_json::json!({
                    "field": "name",
                    "value": value,
                    "source": error.to_string(),
                }),
            )
        })
        .map_err(Into::into)
}

#[cfg(feature = "python")]
fn parse_columns(value: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<Vec<FieldSpec>> {
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
fn parse_dims(value: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<Vec<Dim>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(Vec::new());
    };
    let items = value.try_iter()?.collect::<PyResult<Vec<_>>>()?;
    if !items.is_empty() && items.iter().all(PyAnyMethods::is_instance_of::<PyDim>) {
        return items
            .iter()
            .map(|item| Ok(item.cast::<PyDim>()?.get().inner.clone()))
            .collect();
    }
    dims_from_json(pyobject_to_json(value)?)
}

#[cfg(feature = "python")]
fn dims_from_json(value: Value) -> WyrdPyResult<Vec<Dim>> {
    match value {
        Value::Array(values) if values.iter().all(Value::is_i64) => values
            .into_iter()
            .map(|value| {
                value.as_i64().map(Dim::Fixed).ok_or_else(|| {
                    crate::error::validation("schema dimension values must be signed integers")
                })
            })
            .collect::<Result<Vec<Dim>, WyrdError>>()
            .map_err(Into::into),
        value => Ok(serde_json::from_value(value)?),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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
            extra: BTreeMap::default(),
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
            extra: BTreeMap::default(),
        };

        assert_eq!(PyFieldSpec::from(field.clone()).into_inner(), field);
    }
}
