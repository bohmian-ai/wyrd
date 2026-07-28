//! Python projections for the Rust-owned Wyrd client SDK.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyMapping, PyModule, PyTuple};
use secrecy::SecretString;
use tempfile::{TempDir, tempdir};
use wyrd_cards::card_ref::{CardRefPy, Kind};
use wyrd_cards::{agent::PyAgentCard, data::DataCard, model::ModelCard, prompt::PromptCard};
use wyrd_interfaces::error::{CardPyResult, WyrdPyError};
use wyrd_registry::{CardSelector, Cards};
use wyrd_semver::{VersionBlock, VersionBump};
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

use crate::WyrdState;

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

/// Normalized Python loader configuration keyed by exact `CardRef` identity.
struct PythonLoadConfig {
    /// Interface objects retained independently of borrowed Python lifetimes.
    interface_by_ref: BTreeMap<String, Py<PyAny>>,
    /// JSON-compatible loader kwargs retained by exact `CardRef`.
    kwargs_by_ref: BTreeMap<String, Py<PyAny>>,
}

/// Python wrapper for a locally hydrated `WyrdState`.
#[pyclass(module = "wyrd.state", name = "WyrdState", skip_from_py_object)]
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
    /// # Cancellation
    ///
    /// The detached filesystem operation can be cancelled by the caller; no
    /// partially constructed Python state is returned, and a later call may
    /// retry local reads.
    #[staticmethod]
    #[pyo3(signature = (path, *, interfaces=None, load_kwargs=None))]
    // justification: pyo3 boundary; the extractor produces an owned PathBuf, and the path is moved into the detached filesystem operation
    #[allow(clippy::needless_pass_by_value)]
    fn from_path(
        py: Python<'_>,
        path: PathBuf,
        interfaces: Option<&Bound<'_, PyMapping>>,
        load_kwargs: Option<&Bound<'_, PyMapping>>,
    ) -> CardPyResult<Self> {
        let inner = py
            .detach(|| WyrdState::from_path(&path))
            .map_err(WyrdPyError::from)?;
        let config = parse_load_config(py, &inner, interfaces, load_kwargs)?;
        let envelopes = build_envelopes(py, &inner)?;
        let prompts = build_prompts(py, &inner)?;
        let agents = build_agents(py, &inner)?;
        let models = build_models(py, &inner, &config)?;
        let data = build_data(py, &inner, &config)?;
        Ok(Self {
            inner,
            envelopes,
            agents,
            prompts,
            models,
            data,
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
) -> CardPyResult<PythonLoadConfig> {
    let mut config = PythonLoadConfig {
        interface_by_ref: BTreeMap::new(),
        kwargs_by_ref: BTreeMap::new(),
    };
    if let Some(mapping) = interfaces {
        for (alias, value) in mapping_items(mapping)? {
            let reference = state.card_ref(&alias)?;
            require_model_data(&alias, reference)?;
            let key = reference.to_string();
            if let Some(existing) = config.interface_by_ref.get(&key)
                && !existing.bind(py).is(&value)
            {
                return Err(WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                    message: "conflicting interface overrides for one Card".to_owned(),
                    details: serde_json::json!({"alias": alias, "card_ref": reference, "stage": "interface", "reason": "aliases resolving to one Card must use the identical interface object"}),
                }));
            }
            config.interface_by_ref.insert(key, value.unbind());
        }
    }
    if let Some(mapping) = load_kwargs {
        for (alias, value) in mapping_items(mapping)? {
            let reference = state.card_ref(&alias)?;
            require_model_data(&alias, reference)?;
            let normalized = normalize_load_kwargs_value(py, state, &alias, reference, &value)?;
            let normalized_json = wyrd_utils::py::pyobject_to_json(normalized.bind(py))?;
            let key = reference.to_string();
            if let Some(existing) = config.kwargs_by_ref.get(&key) {
                let existing_json = wyrd_utils::py::pyobject_to_json(existing.bind(py))?;
                if existing_json != normalized_json {
                    return Err(WyrdPyError::from(WyrdError::SdkRuntimeHydrationFailed {
                        message: "conflicting loader kwargs for one Card".to_owned(),
                        details: serde_json::json!({"alias": alias, "card_ref": reference, "stage": "interface", "reason": "aliases resolving to one Card must use equivalent loader kwargs"}),
                    }));
                }
            }
            config.kwargs_by_ref.insert(key, normalized.into_any());
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
fn build_envelopes(
    py: Python<'_>,
    state: &WyrdState,
) -> CardPyResult<BTreeMap<String, Py<PyCardEnvelope>>> {
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

/// Construct and hydrate each exact Prompt Card once.
///
/// # Errors
///
/// Returns prompt conversion, hydration, or Python-allocation errors.
fn build_prompts(
    py: Python<'_>,
    state: &WyrdState,
) -> CardPyResult<BTreeMap<String, Py<PromptCard>>> {
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

/// Construct each exact Agent Card and resolve only referenced prompts.
/// Inline prompt payloads remain untouched.
///
/// # Errors
///
/// Returns agent conversion, prompt hydration, or Python-allocation errors.
fn build_agents(
    py: Python<'_>,
    state: &WyrdState,
) -> CardPyResult<BTreeMap<String, Py<PyAgentCard>>> {
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
                    runtime_hydration_error(state, key, "prompt", "agent prompt hydration failed")
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

/// Construct and eagerly load each exact Model Card from keyed artifacts.
///
/// # Errors
///
/// Returns interface, artifact, loader, state, or Python-allocation errors.
fn build_models(
    py: Python<'_>,
    state: &WyrdState,
    config: &PythonLoadConfig,
) -> CardPyResult<BTreeMap<String, Py<ModelCard>>> {
    let mut values = BTreeMap::new();
    for (key, envelope) in state.cards_of_kind(CardKind::Model) {
        let mut card = ModelCard::from_card(envelope.clone()).map_err(|_| {
            runtime_hydration_error(state, key, "interface", "model holder construction failed")
        })?;
        let interface = config.interface_by_ref.get(key).map(|value| value.bind(py));
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
            config.kwargs_by_ref.get(key).map(|value| value.bind(py)),
        )
        .map_err(|_| {
            runtime_hydration_error(state, key, "artifact_load", "model artifact load failed")
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

/// Construct and eagerly load each exact Data Card from keyed artifacts.
///
/// # Errors
///
/// Returns interface, artifact, loader, state, or Python-allocation errors.
fn build_data(
    py: Python<'_>,
    state: &WyrdState,
    config: &PythonLoadConfig,
) -> CardPyResult<BTreeMap<String, Py<DataCard>>> {
    let mut values = BTreeMap::new();
    for (key, envelope) in state.cards_of_kind(CardKind::Data) {
        let mut card = DataCard::from_card(envelope.clone()).map_err(|_| {
            runtime_hydration_error(state, key, "interface", "data holder construction failed")
        })?;
        let interface = config.interface_by_ref.get(key).map(|value| value.bind(py));
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
            config.kwargs_by_ref.get(key).map(|value| value.bind(py)),
        )
        .map_err(|_| {
            runtime_hydration_error(state, key, "artifact_load", "data artifact load failed")
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
/// model = cards.model.get(uid=reference.uid)
/// model.load()
/// ```
///
/// `get` retrieves and validates the serialized Card envelope. It does not
/// download model or data bytes. `ModelCard.load` and `DataCard.load` hydrate
/// those artifacts afterward.
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
    /// * `version_bump` - Version intent. Defaults to `VersionBump.Minor`.
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
        register_python_card(py, &self.inner, card, version_bump, save_args, None)
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

#[derive(Clone, Copy)]
struct RegistryListQuery<'a> {
    space: Option<&'a str>,
    name: Option<&'a str>,
    version_range: Option<&'a str>,
    status: Option<&'a str>,
    filter: Option<&'a str>,
    include_prerelease: bool,
    limit: Option<i32>,
    cursor: Option<&'a str>,
}

fn list_registry(
    py: Python<'_>,
    registry: &Cards,
    kind: CardKind,
    query: RegistryListQuery<'_>,
) -> CardPyResult<PyCardList> {
    let request = ListCardsRequest {
        kind: Some(kind),
        space: query.space.map(parse_space).transpose()?,
        name: query.name.map(parse_name).transpose()?,
        version_range: query.version_range.map(str::to_owned),
        status: query.status.map(parse_lifecycle_status).transpose()?,
        filter: query
            .filter
            .map(MetadataQuery::parse)
            .transpose()
            .map_err(WyrdPyError::from)?,
        include_prerelease: query.include_prerelease,
        limit: query.limit,
        cursor: query.cursor.map(str::to_owned),
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
    register_python_card(py, registry, card, version_bump, save_args, Some(kind))
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
    /// * `version_bump` - Version intent. Defaults to `VersionBump.Minor`.
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
    #[pyo3(signature = (*, space=None, name=None, version_range=None, status=None, filter=None, include_prerelease=false, limit=None, cursor=None))]
    #[allow(clippy::too_many_arguments)]
    fn list(
        &self,
        py: Python<'_>,
        space: Option<&str>,
        name: Option<&str>,
        version_range: Option<&str>,
        status: Option<&str>,
        filter: Option<&str>,
        include_prerelease: bool,
        limit: Option<i32>,
        cursor: Option<&str>,
    ) -> CardPyResult<PyCardList> {
        list_registry(
            py,
            &self.inner,
            CardKind::Data,
            RegistryListQuery {
                space,
                name,
                version_range,
                status,
                filter,
                include_prerelease,
                limit,
                cursor,
            },
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
    /// and optionally `version`.
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
    /// one. `get` returns the holder and does not download data bytes; call
    /// `DataCard.load` afterward.
    ///
    /// # Arguments
    /// * `uid` - Exact server-assigned UID. It takes precedence over the named
    ///   selector; supplied identity fields are checked against the result.
    /// * `space` - Card space, required when `uid` is omitted.
    /// * `name` - Card name, required when `uid` is omitted.
    /// * `version` - Exact version, or latest when omitted.
    /// * `interface` - Built-in or custom `DataInterface` instance/class used
    ///   to rebuild the Python interface from Card metadata.
    ///
    /// # Returns
    /// A native `DataCard` populated from server-stored Card JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, the envelope fails validation, or a required custom interface is
    /// missing.
    #[pyo3(signature = (*, uid=None, space=None, name=None, version=None, interface=None))]
    fn get(
        &self,
        py: Python<'_>,
        uid: Option<&str>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        interface: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<DataCard> {
        let selector = selector_for_kind(CardKind::Data, uid, space, name, version)?;
        let envelope = download_card(py, &self.inner, selector)?;
        let mut card = DataCard::from_card(envelope)?;
        card.hydrate_interface(py, interface)?;
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
    /// * `version_bump` - Version intent. Defaults to `VersionBump.Minor`.
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
    #[pyo3(signature = (*, space=None, name=None, version_range=None, status=None, filter=None, include_prerelease=false, limit=None, cursor=None))]
    #[allow(clippy::too_many_arguments)]
    fn list(
        &self,
        py: Python<'_>,
        space: Option<&str>,
        name: Option<&str>,
        version_range: Option<&str>,
        status: Option<&str>,
        filter: Option<&str>,
        include_prerelease: bool,
        limit: Option<i32>,
        cursor: Option<&str>,
    ) -> CardPyResult<PyCardList> {
        list_registry(
            py,
            &self.inner,
            CardKind::Model,
            RegistryListQuery {
                space,
                name,
                version_range,
                status,
                filter,
                include_prerelease,
                limit,
                cursor,
            },
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
    /// and optionally `version`.
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
    /// cannot rebuild one from built-in metadata. `get` does not download
    /// model bytes; call `ModelCard.load` afterward.
    ///
    /// # Arguments
    /// * `uid` - Exact server-assigned UID. It takes precedence over the named
    ///   selector.
    /// * `space` - Card space, required when `uid` is omitted.
    /// * `name` - Card name, required when `uid` is omitted.
    /// * `version` - Exact version, or latest when omitted.
    /// * `interface` - Built-in or custom `ModelInterface` instance/class used
    ///   to rebuild the Python interface from Card metadata.
    ///
    /// # Returns
    /// A native `ModelCard` populated from server-stored Card JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is not
    /// found, the envelope fails validation, or a required custom interface is
    /// missing.
    #[pyo3(signature = (*, uid=None, space=None, name=None, version=None, interface=None))]
    fn get(
        &self,
        py: Python<'_>,
        uid: Option<&str>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        interface: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<ModelCard> {
        let selector = selector_for_kind(CardKind::Model, uid, space, name, version)?;
        let envelope = download_card(py, &self.inner, selector)?;
        let mut card = ModelCard::from_card(envelope)?;
        card.hydrate_interface(py, interface)?;
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
    /// * `version_bump` - Version intent. Defaults to `VersionBump.Minor`.
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
    #[pyo3(signature = (*, space=None, name=None, version_range=None, status=None, filter=None, include_prerelease=false, limit=None, cursor=None))]
    #[allow(clippy::too_many_arguments)]
    fn list(
        &self,
        py: Python<'_>,
        space: Option<&str>,
        name: Option<&str>,
        version_range: Option<&str>,
        status: Option<&str>,
        filter: Option<&str>,
        include_prerelease: bool,
        limit: Option<i32>,
        cursor: Option<&str>,
    ) -> CardPyResult<PyCardList> {
        list_registry(
            py,
            &self.inner,
            CardKind::Prompt,
            RegistryListQuery {
                space,
                name,
                version_range,
                status,
                filter,
                include_prerelease,
                limit,
                cursor,
            },
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
    /// and optionally `version`.
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

struct PreparedPythonCard {
    input: wyrd_loader::RegistrationInput,
    tempdir: TempDir,
}

#[derive(serde::Deserialize)]
struct PythonCardEnvelope {
    #[serde(rename = "apiVersion")]
    api_version: ApiVersion,
    kind: CardKind,
    metadata: Metadata,
    spec: serde_json::Value,
}

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
    let bump = version_bump
        .map(parse_version_bump)
        .transpose()?
        .unwrap_or(VersionBump::Minor);
    let prepared = prepare_python_card(py, card, save_args, &kind, bump)?;
    let receipt = py
        .detach(|| {
            let _keep_alive = &prepared.tempdir;
            wyrd_runtime::runtime().block_on(registry.register(&prepared.input))
        })
        .map(PyRegistrationReceipt::from)
        .map_err(WyrdPyError::from)?;
    stamp_python_holder(card, &receipt.inner.root)?;
    Ok(receipt)
}

fn prepare_python_card(
    py: Python<'_>,
    card: &Bound<'_, PyAny>,
    save_args: Option<&Bound<'_, PyAny>>,
    kind: &CardKind,
    bump: VersionBump,
) -> CardPyResult<PreparedPythonCard> {
    let tempdir = tempdir().map_err(|error| WyrdPyError::Io(error.to_string()))?;
    let root = tempdir.path().to_path_buf();
    match kind {
        CardKind::Data => {
            let save_kwargs = extract_data_save_args(py, save_args)?;
            let mut holder = card
                .extract::<PyRefMut<'_, DataCard>>()
                .map_err(|error| WyrdPyError::Python(error.to_string()))?;
            holder.save(
                py,
                root.clone(),
                save_kwargs.as_ref().map(|value| value.bind(py)),
            )?;
        }
        CardKind::Model => {
            let save_kwargs = extract_model_save_args(py, save_args)?;
            let mut holder = card
                .extract::<PyRefMut<'_, ModelCard>>()
                .map_err(|error| WyrdPyError::Python(error.to_string()))?;
            holder.save(
                py,
                root.clone(),
                save_kwargs.as_ref().map(|value| value.bind(py)),
            )?;
        }
        CardKind::Prompt => {
            if save_args.is_some() {
                return Err(WyrdPyError::validation(
                    "PromptCard registration does not accept save_args",
                ));
            }
            let holder = card.extract::<PyRef<'_, PromptCard>>()?;
            holder.save(root.join("card.json"))?;
        }
        _ => {
            return Err(WyrdPyError::validation(
                "Cards.register supports DataCard, ModelCard, and PromptCard",
            ));
        }
    }

    let envelope_json = card
        .call_method0("_to_card_envelope_json")?
        .extract::<String>()?;
    let mut envelope: PythonCardEnvelope = serde_json::from_str(&envelope_json)?;
    if envelope.api_version.as_str() != ApiVersion::V1 || &envelope.kind != kind {
        return Err(WyrdPyError::validation(
            "card envelope kind or apiVersion does not match the native holder",
        ));
    }
    envelope.metadata.uid = None;
    envelope.metadata.spec_hash = None;
    envelope.metadata.artifact_hash = None;
    envelope.metadata.bump = Some(bump);

    let manifest = wyrd_loader::build_artifact_manifest(&root, "card.json")
        .map_err(|error| loader_manifest_error(&error))?;
    Ok(PreparedPythonCard {
        input: wyrd_loader::RegistrationInput {
            submissions: vec![wyrd_spec::registry::CardSubmission {
                api_version: envelope.api_version,
                kind: envelope.kind,
                metadata: envelope.metadata,
                spec: envelope.spec,
                artifacts: manifest.entries,
            }],
            artifact_sources: manifest.sources,
        },
        tempdir,
    })
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
