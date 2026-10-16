//! `PyO3` boundary for `wyrd.verify`: the evidence values `Observe.verify`
//! and `Observe.submit` accept, and the queued execution and result they
//! return.
//!
//! Each class wraps the shared `wyrd_client::verify` type unchanged; file
//! hashing, upload, polling, and backoff stay in the shared client.

use std::path::PathBuf;
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule};
use wyrd_cards::card_ref::CardRefPy;
use wyrd_client::verify::{ArtifactRef, Execution, File};
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::execution::VerificationResult;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, json_to_pyobject};

use crate::observe::invalid_argument;

/// A local file to verify, read only when `verify` or `submit` uses it.
#[pyclass(module = "wyrd._wyrd.verify", name = "File", frozen)]
pub struct PyFile {
    /// The shared client's lazy file.
    pub(crate) inner: File,
}

#[pymethods]
impl PyFile {
    /// A file at a path or in-memory bytes, performing no IO.
    ///
    /// # Arguments
    /// * `source` - A `str` or `os.PathLike` path, or `bytes`.
    /// * `format` - The canonical format, such as `"pdf"`; inferred from the
    ///   filename extension or the slot's single format when omitted.
    /// * `filename` - Display name; defaults to the path's file name.
    ///
    /// # Errors
    /// Raises `WYRD_SPEC_400_VALIDATION` for a source of another type or an
    /// unknown format.
    #[new]
    #[pyo3(signature = (source, *, format=None, filename=None))]
    fn __new__(
        source: &Bound<'_, PyAny>,
        format: Option<&str>,
        filename: Option<String>,
    ) -> WyrdPyResult<Self> {
        let mut inner = if let Ok(bytes) = source.cast::<PyBytes>() {
            File::bytes(bytes.as_bytes().to_vec())
        } else {
            File::path(source.extract::<PathBuf>().map_err(|_| {
                invalid_argument("source", "expected a str or os.PathLike path, or bytes")
            })?)
        };
        if let Some(format) = format {
            inner =
                inner.with_format(format.parse().map_err(|_| {
                    invalid_argument("format", format!("unknown format {format:?}"))
                })?);
        }
        if let Some(filename) = filename {
            inner = inner.with_filename(filename);
        }
        Ok(Self { inner })
    }
}

/// A registered Artifact's file, verified with its normal authorization.
#[pyclass(module = "wyrd._wyrd.verify", name = "ArtifactRef", frozen)]
pub struct PyArtifactRef {
    /// The shared client's Artifact selection.
    pub(crate) inner: ArtifactRef,
}

#[pymethods]
impl PyArtifactRef {
    /// Select `artifact`'s only file, or `path` within it.
    ///
    /// # Arguments
    /// * `artifact` - The exact Artifact Card reference.
    /// * `path` - The file inside the Artifact, when it holds several.
    #[new]
    #[pyo3(signature = (artifact, path=None))]
    fn __new__(artifact: CardRefPy, path: Option<String>) -> Self {
        Self {
            inner: ArtifactRef::new(artifact.0, path),
        }
    }
}

/// `submit`'s answer for scheduled-window samples: recorded, judged later by
/// the Verifier's schedule, with no execution to wait for.
#[pyclass(module = "wyrd._wyrd.verify", name = "Recorded", frozen)]
pub struct PyRecorded;

/// One queued Verifier execution, addressed by its server-issued ID.
#[pyclass(module = "wyrd._wyrd.verify", name = "Execution", frozen)]
pub struct PyExecution(pub(crate) Execution);

#[pymethods]
impl PyExecution {
    /// Rebuild the handle of `id` as `client`, or the ambient client.
    ///
    /// # Errors
    /// Raises `WYRD_SPEC_400_VALIDATION` for an `id` that is not an
    /// execution ID, and the client's configuration or credential error.
    #[staticmethod]
    #[pyo3(signature = (id, client=None))]
    fn connect(
        py: Python<'_>,
        id: &str,
        client: Option<PyRef<'_, crate::client::PyWyrdClient>>,
    ) -> WyrdPyResult<Self> {
        let id = id
            .parse::<ExecutionId>()
            .map_err(|error| invalid_argument("id", error))?;
        let client = crate::client::PyWyrdClient::native(client.as_deref());
        let connected = crate::trace::block_on(py, async move {
            match client {
                Some(client) => Execution::connect_with(id, &client).await,
                None => Execution::connect(id).await,
            }
        })?;
        Ok(Self(connected))
    }

    /// The execution ID.
    #[getter]
    fn id(&self) -> String {
        self.0.id().to_string()
    }

    /// The execution's current status, terminal error, and visible result as
    /// their wire mapping.
    ///
    /// # Errors
    /// Raises the server's not-found error before the execution is visible,
    /// and its authorization or transport error.
    fn status(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let state = crate::trace::block_on(py, self.0.status())?;
        Ok(json_to_pyobject(py, &serde_json::to_value(state)?)?)
    }

    /// Poll until a result, the execution's own error, or `timeout` seconds.
    ///
    /// # Errors
    /// Raises the execution's terminal error, including a delayed
    /// idempotency conflict, `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` when
    /// `timeout` passes first, `WYRD_SPEC_400_VALIDATION` for a negative or
    /// non-finite `timeout`, and any other status-read error.
    fn wait(&self, py: Python<'_>, timeout: f64) -> WyrdPyResult<PyVerificationResult> {
        let timeout = Duration::try_from_secs_f64(timeout)
            .map_err(|error| invalid_argument("timeout", error))?;
        crate::trace::block_on(py, self.0.wait(timeout))
            .map(PyVerificationResult)
            .map_err(WyrdPyError::from)
    }
}

/// Read-only Python view of one attributable Verifier result.
#[pyclass(module = "wyrd._wyrd.verify", name = "VerificationResult", frozen)]
pub struct PyVerificationResult(pub(crate) VerificationResult);

#[pymethods]
impl PyVerificationResult {
    /// Server-issued execution ID.
    #[getter]
    fn execution_id(&self) -> String {
        self.0.execution_id.to_string()
    }

    /// The caller's application Run, when there is one.
    #[getter]
    fn run_id(&self) -> Option<String> {
        self.0.run_id.as_ref().map(|run| run.as_str().to_owned())
    }

    /// The exact Verifier executed.
    #[getter]
    fn verifier(&self) -> CardRefPy {
        CardRefPy(self.0.verifier.clone())
    }

    /// The exact subject judged.
    #[getter]
    fn subject(&self) -> CardRefPy {
        CardRefPy(self.0.subject.clone())
    }

    /// The overall verdict: `passed`, `failed`, or `inconclusive`.
    ///
    /// # Errors
    /// Raises an internal error only if the wire enum stops serializing as a
    /// string.
    #[getter]
    fn verdict(&self) -> WyrdPyResult<String> {
        match serde_json::to_value(self.0.outcome.verdict)? {
            serde_json::Value::String(text) => Ok(text),
            other => Err(invalid_argument(
                "verdict",
                format!("unexpected wire value {other}"),
            )),
        }
    }

    /// Every requirement's judgment as its wire mapping, in plan order.
    ///
    /// # Errors
    /// Raises an internal error when the breakdown cannot be converted.
    #[getter]
    fn requirements(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(&self.0.outcome.requirements)?,
        )?)
    }

    /// Every step's result as its wire mapping, in plan order.
    ///
    /// # Errors
    /// As [`PyVerificationResult::requirements`].
    #[getter]
    fn steps(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(&self.0.outcome.steps)?,
        )?)
    }

    /// The pinned original evidence by slot, as its wire mapping.
    ///
    /// # Errors
    /// As [`PyVerificationResult::requirements`].
    #[getter]
    fn evidence(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(&self.0.outcome.evidence)?,
        )?)
    }
}

/// Register the verification classes on `wyrd._wyrd.verify`.
///
/// # Errors
/// Propagates the Python error from a failed `add_class` call, such as an
/// allocation failure or an incompatible existing class entry.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyFile>()?;
    module.add_class::<PyArtifactRef>()?;
    module.add_class::<PyRecorded>()?;
    module.add_class::<PyExecution>()?;
    module.add_class::<PyVerificationResult>()?;
    Ok(())
}
