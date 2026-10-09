//! `PyO3` boundary for the scoped observation surface.
//!
//! Python holds a JSON document, not a Rust type, so this boundary owns exactly
//! one thing: turning a mapping, dataclass instance, or Pydantic model into the
//! JSON text the shared `wyrd_client::observe` projection already accepts. No
//! validation, projection, record construction, or routing is re-implemented
//! here.

use std::fmt::Display;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyMapping, PyModule, PyString, PyType};
use wyrd_cards::card_ref::CardRefPy;
use wyrd_client::observe::eval::parse_session_id;
use wyrd_client::observe::{EvalObservationOptions, Observe, Run};
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::Judgment;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, json_to_pyobject};

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

/// Serialize a list of Python media descriptors as one JSON array.
///
/// # Errors
/// As [`json_text`], plus a validation error when `value` is not a list.
fn json_array_text(py: Python<'_>, field: &str, value: &Bound<'_, PyAny>) -> WyrdPyResult<String> {
    let items = value
        .cast::<PyList>()
        .map_err(|_| invalid_argument(field, "expected a list of media descriptors"))?;
    let encoded = items
        .iter()
        .map(|item| json_text(py, field, &item))
        .collect::<WyrdPyResult<Vec<_>>>()?;
    Ok(format!("[{}]", encoded.join(",")))
}

/// Read trace and span identity from Python's active OpenTelemetry span.
///
/// Python's OpenTelemetry context never becomes Rust's current `tracing` span across
/// `PyO3`, so the shared Rust fallback cannot see it; this boundary reads it
/// instead and hands the IDs to the ordinary explicit-options path. Both IDs
/// or neither: a missing `opentelemetry` package (the optional `otel` extra),
/// no active span, an invalid context, or any lookup failure supplies nothing,
/// because correlation is best-effort and must never fail an emit.
fn active_span_ids(py: Python<'_>) -> (Option<String>, Option<String>) {
    let lookup = || -> PyResult<Option<(u128, u64)>> {
        let context = py
            .import("opentelemetry.trace")?
            .call_method0("get_current_span")?
            .call_method0("get_span_context")?;
        if !context.getattr("is_valid")?.extract::<bool>()? {
            return Ok(None);
        }
        Ok(Some((
            context.getattr("trace_id")?.extract()?,
            context.getattr("span_id")?.extract()?,
        )))
    };
    match lookup() {
        Ok(Some((trace_id, span_id))) => (
            Some(format!("{trace_id:032x}")),
            Some(format!("{span_id:016x}")),
        ),
        _ => (None, None),
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
    /// Delegates to `wyrd.otel`, which pushes this view's exact `CardRef` and
    /// `run_id` onto the scope stack held in Python's execution-local
    /// OpenTelemetry context value, stamps an already-active recording span
    /// that does not yet carry `wyrd.card_ref`, and ensures the global provider
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
                (run.inner.subject().to_string(), run.inner.run_id().as_str()),
            )
        });
        slf
    }

    /// Restore the correlation that was ambient before the matching entry.
    ///
    /// Passes this view's `CardRef` and `run_id` to `wyrd.otel`, which pops the
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
                (run.inner.subject().to_string(), run.inner.run_id().as_str()),
            )
        });
        false
    }

    /// Invoke this view's tool-free Agent once through the Wyrd gateway with
    /// string `variables` and return its final text.
    ///
    /// The call carries this run and the Agent Card as gateway correlation;
    /// the GIL is released while it runs.
    ///
    /// # Errors
    /// Raises `WYRD_SDK_400_CARD_KIND_MISMATCH` when this view is not an
    /// Agent and `WYRD_AGENT_422_VALIDATION` for an Agent with tools or a
    /// non-gateway Prompt model, both before any IO; otherwise the gateway's
    /// own refusal, such as `WYRD_PERMISSION_403_DENIED_RBAC`.
    #[pyo3(signature = (variables=None))]
    fn invoke(
        &self,
        py: Python<'_>,
        variables: Option<std::collections::BTreeMap<String, String>>,
    ) -> WyrdPyResult<String> {
        let variables = variables.unwrap_or_default();
        let pairs: Vec<(&str, &str)> = variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        py.detach(|| wyrd_runtime::runtime().block_on(self.inner.invoke(&pairs)))
            .map_err(WyrdPyError::from)
    }

    /// The emit surface for this view.
    #[getter]
    fn observe(&self) -> PyObserveHandle {
        PyObserveHandle {
            run: Run::clone(&self.inner),
        }
    }
}

/// Python wrapper for the three emits available on one scoped run.
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
    /// Emit one Drift observation.
    ///
    /// Synchronous: the fixed table was described at `start_bifrost`, so this
    /// only projects and enqueues. Returning is not a durability
    /// acknowledgement; `WyrdState.shutdown()` is the barrier.
    ///
    /// # Errors
    /// Raises `WYRD_SDK_400_BIFROST_NOT_STARTED`,
    /// `WYRD_SDK_409_BIFROST_CLOSED`, `WYRD_SDK_400_INVALID_OBSERVATION` for an
    /// input that is not a flat object of supported scalars, or
    /// `WYRD_CLIENT_429_QUEUE_FULL`.
    #[pyo3(signature = (features, *, session_id=None))]
    fn drift(
        &self,
        py: Python<'_>,
        features: &Bound<'_, PyAny>,
        session_id: Option<&str>,
    ) -> WyrdPyResult<()> {
        let json = json_text(py, "features", features)?;
        let session = parse_session_id(session_id).map_err(WyrdPyError::from)?;
        self.observe()
            .drift_json(&json, session)
            .map_err(WyrdPyError::from)
    }

    /// Emit one Eval observation.
    ///
    /// Explicit `trace_id`/`span_id` win. With neither, Python's active
    /// OpenTelemetry span supplies both when it is valid, and otherwise both
    /// stay absent.
    ///
    /// # Errors
    /// As [`PyObserveHandle::drift`], plus a validation error for a malformed
    /// media descriptor or a `span_id` supplied without `trace_id`.
    #[pyo3(signature = (context, *, session_id=None, media=None, trace_id=None, span_id=None))]
    fn eval(
        &self,
        py: Python<'_>,
        context: &Bound<'_, PyAny>,
        session_id: Option<&str>,
        media: Option<&Bound<'_, PyAny>>,
        trace_id: Option<&str>,
        span_id: Option<&str>,
    ) -> WyrdPyResult<()> {
        let json = json_text(py, "context", context)?;
        let media = media
            .map(|media| json_array_text(py, "media", media))
            .transpose()?;
        let (active_trace, active_span) = if trace_id.is_none() && span_id.is_none() {
            active_span_ids(py)
        } else {
            (None, None)
        };
        let options = EvalObservationOptions::from_parts(
            session_id,
            media.as_deref(),
            trace_id.or(active_trace.as_deref()),
            span_id.or(active_span.as_deref()),
        )
        .map_err(WyrdPyError::from)?;
        self.observe()
            .eval_json(&json, options)
            .map_err(WyrdPyError::from)
    }

    /// Emit one row into a registered caller-owned table.
    ///
    /// Blocking: the first call for a table describes it. Later calls reuse the
    /// writer's cached schema and producer and perform no schema lookup.
    ///
    /// # Errors
    /// As [`PyObserveHandle::drift`], plus the server's error for an unknown,
    /// unauthorized, or unavailable table, and a validation error for a table
    /// outside `vala.datasets`.
    fn record(&self, py: Python<'_>, table: &str, row: &Bound<'_, PyAny>) -> WyrdPyResult<()> {
        let json = json_text(py, "row", row)?;
        py.detach(|| wyrd_runtime::runtime().block_on(self.observe().record_json(table, &json)))
            .map_err(WyrdPyError::from)
    }

    /// Judge this view's subject with a bound Verifier and return its judgment.
    ///
    /// An Eval Verifier takes one context mapping, dataclass instance, or
    /// Pydantic model plus optional media; a Drift Verifier takes a list of flat
    /// feature rows. Blocks with the GIL released for the one server call; a
    /// `failed` verdict returns normally. Nothing is observed, recorded,
    /// enqueued, or dispatched, and Bifrost need not be started.
    ///
    /// # Errors
    /// Raises `WYRD_SDK_404_UNKNOWN_VERIFIER` for a Verifier not bound to this
    /// view's subject and `WYRD_SDK_400_INVALID_OBSERVATION` for input of the
    /// wrong shape, both before any network IO, and otherwise the server's
    /// verification refusal.
    #[pyo3(signature = (verifier, input, *, media=None))]
    fn verify(
        &self,
        py: Python<'_>,
        verifier: &str,
        input: &Bound<'_, PyAny>,
        media: Option<&Bound<'_, PyAny>>,
    ) -> WyrdPyResult<PyJudgment> {
        let json = if input.cast::<PyList>().is_ok() {
            json_array_text(py, "input", input)?
        } else {
            json_text(py, "input", input)?
        };
        let media = media
            .map(|media| json_array_text(py, "media", media))
            .transpose()?
            .map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(|error| invalid_argument("media", error))?
            .unwrap_or_default();
        let observe = self.observe();
        py.detach(|| wyrd_runtime::runtime().block_on(observe.verify_json(verifier, &json, media)))
            .map(|inner| PyJudgment { inner })
            .map_err(WyrdPyError::from)
    }
}

impl PyObserveHandle {
    /// The native emit surface over this handle's view.
    fn observe(&self) -> Observe<'_> {
        self.run.observe()
    }
}

/// Python view of one direct verification judgment.
///
/// Read-only projection of the wire `Judgment`; `passed` is derived from the
/// verdict, not carried on the wire.
#[pyclass(module = "wyrd._wyrd.observe", name = "Judgment", frozen)]
pub struct PyJudgment {
    /// The judgment the server returned.
    inner: Judgment,
}

#[pymethods]
impl PyJudgment {
    /// Whether the expectations held: true only for a `passed` verdict.
    #[getter]
    fn passed(&self) -> bool {
        self.inner.passed()
    }

    /// Transient identity of this execution, shared with its audit and trace.
    #[getter]
    fn execution_id(&self) -> String {
        self.inner.execution_id.to_string()
    }

    /// The exact Verifier executed.
    #[getter]
    fn verifier(&self) -> CardRefPy {
        CardRefPy(self.inner.verifier.clone())
    }

    /// The exact subject judged.
    #[getter]
    fn subject(&self) -> CardRefPy {
        CardRefPy(self.inner.subject.clone())
    }

    /// The Verifier classification, `drift` or `eval`.
    ///
    /// # Errors
    /// Raises an internal error only if the wire enum stops serializing as a
    /// string.
    #[getter]
    fn kind(&self) -> WyrdPyResult<String> {
        wire_string(&self.inner.kind)
    }

    /// The common verdict: `passed`, `failed`, or `inconclusive`.
    ///
    /// # Errors
    /// As [`PyJudgment::kind`].
    #[getter]
    fn verdict(&self) -> WyrdPyResult<String> {
        wire_string(&self.inner.verdict)
    }

    /// Bounded human-readable summary of the verdict.
    #[getter]
    fn summary(&self) -> &str {
        &self.inner.summary
    }

    /// Count-only rollup of a Drift or Eval judgment as its wire mapping;
    /// `None` for a task Verifier.
    ///
    /// # Errors
    /// Raises an internal error when the rollup cannot be converted.
    #[getter]
    fn counts(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(self.inner.counts)?,
        )?)
    }

    /// The engine report as its wire mapping: `{"drift": ...}`, `{"eval": ...}`,
    /// or `{"task": {"result": ...}}`.
    ///
    /// # Errors
    /// Raises an internal error when the report cannot be converted.
    #[getter]
    fn detail(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(&self.inner.detail)?,
        )?)
    }
}

/// The wire spelling of a unit enum value.
///
/// # Errors
/// Raises an internal error when `value` does not serialize as a string.
fn wire_string<T: serde::Serialize>(value: &T) -> WyrdPyResult<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        other => Err(invalid_argument(
            "judgment",
            format!("unexpected wire value {other}"),
        )),
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
    module.add_class::<PyJudgment>()?;
    Ok(())
}
