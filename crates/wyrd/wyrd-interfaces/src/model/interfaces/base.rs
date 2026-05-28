use crate::error::{CardPyResult, WyrdPyError};

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict, PyTuple, PyType},
    std::path::PathBuf,
};

/// Base class for Python model interfaces.
///
/// Subclass this class to provide custom Python-only model materialization.
/// Custom subclasses must override `save` and `load`; the base implementations
/// raise validation errors so missing overrides fail loudly.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.model", subclass))]
pub struct ModelInterface {
    kind: String,
}

#[cfg(feature = "python")]
#[pymethods]
impl ModelInterface {
    /// Create a base model interface instance for Python subclasses.
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn __new__(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        let _ = (args, kwargs);
        Self {
            kind: "Custom".to_string(),
        }
    }

    /// Initialize a Python model interface subclass.
    #[pyo3(signature = (*args, **kwargs))]
    fn __init__(&mut self, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) {
        let _ = (&self.kind, args, kwargs);
    }

    /// Return the stable interface kind used in `ModelCard` metadata.
    #[getter]
    fn kind(&self) -> &str {
        &self.kind
    }

    /// Build an interface instance from serialized `ModelCard` metadata.
    #[classmethod]
    #[pyo3(signature = (metadata))]
    fn from_metadata(
        cls: &Bound<'_, PyType>,
        metadata: &Bound<'_, PyAny>,
    ) -> CardPyResult<Py<PyAny>> {
        let _ = metadata;
        Ok(cls
            .call0()
            .map_err(|error| {
                WyrdPyError::model_validation(format!(
                    "custom ModelInterface class could not be reconstructed from metadata with the default from_metadata implementation; override from_metadata(cls, metadata): {error}"
                ))
            })?
            .unbind())
    }

    /// Save custom model bytes into a local `ModelCard` artifact directory.
    #[pyo3(signature = (path, save_kwargs=None))]
    fn save(&self, path: PathBuf, save_kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<()> {
        let _ = (&self.kind, path, save_kwargs);
        Err(WyrdPyError::model_validation(
            "ModelInterface.save must be implemented by a concrete interface",
        ))
    }

    /// Load custom model bytes from a local `ModelCard` artifact directory.
    #[pyo3(signature = (path, load_kwargs=None))]
    fn load(&mut self, path: PathBuf, load_kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<()> {
        let _ = (&self.kind, path, load_kwargs);
        Err(WyrdPyError::model_validation(
            "ModelInterface.load must be implemented by a concrete interface",
        ))
    }
}

impl ModelInterface {
    pub(super) fn marker(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
        }
    }
}
