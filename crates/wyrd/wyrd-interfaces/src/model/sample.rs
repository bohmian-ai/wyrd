//! Python/Rust wrapper for Wyrd model sample inputs.

use wyrd_spec::card::model::{
    SampleInput as SampleInputSpec, SampleInputKind as SampleInputKindSpec,
};

#[cfg(feature = "python")]
use {
    crate::error::CardPyResult,
    crate::model::interfaces::options::{parse_sample_input_kind, sample_input_kind_token},
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict, PyModule},
    std::path::PathBuf,
    wyrd_utils::py::json_to_pyobject,
};

/// Python-facing wrapper around `wyrd_spec::card::model::SampleInput`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "SampleInput")
)]
pub struct SampleInput {
    kind: SampleInputKindSpec,
    #[cfg(feature = "python")]
    py_obj: Option<Py<PyAny>>,
}

impl SampleInput {
    /// Build a sample input wrapper from a kind tag.
    #[must_use]
    pub const fn from_kind(kind: SampleInputKindSpec) -> Self {
        Self {
            kind,
            #[cfg(feature = "python")]
            py_obj: None,
        }
    }

    /// Build a wrapper from a Wyrd sample-input spec.
    #[must_use]
    pub const fn from_inner(inner: &SampleInputSpec) -> Self {
        Self::from_kind(inner.kind)
    }

    /// Return the durable sample input kind.
    #[must_use]
    pub const fn kind(&self) -> SampleInputKindSpec {
        self.kind
    }

    /// Convert to the Rust spec type.
    #[must_use]
    pub const fn to_rust(&self) -> SampleInputSpec {
        SampleInputSpec { kind: self.kind }
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl SampleInput {
    /// Create a sample input shell from an explicit kind token.
    #[new]
    #[pyo3(signature = (*, kind="none", value=None))]
    fn __new__(kind: &str, value: Option<Py<PyAny>>) -> CardPyResult<Self> {
        Ok(Self {
            kind: parse_sample_input_kind(kind)?,
            py_obj: value,
        })
    }

    /// Return the canonical sample kind token.
    #[getter]
    fn kind_token(&self) -> &'static str {
        sample_input_kind_token(self.kind)
    }

    /// Return whether this shell holds a live Python sample value.
    #[getter]
    fn has_value(&self) -> bool {
        self.py_obj.is_some()
    }

    /// Return the Rust metadata representation as a Python dictionary.
    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(self.to_rust())?,
        )?)
    }

    /// Sample save is deferred to a later `ModelCard` stage.
    #[pyo3(signature = (path, save_kwargs=None))]
    fn save(&self, path: PathBuf, save_kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<()> {
        let _ = (&self.kind, &self.py_obj, path, save_kwargs);
        crate::model::detect::autodetect_not_available()
    }

    /// Sample load is deferred to a later `ModelCard` stage.
    #[pyo3(signature = (path, load_kwargs=None))]
    fn load(&mut self, path: PathBuf, load_kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<()> {
        let _ = (&self.kind, &self.py_obj, path, load_kwargs);
        crate::model::detect::autodetect_not_available()
    }
}

/// Register model sample wrapper classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SampleInput>()?;
    Ok(())
}
