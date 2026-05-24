//! DataCard local holder and Python boundary.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_interfaces::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::validate::DataCardError;
use wyrd_spec::card::data::{
    CustomDataMeta, DataInterface as RustDataInterface, DataSchema, DataSpec, DataSplit, DataStats,
    SqlLogic,
};
use wyrd_spec::envelope::Spec;
use wyrd_spec::ids::{ColumnName, SplitName};
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::CardRef;

#[cfg(feature = "python")]
use {
    crate::artifact::ArtifactCard,
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::{PyAny, PyDict},
    serde_json::json,
    wyrd_interfaces::data::interfaces::{
        ArrowInterface, CustomDataInterface, DataInterface, DataInterfaceHandle,
        HuggingfaceInterface, ImageInterface, JsonlInterface, NumpyInterface, PandasInterface,
        ParquetInterface, PolarsInterface, SqlInterface, TextInterface, TorchInterface,
    },
    wyrd_interfaces::data::io::{load_data, save_data},
    wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, MetadataError},
    wyrd_utils::py::json_to_pyobject,
};

/// Python-holder metadata accumulated by a local DataCard.
///
/// This is not the durable registry record. It mirrors the locked Python holder
/// contract from the DataCard plan and is converted into `DataSpec` when a
/// Rust-only spec body is needed.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", from_py_object))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataCardMetadata {
    /// Interface metadata that will be written into the DataSpec.
    pub interface: RustDataInterface,
    /// Inferred or supplied data schema.
    pub schema: DataSchema,
    /// Existing durable ArtifactCard references for this data card.
    pub artifact_refs: Vec<CardRef>,
    /// Declared split strategies by split label.
    pub splits: HashMap<SplitName, DataSplit>,
    /// Target columns for supervised workflows.
    pub target_columns: Vec<ColumnName>,
    /// Optional SQL logic for SQL-backed data.
    pub sql: Option<SqlLogic>,
    /// Local byte and shape statistics.
    pub stats: DataStats,
}

impl Default for DataCardMetadata {
    fn default() -> Self {
        Self {
            interface: RustDataInterface::Custom(CustomDataMeta {
                loader_module: String::new(),
                loader_class: String::new(),
                extra: BTreeMap::new(),
            }),
            schema: DataSchema::empty(),
            artifact_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: Vec::new(),
            sql: None,
            stats: DataStats {
                row_count: None,
                col_count: None,
                byte_count: 1,
                sha256: "0".repeat(64),
            },
        }
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl DataCardMetadata {
    /// Return the metadata as a JSON-compatible Python dictionary.
    ///
    /// # Errors
    /// Returns a Wyrd error when metadata serialization fails.
    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(py, &serde_json::to_value(self)?)?)
    }

    /// Return the metadata as a JSON string.
    ///
    /// # Errors
    /// Returns a Wyrd error when metadata serialization fails.
    fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(self)?)
    }
}

/// Local Python-facing DataCard holder.
///
/// A DataCard owns local identity, holder metadata, and an optional live Python
/// data interface. It can save and load local filesystem materialization, but
/// it never registers itself and never creates ArtifactCards.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", skip_from_py_object))]
#[derive(Serialize, Deserialize)]
pub struct DataCard {
    /// DataCard space.
    pub space: String,
    /// DataCard name.
    pub name: String,
    /// DataCard version.
    pub version: String,
    /// DataCard UID.
    pub uid: String,
    /// Queryable local labels.
    pub labels: Labels,
    /// Free-form local annotations.
    pub annotations: Annotations,
    /// DataCard holder metadata.
    pub metadata: DataCardMetadata,
    /// Local creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Marker used by Python registry/client code to identify card holders.
    pub is_card: bool,
    /// Held live Python interface. This is skipped in serialized card JSON.
    #[cfg(feature = "python")]
    #[serde(skip)]
    pub interface: Option<Py<PyAny>>,
}

impl DataCard {
    /// Write only the serialized DataCard JSON under `path/card.json`.
    ///
    /// # Errors
    /// Returns a Wyrd error when local filesystem writes or JSON serialization
    /// fail.
    pub fn save_card(&self, path: PathBuf) -> CardPyResult<()> {
        save_card_json(self, &path)
    }

    /// Serialize this local holder as JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization fails.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// Convert serialized holder metadata into the Rust card spec body.
    ///
    /// This method is available without the `python` feature. It uses only the
    /// metadata already stored in the local holder and never attempts to access
    /// live Python interface state.
    ///
    /// # Errors
    /// Returns a DataCard validation error when the stored metadata does not
    /// satisfy the durable spec contract.
    pub fn to_rust_card_body_from_metadata(&self) -> Result<Spec, DataCardError> {
        Ok(Spec::Data(self.to_data_spec_from_metadata()?))
    }

    /// Convert serialized holder metadata into a pure Rust DataSpec.
    ///
    /// This method is available without the `python` feature for server, UI,
    /// and registry paths that need to inspect DataCard attributes without a
    /// Python runtime.
    ///
    /// # Errors
    /// Returns a DataCard validation error when the stored metadata does not
    /// satisfy the durable spec contract.
    pub fn to_data_spec_from_metadata(&self) -> Result<DataSpec, DataCardError> {
        data_spec_from_metadata(&self.metadata, self.metadata.interface.clone())
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl DataCard {
    /// Create a local DataCard from raw data, a data interface, or an ArtifactCard.
    ///
    /// # Arguments
    /// * `data` - Raw framework data, a `DataInterface`, a Python subclass of
    ///   `DataInterface`, or an existing `ArtifactCard`.
    /// * `space` - Optional card space. Defaults to `default`.
    /// * `name` - Optional card name. Defaults to `data`.
    /// * `version` - Optional semantic version. Defaults to `0.1.0`.
    /// * `uid` - Optional UUIDv7 card UID. Defaults to a generated UID.
    /// * `labels` - Optional queryable labels.
    /// * `annotations` - Optional free-form annotations.
    /// * `metadata` - Optional holder metadata to seed before inference.
    ///
    /// # Errors
    /// Returns a Wyrd error when input classification, interface metadata
    /// conversion, or schema inference fails.
    #[new]
    #[pyo3(signature = (data, space=None, name=None, version=None, uid=None, labels=None, annotations=None, metadata=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        data: &Bound<'_, PyAny>,
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        uid: Option<&str>,
        labels: Option<BTreeMap<String, String>>,
        annotations: Option<BTreeMap<String, String>>,
        metadata: Option<DataCardMetadata>,
    ) -> CardPyResult<Self> {
        let py = data.py();
        let mut metadata = metadata.unwrap_or_default();
        let (interface, artifact_ref) = DataCardInput::extract_bound(data)?.into_parts(py)?;

        if let Some(handle) = interface.as_ref() {
            metadata.interface = handle.to_spec_interface(py)?;
            metadata.schema = infer_schema_from_handle(handle, py)?;
        }
        if let Some(artifact_ref) = artifact_ref {
            metadata.artifact_refs.push(artifact_ref);
        }

        Ok(Self {
            space: space.unwrap_or("default").to_owned(),
            name: name.unwrap_or("data").to_owned(),
            version: version.unwrap_or("0.1.0").to_owned(),
            uid: uid.map(str::to_owned).unwrap_or_else(wyrd_utils::uuid7),
            labels: labels_from_user(labels.unwrap_or_default())?,
            annotations: annotations_from_user(annotations.unwrap_or_default())?,
            metadata,
            created_at: utc_now(),
            is_card: true,
            interface: interface.map(|handle| handle.into_py_any(py)).transpose()?,
        })
    }

    /// Return the held live interface, if one is attached.
    #[getter]
    pub fn interface(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.interface
            .as_ref()
            .map(|interface| interface.clone_ref(py))
    }

    /// Replace the held live data interface.
    ///
    /// The replacement may be a built-in interface or a Python subclass of
    /// `DataInterface`. Wyrd updates interface metadata and schema inference
    /// from the replacement when live source data is available.
    ///
    /// # Errors
    /// Returns a Wyrd error when the value is not a supported data interface.
    #[setter]
    pub fn set_interface(
        &mut self,
        py: Python<'_>,
        interface: &Bound<'_, PyAny>,
    ) -> CardPyResult<()> {
        let handle = DataInterfaceHandle::from_interface(interface)?;
        self.metadata.interface = handle.to_spec_interface(py)?;
        self.metadata.schema = infer_schema_from_handle(&handle, py)?;
        self.interface = Some(handle.into_py_any(py)?);
        Ok(())
    }

    /// Return live local data from the held interface.
    ///
    /// # Errors
    /// Returns a Wyrd error when no interface or no live source data is
    /// attached.
    #[getter]
    pub fn data(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let interface = self.interface.as_ref().ok_or_else(|| {
            WyrdPyError::validation("DataCard interface is required for data access")
        })?;
        let handle = DataInterfaceHandle::from_interface(interface.bind(py))?;
        let data = handle
            .source_ref()
            .ok_or_else(|| WyrdPyError::validation("DataCard has no live local data attached"))?;
        Ok(data.clone_ref(py))
    }

    /// Return the DataCard space.
    #[getter]
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Set the DataCard space.
    #[setter]
    pub fn set_space(&mut self, value: String) {
        self.space = value;
    }

    /// Return the DataCard name.
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the DataCard name.
    #[setter]
    pub fn set_name(&mut self, value: String) {
        self.name = value;
    }

    /// Return the DataCard version.
    #[getter]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Set the DataCard version.
    #[setter]
    pub fn set_version(&mut self, value: String) {
        self.version = value;
    }

    /// Return the DataCard UID.
    #[getter]
    pub fn uid(&self) -> &str {
        &self.uid
    }

    /// Set the DataCard UID.
    #[setter]
    pub fn set_uid(&mut self, value: String) {
        self.uid = value;
    }

    /// Return queryable DataCard labels.
    #[getter]
    pub fn labels(&self) -> BTreeMap<String, String> {
        labels_to_strings(&self.labels)
    }

    /// Replace queryable DataCard labels.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// labels.
    #[setter]
    pub fn set_labels(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.labels = labels_from_user(value)?;
        Ok(())
    }

    /// Return free-form DataCard annotations.
    #[getter]
    pub fn annotations(&self) -> BTreeMap<String, String> {
        annotations_to_strings(&self.annotations)
    }

    /// Replace free-form DataCard annotations.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// annotations.
    #[setter]
    pub fn set_annotations(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.annotations = annotations_from_user(value)?;
        Ok(())
    }

    /// Return DataCard holder metadata.
    #[getter]
    pub fn metadata(&self) -> DataCardMetadata {
        self.metadata.clone()
    }

    /// Replace DataCard holder metadata.
    #[setter]
    pub fn set_metadata(&mut self, value: DataCardMetadata) {
        self.metadata = value;
    }

    /// Return the local creation timestamp.
    #[getter]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Set the local creation timestamp.
    #[setter]
    pub fn set_created_at(&mut self, value: DateTime<Utc>) {
        self.created_at = value;
    }

    /// Return whether this object is a card holder.
    #[getter]
    pub fn is_card(&self) -> bool {
        self.is_card
    }

    /// Save local data artifacts and the card JSON under `path`.
    ///
    /// This method only performs local filesystem materialization. It updates
    /// interface metadata and byte stats, then writes `card.json`. It does not
    /// create ArtifactCards, upload bytes, or register anything.
    ///
    /// # Errors
    /// Returns a Wyrd error when no interface is attached, interface save
    /// fails, or card JSON cannot be written.
    #[pyo3(signature = (path, save_kwargs=None))]
    pub fn save(
        &mut self,
        py: Python<'_>,
        path: PathBuf,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let interface = self.interface.as_ref().ok_or_else(|| {
            WyrdPyError::validation("DataCard interface is required for local save")
        })?;
        let bound = interface.bind(py);
        self.metadata.interface =
            DataInterfaceHandle::from_interface(bound)?.to_spec_interface(py)?;
        self.metadata.stats = save_data(bound, &path, save_kwargs)?;
        self.save_card(path)
    }

    /// Load local data artifacts through the held interface.
    ///
    /// The interface reconstructs its convention path from `path`; Wyrd does
    /// not persist a local path in the card JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when no interface is attached or interface load
    /// fails.
    #[pyo3(signature = (path=None, load_kwargs=None))]
    pub fn load(
        &mut self,
        py: Python<'_>,
        path: Option<PathBuf>,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let interface = self.interface.as_ref().ok_or_else(|| {
            WyrdPyError::validation("DataCard interface is required for local load")
        })?;
        load_data(interface.bind(py), path, load_kwargs)
    }

    /// Write only the serialized DataCard JSON under `path/card.json`.
    ///
    /// # Errors
    /// Returns a Wyrd error when local filesystem writes or JSON serialization
    /// fail.
    #[pyo3(name = "save_card")]
    pub fn save_card_py(&self, path: PathBuf) -> CardPyResult<()> {
        self.save_card(path)
    }

    /// Return this DataCard as a JSON string.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization fails.
    #[pyo3(name = "model_dump_json")]
    pub fn model_dump_json_py(&self) -> CardPyResult<String> {
        self.model_dump_json()
    }

    /// Build a DataCard from serialized JSON and optional live data.
    ///
    /// Passing `data` attaches a raw object, explicit interface, Python
    /// subclass, or ArtifactCard. When `data` is omitted, Wyrd rebuilds
    /// built-in and declared custom interfaces from the serialized spec
    /// metadata. Python subclass-backed cards must pass `data=YourInterface()`.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON parsing fails or the serialized custom
    /// interface cannot be rebuilt without an explicit Python object.
    #[staticmethod]
    #[pyo3(name = "model_validate_json", signature = (json_string, data=None))]
    pub fn model_validate_json_py(
        py: Python<'_>,
        json_string: String,
        data: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<Self> {
        let mut card: DataCard = serde_json::from_str(&json_string)?;
        card.attach_data(py, data)?;
        Ok(card)
    }

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
impl DataCard {
    /// Convert this local holder into the Rust card spec body.
    ///
    /// # Errors
    /// Returns a Wyrd error when interface conversion or DataSpec validation
    /// fails.
    pub fn to_rust_card_body(&self, py: Python<'_>) -> CardPyResult<Spec> {
        Ok(Spec::Data(self.to_data_spec(py)?))
    }

    /// Convert this local holder into a pure Rust DataSpec.
    ///
    /// # Errors
    /// Returns a Wyrd error when interface conversion or DataSpec validation
    /// fails.
    pub fn to_data_spec(&self, py: Python<'_>) -> CardPyResult<DataSpec> {
        let interface = self
            .interface
            .as_ref()
            .map(|interface| {
                let handle = DataInterfaceHandle::from_interface(interface.bind(py))?;
                handle.to_spec_interface(py)
            })
            .transpose()?
            .unwrap_or_else(|| self.metadata.interface.clone());

        data_spec_from_metadata(&self.metadata, interface).map_err(WyrdPyError::from)
    }

    fn attach_data(&mut self, py: Python<'_>, data: Option<&Bound<'_, PyAny>>) -> CardPyResult<()> {
        if let Some(data) = data {
            let (interface, artifact_ref) = DataCardInput::extract_bound(data)?.into_parts(py)?;
            if let Some(handle) = interface {
                self.metadata.interface = handle.to_spec_interface(py)?;
                self.metadata.schema = infer_schema_from_handle(&handle, py)?;
                self.interface = Some(handle.into_py_any(py)?);
            }
            if let Some(artifact_ref) = artifact_ref {
                self.metadata.artifact_refs.push(artifact_ref);
            }
            return Ok(());
        }

        self.interface = Some(interface_from_spec(py, &self.metadata.interface)?);
        Ok(())
    }
}

#[cfg(feature = "python")]
enum DataCardInput {
    Raw(Py<PyAny>),
    Interface(DataInterfaceHandle),
    Artifact(ArtifactCard),
}

#[cfg(feature = "python")]
impl DataCardInput {
    fn extract_bound(data: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        if data.is_instance_of::<ArtifactCard>() {
            let artifact = data.extract::<PyRef<'_, ArtifactCard>>()?;
            return Ok(Self::Artifact(artifact.clone()));
        }

        if data.is_instance_of::<DataInterface>() {
            return Ok(Self::Interface(DataInterfaceHandle::from_interface(data)?));
        }

        Ok(Self::Raw(data.clone().unbind()))
    }

    fn into_parts(
        self,
        py: Python<'_>,
    ) -> CardPyResult<(Option<DataInterfaceHandle>, Option<CardRef>)> {
        match self {
            Self::Raw(data) => Ok((
                Some(DataInterfaceHandle::from_raw(py, data.bind(py))?),
                None,
            )),
            Self::Interface(interface) => Ok((Some(interface), None)),
            Self::Artifact(artifact) => Ok((None, Some(artifact.as_card_ref()?))),
        }
    }
}

#[cfg(feature = "python")]
fn infer_schema_from_handle(
    handle: &DataInterfaceHandle,
    py: Python<'_>,
) -> CardPyResult<DataSchema> {
    if let Some(source) = handle.source_ref() {
        handle.infer_schema(py, source.bind(py))
    } else {
        Ok(DataSchema::empty())
    }
}

#[cfg(feature = "python")]
fn interface_from_spec(py: Python<'_>, interface: &RustDataInterface) -> CardPyResult<Py<PyAny>> {
    let handle = match interface {
        RustDataInterface::Pandas(_) => {
            DataInterfaceHandle::Pandas(PandasInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Polars(_) => {
            DataInterfaceHandle::Polars(PolarsInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Arrow(_) => {
            DataInterfaceHandle::Arrow(ArrowInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Parquet(_) => {
            DataInterfaceHandle::Parquet(ParquetInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Numpy(_) => {
            DataInterfaceHandle::Numpy(NumpyInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Torch(_) => {
            DataInterfaceHandle::Torch(TorchInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Sql(_) => {
            DataInterfaceHandle::Sql(SqlInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Jsonl(_) => {
            DataInterfaceHandle::Jsonl(JsonlInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Image(_) => {
            DataInterfaceHandle::Image(ImageInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Text(_) => {
            DataInterfaceHandle::Text(TextInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Huggingface(_) => {
            DataInterfaceHandle::Huggingface(HuggingfaceInterface::from_spec_inner(interface)?)
        }
        RustDataInterface::Custom(meta) => {
            if meta.loader_module.is_empty() || meta.loader_class.is_empty() {
                return Err(WyrdPyError::validation(
                    "custom Python DataInterface cannot be rebuilt from JSON; pass data=YourInterface() to DataCard.model_validate_json",
                ));
            }
            DataInterfaceHandle::Custom(CustomDataInterface::from_spec_inner(interface)?)
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
    WyrdPyError::validation_with_details(
        "invalid DataCard metadata label or annotation",
        json!({ "reason": error.to_string() }),
    )
}

fn save_card_json(card: &DataCard, path: &std::path::Path) -> CardPyResult<()> {
    wyrd_utils::json::write_json_sorted(path.join("card.json"), card)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

fn data_spec_from_metadata(
    metadata: &DataCardMetadata,
    interface: RustDataInterface,
) -> Result<DataSpec, DataCardError> {
    DataSpec::new(
        interface,
        metadata.schema.clone(),
        metadata.artifact_refs.clone(),
        metadata.splits.clone(),
        metadata.target_columns.clone(),
        metadata.sql.clone(),
        metadata.stats.clone(),
    )
}

#[cfg(feature = "python")]
fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use chrono::{DateTime, Utc};
    use wyrd_spec::card::data::{
        DataInterface as RustDataInterface, DataSchema, DataStats, PandasMeta, ParquetCompression,
    };
    use wyrd_spec::card::field::FieldSpec;
    use wyrd_spec::envelope::Spec;
    use wyrd_spec::ids::ColumnName;
    use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};

    use super::{DataCard, DataCardMetadata};

    #[test]
    fn metadata_converts_to_data_spec_without_python_runtime() {
        let mut labels = BTreeMap::new();
        labels.insert(
            LabelKey::new("domain").expect("static label key is valid"),
            LabelValue::new("churn").expect("static label value is valid"),
        );
        let mut annotations = BTreeMap::new();
        annotations.insert(
            AnnotationKey::new("acme.com/source").expect("static annotation key is valid"),
            AnnotationValue::new("warehouse.customer_churn")
                .expect("static annotation value is valid"),
        );
        let card = DataCard {
            space: "default".to_string(),
            name: "data".to_string(),
            version: "0.1.0".to_string(),
            uid: "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2".to_string(),
            labels,
            annotations,
            metadata: DataCardMetadata {
                interface: RustDataInterface::Pandas(PandasMeta {
                    framework_version: "test".to_string(),
                    compression: ParquetCompression::Snappy,
                }),
                schema: DataSchema::new(vec![FieldSpec::new(
                    ColumnName::new("value").expect("static column name is valid"),
                    "int64",
                )]),
                artifact_refs: Vec::new(),
                splits: HashMap::new(),
                target_columns: Vec::new(),
                sql: None,
                stats: DataStats {
                    row_count: Some(2),
                    col_count: Some(1),
                    byte_count: 128,
                    sha256: "a".repeat(64),
                },
            },
            created_at: DateTime::<Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp is valid"),
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        };

        let spec = card
            .to_data_spec_from_metadata()
            .expect("valid metadata should build a DataSpec");
        assert_eq!(spec.interface_kind(), "Pandas");
        assert_eq!(spec.schema.columns.len(), 1);
        assert_eq!(spec.stats().byte_count, 128);

        let body = card
            .to_rust_card_body_from_metadata()
            .expect("valid metadata should build a card spec body");
        assert!(matches!(body, Spec::Data(_)));

        let serialized = card
            .model_dump_json()
            .expect("DataCard holder should serialize");
        assert!(serialized.contains(r#""labels":{"domain":"churn"}"#));
        assert!(
            serialized.contains(r#""annotations":{"acme.com/source":"warehouse.customer_churn"}"#)
        );
        assert!(!serialized.contains(r#""tags""#));
    }
}
