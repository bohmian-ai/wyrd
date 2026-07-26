//! `AgentCard` local holder and Python boundary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::CardRef;

use crate::identity::{card_name, optional_card_uid, space_name, validation_error, version_block};

#[cfg(feature = "python")]
use {
    crate::prompt::PromptReference,
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::PyAny,
    wyrd_interfaces::error::CardPyResult,
    wyrd_interfaces::error::WyrdPyError,
    wyrd_spec::card::agent::AgentRunConfigSpec,
};

/// Local Python-facing holder for a Wyrd Agent Card envelope.
///
/// The holder contains only authored identity, the durable `AgentSpec`, and a
/// live prompt-reference projection for Python callers. Runtime tools,
/// sessions, registries, storage clients, and artifact state remain outside
/// the holder in their owning runtime or client layers.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.cards.agent", skip_from_py_object)
)]
// justification: the Python projection is skipped during serde deserialization; the holder's native fields remain safe to deserialize
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Serialize, Deserialize)]
pub struct AgentCard {
    /// Agent Card space.
    pub space: String,
    /// Agent Card name.
    pub name: String,
    /// Agent Card version.
    pub version: String,
    /// Agent Card UID.
    pub uid: String,
    /// Queryable labels.
    pub labels: Labels,
    /// Free-form annotations.
    pub annotations: Annotations,
    /// Durable Agent Card specification.
    pub spec: AgentSpec,
    /// Prompt Card references derived from the agent specification.
    pub cascade_children: Vec<CardRef>,
    /// Local holder creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Live Python prompt reference, omitted from serialized card JSON.
    #[cfg(feature = "python")]
    #[serde(skip)]
    pub prompt: Option<Py<PromptReference>>,
}

impl AgentCard {
    /// Convert this holder into the shared Wyrd Card envelope.
    ///
    /// # Errors
    /// Returns a Wyrd error when identity or specification serialization fails.
    pub fn to_card(&self) -> Result<Card, WyrdError> {
        let spec = Spec::Agent(self.spec.clone());
        let spec_hash = spec.canonical_hash().map_err(|error| {
            validation_error(
                "AgentCard spec failed canonicalization",
                serde_json::json!({ "source": error.to_string() }),
            )
        })?;
        Ok(Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Agent,
            metadata: Metadata {
                name: card_name("name", &self.name)?,
                version: Some(version_block(&self.version)?.into()),
                bump: None,
                space: Some(space_name(&self.space)?),
                uid: optional_card_uid(&self.uid)?,
                labels: self.labels.clone(),
                annotations: self.annotations.clone(),
                spec_hash: Some(spec_hash),
                artifact_hash: None,
                origin: None,
            },
            spec,
            relationships: Relationships::default(),
            status: None,
        })
    }

    /// Convert this holder identity into an Agent Card reference.
    ///
    /// # Errors
    /// Returns a Wyrd error when identity fields are invalid.
    pub fn as_card_ref(&self) -> Result<CardRef, WyrdError> {
        Ok(CardRef {
            kind: CardKind::Agent,
            name: card_name("name", &self.name)?,
            version: version_block(&self.version)?,
            space: Some(space_name(&self.space)?),
            uid: optional_card_uid(&self.uid)?,
        })
    }

    /// Hydrate a holder directly from a server-returned Card envelope.
    ///
    /// The persisted envelope must be complete. In particular, space, UID,
    /// and the resolved version may not be replaced with authoring defaults.
    ///
    /// # Errors
    /// Returns a Wyrd error when the envelope is not a complete Agent Card.
    pub fn from_card(card: Card) -> Result<Self, WyrdError> {
        if card.api_version.as_str() != ApiVersion::V1 || card.kind != CardKind::Agent {
            return Err(validation_error(
                "AgentCard envelope must use apiVersion wyrd/v1 and kind Agent",
                serde_json::Value::Null,
            ));
        }
        let space = card.metadata.space.as_ref().ok_or_else(|| {
            validation_error(
                "AgentCard envelope missing space in metadata",
                serde_json::Value::Null,
            )
        })?;
        let uid = card.metadata.uid.as_ref().ok_or_else(|| {
            validation_error(
                "AgentCard envelope missing uid in metadata",
                serde_json::Value::Null,
            )
        })?;
        let version = card.metadata.resolved_pin().ok_or_else(|| {
            validation_error(
                "AgentCard envelope missing resolved version pin",
                serde_json::Value::Null,
            )
        })?;
        let Spec::Agent(spec) = card.spec else {
            return Err(validation_error(
                "AgentCard envelope spec must be an Agent spec",
                serde_json::Value::Null,
            ));
        };
        let cascade_children = spec.prompt.as_card_ref().cloned().into_iter().collect();

        Ok(Self {
            space: space.to_string(),
            name: card.metadata.name.to_string(),
            version: version.to_string(),
            uid: uid.to_string(),
            labels: card.metadata.labels,
            annotations: card.metadata.annotations,
            spec,
            cascade_children,
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            #[cfg(feature = "python")]
            prompt: None,
        })
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl AgentCard {
    /// Create an Agent Card from a Python `PromptReference`.
    ///
    /// `PromptReference.inline(prompt)` is the denovo authoring path for an
    /// inline prompt. Registered Prompt Cards can be supplied with
    /// `PromptReference.card(...)` and remain references in the Agent spec.
    #[new]
    #[pyo3(signature = (prompt, space=None, name=None, version=None, uid=None, labels=None, annotations=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        prompt: &Bound<'_, PromptReference>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        uid: Option<&str>,
        labels: Option<std::collections::BTreeMap<String, String>>,
        annotations: Option<std::collections::BTreeMap<String, String>>,
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
        let prompt_ref = prompt.borrow().inner.clone();
        let spec = AgentSpec {
            prompt: prompt_ref,
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec::default(),
            publishes_to: Vec::new(),
        };
        let mut card = Self {
            space: resolved_space.unwrap_or_else(|| "default".to_owned()),
            name: name.unwrap_or("agent").to_owned(),
            version: version.unwrap_or("0.1.0").to_owned(),
            uid: uid.map_or_else(wyrd_utils::uuid7, str::to_owned),
            labels: resolved_labels,
            annotations: resolved_annotations,
            cascade_children: spec.prompt.as_card_ref().cloned().into_iter().collect(),
            spec,
            created_at: Utc::now(),
            prompt: None,
        };
        card.hydrate_prompt(py)?;
        card.to_card().map_err(WyrdPyError::from)?;
        Ok(card)
    }

    /// Return the live Python prompt reference.
    #[getter]
    pub fn prompt(&self, py: Python<'_>) -> CardPyResult<Py<PromptReference>> {
        self.prompt
            .as_ref()
            .map(|prompt| prompt.clone_ref(py))
            .ok_or_else(|| WyrdPyError::validation("AgentCard prompt is not hydrated"))
    }

    /// Hydrate the Python prompt reference from the native Agent spec.
    pub fn hydrate_prompt(&mut self, py: Python<'_>) -> CardPyResult<()> {
        self.prompt = Some(Py::new(
            py,
            PromptReference::from_native(self.spec.prompt.clone()),
        )?);
        Ok(())
    }

    /// Return this Agent Card as a Python dictionary.
    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(&self.to_card()?)?)
            .map_err(Into::into)
    }

    /// Return this Agent Card as a JSON envelope string.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.to_card()?)?)
    }

    /// Hydrate an Agent Card from a complete JSON envelope.
    #[staticmethod]
    #[pyo3(name = "model_validate_json")]
    pub fn model_validate_json_py(py: Python<'_>, json_string: &str) -> CardPyResult<Self> {
        let mut card = Self::from_card(serde_json::from_str(json_string)?)?;
        card.hydrate_prompt(py)?;
        Ok(card)
    }

    /// Return the Agent Card space.
    #[getter]
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Return the Agent Card name.
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the Agent Card version.
    #[getter]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Return the Agent Card UID.
    #[getter]
    pub fn uid(&self) -> &str {
        &self.uid
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
            self.name, self.version, self.space
        )
    }

    // justification: pyo3 boundary; the extractor produces an owned Python object and the visitor is required for the pyclass GC protocol
    #[allow(clippy::needless_pass_by_value)]
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(prompt) = self.prompt.as_ref() {
            visit.call(prompt)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.prompt = None;
    }
}

#[cfg(feature = "python")]
fn labels_from_user(values: std::collections::BTreeMap<String, String>) -> CardPyResult<Labels> {
    values
        .into_iter()
        .map(|(key, value)| {
            Ok((
                wyrd_spec::metadata::LabelKey::new_user(key)
                    .map_err(|error| WyrdPyError::validation(error.to_string()))?,
                wyrd_spec::metadata::LabelValue::new_user(value)
                    .map_err(|error| WyrdPyError::validation(error.to_string()))?,
            ))
        })
        .collect()
}

#[cfg(feature = "python")]
fn annotations_from_user(
    values: std::collections::BTreeMap<String, String>,
) -> CardPyResult<Annotations> {
    values
        .into_iter()
        .map(|(key, value)| {
            Ok((
                wyrd_spec::metadata::AnnotationKey::new_user(key)
                    .map_err(|error| WyrdPyError::validation(error.to_string()))?,
                wyrd_spec::metadata::AnnotationValue::new_user(value)
                    .map_err(|error| WyrdPyError::validation(error.to_string()))?,
            ))
        })
        .collect()
}
