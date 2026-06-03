//! Minimal Python registration for Agent Card holder types.

#![cfg(feature = "python")]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::agent::{AgentBuilder, AgentWithMeta};

/// Python wrapper for a Wyrd Agent holder.
#[pyclass(module = "wyrd._native.cards.agent", name = "_AgentWithMetaInner")]
pub struct PyAgentWithMetaInner {
    inner: AgentWithMeta,
}

#[pymethods]
impl PyAgentWithMetaInner {
    /// Load an Agent holder from a YAML path.
    ///
    /// # Errors
    /// Returns a Python value error when YAML loading or resolution fails.
    #[staticmethod]
    pub fn from_yaml_path(path: &str) -> PyResult<Self> {
        AgentWithMeta::from_yaml_path(path)
            .map(|inner| Self { inner })
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Load an Agent holder from a YAML string.
    ///
    /// # Errors
    /// Returns a Python value error when YAML loading or resolution fails.
    #[staticmethod]
    pub fn from_yaml_str(input: &str) -> PyResult<Self> {
        AgentWithMeta::from_yaml_str(input)
            .map(|inner| Self { inner })
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Save this Agent holder to a YAML path.
    ///
    /// # Errors
    /// Returns a Python value error when save fails.
    pub fn save(&self, path: &str) -> PyResult<()> {
        self.inner
            .save(path)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Convert this Agent holder to a YAML string.
    ///
    /// # Errors
    /// Returns a Python value error when projection fails.
    pub fn to_yaml_string(&self) -> PyResult<String> {
        self.inner
            .to_yaml_string()
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Validate whether this Agent holder is registrable.
    ///
    /// # Errors
    /// Returns a Python value error when runtime-local tools are present.
    pub fn validate_registrable(&self) -> PyResult<()> {
        self.inner
            .validate_registrable()
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Optional card name.
    #[getter]
    pub fn name(&self) -> Option<String> {
        self.inner.name().map(str::to_owned)
    }

    /// Optional card version.
    #[getter]
    pub fn version(&self) -> Option<String> {
        self.inner.version().map(str::to_owned)
    }

    /// Optional card space.
    #[getter]
    pub fn space(&self) -> Option<String> {
        self.inner.space().map(str::to_owned)
    }

    /// Runtime-local tool names.
    #[getter]
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_names().to_vec()
    }
}

/// Python wrapper for a Wyrd Agent builder.
#[pyclass(module = "wyrd._native.cards.agent", name = "_AgentBuilderInner")]
#[derive(Default)]
pub struct PyAgentBuilderInner {
    inner: Option<AgentBuilder>,
}

#[pymethods]
impl PyAgentBuilderInner {
    /// Construct an empty Agent builder.
    #[new]
    pub fn __new__() -> Self {
        Self {
            inner: Some(AgentBuilder::default()),
        }
    }

    /// Set the card name.
    ///
    /// # Errors
    /// Returns a Python value error if the builder was already consumed.
    pub fn name(&mut self, name: &str) -> PyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.name(name));
        Ok(())
    }

    /// Set the card version.
    ///
    /// # Errors
    /// Returns a Python value error if the builder was already consumed.
    pub fn version(&mut self, version: &str) -> PyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.version(version));
        Ok(())
    }

    /// Set the card space.
    ///
    /// # Errors
    /// Returns a Python value error if the builder was already consumed.
    pub fn space(&mut self, space: &str) -> PyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.space(space));
        Ok(())
    }

    /// Build the Agent holder.
    ///
    /// # Errors
    /// Returns a Python value error when the Rust builder fails.
    pub fn build(&mut self) -> PyResult<PyAgentWithMetaInner> {
        let builder = self.take()?;
        builder
            .build()
            .map(|inner| PyAgentWithMetaInner { inner })
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }
}

impl PyAgentBuilderInner {
    fn take(&mut self) -> PyResult<AgentBuilder> {
        self.inner
            .take()
            .ok_or_else(|| PyValueError::new_err("AgentBuilder has already been consumed"))
    }
}

/// Register Agent Card Python classes.
///
/// # Errors
/// Returns `PyO3` module registration errors.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyAgentWithMetaInner>()?;
    module.add_class::<PyAgentBuilderInner>()?;
    Ok(())
}
