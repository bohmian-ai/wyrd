//! `PyO3` boundary for the scoped observation surface.
//!
//! Python holds JSON documents, paths, and bytes, not Rust types, so this
//! boundary owns exactly one thing: converting them into the inputs the shared
//! `wyrd_client::observe` projection accepts, and projecting its results back.
//! No slot projection, hashing, upload, polling, or routing is re-implemented
//! here.

use std::fmt::Display;
use std::path::PathBuf;
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyMapping, PyModule, PyString, PyTuple, PyType};
use wyrd_cards::card_ref::CardRefPy;
use wyrd_client::observe::{
    ArtifactRef, File, Input, Inputs, Observe, Run, Submission, VerificationExecution,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::execution::VerificationResult;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, execution_route_from_py, json_to_pyobject};

/// One boundary validation failure naming the offending argument.
fn invalid_argument(field: &str, reason: impl Display) -> WyrdPyError {
    WyrdPyError::from(WyrdError::Validation {
        message: format!("{field} is invalid: {reason}"),
        details: serde_json::json!({ "field": field, "reason": reason.to_string() }),
    })
}

/// Serialize one Python observation payload as JSON text.
///
/// The three accepted shapes and their exact conversions are the documented
/// Python contract: a Pydantic model uses its own `model_dump_json()`, a
/// dataclass instance is reduced with `dataclasses.asdict`, and a mapping goes
/// straight to `json.dumps(..., allow_nan=False)` once its top-level keys are
/// proven to be strings. Reusing the model's own
/// serializer avoids a second round-trip through `model_dump()`, and
/// `allow_nan=False` is what refuses a non-finite float here rather than
/// emitting the `NaN` literal that no JSON reader accepts.
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` when the value is not one of those shapes
/// or carries something JSON cannot represent.
fn json_text(py: Python<'_>, field: &str, value: &Bound<'_, PyAny>) -> WyrdPyResult<String> {
    if let Ok(dump) = value.getattr("model_dump_json")
        && dump.is_callable()
    {
        return dump
            .call0()
            .and_then(|text| text.extract::<String>())
            .map_err(|error| invalid_argument(field, error));
    }
    let dataclasses = py
        .import("dataclasses")
        .map_err(|error| invalid_argument(field, error))?;
    let reduced = if dataclasses
        .call_method1("is_dataclass", (value,))
        .and_then(|flag| flag.extract::<bool>())
        .unwrap_or(false)
        && !value.is_instance_of::<PyType>()
    {
        dataclasses
            .call_method1("asdict", (value,))
            .map_err(|error| invalid_argument(field, error))?
    } else if value.cast::<PyMapping>().is_ok() {
        value.clone()
    } else {
        return Err(invalid_argument(
            field,
            "expected a mapping, dataclass instance, or Pydantic model",
        ));
    };
    require_string_keys(field, &reduced)?;
    let json = py
        .import("json")
        .map_err(|error| invalid_argument(field, error))?;
    let kwargs = PyDict::new(py);
    kwargs
        .set_item("allow_nan", false)
        .map_err(|error| invalid_argument(field, error))?;
    json.call_method("dumps", (reduced,), Some(&kwargs))
        .and_then(|text| text.extract::<String>())
        .map_err(|error| invalid_argument(field, error))
}

/// Refuse a mapping whose top-level keys are not all `str`.
///
/// `json.dumps` silently stringifies `int`, `float`, `bool`, and `None` keys,
/// so `{1: ...}` and `{"1": ...}` would reach Rust as the same feature. This
/// runs before the dump, so a coerced key never becomes a different identity.
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` naming the first non-`str` key.
fn require_string_keys(field: &str, mapping: &Bound<'_, PyAny>) -> WyrdPyResult<()> {
    let keys = mapping
        .cast::<PyMapping>()
        .map_err(|error| invalid_argument(field, error))?
        .keys()
        .map_err(|error| invalid_argument(field, error))?;
    for key in keys.iter() {
        if !key.is_instance_of::<PyString>() {
            let shown = key
                .repr()
                .map_or_else(|_| "<unrepresentable>".to_owned(), |repr| repr.to_string());
            return Err(invalid_argument(
                field,
                format!("mapping key {shown} is not a str"),
            ));
        }
    }
    Ok(())
}

/// Convert one Python slot value to JSON.
///
/// Mappings, dataclass instances, and Pydantic models convert as
/// [`json_text`] does; any other value goes through
/// `json.dumps(..., allow_nan=False)`, so a non-finite float or an object
/// JSON cannot represent is refused rather than stringified.
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` naming `field` for a value JSON cannot
/// represent.
fn json_value(
    py: Python<'_>,
    field: &str,
    value: &Bound<'_, PyAny>,
) -> WyrdPyResult<serde_json::Value> {
    let text = match json_text(py, field, value) {
        Ok(text) => text,
        Err(_) if value.cast::<PyMapping>().is_err() => py
            .import("json")
            .and_then(|json| {
                let kwargs = PyDict::new(py);
                kwargs.set_item("allow_nan", false)?;
                json.call_method("dumps", (value,), Some(&kwargs))
            })
            .and_then(|text| text.extract::<String>())
            .map_err(|error| invalid_argument(field, error))?,
        Err(error) => return Err(error),
    };
    serde_json::from_str(&text).map_err(|error| invalid_argument(field, error))
}

/// Convert one Python value to the shared client's slot input.
///
/// A [`PyFile`] or [`PyArtifactRef`] keeps its meaning; anything else is an
/// ordinary JSON value whose slot decides its form.
///
/// # Errors
/// As [`json_value`].
fn input(py: Python<'_>, field: &str, value: &Bound<'_, PyAny>) -> WyrdPyResult<Input> {
    if let Ok(file) = value.cast::<PyFile>() {
        return Ok(Input::File(file.get().inner.clone()));
    }
    if let Ok(artifact) = value.cast::<PyArtifactRef>() {
        return Ok(Input::Artifact(artifact.get().inner.clone()));
    }
    json_value(py, field, value).map(Input::Value)
}

/// Convert a call's positional value or keyword slots to [`Inputs`].
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` when both a positional value and keyword
/// slots are given, or more than one positional value, and as [`input`].
fn inputs(
    py: Python<'_>,
    value: &Bound<'_, PyTuple>,
    named: Option<&Bound<'_, PyDict>>,
) -> WyrdPyResult<Inputs> {
    let named = named.filter(|named| !named.is_empty());
    match (value.len(), named) {
        (0, named) => named
            .map(|named| {
                named
                    .iter()
                    .map(|(slot, value)| {
                        let slot = slot.extract::<String>()?;
                        let value = input(py, &slot, &value)?;
                        Ok((slot, value))
                    })
                    .collect::<WyrdPyResult<Vec<_>>>()
            })
            .transpose()
            .map(|named| Inputs::named(named.unwrap_or_default())),
        (1, None) => Ok(Inputs::One(input(py, "value", &value.get_item(0)?)?)),
        _ => Err(invalid_argument(
            "inputs",
            "pass one positional value for a single-slot Verifier or keyword slots, not both",
        )),
    }
}

/// Python wrapper for one invocation or one Card-scoped view of it.
#[pyclass(module = "wyrd._wyrd.observe", name = "Run", frozen)]
pub struct PyRun {
    /// The native view; cloning it is cloning the view, not the writer.
    inner: Run,
}

impl PyRun {
    /// Wrap one native view for Python.
    pub(crate) fn new(inner: Run) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyRun {
    /// The invocation identity this run and every view of it shares.
    #[getter]
    fn run_id(&self) -> String {
        self.inner.run_id().as_str().to_owned()
    }

    /// The alias this view was opened with; `state.card_ref(alias)` is its
    /// exact Card reference.
    #[getter]
    fn alias(&self) -> &str {
        self.inner.alias()
    }

    /// An immutable sibling view scoped to a registered alias.
    ///
    /// # Errors
    /// Raises `WYRD_SDK_404_UNKNOWN_ALIAS` for an alias this bundle does not
    /// register. No network IO occurs.
    fn for_card(&self, alias: &str) -> WyrdPyResult<Self> {
        Ok(Self::new(
            self.inner.for_card(alias).map_err(WyrdPyError::from)?,
        ))
    }

    /// Enter this view's ambient OpenTelemetry span correlation.
    ///
    /// Delegates to `wyrd.otel`, which pushes this view's exact Card UID and
    /// `run_id` onto the scope stack held in Python's execution-local
    /// OpenTelemetry context value, stamps an already-active recording span
    /// that does not yet carry `wyrd.card_uid`, and ensures the global provider
    /// has the Wyrd span processor. No scope state is stored on this immutable
    /// view, so one run may be entered by nested or concurrent scopes.
    /// Telemetry is optional: any failure, including a missing `opentelemetry`
    /// package, is swallowed and the run is returned. Entering never flushes,
    /// starts a span, or calls the server.
    fn __enter__(slf: Bound<'_, Self>) -> Bound<'_, Self> {
        let run = slf.get();
        let _ = slf.py().import("wyrd.otel").and_then(|otel| {
            otel.call_method1(
                "_enter_run",
                (
                    run.inner.subject_uid().as_str(),
                    run.inner.run_id().as_str(),
                ),
            )
        });
        slf
    }

    /// Restore the correlation that was ambient before the matching entry.
    ///
    /// Passes this view's subject Card UID and `run_id` to `wyrd.otel`, which pops the
    /// current execution context's innermost scope only when it equals that
    /// pair; a mismatched or failing exit changes nothing. Failures are
    /// swallowed, and the method always returns `False` so an exception raised
    /// inside the block propagates unchanged. Exiting is not a flush, shutdown,
    /// or durability acknowledgement.
    #[pyo3(signature = (exc_type=None, exc_value=None, traceback=None))]
    fn __exit__(
        slf: &Bound<'_, Self>,
        exc_type: Option<&Bound<'_, PyAny>>,
        exc_value: Option<&Bound<'_, PyAny>>,
        traceback: Option<&Bound<'_, PyAny>>,
    ) -> bool {
        // The exception triple is accepted only to mirror the protocol; it
        // never changes cleanup or suppresses the exception.
        let _ = (exc_type, exc_value, traceback);
        let run = slf.get();
        let _ = slf.py().import("wyrd.otel").and_then(|otel| {
            otel.call_method1(
                "_exit_run",
                (
                    run.inner.subject_uid().as_str(),
                    run.inner.run_id().as_str(),
                ),
            )
        });
        false
    }

    /// Invoke this view's tool-free Agent once with string `variables` and
    /// return its final text.
    ///
    /// `route`, when given, is an execution route dict that replaces the
    /// Agent's declared route whole for this invocation only; it is decoded
    /// into the Rust `ExecutionRoute` and passed to `Run::invoke_on`, which
    /// owns every routing decision. Omitted, the Agent's declared route is
    /// used. The GIL is released while the call runs.
    ///
    /// # Errors
    /// Raises `WYRD_SPEC_400_VALIDATION` for a `route` that is not an
    /// execution route, and otherwise the errors of `Run::invoke_on`.
    #[pyo3(signature = (variables=None, *, route=None))]
    fn invoke(
        &self,
        py: Python<'_>,
        variables: Option<std::collections::BTreeMap<String, String>>,
        route: Option<Bound<'_, PyDict>>,
    ) -> WyrdPyResult<String> {
        let route = route
            .map(|route| execution_route_from_py(&route, "route"))
            .transpose()?;
        let variables = variables.unwrap_or_default();
        let pairs: Vec<(&str, &str)> = variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let invoked = async {
            match &route {
                Some(route) => self.inner.invoke_on(route, &pairs).await,
                None => self.inner.invoke(&pairs).await,
            }
        };
        crate::trace::block_on(py, invoked).map_err(WyrdPyError::from)
    }

    /// The emit surface for this view.
    #[getter]
    fn observe(&self) -> PyObserveHandle {
        PyObserveHandle {
            run: Run::clone(&self.inner),
        }
    }
}

/// Python wrapper for the record, verify, and submit calls on one scoped run.
///
/// It owns its own view clone rather than borrowing the `Run`, because a Python
/// caller keeps `run.observe` alive independently of the object it came from.
#[pyclass(module = "wyrd._wyrd.observe", name = "Observe", frozen)]
pub struct PyObserveHandle {
    /// The view every emit correlates to.
    run: Run,
}

#[pymethods]
impl PyObserveHandle {
    /// Emit one row into a registered caller-owned table.
    ///
    /// Blocking: the first call for a table describes it. Later calls reuse the
    /// writer's cached schema and producer and perform no schema lookup.
    ///
    /// # Errors
    /// Raises `WYRD_SPEC_400_VALIDATION` for a row of another shape,
    /// `WYRD_SDK_400_INVALID_OBSERVATION` for a table outside
    /// `vala.datasets`, `WYRD_SDK_400_BIFROST_NOT_STARTED`,
    /// `WYRD_SDK_409_BIFROST_CLOSED`, `WYRD_CLIENT_429_QUEUE_FULL`, and the
    /// server's error for an unknown, unauthorized, or unavailable table.
    fn record(&self, py: Python<'_>, table: &str, row: &Bound<'_, PyAny>) -> WyrdPyResult<()> {
        let json = json_text(py, "row", row)?;
        crate::trace::block_on(py, self.observe().record_json(table, &json))
            .map_err(WyrdPyError::from)
    }

    /// Execute the bound Verifier `verifier` on this view's subject now.
    ///
    /// Pass one positional value for a single-slot Verifier or one keyword
    /// per slot. The shared client projects each value by its slot, hashes
    /// and uploads `File` values during the call, and returns the result;
    /// a `failed` verdict returns normally. Blocks with the GIL released.
    ///
    /// # Errors
    /// Raises `WYRD_SDK_404_UNKNOWN_VERIFIER`,
    /// `WYRD_VERIFICATION_400_INPUT_INVALID` or `413_INPUT_TOO_LARGE` before
    /// the execution request, `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` for
    /// a scheduled-window Verifier, and otherwise the server's refusal.
    #[pyo3(signature = (verifier, /, *value, **named))]
    fn verify(
        &self,
        py: Python<'_>,
        verifier: &str,
        value: &Bound<'_, PyTuple>,
        named: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<PyVerificationResult> {
        let inputs = inputs(py, value, named)?;
        let observe = self.observe();
        crate::trace::block_on(py, observe.verify(verifier, inputs))
            .map(PyVerificationResult)
            .map_err(WyrdPyError::from)
    }

    /// Submit evidence for the bound Verifier `verifier` without waiting.
    ///
    /// Run evidence is staged for one queued execution, returned as a
    /// `VerificationExecution`; scheduled-window samples are recorded and
    /// `Recorded` is returned. Arguments are as for `verify`.
    ///
    /// # Errors
    /// As [`PyObserveHandle::verify`] for Run evidence; for window samples,
    /// `WYRD_SDK_400_BIFROST_NOT_STARTED`, `WYRD_SDK_409_BIFROST_CLOSED`, or
    /// `WYRD_CLIENT_429_QUEUE_FULL`.
    #[pyo3(signature = (verifier, /, *value, **named))]
    fn submit(
        &self,
        py: Python<'_>,
        verifier: &str,
        value: &Bound<'_, PyTuple>,
        named: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let inputs = inputs(py, value, named)?;
        let observe = self.observe();
        match crate::trace::block_on(py, observe.submit(verifier, inputs))? {
            Submission::Recorded => Ok(Py::new(py, PyRecorded)?.into_any()),
            Submission::Queued(inner) => {
                Ok(Py::new(py, PyVerificationExecution(inner))?.into_any())
            }
        }
    }
}

impl PyObserveHandle {
    /// The native emit surface over this handle's view.
    fn observe(&self) -> Observe<'_> {
        self.run.observe()
    }
}

/// A local file to verify, read only when `verify` or `submit` uses it.
#[pyclass(module = "wyrd._wyrd.observe", name = "File", frozen)]
pub struct PyFile {
    /// The shared client's lazy file.
    inner: File,
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
#[pyclass(module = "wyrd._wyrd.observe", name = "ArtifactRef", frozen)]
pub struct PyArtifactRef {
    /// The shared client's Artifact selection.
    inner: ArtifactRef,
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
#[pyclass(module = "wyrd._wyrd.observe", name = "Recorded", frozen)]
pub struct PyRecorded;

/// One queued Verifier execution, addressed by its server-issued ID.
#[pyclass(module = "wyrd._wyrd.observe", name = "VerificationExecution", frozen)]
pub struct PyVerificationExecution(VerificationExecution);

#[pymethods]
impl PyVerificationExecution {
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
                Some(client) => VerificationExecution::connect_with(id, &client).await,
                None => VerificationExecution::connect(id).await,
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
#[pyclass(module = "wyrd._wyrd.observe", name = "VerificationResult", frozen)]
pub struct PyVerificationResult(VerificationResult);

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

/// Register the scoped observation classes on `wyrd._wyrd.observe`.
///
/// # Errors
/// Propagates the Python error from a failed `add_class` call, such as an
/// allocation failure or an incompatible existing class entry.
pub fn register_run(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyRun>()?;
    module.add_class::<PyObserveHandle>()?;
    module.add_class::<PyFile>()?;
    module.add_class::<PyArtifactRef>()?;
    module.add_class::<PyRecorded>()?;
    module.add_class::<PyVerificationExecution>()?;
    module.add_class::<PyVerificationResult>()?;
    Ok(())
}
