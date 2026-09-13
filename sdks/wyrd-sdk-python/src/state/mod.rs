//! Python projections for the Rust-owned Wyrd client SDK.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyMapping, PyModule, PyTuple};
use secrecy::SecretString;
use tempfile::{TempDir, tempdir};
use wyrd_cards::card_ref::{CardRefPy, Kind};
use wyrd_cards::{agent::PyAgentCard, data::DataCard, model::ModelCard, prompt::PromptCard};
use wyrd_client::cards::{CardSelector, Cards};
use wyrd_interfaces::error::{CardPyResult, WyrdPyError};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind, Metadata};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::query::MetadataQuery;
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::registry::{
    CardLifecycleStatus, CardRegistrationOutcome, CardSummary, ListCardsRequest, ListCardsResponse,
    RegistrationOutcomeKind, RegistrationReceipt,
};

use wyrd_client::state::WyrdState;

/// Python state hydration boundary and all-or-nothing holder owner.
mod hydrator;
/// Python Card registration boundary and prepared-registration owner.
mod registry;

use hydrator::{PythonLoadConfig, PythonStateHydrator};
use registry::PythonCardRegistry;

/// Immutable Python projection of one complete Card envelope.
#[pyclass(module = "wyrd.state", name = "CardEnvelope", frozen)]
pub struct PyCardEnvelope {
    /// Complete native Card retained as the projection source of truth.
    card: Card,
    /// Exact identity validated during bundle loading.
    card_ref: CardRef,
    /// Stable aliases resolving to this exact Card.
    aliases: Vec<String>,
}

/// Immutable Python descriptor for one verified local artifact.
#[pyclass(module = "wyrd.state", name = "HydratedArtifact", frozen)]
pub struct PyHydratedArtifact {
    /// Manifest-relative artifact path.
    relative_path: String,
    /// Confined absolute local payload path.
    local_path: PathBuf,
    /// Verified base64-encoded SHA-256 digest supplied by the manifest.
    sha256: String,
    /// Verified payload length.
    size_bytes: u64,
    /// Optional declared media type.
    content_type: Option<String>,
}

/// Python wrapper for a locally hydrated `WyrdState`.
#[pyclass(
    module = "wyrd.state",
    name = "WyrdState",
    skip_from_py_object,
    weakref
)]
pub struct PyWyrdState {
    /// Native immutable validated graph.
    inner: WyrdState,
    /// Generic Card projections by exact `CardRef`.
    envelopes: BTreeMap<String, Py<PyCardEnvelope>>,
    /// Hydrated Agent holders by exact `CardRef`.
    agents: BTreeMap<String, Py<PyAgentCard>>,
    /// Hydrated Prompt holders by exact `CardRef`.
    prompts: BTreeMap<String, Py<PromptCard>>,
    /// Loaded Model holders by exact `CardRef`.
    models: BTreeMap<String, Py<ModelCard>>,
    /// Loaded Data holders by exact `CardRef`.
    data: BTreeMap<String, Py<DataCard>>,
}

#[pymethods]
impl PyWyrdState {
    /// Load and eagerly hydrate a complete local Service bundle offline.
    ///
    /// The native graph is loaded first while detached from the GIL. Python
    /// mappings are then normalized and each exact `CardRef` is hydrated once;
    /// construction publishes in order: envelopes, Prompts, Agents, Models,
    /// then Data. No registry or network access occurs. A failed local read or
    /// interface load leaves no published Python state; local reads may be
    /// partial before failure and callers may retry from the original path.
    ///
    /// ```python
    /// from wyrd.state import WyrdState
    ///
    /// state = WyrdState.from_path("./bundle")
    /// model = state.model("fraud-model")
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a stable SDK error for malformed bundles, mapping conversion,
    /// Card-kind conflicts, runtime loader failures, or Python allocation.
    ///
    #[staticmethod]
    #[pyo3(signature = (path, *, interfaces=None, load_kwargs=None, trusted_artifact_hashes=None))]
    // justification: pyo3 boundary; the extractor produces an owned PathBuf, and the path is moved into the detached filesystem operation
    #[allow(clippy::needless_pass_by_value)]
    fn from_path(
        py: Python<'_>,
        path: PathBuf,
        interfaces: Option<&Bound<'_, PyMapping>>,
        load_kwargs: Option<&Bound<'_, PyMapping>>,
        trusted_artifact_hashes: Option<&Bound<'_, PyMapping>>,
    ) -> CardPyResult<Self> {
        let inner = py
            .detach(|| WyrdState::from_path(&path))
            .map_err(WyrdPyError::from)?;
        let config = PythonStateHydrator::normalize_config(
            py,
            &inner,
            interfaces,
            load_kwargs,
            trusted_artifact_hashes,
        )?;
        let hydrated = PythonStateHydrator::new(&inner, config).hydrate(py)?;
        Ok(Self {
            inner,
            envelopes: hydrated.envelopes,
            agents: hydrated.agents,
            prompts: hydrated.prompts,
            models: hydrated.models,
            data: hydrated.data,
        })
    }

    /// Return the persistent root Service envelope without reading payloads.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state error if the validated root envelope is absent.
    #[getter]
    fn service(&self, py: Python<'_>) -> CardPyResult<Py<PyCardEnvelope>> {
        // The index invariant guarantees the root envelope exists after load.
        self.envelopes
            .get(&self.inner.root_ref().to_string())
            .map(|v| v.clone_ref(py))
            .ok_or_else(|| {
                WyrdPyError::from(WyrdError::SdkInvalidStateBundle {
                    message: "root envelope is missing".to_owned(),
                    details: serde_json::json!({"card_ref": self.inner.root_ref()}),
                })
            })
    }

    /// Persisted aliases in stable order.
    ///
    /// # Errors
    ///
    /// Returns a stable SDK allocation error if Python cannot allocate the
    /// result tuple.
    #[getter]
    fn aliases(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let aliases = self.inner.aliases().map(str::to_owned).collect::<Vec<_>>();
        Ok(PyTuple::new(py, aliases)?.unbind().into_any())
    }

    /// Return a complete envelope selected by alias.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias or invalid-state errors when the alias cannot be
    /// resolved to its persistent envelope.
    fn card(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PyCardEnvelope>> {
        envelope_inner(&self.inner, &self.envelopes, py, alias, None)
    }
    /// Return an exact `CardRef` selected by alias.
    ///
    /// # Errors
    ///
    /// Returns an unknown-alias error when the persisted alias is absent.
    fn card_ref(&self, alias: &str) -> CardPyResult<CardRefPy> {
        Ok(CardRefPy(self.inner.card_ref(alias)?.clone()))
    }
    /// Return verified artifact descriptors without reading payload bytes.
    ///
    /// # Errors
    ///
    /// Returns artifact lookup, Python allocation, or tuple allocation errors.
    fn artifacts(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PyAny>> {
        let card_ref = self.inner.card_ref(alias)?.clone();
        let items = self
            .inner
            .artifacts(alias)?
            .iter()
            .map(|artifact| {
                Py::new(
                    py,
                    PyHydratedArtifact {
                        relative_path: artifact.relative_path().to_owned(),
                        local_path: artifact.local_path().to_path_buf(),
                        sha256: artifact.sha256().to_owned(),
                        size_bytes: artifact.size_bytes(),
                        content_type: artifact.content_type().map(str::to_owned),
                    },
                )
                .map_err(|_| WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                    message: "Python artifact allocation failed".to_owned(),
                    details: serde_json::json!({"alias": alias, "card_ref": card_ref, "stage": "python_allocation", "reason": "artifact allocation failed"}),
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;
        PyTuple::new(py, items)
            .map(|tuple| tuple.unbind().into_any())
            .map_err(|_| WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                message: "Python artifact tuple allocation failed".to_owned(),
                details: serde_json::json!({"alias": alias, "card_ref": card_ref, "stage": "python_allocation", "reason": "artifact tuple allocation failed"}),
            }))
    }
    /// Return the eagerly loaded Model holder selected by alias.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn model(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<ModelCard>> {
        typed_model(py, &self.inner, alias, &self.models)
    }
    /// Return the eagerly loaded Data holder selected by alias.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn data(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<DataCard>> {
        typed_data(py, &self.inner, alias, &self.data)
    }
    /// Return the hydrated Agent holder selected by alias.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn agent(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PyAgentCard>> {
        typed_agent(py, &self.inner, alias, &self.agents)
    }
    /// Return the hydrated Prompt holder selected by alias.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn prompt(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PromptCard>> {
        typed_prompt(py, &self.inner, alias, &self.prompts)
    }
    /// Return the persistent kind-checked Eval envelope without payload reads.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn eval(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PyCardEnvelope>> {
        envelope_inner(
            &self.inner,
            &self.envelopes,
            py,
            alias,
            Some(CardKind::Eval),
        )
    }
    /// Return the persistent kind-checked Drift envelope without payload reads.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn drift(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PyCardEnvelope>> {
        envelope_inner(
            &self.inner,
            &self.envelopes,
            py,
            alias,
            Some(CardKind::Drift),
        )
    }
    /// Return the persistent kind-checked Workflow envelope without payload reads.
    ///
    /// # Errors
    ///
    /// Returns unknown-alias, kind-mismatch, or invalid-state errors.
    fn workflow(&self, py: Python<'_>, alias: &str) -> CardPyResult<Py<PyCardEnvelope>> {
        envelope_inner(
            &self.inner,
            &self.envelopes,
            py,
            alias,
            Some(CardKind::Workflow),
        )
    }

    /// Visit every retained Python holder for cyclic garbage collection.
    // justification: pyo3 GC callbacks receive their visitor by value.
    #[allow(clippy::needless_pass_by_value)]
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        for value in self.envelopes.values() {
            visit.call(value)?;
        }
        for value in self.agents.values() {
            visit.call(value)?;
        }
        for value in self.prompts.values() {
            visit.call(value)?;
        }
        for value in self.models.values() {
            visit.call(value)?;
        }
        for value in self.data.values() {
            visit.call(value)?;
        }
        Ok(())
    }

    /// Break every retained Python reference before cyclic collection.
    fn __clear__(&mut self) {
        self.envelopes.clear();
        self.agents.clear();
        self.prompts.clear();
        self.models.clear();
        self.data.clear();
    }
}

/// Resolve an envelope by alias and optionally enforce its kind.
///
/// # Errors
///
/// Returns unknown-alias, kind-mismatch, or invalid-state errors.
fn envelope_inner(
    state: &WyrdState,
    envelopes: &BTreeMap<String, Py<PyCardEnvelope>>,
    py: Python<'_>,
    alias: &str,
    expected: Option<CardKind>,
) -> CardPyResult<Py<PyCardEnvelope>> {
    let reference = state.card_ref(alias)?;
    if let Some(expected) = expected {
        require_kind(alias, reference, &expected)?;
    }
    envelopes
        .get(&reference.to_string())
        .map(|v| v.clone_ref(py))
        .ok_or_else(|| {
            WyrdPyError::from(WyrdError::SdkInvalidStateBundle {
                message: "Card envelope is missing".to_owned(),
                details: serde_json::json!({"alias": alias, "card_ref": reference}),
            })
        })
}

/// Resolve the persistent Model holder for an alias.
///
/// # Errors
///
/// Returns unknown-alias, kind-mismatch, or missing-holder errors.
fn typed_model(
    py: Python<'_>,
    state: &WyrdState,
    alias: &str,
    values: &BTreeMap<String, Py<ModelCard>>,
) -> CardPyResult<Py<ModelCard>> {
    typed_holder_impl(py, state, alias, &CardKind::Model, values)
}
/// Resolve the persistent Data holder for an alias.
///
/// # Errors
///
/// Returns unknown-alias, kind-mismatch, or missing-holder errors.
fn typed_data(
    py: Python<'_>,
    state: &WyrdState,
    alias: &str,
    values: &BTreeMap<String, Py<DataCard>>,
) -> CardPyResult<Py<DataCard>> {
    typed_holder_impl(py, state, alias, &CardKind::Data, values)
}
/// Resolve the persistent Agent holder for an alias.
///
/// # Errors
///
/// Returns unknown-alias, kind-mismatch, or missing-holder errors.
fn typed_agent(
    py: Python<'_>,
    state: &WyrdState,
    alias: &str,
    values: &BTreeMap<String, Py<PyAgentCard>>,
) -> CardPyResult<Py<PyAgentCard>> {
    typed_holder_impl(py, state, alias, &CardKind::Agent, values)
}
/// Resolve the persistent Prompt holder for an alias.
///
/// # Errors
///
/// Returns unknown-alias, kind-mismatch, or missing-holder errors.
fn typed_prompt(
    py: Python<'_>,
    state: &WyrdState,
    alias: &str,
    values: &BTreeMap<String, Py<PromptCard>>,
) -> CardPyResult<Py<PromptCard>> {
    typed_holder_impl(py, state, alias, &CardKind::Prompt, values)
}

/// Look up a typed holder by exact `CardRef` after validating its kind.
///
/// # Errors
///
/// Returns unknown-alias, kind-mismatch, or invalid-state errors.
fn typed_holder_impl<T>(
    py: Python<'_>,
    state: &WyrdState,
    alias: &str,
    expected: &CardKind,
    values: &BTreeMap<String, Py<T>>,
) -> CardPyResult<Py<T>> {
    let reference = state.card_ref(alias)?;
    require_kind(alias, reference, expected)?;
    values
        .get(&reference.to_string())
        .map(|v| v.clone_ref(py))
        .ok_or_else(|| {
            WyrdPyError::from(WyrdError::SdkInvalidStateBundle {
                message: "typed Card holder is missing".to_owned(),
                details: serde_json::json!({"alias": alias, "card_ref": reference}),
            })
        })
}

#[pymethods]
impl PyCardEnvelope {
    /// Return the exact versioned Card identity.
    #[getter]
    fn card_ref(&self) -> CardRefPy {
        CardRefPy(self.card_ref.clone())
    }
    /// Return the persistent aliases for this Card in stable order.
    ///
    /// The tuple owns its Python strings while the envelope retains the
    /// authoritative alias list; this accessor performs no payload or IO read.
    ///
    /// # Errors
    ///
    /// Returns invalid-state when no alias exists, or a stable allocation
    /// error containing the exact alias and `CardRef`.
    #[getter]
    fn aliases(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let alias = self.aliases.first().ok_or_else(|| {
            WyrdPyError::from(WyrdError::SdkInvalidStateBundle {
                message: "Card has no stable alias".to_owned(),
                details: serde_json::json!({"card_ref": self.card_ref}),
            })
        })?;
        PyTuple::new(py, self.aliases.clone())
            .map(|tuple| tuple.unbind().into_any())
            .map_err(|_| WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                message: "Python envelope alias tuple allocation failed".to_owned(),
                details: serde_json::json!({"alias": alias, "card_ref": self.card_ref, "stage": "python_allocation", "reason": "alias tuple allocation failed"}),
            }))
    }
    /// Return the retained Card kind.
    #[getter]
    fn kind(&self) -> Kind {
        kind_from_card_kind(&self.card.kind)
    }
    /// Return owned JSON-compatible metadata from the retained envelope.
    ///
    /// # Errors
    ///
    /// Returns serialization or Python conversion errors; no network or payload
    /// read occurs.
    #[getter]
    fn metadata(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        json_to_py(py, &serde_json::to_value(&self.card.metadata)?)
    }
    /// Return owned JSON-compatible Card spec from the retained envelope.
    ///
    /// # Errors
    ///
    /// Returns serialization or Python conversion errors.
    #[getter]
    fn spec(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        json_to_py(py, &serde_json::to_value(&self.card.spec)?)
    }
    /// Return owned JSON-compatible server relationships from the envelope.
    ///
    /// # Errors
    ///
    /// Returns serialization or Python conversion errors.
    #[getter]
    fn relationships(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        json_to_py(py, &serde_json::to_value(&self.card.relationships)?)
    }
    /// Return owned JSON-compatible server status, if present.
    ///
    /// # Errors
    ///
    /// Returns serialization or Python conversion errors.
    #[getter]
    fn status(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        self.card
            .status
            .as_ref()
            .map(|value| json_to_py(py, &serde_json::to_value(value)?))
            .transpose()
    }
    /// Return the complete retained Card envelope as an owned mapping.
    ///
    /// # Errors
    ///
    /// Returns serialization or Python conversion errors.
    fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        json_to_py(py, &serde_json::to_value(&self.card)?)
    }
    /// Serialize the complete retained Card envelope as JSON.
    ///
    /// # Errors
    ///
    /// Returns the serializer error when the envelope cannot be represented.
    fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.card)?)
    }
}

#[pymethods]
impl PyHydratedArtifact {
    /// Return the manifest-relative artifact path.
    #[getter]
    fn relative_path(&self) -> &str {
        &self.relative_path
    }
    /// Return the absolute confined payload path.
    #[getter]
    fn local_path(&self) -> PathBuf {
        self.local_path.clone()
    }
    /// Return the verified base64-encoded SHA-256 digest.
    #[getter]
    fn sha256(&self) -> &str {
        &self.sha256
    }
    /// Return the verified payload length.
    #[getter]
    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
    /// Return the declared media type, when present.
    #[getter]
    fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

/// Convert validated JSON into a newly allocated Python object.
///
/// # Errors
///
/// Returns a stable Python conversion error when allocation or conversion fails.
fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> CardPyResult<Py<PyAny>> {
    wyrd_utils::py::json_to_pyobject(py, value).map_err(Into::into)
}

/// Enforce that an alias resolves to the expected Card kind.
///
/// # Errors
///
/// Returns a stable Card-kind mismatch error when kinds differ.
fn require_kind(alias: &str, card_ref: &CardRef, expected: &CardKind) -> CardPyResult<()> {
    if &card_ref.kind != expected {
        return Err(WyrdPyError::from(WyrdError::SdkCardKindMismatch {
            message: format!("alias `{alias}` is not a {} Card", expected.wire_name()),
            details: serde_json::json!({"alias": alias, "expected_kind": expected.wire_name(), "actual_kind": card_ref.kind.wire_name(), "card_ref": card_ref}),
        }));
    }
    Ok(())
}

/// Extract string-keyed entries from a Python mapping without retaining borrows.
///
/// # Errors
///
/// Returns Python extraction or iteration errors for malformed mappings.
fn mapping_items<'py>(
    mapping: &'py Bound<'py, PyMapping>,
) -> CardPyResult<Vec<(String, Bound<'py, PyAny>)>> {
    let items = mapping.call_method0("items")?;
    items
        .try_iter()?
        .map(|item| {
            let item = item?;
            let tuple = item.cast::<PyTuple>()?;
            Ok((tuple.get_item(0)?.extract()?, tuple.get_item(1)?))
        })
        .collect::<Result<Vec<_>, PyErr>>()
        .map_err(Into::into)
}

/// Normalize loader mappings by exact `CardRef` identity and reject conflicts.
///
/// # Errors
///
/// Returns mapping, alias, kind, conversion, or conflicting-alias errors.
fn parse_load_config(
    py: Python<'_>,
    state: &WyrdState,
    interfaces: Option<&Bound<'_, PyMapping>>,
    load_kwargs: Option<&Bound<'_, PyMapping>>,
    trusted_artifact_hashes: Option<&Bound<'_, PyMapping>>,
) -> CardPyResult<PythonLoadConfig> {
    let mut config = PythonLoadConfig {
        interfaces: BTreeMap::new(),
        load_kwargs: BTreeMap::new(),
        trusted_artifact_hashes: BTreeMap::new(),
    };
    if let Some(mapping) = interfaces {
        for (alias, value) in mapping_items(mapping)? {
            let reference = state.card_ref(&alias)?;
            require_model_data(&alias, reference)?;
            let key = reference.to_string();
            if let Some(existing) = config.interfaces.get(&key)
                && !existing.bind(py).is(&value)
            {
                return Err(WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                    message: "conflicting interface overrides for one Card".to_owned(),
                    details: serde_json::json!({"alias": alias, "card_ref": reference, "stage": "interface", "reason": "aliases resolving to one Card must use the identical interface object"}),
                }));
            }
            config.interfaces.insert(key, value.unbind());
        }
    }
    if let Some(mapping) = load_kwargs {
        for (alias, value) in mapping_items(mapping)? {
            let reference = state.card_ref(&alias)?;
            require_model_data(&alias, reference)?;
            let normalized = normalize_load_kwargs_value(py, state, &alias, reference, &value)?;
            let normalized_json = wyrd_utils::py::pyobject_to_json(normalized.bind(py))?;
            let key = reference.to_string();
            if let Some(existing) = config.load_kwargs.get(&key) {
                let existing_json = wyrd_utils::py::pyobject_to_json(existing.bind(py))?;
                if existing_json != normalized_json {
                    return Err(WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                        message: "conflicting loader kwargs for one Card".to_owned(),
                        details: serde_json::json!({"alias": alias, "card_ref": reference, "stage": "interface", "reason": "aliases resolving to one Card must use equivalent loader kwargs"}),
                    }));
                }
            }
            config.load_kwargs.insert(key, normalized.into_any());
        }
    }
    if let Some(mapping) = trusted_artifact_hashes {
        for (alias, value) in mapping_items(mapping)? {
            let reference = state.card_ref(&alias)?;
            require_model_data(&alias, reference)?;
            let expected = value.extract::<String>().map_err(|_| {
                runtime_hydration_error(
                    state,
                    &reference.to_string(),
                    "artifact_trust",
                    "trusted artifact manifest hash must be a string",
                )
            })?;
            let key = reference.to_string();
            if let Some(existing) = config.trusted_artifact_hashes.get(&key)
                && existing != &expected
            {
                return Err(runtime_hydration_error(
                    state,
                    &key,
                    "artifact_trust",
                    "aliases resolving to one Card must use the identical trusted artifact manifest hash",
                ));
            }
            config.trusted_artifact_hashes.insert(key, expected);
        }
    }
    Ok(config)
}

/// Normalize one loader argument value to a real Python dictionary.
///
/// Typed `ModelLoadArgs` and `DataLoadArgs` use their `to_dict` methods,
/// ordinary mapping implementations are materialized through Python's
/// `dict` constructor, and dictionaries are retained directly. Unsupported
/// values are rejected with the stable runtime-hydration error before any
/// holder loading begins.
///
/// # Errors
///
/// Returns a stable runtime-hydration error when the value is not a supported
/// loader-argument shape or cannot be converted to a dictionary; Python
/// conversion errors are intentionally not exposed as traceback text.
fn normalize_load_kwargs_value<'py>(
    py: Python<'py>,
    state: &WyrdState,
    alias: &str,
    reference: &CardRef,
    value: &Bound<'py, PyAny>,
) -> CardPyResult<Py<PyDict>> {
    let conversion_error = || {
        runtime_hydration_error(
            state,
            &reference.to_string(),
            "interface",
            "loader kwargs conversion failed",
        )
    };
    let converted = if value.is_instance_of::<PyDict>() {
        value
            .cast::<PyDict>()
            .cloned()
            .map(Bound::into_any)
            .map_err(|_| conversion_error())?
    } else if value.is_instance_of::<PyModelLoadArgs>() || value.is_instance_of::<PyDataLoadArgs>()
    {
        value
            .call_method0("to_dict")
            .map_err(|_| conversion_error())?
    } else if value.is_instance_of::<PyMapping>() {
        py.import("builtins")
            .and_then(|builtins| builtins.getattr("dict"))
            .and_then(|dict| dict.call1((value,)))
            .map_err(|_| conversion_error())?
    } else {
        return Err(WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
            message: "unsupported loader kwargs value".to_owned(),
            details: serde_json::json!({
                "alias": alias,
                "card_ref": reference,
                "stage": "interface",
                "reason": "loader kwargs must be a dict, ModelLoadArgs, DataLoadArgs, or Mapping",
            }),
        }));
    };

    converted
        .cast::<PyDict>()
        .cloned()
        .map(Bound::unbind)
        .map_err(|_| conversion_error())
}

/// Require a loader override to target a Model or Data Card.
///
/// # Errors
///
/// Returns a stable Card-kind mismatch error for other kinds.
fn require_model_data(alias: &str, reference: &CardRef) -> CardPyResult<()> {
    if !matches!(reference.kind, CardKind::Model | CardKind::Data) {
        return Err(WyrdPyError::from(WyrdError::SdkCardKindMismatch {
            message: format!("loader alias `{alias}` must select Model or Data"),
            details: serde_json::json!({"alias": alias, "card_ref": reference, "expected_kind": "Model|Data", "actual_kind": reference.kind.wire_name()}),
        }));
    }
    Ok(())
}

/// Build the redacted runtime-hydration error for one validated exact `CardRef`.
///
/// `key` must identify an indexed Card and `reason` is deliberately a static,
/// caller-selected phrase; arbitrary Python exceptions are never stringified.
/// The first persisted alias is used for stable diagnostics.
///
fn runtime_hydration_error(
    state: &WyrdState,
    key: &str,
    stage_name: &str,
    reason: &'static str,
) -> WyrdPyError {
    let card_ref = match state.card_ref_by_key(key) {
        Ok(card_ref) => card_ref.clone(),
        Err(error) => return WyrdPyError::from(error),
    };
    let alias = match primary_alias(state, key) {
        Ok(alias) => alias,
        Err(error) => return error,
    };
    WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
        message: format!("runtime hydration failed at {stage_name}"),
        details: serde_json::json!({"alias": alias, "card_ref": card_ref, "stage": stage_name, "reason": reason}),
    })
}

/// Build a runtime-hydration error while retaining a safe textual Python cause.
///
/// The stable Wyrd details remain unchanged; the cause is attached only when
/// the error crosses back into Python, so Rust state and logs never retain a
/// Python-owned exception.
fn runtime_hydration_error_with_cause(
    state: &WyrdState,
    key: &str,
    stage_name: &str,
    reason: &'static str,
    cause: impl Into<String>,
) -> WyrdPyError {
    let base = runtime_hydration_error(state, key, stage_name, reason);
    let cause = cause.into();
    match base {
        WyrdPyError::Spec(error) => WyrdPyError::spec_with_cause(error, cause),
        other => other,
    }
}

/// Return the first persisted alias for a validated exact `CardRef`.
///
/// Aliases are already sorted during native bundle assembly, so this is a
/// deterministic identity label and performs no filesystem or registry work.
///
/// # Errors
///
/// Returns an invalid-state error when the key is absent or has no aliases.
fn primary_alias<'a>(state: &'a WyrdState, key: &str) -> CardPyResult<&'a str> {
    state
        .aliases_by_key(key)?
        .first()
        .map(String::as_str)
        .ok_or_else(|| {
            WyrdPyError::from(WyrdError::SdkInvalidStateBundle {
                message: "Card has no stable primary alias".to_owned(),
                details: serde_json::json!({"card_ref": key}),
            })
        })
}

/// Report missing verified artifacts without touching the filesystem.
///
/// The returned stable error includes the exact primary alias, `card_ref`,
/// `stage: artifact_load`, and a safe redacted reason.
///
/// # Errors
///
/// This helper returns the stable invalid-state error if the validated Card
/// key or primary alias is absent; it performs no filesystem read.
fn missing_runtime_artifacts(state: &WyrdState, key: &str) -> WyrdPyError {
    runtime_hydration_error(
        state,
        key,
        "artifact_load",
        "verified artifact directory is missing",
    )
}

/// Allocate one immutable Python envelope for each exact `CardRef`.
///
/// # Errors
///
/// Returns invalid-state or Python-allocation errors.
impl PythonStateHydrator<'_> {
    /// Allocate immutable envelope projections for every exact `CardRef`.
    ///
    /// # Errors
    ///
    /// Returns invalid-state or Python-allocation errors.
    fn hydrate_envelopes(
        &self,
        py: Python<'_>,
    ) -> CardPyResult<BTreeMap<String, Py<PyCardEnvelope>>> {
        let state = self.state;
        let mut values = BTreeMap::new();
        for (key, card) in state.cards() {
            let card_ref = state.card_ref_by_key(key)?.clone();
            let aliases = state.aliases_by_key(key)?.to_vec();
            if aliases.is_empty() {
                return Err(WyrdPyError::from(WyrdError::SdkInvalidStateBundle {
                    message: "Card has no stable alias".to_owned(),
                    details: serde_json::json!({"card_ref": card_ref}),
                }));
            }
            values.insert(
                key.to_owned(),
                Py::new(
                    py,
                    PyCardEnvelope {
                        card: card.clone(),
                        card_ref,
                        aliases,
                    },
                )
                .map_err(|_| {
                    runtime_hydration_error(
                        state,
                        key,
                        "python_allocation",
                        "Python envelope allocation failed",
                    )
                })?,
            );
        }
        Ok(values)
    }
}

/// Construct and hydrate each exact Prompt Card once.
///
/// # Errors
///
/// Returns prompt conversion, hydration, or Python-allocation errors.
impl PythonStateHydrator<'_> {
    /// Construct and hydrate every exact Prompt holder.
    ///
    /// # Errors
    ///
    /// Returns prompt conversion, hydration, or allocation errors.
    fn hydrate_prompts(&self, py: Python<'_>) -> CardPyResult<BTreeMap<String, Py<PromptCard>>> {
        let state = self.state;
        let mut values = BTreeMap::new();
        for (key, envelope) in state.cards_of_kind(CardKind::Prompt) {
            let mut card = PromptCard::from_card(envelope.clone()).map_err(|_| {
                runtime_hydration_error(state, key, "prompt", "prompt holder construction failed")
            })?;
            card.hydrate_prompt(py).map_err(|_| {
                runtime_hydration_error(state, key, "prompt", "prompt hydration failed")
            })?;
            values.insert(
                key.to_owned(),
                Py::new(py, card).map_err(|_| {
                    runtime_hydration_error(
                        state,
                        key,
                        "python_allocation",
                        "Python holder allocation failed",
                    )
                })?,
            );
        }
        Ok(values)
    }
}

/// Construct each exact Agent Card and resolve only referenced prompts.
/// Inline prompt payloads remain untouched.
///
/// # Errors
///
/// Returns agent conversion, prompt hydration, or Python-allocation errors.
impl PythonStateHydrator<'_> {
    /// Construct and hydrate every exact Agent holder.
    ///
    /// # Errors
    ///
    /// Returns agent conversion, prompt resolution, hydration, or allocation errors.
    fn hydrate_agents(&self, py: Python<'_>) -> CardPyResult<BTreeMap<String, Py<PyAgentCard>>> {
        let state = self.state;
        let mut values = BTreeMap::new();
        for (key, envelope) in state.cards_of_kind(CardKind::Agent) {
            let mut card = PyAgentCard::from_card(py, envelope.clone()).map_err(|_| {
                runtime_hydration_error(state, key, "prompt", "agent holder construction failed")
            })?;
            let alias = primary_alias(state, key)?;
            if !matches!(card.native().spec.prompt, InlineableRef::Inline(_)) {
                let prompt = state.agent_prompt(alias).map_err(|_| {
                    runtime_hydration_error(state, key, "prompt", "agent prompt resolution failed")
                })?;
                card.hydrate_resolved_prompt(py, prompt.clone())
                    .map_err(|_| {
                        runtime_hydration_error(
                            state,
                            key,
                            "prompt",
                            "agent prompt hydration failed",
                        )
                    })?;
            }
            values.insert(
                key.to_owned(),
                Py::new(py, card).map_err(|_| {
                    runtime_hydration_error(
                        state,
                        key,
                        "python_allocation",
                        "Python holder allocation failed",
                    )
                })?,
            );
        }
        Ok(values)
    }
}

/// Construct and eagerly load each exact Model Card from keyed artifacts.
///
/// # Errors
///
/// Returns interface, artifact, loader, state, or Python-allocation errors.
impl PythonStateHydrator<'_> {
    /// Construct and eagerly load every exact Model holder.
    ///
    /// # Errors
    ///
    /// Returns interface, artifact, loader, state, or Python-allocation errors.
    fn hydrate_models(&self, py: Python<'_>) -> CardPyResult<BTreeMap<String, Py<ModelCard>>> {
        let state = self.state;
        let config = &self.config;
        let mut values = BTreeMap::new();
        for (key, envelope) in state.cards_of_kind(CardKind::Model) {
            let mut card = ModelCard::from_card(envelope.clone()).map_err(|_| {
                runtime_hydration_error(state, key, "interface", "model holder construction failed")
            })?;
            let interface = config.interfaces.get(key).map(|value| value.bind(py));
            card.hydrate_interface(py, interface).map_err(|_| {
                runtime_hydration_error(state, key, "interface", "model interface hydration failed")
            })?;
            let artifact_dir = state
                .artifact_dir_by_key(key)?
                .map(PathBuf::from)
                .ok_or_else(|| missing_runtime_artifacts(state, key))?;
            card.load(
                py,
                Some(artifact_dir),
                config.load_kwargs.get(key).map(|value| value.bind(py)),
            )
            .map_err(|error| {
                runtime_hydration_error_with_cause(
                    state,
                    key,
                    "artifact_load",
                    "model artifact load failed",
                    error.to_string(),
                )
            })?;
            values.insert(
                key.to_owned(),
                Py::new(py, card).map_err(|_| {
                    runtime_hydration_error(
                        state,
                        key,
                        "python_allocation",
                        "Python holder allocation failed",
                    )
                })?,
            );
        }
        Ok(values)
    }
}

/// Construct and eagerly load each exact Data Card from keyed artifacts.
///
/// # Errors
///
/// Returns interface, artifact, loader, state, or Python-allocation errors.
impl PythonStateHydrator<'_> {
    /// Construct and eagerly load every exact Data holder.
    ///
    /// # Errors
    ///
    /// Returns interface, artifact, loader, state, or Python-allocation errors.
    fn hydrate_data(&self, py: Python<'_>) -> CardPyResult<BTreeMap<String, Py<DataCard>>> {
        let state = self.state;
        let config = &self.config;
        let mut values = BTreeMap::new();
        for (key, envelope) in state.cards_of_kind(CardKind::Data) {
            let mut card = DataCard::from_card(envelope.clone()).map_err(|_| {
                runtime_hydration_error(state, key, "interface", "data holder construction failed")
            })?;
            let interface = config.interfaces.get(key).map(|value| value.bind(py));
            card.hydrate_interface(py, interface).map_err(|_| {
                runtime_hydration_error(state, key, "interface", "data interface hydration failed")
            })?;
            let artifact_dir = state
                .artifact_dir_by_key(key)?
                .map(PathBuf::from)
                .ok_or_else(|| missing_runtime_artifacts(state, key))?;
            card.load(
                py,
                Some(artifact_dir),
                config.load_kwargs.get(key).map(|value| value.bind(py)),
            )
            .map_err(|error| {
                runtime_hydration_error_with_cause(
                    state,
                    key,
                    "artifact_load",
                    "data artifact load failed",
                    error.to_string(),
                )
            })?;
            values.insert(
                key.to_owned(),
                Py::new(py, card).map_err(|_| {
                    runtime_hydration_error(
                        state,
                        key,
                        "python_allocation",
                        "Python holder allocation failed",
                    )
                })?,
            );
        }
        Ok(values)
    }
}

/// Python version bump used by registration.
#[pyclass(module = "wyrd.cards", name = "VersionBump", eq, from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyVersionBump {
    native: VersionBump,
}

impl PyVersionBump {
    fn as_native(&self) -> VersionBump {
        self.native.clone()
    }
}

#[pymethods]
impl PyVersionBump {
    /// Major version bump constant.
    #[classattr]
    #[pyo3(name = "Major")]
    fn major() -> Self {
        Self {
            native: VersionBump::Major,
        }
    }

    /// Minor version bump constant.
    #[classattr]
    #[pyo3(name = "Minor")]
    fn minor() -> Self {
        Self {
            native: VersionBump::Minor,
        }
    }

    /// Patch version bump constant.
    #[classattr]
    #[pyo3(name = "Patch")]
    fn patch() -> Self {
        Self {
            native: VersionBump::Patch,
        }
    }

    /// Create a pre-release bump.
    ///
    /// The identifier is appended as the semantic-version pre-release token.
    #[staticmethod]
    fn pre(identifier: String) -> Self {
        Self {
            native: VersionBump::Pre { identifier },
        }
    }

    /// Create a build-metadata bump.
    ///
    /// The metadata is appended as the semantic-version build token.
    #[staticmethod]
    fn build(metadata: String) -> Self {
        Self {
            native: VersionBump::Build { metadata },
        }
    }

    /// Create a pre-release and build-metadata bump.
    #[staticmethod]
    fn pre_build(pre: String, build: String) -> Self {
        Self {
            native: VersionBump::PreBuild { pre, build },
        }
    }
}

/// Typed Python options passed to `DataCard` interface `save` calls.
#[pyclass(module = "wyrd.cards", name = "DataSaveArgs")]
pub struct PyDataSaveArgs {
    values: Py<PyDict>,
}

/// Typed Python options passed to `ModelCard` interface `save` calls.
#[pyclass(module = "wyrd.cards", name = "ModelSaveArgs")]
pub struct PyModelSaveArgs {
    values: Py<PyDict>,
}

/// Typed Python options passed to `DataCard` interface `load` calls.
#[pyclass(module = "wyrd.cards", name = "DataLoadArgs")]
pub struct PyDataLoadArgs {
    values: Py<PyDict>,
}

/// Typed Python options passed to `ModelCard` interface `load` calls.
#[pyclass(module = "wyrd.cards", name = "ModelLoadArgs")]
pub struct PyModelLoadArgs {
    values: Py<PyDict>,
}

fn option_values(
    py: Python<'_>,
    values: Option<&Bound<'_, PyAny>>,
    boolean: Option<(&str, bool)>,
) -> CardPyResult<Py<PyDict>> {
    let result = PyDict::new(py);
    if let Some(values) = values {
        let values = if values.is_instance_of::<PyDict>() {
            values.clone()
        } else {
            py.import("builtins")?.getattr("dict")?.call1((values,))?
        };
        let values = values.cast::<PyDict>()?;
        for (key, value) in values.iter() {
            result.set_item(key, value)?;
        }
    }
    if let Some((key, value)) = boolean {
        result.set_item(key, value)?;
    }
    Ok(result.unbind())
}

macro_rules! option_methods {
    ($type:ty) => {
        #[pymethods]
        impl $type {
            /// Create typed interface options from JSON-compatible values.
            #[new]
            #[pyo3(signature = (values=None))]
            fn __new__(py: Python<'_>, values: Option<&Bound<'_, PyAny>>) -> CardPyResult<Self> {
                Ok(Self {
                    values: option_values(py, values, None)?,
                })
            }

            /// Return the options as a Python dictionary for interface calls.
            fn to_dict(&self, py: Python<'_>) -> Py<PyDict> {
                self.values.clone_ref(py)
            }
        }
    };
}

option_methods!(PyModelSaveArgs);
option_methods!(PyDataLoadArgs);
option_methods!(PyModelLoadArgs);

#[pymethods]
impl PyDataSaveArgs {
    /// Create data-interface save options.
    ///
    /// `copy_bytes` is copied into the options dictionary when provided.
    #[new]
    #[pyo3(signature = (values=None, copy_bytes=None))]
    fn __new__(
        py: Python<'_>,
        values: Option<&Bound<'_, PyAny>>,
        copy_bytes: Option<bool>,
    ) -> CardPyResult<Self> {
        Ok(Self {
            values: option_values(py, values, copy_bytes.map(|value| ("copy_bytes", value)))?,
        })
    }

    /// Return the options as a Python dictionary for the data interface.
    fn to_dict(&self, py: Python<'_>) -> Py<PyDict> {
        self.values.clone_ref(py)
    }
}

/// A metadata-only Card list item.
#[pyclass(module = "wyrd.cards", name = "CardSummary")]
pub struct PyCardSummary {
    inner: CardSummary,
}

#[pymethods]
impl PyCardSummary {
    /// Card UID.
    #[getter]
    fn uid(&self) -> String {
        self.inner.card_uid.to_string()
    }

    /// Card kind.
    #[getter]
    fn kind(&self) -> Kind {
        kind_from_card_kind(&self.inner.kind)
    }

    /// Card space.
    #[getter]
    fn space(&self) -> String {
        self.inner.space.to_string()
    }

    /// Card name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.to_string()
    }

    /// Exact resolved version.
    #[getter]
    fn version(&self) -> String {
        self.inner.version.to_string()
    }

    /// BLAKE3 specification hash.
    #[getter]
    fn spec_hash(&self) -> &str {
        &self.inner.spec_hash
    }

    /// Optional BLAKE3 artifact manifest hash.
    #[getter]
    fn artifact_hash(&self) -> Option<&str> {
        self.inner.artifact_hash.as_deref()
    }

    /// Server lifecycle status.
    #[getter]
    fn status(&self) -> &'static str {
        lifecycle_status_name(self.inner.status)
    }

    /// Creation timestamp in RFC 3339 form.
    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    /// Last update timestamp in RFC 3339 form.
    #[getter]
    fn updated_at(&self) -> String {
        self.inner.updated_at.to_rfc3339()
    }

    /// Exact Card reference represented by this summary.
    #[getter]
    fn card_ref(&self) -> CardRefPy {
        CardRefPy(card_ref_from_summary(&self.inner))
    }
}

/// Cursor-paginated metadata-only Card list.
#[pyclass(module = "wyrd.cards", name = "CardList")]
pub struct PyCardList {
    items: Vec<PyCardSummary>,
    refs: Vec<CardRefPy>,
    next_cursor: Option<String>,
}

#[pymethods]
impl PyCardList {
    /// Metadata-only Card summaries in this page.
    #[getter]
    fn items(&self) -> Vec<PyCardSummary> {
        self.items
            .iter()
            .map(|item| PyCardSummary {
                inner: item.inner.clone(),
            })
            .collect()
    }

    /// Exact references in this page.
    #[getter]
    fn refs(&self) -> Vec<CardRefPy> {
        self.refs.clone()
    }

    /// Opaque cursor for the next page.
    #[getter]
    fn next_cursor(&self) -> Option<String> {
        self.next_cursor.clone()
    }
}

/// One dependency-first registration outcome.
#[pyclass(module = "wyrd.cards", name = "RegistrationOutcome")]
pub struct PyRegistrationOutcome {
    inner: CardRegistrationOutcome,
}

#[pymethods]
impl PyRegistrationOutcome {
    /// Server-resolved identity.
    #[getter]
    fn card_ref(&self) -> CardRefPy {
        CardRefPy(self.inner.card_ref.clone())
    }

    /// Resolved specification hash.
    #[getter]
    fn spec_hash(&self) -> &str {
        &self.inner.spec_hash
    }

    /// Resolved artifact manifest hash.
    #[getter]
    fn artifact_hash(&self) -> Option<&str> {
        self.inner.artifact_hash.as_deref()
    }

    /// Final lifecycle status.
    #[getter]
    fn status(&self) -> &'static str {
        lifecycle_status_name(self.inner.status)
    }

    /// Registration result.
    #[getter]
    fn outcome(&self) -> &'static str {
        registration_outcome_name(self.inner.outcome)
    }

    /// Durable Card blob URI when one was written.
    #[getter]
    fn card_blob_uri(&self) -> Option<String> {
        self.inner.card_blob_uri.as_ref().map(ToString::to_string)
    }
}

/// Public registration result returned after upload and completion succeed.
#[pyclass(module = "wyrd.cards", name = "RegistrationReceipt")]
pub struct PyRegistrationReceipt {
    inner: RegistrationReceipt,
}

impl From<RegistrationReceipt> for PyRegistrationReceipt {
    fn from(inner: RegistrationReceipt) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyRegistrationReceipt {
    /// Server-resolved graph root.
    #[getter]
    fn root(&self) -> CardRefPy {
        CardRefPy(self.inner.root.clone())
    }

    /// Dependency-first registration outcomes.
    #[getter]
    fn outcomes(&self) -> Vec<PyRegistrationOutcome> {
        self.inner
            .outcomes
            .iter()
            .cloned()
            .map(|inner| PyRegistrationOutcome { inner })
            .collect()
    }
}

/// Tenant-scoped client for registered Wyrd Cards.
///
/// `Cards` owns the connection and authentication context used by registry
/// operations. Use the kind-specific properties for normal Python calls:
///
/// ```python
/// cards = Cards()
/// reference = cards.model.resolve_latest(space="ml", name="fraud-model")
/// model = cards.model.get(uid=reference.uid, eager_load=True)
/// ```
///
/// `get` retrieves and validates the serialized Card envelope. Pass
/// `eager_load=True` to download verified model or data artifacts and hydrate
/// the holder before it is returned.
#[pyclass(module = "wyrd.cards", name = "Cards")]
pub struct PyCards {
    inner: Cards,
}

#[pymethods]
impl PyCards {
    /// Construct a tenant-scoped Card client.
    ///
    /// When `server_url` or `api_key` is omitted, the shared Wyrd client
    /// configuration supplies the value. The handle is cheap to clone and the
    /// kind-specific properties retain this same connection context.
    ///
    /// # Arguments
    /// * `server_url` - Optional Wyrd server URL override.
    /// * `api_key` - Optional API key override.
    ///
    /// # Errors
    /// Returns a Wyrd error when local configuration or the API-key override
    /// cannot be loaded.
    #[new]
    #[pyo3(signature = (server_url=None, api_key=None))]
    // justification: pyo3 boundary; Python callers provide owned optional strings and api_key is consumed into SecretString
    #[allow(clippy::needless_pass_by_value)]
    fn __new__(server_url: Option<String>, api_key: Option<String>) -> CardPyResult<Self> {
        Cards::new(server_url.as_deref(), api_key.map(SecretString::from))
            .map(|inner| Self { inner })
            .map_err(WyrdPyError::from)
    }

    /// Return the typed view for `DataCard` operations.
    ///
    /// Use `cards.data.get`, `cards.data.register`, `cards.data.list`,
    /// `cards.data.resolve_latest`, and `cards.data.delete`. The view uses the
    /// parent client's connection and tenant context.
    #[getter]
    fn data(&self) -> PyDataCardRegistry {
        PyDataCardRegistry {
            inner: self.inner.clone(),
        }
    }

    /// Return the typed view for `ModelCard` operations.
    ///
    /// Use `cards.model.get`, `cards.model.register`, `cards.model.list`,
    /// `cards.model.resolve_latest`, and `cards.model.delete`. The view uses
    /// the parent client's connection and tenant context.
    #[getter]
    fn model(&self) -> PyModelCardRegistry {
        PyModelCardRegistry {
            inner: self.inner.clone(),
        }
    }

    /// Return the typed view for `PromptCard` operations.
    ///
    /// Use `cards.prompt.get`, `cards.prompt.register`, `cards.prompt.list`,
    /// `cards.prompt.resolve_latest`, and `cards.prompt.delete`. Prompt cards
    /// have no separate artifact hydration step.
    #[getter]
    fn prompt(&self) -> PyPromptCardRegistry {
        PyPromptCardRegistry {
            inner: self.inner.clone(),
        }
    }

    /// Register a caller-owned `DataCard`, `ModelCard`, or `PromptCard`.
    ///
    /// Registration serializes the complete Card envelope, saves `DataCard` or
    /// `ModelCard` artifacts through the attached interface, uploads the
    /// resulting manifest, and waits for server completion. On success, the
    /// same caller-owned card is updated with the server-assigned UID,
    /// resolved version, and normalized identity fields.
    ///
    /// # Arguments
    /// * `card` - Native Wyrd card holder to register.
    /// * `version_bump` - Version intent. Defaults to server-compatible `VersionBump.Patch`.
    ///   Use `VersionBump.pre`, `build`, or `pre_build` for version metadata.
    /// * `save_args` - `DataSaveArgs` or `ModelSaveArgs` forwarded to the
    ///   corresponding artifact interface. Prompt cards do not accept it.
    ///
    /// # Returns
    /// A receipt containing the resolved root reference and registration
    /// outcomes.
    ///
    /// # Errors
    /// Returns a Wyrd error when the holder is unsupported, required interface
    /// data is missing, serialization or artifact saving fails, the server
    /// rejects the registration, or completion fails.
    #[pyo3(signature = (card, version_bump=None, save_args=None))]
    fn register(
        &self,
        py: Python<'_>,
        card: &Bound<'_, PyAny>,
        version_bump: Option<&Bound<'_, PyAny>>,
        save_args: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<PyRegistrationReceipt> {
        PythonCardRegistry::new(&self.inner).register(py, card, version_bump, save_args)
    }

    /// Register a declarative Card bundle from a local path.
    ///
    /// Use `register` for Python-authored `DataCard`, `ModelCard`, and
    /// `PromptCard` objects. This method is for file-based bundles that the
    /// shared loader can validate and materialize.
    ///
    /// # Arguments
    /// * `path` - Directory containing the declarative Card bundle.
    ///
    /// # Returns
    /// A receipt for the completed registration.
    ///
    /// # Errors
    /// Returns a Wyrd error when the bundle is invalid, its artifacts cannot
    /// be read, or registration does not complete.
    #[pyo3(signature = (path))]
    // justification: pyo3 boundary; the extractor produces an owned PathBuf, and the path is moved into the detached filesystem/network operation
    #[allow(clippy::needless_pass_by_value)]
    fn register_from_path(
        &self,
        py: Python<'_>,
        path: PathBuf,
    ) -> CardPyResult<PyRegistrationReceipt> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.inner.register_from_path(&path)))
            .map(PyRegistrationReceipt::from)
            .map_err(WyrdPyError::from)
    }
}

/// Typed operations for registered `DataCard` objects.
///
/// Obtain this view from `Cards.data`. It uses the connection and tenant
/// context from the parent `Cards` object. Callers normally do not construct
/// this type directly.
#[pyclass(module = "wyrd.cards", name = "DataCardRegistry")]
pub struct PyDataCardRegistry {
    inner: Cards,
}

/// Typed operations for registered `ModelCard` objects.
///
/// Obtain this view from `Cards.model`. It uses the connection and tenant
/// context from the parent `Cards` object. Callers normally do not construct
/// this type directly.
#[pyclass(module = "wyrd.cards", name = "ModelCardRegistry")]
pub struct PyModelCardRegistry {
    inner: Cards,
}

/// Typed operations for registered `PromptCard` objects.
///
/// Obtain this view from `Cards.prompt`. Prompt cards have no artifact save or
/// load options. Callers normally do not construct this type directly.
#[pyclass(module = "wyrd.cards", name = "PromptCardRegistry")]
pub struct PyPromptCardRegistry {
    inner: Cards,
}

/// Owned typed arguments shared by Data, Model, and Prompt registry lists.
///
/// `PyO3` collects keyword-only list arguments into one dictionary. This
/// value validates the accepted keyword set and releases every dictionary
/// borrow before registry IO begins.
struct RegistryListQuery {
    /// Optional Card space filter.
    space: Option<String>,
    /// Optional Card name filter.
    name: Option<String>,
    /// Optional semantic-version range filter.
    version_range: Option<String>,
    /// Optional Card lifecycle status filter.
    status: Option<String>,
    /// Optional metadata query expression.
    filter: Option<String>,
    /// Whether prerelease versions participate in the result.
    include_prerelease: bool,
    /// Optional page-size limit.
    limit: Option<i32>,
    /// Optional opaque continuation cursor.
    cursor: Option<String>,
}

/// Normalizes the public Python list boundary before registry access.
impl RegistryListQuery {
    /// Parse and validate one Python registry `list` keyword mapping.
    ///
    /// # Errors
    ///
    /// Returns a Python type error for unknown keywords or values that do not
    /// match the public string, boolean, or integer shapes.
    fn from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<Self> {
        if let Some(kwargs) = kwargs {
            for (key, _) in kwargs {
                let key = key.extract::<String>()?;
                if !matches!(
                    key.as_str(),
                    "space"
                        | "name"
                        | "version_range"
                        | "status"
                        | "filter"
                        | "include_prerelease"
                        | "limit"
                        | "cursor"
                ) {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "list() got an unexpected keyword argument '{key}'"
                    ))
                    .into());
                }
            }
        }
        Ok(Self {
            space: RegistryGetQuery::optional_string(kwargs, "space")?,
            name: RegistryGetQuery::optional_string(kwargs, "name")?,
            version_range: RegistryGetQuery::optional_string(kwargs, "version_range")?,
            status: RegistryGetQuery::optional_string(kwargs, "status")?,
            filter: RegistryGetQuery::optional_string(kwargs, "filter")?,
            include_prerelease: RegistryGetQuery::optional_bool(kwargs, "include_prerelease")?
                .unwrap_or(false),
            limit: Self::optional_i32(kwargs, "limit")?,
            cursor: RegistryGetQuery::optional_string(kwargs, "cursor")?,
        })
    }

    /// Extract an optional 32-bit integer keyword.
    ///
    /// # Errors
    ///
    /// Returns a Python extraction error when a present non-`None` value is
    /// not representable as an `i32`.
    fn optional_i32(kwargs: Option<&Bound<'_, PyDict>>, name: &str) -> CardPyResult<Option<i32>> {
        let Some(value) = kwargs
            .map(|values| values.get_item(name))
            .transpose()?
            .flatten()
        else {
            return Ok(None);
        };
        if value.is_none() {
            return Ok(None);
        }
        value.extract().map(Some).map_err(Into::into)
    }
}

/// Owned typed arguments shared by Data and Model registry lookups.
///
/// `PyO3` collects keyword-only arguments into one dictionary so the public
/// bindings can preserve their stable call shape without repeated wide Rust
/// signatures. This value validates the accepted keyword set before registry
/// IO begins.
struct RegistryGetQuery {
    /// Exact server-assigned Card UID selector.
    uid: Option<String>,
    /// Card space used by a named selector.
    space: Option<String>,
    /// Card name used by a named selector.
    name: Option<String>,
    /// Exact version used by a named selector.
    version: Option<String>,
    /// Optional caller-supplied Python interface.
    interface: Option<Py<PyAny>>,
    /// Whether verified artifacts must load before return.
    eager_load: bool,
    /// Optional typed or mapping loader keyword arguments.
    load_kwargs: Option<Py<PyAny>>,
}

/// Normalizes the public Python get boundary before registry access.
impl RegistryGetQuery {
    /// Parse and validate one Python registry `get` keyword mapping.
    ///
    /// # Errors
    ///
    /// Returns a Python type error for unknown keywords or values that do not
    /// match the public string, boolean, or object shapes.
    fn from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        if let Some(kwargs) = kwargs {
            for (key, _) in kwargs {
                let key = key.extract::<String>()?;
                if !matches!(
                    key.as_str(),
                    "uid"
                        | "space"
                        | "name"
                        | "version"
                        | "interface"
                        | "eager_load"
                        | "load_kwargs"
                ) {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "get() got an unexpected keyword argument '{key}'"
                    )));
                }
            }
        }
        Ok(Self {
            uid: Self::optional_string(kwargs, "uid")?,
            space: Self::optional_string(kwargs, "space")?,
            name: Self::optional_string(kwargs, "name")?,
            version: Self::optional_string(kwargs, "version")?,
            interface: Self::optional_object(kwargs, "interface")?,
            eager_load: Self::optional_bool(kwargs, "eager_load")?.unwrap_or(false),
            load_kwargs: Self::optional_object(kwargs, "load_kwargs")?,
        })
    }

    /// Extract an optional string keyword without retaining a Python borrow.
    ///
    /// # Errors
    ///
    /// Returns a Python extraction error when a present non-`None` value is
    /// not a string.
    fn optional_string(kwargs: Option<&Bound<'_, PyDict>>, name: &str) -> PyResult<Option<String>> {
        let Some(value) = kwargs
            .map(|values| values.get_item(name))
            .transpose()?
            .flatten()
        else {
            return Ok(None);
        };
        if value.is_none() {
            return Ok(None);
        }
        value.extract().map(Some)
    }

    /// Extract an optional boolean keyword.
    ///
    /// # Errors
    ///
    /// Returns a Python extraction error when a present non-`None` value is
    /// not a boolean.
    fn optional_bool(kwargs: Option<&Bound<'_, PyDict>>, name: &str) -> PyResult<Option<bool>> {
        let Some(value) = kwargs
            .map(|values| values.get_item(name))
            .transpose()?
            .flatten()
        else {
            return Ok(None);
        };
        if value.is_none() {
            return Ok(None);
        }
        value.extract().map(Some)
    }

    /// Retain an optional arbitrary Python object beyond the dictionary borrow.
    ///
    /// # Errors
    ///
    /// Returns a Python dictionary lookup error when the mapping cannot be
    /// inspected.
    fn optional_object(
        kwargs: Option<&Bound<'_, PyDict>>,
        name: &str,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(value) = kwargs
            .map(|values| values.get_item(name))
            .transpose()?
            .flatten()
        else {
            return Ok(None);
        };
        if value.is_none() {
            return Ok(None);
        }
        Ok(Some(value.unbind()))
    }
}

/// Execute one normalized typed-registry list request without holding the GIL.
///
/// # Errors
///
/// Returns validation errors for malformed filters and Wyrd transport errors
/// when the server request fails.
fn list_registry(
    py: Python<'_>,
    registry: &Cards,
    kind: CardKind,
    query: RegistryListQuery,
) -> CardPyResult<PyCardList> {
    let request = ListCardsRequest {
        kind: Some(kind),
        space: query.space.as_deref().map(parse_space).transpose()?,
        name: query.name.as_deref().map(parse_name).transpose()?,
        version_range: query.version_range,
        status: query
            .status
            .as_deref()
            .map(parse_lifecycle_status)
            .transpose()?,
        filter: query
            .filter
            .as_deref()
            .map(MetadataQuery::parse)
            .transpose()
            .map_err(WyrdPyError::from)?,
        include_prerelease: query.include_prerelease,
        limit: query.limit,
        cursor: query.cursor,
    };
    py.detach(|| wyrd_runtime::runtime().block_on(registry.list(request)))
        .map(PyCardList::from)
        .map_err(WyrdPyError::from)
}

fn resolve_latest_registry(
    py: Python<'_>,
    registry: &Cards,
    kind: CardKind,
    space: &str,
    name: &str,
) -> CardPyResult<CardRefPy> {
    let space = parse_space(space)?;
    let name = parse_name(name)?;
    py.detach(|| wyrd_runtime::runtime().block_on(registry.resolve_latest(kind, space, name)))
        .map(CardRefPy)
        .map_err(WyrdPyError::from)
}

fn delete_registry(
    py: Python<'_>,
    registry: &Cards,
    kind: CardKind,
    uid: Option<&str>,
    space: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
) -> CardPyResult<()> {
    let selector = selector_for_kind(kind, uid, space, name, version)?;
    py.detach(|| wyrd_runtime::runtime().block_on(registry.delete(selector)))
        .map_err(WyrdPyError::from)
}

fn register_view_card(
    py: Python<'_>,
    registry: &Cards,
    card: &Bound<'_, PyAny>,
    version_bump: Option<&Bound<'_, PyAny>>,
    save_args: Option<&Bound<'_, PyAny>>,
    kind: &CardKind,
) -> CardPyResult<PyRegistrationReceipt> {
    PythonCardRegistry::new(registry).register_typed(py, card, version_bump, save_args, kind)
}

#[pymethods]
impl PyDataCardRegistry {
    /// Register a `DataCard` and upload its saved data artifacts.
    ///
    /// The caller-owned card receives the server-assigned identity after the
    /// operation completes. `save_args` is forwarded to the card's data
    /// interface and is not stored as registry metadata.
    ///
    /// # Arguments
    /// * `card` - `DataCard` to register.
    /// * `version_bump` - Version intent. Defaults to server-compatible `VersionBump.Patch`.
    /// * `save_args` - Optional `DataSaveArgs` for the data interface.
    ///
    /// # Returns
    /// A receipt for the completed registration.
    ///
    /// # Errors
    /// Returns a Wyrd error when the interface, serialization, artifact upload,
    /// validation, or server completion fails.
    #[pyo3(signature = (card, version_bump=None, save_args=None))]
    fn register(
        &self,
        py: Python<'_>,
        card: &Bound<'_, PyAny>,
        version_bump: Option<&Bound<'_, PyAny>>,
        save_args: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<PyRegistrationReceipt> {
        register_view_card(
            py,
            &self.inner,
            card,
            version_bump,
            save_args,
            &CardKind::Data,
        )
    }

    /// List metadata-only `DataCard` summaries.
    ///
    /// The result contains identity, hashes, lifecycle status, and exact
    /// references. It does not download Card envelopes or data artifacts.
    /// Use `get` for the full holder.
    ///
    /// # Arguments
    /// * `space` - Optional space filter.
    /// * `name` - Optional name filter.
    /// * `version_range` - Optional semantic-version range.
    /// * `status` - Optional lifecycle status filter.
    /// * `filter` - Optional metadata query expression.
    /// * `include_prerelease` - Include pre-release versions.
    /// * `limit` - Maximum number of summaries in this page.
    /// * `cursor` - Opaque cursor returned by a previous page.
    ///
    /// # Returns
    /// A cursor-paginated `CardList`.
    ///
    /// # Errors
    /// Returns a Wyrd error when a filter, version range, or server request is
    /// invalid.
    #[pyo3(signature = (**kwargs))]
    fn list(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<PyCardList> {
        list_registry(
            py,
            &self.inner,
            CardKind::Data,
            RegistryListQuery::from_kwargs(kwargs)?,
        )
    }

    /// Resolve the latest `DataCard` reference for a name.
    ///
    /// This returns identity only. Call `get(uid=reference.uid)` for the
    /// complete card envelope.
    ///
    /// # Arguments
    /// * `space` - Card space.
    /// * `name` - Card name.
    ///
    /// # Returns
    /// The latest exact `CardRef` selected by the server.
    ///
    /// # Errors
    /// Returns a Wyrd error when the identity is invalid or no matching Card
    /// exists.
    #[pyo3(signature = (*, space, name))]
    fn resolve_latest(&self, py: Python<'_>, space: &str, name: &str) -> CardPyResult<CardRefPy> {
        resolve_latest_registry(py, &self.inner, CardKind::Data, space, name)
    }

    /// Delete a registered `DataCard` and its stored artifacts.
    ///
    /// Provide `uid` for an exact deletion. Otherwise provide `space`, `name`,
    /// and the exact `version`.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, or deletion is rejected.
    #[pyo3(signature = (*, uid=None, space=None, name=None, version=None))]
    fn delete(
        &self,
        py: Python<'_>,
        uid: Option<&str>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
    ) -> CardPyResult<()> {
        delete_registry(py, &self.inner, CardKind::Data, uid, space, name, version)
    }

    /// Retrieve and validate one complete `DataCard` envelope.
    ///
    /// Pass `uid` for an exact lookup. Without `uid`, `space` and `name` are
    /// required; omitting `version` selects the latest resolved version. A
    /// custom data interface must be supplied when the serialized Card uses
    /// one. With `eager_load=True`, `get` downloads verified artifacts and
    /// forwards `load_kwargs` only to the Data holder load operation. Otherwise
    /// it returns the hydrated holder without loading artifacts.
    ///
    /// # Arguments
    /// * `uid` - Exact server-assigned UID. It takes precedence over the named
    ///   selector; supplied identity fields are checked against the result.
    /// * `space` - Card space, required when `uid` is omitted.
    /// * `name` - Card name, required when `uid` is omitted.
    /// * `version` - Exact version, or latest when omitted.
    /// * `interface` - Built-in or custom `DataInterface` instance/class used
    ///   to rebuild the Python interface from Card metadata.
    /// * `eager_load` - Whether to download verified artifacts and load the
    ///   Data holder before returning.
    /// * `load_kwargs` - Optional typed or mapping arguments forwarded only to
    ///   the eager Data holder load operation.
    ///
    /// # Returns
    /// A native `DataCard` populated from server-stored Card JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, the envelope fails validation, or a required custom interface is
    /// missing.
    #[pyo3(signature = (**kwargs))]
    fn get(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<DataCard> {
        let query = RegistryGetQuery::from_kwargs(kwargs)?;
        let selector = selector_for_kind(
            CardKind::Data,
            query.uid.as_deref(),
            query.space.as_deref(),
            query.name.as_deref(),
            query.version.as_deref(),
        )?;
        let envelope = download_card(py, &self.inner, selector)?;
        let mut card = DataCard::from_card(envelope).map_err(WyrdPyError::from)?;
        card.hydrate_interface(
            py,
            query.interface.as_ref().map(|interface| interface.bind(py)),
        )?;
        if query.eager_load {
            let uid = card
                .as_card_ref()
                .map_err(WyrdPyError::from)?
                .uid
                .ok_or_else(|| {
                    WyrdPyError::validation("server DataCard response requires a Card UID")
                })?;
            let workspace = tempfile::Builder::new()
                .prefix("wyrd-data-")
                .tempdir()
                .map_err(|error| WyrdPyError::Io(error.to_string()))?;
            let path = workspace.path().join("artifacts");
            let client = self.inner.clone();
            let download_path = path.clone();
            py.detach(move || {
                wyrd_runtime::runtime().block_on(client.download_artifacts_to(&uid, &download_path))
            })
            .map_err(WyrdPyError::from)?;
            card.load(
                py,
                Some(path),
                query.load_kwargs.as_ref().map(|args| args.bind(py)),
            )?;
            card.artifact_workspace = Some(workspace);
        }
        Ok(card)
    }
}

#[pymethods]
impl PyModelCardRegistry {
    /// Register a `ModelCard` and upload its saved model artifacts.
    ///
    /// The caller-owned card receives the server-assigned identity after the
    /// operation completes. `save_args` is forwarded to the model interface.
    ///
    /// # Arguments
    /// * `card` - `ModelCard` to register.
    /// * `version_bump` - Version intent. Defaults to server-compatible `VersionBump.Patch`.
    /// * `save_args` - Optional `ModelSaveArgs` for the model interface.
    ///
    /// # Returns
    /// A receipt for the completed registration.
    ///
    /// # Errors
    /// Returns a Wyrd error when the interface, serialization, artifact upload,
    /// validation, or server completion fails.
    #[pyo3(signature = (card, version_bump=None, save_args=None))]
    fn register(
        &self,
        py: Python<'_>,
        card: &Bound<'_, PyAny>,
        version_bump: Option<&Bound<'_, PyAny>>,
        save_args: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<PyRegistrationReceipt> {
        register_view_card(
            py,
            &self.inner,
            card,
            version_bump,
            save_args,
            &CardKind::Model,
        )
    }

    /// List metadata-only `ModelCard` summaries.
    ///
    /// This operation does not fetch model bytes. Use `get` for the full
    /// envelope and `ModelCard.load` for artifact hydration.
    ///
    /// # Arguments
    /// * `space` - Optional space filter.
    /// * `name` - Optional name filter.
    /// * `version_range` - Optional semantic-version range.
    /// * `status` - Optional lifecycle status filter.
    /// * `filter` - Optional metadata query expression.
    /// * `include_prerelease` - Include pre-release versions.
    /// * `limit` - Maximum number of summaries in this page.
    /// * `cursor` - Opaque cursor returned by a previous page.
    ///
    /// # Returns
    /// A cursor-paginated `CardList`.
    ///
    /// # Errors
    /// Returns a Wyrd error when a filter, version range, or server request is
    /// invalid.
    #[pyo3(signature = (**kwargs))]
    fn list(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<PyCardList> {
        list_registry(
            py,
            &self.inner,
            CardKind::Model,
            RegistryListQuery::from_kwargs(kwargs)?,
        )
    }

    /// Resolve the latest `ModelCard` reference for a name.
    ///
    /// This returns identity only. Call `get(uid=reference.uid)` for the
    /// complete card envelope.
    ///
    /// # Arguments
    /// * `space` - Card space.
    /// * `name` - Card name.
    ///
    /// # Returns
    /// The latest exact `CardRef` selected by the server.
    ///
    /// # Errors
    /// Returns a Wyrd error when the identity is invalid or no matching Card
    /// exists.
    #[pyo3(signature = (*, space, name))]
    fn resolve_latest(&self, py: Python<'_>, space: &str, name: &str) -> CardPyResult<CardRefPy> {
        resolve_latest_registry(py, &self.inner, CardKind::Model, space, name)
    }

    /// Delete a registered `ModelCard` and its stored artifacts.
    ///
    /// Provide `uid` for an exact deletion. Otherwise provide `space`, `name`,
    /// and the exact `version`.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, or deletion is rejected.
    #[pyo3(signature = (*, uid=None, space=None, name=None, version=None))]
    fn delete(
        &self,
        py: Python<'_>,
        uid: Option<&str>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
    ) -> CardPyResult<()> {
        delete_registry(py, &self.inner, CardKind::Model, uid, space, name, version)
    }

    /// Retrieve and validate one complete `ModelCard` envelope.
    ///
    /// Pass `uid` for an exact lookup. Without `uid`, `space` and `name` are
    /// required; omitting `version` selects the latest resolved version. A
    /// custom model interface must be supplied when the serialized Card
    /// cannot rebuild one from built-in metadata. Artifact download and local
    /// interface loading occur only when `eager_load=True`. When eager loading
    /// is enabled, `load_kwargs` is forwarded only to the Model holder load
    /// operation.
    ///
    /// # Arguments
    /// * `uid` - Exact server-assigned UID. It takes precedence over the named
    ///   selector.
    /// * `space` - Card space, required when `uid` is omitted.
    /// * `name` - Card name, required when `uid` is omitted.
    /// * `version` - Exact version, or latest when omitted.
    /// * `interface` - Built-in or custom `ModelInterface` instance/class used
    ///   to rebuild the Python interface from Card metadata.
    /// * `eager_load` - Whether to download verified artifacts and load the
    ///   Model holder before returning.
    /// * `load_kwargs` - Optional typed or mapping arguments forwarded only to
    ///   the eager Model holder load operation.
    ///
    /// # Returns
    /// A native `ModelCard` populated from server-stored Card JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, the envelope fails validation, or a required custom interface is
    /// missing. With `eager_load=True`, `get` retains verified artifacts on
    /// the holder after the interface loads successfully.
    #[pyo3(signature = (**kwargs))]
    fn get(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<ModelCard> {
        let query = RegistryGetQuery::from_kwargs(kwargs)?;
        let selector = selector_for_kind(
            CardKind::Model,
            query.uid.as_deref(),
            query.space.as_deref(),
            query.name.as_deref(),
            query.version.as_deref(),
        )?;
        let envelope = download_card(py, &self.inner, selector)?;
        let mut card = ModelCard::from_card(envelope).map_err(WyrdPyError::from)?;
        card.hydrate_interface(
            py,
            query.interface.as_ref().map(|interface| interface.bind(py)),
        )?;
        if query.eager_load {
            let uid = card
                .as_card_ref()
                .map_err(WyrdPyError::from)?
                .uid
                .ok_or_else(|| {
                    WyrdPyError::model_validation("server ModelCard response requires a Card UID")
                })?;
            let workspace = tempfile::Builder::new()
                .prefix("wyrd-model-")
                .tempdir()
                .map_err(|error| WyrdPyError::Io(error.to_string()))?;
            let path = workspace.path().join("artifacts");
            let client = self.inner.clone();
            let download_path = path.clone();
            py.detach(move || {
                wyrd_runtime::runtime().block_on(client.download_artifacts_to(&uid, &download_path))
            })
            .map_err(WyrdPyError::from)?;
            card.load(
                py,
                Some(path),
                query.load_kwargs.as_ref().map(|args| args.bind(py)),
            )?;
            card.artifact_workspace = Some(workspace);
        }
        Ok(card)
    }
}

#[pymethods]
impl PyPromptCardRegistry {
    /// Register a `PromptCard` and update its server identity.
    ///
    /// Prompt cards have no artifact save arguments. Registration persists the
    /// serialized prompt Card and waits for server completion.
    ///
    /// # Arguments
    /// * `card` - `PromptCard` to register.
    /// * `version_bump` - Version intent. Defaults to server-compatible `VersionBump.Patch`.
    ///
    /// # Returns
    /// A receipt for the completed registration.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization, validation, upload, or server
    /// completion fails.
    #[pyo3(signature = (card, version_bump=None))]
    fn register(
        &self,
        py: Python<'_>,
        card: &Bound<'_, PyAny>,
        version_bump: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<PyRegistrationReceipt> {
        register_view_card(py, &self.inner, card, version_bump, None, &CardKind::Prompt)
    }

    /// List metadata-only `PromptCard` summaries.
    ///
    /// # Arguments
    /// * `space` - Optional space filter.
    /// * `name` - Optional name filter.
    /// * `version_range` - Optional semantic-version range.
    /// * `status` - Optional lifecycle status filter.
    /// * `filter` - Optional metadata query expression.
    /// * `include_prerelease` - Include pre-release versions.
    /// * `limit` - Maximum number of summaries in this page.
    /// * `cursor` - Opaque cursor returned by a previous page.
    ///
    /// # Returns
    /// A cursor-paginated `CardList`. It contains metadata only.
    ///
    /// # Errors
    /// Returns a Wyrd error when a filter, version range, or server request is
    /// invalid.
    #[pyo3(signature = (**kwargs))]
    fn list(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<PyCardList> {
        list_registry(
            py,
            &self.inner,
            CardKind::Prompt,
            RegistryListQuery::from_kwargs(kwargs)?,
        )
    }

    /// Resolve the latest `PromptCard` reference for a name.
    ///
    /// This returns identity only. Call `get(uid=reference.uid)` for the
    /// complete prompt Card.
    ///
    /// # Arguments
    /// * `space` - Card space.
    /// * `name` - Card name.
    ///
    /// # Returns
    /// The latest exact `CardRef` selected by the server.
    ///
    /// # Errors
    /// Returns a Wyrd error when the identity is invalid or no matching Card
    /// exists.
    #[pyo3(signature = (*, space, name))]
    fn resolve_latest(&self, py: Python<'_>, space: &str, name: &str) -> CardPyResult<CardRefPy> {
        resolve_latest_registry(py, &self.inner, CardKind::Prompt, space, name)
    }

    /// Delete a registered `PromptCard` and its stored Card envelope.
    ///
    /// Provide `uid` for an exact deletion. Otherwise provide `space`, `name`,
    /// and the exact `version`.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, or deletion is rejected.
    #[pyo3(signature = (*, uid=None, space=None, name=None, version=None))]
    fn delete(
        &self,
        py: Python<'_>,
        uid: Option<&str>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
    ) -> CardPyResult<()> {
        delete_registry(py, &self.inner, CardKind::Prompt, uid, space, name, version)
    }

    /// Retrieve and validate one complete `PromptCard` envelope.
    ///
    /// Pass `uid` for an exact lookup. Without `uid`, `space` and `name` are
    /// required; omitting `version` selects the latest resolved version.
    /// Prompt cards do not have a separate artifact hydration step.
    ///
    /// # Arguments
    /// * `uid` - Exact server-assigned UID. It takes precedence over the named
    ///   selector.
    /// * `space` - Card space, required when `uid` is omitted.
    /// * `name` - Card name, required when `uid` is omitted.
    /// * `version` - Exact version, or latest when omitted.
    ///
    /// # Returns
    /// A native `PromptCard` populated from server-stored Card JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, or the envelope fails validation.
    #[pyo3(signature = (*, uid=None, space=None, name=None, version=None))]
    fn get(
        &self,
        py: Python<'_>,
        uid: Option<&str>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
    ) -> CardPyResult<PromptCard> {
        let selector = selector_for_kind(CardKind::Prompt, uid, space, name, version)?;
        let envelope = download_card(py, &self.inner, selector)?;
        let mut card = PromptCard::from_card(envelope)?;
        card.hydrate_prompt(py)?;
        Ok(card)
    }
}

/// Saved Python holder state awaiting native manifest construction.
struct SavedPythonCard {
    /// Validated envelope carrying one coherent server-native version intent.
    envelope: PythonCardEnvelope,
    /// Private workspace containing the holder's serialized payload.
    tempdir: TempDir,
}

/// Holder envelope plus its server-native authored version intent.
struct SavedCardEnvelope {
    /// Serialized holder envelope after its Python save callback completes.
    json: String,
    /// Exact pin, registration scope, or auto-versioning intent.
    version: Option<VersionSpec>,
}

/// Registration input whose manifest paths are confined to its retained workspace.
struct PreparedPythonCard {
    /// Native input whose artifact sources were derived from `tempdir`.
    input: wyrd_loader::RegistrationInput,
    /// Workspace retained until hashing, upload, and completion finish.
    tempdir: TempDir,
}

/// Establishes and exposes the native registration preparation invariant.
impl PreparedPythonCard {
    /// Build and validate the native manifest for one saved Python holder.
    ///
    /// This constructor establishes the invariant that every artifact source
    /// in `input` lives beneath the retained `tempdir`. It performs recursive
    /// filesystem hashing and must therefore run outside the Python GIL.
    ///
    /// # Errors
    /// Returns a stable loader-manifest error when a saved path is unsafe,
    /// unreadable, or cannot be hashed.
    fn new(saved: SavedPythonCard) -> CardPyResult<Self> {
        let root = saved.tempdir.path();
        let manifest = wyrd_loader::build_artifact_manifest(root, "card.json")
            .map_err(|error| loader_manifest_error(&error))?;
        Ok(Self {
            input: wyrd_loader::RegistrationInput {
                submissions: vec![wyrd_spec::registry::CardSubmission {
                    api_version: saved.envelope.api_version,
                    kind: saved.envelope.kind,
                    metadata: saved.envelope.metadata,
                    spec: saved.envelope.spec,
                    artifacts: manifest.entries,
                }],
                artifact_sources: manifest.sources,
            },
            tempdir: saved.tempdir,
        })
    }
}

#[derive(serde::Deserialize)]
struct PythonCardEnvelope {
    #[serde(rename = "apiVersion")]
    api_version: ApiVersion,
    kind: CardKind,
    metadata: Metadata,
    spec: serde_json::Value,
}

/// Save one Python Card holder into a native registration input and await the
/// server receipt before mutating the caller-owned identity.
///
/// Exact metadata pins are mutually exclusive with an explicitly supplied
/// version bump; omitted bumps use the server-compatible Patch default.
///
/// # Errors
/// Returns holder validation, local save, manifest, transport, or server
/// completion errors. The holder is stamped only after a successful receipt.
fn register_python_card(
    py: Python<'_>,
    registry: &Cards,
    card: &Bound<'_, PyAny>,
    version_bump: Option<&Bound<'_, PyAny>>,
    save_args: Option<&Bound<'_, PyAny>>,
    expected_kind: Option<&CardKind>,
) -> CardPyResult<PyRegistrationReceipt> {
    let kind = holder_kind(card)?;
    if let Some(expected) = expected_kind
        && expected != &kind
    {
        return Err(WyrdPyError::validation(format!(
            "typed registry expects {}, got {}",
            expected.wire_name(),
            kind.wire_name()
        )));
    }
    let bump_supplied = version_bump.is_some();
    let bump = version_bump
        .map(parse_version_bump)
        .transpose()?
        .unwrap_or(VersionBump::Patch);
    let saved = save_python_card(py, card, save_args, &kind, bump, bump_supplied)?;
    let receipt = py
        .detach(move || {
            let prepared = PreparedPythonCard::new(saved)?;
            let _keep_alive = &prepared.tempdir;
            wyrd_runtime::runtime()
                .block_on(registry.register(&prepared.input))
                .map_err(WyrdPyError::from)
        })
        .map(PyRegistrationReceipt::from)?;
    stamp_python_holder(card, &receipt.inner.root)?;
    Ok(receipt)
}

/// Run the Python holder callback and normalize its server-native envelope.
///
/// # Errors
/// Returns Python extraction, local save, envelope validation, version-intent,
/// or serialization errors. No hashing or remote operation is started here.
fn save_python_card(
    py: Python<'_>,
    card: &Bound<'_, PyAny>,
    save_args: Option<&Bound<'_, PyAny>>,
    kind: &CardKind,
    bump: VersionBump,
    bump_supplied: bool,
) -> CardPyResult<SavedPythonCard> {
    let tempdir = tempdir().map_err(|error| WyrdPyError::Io(error.to_string()))?;
    let root = tempdir.path().to_path_buf();
    let saved_envelope = match kind {
        CardKind::Data => {
            let save_kwargs = extract_data_save_args(py, save_args)?;
            let mut holder = card
                .extract::<PyRefMut<'_, DataCard>>()
                .map_err(|error| WyrdPyError::Python(error.to_string()))?;
            let authored = registration_version(&holder.version)?;
            let original = holder.version.clone();
            holder.version = local_save_version(authored.as_ref());
            let result = holder
                .save(
                    py,
                    root.clone(),
                    save_kwargs.as_ref().map(|value| value.bind(py)),
                )
                .and_then(|()| holder.model_dump_json());
            holder.version = original;
            SavedCardEnvelope {
                json: result?,
                version: authored,
            }
        }
        CardKind::Model => {
            let save_kwargs = extract_model_save_args(py, save_args)?;
            let mut holder = card
                .extract::<PyRefMut<'_, ModelCard>>()
                .map_err(|error| WyrdPyError::Python(error.to_string()))?;
            let authored = registration_version(&holder.version)?;
            let original = holder.version.clone();
            holder.version = local_save_version(authored.as_ref());
            let result = holder
                .save(
                    py,
                    root.clone(),
                    save_kwargs.as_ref().map(|value| value.bind(py)),
                )
                .and_then(|()| holder.model_dump_json());
            holder.version = original;
            SavedCardEnvelope {
                json: result?,
                version: authored,
            }
        }
        CardKind::Prompt => {
            if save_args.is_some() {
                return Err(WyrdPyError::validation(
                    "PromptCard registration does not accept save_args",
                ));
            }
            let mut holder = card
                .extract::<PyRefMut<'_, PromptCard>>()
                .map_err(|error| WyrdPyError::Python(error.to_string()))?;
            let authored = registration_version(&holder.version)?;
            let original = holder.version.clone();
            holder.version = local_save_version(authored.as_ref());
            let result = holder
                .save(root.join("card.json"))
                .and_then(|()| holder.model_dump_json());
            holder.version = original;
            SavedCardEnvelope {
                json: result?,
                version: authored,
            }
        }
        _ => {
            return Err(WyrdPyError::validation(
                "Cards.register supports DataCard, ModelCard, and PromptCard",
            ));
        }
    };

    let mut envelope: PythonCardEnvelope = serde_json::from_str(&saved_envelope.json)?;
    envelope.metadata.version = saved_envelope.version;
    if envelope.api_version.as_str() != ApiVersion::V1 || &envelope.kind != kind {
        return Err(WyrdPyError::validation(
            "card envelope kind or apiVersion does not match the native holder",
        ));
    }
    let exact_pin = envelope
        .metadata
        .version
        .as_ref()
        .is_some_and(VersionSpec::is_pin);
    if bump_supplied && exact_pin {
        return Err(WyrdPyError::validation(
            "version_bump cannot be combined with an exact metadata.version pin; use a scope or omit version",
        ));
    }
    envelope.metadata.uid = None;
    envelope.metadata.spec_hash = None;
    envelope.metadata.artifact_hash = None;
    envelope.metadata.bump = (!exact_pin).then_some(bump);

    Ok(SavedPythonCard { envelope, tempdir })
}

/// Parse one Python holder's authored registration version.
///
/// An empty value selects server auto-versioning. Exact triples remain pins;
/// one- and two-component values remain registration scopes.
///
/// # Errors
/// Returns a validation error when `value` is not a supported registration
/// version shape.
fn registration_version(value: &str) -> CardPyResult<Option<VersionSpec>> {
    if value.is_empty() {
        return Ok(None);
    }
    VersionSpec::parse(value)
        .map(Some)
        .map_err(|error| WyrdPyError::validation(format!("invalid card version: {error}")))
}

/// Return an exact placeholder accepted by local Card serialization.
///
/// The placeholder is never submitted. [`save_python_card`] restores the
/// authored [`VersionSpec`] after the Python callback and local write finish.
fn local_save_version(version: Option<&VersionSpec>) -> String {
    match version {
        Some(VersionSpec::Pin(pin)) => pin.to_string(),
        Some(VersionSpec::Scope(scope)) => {
            let body = scope
                .as_str()
                .strip_prefix(['^', '~'])
                .unwrap_or(scope.as_str());
            match body.matches('.').count() {
                0 => format!("{body}.0.0"),
                1 => format!("{body}.0"),
                _ => body.to_owned(),
            }
        }
        None => wyrd_semver::seed_version().to_string(),
    }
}

fn extract_data_save_args(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<Option<Py<PyDict>>> {
    value
        .map(|value| {
            value
                .extract::<PyRef<'_, PyDataSaveArgs>>()
                .map(|args| args.to_dict(py))
                .map_err(|_| WyrdPyError::validation("DataCard registration requires DataSaveArgs"))
        })
        .transpose()
}

fn extract_model_save_args(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<Option<Py<PyDict>>> {
    value
        .map(|value| {
            value
                .extract::<PyRef<'_, PyModelSaveArgs>>()
                .map(|args| args.to_dict(py))
                .map_err(|_| {
                    WyrdPyError::validation("ModelCard registration requires ModelSaveArgs")
                })
        })
        .transpose()
}

fn download_card(py: Python<'_>, registry: &Cards, selector: CardSelector) -> CardPyResult<Card> {
    py.detach(|| wyrd_runtime::runtime().block_on(registry.get(selector)))
        .map_err(WyrdPyError::from)
}

fn stamp_python_holder(
    card: &Bound<'_, PyAny>,
    card_ref: &wyrd_spec::reference::CardRef,
) -> CardPyResult<()> {
    let uid = card_ref.uid.as_ref().ok_or_else(|| {
        WyrdPyError::validation("registration response did not contain a Card UID")
    })?;
    card.setattr("uid", uid.to_string())?;
    card.setattr("version", card_ref.version.to_string())?;
    if let Some(space) = &card_ref.space {
        card.setattr("space", space.to_string())?;
    }
    card.setattr("name", card_ref.name.to_string())?;
    Ok(())
}

fn holder_kind(card: &Bound<'_, PyAny>) -> CardPyResult<CardKind> {
    if card.is_instance_of::<DataCard>() {
        Ok(CardKind::Data)
    } else if card.is_instance_of::<ModelCard>() {
        Ok(CardKind::Model)
    } else if card.is_instance_of::<PromptCard>() {
        Ok(CardKind::Prompt)
    } else {
        Err(WyrdPyError::validation(
            "Cards.register requires a wyrd DataCard, ModelCard, or PromptCard",
        ))
    }
}

fn parse_version_bump(value: &Bound<'_, PyAny>) -> CardPyResult<VersionBump> {
    value
        .extract::<PyRef<'_, PyVersionBump>>()
        .map(|bump| bump.as_native())
        .map_err(|_| WyrdPyError::validation("version_bump must be a VersionBump"))
}

fn selector_for_kind(
    kind: CardKind,
    uid: Option<&str>,
    space: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
) -> CardPyResult<CardSelector> {
    if let Some(uid) = uid {
        let uid = CardUid::new(uid).map_err(|error| WyrdPyError::validation(error.to_string()))?;
        let selector = CardSelector::uid(kind, uid).with_identity_assertions(
            space.map(parse_space).transpose()?,
            name.map(parse_name).transpose()?,
        );
        return version
            .map(parse_version)
            .transpose()?
            .map_or(Ok(selector.clone()), |version| {
                Ok(selector.with_version(version))
            });
    }

    let space = space
        .ok_or_else(|| WyrdPyError::validation("space is required when uid is not provided"))
        .and_then(parse_space)?;
    let name = name
        .ok_or_else(|| WyrdPyError::validation("name is required when uid is not provided"))
        .and_then(parse_name)?;
    let selector = CardSelector::named(kind, space, name);
    version
        .map(parse_version)
        .transpose()?
        .map_or(Ok(selector.clone()), |version| {
            Ok(selector.with_version(version))
        })
}

fn parse_space(value: &str) -> CardPyResult<SpaceName> {
    SpaceName::new(value).map_err(|error| WyrdPyError::validation(error.to_string()))
}

fn parse_name(value: &str) -> CardPyResult<CardName> {
    CardName::new(value).map_err(|error| WyrdPyError::validation(error.to_string()))
}

fn parse_version(value: &str) -> CardPyResult<VersionBlock> {
    VersionBlock::parse(value).map_err(|error| WyrdPyError::validation(error.to_string()))
}

fn parse_lifecycle_status(value: &str) -> CardPyResult<CardLifecycleStatus> {
    serde_json::from_value(serde_json::Value::String(value.to_ascii_lowercase()))
        .map_err(|_| WyrdPyError::validation(format!("unknown Card lifecycle status: {value}")))
}

fn card_ref_from_summary(summary: &CardSummary) -> wyrd_spec::reference::CardRef {
    wyrd_spec::reference::CardRef {
        kind: summary.kind.clone(),
        name: summary.name.clone(),
        version: summary.version.clone(),
        space: Some(summary.space.clone()),
        uid: Some(summary.card_uid.clone()),
    }
}

fn kind_from_card_kind(kind: &CardKind) -> Kind {
    match kind {
        CardKind::Data => Kind::Data,
        CardKind::Model => Kind::Model,
        CardKind::Experiment => Kind::Experiment,
        CardKind::Prompt => Kind::Prompt,
        CardKind::Agent => Kind::Agent,
        CardKind::Workflow => Kind::Workflow,
        CardKind::Eval => Kind::Eval,
        CardKind::Drift => Kind::Drift,
        CardKind::Service => Kind::Service,
        CardKind::Policy => Kind::Policy,
        CardKind::Mcp => Kind::Mcp,
        CardKind::Audit => Kind::Audit,
        CardKind::Artifact => Kind::Artifact,
        CardKind::Trigger => Kind::Trigger,
        CardKind::Operator => Kind::Operator,
        CardKind::Source => Kind::Source,
        CardKind::External => Kind::External,
    }
}

fn lifecycle_status_name(status: CardLifecycleStatus) -> &'static str {
    match status {
        CardLifecycleStatus::Pending => "pending",
        CardLifecycleStatus::Active => "active",
        CardLifecycleStatus::Deprecated => "deprecated",
        CardLifecycleStatus::Failed => "failed",
        CardLifecycleStatus::Expired => "expired",
        CardLifecycleStatus::Deleted => "deleted",
    }
}

fn registration_outcome_name(outcome: RegistrationOutcomeKind) -> &'static str {
    match outcome {
        RegistrationOutcomeKind::Registered => "registered",
        RegistrationOutcomeKind::IdempotentNoop => "idempotent_noop",
        RegistrationOutcomeKind::Deduplicated => "deduplicated",
    }
}

fn loader_manifest_error(error: &wyrd_loader::LoadError) -> WyrdPyError {
    WyrdPyError::from(WyrdError::LoaderInvalidEnvelope {
        message: error.to_string(),
        details: serde_json::json!({ "diagnostics": &error.diagnostics }),
    })
}

impl From<ListCardsResponse> for PyCardList {
    fn from(response: ListCardsResponse) -> Self {
        let refs = response
            .items
            .iter()
            .map(|item| CardRefPy(card_ref_from_summary(item)))
            .collect();
        Self {
            items: response
                .items
                .into_iter()
                .map(|inner| PyCardSummary { inner })
                .collect(),
            refs,
            next_cursor: response.next_cursor,
        }
    }
}

/// Register the native offline-state classes on `wyrd._wyrd.state`.
///
/// The Python package exposes this native module through `wyrd.state`. During
/// extension initialisation, registration adds the `WyrdState` runtime handle
/// first, followed by its immutable `CardEnvelope` and `HydratedArtifact`
/// projections. Keeping these classes together ensures that a hydrated local
/// bundle can be projected without importing the server-facing card registry;
/// the registration itself performs no bundle loading, filesystem access, or
/// network work.
///
/// # Errors
///
/// Returns a [`CardPyResult`] error when `PyO3` cannot allocate or mutate the
/// module while registering `PyWyrdState`, `PyCardEnvelope`, or
/// `PyHydratedArtifact`. In particular, failures from each corresponding
/// `add_class` call (such as Python allocation errors or a duplicate/incompatible
/// class entry) are propagated unchanged to the extension initialiser.
pub fn register_state(module: &Bound<'_, PyModule>) -> CardPyResult<()> {
    module.add_class::<PyWyrdState>()?;
    module.add_class::<PyCardEnvelope>()?;
    module.add_class::<PyHydratedArtifact>()?;
    Ok(())
}

/// Register the client Card handle on `wyrd.cards`.
pub fn register_cards(module: &Bound<'_, PyModule>) -> CardPyResult<()> {
    module.add_class::<PyVersionBump>()?;
    module.add_class::<PyDataSaveArgs>()?;
    module.add_class::<PyModelSaveArgs>()?;
    module.add_class::<PyDataLoadArgs>()?;
    module.add_class::<PyModelLoadArgs>()?;
    module.add_class::<PyCards>()?;
    module.add_class::<PyDataCardRegistry>()?;
    module.add_class::<PyModelCardRegistry>()?;
    module.add_class::<PyPromptCardRegistry>()?;
    module.add_class::<PyCardSummary>()?;
    module.add_class::<PyCardList>()?;
    module.add_class::<PyRegistrationOutcome>()?;
    module.add_class::<PyRegistrationReceipt>()?;
    Ok(())
}
