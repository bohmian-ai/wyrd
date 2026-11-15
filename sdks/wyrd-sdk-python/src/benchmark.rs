//! `wyrd._wyrd._benchmark`: the benchmark recorder and heap sampler of a
//! `bench` build of this module, for the L1 Verification benchmark's Python
//! driver only.
//!
//! The whole module, its global allocator, and the recorder it installs
//! exist only with the `bench` feature, which no published wheel enables:
//! a production wheel has no `_benchmark` submodule, no recorder, and the
//! system allocator. A `bench` build records the shared `wyrd_client`
//! timings and this module's own `sdk.verify` boundary into the one
//! `wyrd_benchmark` registry, and reports every native allocation of this
//! module to the shared heap sampler, which ignores them until started.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule};

/// This module's allocator: the system allocator reporting each native
/// allocation to [`wyrd_benchmark::heap`].
#[global_allocator]
static GLOBAL: wyrd_benchmark::heap::SampledSystem = wyrd_benchmark::heap::SampledSystem;

/// Installs the benchmark recorder as this process's `metrics` recorder.
///
/// # Errors
///
/// Raises `RuntimeError` when another `metrics` recorder is installed.
#[pyfunction]
fn install() -> PyResult<()> {
    wyrd_benchmark::install()
        .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
}

/// The cumulative benchmark snapshot of this process, as JSON.
///
/// # Errors
///
/// Raises `RuntimeError` when the snapshot cannot be serialized.
#[pyfunction]
fn snapshot() -> PyResult<String> {
    serde_json::to_string(&wyrd_benchmark::snapshot())
        .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
}

/// Starts sampling this module's native allocations.
#[pyfunction]
fn heap_start() {
    wyrd_benchmark::heap::start();
}

/// The sampled native allocations since [`heap_start`], allocated and still
/// live, as a gzipped pprof profile.
#[pyfunction]
fn heap_profile(py: Python<'_>) -> Bound<'_, PyBytes> {
    let profile = py.detach(wyrd_benchmark::heap::profile);
    PyBytes::new(py, &profile)
}

/// Registers the benchmark functions on `module`.
///
/// # Errors
///
/// Returns a Python error when a function cannot be added.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(install, module)?)?;
    module.add_function(wrap_pyfunction!(snapshot, module)?)?;
    module.add_function(wrap_pyfunction!(heap_start, module)?)?;
    module.add_function(wrap_pyfunction!(heap_profile, module)?)?;
    Ok(())
}
