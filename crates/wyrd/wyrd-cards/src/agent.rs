//! Canonical Agent Card re-export and Python adapter.

pub use wyrd_spec::card::agent::AgentCard;

#[cfg(feature = "python")]
use {
    crate::card_ref::CardRefPy,
    crate::prompt::PromptReference,
    chrono::Utc,
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::PyAny,
    std::collections::BTreeMap,
    wyrd_interfaces::error::{CardPyResult, WyrdPyError},
    wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec},
    wyrd_spec::envelope::{Card, CardKind},
    wyrd_spec::metadata::{
        AnnotationKey, AnnotationValue, Annotations, LabelKey, LabelValue, Labels, MetadataError,
    },
    wyrd_spec::reference::InlineableRef,
};

/// Python adapter around the canonical native [`AgentCard`].
///
/// The native card remains the only owner of durable identity and spec state.
/// This adapter adds typed Python projections for the prompt reference and the
/// resolved runtime prompt without duplicating envelope conversion.
#[cfg(feature = "python")]
#[pyclass(module = "wyrd.cards.agent", name = "AgentCard", skip_from_py_object)]
pub struct PyAgentCard {
    inner: AgentCard,
    prompt_ref: Option<Py<PromptReference>>,
    prompt: Option<Py<skald_prompt::Prompt>>,
}

#[cfg(feature = "python")]
impl PyAgentCard {
    /// Build the Python adapter from a canonical native Agent Card.
    ///
    /// Inline prompts are projected immediately. Card-backed prompts remain
    /// unresolved until a service runtime supplies the corresponding Prompt
    /// Card.
    ///
    /// # Errors
    /// Returns a Python allocation error when a prompt projection cannot be
    /// created.
    pub fn from_native(py: Python<'_>, inner: AgentCard) -> CardPyResult<Self> {
        let prompt = inline_runtime_prompt(py, &inner.spec.prompt)?;
        let prompt_ref = Py::new(py, PromptReference::from_native(inner.spec.prompt.clone()))?;
        Ok(Self {
            inner,
            prompt_ref: Some(prompt_ref),
            prompt,
        })
    }

    /// Build the Python adapter from a shared Card envelope.
    ///
    /// # Errors
    /// Returns a Wyrd error when the envelope is not a complete registered
    /// Agent Card or a Python allocation fails.
    pub fn from_card(py: Python<'_>, card: Card) -> CardPyResult<Self> {
        let inner = AgentCard::from_envelope(card)?;
        if inner.uid.is_empty() {
            return Err(WyrdPyError::validation(
                "AgentCard persisted envelope is missing metadata.uid",
            ));
        }
        Self::from_native(py, inner)
    }

    /// Borrow the canonical native Agent Card.
    #[must_use]
    pub const fn native(&self) -> &AgentCard {
        &self.inner
    }

    /// Attach a resolved runtime prompt to a card-backed prompt reference.
    ///
    /// This is the local hydration seam used by service runtime owners. It does
    /// not alter the durable prompt reference stored in the Agent spec.
    ///
    /// # Errors
    /// Returns a Python allocation error.
    pub fn hydrate_resolved_prompt(
        &mut self,
        py: Python<'_>,
        prompt: skald_spec::Prompt,
    ) -> CardPyResult<()> {
        self.prompt = Some(Py::new(py, skald_prompt::Prompt::from_native(prompt))?);
        Ok(())
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PyAgentCard {
    /// Create an Agent Card from a Python `PromptReference`.
    #[new]
    #[pyo3(signature = (prompt, space=None, name=None, version=None, uid=None, labels=None, annotations=None))]
    // justification: pyo3 #[new] signature mirrors the complete Python AgentCard constructor
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        prompt: &Bound<'_, PromptReference>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        uid: Option<&str>,
        labels: Option<BTreeMap<String, String>>,
        annotations: Option<BTreeMap<String, String>>,
    ) -> CardPyResult<Self> {
        let py = prompt.py();
        let mut resolved_space = space.map(str::to_owned);
        let mut resolved_labels = labels_from_user(labels.unwrap_or_default())?;
        let mut resolved_annotations = annotations_from_user(annotations.unwrap_or_default())?;
        crate::identity::apply_repo_defaults(
            &CardKind::Agent,
            &mut resolved_space,
            &mut resolved_labels,
            &mut resolved_annotations,
        );
        let spec = AgentSpec {
            prompt: prompt.borrow().inner.clone(),
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec::default(),
            publishes_to: Vec::new(),
        };
        let inner = AgentCard {
            space: resolved_space.unwrap_or_else(|| "default".to_owned()),
            name: name.unwrap_or("agent").to_owned(),
            version: version.unwrap_or("0.1.0").to_owned(),
            uid: uid.map_or_else(wyrd_utils::uuid7, str::to_owned),
            labels: resolved_labels,
            annotations: resolved_annotations,
            cascade_children: spec.prompt.as_card_ref().cloned().into_iter().collect(),
            spec,
            created_at: Utc::now(),
        };
        inner.to_envelope().map_err(WyrdPyError::from)?;
        Self::from_native(py, inner)
    }

    /// Return the durable prompt reference.
    #[getter]
    pub fn prompt_ref(&self, py: Python<'_>) -> CardPyResult<Py<PromptReference>> {
        self.prompt_ref
            .as_ref()
            .map(|prompt_ref| prompt_ref.clone_ref(py))
            .ok_or_else(|| WyrdPyError::validation("AgentCard prompt reference was cleared"))
    }

    /// Return the resolved runtime prompt when available.
    #[getter]
    pub fn prompt(&self, py: Python<'_>) -> Option<Py<skald_prompt::Prompt>> {
        self.prompt.as_ref().map(|prompt| prompt.clone_ref(py))
    }

    /// Return this Agent Card as a Python dictionary.
    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(self.inner.to_envelope()?)?)
            .map_err(Into::into)
    }

    /// Return this Agent Card as a JSON envelope string.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner.to_envelope()?)?)
    }

    /// Hydrate an Agent Card from a complete JSON envelope.
    #[staticmethod]
    #[pyo3(name = "model_validate_json")]
    pub fn model_validate_json_py(py: Python<'_>, json_string: &str) -> CardPyResult<Self> {
        Self::from_card(py, serde_json::from_str(json_string)?)
    }

    /// Return the Agent Card space.
    #[getter]
    pub fn space(&self) -> &str {
        &self.inner.space
    }

    /// Return the Agent Card name.
    #[getter]
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// Return the Agent Card version.
    #[getter]
    pub fn version(&self) -> &str {
        &self.inner.version
    }

    /// Return the Agent Card UID.
    #[getter]
    pub fn uid(&self) -> &str {
        &self.inner.uid
    }

    /// Return queryable Agent Card labels.
    #[getter]
    pub fn labels(&self) -> BTreeMap<String, String> {
        labels_to_strings(&self.inner.labels)
    }

    /// Return free-form Agent Card annotations.
    #[getter]
    pub fn annotations(&self) -> BTreeMap<String, String> {
        annotations_to_strings(&self.inner.annotations)
    }

    /// Return the durable Agent spec as a Python dictionary.
    #[getter]
    pub fn spec(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(&self.inner.spec)?)
            .map_err(Into::into)
    }

    /// Return derived prompt cascade children.
    #[getter]
    pub fn cascade_children(&self) -> Vec<CardRefPy> {
        self.inner
            .cascade_children
            .iter()
            .cloned()
            .map(CardRefPy)
            .collect()
    }

    /// Return the Agent Card kind.
    #[getter]
    pub fn kind(&self) -> &'static str {
        "Agent"
    }

    /// Return the private registry conversion used by registration.
    #[pyo3(name = "_to_card_envelope_json")]
    pub fn to_card_envelope_json_py(&self) -> CardPyResult<String> {
        self.model_dump_json()
    }

    /// Return a readable Agent Card representation.
    pub fn __repr__(&self) -> String {
        format!(
            "AgentCard(name={:?}, version={:?}, space={:?})",
            self.inner.name, self.inner.version, self.inner.space
        )
    }

    // justification: PyO3 requires owned Python objects for the GC visitor
    #[allow(clippy::needless_pass_by_value)]
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(prompt_ref) = self.prompt_ref.as_ref() {
            visit.call(prompt_ref)?;
        }
        if let Some(prompt) = self.prompt.as_ref() {
            visit.call(prompt)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.prompt_ref = None;
        self.prompt = None;
    }
}

#[cfg(feature = "python")]
fn inline_runtime_prompt(
    py: Python<'_>,
    prompt_ref: &InlineableRef<skald_spec::Prompt>,
) -> CardPyResult<Option<Py<skald_prompt::Prompt>>> {
    match prompt_ref {
        InlineableRef::Inline(prompt) => Ok(Some(Py::new(
            py,
            skald_prompt::Prompt::from_native((**prompt).clone()),
        )?)),
        InlineableRef::Ref(_) | InlineableRef::Sibling { .. } | InlineableRef::Path(_) => Ok(None),
    }
}

#[cfg(feature = "python")]
fn labels_from_user(values: BTreeMap<String, String>) -> CardPyResult<Labels> {
    values
        .into_iter()
        .map(|(key, value)| {
            Ok((
                LabelKey::new_user(key).map_err(metadata_error)?,
                LabelValue::new_user(value).map_err(metadata_error)?,
            ))
        })
        .collect()
}

#[cfg(feature = "python")]
fn annotations_from_user(values: BTreeMap<String, String>) -> CardPyResult<Annotations> {
    values
        .into_iter()
        .map(|(key, value)| {
            Ok((
                AnnotationKey::new_user(key).map_err(metadata_error)?,
                AnnotationValue::new_user(value).map_err(metadata_error)?,
            ))
        })
        .collect()
}

#[cfg(feature = "python")]
fn labels_to_strings(values: &Labels) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[cfg(feature = "python")]
fn annotations_to_strings(values: &Annotations) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[cfg(feature = "python")]
fn metadata_error(error: MetadataError) -> WyrdPyError {
    WyrdPyError::validation_with_details(
        "invalid AgentCard metadata label or annotation",
        serde_json::json!({ "reason": error.to_string() }),
    )
}
