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
use wyrd_spec::envelope::Spec;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::ApiVersion;

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::{PyAny, PyDict, PyType, PyTypeMethods},
    std::path::PathBuf,
    wyrd_interfaces::error::WyrdPyError,
    wyrd_interfaces::model::interfaces::{
        CatboostInterface as ModelCatboostInterface,
        HuggingfaceInterface as ModelHuggingfaceInterface,
        LightgbmInterface as ModelLightgbmInterface, LightningInterface as ModelLightningInterface,
        ModelInterface, ModelInterfaceHandle, SklearnInterface as ModelSklearnInterface,
        TensorflowInterface as ModelTensorflowInterface, TorchInterface as ModelTorchInterface,
        XgboostInterface as ModelXgboostInterface,
    },
    wyrd_interfaces::model::io::{load_model, save_model},
    wyrd_interfaces::model::sample::SampleInput,
    wyrd_interfaces::model::signature::ModelSignature,
    wyrd_spec::envelope::{CardKind, Metadata as EnvelopeMetadata},
    wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, MetadataError},
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
    /// Existing durable `ArtifactCard` references for this model card.
    pub artifact_refs: Vec<CardRef>,
}

impl Default for ModelCardMetadata {
    fn default() -> Self {
        Self {
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
            artifact_refs: Vec::new(),
        }
    }
}

/// Local Python-facing `ModelCard` holder.
///
/// A `ModelCard` owns local identity, holder metadata, and an optional live
/// Python model interface. It can save and load local filesystem
/// materialization, but it never registers itself and never creates
/// `ArtifactCards`.
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
    /// Local creation timestamp.
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
                    artifact_refs: envelope.spec.artifact_refs,
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
    Raw(Py<PyAny>),
    Interface(ModelInterfaceHandle),
    InterfaceClass(Py<PyAny>),
}

#[cfg(feature = "python")]
impl ModelCardInput {
    fn extract_bound(
        model_or_interface: &Bound<'_, PyAny>,
        allow_interface_class: bool,
    ) -> CardPyResult<Self> {
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
        artifact_refs: metadata.artifact_refs.clone(),
    }
}

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
fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use wyrd_spec::card::field::FieldSpec;
    use wyrd_spec::card::model::{
        ModelInterface as RustModelInterface, ModelSignature as RustModelSignature, SklearnMeta,
        TaskType,
    };
    use wyrd_spec::envelope::Spec;
    use wyrd_spec::ids::ColumnName;
    use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};

    use super::{ModelCard, ModelCardMetadata};

    fn valid_signature() -> RustModelSignature {
        RustModelSignature::new(
            vec![FieldSpec::new(
                ColumnName::new("feature").expect("static column name is valid"),
                "float64",
            )],
            vec![FieldSpec::new(
                ColumnName::new("prediction").expect("static column name is valid"),
                "float64",
            )],
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
        let serialized = model_card()
            .model_dump_json()
            .expect("ModelCard holder should serialize");

        assert!(serialized.contains(r#""apiVersion":"wyrd/v1""#));
        assert!(serialized.contains(r#""kind":"Model""#));
        assert!(serialized.contains(r#""labels":{"domain":"churn"}"#));
        assert!(serialized.contains(r#""spec":{"interface":{"kind":"Sklearn""#));
    }

    fn model_card() -> ModelCard {
        let mut labels = BTreeMap::new();
        labels.insert(
            LabelKey::new("domain").expect("static label key is valid"),
            LabelValue::new("churn").expect("static label value is valid"),
        );
        let mut annotations = BTreeMap::new();
        annotations.insert(
            AnnotationKey::new("acme.com/source").expect("static annotation key is valid"),
            AnnotationValue::new("training-run-1").expect("static annotation value is valid"),
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
                artifact_refs: Vec::new(),
            },
            created_at: DateTime::<Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp is valid"),
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        }
    }
}
