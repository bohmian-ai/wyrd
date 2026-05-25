//! Python/Rust wrapper for Wyrd model signatures.

use wyrd_spec::card::model::ModelSignature as ModelSignatureSpec;

#[cfg(feature = "python")]
use {
    crate::data::schema::PyFieldSpec,
    crate::error::CardPyResult,
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyModule},
    wyrd_spec::card::field::FieldSpec,
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
