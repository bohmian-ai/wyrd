//! Python/Rust wrappers for Wyrd data statistics value objects.

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyAny, PyModule};
#[cfg(feature = "python")]
use wyrd_utils::py::json_to_pyobject;

#[cfg(feature = "python")]
use crate::error::CardPyResult;
use wyrd_spec::card::data::DataStats;

/// Python-facing wrapper for Wyrd `DataStats`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.data", name = "DataStats", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyDataStats {
    inner: DataStats,
}

impl PyDataStats {
    /// Build a wrapper from Wyrd data stats.
    #[must_use]
    pub const fn from_inner(inner: DataStats) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped Wyrd data stats.
    #[must_use]
    pub const fn inner(&self) -> &DataStats {
        &self.inner
    }

    /// Unwrap the Wyrd data stats.
    #[must_use]
    pub fn into_inner(self) -> DataStats {
        self.inner
    }
}

impl From<DataStats> for PyDataStats {
    fn from(inner: DataStats) -> Self {
        Self::from_inner(inner)
    }
}

impl From<PyDataStats> for DataStats {
    fn from(value: PyDataStats) -> Self {
        value.into_inner()
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PyDataStats {
    #[new]
    #[pyo3(signature = (byte_count, sha256, row_count=None, col_count=None))]
    fn __new__(
        byte_count: u64,
        sha256: String,
        row_count: Option<u64>,
        col_count: Option<u32>,
    ) -> Self {
        Self::from_inner(DataStats {
            row_count,
            col_count,
            byte_count,
            sha256,
        })
    }

    #[getter]
    fn row_count(&self) -> Option<u64> {
        self.inner.row_count
    }

    #[getter]
    fn col_count(&self) -> Option<u32> {
        self.inner.col_count
    }

    #[getter]
    fn byte_count(&self) -> u64 {
        self.inner.byte_count
    }

    #[getter]
    fn sha256(&self) -> String {
        self.inner.sha256.clone()
    }

    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(&self.inner)?)?)
    }
}

/// Register the data stats wrapper class on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDataStats>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PyDataStats;
    use wyrd_spec::card::data::DataStats;

    #[test]
    fn data_stats_round_trips_inner() {
        let stats = DataStats {
            row_count: Some(10),
            col_count: Some(2),
            byte_count: 128,
            sha256: "a".repeat(64),
        };

        assert_eq!(PyDataStats::from(stats.clone()).into_inner(), stats);
    }
}
