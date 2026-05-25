//! Python/Rust wrapper for Wyrd model signatures.

use wyrd_spec::card::model::ModelSignature as ModelSignatureSpec;

#[cfg(feature = "python")]
use {
    crate::data::dtype::{dtype_string_from_py, fields_from_py, is_framework_class},
    crate::data::schema::PyFieldSpec,
    crate::error::{CardPyResult, WyrdPyError},
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString, PyType},
    std::collections::BTreeMap,
    wyrd_spec::card::field::{Dim, FieldSpec},
    wyrd_spec::ids::ColumnName,
    wyrd_utils::py::{json_to_pyobject, pyobject_to_json},
};

/// Python-facing wrapper around `wyrd_spec::card::model::ModelSignature`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "ModelSignature", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSignature {
    inner: ModelSignatureSpec,
}

impl ModelSignature {
    /// Build a wrapper from a Wyrd model signature.
    #[must_use]
    pub const fn from_inner(inner: ModelSignatureSpec) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped Wyrd model signature.
    #[must_use]
    pub const fn inner(&self) -> &ModelSignatureSpec {
        &self.inner
    }

    /// Unwrap the Wyrd model signature.
    #[must_use]
    pub fn into_inner(self) -> ModelSignatureSpec {
        self.inner
    }
}

impl From<ModelSignatureSpec> for ModelSignature {
    fn from(inner: ModelSignatureSpec) -> Self {
        Self::from_inner(inner)
    }
}

impl From<ModelSignature> for ModelSignatureSpec {
    fn from(value: ModelSignature) -> Self {
        value.into_inner()
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl ModelSignature {
    /// Create a model signature from explicit field specs.
    #[new]
    #[pyo3(signature = (inputs, outputs))]
    fn __new__(inputs: &Bound<'_, PyAny>, outputs: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        Ok(Self::from_inner(ModelSignatureSpec::from_fields(
            parse_fields(inputs)?,
            parse_fields(outputs)?,
        )?))
    }

    /// Return input fields in declaration order.
    #[getter]
    fn inputs(&self) -> Vec<PyFieldSpec> {
        self.inner
            .inputs
            .iter()
            .cloned()
            .map(PyFieldSpec::from)
            .collect()
    }

    /// Return output fields in declaration order.
    #[getter]
    fn outputs(&self) -> Vec<PyFieldSpec> {
        self.inner
            .outputs
            .iter()
            .cloned()
            .map(PyFieldSpec::from)
            .collect()
    }

    /// Return the Rust metadata representation as a Python dictionary.
    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }

    /// Infer a model signature from live sample input and output objects.
    #[classmethod]
    #[pyo3(signature = (*, inputs, outputs))]
    fn from_data(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        inputs: &Bound<'_, PyAny>,
        outputs: &Bound<'_, PyAny>,
    ) -> CardPyResult<Self> {
        let signature =
            ModelSignatureSpec::from_fields(infer_fields(py, inputs)?, infer_fields(py, outputs)?)?;
        Ok(Self::from_inner(signature))
    }
}

/// Register model signature wrapper classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<ModelSignature>()?;
    Ok(())
}

#[cfg(feature = "python")]
fn parse_fields(value: &Bound<'_, PyAny>) -> CardPyResult<Vec<FieldSpec>> {
    let mut fields = Vec::new();
    for item in value.try_iter()? {
        let item = item?;
        if let Ok(field) = item.extract::<PyRef<'_, PyFieldSpec>>() {
            fields.push(field.inner().clone());
        } else {
            fields.push(serde_json::from_value(pyobject_to_json(&item)?)?);
        }
    }
    Ok(fields)
}

#[cfg(feature = "python")]
fn infer_fields(py: Python<'_>, obj: &Bound<'_, PyAny>) -> CardPyResult<Vec<FieldSpec>> {
    if is_framework_class(py, obj, "pandas", "DataFrame")?
        || is_framework_class(py, obj, "polars", "DataFrame")?
        || is_framework_class(py, obj, "pyarrow", "Table")?
    {
        return fields_from_py(py, obj);
    }
    if is_framework_class(py, obj, "numpy", "ndarray")?
        || is_framework_class(py, obj, "torch", "Tensor")?
    {
        return Ok(mark_batch_axis(fields_from_py(py, obj)?));
    }
    if obj.is_instance_of::<PyDict>() {
        return fields_from_dict_of_ndarray(py, obj);
    }
    if obj.is_instance_of::<PyList>() {
        let list = obj.cast::<PyList>()?;
        if list_is_all_str(list) {
            return text_field(true);
        }
    }
    if obj.is_instance_of::<PyString>() {
        return text_field(false);
    }
    Err(WyrdPyError::missing_signature(
        "ModelSignature.from_data requires pandas, polars, pyarrow, numpy, torch, dict[str, numpy.ndarray], list[str], or str values",
    ))
}

#[cfg(feature = "python")]
fn fields_from_dict_of_ndarray(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
) -> CardPyResult<Vec<FieldSpec>> {
    let dict = obj.cast::<PyDict>()?;
    let mut fields = Vec::with_capacity(dict.len());
    for (key, value) in dict.iter() {
        let name: String = key
            .extract()
            .map_err(|_| WyrdPyError::missing_signature("dict keys must be strings"))?;
        if !is_framework_class(py, &value, "numpy", "ndarray")? {
            return Err(WyrdPyError::missing_signature(format!(
                "dict value for key {name:?} is not numpy.ndarray"
            )));
        }
        let dtype = dtype_string_from_py(py, "numpy", &value)?;
        let shape = value.getattr("shape")?.extract::<Vec<i64>>()?;
        fields.push(FieldSpec {
            name: ColumnName::new(&name).map_err(|source| {
                WyrdPyError::missing_signature(format!(
                    "dict key {name:?} is not a valid field name: {source}"
                ))
            })?,
            dtype,
            shape: batch_first_dims(shape),
            nullable: false,
            extra: BTreeMap::new(),
        });
    }
    Ok(fields)
}

#[cfg(feature = "python")]
fn list_is_all_str(list: &Bound<'_, PyList>) -> bool {
    !list.is_empty() && list.iter().all(|item| item.is_instance_of::<PyString>())
}

#[cfg(feature = "python")]
fn text_field(dynamic_batch: bool) -> CardPyResult<Vec<FieldSpec>> {
    Ok(vec![FieldSpec {
        name: ColumnName::new("text")
            .map_err(|source| WyrdPyError::missing_signature(source.to_string()))?,
        dtype: "utf8".to_string(),
        shape: if dynamic_batch {
            vec![Dim::Dynamic(Some("batch".to_string()))]
        } else {
            Vec::new()
        },
        nullable: false,
        extra: BTreeMap::new(),
    }])
}

#[cfg(feature = "python")]
fn mark_batch_axis(fields: Vec<FieldSpec>) -> Vec<FieldSpec> {
    fields
        .into_iter()
        .map(|mut field| {
            field.shape = batch_first_dims(
                field
                    .shape
                    .into_iter()
                    .filter_map(|dim| match dim {
                        Dim::Fixed(value) => Some(value),
                        Dim::Dynamic(_) => None,
                    })
                    .collect(),
            );
            field
        })
        .collect()
}

#[cfg(feature = "python")]
fn batch_first_dims(raw: Vec<i64>) -> Vec<Dim> {
    raw.into_iter()
        .enumerate()
        .map(|(index, value)| {
            if index == 0 {
                Dim::Dynamic(Some("batch".to_string()))
            } else {
                Dim::Fixed(value)
            }
        })
        .collect()
}

#[cfg(all(test, feature = "python"))]
mod tests {
    use wyrd_spec::card::field::Dim;

    #[test]
    fn batch_first_dims_marks_axis_zero_dynamic() {
        let dims = super::batch_first_dims(vec![4, 8, 16]);
        assert_eq!(
            dims,
            vec![
                Dim::Dynamic(Some("batch".to_string())),
                Dim::Fixed(8),
                Dim::Fixed(16)
            ]
        );
    }

    #[test]
    fn batch_first_dims_keeps_scalar_shape_empty() {
        assert!(super::batch_first_dims(Vec::new()).is_empty());
    }
}
