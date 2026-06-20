//! `PromptCard` local holder and Python boundary.

#[cfg(feature = "python")]
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wyrd_interfaces::error::CardPyResult;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::prompt::{PromptRef as NativePromptRef, PromptSpec};
use wyrd_spec::envelope::{
    Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec, SpecHash,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::CardRef;

use crate::identity::{card_name, optional_card_uid, space_name, validation_error, version_block};

/// PromptCard filesystem IO helpers.
pub mod io;

#[cfg(feature = "python")]
use {
    crate::card_ref::CardRefPy,
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::{PyAny, PyAnyMethods},
    std::path::PathBuf,
    wyrd_interfaces::error::WyrdPyError,
    wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, MetadataError},
};

/// Python-holder metadata accumulated by a local `PromptCard`.
///
/// This is not a registry record. It stores the native Skald prompt that will
/// be wrapped in a Wyrd `PromptSpec` when the holder is serialized.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.prompt", from_py_object))]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptCardMetadata {
    /// Native Skald prompt stored in the `PromptCard` spec body.
    pub prompt: skald_spec::Prompt,
}

/// Python-facing Wyrd prompt reference.
///
/// A prompt reference points at either a registered Prompt Card or an inline
/// Prompt spec.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.prompt", name = "PromptRef", skip_from_py_object)
)]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptRef {
    /// Wrapped native prompt reference.
    pub inner: NativePromptRef,
}

impl PromptRef {
    /// Wrap a native prompt reference.
    #[must_use]
    pub const fn from_native(inner: NativePromptRef) -> Self {
        Self { inner }
    }
}

impl PromptCardMetadata {
    /// Convert this holder metadata into a validated `PromptSpec`.
    ///
    /// # Errors
    /// Returns a prompt validation error when the native prompt violates the
    /// `PromptCard` envelope invariants.
    pub fn to_spec(&self) -> Result<PromptSpec, WyrdError> {
        PromptSpec::new(self.prompt.clone())
    }
}

/// Local Python-facing `PromptCard` holder.
///
/// A `PromptCard` owns local identity, holder metadata, and an optional live
/// Python `Prompt`. It can save and load local filesystem materialization, but
/// it never registers itself.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.prompt", skip_from_py_object)
)]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Serialize, Deserialize)]
pub struct PromptCard {
    /// `PromptCard` space.
    pub space: String,
    /// `PromptCard` name.
    pub name: String,
    /// `PromptCard` version.
    pub version: String,
    /// `PromptCard` UID.
    pub uid: String,
    /// Queryable local labels.
    pub labels: Labels,
    /// Free-form local annotations.
    pub annotations: Annotations,
    /// `PromptCard` holder metadata.
    pub metadata: PromptCardMetadata,
    /// Local creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Marker used by Python registry/client code to identify card holders.
    pub is_card: bool,
    /// Held live Python Prompt. This is skipped in serialized card JSON.
    #[cfg(feature = "python")]
    #[serde(skip)]
    pub prompt: Option<Py<PyAny>>,
}

impl PromptCard {
    /// Create a local `PromptCard` holder around a native prompt.
    #[must_use]
    pub fn from_native_prompt(prompt: skald_spec::Prompt) -> Self {
        Self {
            space: "default".to_owned(),
            name: "prompt".to_owned(),
            version: "0.1.0".to_owned(),
            uid: wyrd_utils::uuid7(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            metadata: PromptCardMetadata { prompt },
            created_at: Utc::now(),
            is_card: true,
            #[cfg(feature = "python")]
            prompt: None,
        }
    }

    /// Serialize this local holder as a Wyrd card-envelope JSON string.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation, identity validation, or
    /// JSON serialization fails.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.to_card()?)?)
    }

    /// Convert serialized holder metadata into a Rust card spec body.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation fails.
    pub fn to_rust_card_body_from_metadata(&self) -> Result<Spec, WyrdError> {
        Ok(Spec::Prompt(self.to_prompt_spec_from_metadata()?))
    }

    /// Convert serialized holder metadata into a pure Rust `PromptSpec`.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation fails.
    pub fn to_prompt_spec_from_metadata(&self) -> Result<PromptSpec, WyrdError> {
        self.metadata.to_spec()
    }

    /// Convert this holder into a Wyrd Prompt Card envelope.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation or identity validation
    /// fails.
    pub fn to_card(&self) -> Result<Card, WyrdError> {
        let spec = Spec::Prompt(self.to_prompt_spec_from_metadata()?);
        let spec_hash = spec.canonical_hash().map_err(|e| {
            validation_error(
                "PromptCard spec failed canonicalization",
                json!({ "source": e.to_string() }),
            )
        })?;
        Ok(Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Prompt,
            metadata: self.to_envelope_metadata(spec_hash)?,
            spec,
            relationships: Relationships::default(),
            status: None,
        })
    }

    /// Convert this holder identity into a Prompt Card reference.
    ///
    /// # Errors
    /// Returns a Wyrd error when identity fields are invalid.
    pub fn as_card_ref(&self) -> Result<CardRef, WyrdError> {
        Ok(CardRef {
            kind: CardKind::Prompt,
            name: card_name("name", &self.name)?,
            version: version_block(&self.version)?,
            space: space_name(&self.space)?,
            uid: optional_card_uid(&self.uid)?,
        })
    }

    /// Build a local holder from a Wyrd Prompt Card envelope.
    ///
    /// # Errors
    /// Returns a Wyrd error when the envelope is not a Prompt Card.
    pub fn from_card(card: Card) -> Result<Self, WyrdError> {
        if card.api_version.as_str() != ApiVersion::V1 || card.kind != CardKind::Prompt {
            return Err(validation_error(
                "PromptCard JSON must use apiVersion wyrd/v1 and kind Prompt",
                json!({
                    "apiVersion": card.api_version.as_str(),
                    "kind": card.kind.wire_name(),
                }),
            ));
        }

        let Spec::Prompt(spec) = card.spec else {
            return Err(validation_error(
                "PromptCard JSON spec must be a Prompt spec",
                json!({ "kind": card.kind.wire_name() }),
            ));
        };

        Ok(Self {
            space: card
                .metadata
                .space
                .as_ref()
                .map_or_else(|| "default".to_owned(), ToString::to_string),
            name: card.metadata.name.to_string(),
            version: card
                .metadata
                .resolved_pin()
                .map(ToString::to_string)
                .ok_or_else(|| {
                    validation_error(
                        "PromptCard envelope missing resolved version pin",
                        serde_json::Value::Null,
                    )
                })?,
            uid: card
                .metadata
                .uid
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            labels: card.metadata.labels,
            annotations: card.metadata.annotations,
            metadata: PromptCardMetadata {
                prompt: spec.prompt,
            },
            created_at: Utc::now(),
            is_card: true,
            #[cfg(feature = "python")]
            prompt: None,
        })
    }

    fn to_envelope_metadata(&self, spec_hash: SpecHash) -> Result<EnvelopeMetadata, WyrdError> {
        Ok(EnvelopeMetadata {
            name: card_name("name", &self.name)?,
            version: Some(version_block(&self.version)?.into()),
            bump: None,
            space: Some(space_name(&self.space)?),
            uid: optional_card_uid(&self.uid)?,
            labels: self.labels.clone(),
            annotations: self.annotations.clone(),
            spec_hash: Some(spec_hash),
            artifact_hash: None,
        })
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PromptRef {
    /// Build a `PromptRef` that points at a registered Prompt Card.
    ///
    /// # Errors
    /// Returns a Wyrd error when the card identity fields are invalid.
    #[staticmethod]
    #[pyo3(signature = (name, version, *, space, uid=None))]
    pub fn card(name: &str, version: &str, space: &str, uid: Option<&str>) -> CardPyResult<Self> {
        let card_ref = CardRef {
            kind: CardKind::Prompt,
            name: card_name("name", name)?,
            version: version_block(version)?,
            space: space_name(space)?,
            uid: uid.map_or(Ok(None), optional_card_uid)?,
        };
        Ok(Self::from_native(NativePromptRef::Card(card_ref)))
    }

    /// Build an inline `PromptRef` from a Python `Prompt`.
    ///
    /// # Errors
    /// Returns a Wyrd error when the prompt is invalid.
    #[staticmethod]
    pub fn inline(prompt: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        let spec = PromptSpec::new(native_prompt_from_py(prompt)?)?;
        Ok(Self::from_native(NativePromptRef::Inline(Box::new(spec))))
    }

    /// Return `card` or `inline`.
    #[getter]
    pub fn kind(&self) -> &'static str {
        match self.inner {
            NativePromptRef::Card(_) => "card",
            NativePromptRef::Inline(_) => "inline",
        }
    }

    /// Return this prompt reference as a Python dictionary.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON conversion fails.
    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(&self.inner)?)
            .map_err(Into::into)
    }

    /// Return this prompt reference as JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON serialization fails.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner)?)
    }

    /// Rebuild a `PromptRef` from JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON parsing or validation fails.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> CardPyResult<Self> {
        Ok(Self::from_native(serde_json::from_str(data)?))
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!("PromptRef(kind={:?})", self.kind())
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PromptCardMetadata {
    /// Create `PromptCard` metadata from an optional Python `Prompt`.
    ///
    /// # Errors
    /// Returns a Wyrd error when `prompt` or `model_settings` is invalid.
    #[new]
    #[pyo3(signature = (prompt=None, *, model_settings=None))]
    pub fn __new__(
        prompt: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<Self> {
        let prompt = prompt
            .map(native_prompt_from_py)
            .transpose()?
            .unwrap_or_else(default_prompt);
        let prompt = skald_prompt::apply_model_settings(&prompt, model_settings)?;
        Ok(Self { prompt })
    }

    /// Convert this metadata to a Python dictionary for inspection.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON conversion fails.
    pub fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(
            py,
            &serde_json::to_value(self).map_err(|error| WyrdPyError::Json(error.to_string()))?,
        )
        .map_err(Into::into)
    }

    /// Convert this metadata into a `PromptSpec` dictionary.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation or JSON conversion fails.
    #[pyo3(name = "to_spec")]
    pub fn to_spec_py(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let spec = PromptCardMetadata::to_spec(self)?;
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(spec)?).map_err(Into::into)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PromptCard {
    /// Create a local `PromptCard` from a Python `Prompt`.
    ///
    /// # Arguments
    /// * `prompt` - A `wyrd.prompt.Prompt` builder.
    /// * `space` - Optional card space. Defaults to `default`.
    /// * `name` - Optional card name. Defaults to `prompt`.
    /// * `version` - Optional semantic version. Defaults to `0.1.0`.
    /// * `uid` - Optional `UUIDv7` card UID. Defaults to a generated UID.
    /// * `labels` - Optional queryable labels.
    /// * `annotations` - Optional free-form annotations.
    /// * `metadata` - Optional holder metadata to seed before prompt capture.
    /// * `model_settings` - Optional provider-native generation settings.
    ///
    /// # Errors
    /// Returns a Wyrd error when the prompt, settings, or metadata labels are invalid.
    #[new]
    #[pyo3(signature = (prompt, space=None, name=None, version=None, uid=None, labels=None, annotations=None, metadata=None, model_settings=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        py: Python<'_>,
        prompt: &Bound<'_, PyAny>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        uid: Option<&str>,
        labels: Option<BTreeMap<String, String>>,
        annotations: Option<BTreeMap<String, String>>,
        metadata: Option<PromptCardMetadata>,
        model_settings: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<Self> {
        let mut metadata = metadata.unwrap_or(PromptCardMetadata {
            prompt: default_prompt(),
        });
        metadata.prompt =
            skald_prompt::apply_model_settings(&native_prompt_from_py(prompt)?, model_settings)?;
        let prompt = if model_settings.is_some_and(|value| !value.is_none()) {
            Some(skald_prompt::prompt_py(metadata.prompt.clone(), py)?)
        } else {
            Some(prompt.clone().unbind())
        };

        Ok(Self {
            space: space.unwrap_or("default").to_owned(),
            name: name.unwrap_or("prompt").to_owned(),
            version: version.unwrap_or("0.1.0").to_owned(),
            uid: uid.map_or_else(wyrd_utils::uuid7, str::to_owned),
            labels: labels_from_user(labels.unwrap_or_default())?,
            annotations: annotations_from_user(annotations.unwrap_or_default())?,
            metadata,
            created_at: Utc::now(),
            is_card: true,
            prompt,
        })
    }

    /// Return the held live prompt, if one is attached.
    #[getter]
    pub fn prompt(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        if let Some(prompt) = self.prompt.as_ref() {
            return Ok(prompt.clone_ref(py));
        }
        skald_prompt::prompt_py(self.metadata.prompt.clone(), py)
    }

    /// Replace the held live prompt and `PromptCard` metadata.
    ///
    /// # Errors
    /// Returns a Wyrd error when the value is not a `wyrd.prompt.Prompt`.
    #[setter]
    pub fn set_prompt(&mut self, prompt: &Bound<'_, PyAny>) -> CardPyResult<()> {
        self.metadata.prompt = native_prompt_from_py(prompt)?;
        self.prompt = Some(prompt.clone().unbind());
        Ok(())
    }

    /// Return typed provider generation settings, or `None` for raw prompts.
    #[getter]
    pub fn model_settings(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        skald_prompt::model_settings_py(&self.metadata.prompt, py)
    }

    /// Return the `PromptCard` space.
    #[getter]
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Set the `PromptCard` space.
    #[setter]
    pub fn set_space(&mut self, value: String) {
        self.space = value;
    }

    /// Return the `PromptCard` name.
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the `PromptCard` name.
    #[setter]
    pub fn set_name(&mut self, value: String) {
        self.name = value;
    }

    /// Return the `PromptCard` version.
    #[getter]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Set the `PromptCard` version.
    #[setter]
    pub fn set_version(&mut self, value: String) {
        self.version = value;
    }

    /// Return the `PromptCard` UID.
    #[getter]
    pub fn uid(&self) -> &str {
        &self.uid
    }

    /// Set the `PromptCard` UID.
    #[setter]
    pub fn set_uid(&mut self, value: String) {
        self.uid = value;
    }

    /// Return queryable `PromptCard` labels.
    #[getter]
    pub fn labels(&self) -> BTreeMap<String, String> {
        labels_to_strings(&self.labels)
    }

    /// Replace queryable `PromptCard` labels.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// labels.
    #[setter]
    pub fn set_labels(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.labels = labels_from_user(value)?;
        Ok(())
    }

    /// Return free-form `PromptCard` annotations.
    #[getter]
    pub fn annotations(&self) -> BTreeMap<String, String> {
        annotations_to_strings(&self.annotations)
    }

    /// Replace free-form `PromptCard` annotations.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// annotations.
    #[setter]
    pub fn set_annotations(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.annotations = annotations_from_user(value)?;
        Ok(())
    }

    /// Return the `PromptCard` holder metadata.
    #[getter]
    pub fn metadata(&self) -> PromptCardMetadata {
        self.metadata.clone()
    }

    /// Replace `PromptCard` holder metadata.
    #[setter]
    pub fn set_metadata(&mut self, value: PromptCardMetadata) {
        self.metadata = value;
        self.prompt = None;
    }

    /// Return the `PromptSpec` content hash.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation fails.
    #[getter]
    pub fn content_hash(&self) -> CardPyResult<String> {
        Ok(self.to_prompt_spec_from_metadata()?.content_hash())
    }

    /// Return declared text parameters.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation fails.
    #[getter]
    pub fn parameters(&self) -> CardPyResult<Vec<String>> {
        Ok(self
            .to_prompt_spec_from_metadata()?
            .parameters()
            .into_iter()
            .map(|name| name.to_string())
            .collect())
    }

    /// Return true when the prompt declares no text variables.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation fails.
    #[getter]
    pub fn is_fully_bound(&self) -> CardPyResult<bool> {
        Ok(self.to_prompt_spec_from_metadata()?.is_fully_bound())
    }

    /// Convert this `PromptCard`'s identity into a Wyrd `CardRef`.
    ///
    /// # Errors
    /// Returns a Wyrd validation error when identity fields fail newtype
    /// invariants.
    #[pyo3(name = "as_card_ref")]
    pub fn as_card_ref_py(&self) -> CardPyResult<CardRefPy> {
        self.as_card_ref().map(CardRefPy).map_err(Into::into)
    }

    /// Save this `PromptCard` envelope to a local JSON or YAML file.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation, serialization, or
    /// filesystem IO fails.
    #[wyrd_test_contract_macros::critical("python:PromptCard.save")]
    #[allow(clippy::needless_pass_by_value)]
    pub fn save(&self, path: PathBuf) -> CardPyResult<()> {
        Ok(io::write_card_file(&self.to_card()?, &path)?)
    }

    /// Load a local `PromptCard` envelope from a JSON or YAML file.
    ///
    /// Accepts both the stored native format and the declarative authoring
    /// format (when the `spec` has a `provider` key). Alias for `from_path`.
    ///
    /// # Errors
    /// Returns a Wyrd error when filesystem IO, parsing, or envelope validation
    /// fails.
    #[wyrd_test_contract_macros::critical("python:PromptCard.load")]
    #[staticmethod]
    #[allow(clippy::needless_pass_by_value)]
    pub fn load(py: Python<'_>, path: PathBuf) -> CardPyResult<Self> {
        let mut card = Self::from_card(io::read_card_file(&path)?)?;
        card.prompt = Some(skald_prompt::prompt_py(card.metadata.prompt.clone(), py)?);
        Ok(card)
    }

    /// Load a local `PromptCard` envelope from a JSON or YAML file.
    ///
    /// Accepts both the stored native format and the declarative authoring
    /// format (when the `spec` has a `provider` key). Preferred alias for
    /// `load`.
    ///
    /// # Errors
    /// Returns a Wyrd error when filesystem IO, parsing, or envelope validation
    /// fails.
    #[wyrd_test_contract_macros::critical("python:PromptCard.from_path")]
    #[staticmethod]
    #[pyo3(name = "from_path")]
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_path_py(py: Python<'_>, path: PathBuf) -> CardPyResult<Self> {
        let mut card = Self::from_card(io::read_card_file(&path)?)?;
        card.prompt = Some(skald_prompt::prompt_py(card.metadata.prompt.clone(), py)?);
        Ok(card)
    }

    /// Return this `PromptCard` as a JSON envelope string.
    ///
    /// # Errors
    /// Returns a Wyrd error when `PromptSpec` validation or serialization fails.
    #[wyrd_test_contract_macros::critical("python:PromptCard.model_dump_json")]
    #[pyo3(name = "model_dump_json")]
    pub fn model_dump_json_py(&self) -> CardPyResult<String> {
        self.model_dump_json()
    }

    /// Build a `PromptCard` from serialized Wyrd card-envelope JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON parsing or envelope validation fails.
    #[wyrd_test_contract_macros::critical("python:PromptCard.model_validate_json")]
    #[staticmethod]
    #[pyo3(name = "model_validate_json")]
    pub fn model_validate_json_py(py: Python<'_>, json_string: &str) -> CardPyResult<Self> {
        let mut card = Self::from_card(serde_json::from_str(json_string)?)?;
        card.prompt = Some(skald_prompt::prompt_py(card.metadata.prompt.clone(), py)?);
        Ok(card)
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "PromptCard(name={:?}, version={:?}, provider={:?}, model={:?})",
            self.name,
            self.version,
            self.metadata.prompt.request.provider(),
            self.metadata.prompt.model
        )
    }

    /// Return a pretty JSON string for interactive inspection.
    pub fn __str__(&self) -> String {
        match self.to_card() {
            Ok(card) => wyrd_utils::json::pretty_json_string(&card),
            Err(error) => format!("PromptCard(invalid={error})"),
        }
    }

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
fn native_prompt_from_py(prompt: &Bound<'_, PyAny>) -> CardPyResult<skald_spec::Prompt> {
    let prompt = prompt
        .extract::<PyRef<'_, skald_prompt::Prompt>>()
        .map_err(|_| WyrdPyError::validation("PromptCard requires a wyrd.prompt.Prompt"))?;
    Ok(prompt.native().clone())
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
        "invalid PromptCard metadata label or annotation",
        json!({ "reason": error.to_string() }),
    )
}

#[cfg(any(feature = "python", test))]
fn default_prompt() -> skald_spec::Prompt {
    let body = match serde_json::value::RawValue::from_string("{}".to_owned()) {
        Ok(body) => body,
        Err(error) => panic!("static raw JSON object is valid: {error}"),
    };
    match skald_spec::Prompt::new(
        skald_spec::ProviderRequest::RawV1 {
            provider: skald_spec::ProviderName::Custom("placeholder".to_owned()),
            body,
        },
        "placeholder",
        None,
        skald_spec::ResponseType::Text,
    ) {
        Ok(prompt) => prompt,
        Err(error) => panic!("static placeholder prompt is valid: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_holder_metadata_to_prompt_spec() {
        let prompt = skald_spec::Prompt::new(
            skald_spec::ProviderRequest::RawV1 {
                provider: skald_spec::ProviderName::Custom("unit".to_owned()),
                body: serde_json::value::RawValue::from_string(r#"{"messages":["hi"]}"#.to_owned())
                    .expect("static raw JSON is valid"),
            },
            "unit-model",
            None,
            skald_spec::ResponseType::Text,
        )
        .expect("static prompt is valid");

        let card = PromptCard::from_native_prompt(prompt.clone());

        let spec = card
            .to_prompt_spec_from_metadata()
            .expect("valid prompt converts to PromptSpec");
        assert_eq!(spec.prompt, prompt);
    }

    #[test]
    fn builds_prompt_card_ref_from_holder_identity() {
        let mut card = PromptCard::from_native_prompt(default_prompt());
        card.space = "growth".to_owned();
        card.name = "lead-scoring".to_owned();
        card.version = "1.2.3".to_owned();

        let card_ref = card.as_card_ref().expect("identity is valid");

        assert_eq!(card_ref.kind, CardKind::Prompt);
        assert_eq!(card_ref.name.to_string(), "lead-scoring");
        assert_eq!(card_ref.version.to_string(), "1.2.3");
        assert_eq!(card_ref.space.to_string(), "growth");
    }
}
