//! `DataCard` local holder and Python boundary.

use std::collections::{BTreeMap, HashMap};
#[cfg(feature = "python")]
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_interfaces::error::CardPyResult;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::data::{
    CustomDataMeta, DataInterface as RustDataInterface, DataSchema, DataSpec, DataSplit, DataStats,
    SqlLogic,
};
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ColumnName, SplitName};
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, Ref};

#[cfg(feature = "python")]
use {
    crate::card_ref::CardRefPy,
    pyo3::prelude::*,
    pyo3::pyclass::{PyTraverseError, PyVisit},
    pyo3::types::{PyAny, PyDict, PyType, PyTypeMethods},
    serde_json::json,
    std::path::PathBuf,
    wyrd_interfaces::data::interfaces::{
        ArrowInterface, DataInterface, DataInterfaceHandle, HuggingfaceInterface, ImageInterface,
        JsonlInterface, NumpyInterface, PandasInterface, ParquetInterface, PolarsInterface,
        SqlInterface, TextInterface, TorchInterface,
    },
    wyrd_interfaces::data::io::save_data,
    wyrd_interfaces::data::io::sql_logic_from_data,
    wyrd_interfaces::data::schema::PyDataSchema,
    wyrd_interfaces::data::stats::PyDataStats,
    wyrd_interfaces::error::WyrdPyError,
    wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, MetadataError},
};

/// Python-holder metadata accumulated by a local `DataCard`.
///
/// This is not the durable registry record. It mirrors the locked Python holder
/// contract from the `DataCard` plan and is converted into `DataSpec` when a
/// Rust-only spec body is needed.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", from_py_object))]
// justification: pyo3 #[pyclass] generates unsafe impl for internal invariants; the Deserialize path constructs a plain Rust struct and does not exercise the unsafe boundary
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataCardMetadata {
    /// Interface metadata that will be written into the `DataSpec`.
    pub interface: RustDataInterface,
    /// Inferred or supplied data schema.
    pub schema: DataSchema,
    /// Existing durable card references for this data card.
    pub card_refs: Vec<CardRef>,
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
            card_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: Vec::new(),
            sql: None,
            stats: DataStats {
                row_count: None,
                col_count: None,
                byte_count: 0,
                sha256: "0".repeat(64),
            },
        }
    }
}

/// Local Python-facing `DataCard` holder.
///
/// A `DataCard` owns local identity, holder metadata, and an optional live Python
/// data interface. It can save local materialization and load either a supplied
/// local path or the server-owned artifacts associated with a retrieved Card.
/// It never registers itself and never creates Artifact cards.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", skip_from_py_object))]
// justification: pyo3 #[pyclass] generates unsafe impl for internal invariants; the Deserialize path constructs a plain Rust struct and does not exercise the unsafe boundary
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Serialize, Deserialize)]
pub struct DataCard {
    /// `DataCard` space.
    pub space: String,
    /// `DataCard` name.
    pub name: String,
    /// `DataCard` version.
    pub version: String,
    /// `DataCard` UID.
    pub uid: String,
    /// Queryable local labels.
    pub labels: Labels,
    /// Free-form local annotations.
    pub annotations: Annotations,
    /// `DataCard` holder metadata.
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
    /// Write only the serialized `DataCard` JSON under `path/card.json`.
    ///
    /// # Errors
    /// Returns a Wyrd error when local filesystem writes or JSON serialization
    /// fail.
    #[cfg(feature = "python")]
    fn write_card_json(&self, path: &Path) -> CardPyResult<()> {
        write_card_json_file(self, path)
    }

    /// Serialize this local holder as JSON.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization fails.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.to_card_envelope())?)
    }

    /// Convert serialized holder metadata into the Rust card spec body.
    ///
    /// This method is available without the `python` feature. It uses only the
    /// metadata already stored in the local holder and never attempts to access
    /// live Python interface state. Stats are not validated — call `spec.validate()`
    /// if durable-contract invariants must be checked.
    pub fn to_rust_card_body_from_metadata(&self) -> Spec {
        Spec::Data(self.to_data_spec_from_metadata())
    }

    /// Convert serialized holder metadata into a pure Rust `DataSpec`.
    ///
    /// This method is available without the `python` feature for server, UI,
    /// and registry paths that need to inspect `DataCard` attributes without a
    /// Python runtime. Stats are not validated here — call `spec.validate()` if
    /// durable-contract invariants must be checked.
    pub fn to_data_spec_from_metadata(&self) -> DataSpec {
        data_spec_from_metadata(&self.metadata, self.metadata.interface.clone())
    }

    /// Convert this holder identity into a Data Card reference.
    ///
    /// # Errors
    /// Returns a Wyrd error when identity fields are invalid.
    pub fn as_card_ref(&self) -> Result<CardRef, WyrdError> {
        use crate::identity::{card_name, optional_card_uid, space_name, version_block};

        Ok(CardRef {
            kind: CardKind::Data,
            name: card_name("name", &self.name)?,
            version: version_block(&self.version)?,
            space: Some(space_name(&self.space)?),
            uid: optional_card_uid(&self.uid)?,
        })
    }

    fn to_card_envelope(&self) -> DataCardEnvelope<'_> {
        DataCardEnvelope {
            api_version: ApiVersion::V1,
            kind: "Data",
            metadata: DataCardEnvelopeMetadata {
                name: &self.name,
                version: &self.version,
                space: &self.space,
                uid: &self.uid,
                labels: &self.labels,
                annotations: &self.annotations,
            },
            spec: self.to_data_spec_from_metadata(),
            relationships: Vec::new(),
            status: None,
        }
    }
}

#[derive(Serialize)]
struct DataCardEnvelope<'a> {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    metadata: DataCardEnvelopeMetadata<'a>,
    spec: DataSpec,
    relationships: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct DataCardEnvelopeMetadata<'a> {
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
#[pymethods]
impl DataCardMetadata {
    /// Return this metadata as a Python-serializable dict for inspection.
    pub fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(
            py,
            &serde_json::to_value(self).map_err(|e| WyrdPyError::Io(e.to_string()))?,
        )
        .map_err(Into::into)
    }
}

#[cfg(feature = "python")]
impl DataCard {
    /// Build a native holder from a server-returned Data Card envelope.
    ///
    /// This path consumes the native envelope directly and does not re-enter
    /// the Python package or deserialize through a Python model class.
    pub fn from_card(card: wyrd_spec::envelope::Card) -> Result<Self, WyrdError> {
        if card.api_version.as_str() != ApiVersion::V1 || card.kind != CardKind::Data {
            return Err(WyrdError::DataValidation {
                message: "DataCard envelope must use apiVersion wyrd/v1 and kind Data".to_owned(),
                details: serde_json::Value::Null,
            });
        }
        let metadata = card.metadata;
        let Spec::Data(spec) = card.spec else {
            return Err(WyrdError::DataValidation {
                message: "DataCard envelope spec must be a Data spec".to_owned(),
                details: serde_json::Value::Null,
            });
        };
        spec.validate()?;
        let version = metadata
            .resolved_pin()
            .map(ToString::to_string)
            .ok_or_else(|| WyrdError::DataValidation {
                message: "DataCard envelope missing resolved version pin".to_owned(),
                details: serde_json::Value::Null,
            })?;
        let card_refs = spec
            .card_refs
            .into_iter()
            .map(|reference| match reference {
                Ref::Ref(card_ref) => Ok(card_ref),
                Ref::Sibling { sibling } => Ok(sibling),
                Ref::Path(path) => Err(WyrdError::DataValidation {
                    message: format!(
                        "DataCard contains unresolved card reference path: {}",
                        path.display()
                    ),
                    details: serde_json::Value::Null,
                }),
            })
            .collect::<Result<Vec<_>, WyrdError>>()?;
        let space = metadata
            .space
            .as_ref()
            .ok_or_else(|| WyrdError::DataValidation {
                message: "DataCard envelope missing space in metadata".to_owned(),
                details: serde_json::Value::Null,
            })?;
        let uid = metadata
            .uid
            .as_ref()
            .ok_or_else(|| WyrdError::DataValidation {
                message: "DataCard envelope missing uid in metadata".to_owned(),
                details: serde_json::Value::Null,
            })?;
        Ok(Self {
            space: space.to_string(),
            name: metadata.name.to_string(),
            version,
            uid: uid.to_string(),
            labels: metadata.labels,
            annotations: metadata.annotations,
            metadata: DataCardMetadata {
                interface: spec.interface,
                schema: spec.schema,
                card_refs,
                splits: spec.splits,
                target_columns: spec.target_columns,
                sql: spec.sql,
                stats: spec.stats,
            },
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        })
    }

    /// Hydrate the holder's built-in or caller-supplied Python interface.
    #[cfg(feature = "python")]
    pub fn hydrate_interface(
        &mut self,
        py: Python<'_>,
        interface: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<()> {
        self.attach_data(py, interface)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl DataCard {
    /// Create a local `DataCard` from raw data, a data interface, or an Artifact card reference.
    ///
    /// # Arguments
    /// * `data` - Raw framework data, a `DataInterface`, a Python subclass of
    ///   `DataInterface`, or a `CardRef` with kind `Artifact`.
    /// * `space` - Optional card space. Defaults to `default`.
    /// * `name` - Optional card name. Defaults to `data`.
    /// * `version` - Optional semantic version. Defaults to `0.1.0`.
    /// * `uid` - Optional `UUIDv7` card UID. Defaults to a generated UID.
    /// * `labels` - Optional queryable labels.
    /// * `annotations` - Optional free-form annotations.
    /// * `metadata` - Optional holder metadata to seed before inference.
    ///
    /// # Errors
    /// Returns a Wyrd error when input classification, interface metadata
    /// conversion, or schema inference fails.
    #[new]
    #[pyo3(signature = (data, space=None, name=None, version=None, uid=None, labels=None, annotations=None, metadata=None))]
    // justification: pyo3 #[new] signature must match the Python API surface; params correspond 1:1 to the DataCard() Python constructor
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
        let (interface, card_ref) =
            DataCardInput::extract_bound(data, false)?.into_parts(py, &metadata)?;

        if let Some(handle) = interface.as_ref() {
            metadata.interface = handle.to_spec_interface(py)?;
            metadata.schema = infer_schema_from_handle(handle, py)?;
            metadata.sql = sql_logic_from_handle(handle, py)?;
        }
        if let Some(card_ref) = card_ref {
            metadata.card_refs.push(card_ref);
        }

        let mut resolved_space = space.map(str::to_owned);
        let mut resolved_labels = labels_from_user(labels.unwrap_or_default())?;
        let mut resolved_annotations = annotations_from_user(annotations.unwrap_or_default())?;
        crate::identity::apply_repo_defaults(
            &CardKind::Data,
            &mut resolved_space,
            &mut resolved_labels,
            &mut resolved_annotations,
        );

        Ok(Self {
            space: resolved_space.unwrap_or_else(|| "default".to_owned()),
            name: name.unwrap_or("data").to_owned(),
            version: version.unwrap_or("0.1.0").to_owned(),
            uid: uid.map_or_else(wyrd_utils::uuid7, str::to_owned),
            labels: resolved_labels,
            annotations: resolved_annotations,
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
        if let Some(data) = handle.source_ref() {
            return Ok(data.clone_ref(py));
        }
        if matches!(handle, DataInterfaceHandle::Subclass(_)) {
            return Ok(interface
                .bind(py)
                .getattr("data")
                .map_err(|_| {
                    WyrdPyError::validation(
                        "custom Python DataInterface must expose a data attribute for DataCard.data",
                    )
                })?
                .unbind());
        }
        Err(WyrdPyError::validation(
            "DataCard has no live local data attached",
        ))
    }

    /// Return the `DataCard` space.
    #[getter]
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Set the `DataCard` space.
    #[setter]
    pub fn set_space(&mut self, value: String) {
        self.space = value;
    }

    /// Return the `DataCard` name.
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the `DataCard` name.
    #[setter]
    pub fn set_name(&mut self, value: String) {
        self.name = value;
    }

    /// Return the `DataCard` version.
    #[getter]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Set the `DataCard` version.
    #[setter]
    pub fn set_version(&mut self, value: String) {
        self.version = value;
    }

    /// Return the `DataCard` UID.
    #[getter]
    pub fn uid(&self) -> &str {
        &self.uid
    }

    /// Convert this `DataCard`'s identity into a Wyrd `CardRef`.
    ///
    /// # Errors
    /// Returns a Wyrd validation error when identity fields fail newtype
    /// invariants.
    #[pyo3(name = "as_card_ref")]
    pub fn as_card_ref_py(&self) -> CardPyResult<CardRefPy> {
        self.as_card_ref().map(CardRefPy).map_err(Into::into)
    }

    /// Set the `DataCard` UID.
    #[setter]
    pub fn set_uid(&mut self, value: String) {
        self.uid = value;
    }

    /// Return queryable `DataCard` labels.
    #[getter]
    pub fn labels(&self) -> BTreeMap<String, String> {
        labels_to_strings(&self.labels)
    }

    /// Replace queryable `DataCard` labels.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// labels.
    #[setter]
    pub fn set_labels(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.labels = labels_from_user(value)?;
        Ok(())
    }

    /// Return free-form `DataCard` annotations.
    #[getter]
    pub fn annotations(&self) -> BTreeMap<String, String> {
        annotations_to_strings(&self.annotations)
    }

    /// Replace free-form `DataCard` annotations.
    ///
    /// # Errors
    /// Returns a Wyrd error when a key or value is invalid for user-authored
    /// annotations.
    #[setter]
    pub fn set_annotations(&mut self, value: BTreeMap<String, String>) -> CardPyResult<()> {
        self.annotations = annotations_from_user(value)?;
        Ok(())
    }

    /// Return `DataCard` holder metadata.
    #[getter]
    pub fn metadata(&self) -> DataCardMetadata {
        self.metadata.clone()
    }

    /// Return the `DataCard` schema.
    #[getter]
    pub fn schema(&self) -> PyDataSchema {
        PyDataSchema::from(self.metadata.schema.clone())
    }

    /// Return the `DataCard` byte and shape statistics.
    #[getter]
    pub fn stats(&self) -> PyDataStats {
        PyDataStats::from(self.metadata.stats.clone())
    }

    /// Replace `DataCard` holder metadata.
    #[setter]
    pub fn set_metadata(&mut self, value: DataCardMetadata) {
        self.metadata = value;
    }

    /// Save local data artifacts and the card JSON under `path`.
    ///
    /// This method only performs local filesystem materialization. It updates
    /// interface metadata and byte stats, then writes `card.json`. It does not
    /// create Artifact cards, upload bytes, or register anything.
    ///
    /// # Errors
    /// Returns a Wyrd error when no interface is attached, interface save
    /// fails, or card JSON cannot be written.
    #[wyrd_test_contract_macros::critical("python:DataCard.save")]
    #[pyo3(signature = (path, save_kwargs=None))]
    // justification: pyo3 boundary; the extractor produces an owned value (PathBuf/PyRef/newtype), taking it by reference would require a caller-side clone
    #[allow(clippy::needless_pass_by_value)]
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
        self.write_card_json(&path)
    }

    /// Load data artifacts through the held interface.
    ///
    /// Pass `path` to load an existing local materialization. Without a path,
    /// call this on a `DataCard` returned by `Cards.data.get`; Wyrd uses the
    /// configured Wyrd client to obtain the server's artifact inventory,
    /// downloads and verifies the artifacts into an operation-local temporary
    /// directory, and then invokes the interface.
    ///
    /// `get` itself only retrieves and validates the serialized Card envelope.
    /// This method is the explicit boundary where data bytes enter memory.
    /// The temporary directory remains alive until the interface load returns.
    ///
    /// # Arguments
    /// * `path` - Optional local materialization directory. Omit it for a
    ///   server-backed Card returned by `Cards.data.get`.
    /// * `load_kwargs` - Optional `DataLoadArgs` or JSON-compatible mapping
    ///   forwarded to the data interface.
    ///
    /// # Errors
    /// Returns a Wyrd error when the configured client is unavailable, the
    /// Card has no server UID, artifact download or verification fails, no
    /// interface is attached, or interface load fails.
    #[wyrd_test_contract_macros::critical("python:DataCard.load")]
    #[pyo3(signature = (path=None, load_kwargs=None))]
    // justification: pyo3 boundary; the extractor produces an owned value (PathBuf/PyRef/newtype), taking it by reference would require a caller-side clone
    #[allow(clippy::needless_pass_by_value)]
    pub fn load(
        &mut self,
        py: Python<'_>,
        path: Option<PathBuf>,
        load_kwargs: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<()> {
        let load_kwargs = normalize_load_kwargs(load_kwargs)?;
        let temporary_directory = if path.is_none() {
            let uid = self.as_card_ref()?.uid.ok_or_else(|| {
                WyrdPyError::validation("DataCard.load requires a server-assigned Card UID")
            })?;
            let cards = wyrd_registry::Cards::new(None, None).map_err(WyrdPyError::from)?;
            let temporary = tempfile::Builder::new()
                .prefix("wyrd-data-")
                .tempdir()
                .map_err(|error| WyrdPyError::Io(error.to_string()))?;
            let destination = temporary.path().to_path_buf();
            py.detach(move || {
                wyrd_runtime::runtime().block_on(cards.download_artifacts_to(&uid, &destination))
            })
            .map_err(WyrdPyError::from)?;
            Some(temporary)
        } else {
            None
        };
        let path = path.or_else(|| {
            temporary_directory
                .as_ref()
                .map(|directory| directory.path().to_path_buf())
        });
        let interface = self.interface.as_ref().ok_or_else(|| {
            WyrdPyError::validation("DataCard interface is required for local load")
        })?;
        interface
            .bind(py)
            .call_method1("load", (path, load_kwargs.as_ref()))?;
        Ok(())
    }

    /// Return this `DataCard` as a JSON string.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization fails.
    #[wyrd_test_contract_macros::critical("python:DataCard.model_dump_json")]
    #[pyo3(name = "model_dump_json")]
    pub fn model_dump_json_py(&self) -> CardPyResult<String> {
        self.model_dump_json()
    }

    /// Return this `DataCard` as a Python dictionary.
    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let value = serde_json::to_value(self.to_card_envelope())?;
        wyrd_utils::py::json_to_pyobject(py, &value).map_err(Into::into)
    }

    /// Return the single card-envelope JSON conversion used by registry
    /// adapters.
    #[pyo3(name = "_to_card_envelope_json")]
    pub fn to_card_envelope_json_py(&self) -> CardPyResult<String> {
        self.model_dump_json()
    }

    /// Return a pretty JSON representation for interactive inspection.
    pub fn __str__(&self) -> String {
        wyrd_utils::json::pretty_json_string(&self.to_card_envelope())
    }

    /// Build a `DataCard` from serialized JSON and an optional interface hook.
    ///
    /// When `interface` is omitted, Wyrd rebuilds built-in interfaces from
    /// serialized spec metadata. Python subclass-backed cards must pass
    /// `interface=YourInterface` so Wyrd can call
    /// `YourInterface.from_metadata(...)`; initialized interface instances are
    /// also accepted for lower-level tests and internal callers.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON parsing or Card validation fails, or the
    /// serialized custom interface cannot be rebuilt without an explicit
    /// Python interface object.
    #[wyrd_test_contract_macros::critical("python:DataCard.model_validate_json")]
    #[staticmethod]
    #[pyo3(name = "model_validate_json", signature = (json_string, interface=None))]
    pub fn model_validate_json_py(
        py: Python<'_>,
        json_string: &str,
        interface: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<Self> {
        let mut card = Self::from_card(serde_json::from_str(json_string)?)?;
        card.hydrate_interface(py, interface)?;
        Ok(card)
    }

    // justification: pyo3 boundary; the extractor produces an owned value (PathBuf/PyRef/newtype), taking it by reference would require a caller-side clone
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
impl DataCard {
    fn attach_data(&mut self, py: Python<'_>, data: Option<&Bound<'_, PyAny>>) -> CardPyResult<()> {
        if let Some(data) = data {
            let (interface, card_ref) =
                DataCardInput::extract_bound(data, true)?.into_parts(py, &self.metadata)?;
            if let Some(handle) = interface {
                self.metadata.interface = handle.to_spec_interface(py)?;
                self.metadata.schema = infer_schema_from_handle(&handle, py)?;
                self.metadata.sql = sql_logic_from_handle(&handle, py)?;
                self.interface = Some(handle.into_py_any(py)?);
            }
            if let Some(card_ref) = card_ref {
                self.metadata.card_refs.push(card_ref);
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
    InterfaceClass(Py<PyAny>),
    Artifact(CardRef),
}

#[cfg(feature = "python")]
impl DataCardInput {
    fn extract_bound(data: &Bound<'_, PyAny>, allow_interface_class: bool) -> CardPyResult<Self> {
        if data.is_instance_of::<CardRefPy>() {
            let card_ref = data.extract::<CardRefPy>()?.0;
            if card_ref.kind != CardKind::Artifact {
                return Err(WyrdPyError::validation_with_details(
                    "DataCard Artifact input requires a CardRef with kind=Artifact",
                    json!({
                        "field": "data",
                        "expected_kind": "Artifact",
                        "actual_kind": card_ref.kind.wire_name(),
                    }),
                ));
            }
            return Ok(Self::Artifact(card_ref));
        }

        if data.is_instance_of::<DataInterface>() {
            return Ok(Self::Interface(DataInterfaceHandle::from_interface(data)?));
        }

        if let Ok(interface_type) = data.cast::<PyType>()
            && interface_type.is_subclass_of::<DataInterface>()?
        {
            if !allow_interface_class {
                return Err(WyrdPyError::validation(
                    "DataCard construction requires live data or an initialized DataInterface instance; pass interface classes to retrieval surfaces such as cards.get(..., interface=YourInterface)",
                ));
            }
            return Ok(Self::InterfaceClass(data.clone().unbind()));
        }

        Ok(Self::Raw(data.clone().unbind()))
    }

    fn into_parts(
        self,
        py: Python<'_>,
        metadata: &DataCardMetadata,
    ) -> CardPyResult<(Option<DataInterfaceHandle>, Option<CardRef>)> {
        match self {
            Self::Raw(data) => Ok((
                Some(DataInterfaceHandle::from_raw(py, data.bind(py))?),
                None,
            )),
            Self::Interface(interface) => Ok((Some(interface), None)),
            Self::InterfaceClass(interface_class) => {
                let interface = interface_class
                    .bind(py)
                    .call_method1("from_metadata", (metadata.clone(),))?;
                Ok((Some(DataInterfaceHandle::from_interface(&interface)?), None))
            }
            Self::Artifact(card_ref) => Ok((None, Some(card_ref))),
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
fn sql_logic_from_handle(
    handle: &DataInterfaceHandle,
    py: Python<'_>,
) -> CardPyResult<Option<SqlLogic>> {
    match handle {
        DataInterfaceHandle::Sql(_) => sql_logic_from_data(py, handle.source_ref()).map(Some),
        _ => Ok(None),
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
        RustDataInterface::Custom(_) => {
            return Err(WyrdPyError::validation(
                "custom Python DataInterface cannot be rebuilt from JSON without a class; pass interface=YourInterface to DataCard.model_validate_json or cards.get",
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
    WyrdPyError::validation_with_details(
        "invalid DataCard metadata label or annotation",
        json!({ "reason": error.to_string() }),
    )
}

#[cfg(feature = "python")]
fn write_card_json_file(card: &DataCard, path: &std::path::Path) -> CardPyResult<()> {
    wyrd_utils::json::write_json_sorted(path.join("card.json"), &card.to_card_envelope())
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
fn normalize_load_kwargs<'py>(
    value: Option<&Bound<'py, PyAny>>,
) -> CardPyResult<Option<Bound<'py, PyDict>>> {
    value
        .map(|value| {
            if value.is_instance_of::<PyDict>() {
                Ok(value.cast::<PyDict>()?.clone())
            } else {
                let converted = value.call_method0("to_dict")?;
                Ok(converted.cast::<PyDict>()?.clone())
            }
        })
        .transpose()
}

fn data_spec_from_metadata(metadata: &DataCardMetadata, interface: RustDataInterface) -> DataSpec {
    // Skip validation — used by model_dump_json for draft cards where stats are
    // placeholder zeros. Validation runs on the deserialization path
    // (model_validate_json → spec.validate()).
    DataSpec {
        interface,
        schema: metadata.schema.clone(),
        card_refs: metadata.card_refs.iter().cloned().map(Ref::Ref).collect(),
        splits: metadata.splits.clone(),
        target_columns: metadata.target_columns.clone(),
        sql: metadata.sql.clone(),
        stats: metadata.stats.clone(),
        publishes_to: Vec::new(),
    }
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
    use wyrd_spec::envelope::{CardKind, Spec};
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
                card_refs: Vec::new(),
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

        let spec = card.to_data_spec_from_metadata();
        assert_eq!(spec.interface_kind(), "Pandas");
        assert_eq!(spec.schema.columns.len(), 1);
        assert_eq!(spec.stats().byte_count, 128);

        let body = card.to_rust_card_body_from_metadata();
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

    #[test]
    fn as_card_ref_returns_err_for_empty_name() {
        let card = DataCard {
            space: "default".to_string(),
            name: String::new(),
            version: "0.1.0".to_string(),
            uid: String::new(),
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            metadata: DataCardMetadata::default(),
            created_at: DateTime::<Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp is valid"),
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        };
        assert!(card.as_card_ref().is_err());
    }

    #[test]
    fn as_card_ref_returns_err_for_invalid_version() {
        let card = DataCard {
            space: "default".to_string(),
            name: "my-dataset".to_string(),
            version: "not-semver".to_string(),
            uid: String::new(),
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            metadata: DataCardMetadata::default(),
            created_at: DateTime::<Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp is valid"),
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        };
        assert!(card.as_card_ref().is_err());
    }

    #[test]
    fn as_card_ref_returns_data_kind() {
        let mut labels = BTreeMap::new();
        labels.insert(
            LabelKey::new("domain").expect("static label key is valid"),
            LabelValue::new("churn").expect("static label value is valid"),
        );
        let card = DataCard {
            space: "default".to_string(),
            name: "data".to_string(),
            version: "0.1.0".to_string(),
            uid: "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2".to_string(),
            labels,
            annotations: BTreeMap::new(),
            metadata: DataCardMetadata::default(),
            created_at: DateTime::<Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp is valid"),
            is_card: true,
            #[cfg(feature = "python")]
            interface: None,
        };

        let card_ref = card.as_card_ref().expect("identity is valid");

        assert_eq!(card_ref.kind, CardKind::Data);
        assert_eq!(card_ref.name.as_str(), "data");
        assert_eq!(card_ref.version.as_str(), "0.1.0");
    }
}
