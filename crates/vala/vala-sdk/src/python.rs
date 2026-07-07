//! PyO3 boundary for the Vala client SDK: the `bifrost` write handle and the
//! `observe` fire-and-forget telemetry function.
//!
//! Every method converts its Python inputs to the native C4b types at the edge
//! (JSON-Schema text to an Arrow schema, a card-ref string to [`CardRef`]) and
//! then calls the Rust-native handle — no queue or engine logic is
//! re-implemented here. [`crate::scope::ProducerKey`]-equivalent pool keys and
//! [`ClientScope`] stay opaque: Python never names them.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Python-facing Bifrost write handle over the pooled C4b producers.
///
/// The networked ingest transport is not yet wired — constructing this class
/// raises `RuntimeError` until the gRPC ingest path lands. Callers should catch
/// the error and disable any code paths that require live Bifrost writes.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "Bifrost")]
pub struct Bifrost {
    _private: (),
}

#[pymethods]
impl Bifrost {
    /// Not yet available — raises `RuntimeError` until gRPC ingest is wired.
    #[new]
    #[pyo3(signature = (_server_url, _api_key))]
    fn new(_server_url: String, _api_key: String) -> PyResult<Self> {
        Err(PyRuntimeError::new_err(
            "bifrost transport unavailable: gRPC ingest not wired",
        ))
    }

    /// Explicit write path: enqueue one JSON `row`, propagating queue-full.
    #[pyo3(signature = (table, schema, row, card_ref, run_id=None))]
    #[allow(unused_variables)]
    fn insert(
        &self,
        table: &str,
        schema: &str,
        row: &str,
        card_ref: &str,
        run_id: Option<String>,
    ) -> PyResult<()> {
        Err(PyRuntimeError::new_err(
            "bifrost transport unavailable: gRPC ingest not wired",
        ))
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[getter]
    fn dropped(&self) -> u64 {
        0
    }

    /// Number of distinct producers currently pooled.
    #[getter]
    fn producer_count(&self) -> usize {
        0
    }
}

/// Record one telemetry observation, fire-and-forget.
///
/// Not yet available — raises `RuntimeError` until gRPC ingest is wired.
#[pyfunction]
#[pyo3(signature = (bifrost, table, schema, row, card_ref, run_id=None))]
#[allow(unused_variables)]
fn record(
    bifrost: PyRef<'_, Bifrost>,
    table: &str,
    schema: &str,
    row: &str,
    card_ref: &str,
    run_id: Option<String>,
) -> PyResult<()> {
    Err(PyRuntimeError::new_err(
        "bifrost transport unavailable: gRPC ingest not wired",
    ))
}

/// Register the `bifrost` write handle on the supplied module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_bifrost(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Bifrost>()?;
    Ok(())
}

/// Register the `observe` fire-and-forget telemetry function on the module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_observe(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(record, module)?)?;
    Ok(())
}
