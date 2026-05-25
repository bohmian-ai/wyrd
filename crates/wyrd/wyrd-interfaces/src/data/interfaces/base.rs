use crate::error::{CardPyResult, WyrdPyError};

#[cfg(feature = "python")]
use {
    crate::data::stats::PyDataStats,
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict, PyTuple, PyType},
    std::path::PathBuf,
};

/// Base class for Python data interfaces.
///
/// Subclass this class to provide custom Python-only data materialization.
/// Custom subclasses must override `save` and `load`; the base implementations
/// raise validation errors so missing overrides fail loudly.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", subclass))]
pub struct DataInterface {
    kind: String,
}

#[cfg(feature = "python")]
#[pymethods]
impl DataInterface {
    /// Create a base data interface instance for Python subclasses.
    ///
    /// This initializer exists so `class MyInterface(DataInterface)` can call
    /// `super().__init__()` without satisfying a built-in interface
    /// constructor. Built-in interfaces bypass this initializer and set their
    /// own stable kind markers.
    ///
    /// # Arguments
    ///
    /// * `args` - Positional arguments accepted for Python subclass
    ///   compatibility and ignored by the base class.
    /// * `kwargs` - Keyword arguments accepted for Python subclass
    ///   compatibility and ignored by the base class.
    ///
    /// # Returns
    ///
    /// A base interface marker with kind `Custom`.
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn __new__(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        let _ = (args, kwargs);
        Self {
            kind: "Custom".to_string(),
        }
    }

    /// Initialize a Python data interface subclass.
    ///
    /// The base initializer is intentionally a no-op. It allows custom Python
    /// subclasses to call `super().__init__(...)` while keeping materialization
    /// behavior owned by the subclass implementation.
    ///
    /// # Arguments
    ///
    /// * `args` - Positional arguments supplied by a Python subclass.
    /// * `kwargs` - Keyword arguments supplied by a Python subclass.
    #[pyo3(signature = (*args, **kwargs))]
    fn __init__(&mut self, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) {
        let _ = (&self.kind, args, kwargs);
    }

    /// Return the stable interface kind used in `DataCard` metadata.
    ///
    /// # Returns
    ///
    /// The Wyrd data interface kind. Python subclasses report `Custom`.
    #[getter]
    fn kind(&self) -> &str {
        &self.kind
    }

    /// Build an interface instance from serialized `DataCard` metadata.
    ///
    /// Registry and client retrieval surfaces call this classmethod when a
    /// user passes a custom interface class such as
    /// `wyrd.cards.get(..., interface=MyInterface)`. The default implementation
    /// instantiates the concrete subclass with no arguments. Override this
    /// method when the subclass needs metadata values to reconstruct local
    /// configuration before ``DataCard`.load(...)` hydrates data.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Python-facing `DataCard` metadata object parsed from the
    ///   serialized card envelope.
    ///
    /// # Returns
    ///
    /// An initialized interface instance.
    ///
    /// # Errors
    ///
    /// Returns a Wyrd validation error when the concrete subclass cannot be
    /// constructed with no arguments. In that case, override
    /// `from_metadata(cls, metadata)`.
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
                WyrdPyError::validation(format!(
                    "custom DataInterface class could not be reconstructed from metadata with the default from_metadata implementation; override from_metadata(cls, metadata): {error}"
                ))
            })?
            .unbind())
    }

    /// Save custom data into a local `DataCard` artifact directory.
    ///
    /// Custom Python subclasses must override this method. The base
    /// implementation never writes files; it raises a validation error so a
    /// missing override fails before a ``DataCard`` can silently record an empty
    /// artifact.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory containing the local `DataCard` materialization. A
    ///   custom implementation should write all artifact bytes under this
    ///   directory using the layout documented by the subclass.
    /// * `save_kwargs` - Optional Python keyword arguments passed through from
    ///   ``DataCard`.save(...)` for custom implementation-specific behavior.
    ///
    /// # Returns
    ///
    /// A `DataStats` object describing the bytes written by the custom
    /// implementation.
    ///
    /// # Errors
    ///
    /// Always returns `WYRD_DATA_400_VALIDATION` from the base class because
    /// subclasses are required to implement their own save behavior.
    #[pyo3(signature = (path, save_kwargs=None))]
    fn save(
        &self,
        path: PathBuf,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<PyDataStats> {
        let _ = (&self.kind, path, save_kwargs);
        Err(WyrdPyError::validation(
            "DataInterface.save must be implemented by a concrete interface",
        ))
    }

    /// Load custom data from a local `DataCard` artifact directory.
    ///
    /// Custom Python subclasses must override this method. The base
    /// implementation never mutates the interface; it raises a validation error
    /// so missing reload behavior is explicit to both developers and agents.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory containing the local `DataCard` materialization.
    /// * `load_kwargs` - Optional Python keyword arguments passed through from
    ///   ``DataCard`.load(...)` for custom implementation-specific behavior.
    ///
    /// # Errors
    ///
    /// Always returns `WYRD_DATA_400_VALIDATION` from the base class because
    /// subclasses are required to implement their own load behavior.
    #[pyo3(signature = (path, load_kwargs=None))]
    fn load(&mut self, path: PathBuf, load_kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<()> {
        let _ = (&self.kind, path, load_kwargs);
        Err(WyrdPyError::validation(
            "DataInterface.load must be implemented by a concrete interface",
        ))
    }
}

impl DataInterface {
    pub(super) fn marker(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
        }
    }
}
