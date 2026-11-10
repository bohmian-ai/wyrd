//! `wyrd.authoring`: the Python projection of `wyrd_client::authoring`.
//!
//! Each class wraps one shared Rust authoring value and nothing else. A
//! constructor takes the value's fixed arguments plus its other YAML fields as
//! keywords; a field setter replaces its field and an add method appends one
//! entry, both mutating the receiver and returning it. Passing a child copies
//! its current native value, so later edits to the child change nothing
//! already passed. Construction and editing never validate, read files, or
//! touch the network; `validate()` and declaration run the shared Rust
//! validation.
//!
//! Python adapts only arguments: plain JSON values, the native authoring
//! values, `wyrd.cards.CardRef`, and, for an input slot, a supported scalar
//! class or a class with `model_json_schema()` such as a Pydantic model. This
//! module never imports Pydantic.

use std::path::PathBuf;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyDict, PyFloat, PyInt, PyList, PyMapping, PyModule, PyString, PyTuple, PyType,
};
use serde_json::{Value, json};
use wyrd_cards::card_ref::CardRefPy;
use wyrd_client::authoring::{
    Action, Agent, Artifact, ArtifactFile, Audit, Binding, CardDefinition, CardRef, Check,
    CodeSource, Component, Data, Experiment, FieldValue, InputSlot, Mcp, Model, Operator, Prompt,
    Requirement, Service, Step, Trigger, Verifier,
};
use wyrd_client::cards::Declarable;
use wyrd_spec::envelope::CardKind;

use crate::observe::{invalid_argument, json_value};
use crate::operators::to_python;

/// Converts one Python argument for the field `field` into a native value.
type Convert<T> = fn(&str, &Bound<'_, PyAny>) -> PyResult<T>;

/// `value` as canonical JSON, the way observation evidence converts.
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` naming `field` when JSON cannot
/// represent `value`.
fn json(field: &str, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    Ok(json_value(value.py(), field, value)?)
}

/// `value` as a Python string.
///
/// # Errors
/// Raises `TypeError` when `value` is not a `str`.
fn string(_field: &str, value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.extract()
}

/// The JSON Schema a Python class stands for: `str`, `int`, `float`, `bool`
/// and `None`'s type map to their JSON types; any other class supplies its
/// own `model_json_schema()`, as a Pydantic model does.
///
/// # Errors
/// Raises `TypeError` for a class with neither form, and the class's own
/// error when its schema cannot be produced.
fn class_schema(field: &str, class: &Bound<'_, PyType>) -> PyResult<Value> {
    let py = class.py();
    let scalar = [
        (py.get_type::<PyBool>(), "boolean"),
        (py.get_type::<PyString>(), "string"),
        (py.get_type::<PyInt>(), "integer"),
        (py.get_type::<PyFloat>(), "number"),
        (py.None().bind(py).get_type(), "null"),
    ];
    if let Some((_, kind)) = scalar.iter().find(|(scalar, _)| scalar.is(class)) {
        return Ok(json!({ "type": kind }));
    }
    match class.getattr("model_json_schema") {
        Ok(schema) => json(field, &schema.call0()?),
        Err(_) => Err(PyTypeError::new_err(format!(
            "{field}: {} has no JSON Schema; pass str, int, float, bool, None, a class with \
             model_json_schema(), or a JSON Schema",
            class.qualname()?
        ))),
    }
}

/// A JSON Schema: a mapping as is, or the schema a class stands for.
///
/// # Errors
/// As [`class_schema`] and [`json`].
fn schema(field: &str, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    match value.cast::<PyType>() {
        Ok(class) => class_schema(field, class),
        Err(_) => json(field, value),
    }
}

/// An input slot: an `InputSlot`, or else a source slot's reference string,
/// a canonical slot mapping, or a JSON Schema or class standing for one, as
/// the shared `InputSlot` conversion reads them.
///
/// # Errors
/// As [`schema`].
fn slot(field: &str, value: &Bound<'_, PyAny>) -> PyResult<InputSlot> {
    if let Ok(slot) = value.cast::<PyInputSlot>() {
        return Ok(slot.borrow().inner.clone());
    }
    Ok(InputSlot::from(schema(field, value)?))
}

/// The named authoring Card `value` is, copied as it is now.
pub(crate) fn definition(value: &Bound<'_, PyAny>) -> Option<CardDefinition> {
    /// Return the first matching class's copy.
    macro_rules! cards {
        ($($class:ty),*) => {
            $(
                if let Ok(card) = value.cast::<$class>() {
                    return Some(card.borrow().inner.clone().into());
                }
            )*
        };
    }
    cards!(
        PyService,
        PyAgent,
        PyPrompt,
        PyVerifier,
        PyArtifact,
        PyModel,
        PyData,
        PyExperiment,
        PyMcp,
        PyAudit,
        PyTrigger,
        PyOperator
    );
    None
}

/// What `Cards.declare` and `WyrdState.declare` take besides a holder: an
/// authored Card, copied as it is now, or the path of a Card file or bundle
/// directory.
pub(crate) fn declarable(value: &Bound<'_, PyAny>) -> Option<Declarable<'static>> {
    match definition(value) {
        Some(card) => Some(Declarable::Card(Box::new(card))),
        None => value.extract::<PathBuf>().ok().map(Declarable::Path),
    }
}

/// A value at any authoring position: a Card, which becomes a sibling
/// reference; an authoring or `wyrd.cards` `CardRef`, which stays external;
/// any other authoring value; a `dict` or `list` composed entry by entry, so
/// these may stand anywhere inside it; or plain JSON, such as a reference
/// string.
///
/// # Errors
/// As [`json`], and as [`object`] for a mapping; also as [`enter`] for a
/// container nested too deep, which stops a container that holds itself.
fn field(field: &str, value: &Bound<'_, PyAny>) -> PyResult<FieldValue> {
    nested(field, value, 0)
}

/// The deepest container nesting a value may reach, the same bound
/// `serde_json` applies to the plain JSON that [`json`] parses.
const MAX_DEPTH: usize = 127;

/// The nesting depth inside one more container around a value at `depth`.
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` naming `field` past [`MAX_DEPTH`].
fn enter(field: &str, depth: usize) -> PyResult<usize> {
    if depth >= MAX_DEPTH {
        return Err(invalid_argument(
            field,
            format!("containers nest deeper than {MAX_DEPTH} levels"),
        )
        .into());
    }
    Ok(depth + 1)
}

/// [`field`] for a value already inside `depth` containers.
///
/// # Errors
/// As [`field`].
fn nested(field: &str, value: &Bound<'_, PyAny>, depth: usize) -> PyResult<FieldValue> {
    if let Some(card) = definition(value) {
        return Ok(card.into());
    }
    /// Return the first matching class's copy.
    macro_rules! values {
        ($($class:ty),*) => {
            $(
                if let Ok(value) = value.cast::<$class>() {
                    return Ok(value.borrow().inner.clone().into());
                }
            )*
        };
    }
    values!(
        PyCardRef,
        PyArtifactFile,
        PyCodeSource,
        PyComponent,
        PyBinding,
        PyStep,
        PyRequirement,
        PyInputSlot,
        PyAction,
        PyCheck
    );
    if let Ok(card_ref) = value.cast::<CardRefPy>() {
        return Ok(card_ref.get().0.clone().into());
    }
    if let Ok(mapping) = value.cast::<PyMapping>() {
        return Ok(object(field, mapping, depth)?.into_iter().collect());
    }
    if value.is_instance_of::<PyList>() || value.is_instance_of::<PyTuple>() {
        let depth = enter(field, depth)?;
        return value
            .try_iter()?
            .map(|item| nested(field, &item?, depth))
            .collect();
    }
    json(field, value).map(FieldValue::from)
}

/// The entries of a canonical mapping inside `depth` containers, each value
/// converted by [`field`].
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` naming `field` for a key that is not a
/// `str`, as plain JSON refuses one, as [`enter`] for the mapping itself, and
/// each value's conversion error.
fn object(
    field: &str,
    mapping: &Bound<'_, PyMapping>,
    depth: usize,
) -> PyResult<Vec<(String, FieldValue)>> {
    let depth = enter(field, depth)?;
    mapping
        .items()?
        .iter()
        .map(|entry| {
            let (key, value): (Bound<'_, PyAny>, Bound<'_, PyAny>) = entry.extract()?;
            let Ok(key) = key.cast::<PyString>() else {
                let shown = key
                    .repr()
                    .map_or_else(|_| "<unrepresentable>".to_owned(), |repr| repr.to_string());
                return Err(
                    invalid_argument(field, format!("mapping key {shown} is not a str")).into(),
                );
            };
            Ok((key.to_str()?.to_owned(), nested(field, &value, depth)?))
        })
        .collect()
}

/// A verification binding: a `Binding`, or a Verifier, `CardRef` or YAML
/// path bound with no trigger or Operators, or a canonical binding mapping
/// whose values convert as [`field`] converts them.
///
/// # Errors
/// As [`field`].
fn binding(field_name: &str, value: &Bound<'_, PyAny>) -> PyResult<Binding> {
    if let Ok(binding) = value.cast::<PyBinding>() {
        return Ok(binding.borrow().inner.clone());
    }
    if let Ok(mapping) = value.cast::<PyMapping>() {
        return Ok(object(field_name, mapping, 0)?.into_iter().collect());
    }
    field(field_name, value).map(Binding::new)
}

/// Generate the converter for a sub-value: its class, or its canonical
/// mapping kept as authored, with any authoring value inside it converted as
/// [`field`] converts it.
macro_rules! canonical {
    ($($name:ident: $class:ty => $native:ty),* $(,)?) => {
        $(
            #[doc = concat!("A `", stringify!($native), "`, or its canonical mapping kept as authored, whose values convert as [`field`] converts them.")]
            ///
            /// # Errors
            /// As [`field`]; any other value as [`json`], kept for validation
            /// to refuse.
            fn $name(field: &str, value: &Bound<'_, PyAny>) -> PyResult<$native> {
                if let Ok(value) = value.cast::<$class>() {
                    return Ok(value.borrow().inner.clone());
                }
                if let Ok(mapping) = value.cast::<PyMapping>() {
                    return Ok(object(field, mapping, 0)?.into_iter().collect());
                }
                json(field, value).map(<$native>::from)
            }
        )*
    };
}

canonical!(
    component: PyComponent => Component,
    step: PyStep => Step,
    requirement: PyRequirement => Requirement,
    action: PyAction => Action,
    check: PyCheck => Check,
);

/// The items of a list field, each converted by `convert`.
///
/// # Errors
/// Raises `TypeError` for a string, bytes, mapping or non-iterable value,
/// and each item's conversion error.
fn items<T>(field: &str, value: &Bound<'_, PyAny>, convert: Convert<T>) -> PyResult<Vec<T>> {
    if value.is_instance_of::<PyString>()
        || value.is_instance_of::<pyo3::types::PyBytes>()
        || value.cast::<PyMapping>().is_ok()
    {
        return Err(PyTypeError::new_err(format!("{field} takes a list")));
    }
    value
        .try_iter()?
        .map(|item| convert(field, &item?))
        .collect()
}

/// The entries of a map field, each value converted by `convert`.
///
/// # Errors
/// Raises `TypeError` for a value that is not a mapping or a key that is not
/// a `str`, and each value's conversion error.
fn entries<T>(
    field: &str,
    value: &Bound<'_, PyAny>,
    convert: Convert<T>,
) -> PyResult<Vec<(String, T)>> {
    let mapping = value
        .cast::<PyMapping>()
        .map_err(|_| PyTypeError::new_err(format!("{field} takes a mapping")))?;
    mapping
        .items()?
        .iter()
        .map(|entry| {
            let (key, value): (String, Bound<'_, PyAny>) = entry.extract()?;
            Ok((key, convert(field, &value)?))
        })
        .collect()
}

/// The refusal of a keyword that names no field of `class`.
fn unexpected(class: &str, field: &str) -> PyErr {
    PyTypeError::new_err(format!(
        "{class}() got an unexpected keyword argument '{field}'"
    ))
}

/// Generate one authoring class over a native value: its field setters, list
/// and map setters with their add methods, keyword-field initialization, and
/// any further methods given verbatim.
macro_rules! authoring_class {
    (
        $(#[$doc:meta])*
        $py:ident($native:ty) = $name:literal {
            one { $($one:ident: $one_convert:ident),* $(,)? }
            list { $($list:ident, $list_add:ident: $list_convert:ident),* $(,)? }
            map { $($map:ident, $map_add:ident: $map_convert:ident),* $(,)? }
            $(nullable { $($null:ident: $clear:ident),* $(,)? })?
        }
        $($methods:tt)*
    ) => {
        $(#[$doc])*
        #[pyclass(module = "wyrd.authoring", name = $name)]
        pub(crate) struct $py {
            /// The shared native value.
            inner: $native,
        }

        impl $py {
            /// `inner` with the field `field` set from `value`, as its setter
            /// sets it; `None` at a nullable field sets it to `null`.
            ///
            /// # Errors
            /// Raises `TypeError` for a name that is no field, and the
            /// value's conversion error.
            fn set(inner: $native, field: &str, value: &Bound<'_, PyAny>) -> PyResult<$native> {
                Ok(match field {
                    $($(stringify!($null) if value.is_none() => inner.$clear(),)*)?
                    $(stringify!($one) => inner.$one($one_convert(field, value)?),)*
                    $(stringify!($list) => inner.$list(items(field, value, $list_convert)?),)*
                    $(stringify!($map) => inner.$map(entries(field, value, $map_convert)?),)*
                    _ => return Err(unexpected($name, field)),
                })
            }

            /// `inner` with every keyword field set, in keyword order.
            ///
            /// # Errors
            /// As [`Self::set`].
            fn with_fields(
                mut inner: $native,
                fields: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<$native> {
                for (key, value) in fields.into_iter().flatten() {
                    inner = Self::set(inner, key.extract::<&str>()?, &value)?;
                }
                Ok(inner)
            }

            /// Set the field `field` of the receiver from `value` and return
            /// the receiver; `value` converts while the receiver is not
            /// borrowed, so it may be the receiver itself or a callback that
            /// edits it.
            ///
            /// # Errors
            /// As [`Self::set`].
            fn edit<'py>(
                slf: Bound<'py, Self>,
                field: &str,
                value: &Bound<'py, PyAny>,
            ) -> PyResult<Bound<'py, Self>> {
                let current = slf.borrow().inner.clone();
                let inner = Self::set(current, field, value)?;
                slf.borrow_mut().inner = inner;
                Ok(slf)
            }
        }

        #[pymethods]
        impl $py {
            $(
                #[doc = concat!("Set `", stringify!($one), "`, replacing its earlier value and whatever it referenced; returns this value.")]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = concat!("As [`", stringify!($one_convert), "`], naming `", stringify!($one), "`, when `value` does not convert; nothing is then set.")]
                fn $one<'py>(
                    slf: Bound<'py, Self>,
                    value: &Bound<'py, PyAny>,
                ) -> PyResult<Bound<'py, Self>> {
                    Self::edit(slf, stringify!($one), value)
                }
            )*
            $(
                #[doc = concat!("Set the whole `", stringify!($list), "` list, replacing every earlier entry; returns this value.")]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = concat!("As [`items`] with [`", stringify!($list_convert), "`], naming `", stringify!($list), "`, when `items` does not convert; nothing is then set.")]
                fn $list<'py>(
                    slf: Bound<'py, Self>,
                    items: &Bound<'py, PyAny>,
                ) -> PyResult<Bound<'py, Self>> {
                    Self::edit(slf, stringify!($list), items)
                }

                #[doc = concat!("Append one entry to `", stringify!($list), "`; a repeated ID is refused at validation. Returns this value.")]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = concat!("As [`", stringify!($list_convert), "`], naming `", stringify!($list), "`, when `item` does not convert; nothing is then appended.")]
                fn $list_add<'py>(
                    slf: Bound<'py, Self>,
                    item: &Bound<'py, PyAny>,
                ) -> PyResult<Bound<'py, Self>> {
                    let item = $list_convert(stringify!($list), item)?;
                    slf.borrow_mut().inner.$list_add(item);
                    Ok(slf)
                }
            )*
            $(
                #[doc = concat!("Set the whole `", stringify!($map), "` map, replacing every earlier entry; returns this value.")]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = concat!("As [`entries`] with [`", stringify!($map_convert), "`], naming `", stringify!($map), "`, when `entries` does not convert; nothing is then set.")]
                fn $map<'py>(
                    slf: Bound<'py, Self>,
                    entries: &Bound<'py, PyAny>,
                ) -> PyResult<Bound<'py, Self>> {
                    Self::edit(slf, stringify!($map), entries)
                }

                #[doc = concat!("Insert the entry `key` into `", stringify!($map), "`; a repeated key is refused at validation. Returns this value.")]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = concat!("As [`", stringify!($map_convert), "`], naming `", stringify!($map), "`, when `value` does not convert; this value is then unchanged.")]
                fn $map_add<'py>(
                    slf: Bound<'py, Self>,
                    key: String,
                    value: &Bound<'py, PyAny>,
                ) -> PyResult<Bound<'py, Self>> {
                    let value = $map_convert(stringify!($map), value)?;
                    slf.borrow_mut().inner.$map_add(key, value);
                    Ok(slf)
                }
            )*
            $($methods)*
        }
    };
}

/// Generate one named Card class: `(name, version, **fields)` construction,
/// labels and annotations, its reference, its spec, and validation.
macro_rules! card_class {
    (
        $(#[$doc:meta])*
        $py:ident($native:ident) = $name:literal {
            one { $($one:tt)* }
            list { $($list:tt)* }
            map { $($map:tt)* }
            $(nullable { $($null:tt)* })?
        }
        $($methods:tt)*
    ) => {
        authoring_class! {
            $(#[$doc])*
            $py($native) = $name {
                one { $($one)* }
                list { $($list)* }
                map { labels, add_label: string, annotations, add_annotation: string, $($map)* }
                $(nullable { $($null)* })?
            }

            /// An empty Card named `name` at `version`, with any spec fields
            /// given as keywords; nothing is validated until its graph is.
            ///
            /// # Errors
            /// Raises `TypeError` for a keyword that names no field, and a
            /// field value's conversion error.
            #[new]
            #[pyo3(signature = (name, version, **fields))]
            fn __new__(
                name: String,
                version: String,
                fields: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<Self> {
                Ok(Self {
                    inner: Self::with_fields(<$native>::new(name, version), fields)?,
                })
            }

            /// The exact reference this Card registers as.
            fn card_ref(&self) -> PyCardRef {
                PyCardRef {
                    inner: self.inner.card_ref(),
                }
            }

            /// The spec as authored so far, before validation; a
            /// `CodeSource.file` position is `None` until validation.
            ///
            /// # Errors
            /// Raises a Python error when the spec cannot be converted.
            fn spec(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
                Ok(to_python(py, &self.inner.spec())?)
            }

            /// Validate the complete graph this Card roots exactly as its
            /// declaration would, without registering or uploading anything;
            /// local code files are read with the GIL released.
            ///
            /// # Errors
            /// Raises the shared validation's `WyrdError`, such as
            /// `WYRD_LOADER_400_INVALID_ENVELOPE` naming each refused YAML
            /// field path.
            fn validate(&self, py: Python<'_>) -> PyResult<()> {
                let card = self.inner.clone();
                Ok(py
                    .detach(|| card.validate())
                    .map_err(wyrd_utils::py::WyrdPyError::from)?)
            }

            $($methods)*
        }
    };
}

card_class! {
    /// A Service Card: a running system, its components, and the Verifiers
    /// that check it.
    PyService(Service) = "Service" {
        one {
            description: json, service_type: json, entry_point: json, runtime: json,
            content_hash: json, lock_hash: json,
        }
        list {
            components, add_component: component, verified_by, add_verified_by: binding,
            tables, add_table: json, credential_refs, add_credential_ref: json,
        }
        map {
            deployment, add_deployment: json, service_config, add_service_config: json,
            metadata, add_metadata: json,
        }
    }
}

card_class! {
    /// An Agent Card: a prompt run through an execution route with tools.
    PyAgent(Agent) = "Agent" {
        one { prompt: field, execution_route: json, run_config: json }
        list { tool_names, add_tool_name: json, verified_by, add_verified_by: binding }
        map {}
    }
}

card_class! {
    /// A Prompt Card: provider, model, and the authored prompt.
    PyPrompt(Prompt) = "Prompt" {
        one {
            provider: json, model: json, system: json, settings: json,
            response_format: json, format: json,
        }
        list { messages, add_message: json }
        map { body, add_body: json }
        nullable { messages: clear_messages, body: clear_body }
    }
}

card_class! {
    /// A Verifier Card: the evidence it accepts, the steps that compute over
    /// it, and the requirements that judge it.
    PyVerifier(Verifier) = "Verifier" {
        one { description: json, decide: json, context_capture: json, order_by: json }
        list { steps, add_step: step, requirements, add_requirement: requirement }
        map { inputs, add_input: slot }
    }

    /// The step stages the server would execute, after validating this
    /// Verifier as its YAML file would be, as `{stages, task_count}`.
    ///
    /// # Errors
    /// Raises `WYRD_LOADER_400_INVALID_ENVELOPE` for a field that does not
    /// decode, and the shared Verifier validation's refusal.
    fn plan(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let verifier = self.inner.clone();
        let plan = py
            .detach(|| verifier.plan())
            .map_err(wyrd_utils::py::WyrdPyError::from)?;
        Ok(to_python(py, &plan)?)
    }
}

card_class! {
    /// An Artifact Card: a local file or directory published before the graph
    /// that references it, or an Artifact defined only by its spec.
    PyArtifact(Artifact) = "Artifact" {
        one {
            artifact_kind: json, content_type: json, size_bytes: json, integrity: json,
            schema_ref: field, framework_adapter: json, external_uri: json,
        }
        list { artifact_uris, add_artifact_uri: json }
        map { metadata, add_metadata: json }
    }

    /// The `file` Artifact named `name` at `version`, published from the
    /// file or directory at `path` before its graph; nothing is read until
    /// it is published.
    ///
    /// # Errors
    /// As the constructor.
    #[staticmethod]
    #[pyo3(signature = (name, version, path, **fields))]
    fn from_path(
        name: String,
        version: String,
        path: PathBuf,
        fields: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Artifact::from_path(name, version, path), fields)?,
        })
    }

    /// Set whether supported documents are extracted beside their originals
    /// when the local source is published; returns this Artifact.
    fn extract(slf: Bound<'_, Self>, extract: bool) -> Bound<'_, Self> {
        {
            let mut this = slf.borrow_mut();
            this.inner = this.inner.clone().extract(extract);
        }
        slf
    }

    /// Select the fixed file or directory prefix `path` of this exact
    /// Artifact version; a local Artifact is published first.
    fn file(&self, path: String) -> PyArtifactFile {
        PyArtifactFile {
            inner: self.inner.file(path),
        }
    }
}

card_class! {
    /// A Model Card: a trained model's interface, task, and signature.
    PyModel(Model) = "Model" {
        one { interface: json, task_type: json, signature: json, sample_input: json }
        list { card_refs, add_card_ref: field }
        map {}
    }
}

card_class! {
    /// A Data Card: a dataset's interface, schema, splits, and target columns.
    PyData(Data) = "Data" {
        one { interface: json, schema: json, sql: json, stats: json }
        list { card_refs, add_card_ref: field, target_columns, add_target_column: json }
        map { splits, add_split: field }
    }
}

card_class! {
    /// An Experiment Card: the targets and parameters of a set of runs, with
    /// their summary metrics and best run.
    PyExperiment(Experiment) = "Experiment" {
        one { experiment_type: json, description: json, best_run_ref: json }
        list {
            target_refs, add_target_ref: field, run_refs, add_run_ref: json,
            summary_metrics, add_summary_metric: json, card_refs, add_card_ref: field,
        }
        map { default_parameters, add_default_parameter: json, details, add_detail: json }
    }
}

card_class! {
    /// An Mcp Card: an MCP server and the tools it exposes.
    PyMcp(Mcp) = "Mcp" {
        one { description: json, server_name: json, transport: json }
        list { tool_refs, add_tool_ref: field, scopes, add_scope: json }
        map { details, add_detail: json }
    }
}

card_class! {
    /// An Audit Card: subjects, policies, and evidence.
    PyAudit(Audit) = "Audit" {
        one { description: json }
        list {
            subject_refs, add_subject_ref: field, policy_refs, add_policy_ref: field,
            evidence_refs, add_evidence_ref: field,
        }
        map { details, add_detail: json }
    }
}

card_class! {
    /// A Trigger Card: when a binding's Verifier runs.
    PyTrigger(Trigger) = "Trigger" {
        one { description: json, kind: json, cron: json, tz: json, after: json }
        list {}
        map {}
    }
}

card_class! {
    /// An Operator Card: what runs when a binding's Verifier fails or changes
    /// verdict.
    PyOperator(Operator) = "Operator" {
        one { description: json, action: json, budget: json }
        list {}
        map {}
    }
}

authoring_class! {
    /// One Service component: an alias for a referenced Card and the
    /// Verifiers that check it.
    PyComponent(Component) = "Component" {
        one { source: json }
        list { verified_by, add_verified_by: binding, credential_refs, add_credential_ref: json }
        map { config, add_config: json }
    }

    /// The component `alias` for `ref`: a Card, an exact `CardRef`, or a YAML
    /// reference path, with any other fields given as keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[new]
    #[pyo3(signature = (alias, r#ref, **fields))]
    fn __new__(
        alias: String,
        r#ref: &Bound<'_, PyAny>,
        fields: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Component::new(alias, field("ref", r#ref)?), fields)?,
        })
    }
}

authoring_class! {
    /// One verification binding: the Verifier to run, when it runs, and the
    /// Operators that react to its verdicts.
    PyBinding(Binding) = "Binding" {
        one { runs_on: field }
        list { on_failure, add_on_failure: field, on_verdict_change, add_on_verdict_change: field }
        map {}
    }

    /// Bind `verifier`: a `Verifier`, an exact `CardRef`, or a YAML reference
    /// path, with any other fields given as keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[new]
    #[pyo3(signature = (verifier, **fields))]
    fn __new__(verifier: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Binding::new(field("verifier", verifier)?), fields)?,
        })
    }
}

authoring_class! {
    /// One data-producing Verifier step.
    PyStep(Step) = "Step" {
        one { when: json, timeout_seconds: json, retries: json }
        list { depends_on, add_depends_on: json, requirements, add_requirement: requirement }
        map {}
    }

    /// The step `id` running `action`, with any other fields given as
    /// keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[new]
    #[pyo3(signature = (id, action, **fields))]
    fn __new__(
        id: String,
        action: &Bound<'_, PyAny>,
        fields: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Step::new(id, self::action("action", action)?), fields)?,
        })
    }
}

/// One requirement, producing exactly one verdict.
#[pyclass(module = "wyrd.authoring", name = "Requirement")]
pub(crate) struct PyRequirement {
    /// The shared native requirement.
    inner: Requirement,
}

#[pymethods]
impl PyRequirement {
    /// The requirement `id` claiming `statement`, judged by `check`.
    ///
    /// # Errors
    /// As [`json`] for a canonical check mapping.
    #[new]
    #[pyo3(signature = (id, statement, check))]
    fn __new__(id: String, statement: String, check: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Requirement::new(id, statement, self::check("check", check)?),
        })
    }
}

authoring_class! {
    /// One named evidence slot of a Verifier.
    PyInputSlot(InputSlot) = "InputSlot" {
        one { max_bytes: json }
        list {}
        map {}
    }

    /// A JSON value constrained by `schema`: a JSON Schema, or a class that
    /// stands for one.
    ///
    /// # Errors
    /// As [`schema`].
    #[staticmethod]
    #[pyo3(signature = (schema))]
    fn json(schema: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: InputSlot::json(self::schema("schema", schema)?),
        })
    }

    /// A pinned file in one of `formats`, such as `"pdf"`.
    ///
    /// # Errors
    /// As the field setters.
    #[staticmethod]
    #[pyo3(signature = (formats, **fields))]
    fn media(formats: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(InputSlot::media(items("formats", formats, json)?), fields)?,
        })
    }

    /// A trace resolved through tenant-scoped trace reads.
    #[staticmethod]
    #[pyo3(signature = ())]
    fn trace() -> Self {
        Self {
            inner: InputSlot::trace(),
        }
    }

    /// Recorded measurements of `features` for a scheduled window.
    ///
    /// # Errors
    /// As [`items`].
    #[staticmethod]
    #[pyo3(signature = (features))]
    fn samples(features: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: InputSlot::samples(items("features", features, json)?),
        })
    }

    /// A group Verifier's input authored as the source reference `text`.
    #[staticmethod]
    #[pyo3(signature = (text))]
    fn source(text: String) -> Self {
        Self {
            inner: InputSlot::source(text),
        }
    }
}

authoring_class! {
    /// A step action: extract, an Agent call, or sandboxed code.
    PyAction(Action) = "Action" {
        one { runtime: json }
        list {}
        map { inputs, bind_input: json, workspace, add_workspace: field }
    }

    /// Extract text and structure from the media slot `from_`, such as
    /// `inputs.report`.
    #[staticmethod]
    #[pyo3(signature = (from_))]
    fn extract(from_: String) -> Self {
        Self {
            inner: Action::extract(from_),
        }
    }

    /// Invoke `agent`: an `Agent`, an exact `CardRef`, a YAML reference path,
    /// or an inline Agent mapping, with any other fields given as keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[staticmethod]
    #[pyo3(signature = (agent, **fields))]
    fn agent(agent: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Action::agent(field("agent", agent)?), fields)?,
        })
    }

    /// Run `entrypoint` of `source` as sandboxed code returning JSON data,
    /// with any other fields given as keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[staticmethod]
    #[pyo3(signature = (source, entrypoint, **fields))]
    fn code(
        source: &Bound<'_, PyAny>,
        entrypoint: String,
        fields: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Action::code(field("source", source)?, entrypoint), fields)?,
        })
    }
}

authoring_class! {
    /// A verdict-producing requirement check.
    PyCheck(Check) = "Check" {
        one { runtime: json, response_format: json }
        list {}
        map { inputs, bind_input: json, workspace, add_workspace: field }
    }

    /// Compare the one value at `path` with `expected` by `operator`, such as
    /// `"equals"` or its object form; the YAML `kind` is `assert`.
    ///
    /// # Errors
    /// As [`json`].
    #[staticmethod]
    #[pyo3(signature = (path, operator, expected))]
    fn assert_(
        path: String,
        operator: &Bound<'_, PyAny>,
        expected: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Check::assert(path, json("operator", operator)?, json("expected", expected)?),
        })
    }

    /// Compare every value `each` selects through `[*]` with `expected` by
    /// `operator`.
    ///
    /// # Errors
    /// As [`json`].
    #[staticmethod]
    #[pyo3(signature = (each, operator, expected))]
    fn each(
        each: String,
        operator: &Bound<'_, PyAny>,
        expected: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Check::each(each, json("operator", operator)?, json("expected", expected)?),
        })
    }

    /// Compare the spans `span_selector` selects in the trace slot `on` with
    /// `expected` by `operator`.
    ///
    /// # Errors
    /// As [`json`].
    #[staticmethod]
    #[pyo3(signature = (on, span_selector, operator, expected))]
    fn trace(
        on: String,
        span_selector: String,
        operator: &Bound<'_, PyAny>,
        expected: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Check::trace(
                on,
                span_selector,
                json("operator", operator)?,
                json("expected", expected)?,
            ),
        })
    }

    /// Judge with `agent`, which returns the Verdict schema, with any other
    /// fields given as keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[staticmethod]
    #[pyo3(signature = (agent, **fields))]
    fn agent(agent: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Check::agent(field("agent", agent)?), fields)?,
        })
    }

    /// Judge with `entrypoint` of `source`, which returns the Verdict schema,
    /// with any other fields given as keywords.
    ///
    /// # Errors
    /// As the field setters.
    #[staticmethod]
    #[pyo3(signature = (source, entrypoint, **fields))]
    fn code(
        source: &Bound<'_, PyAny>,
        entrypoint: String,
        fields: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Self::with_fields(Check::code(field("source", source)?, entrypoint), fields)?,
        })
    }

    /// Judge statistical drift over the samples slot `on` with the drift
    /// fields `spec`: method, signal, condition, and profile; a Card or
    /// `CardRef` may stand at `signal.baseline_ref`.
    ///
    /// # Errors
    /// As [`field`].
    #[staticmethod]
    #[pyo3(signature = (on, spec))]
    fn drift(on: String, spec: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Check::drift(on, field("spec", spec)?),
        })
    }
}

/// `value` as a Card kind: a `wyrd.cards.CardKind` or its wire name.
///
/// # Errors
/// Raises `WYRD_SPEC_400_VALIDATION` for an unknown kind.
fn kind_arg(value: &Bound<'_, PyAny>) -> PyResult<CardKind> {
    let name: String = match value.extract() {
        Ok(name) => name,
        Err(_) => value.getattr("name")?.extract()?,
    };
    CardKind::from_wire_name(&name)
        .ok_or_else(|| invalid_argument("kind", format!("unknown Card kind {name:?}")).into())
}

/// An exact Card reference, kept as authored until validation; at a
/// reference position it stays external and nothing is uploaded for it.
#[pyclass(module = "wyrd.authoring", name = "CardRef", frozen, eq)]
#[derive(PartialEq)]
pub(crate) struct PyCardRef {
    /// The shared native reference.
    inner: CardRef,
}

/// Generate `CardRef` shorthands, one per named kind.
macro_rules! card_ref_shorthands {
    ($($name:ident => $kind:ident),* $(,)?) => {
        #[pymethods]
        impl PyCardRef {
            /// The exact reference to the `kind` Card `name` at `version`.
            ///
            /// # Errors
            /// As [`kind_arg`].
            #[new]
            #[pyo3(signature = (kind, name, version))]
            fn __new__(kind: &Bound<'_, PyAny>, name: String, version: String) -> PyResult<Self> {
                Ok(Self {
                    inner: CardRef::new(kind_arg(kind)?, name, version),
                })
            }

            $(
                #[doc = concat!("The exact reference to the ", stringify!($kind), " Card `name` at `version`.")]
                #[staticmethod]
                fn $name(name: String, version: String) -> Self {
                    Self {
                        inner: CardRef::$name(name, version),
                    }
                }
            )*

            /// The referenced kind, as a `wyrd.cards.CardKind`.
            ///
            /// # Errors
            /// Raises a Python error when `wyrd.cards` cannot be imported.
            #[getter]
            fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                py.import("wyrd.cards")?
                    .getattr("CardKind")?
                    .getattr(self.inner.kind().wire_name())
            }

            /// The referenced name, as authored.
            #[getter]
            fn name(&self) -> &str {
                self.inner.name()
            }

            /// The referenced version, as authored.
            #[getter]
            fn version(&self) -> &str {
                self.inner.version()
            }

            /// Select the fixed file or directory prefix `path` of this
            /// registered Artifact version, with nothing uploaded.
            fn file(&self, path: String) -> PyArtifactFile {
                PyArtifactFile {
                    inner: self.inner.file(path),
                }
            }
        }
    };
}

card_ref_shorthands!(
    data => Data,
    model => Model,
    artifact => Artifact,
    experiment => Experiment,
    prompt => Prompt,
    agent => Agent,
    mcp => Mcp,
    service => Service,
    audit => Audit,
    verifier => Verifier,
    trigger => Trigger,
    operator => Operator,
);

/// A fixed file or directory prefix of one exact Artifact version, mounted
/// into a workspace or run as code; built by `Artifact.file` or
/// `CardRef.file`.
#[pyclass(module = "wyrd.authoring", name = "ArtifactFile", frozen)]
pub(crate) struct PyArtifactFile {
    /// The shared native selection.
    inner: ArtifactFile,
}

/// The source of a code action or check: inline text, or a local file
/// captured as inline text when its graph is validated.
#[pyclass(module = "wyrd.authoring", name = "CodeSource", frozen)]
pub(crate) struct PyCodeSource {
    /// The shared native source.
    inner: CodeSource,
}

#[pymethods]
impl PyCodeSource {
    /// Run `text`, embedded in the Card as `{inline: text}`.
    #[staticmethod]
    fn inline(text: String) -> Self {
        Self {
            inner: CodeSource::inline(text),
        }
    }

    /// Run the UTF-8 file at `path`, read when the graph is validated or
    /// declared; nothing is read now.
    #[staticmethod]
    fn file(path: PathBuf) -> Self {
        Self {
            inner: CodeSource::file(path),
        }
    }
}

/// Register every authoring class on `wyrd._wyrd.authoring`.
///
/// # Errors
/// Propagates the Python error from any failed `add_class` call.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyService>()?;
    module.add_class::<PyAgent>()?;
    module.add_class::<PyPrompt>()?;
    module.add_class::<PyVerifier>()?;
    module.add_class::<PyArtifact>()?;
    module.add_class::<PyModel>()?;
    module.add_class::<PyData>()?;
    module.add_class::<PyExperiment>()?;
    module.add_class::<PyMcp>()?;
    module.add_class::<PyAudit>()?;
    module.add_class::<PyTrigger>()?;
    module.add_class::<PyOperator>()?;
    module.add_class::<PyComponent>()?;
    module.add_class::<PyBinding>()?;
    module.add_class::<PyStep>()?;
    module.add_class::<PyRequirement>()?;
    module.add_class::<PyInputSlot>()?;
    module.add_class::<PyAction>()?;
    module.add_class::<PyCheck>()?;
    module.add_class::<PyCardRef>()?;
    module.add_class::<PyArtifactFile>()?;
    module.add_class::<PyCodeSource>()?;
    Ok(())
}
