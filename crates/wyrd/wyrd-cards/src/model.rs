//! `ModelCard` local holder and Python boundary.

use std::collections::BTreeMap;
#[cfg(feature = "python")]
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_interfaces::error::CardPyResult;
use wyrd_spec::card::model::{
    CustomMeta as CustomModelMeta, ModelInterface as RustModelInterface,
    ModelSignature as RustModelSignature, ModelSpec, SampleInput as RustSampleInput, TaskType,
};
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::ApiVersion;

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::{PyAny, PyDict, PyType, PyTypeMethods},
    std::path::PathBuf,
    wyrd_interfaces::data::dtype::{extract_pathbuf, is_path_like},
    wyrd_interfaces::error::WyrdPyError,
    wyrd_interfaces::model::interfaces::{
        CatboostInterface as ModelCatboostInterface,
        HuggingfaceInterface as ModelHuggingfaceInterface,
        LightgbmInterface as ModelLightgbmInterface, LightningInterface as ModelLightningInterface,
        ModelInterface, ModelInterfaceHandle, SklearnInterface as ModelSklearnInterface,
        TensorflowInterface as ModelTensorflowInterface, TorchInterface as ModelTorchInterface,
        XgboostInterface as ModelXgboostInterface, parse_task_type,
    },
    wyrd_interfaces::model::io::{load_model, save_model},
    wyrd_interfaces::model::sample::SampleInput,
    wyrd_interfaces::model::signature::ModelSignature,
    wyrd_spec::card::model::{
        HuggingFaceTask, HuggingfaceMeta, TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat,
    },
    wyrd_spec::envelope::Metadata as EnvelopeMetadata,
    wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, MetadataError},
    wyrd_utils::py::pyobject_to_json,
};

/// Python-holder metadata accumulated by a local `ModelCard`.
///
/// This is not the durable registry record. It is converted into `ModelSpec`
/// when a Rust-only spec body is needed.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.model", from_py_object))]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCardMetadata {
    /// Interface metadata that will be written into the `ModelSpec`.
    pub interface: RustModelInterface,
    /// Declared model task type.
    pub task_type: TaskType,
    /// Typed input/output signature.
    pub signature: RustModelSignature,
    /// Optional sample input descriptor.
    pub sample_input: Option<RustSampleInput>,
    /// Existing durable Artifact card references for this model card.
    pub card_refs: Vec<CardRef>,
}

impl Default for ModelCardMetadata {
    fn default() -> Self {
        Self {
            // Keep this sentinel in sync with `is_default_custom_interface`.
            interface: RustModelInterface::Custom(CustomModelMeta {
                framework_version: String::new(),
                model_subtype: None,
                loader_module: String::new(),
                loader_class: String::new(),
                extra: BTreeMap::new(),
            }),
            task_type: TaskType::Other,
            signature: RustModelSignature::new(Vec::new(), Vec::new()),
            sample_input: None,
            card_refs: Vec::new(),
        }
    }
}

/// Local Python-facing `ModelCard` holder.
///
/// A `ModelCard` owns local identity, holder metadata, and an optional live
/// Python model interface. It can save and load local filesystem
/// materialization, but it never registers itself and never creates Artifact
/// cards.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", skip_from_py_object)
)]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Serialize, Deserialize)]
pub struct ModelCard {
    /// `ModelCard` space.
    pub space: String,
    /// `ModelCard` name.
    pub name: String,
    /// `ModelCard` version.
    pub version: String,
    /// `ModelCard` UID.
    pub uid: String,
    /// Queryable local labels.
    pub labels: Labels,
    /// Free-form local annotations.
    pub annotations: Annotations,
    /// `ModelCard` holder metadata.
    pub metadata: ModelCardMetadata,
    /// Local holder creation timestamp.
    ///
    /// This is construction state for the in-process holder, not part of the
    /// durable Wyrd Card envelope emitted by `model_dump_json`.
    pub created_at: DateTime<Utc>,
    /// Marker used by Python registry/client code to identify card holders.
    pub is_card: bool,
    /// Held live Python interface. This is skipped in serialized card JSON.
    #[cfg(feature = "python")]
    #[serde(skip)]
    pub interface: Option<Py<PyAny>>,
}

impl ModelCard {
    /// Write only the serialized `ModelCard` JSON under `path/card.json`.
    ///
    /// # Errors
    /// Returns a Wyrd error when local filesystem writes or JSON serialization
    /// fail.
    #[cfg(feature = "python")]
    fn write_card_json(&self, path: &Path) -> CardPyResult<()> {
        write_model_card_json_file(self, path)
    }

    /// Serialize this local holder as JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization fails.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.to_card_envelope())?)
    }

    /// Convert serialized holder metadata into the Rust card spec body.
    #[must_use]
    pub fn to_rust_card_body_from_metadata(&self) -> Spec {
        Spec::Model(self.to_model_spec_from_metadata())
    }

    /// Convert serialized holder metadata into a pure Rust `ModelSpec`.
    #[must_use]
    pub fn to_model_spec_from_metadata(&self) -> ModelSpec {
        model_spec_from_metadata(&self.metadata, self.metadata.interface.clone())
    }

    /// Convert this holder identity into a Model Card reference.
    ///
    /// # Errors
    /// Returns a Wyrd error when identity fields are invalid.
    pub fn as_card_ref(&self) -> Result<CardRef, WyrdError> {
        use crate::identity::{card_name, optional_card_uid, optional_space_name, version_block};

        Ok(CardRef {
            kind: CardKind::Model,
            name: card_name("name", &self.name)?,
            version: version_block(&self.version)?,
            space: optional_space_name(&self.space)?,
            uid: optional_card_uid(&self.uid)?,
        })
    }

    fn to_card_envelope(&self) -> ModelCardEnvelope<'_> {
        ModelCardEnvelope {
            api_version: ApiVersion::V1,
            kind: "Model",
            metadata: ModelCardEnvelopeMetadata {
                name: &self.name,
                version: &self.version,
                space: &self.space,
                uid: &self.uid,
                labels: &self.labels,
                annotations: &self.annotations,
            },
            spec: self.to_model_spec_from_metadata(),
            relationships: Vec::new(),
            status: None,
        }
    }
}

#[derive(Serialize)]
struct ModelCardEnvelope<'a> {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: ModelCardEnvelopeMetadata<'a>,
    spec: ModelSpec,
    relationships: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ModelCardEnvelopeMetadata<'a> {
    name: &'a str,
    version: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    space: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    uid: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    labels: &'a Labels,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    annotations: &'a Annotations,
}

#[cfg(feature = "python")]
#[derive(Deserialize)]
struct SerializedModelCardEnvelope {
    #[serde(rename = "apiVersion")]
    api_version: ApiVersion,
    kind: CardKind,
    metadata: EnvelopeMetadata,
    spec: ModelSpec,
}

#[cfg(feature = "python")]
#[pymethods]
impl ModelCardMetadata {
    /// Create `ModelCard` holder metadata from Python values.
    ///
    /// # Errors
    /// Returns a Wyrd error when task type, signature, sample input, interface,
    /// or artifact reference values cannot be parsed into the Rust spec shape.
    #[new]
    #[pyo3(signature = (*, interface=None, task_type="other", signature=None, sample_input=None, card_refs=None))]
    pub fn __new__(
        py: Python<'_>,
        interface: Option<&Bound<'_, PyAny>>,
        task_type: &str,
        signature: Option<&Bound<'_, PyAny>>,
        sample_input: Option<&Bound<'_, PyAny>>,
        card_refs: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<Self> {
        Ok(Self {
            interface: parse_metadata_interface(py, interface)?,
            task_type: parse_task_type(task_type)?,
            signature: parse_metadata_signature(signature)?,
            sample_input: parse_metadata_sample_input(sample_input)?,
            card_refs: parse_metadata_card_refs(card_refs)?,
        })
    }

    /// Return this metadata as a Python-serializable dict for inspection.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON conversion fails.
    pub fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(
            py,
            &serde_json::to_value(self).map_err(|e| WyrdPyError::Io(e.to_string()))?,
        )
        .map_err(Into::into)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl ModelCard {
    /// Create a local `ModelCard` from a raw model or model interface.
    ///
    /// # Errors
    /// Returns a Wyrd error when input classification, interface metadata
    /// conversion, or model spec validation fails.
    #[new]
    #[pyo3(signature = (model_or_interface, space=None, name=None, version=None, uid=None, labels=None, annotations=None, metadata=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        model_or_interface: &Bound<'_, PyAny>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        uid: Option<&str>,
        labels: Option<BTreeMap<String, String>>,
        annotations: Option<BTreeMap<String, String>>,
        metadata: Option<ModelCardMetadata>,
    ) -> CardPyResult<Self> {
        let py = model_or_interface.py();
        let mut metadata = metadata.unwrap_or_default();
        let handle =
            ModelCardInput::extract_bound(model_or_interface, false)?.into_handle(py, &metadata)?;
        metadata.interface = handle.to_spec_interface(py)?;

        let card = Self {
            space: space.unwrap_or("default").to_owned(),
            name: name.unwrap_or("model").to_owned(),
            version: version.unwrap_or("0.1.0").to_owned(),
            uid: uid.map_or_else(wyrd_utils::uuid7, str::to_owned),
            labels: labels_from_user(labels.unwrap_or_default())?,
            annotations: annotations_from_user(annotations.unwrap_or_default())?,
            metadata,
            created_at: utc_now(),
            is_card: true,
            interface: Some(handle.into_py_any(py)?),
        };
        card.to_model_spec_from_metadata().validate()?;
        Ok(card)
    }

    /// Return the held live interface, if one is attached.
    #[getter]
    pub fn interface(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.interface
            .as_ref()
            .map(|interface| interface.clone_ref(py))
    }

    /// Replace the held live model interface.
    ///
    /// # Errors
    /// Returns a Wyrd error when the value is not a supported model interface
    /// or raw model object.
    #[setter]
    pub fn set_interface(
        &mut self,
        py: Python<'_>,
        interface: &Bound<'_, PyAny>,
    ) -> CardPyResult<()> {
        let handle =
            ModelCardInput::extract_bound(interface, false)?.into_handle(py, &self.metadata)?;
        self.metadata.interface = handle.to_spec_interface(py)?;
        self.interface = Some(handle.into_py_any(py)?);
        self.to_model_spec_from_metadata().validate()?;
        Ok(())
    }

    /// Return the `ModelCard` space.
    #[getter]
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Set the `ModelCard` space.
    #[setter]
    pub fn set_space(&mut self, value: String) {
        self.space = value;
    }

    /// Return the `ModelCard` name.
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the `ModelCard` name.
    #[setter]
    pub fn set_name(&mut self, value: String) {
        self.name = value;
    }

    /// Return the `ModelCard` version.
    #[getter]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Set the `ModelCard` version.
    #[setter]
    pub fn set_version(&mut self, value: String) {
        self.version = value;
    }

    /// Return the `ModelCard` UID.
    #[getter]
    pub fn uid(&self) -> &str {
        &self.uid
    }

    /// Convert this ModelCard's identity into a Wyrd CardRef.
    ///
    /// # Errors
    /// Returns a Wyrd validation error when identity fields fail newtype
    /// invariants.
    #[pyo3(name = "as_card_ref")]
    pub fn as_card_ref_py(&self) -> CardPyResult<CardRef> {
        self.as_card_ref().map_err(Into::into)
    }

    /// Set the `ModelCard` UID.
    #[setter]
    pub fn set_uid(&mut self, value: String) {
        self.uid = value;
    }

    /// Return queryable `ModelCard` labels.
    #[getter]
    pub fn labels(&self) -> BTreeMap<String, String> {
        labels_to_strings(&self.labels)
    }

    /// Replace queryable `ModelCard` labels.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// labels.
    #[setter]
    pub fn set_labels(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.labels = labels_from_user(value)?;
        Ok(())
    }

    /// Return free-form `ModelCard` annotations.
    #[getter]
    pub fn annotations(&self) -> BTreeMap<String, String> {
        annotations_to_strings(&self.annotations)
    }

    /// Replace free-form `ModelCard` annotations.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// annotations.
    #[setter]
    pub fn set_annotations(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.annotations = annotations_from_user(value)?;
        Ok(())
    }

    /// Return `ModelCard` holder metadata.
    #[getter]
    pub fn metadata(&self) -> ModelCardMetadata {
        self.metadata.clone()
    }

    /// Return the canonical model task type token.
    #[getter]
    pub fn task_type(&self) -> &'static str {
        task_type_token(self.metadata.task_type)
    }

    /// Replace `ModelCard` holder metadata.
    ///
    /// # Errors
    /// Returns a Wyrd error when the replacement metadata is not a valid
    /// `ModelSpec`.
    #[setter]
    pub fn set_metadata(&mut self, value: ModelCardMetadata) -> CardPyResult<()> {
        self.metadata = value;
        self.to_model_spec_from_metadata().validate()?;
        Ok(())
    }

    /// Return the typed model signature.
    ///
    /// # Errors
    /// Returns a Python allocation error if the wrapper cannot be created.
    #[getter]
    pub fn signature(&self, py: Python<'_>) -> CardPyResult<Py<ModelSignature>> {
        Ok(Py::new(
            py,
            ModelSignature::from_inner(self.metadata.signature.clone()),
        )?)
    }

    /// Return the optional sample input descriptor.
    ///
    /// # Errors
    /// Returns a Python allocation error if the wrapper cannot be created.
    #[getter]
    pub fn sample_input(&self, py: Python<'_>) -> CardPyResult<Option<Py<SampleInput>>> {
        self.metadata
            .sample_input
            .as_ref()
            .map(|sample| Py::new(py, SampleInput::from_inner(sample)).map_err(Into::into))
            .transpose()
    }

    /// Save local model artifacts and the card JSON under `path`.
    ///
    /// # Errors
    /// Returns a Wyrd error when no interface is attached, interface save
    /// fails, or card JSON cannot be written.
    #[wyrd_test_contract_macros::critical("python:ModelCard.save")]
    #[pyo3(signature = (path, save_kwargs=None))]
    #[allow(clippy::needless_pass_by_value)]
    pub fn save(
        &mut self,
        py: Python<'_>,
        path: PathBuf,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let interface = self.interface.as_ref().ok_or_else(|| {
            WyrdPyError::model_validation("ModelCard interface is required for local save")
        })?;
        let bound = interface.bind(py);
        self.metadata.interface =
            ModelInterfaceHandle::from_interface(bound)?.to_spec_interface(py)?;
        self.to_model_spec_from_metadata().validate()?;
        save_model(bound, &path, save_kwargs)?;
        self.write_card_json(&path)
    }

    /// Load local model artifacts through the held interface.
    ///
    /// # Errors
    /// Returns a Wyrd error when no interface is attached or interface load
    /// fails.
    #[wyrd_test_contract_macros::critical("python:ModelCard.load")]
    #[pyo3(signature = (path=None, load_kwargs=None))]
    #[allow(clippy::needless_pass_by_value)]
    pub fn load(
        &mut self,
        py: Python<'_>,
        path: Option<PathBuf>,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let interface = self.interface.as_ref().ok_or_else(|| {
            WyrdPyError::model_validation("ModelCard interface is required for local load")
        })?;
        load_model(interface.bind(py), path, load_kwargs)
    }

    /// Return this `ModelCard` as a JSON string.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization fails.
    #[wyrd_test_contract_macros::critical("python:ModelCard.model_dump_json")]
    #[pyo3(name = "model_dump_json")]
    pub fn model_dump_json_py(&self) -> CardPyResult<String> {
        self.model_dump_json()
    }

    /// Return a pretty JSON representation for interactive inspection.
    pub fn __str__(&self) -> String {
        wyrd_utils::json::pretty_json_string(&self.to_card_envelope())
    }

    /// Build a `ModelCard` from serialized JSON and an optional interface hook.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON parsing fails or the serialized custom
    /// interface cannot be rebuilt without an explicit Python object.
    #[wyrd_test_contract_macros::critical("python:ModelCard.model_validate_json")]
    #[staticmethod]
    #[pyo3(name = "model_validate_json", signature = (json_string, interface=None))]
    pub fn model_validate_json_py(
        py: Python<'_>,
        json_string: &str,
        interface: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<Self> {
        let mut card = Self::from_card_json(json_string)?;
        card.attach_model(py, interface)?;
        Ok(card)
    }

    #[allow(clippy::needless_pass_by_value)]
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(interface) = self.interface.as_ref() {
            visit.call(interface)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.interface = None;
    }
}

#[cfg(feature = "python")]
impl ModelCard {
    /// Convert this local holder into the Rust card spec body.
    ///
    /// # Errors
    /// Returns a Wyrd error when interface conversion or `ModelSpec` validation
    /// fails.
    pub fn to_rust_card_body(&self, py: Python<'_>) -> CardPyResult<Spec> {
        Ok(Spec::Model(self.to_model_spec(py)?))
    }

    /// Convert this local holder into a pure Rust `ModelSpec`.
    ///
    /// # Errors
    /// Returns a Wyrd error when interface conversion or `ModelSpec` validation
    /// fails.
    pub fn to_model_spec(&self, py: Python<'_>) -> CardPyResult<ModelSpec> {
        let interface = self
            .interface
            .as_ref()
            .map(|interface| {
                let handle = ModelInterfaceHandle::from_interface(interface.bind(py))?;
                handle.to_spec_interface(py)
            })
            .transpose()?
            .unwrap_or_else(|| self.metadata.interface.clone());

        let spec = model_spec_from_metadata(&self.metadata, interface);
        spec.validate()?;
        Ok(spec)
    }

    fn attach_model(
        &mut self,
        py: Python<'_>,
        interface: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<()> {
        if let Some(interface) = interface {
            let handle =
                ModelCardInput::extract_bound(interface, true)?.into_handle(py, &self.metadata)?;
            self.metadata.interface = handle.to_spec_interface(py)?;
            self.interface = Some(handle.into_py_any(py)?);
            self.to_model_spec_from_metadata().validate()?;
            return Ok(());
        }

        self.to_model_spec_from_metadata().validate()?;
        self.interface = Some(interface_from_model_spec(py, &self.metadata.interface)?);
        Ok(())
    }

    fn from_card_json(json_string: &str) -> CardPyResult<Self> {
        if let Ok(envelope) = serde_json::from_str::<SerializedModelCardEnvelope>(json_string) {
            if envelope.api_version.as_str() != ApiVersion::V1 || envelope.kind != CardKind::Model {
                return Err(WyrdPyError::model_validation(
                    "ModelCard JSON must use apiVersion wyrd/v1 and kind Model",
                ));
            }
            return Ok(Self {
                space: envelope
                    .metadata
                    .space
                    .as_ref()
                    .map_or_else(|| "default".to_string(), ToString::to_string),
                name: envelope.metadata.name.to_string(),
                version: envelope.metadata.version.to_string(),
                uid: envelope
                    .metadata
                    .uid
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                labels: envelope.metadata.labels,
                annotations: envelope.metadata.annotations,
                metadata: ModelCardMetadata {
                    interface: envelope.spec.interface,
                    task_type: envelope.spec.task_type,
                    signature: envelope.spec.signature,
                    sample_input: envelope.spec.sample_input,
                    card_refs: envelope.spec.card_refs,
                },
                created_at: utc_now(),
                is_card: true,
                interface: None,
            });
        }

        Err(WyrdPyError::model_validation(
            "ModelCard JSON must use the Wyrd envelope: apiVersion wyrd/v1, kind Model, metadata, spec",
        ))
    }
}

#[cfg(feature = "python")]
enum ModelCardInput {
    /// Local model artifact path using one of Wyrd's model save layouts.
    ArtifactPath(PathBuf),
    /// Raw framework model object to auto-detect.
    Raw(Py<PyAny>),
    /// Initialized model interface object.
    Interface(ModelInterfaceHandle),
    /// Python subclass used only when rehydrating custom JSON metadata.
    InterfaceClass(Py<PyAny>),
}

#[cfg(feature = "python")]
impl ModelCardInput {
    fn extract_bound(
        model_or_interface: &Bound<'_, PyAny>,
        allow_interface_class: bool,
    ) -> CardPyResult<Self> {
        if is_path_like(model_or_interface.py(), model_or_interface)? {
            return Ok(Self::ArtifactPath(extract_pathbuf(model_or_interface)?));
        }

        if model_or_interface.is_instance_of::<ModelInterface>() {
            return Ok(Self::Interface(ModelInterfaceHandle::from_interface(
                model_or_interface,
            )?));
        }

        if let Ok(interface_type) = model_or_interface.cast::<PyType>() {
            if interface_type.is_subclass_of::<ModelInterface>()? {
                if !allow_interface_class {
                    return Err(WyrdPyError::model_validation(
                        "ModelCard construction requires a raw model or initialized ModelInterface instance; pass interface classes to retrieval surfaces such as cards.get(..., interface=YourInterface)",
                    ));
                }
                return Ok(Self::InterfaceClass(model_or_interface.clone().unbind()));
            }
        }

        Ok(Self::Raw(model_or_interface.clone().unbind()))
    }

    fn into_handle(
        self,
        py: Python<'_>,
        metadata: &ModelCardMetadata,
    ) -> CardPyResult<ModelInterfaceHandle> {
        match self {
            Self::ArtifactPath(path) => load_handle_from_artifact_path(py, &path, metadata),
            Self::Raw(model) => ModelInterfaceHandle::from_raw(py, model.bind(py)),
            Self::Interface(interface) => Ok(interface),
            Self::InterfaceClass(interface_class) => {
                let interface = interface_class
                    .bind(py)
                    .call_method1("from_metadata", (metadata.clone(),))?;
                ModelInterfaceHandle::from_interface(&interface)
            }
        }
    }
}

#[cfg(feature = "python")]
/// Build a live model interface by loading a Wyrd model artifact path.
///
/// The path may be either a Wyrd model materialization root or one of the
/// convention filenames/directories under that root. Signature metadata still
/// comes from the caller because Wyrd cannot infer a model signature from model
/// bytes safely.
///
/// # Errors
/// Returns a Wyrd error when the path cannot be mapped to a supported
/// interface, the interface requires extra live state, or local load fails.
fn load_handle_from_artifact_path(
    py: Python<'_>,
    path: &Path,
    metadata: &ModelCardMetadata,
) -> CardPyResult<ModelInterfaceHandle> {
    let root = model_artifact_root(path);
    let interface = model_interface_for_artifact_path(py, path, metadata)?;
    let py_interface = interface_from_model_spec(py, &interface)?;
    load_model(py_interface.bind(py), Some(root), None)?;
    ModelInterfaceHandle::from_interface(py_interface.bind(py))
}

#[cfg(feature = "python")]
/// Select the model interface metadata to use for a local artifact path.
///
/// Explicit `metadata.interface` wins for ambiguous layouts such as
/// `model.joblib`, which is shared by several tree/tabular frameworks.
/// Otherwise this function infers only layouts that are unambiguous and can be
/// loaded without an extra Python class or model instance.
///
/// # Errors
/// Returns a Wyrd validation error for ambiguous or stateful layouts and an
/// unknown-model-type error for paths that do not match a Wyrd model artifact
/// convention.
fn model_interface_for_artifact_path(
    py: Python<'_>,
    path: &Path,
    metadata: &ModelCardMetadata,
) -> CardPyResult<RustModelInterface> {
    if !is_default_custom_interface(&metadata.interface) {
        return Ok(metadata.interface.clone());
    }

    let root = model_artifact_root(path);
    if path.ends_with("model.pt") || root.join("model.pt").is_file() {
        return Ok(RustModelInterface::Torch(TorchMeta {
            framework_version: module_version_or_unknown(py, "torch"),
            model_subtype: None,
            save_format: TorchSaveFormat::Pickle,
        }));
    }
    if path.ends_with("model.keras") || root.join("model.keras").is_file() {
        return Ok(RustModelInterface::Tensorflow(TensorflowMeta {
            framework_version: module_version_or_unknown(py, "tensorflow"),
            model_subtype: None,
            save_format: TfSaveFormat::Keras,
        }));
    }
    if path.ends_with("savedmodel") || root.join("savedmodel").is_dir() {
        return Ok(RustModelInterface::Tensorflow(TensorflowMeta {
            framework_version: module_version_or_unknown(py, "tensorflow"),
            model_subtype: None,
            save_format: TfSaveFormat::SavedModel,
        }));
    }
    if path.ends_with("model") || root.join("model").join("config.json").is_file() {
        return Ok(RustModelInterface::Huggingface(HuggingfaceMeta {
            framework_version: module_version_or_unknown(py, "transformers"),
            model_subtype: None,
            hf_task: HuggingFaceTask::Other,
            repo_id: None,
            revision: None,
        }));
    }

    if path.ends_with("model.joblib") || root.join("model.joblib").is_file() {
        return Err(WyrdPyError::model_validation(
            "ModelCard path input for model.joblib requires metadata.interface because joblib artifacts are shared by Sklearn, Xgboost, Lightgbm, and Catboost",
        ));
    }
    if path.ends_with("model.safetensors") || root.join("model.safetensors").is_file() {
        return Err(WyrdPyError::model_validation(
            "ModelCard path input for model.safetensors requires an explicit TorchInterface with an attached nn.Module; construct the interface and call load(path)",
        ));
    }
    if path.ends_with("model.ckpt") || root.join("model.ckpt").is_file() {
        return Err(WyrdPyError::model_validation(
            "ModelCard path input for model.ckpt requires an explicit LightningInterface with an attached LightningModule class or instance; construct the interface and call load(path)",
        ));
    }

    Err(WyrdPyError::unknown_model_type(
        "path",
        safe_path_label(path),
    ))
}

#[cfg(feature = "python")]
/// Return whether metadata still has the constructor's empty custom sentinel.
fn is_default_custom_interface(interface: &RustModelInterface) -> bool {
    matches!(
        interface,
        RustModelInterface::Custom(meta)
            if meta.framework_version.is_empty()
                && meta.loader_module.is_empty()
                && meta.loader_class.is_empty()
                && meta.model_subtype.is_none()
                && meta.extra.is_empty()
    )
}

#[cfg(feature = "python")]
fn safe_path_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "<path>".to_string())
}

#[cfg(feature = "python")]
/// Normalize a model artifact path to the Wyrd materialization root.
///
/// Built-in loaders read from convention paths such as `path/model.joblib` or
/// `path/model.keras`. When callers pass the convention file or subdirectory
/// itself, this helper returns its parent so the existing loader can be reused.
fn model_artifact_root(path: &Path) -> PathBuf {
    if path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(
                    name,
                    "model.joblib"
                        | "model.pt"
                        | "model.safetensors"
                        | "model.ckpt"
                        | "model.keras"
                )
            })
    {
        return path
            .parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf);
    }
    if path.is_dir()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "model" || name == "savedmodel")
    {
        return path
            .parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf);
    }
    path.to_path_buf()
}

#[cfg(feature = "python")]
/// Return an installed Python package version, falling back to `unknown`.
fn module_version_or_unknown(py: Python<'_>, package: &str) -> String {
    wyrd_utils::py::module_version(py, package)
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(feature = "python")]
fn interface_from_model_spec(
    py: Python<'_>,
    interface: &RustModelInterface,
) -> CardPyResult<Py<PyAny>> {
    let handle = match interface {
        RustModelInterface::Sklearn(_) => {
            ModelInterfaceHandle::Sklearn(ModelSklearnInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Xgboost(_) => {
            ModelInterfaceHandle::Xgboost(ModelXgboostInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Lightgbm(_) => {
            ModelInterfaceHandle::Lightgbm(ModelLightgbmInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Catboost(_) => {
            ModelInterfaceHandle::Catboost(ModelCatboostInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Torch(_) => {
            ModelInterfaceHandle::Torch(ModelTorchInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Lightning(_) => {
            ModelInterfaceHandle::Lightning(ModelLightningInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Tensorflow(_) => {
            ModelInterfaceHandle::Tensorflow(ModelTensorflowInterface::from_spec_inner(interface)?)
        }
        RustModelInterface::Huggingface(_) => ModelInterfaceHandle::Huggingface(
            ModelHuggingfaceInterface::from_spec_inner(interface)?,
        ),
        RustModelInterface::Custom(_) => {
            return Err(WyrdPyError::model_validation(
                "custom Python ModelInterface cannot be rebuilt from JSON without a class; pass interface=YourInterface to ModelCard.model_validate_json or cards.get",
            ));
        }
    };
    handle.into_py_any(py)
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
    WyrdPyError::model_validation(format!(
        "invalid ModelCard metadata label or annotation: {error}"
    ))
}

#[cfg(feature = "python")]
fn write_model_card_json_file(card: &ModelCard, path: &std::path::Path) -> CardPyResult<()> {
    wyrd_utils::json::write_json_sorted(path.join("card.json"), &card.to_card_envelope())
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

fn model_spec_from_metadata(
    metadata: &ModelCardMetadata,
    interface: RustModelInterface,
) -> ModelSpec {
    ModelSpec {
        interface,
        task_type: metadata.task_type,
        signature: metadata.signature.clone(),
        sample_input: metadata.sample_input.clone(),
        card_refs: metadata.card_refs.clone(),
    }
}

#[cfg(feature = "python")]
fn task_type_token(value: TaskType) -> &'static str {
    match value {
        TaskType::BinaryClassification => "binary_classification",
        TaskType::MultiClassClassification => "multi_class_classification",
        TaskType::Regression => "regression",
        TaskType::Clustering => "clustering",
        TaskType::AnomalyDetection => "anomaly_detection",
        TaskType::Forecasting => "forecasting",
        TaskType::Generation => "generation",
        TaskType::Other => "other",
    }
}

#[cfg(feature = "python")]
fn parse_metadata_interface(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<RustModelInterface> {
    let Some(value) = value else {
        return Ok(ModelCardMetadata::default().interface);
    };
    if value.is_none() {
        return Ok(ModelCardMetadata::default().interface);
    }
    if let Ok(handle) = ModelInterfaceHandle::from_interface(value) {
        return handle.to_spec_interface(py);
    }
    Ok(serde_json::from_value(pyobject_to_json(value)?)?)
}

#[cfg(feature = "python")]
fn parse_metadata_signature(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<RustModelSignature> {
    let Some(value) = value else {
        return Ok(ModelCardMetadata::default().signature);
    };
    if value.is_none() {
        return Ok(ModelCardMetadata::default().signature);
    }
    if let Ok(signature) = value.extract::<PyRef<'_, ModelSignature>>() {
        return Ok(signature.inner().clone());
    }
    Ok(serde_json::from_value(pyobject_to_json(value)?)?)
}

#[cfg(feature = "python")]
fn parse_metadata_sample_input(
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<Option<RustSampleInput>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_none() {
        return Ok(None);
    }
    if let Ok(sample_input) = value.extract::<PyRef<'_, SampleInput>>() {
        return Ok(Some(sample_input.to_rust()));
    }
    Ok(Some(serde_json::from_value(pyobject_to_json(value)?)?))
}

#[cfg(feature = "python")]
fn parse_metadata_card_refs(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<Vec<CardRef>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_none() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_value(pyobject_to_json(value)?)?)
}

#[cfg(feature = "python")]
fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use wyrd_spec::card::field::FieldSpec;
    use wyrd_spec::card::model::{
        ModelInterface as RustModelInterface, ModelSignature as RustModelSignature, SklearnMeta,
        TaskType,
    };
    use wyrd_spec::envelope::{CardKind, Spec};
    use wyrd_spec::ids::ColumnName;
    use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};

    use super::{ModelCard, ModelCardMetadata};

    fn valid_signature() -> RustModelSignature {
        RustModelSignature::new(
            vec![FieldSpec::new(column_name("feature"), "float64")],
            vec![FieldSpec::new(column_name("prediction"), "float64")],
        )
    }

    #[test]
    fn metadata_converts_to_model_spec_without_python_runtime() {
        let card = model_card();

        let spec = card.to_model_spec_from_metadata();
        assert_eq!(spec.interface_kind(), "Sklearn");
        assert_eq!(spec.signature.inputs.len(), 1);
        assert!(spec.validate().is_ok());

        let body = card.to_rust_card_body_from_metadata();
        assert!(matches!(body, Spec::Model(_)));
    }

    #[test]
    fn model_card_envelope_serializes_kind_model() {
        let serialized = match model_card().model_dump_json() {
            Ok(value) => value,
            Err(error) => panic!("ModelCard holder should serialize: {error}"),
        };

        assert!(serialized.contains(r#""apiVersion":"wyrd/v1""#));
        assert!(serialized.contains(r#""kind":"Model""#));
        assert!(serialized.contains(r#""labels":{"domain":"churn"}"#));
        assert!(serialized.contains(r#""spec":{"interface":{"kind":"Sklearn""#));
    }

    #[test]
    fn as_card_ref_returns_model_kind() {
        let card = model_card();
        let card_ref = card.as_card_ref().expect("identity is valid");

        assert_eq!(card_ref.kind, CardKind::Model);
        assert_eq!(card_ref.name.as_str(), "model");
        assert_eq!(card_ref.version.as_str(), "0.1.0");
    }

    fn model_card() -> ModelCard {
        let mut labels = BTreeMap::new();
        labels.insert(label_key("domain"), label_value("churn"));
        let mut annotations = BTreeMap::new();
        annotations.insert(
            annotation_key("acme.com/source"),
            annotation_value("training-run-1"),
        );
        ModelCard {
            space: "default".to_string(),
            name: "model".to_string(),
            version: "0.1.0".to_string(),
            uid: "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2".to_string(),
            labels,
            annotations,
            metadata: ModelCardMetadata {
                interface: RustModelInterface::Sklearn(SklearnMeta {
                    framework_version: "test".to_string(),
                    model_subtype: Some("LogisticRegression".to_string()),
                }),
                task_type: TaskType::BinaryClassification,
                signature: valid_signature(),
                sample_input: None,
                card_refs: Vec::new(),
            },
            created_at: Utc::now(),
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        }
    }

    fn column_name(value: &str) -> ColumnName {
        match ColumnName::new(value) {
            Ok(name) => name,
            Err(error) => panic!("static column name is valid: {error}"),
        }
    }

    fn label_key(value: &str) -> LabelKey {
        match LabelKey::new(value) {
            Ok(key) => key,
            Err(error) => panic!("static label key is valid: {error}"),
        }
    }

    fn label_value(value: &str) -> LabelValue {
        match LabelValue::new(value) {
            Ok(label_value) => label_value,
            Err(error) => panic!("static label value is valid: {error}"),
        }
    }

    fn annotation_key(value: &str) -> AnnotationKey {
        match AnnotationKey::new(value) {
            Ok(key) => key,
            Err(error) => panic!("static annotation key is valid: {error}"),
        }
    }

    fn annotation_value(value: &str) -> AnnotationValue {
        match AnnotationValue::new(value) {
            Ok(annotation_value) => annotation_value,
            Err(error) => panic!("static annotation value is valid: {error}"),
        }
    }
}
