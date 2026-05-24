//! Python-facing data interface holders and Rust metadata conversion.

use std::collections::BTreeMap;
#[cfg(feature = "python")]
use std::fs;
#[cfg(feature = "python")]
use std::path::{Path, PathBuf};

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyAny, PyDict, PyModule, PyTuple};
#[cfg(feature = "python")]
use serde::Serialize;
#[cfg(feature = "python")]
use wyrd_utils::py::{json_to_pyobject, module_version, pyobject_to_json};

#[cfg(feature = "python")]
use crate::data::dtype::{self, DataSourceKind};
#[cfg(feature = "python")]
use crate::data::io::{
    ImageManifest, ManifestEntry, TextManifest, custom_load_kwargs, huggingface_pointer,
    image_manifest_from_data, image_manifest_schema, import_custom_loader, manifest_json_to_py,
    pointer_to_kwargs, read_huggingface_pointer, read_jsonl_to_py, serde_json_file_to_py,
    sql_logic_from_data, string_map_to_kwargs, text_manifest_from_data, text_manifest_schema,
    write_jsonl_normalized,
};
#[cfg(feature = "python")]
use crate::data::layout::LocalArtifactLayout;
#[cfg(feature = "python")]
use crate::data::stats::PyDataStats;
use crate::error::{CardPyResult, WyrdPyError};
#[cfg(feature = "python")]
use wyrd_spec::card::data::DataStats;
use wyrd_spec::card::data::{
    ArrowFormat, ArrowMeta, ColorMode, CustomDataMeta, DataInterface as RustDataInterface,
    DataSchema, HuggingfaceMeta, ImageFormat, ImageMeta, JsonlCompression, JsonlMeta, NumpyFormat,
    NumpyMeta, PandasMeta, ParquetCompression, ParquetMeta, PolarsMeta, SqlMeta, TextMeta,
    TorchMeta, TorchSaveFormat,
};
use wyrd_spec::reference::CardRef;

/// Base class for Python data interfaces.
///
/// Subclass this class to provide custom Python-only data materialization.
/// Custom subclasses must override `save` and `load`; the base implementations
/// raise validation errors so missing overrides fail loudly.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", subclass))]
pub struct DataInterface {
    kind: String,
}

#[cfg(feature = "python")]
#[pymethods]
impl DataInterface {
    /// Create a base data interface instance for Python subclasses.
    ///
    /// This initializer exists so `class MyInterface(DataInterface)` can call
    /// `super().__init__()` without satisfying a built-in interface
    /// constructor. Built-in interfaces bypass this initializer and set their
    /// own stable kind markers.
    ///
    /// # Arguments
    ///
    /// * `args` - Positional arguments accepted for Python subclass
    ///   compatibility and ignored by the base class.
    /// * `kwargs` - Keyword arguments accepted for Python subclass
    ///   compatibility and ignored by the base class.
    ///
    /// # Returns
    ///
    /// A base interface marker with kind `Custom`.
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn __new__(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        let _ = (args, kwargs);
        Self {
            kind: "Custom".to_string(),
        }
    }

    /// Initialize a Python data interface subclass.
    ///
    /// The base initializer is intentionally a no-op. It allows custom Python
    /// subclasses to call `super().__init__(...)` while keeping materialization
    /// behavior owned by the subclass implementation.
    ///
    /// # Arguments
    ///
    /// * `args` - Positional arguments supplied by a Python subclass.
    /// * `kwargs` - Keyword arguments supplied by a Python subclass.
    #[pyo3(signature = (*args, **kwargs))]
    fn __init__(&mut self, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) {
        let _ = (args, kwargs);
    }

    /// Return the stable interface kind used in DataCard metadata.
    ///
    /// # Returns
    ///
    /// The Wyrd data interface kind. Python subclasses report `Custom`.
    #[getter]
    fn kind(&self) -> &str {
        &self.kind
    }

    /// Save custom data into a local DataCard artifact directory.
    ///
    /// Custom Python subclasses must override this method. The base
    /// implementation never writes files; it raises a validation error so a
    /// missing override fails before a `DataCard` can silently record an empty
    /// artifact.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory containing the local DataCard materialization. A
    ///   custom implementation should write all artifact bytes under this
    ///   directory using the layout documented by the subclass.
    /// * `save_kwargs` - Optional Python keyword arguments passed through from
    ///   `DataCard.save(...)` for custom implementation-specific behavior.
    ///
    /// # Returns
    ///
    /// A `DataStats` object describing the bytes written by the custom
    /// implementation.
    ///
    /// # Errors
    ///
    /// Always returns `WYRD_DATA_400_VALIDATION` from the base class because
    /// subclasses are required to implement their own save behavior.
    #[pyo3(signature = (path, save_kwargs=None))]
    fn save(
        &self,
        path: PathBuf,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<PyDataStats> {
        let _ = (path, save_kwargs);
        Err(WyrdPyError::validation(
            "DataInterface.save must be implemented by a concrete interface",
        ))
    }

    /// Load custom data from a local DataCard artifact directory.
    ///
    /// Custom Python subclasses must override this method. The base
    /// implementation never mutates the interface; it raises a validation error
    /// so missing reload behavior is explicit to both developers and agents.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory containing the local DataCard materialization.
    /// * `load_kwargs` - Optional Python keyword arguments passed through from
    ///   `DataCard.load(...)` for custom implementation-specific behavior.
    ///
    /// # Errors
    ///
    /// Always returns `WYRD_DATA_400_VALIDATION` from the base class because
    /// subclasses are required to implement their own load behavior.
    #[pyo3(signature = (path, load_kwargs=None))]
    fn load(&mut self, path: PathBuf, load_kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<()> {
        let _ = (path, load_kwargs);
        Err(WyrdPyError::validation(
            "DataInterface.load must be implemented by a concrete interface",
        ))
    }
}

impl DataInterface {
    fn marker(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
        }
    }
}

macro_rules! interface_struct {
    ($(#[$meta:meta])+ $name:ident { $($field:ident : $field_ty:ty),+ $(,)? }) => {
        $(#[$meta])+
        #[cfg_attr(feature = "python", pyclass(module = "wyrd.data", extends = DataInterface))]
        pub struct $name {
            $($field: $field_ty),+
        }
    };
}

interface_struct!(
/// Data interface for pandas DataFrame values.
///
/// `save` validates a pandas DataFrame, writes `data/data.parquet` with the
/// configured parquet compression, and returns deterministic `DataStats`.
/// `load` reads the local parquet artifact back into a pandas DataFrame.
PandasInterface {
    data: Option<Py<PyAny>>,
    compression: String,
});

interface_struct!(
/// Data interface for polars DataFrame values.
///
/// `save` validates a polars DataFrame, writes `data/data.parquet` with the
/// configured parquet compression, and returns deterministic `DataStats`.
/// `load` reads the local parquet artifact back into a polars DataFrame.
PolarsInterface {
    data: Option<Py<PyAny>>,
    compression: String,
});

interface_struct!(
/// Data interface for PyArrow table values.
///
/// `save` validates a `pyarrow.Table` and writes either `data/data.parquet` or
/// `data/data.arrow` based on `format`. `load` restores the table from the
/// matching local artifact.
ArrowInterface {
    data: Option<Py<PyAny>>,
    format: String,
});

interface_struct!(
/// Data interface for an existing parquet file or table-like parquet source.
///
/// `save` copies a path-like parquet file or writes a table-like source to
/// `data/data.parquet`. `load` restores the local artifact as a PyArrow table.
ParquetInterface {
    data: Option<Py<PyAny>>,
    compression: String,
    row_group_size: Option<u32>,
});

interface_struct!(
/// Data interface for NumPy ndarray values.
///
/// `save` validates a NumPy array and writes `data/data.npy` or
/// `data/data.npz` based on `format`. `load` restores the saved array with
/// pickle loading disabled.
NumpyInterface {
    data: Option<Py<PyAny>>,
    dtype: Option<String>,
    shape: Option<Vec<i64>>,
    format: String,
});

interface_struct!(
/// Data interface for Torch tensors or tensor mappings.
///
/// `save` validates a tensor or mapping and writes `data/data.safetensors` or
/// `data/data.pt` based on `save_format`. `load` restores the saved tensor
/// payload from the local artifact.
TorchInterface {
    data: Option<Py<PyAny>>,
    save_format: String,
});

interface_struct!(
/// Data interface for SQL query bundles.
///
/// `save` serializes SQL logic to `data/sql.json` with deterministic key
/// ordering. `load` reads that JSON artifact back into a Python object.
SqlInterface {
    data: Option<Py<PyAny>>,
    dialect: String,
    connection_hint: Option<String>,
});

interface_struct!(
/// Data interface for JSON Lines data.
///
/// `save` normalizes a path or iterable of JSON objects to the configured JSONL
/// artifact path, including gzip or zstd compression when requested. `load`
/// reads the local JSONL artifact back into Python JSON objects.
JsonlInterface {
    data: Option<Py<PyAny>>,
    compression: String,
    lines_per_file: Option<u64>,
});

interface_struct!(
/// Data interface for image datasets represented by file manifests.
///
/// `save` writes `data/manifest.json` from a directory, path list, or manifest
/// value and can copy referenced bytes when `save_kwargs["copy_bytes"]` is
/// true. `load` reads the manifest JSON back into Python.
ImageInterface {
    data: Option<Py<PyAny>>,
    format: String,
    color_mode: String,
    manifest_ref: Option<CardRef>,
});

interface_struct!(
/// Data interface for text datasets represented by file manifests.
///
/// `save` writes `data/manifest.json` from a directory, path list, or manifest
/// value and can copy referenced bytes when `save_kwargs["copy_bytes"]` is
/// true. `load` reads the manifest JSON back into Python.
TextInterface {
    data: Option<Py<PyAny>>,
    encoding: String,
    manifest_ref: Option<CardRef>,
});

interface_struct!(
/// Data interface for Hugging Face datasets.
///
/// `save` writes a local dataset to `data/dataset` when live data is present,
/// or writes a pinned remote pointer to `data/dataset_pointer.json` when only
/// metadata is available. `load` restores local datasets or requires
/// `load_kwargs["allow_remote"] = True` before loading a remote pointer.
HuggingfaceInterface {
    data: Option<Py<PyAny>>,
    dataset_id: String,
    revision: Option<String>,
    split: Option<String>,
    config: Option<String>,
});

interface_struct!(
/// Data interface for declared custom Python loaders.
///
/// `save` imports `loader_module.loader_class` and calls its `save` method with
/// the configured `extra` keyword arguments. `load` calls the same loader's
/// `load` method against the local `data/custom` directory.
CustomDataInterface {
    data: Option<Py<PyAny>>,
    loader_module: String,
    loader_class: String,
    extra: BTreeMap<String, String>,
});

#[cfg(feature = "python")]
macro_rules! impl_interface_methods {
    ($type:ty { $($item:item)* }) => {
        #[pymethods]
        impl $type {
            $($item)*

            /// Return whether this interface currently holds live Python source data.
            ///
            /// # Returns
            ///
            /// `true` when the interface contains a Python object that can be
            /// saved into the local DataCard artifact layout. Sourceless
            /// interfaces reconstructed from metadata return `false` until
            /// `load` attaches data.
            #[getter]
            fn has_source(&self) -> bool {
                self.data.is_some()
            }

            /// Return the Rust metadata representation as a Python dictionary.
            ///
            /// # Returns
            ///
            /// A JSON-compatible dictionary containing the interface metadata
            /// that will be stored in the DataCard spec.
            ///
            /// # Errors
            ///
            /// Returns a Wyrd data validation error when interface options such
            /// as compression, serialization format, or color mode are invalid.
            fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
                interface_to_dict(py, self.to_spec_interface(py)?)
            }

            /// Save this interface's source data into the local DataCard layout.
            ///
            /// The method validates the held Python object, materializes bytes
            /// under `path`, and returns deterministic byte statistics for the
            /// artifact that was written. The concrete interface controls the
            /// exact convention path, for example `data/data.parquet`,
            /// `data/data.npy`, or `data/manifest.json`.
            ///
            /// # Arguments
            ///
            /// * `path` - Directory containing the local DataCard
            ///   materialization.
            /// * `save_kwargs` - Optional Python keyword arguments for
            ///   interface-specific behavior. Manifest-backed interfaces use
            ///   `copy_bytes`; other built-ins currently ignore this value.
            ///
            /// # Returns
            ///
            /// A `DataStats` object containing byte count, digest, and any
            /// schema-derived column count available for the saved artifact.
            ///
            /// # Errors
            ///
            /// Returns a Wyrd error when the interface has no live source data,
            /// the source object does not match the expected framework type,
            /// an option value is invalid, or local filesystem materialization
            /// fails.
            #[pyo3(signature = (path, save_kwargs=None))]
            fn save(
                &self,
                py: Python<'_>,
                path: PathBuf,
                save_kwargs: Option<&Bound<'_, PyDict>>,
            ) -> CardPyResult<PyDataStats> {
                self.save_inner(py, &path, save_kwargs).map(PyDataStats::from)
            }

            /// Load this interface's source data from the local DataCard layout.
            ///
            /// The method reads the convention path for the concrete interface
            /// from `path` and attaches the loaded Python object back to the
            /// interface instance. For pinned remote Hugging Face pointers,
            /// callers must explicitly pass `allow_remote=true` in
            /// `load_kwargs`.
            ///
            /// # Arguments
            ///
            /// * `path` - Directory containing the local DataCard
            ///   materialization.
            /// * `load_kwargs` - Optional Python keyword arguments for
            ///   interface-specific behavior.
            ///
            /// # Errors
            ///
            /// Returns a Wyrd error when the expected local artifact is
            /// missing, framework deserialization fails, remote loading is not
            /// explicitly allowed, or local filesystem access fails.
            #[pyo3(signature = (path, load_kwargs=None))]
            fn load(
                &mut self,
                py: Python<'_>,
                path: PathBuf,
                load_kwargs: Option<&Bound<'_, PyDict>>,
            ) -> CardPyResult<()> {
                self.load_inner(py, &path, load_kwargs)
            }
        }
    };
}

#[cfg(feature = "python")]
impl_interface_methods!(PandasInterface {
    /// Create a pandas data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional pandas DataFrame to materialize when saving.
    /// * `compression` - Parquet compression codec. Accepted values are
    ///   `none`, `snappy`, `gzip`, `zstd`, and `lz4`.
    ///
    /// # Returns
    ///
    /// A pandas interface with kind `Pandas`.
    #[new]
    #[pyo3(signature = (*, data=None, compression="snappy"))]
    fn __new__(data: Option<Py<PyAny>>, compression: &str) -> (Self, DataInterface) {
        (
            Self {
                data,
                compression: compression.to_string(),
            },
            DataInterface::marker("Pandas"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(PolarsInterface {
    /// Create a polars data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional polars DataFrame to materialize when saving.
    /// * `compression` - Parquet compression codec. Accepted values are
    ///   `none`, `snappy`, `gzip`, `zstd`, and `lz4`.
    ///
    /// # Returns
    ///
    /// A polars interface with kind `Polars`.
    #[new]
    #[pyo3(signature = (*, data=None, compression="snappy"))]
    fn __new__(data: Option<Py<PyAny>>, compression: &str) -> (Self, DataInterface) {
        (
            Self {
                data,
                compression: compression.to_string(),
            },
            DataInterface::marker("Polars"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(ArrowInterface {
    /// Create a PyArrow table interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional `pyarrow.Table` to materialize when saving.
    /// * `format` - Serialization format. Accepted values are `parquet` and
    ///   `ipc`.
    ///
    /// # Returns
    ///
    /// An Arrow interface with kind `Arrow`.
    #[new]
    #[pyo3(signature = (*, data=None, format="parquet"))]
    fn __new__(data: Option<Py<PyAny>>, format: &str) -> (Self, DataInterface) {
        (
            Self {
                data,
                format: format.to_string(),
            },
            DataInterface::marker("Arrow"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(ParquetInterface {
    /// Create a parquet data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional parquet path or table-like object to materialize
    ///   when saving.
    /// * `compression` - Parquet compression codec for table-like writes.
    /// * `row_group_size` - Optional declared parquet row group size metadata.
    ///
    /// # Returns
    ///
    /// A parquet interface with kind `Parquet`.
    #[new]
    #[pyo3(signature = (*, data=None, compression="snappy", row_group_size=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        compression: &str,
        row_group_size: Option<u32>,
    ) -> (Self, DataInterface) {
        (
            Self {
                data,
                compression: compression.to_string(),
                row_group_size,
            },
            DataInterface::marker("Parquet"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(NumpyInterface {
    /// Create a NumPy data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional NumPy ndarray to materialize when saving.
    /// * `dtype` - Optional declared dtype. When omitted, Wyrd attempts to
    ///   infer it from `data`.
    /// * `shape` - Optional declared array shape. When omitted, Wyrd attempts
    ///   to infer it from `data`.
    /// * `format` - Serialization format. Accepted values are `npy` and `npz`.
    ///
    /// # Returns
    ///
    /// A NumPy interface with kind `Numpy`.
    #[new]
    #[pyo3(signature = (*, data=None, dtype=None, shape=None, format="npy"))]
    fn __new__(
        data: Option<Py<PyAny>>,
        dtype: Option<String>,
        shape: Option<Vec<i64>>,
        format: &str,
    ) -> (Self, DataInterface) {
        (
            Self {
                data,
                dtype,
                shape,
                format: format.to_string(),
            },
            DataInterface::marker("Numpy"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(TorchInterface {
    /// Create a Torch data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional Torch tensor or mapping of tensor values to
    ///   materialize when saving.
    /// * `save_format` - Serialization format. Accepted values are
    ///   `safetensors` and `pickle`.
    ///
    /// # Returns
    ///
    /// A Torch interface with kind `Torch`.
    #[new]
    #[pyo3(signature = (*, data=None, save_format="safetensors"))]
    fn __new__(data: Option<Py<PyAny>>, save_format: &str) -> (Self, DataInterface) {
        (
            Self {
                data,
                save_format: save_format.to_string(),
            },
            DataInterface::marker("Torch"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(SqlInterface {
    /// Create a SQL data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional SQL query bundle or JSON-compatible SQL logic.
    /// * `dialect` - SQL dialect label recorded in the DataCard spec.
    /// * `connection_hint` - Optional human-readable connection hint.
    ///
    /// # Returns
    ///
    /// A SQL interface with kind `Sql`.
    #[new]
    #[pyo3(signature = (*, data=None, dialect, connection_hint=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        dialect: String,
        connection_hint: Option<String>,
    ) -> (Self, DataInterface) {
        (
            Self {
                data,
                dialect,
                connection_hint,
            },
            DataInterface::marker("Sql"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(JsonlInterface {
    /// Create a JSON Lines data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional path or iterable of JSON-compatible records.
    /// * `compression` - JSONL compression mode. Accepted values are `none`,
    ///   `gzip`, and `zstd`.
    /// * `lines_per_file` - Optional declared line-count partition size.
    ///
    /// # Returns
    ///
    /// A JSON Lines interface with kind `Jsonl`.
    #[new]
    #[pyo3(signature = (*, data=None, compression="none", lines_per_file=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        compression: &str,
        lines_per_file: Option<u64>,
    ) -> (Self, DataInterface) {
        (
            Self {
                data,
                compression: compression.to_string(),
                lines_per_file,
            },
            DataInterface::marker("Jsonl"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(ImageInterface {
    /// Create an image manifest data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional directory, path iterable, or manifest-like value.
    /// * `format` - Image format family. Accepted values are `png`, `jpeg`,
    ///   `webp`, and `mixed`.
    /// * `color_mode` - Declared color mode. Accepted values are `rgb`, `rgba`,
    ///   and `grayscale`.
    /// * `manifest_ref` - Optional CardRef pointing at an external manifest
    ///   card.
    ///
    /// # Returns
    ///
    /// An image interface with kind `Image`.
    ///
    /// # Errors
    ///
    /// Returns a Wyrd error when `manifest_ref` cannot be parsed as a CardRef.
    #[new]
    #[pyo3(signature = (*, data=None, format="mixed", color_mode="rgb", manifest_ref=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        format: &str,
        color_mode: &str,
        manifest_ref: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<(Self, DataInterface)> {
        Ok((
            Self {
                data,
                format: format.to_string(),
                color_mode: color_mode.to_string(),
                manifest_ref: parse_card_ref(manifest_ref)?,
            },
            DataInterface::marker("Image"),
        ))
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(TextInterface {
    /// Create a text manifest data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional directory, path iterable, or manifest-like value.
    /// * `encoding` - Text encoding label recorded in the DataCard spec.
    /// * `manifest_ref` - Optional CardRef pointing at an external manifest
    ///   card.
    ///
    /// # Returns
    ///
    /// A text interface with kind `Text`.
    ///
    /// # Errors
    ///
    /// Returns a Wyrd error when `manifest_ref` cannot be parsed as a CardRef.
    #[new]
    #[pyo3(signature = (*, data=None, encoding="utf-8", manifest_ref=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        encoding: &str,
        manifest_ref: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<(Self, DataInterface)> {
        Ok((
            Self {
                data,
                encoding: encoding.to_string(),
                manifest_ref: parse_card_ref(manifest_ref)?,
            },
            DataInterface::marker("Text"),
        ))
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(HuggingfaceInterface {
    /// Create a Hugging Face dataset interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional Hugging Face dataset object to materialize locally.
    /// * `dataset_id` - Dataset identifier used for metadata and pointer-only
    ///   saves.
    /// * `revision` - Optional pinned dataset revision. Required for
    ///   pointer-only remote saves.
    /// * `split` - Optional dataset split.
    /// * `config` - Optional dataset config name.
    ///
    /// # Returns
    ///
    /// A Hugging Face interface with kind `Huggingface`.
    #[new]
    #[pyo3(signature = (*, data=None, dataset_id, revision=None, split=None, config=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        dataset_id: String,
        revision: Option<String>,
        split: Option<String>,
        config: Option<String>,
    ) -> (Self, DataInterface) {
        (
            Self {
                data,
                dataset_id,
                revision,
                split,
                config,
            },
            DataInterface::marker("Huggingface"),
        )
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(CustomDataInterface {
    /// Create a declared custom loader interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional Python source object passed to the custom loader's
    ///   `save` method.
    /// * `loader_module` - Importable Python module containing the custom
    ///   loader class.
    /// * `loader_class` - Loader class name to import from `loader_module`.
    /// * `extra` - Optional string keyword arguments passed to the custom
    ///   loader.
    ///
    /// # Returns
    ///
    /// A custom declared-loader interface with kind `Custom`.
    #[new]
    #[pyo3(signature = (*, data=None, loader_module, loader_class, extra=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        loader_module: String,
        loader_class: String,
        extra: Option<BTreeMap<String, String>>,
    ) -> (Self, DataInterface) {
        (
            Self {
                data,
                loader_module,
                loader_class,
                extra: extra.unwrap_or_default(),
            },
            DataInterface::marker("Custom"),
        )
    }
});

macro_rules! impl_to_spec {
    ($type:ty, $meta:ty, $variant:ident, $message:literal, $body:expr) => {
        impl $type {
            /// Convert this local Python holder into Rust-only metadata.
            #[cfg(feature = "python")]
            pub fn to_rust(&self, py: Python<'_>) -> CardPyResult<$meta> {
                $body(self, py)
            }

            /// Convert this local Python holder into a Rust-only interface enum.
            #[cfg(feature = "python")]
            pub fn to_spec_interface(&self, py: Python<'_>) -> CardPyResult<RustDataInterface> {
                Ok(RustDataInterface::$variant(self.to_rust(py)?))
            }

            /// Rebuild a sourceless Python holder from Rust-only metadata.
            pub fn from_spec_inner(interface: &RustDataInterface) -> CardPyResult<Self> {
                match interface {
                    RustDataInterface::$variant(meta) => Ok(Self::from_meta(meta)),
                    _ => Err(WyrdPyError::validation($message)),
                }
            }
        }
    };
}

impl_to_spec!(
    PandasInterface,
    PandasMeta,
    Pandas,
    "PandasInterface metadata must contain Pandas metadata",
    |value: &PandasInterface, py| {
        Ok(PandasMeta {
            framework_version: module_version(py, "pandas")?
                .unwrap_or_else(|| "unknown".to_string()),
            compression: parse_parquet_compression(&value.compression)?,
        })
    }
);

impl PandasInterface {
    fn from_meta(meta: &PandasMeta) -> Self {
        Self {
            data: None,
            compression: parquet_compression_token(meta.compression).to_string(),
        }
    }
}

impl_to_spec!(
    PolarsInterface,
    PolarsMeta,
    Polars,
    "PolarsInterface metadata must contain Polars metadata",
    |value: &PolarsInterface, py| {
        Ok(PolarsMeta {
            framework_version: module_version(py, "polars")?
                .unwrap_or_else(|| "unknown".to_string()),
            compression: parse_parquet_compression(&value.compression)?,
        })
    }
);

impl PolarsInterface {
    fn from_meta(meta: &PolarsMeta) -> Self {
        Self {
            data: None,
            compression: parquet_compression_token(meta.compression).to_string(),
        }
    }
}

impl_to_spec!(
    ArrowInterface,
    ArrowMeta,
    Arrow,
    "ArrowInterface metadata must contain Arrow metadata",
    |value: &ArrowInterface, py| {
        Ok(ArrowMeta {
            framework_version: module_version(py, "pyarrow")?
                .unwrap_or_else(|| "unknown".to_string()),
            format: parse_arrow_format(&value.format)?,
        })
    }
);

impl ArrowInterface {
    fn from_meta(meta: &ArrowMeta) -> Self {
        Self {
            data: None,
            format: arrow_format_token(meta.format).to_string(),
        }
    }
}

impl_to_spec!(
    ParquetInterface,
    ParquetMeta,
    Parquet,
    "ParquetInterface metadata must contain Parquet metadata",
    |value: &ParquetInterface, _py| {
        Ok(ParquetMeta {
            compression: parse_parquet_compression(&value.compression)?,
            row_group_size: value.row_group_size,
        })
    }
);

impl ParquetInterface {
    fn from_meta(meta: &ParquetMeta) -> Self {
        Self {
            data: None,
            compression: parquet_compression_token(meta.compression).to_string(),
            row_group_size: meta.row_group_size,
        }
    }
}

impl_to_spec!(
    NumpyInterface,
    NumpyMeta,
    Numpy,
    "NumpyInterface metadata must contain Numpy metadata",
    |value: &NumpyInterface, py| {
        let dtype = if let Some(dtype) = value.dtype.clone() {
            dtype
        } else if let Some(inferred) = dtype::numpy_dtype(py, value.data.as_ref()) {
            inferred?
        } else {
            "unknown".to_string()
        };
        let shape = if let Some(shape) = value.shape.clone() {
            shape
        } else if let Some(inferred) = dtype::numpy_shape(py, value.data.as_ref()) {
            inferred?
        } else {
            Vec::new()
        };
        Ok(NumpyMeta {
            dtype,
            shape,
            format: parse_numpy_format(&value.format)?,
        })
    }
);

impl NumpyInterface {
    fn from_meta(meta: &NumpyMeta) -> Self {
        Self {
            data: None,
            dtype: Some(meta.dtype.clone()),
            shape: Some(meta.shape.clone()),
            format: numpy_format_token(meta.format).to_string(),
        }
    }
}

impl_to_spec!(
    TorchInterface,
    TorchMeta,
    Torch,
    "TorchInterface metadata must contain Torch metadata",
    |value: &TorchInterface, py| {
        Ok(TorchMeta {
            framework_version: module_version(py, "torch")?
                .unwrap_or_else(|| "unknown".to_string()),
            save_format: parse_torch_save_format(&value.save_format)?,
        })
    }
);

impl TorchInterface {
    fn from_meta(meta: &TorchMeta) -> Self {
        Self {
            data: None,
            save_format: torch_save_format_token(meta.save_format).to_string(),
        }
    }
}

impl_to_spec!(
    SqlInterface,
    SqlMeta,
    Sql,
    "SqlInterface metadata must contain Sql metadata",
    |value: &SqlInterface, _py| {
        Ok(SqlMeta {
            dialect: value.dialect.clone(),
            connection_hint: value.connection_hint.clone(),
        })
    }
);

impl SqlInterface {
    fn from_meta(meta: &SqlMeta) -> Self {
        Self {
            data: None,
            dialect: meta.dialect.clone(),
            connection_hint: meta.connection_hint.clone(),
        }
    }
}

impl_to_spec!(
    JsonlInterface,
    JsonlMeta,
    Jsonl,
    "JsonlInterface metadata must contain Jsonl metadata",
    |value: &JsonlInterface, _py| {
        Ok(JsonlMeta {
            compression: parse_jsonl_compression(&value.compression)?,
            lines_per_file: value.lines_per_file,
        })
    }
);

impl JsonlInterface {
    fn from_meta(meta: &JsonlMeta) -> Self {
        Self {
            data: None,
            compression: jsonl_compression_token(meta.compression).to_string(),
            lines_per_file: meta.lines_per_file,
        }
    }
}

impl_to_spec!(
    ImageInterface,
    ImageMeta,
    Image,
    "ImageInterface metadata must contain Image metadata",
    |value: &ImageInterface, _py| {
        Ok(ImageMeta {
            format: parse_image_format(&value.format)?,
            manifest_ref: value.manifest_ref.clone(),
            color_mode: parse_color_mode(&value.color_mode)?,
        })
    }
);

impl ImageInterface {
    fn from_meta(meta: &ImageMeta) -> Self {
        Self {
            data: None,
            format: image_format_token(meta.format).to_string(),
            color_mode: color_mode_token(meta.color_mode).to_string(),
            manifest_ref: meta.manifest_ref.clone(),
        }
    }
}

impl_to_spec!(
    TextInterface,
    TextMeta,
    Text,
    "TextInterface metadata must contain Text metadata",
    |value: &TextInterface, _py| {
        Ok(TextMeta {
            encoding: value.encoding.clone(),
            manifest_ref: value.manifest_ref.clone(),
        })
    }
);

impl TextInterface {
    fn from_meta(meta: &TextMeta) -> Self {
        Self {
            data: None,
            encoding: meta.encoding.clone(),
            manifest_ref: meta.manifest_ref.clone(),
        }
    }
}

impl_to_spec!(
    HuggingfaceInterface,
    HuggingfaceMeta,
    Huggingface,
    "HuggingfaceInterface metadata must contain Huggingface metadata",
    |value: &HuggingfaceInterface, _py| {
        Ok(HuggingfaceMeta {
            dataset_id: value.dataset_id.clone(),
            revision: value.revision.clone(),
            split: value.split.clone(),
            config: value.config.clone(),
        })
    }
);

impl HuggingfaceInterface {
    fn from_meta(meta: &HuggingfaceMeta) -> Self {
        Self {
            data: None,
            dataset_id: meta.dataset_id.clone(),
            revision: meta.revision.clone(),
            split: meta.split.clone(),
            config: meta.config.clone(),
        }
    }
}

impl_to_spec!(
    CustomDataInterface,
    CustomDataMeta,
    Custom,
    "CustomDataInterface metadata must contain Custom metadata",
    |value: &CustomDataInterface, _py| {
        Ok(CustomDataMeta {
            loader_module: value.loader_module.clone(),
            loader_class: value.loader_class.clone(),
            extra: value.extra.clone(),
        })
    }
);

impl CustomDataInterface {
    fn from_meta(meta: &CustomDataMeta) -> Self {
        Self {
            data: None,
            loader_module: meta.loader_module.clone(),
            loader_class: meta.loader_class.clone(),
            extra: meta.extra.clone(),
        }
    }
}

#[cfg(feature = "python")]
impl PandasInterface {
    /// Save the held pandas dataframe to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for pandas calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future pandas-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a pandas
    /// DataFrame, compression is invalid, pandas parquet writing fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("PandasInterface.save requires a pandas.DataFrame")
        })?;
        dtype::ensure_pandas_dataframe(py, data.bind(py))?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        let compression = parse_parquet_compression(&self.compression)?;
        let compression_py = parquet_compression_token(compression);
        let kwargs = PyDict::new(py);
        kwargs.set_item("engine", "pyarrow")?;
        kwargs.set_item("compression", compression_py)?;
        data.bind(py)
            .call_method("to_parquet", (&absolute_path,), Some(&kwargs))?;
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Pandas")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a pandas dataframe from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for pandas calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future pandas-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/data.parquet` is missing or pandas cannot
    /// read the parquet artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("engine", "pyarrow")?;
        let pandas = py.import("pandas")?;
        let loaded = pandas.call_method("read_parquet", (&absolute_path,), Some(&kwargs))?;
        self.data = Some(loaded.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl PolarsInterface {
    /// Save the held polars dataframe to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for polars calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future polars-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a polars
    /// DataFrame, compression is invalid, polars parquet writing fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("PolarsInterface.save requires a polars.DataFrame")
        })?;
        dtype::ensure_polars_dataframe(py, data.bind(py))?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        let compression = parse_parquet_compression(&self.compression)?;
        let compression_py = parquet_compression_token(compression);
        data.bind(py)
            .call_method1("write_parquet", (&absolute_path, compression_py))?;
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Polars")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a polars dataframe from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for polars calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future polars-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/data.parquet` is missing or polars cannot
    /// read the parquet artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        let polars = py.import("polars")?;
        self.data = Some(
            polars
                .call_method1("read_parquet", (&absolute_path,))?
                .unbind(),
        );
        Ok(())
    }
}

#[cfg(feature = "python")]
impl ArrowInterface {
    /// Save the held PyArrow table to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future Arrow-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet` or `data/data.arrow`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a
    /// `pyarrow.Table`, format is invalid, PyArrow serialization fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("ArrowInterface.save requires a pyarrow.Table")
        })?;
        dtype::ensure_pyarrow_table(py, data.bind(py))?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        match parse_arrow_format(&self.format)? {
            ArrowFormat::Parquet => {
                let parquet = py.import("pyarrow.parquet")?;
                parquet.call_method1("write_table", (data.bind(py), &absolute_path))?;
            }
            ArrowFormat::Ipc => {
                let pyarrow = py.import("pyarrow")?;
                let ipc = py.import("pyarrow.ipc")?;
                let sink = pyarrow.call_method1("OSFile", (&absolute_path, "wb"))?;
                let writer =
                    ipc.call_method1("new_file", (&sink, data.bind(py).getattr("schema")?))?;
                writer.call_method1("write_table", (data.bind(py),))?;
                writer.call_method0("close")?;
                sink.call_method0("close")?;
            }
        }
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Arrow")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a PyArrow table from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future Arrow-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected Arrow artifact is missing, format is
    /// invalid, or PyArrow cannot deserialize the artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(match parse_arrow_format(&self.format)? {
            ArrowFormat::Parquet => py
                .import("pyarrow.parquet")?
                .call_method1("read_table", (&absolute_path,))?
                .unbind(),
            ArrowFormat::Ipc => {
                let ipc = py.import("pyarrow.ipc")?;
                let reader = ipc.call_method1("open_file", (&absolute_path,))?;
                reader.call_method0("read_all")?.unbind()
            }
        });
        Ok(())
    }
}

#[cfg(feature = "python")]
impl ParquetInterface {
    /// Save a parquet path or table-like source to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for path detection or PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future parquet-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, a path-like source is not
    /// a local file, compression is invalid, table serialization fails, or
    /// local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "ParquetInterface.save requires a parquet path or table-like source",
            )
        })?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        if dtype::is_path_like(py, data.bind(py))? {
            let source = dtype::extract_pathbuf(data.bind(py))?;
            require_local_file(&source)?;
            fs::copy(&source, &absolute_path)?;
        } else {
            let compression = parse_parquet_compression(&self.compression)?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("compression", parquet_compression_token(compression))?;
            let parquet = py.import("pyarrow.parquet")?;
            parquet.call_method(
                "write_table",
                (data.bind(py), &absolute_path),
                Some(&kwargs),
            )?;
        }
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Parquet")
            .unwrap_or_else(|_| DataSchema::empty());
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a PyArrow table from the local parquet artifact.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future parquet-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/data.parquet` is missing or PyArrow cannot
    /// read the parquet artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(
            py.import("pyarrow.parquet")?
                .call_method1("read_table", (&absolute_path,))?
                .unbind(),
        );
        Ok(())
    }
}

#[cfg(feature = "python")]
impl NumpyInterface {
    /// Save the held NumPy array to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for NumPy calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future NumPy-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.npy` or `data/data.npz`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a NumPy
    /// ndarray, format is invalid, NumPy serialization fails, schema inference
    /// fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("NumpyInterface.save requires a numpy.ndarray")
        })?;
        dtype::ensure_numpy_array(py, data.bind(py))?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        let numpy = py.import("numpy")?;
        match parse_numpy_format(&self.format)? {
            NumpyFormat::Npy => {
                let kwargs = PyDict::new(py);
                kwargs.set_item("allow_pickle", false)?;
                numpy.call_method("save", (&absolute_path, data.bind(py)), Some(&kwargs))?;
            }
            NumpyFormat::Npz => {
                let kwargs = PyDict::new(py);
                kwargs.set_item("value", data.bind(py))?;
                numpy.call_method("savez", (&absolute_path,), Some(&kwargs))?;
            }
        }
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Numpy")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a NumPy array from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for NumPy calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future NumPy-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected NumPy artifact is missing, format is
    /// invalid, or NumPy cannot deserialize the artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        let numpy = py.import("numpy")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("allow_pickle", false)?;
        self.data = Some(match parse_numpy_format(&self.format)? {
            NumpyFormat::Npy => numpy
                .call_method("load", (&absolute_path,), Some(&kwargs))?
                .unbind(),
            NumpyFormat::Npz => {
                let loaded = numpy.call_method("load", (&absolute_path,), Some(&kwargs))?;
                loaded.get_item("value")?.unbind()
            }
        });
        Ok(())
    }
}

#[cfg(feature = "python")]
impl TorchInterface {
    /// Save the held Torch tensor or mapping to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for Torch or safetensors calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future Torch-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.safetensors` or `data/data.pt`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a Torch
    /// tensor or tensor mapping, save format is invalid, serialization fails,
    /// or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "TorchInterface.save requires a torch.Tensor or tensor mapping",
            )
        })?;
        dtype::ensure_torch_tensor_or_mapping(py, data.bind(py))?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        match parse_torch_save_format(&self.save_format)? {
            TorchSaveFormat::Safetensors => {
                py.import("safetensors.torch")?.call_method1(
                    "save_file",
                    (torch_to_safetensor_map(py, data.bind(py))?, &absolute_path),
                )?;
            }
            TorchSaveFormat::Pickle => {
                py.import("torch")?
                    .call_method1("save", (data.bind(py), &absolute_path))?;
            }
        }
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Torch")
            .unwrap_or_else(|_| DataSchema::empty());
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a Torch tensor or mapping from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for Torch or safetensors calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future Torch-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected Torch artifact is missing, save
    /// format is invalid, or framework deserialization fails.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(match parse_torch_save_format(&self.save_format)? {
            TorchSaveFormat::Safetensors => py
                .import("safetensors.torch")?
                .call_method1("load_file", (&absolute_path,))?
                .unbind(),
            TorchSaveFormat::Pickle => {
                let kwargs = PyDict::new(py);
                kwargs.set_item("weights_only", true)?;
                py.import("torch")?
                    .call_method("load", (&absolute_path,), Some(&kwargs))?
                    .unbind()
            }
        });
        Ok(())
    }
}

#[cfg(feature = "python")]
impl SqlInterface {
    /// Save the SQL query bundle to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future SQL-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/sql.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when the SQL payload cannot be converted to Wyrd SQL
    /// logic, JSON writing fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let query_bundle = sql_logic_from_data(py, self.data.as_ref())?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        write_json_sorted(&absolute_path, &query_bundle)?;
        let schema = DataSchema::empty();
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load the SQL query bundle from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future SQL-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/sql.json` is missing or cannot be parsed as
    /// JSON.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(serde_json_file_to_py(py, &absolute_path)?.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl JsonlInterface {
    /// Save JSON Lines data to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future JSONL-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for the configured JSONL artifact path.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, compression is invalid,
    /// JSONL normalization fails, schema inference fails, or local stats cannot
    /// be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "JsonlInterface.save requires a path, iterable of dicts, or file-like object",
            )
        })?;
        let compression = parse_jsonl_compression(&self.compression)?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        write_jsonl_normalized(py, data.bind(py).clone(), &absolute_path, compression)?;
        let schema = dtype::infer_schema_for_interface(py, data.bind(py), "Jsonl")
            .unwrap_or_else(|_| DataSchema::empty());
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load JSON Lines data from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `load_kwargs` - Optional loader-specific keyword arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured JSONL artifact is missing,
    /// compression is invalid, or JSONL decoding fails.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let compression = parse_jsonl_compression(&self.compression)?;
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(
            read_jsonl_to_py(
                py,
                &absolute_path,
                Some(jsonl_compression_token(compression)),
                load_kwargs,
            )?
            .unbind(),
        );
        Ok(())
    }
}

#[cfg(feature = "python")]
impl ImageInterface {
    /// Save the image manifest, optionally copying referenced bytes.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for manifest conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `save_kwargs` - Optional keyword arguments. `copy_bytes=true` copies
    ///   referenced files into `data/images`.
    ///
    /// # Returns
    ///
    /// File statistics for `data/manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, format or color mode is
    /// invalid, manifest creation fails, byte copying fails, JSON writing
    /// fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "ImageInterface.save requires a directory, paths, or manifest",
            )
        })?;
        let copy_bytes = bool_kwarg(save_kwargs, "copy_bytes")?;
        let manifest = image_manifest_from_data(
            py,
            data.bind(py).clone(),
            parse_image_format(&self.format)?,
            parse_color_mode(&self.color_mode)?,
        )?;
        if copy_bytes {
            copy_manifest_files(&manifest, &path.join("data/images"))?;
        }
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        write_json_sorted(&absolute_path, &manifest)?;
        let schema = image_manifest_schema();
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load the image manifest from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future image-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/manifest.json` is missing or cannot be
    /// parsed as JSON.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(manifest_json_to_py(py, &absolute_path)?.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl TextInterface {
    /// Save the text manifest, optionally copying referenced bytes.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for manifest conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `save_kwargs` - Optional keyword arguments. `copy_bytes=true` copies
    ///   referenced files into `data/files`.
    ///
    /// # Returns
    ///
    /// File statistics for `data/manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, manifest creation fails,
    /// byte copying fails, JSON writing fails, or local stats cannot be
    /// computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "TextInterface.save requires a directory, paths, or manifest",
            )
        })?;
        let copy_bytes = bool_kwarg(save_kwargs, "copy_bytes")?;
        let manifest = text_manifest_from_data(py, data.bind(py).clone(), &self.encoding)?;
        if copy_bytes {
            copy_manifest_files(&manifest, &path.join("data/files"))?;
        }
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        write_json_sorted(&absolute_path, &manifest)?;
        let schema = text_manifest_schema();
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load the text manifest from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future text-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/manifest.json` is missing or cannot be
    /// parsed as JSON.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(manifest_json_to_py(py, &absolute_path)?.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl HuggingfaceInterface {
    /// Save a local Hugging Face dataset or pinned remote pointer.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for dataset calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future Hugging Face-specific save
    ///   options.
    ///
    /// # Returns
    ///
    /// Path statistics for `data/dataset` or `data/dataset_pointer.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when pointer-only save lacks a pinned revision, local
    /// dataset serialization fails, pointer JSON writing fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let artifact_path = if let Some(data) = self.data.as_ref() {
            let dataset_path = path.join("data/dataset");
            fs::create_dir_all(path.join("data"))?;
            data.bind(py)
                .call_method1("save_to_disk", (&dataset_path,))?;
            dataset_path
        } else {
            let revision = self.revision.as_ref().ok_or_else(|| {
                WyrdPyError::validation("Huggingface pointer-only save requires a pinned revision")
            })?;
            let absolute_path = self.to_spec_interface(py)?.artifact_path(path)?;
            write_json_sorted(
                &absolute_path,
                &huggingface_pointer(
                    &self.dataset_id,
                    revision,
                    self.split.as_deref(),
                    self.config.as_deref(),
                ),
            )?;
            absolute_path
        };
        let schema = self
            .data
            .as_ref()
            .and_then(|data| {
                dtype::infer_schema_for_interface(py, data.bind(py), "Huggingface").ok()
            })
            .unwrap_or_else(DataSchema::empty);
        data_stats_for_path(&artifact_path, Some(&schema))
    }

    /// Load a local Hugging Face dataset or caller-approved pinned remote pointer.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for `datasets` calls.
    /// * `path` - Local DataCard materialization root.
    /// * `load_kwargs` - Optional keyword arguments. Remote pointer loading
    ///   requires `allow_remote=true`.
    ///
    /// # Errors
    ///
    /// Returns an error when neither a local dataset nor pointer exists, remote
    /// loading is not explicitly allowed, pointer parsing fails, or the
    /// `datasets` package cannot load the dataset.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let datasets = py.import("datasets")?;
        let pointer_path = path.join("data/dataset_pointer.json");
        let dataset_dir = path.join("data/dataset");
        self.data = Some(if pointer_path.exists() {
            let allow_remote = bool_kwarg(load_kwargs, "allow_remote")?;
            if !allow_remote {
                return Err(WyrdPyError::validation(
                    "Remote HuggingFace load requires load_kwargs.allow_remote=true",
                ));
            }
            let pointer = read_huggingface_pointer(&pointer_path)?;
            let kwargs = pointer_to_kwargs(py, pointer)?;
            datasets
                .call_method("load_dataset", (), Some(&kwargs))?
                .unbind()
        } else {
            require_local_path(&dataset_dir)?;
            datasets
                .call_method1("load_from_disk", (&dataset_dir,))?
                .unbind()
        });
        Ok(())
    }
}

#[cfg(feature = "python")]
impl CustomDataInterface {
    /// Save custom data through the declared custom loader.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for importing and calling the loader.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future declared-loader save options.
    ///
    /// # Returns
    ///
    /// Path statistics for `data/custom`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the loader cannot be
    /// imported, loader `save` fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let data = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("CustomDataInterface.save requires source data")
        })?;
        let custom_dir = self.to_spec_interface(py)?.artifact_path(path)?;
        fs::create_dir_all(&custom_dir)?;
        let loader = import_custom_loader(py, &self.loader_module, &self.loader_class)?;
        let kwargs = string_map_to_kwargs(py, &self.extra)?;
        loader.call_method("save", (data.bind(py), &custom_dir), Some(&kwargs))?;
        let schema = DataSchema::empty();
        data_stats_for_path(&custom_dir, Some(&schema))
    }

    /// Load custom data through the declared custom loader.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for importing and calling the loader.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future declared-loader load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/custom` is missing, the loader cannot be
    /// imported, or loader `load` fails.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let custom_dir = self.to_spec_interface(py)?.artifact_path(path)?;
        require_local_path(&custom_dir)?;
        let loader = import_custom_loader(py, &self.loader_module, &self.loader_class)?;
        let metadata = BTreeMap::new();
        let kwargs = custom_load_kwargs(py, &self.extra, &metadata)?;
        self.data = Some(
            loader
                .call_method("load", (&custom_dir,), Some(&kwargs))?
                .unbind(),
        );
        Ok(())
    }
}

/// Python-compatible dispatch wrapper for local data interface holders.
#[cfg(feature = "python")]
#[allow(dead_code)]
pub(crate) enum DataInterfaceHandle {
    /// Pandas dataframe interface.
    Pandas(PandasInterface),
    /// Polars dataframe interface.
    Polars(PolarsInterface),
    /// `PyArrow` table interface.
    Arrow(ArrowInterface),
    /// Parquet path or table interface.
    Parquet(ParquetInterface),
    /// `NumPy` array interface.
    Numpy(NumpyInterface),
    /// Torch tensor interface.
    Torch(TorchInterface),
    /// SQL query bundle interface.
    Sql(SqlInterface),
    /// JSON Lines interface.
    Jsonl(JsonlInterface),
    /// Image manifest interface.
    Image(ImageInterface),
    /// Text manifest interface.
    Text(TextInterface),
    /// Hugging Face dataset interface.
    Huggingface(HuggingfaceInterface),
    /// Custom loader interface.
    Custom(CustomDataInterface),
    /// Python subclass of the base `DataInterface`.
    Subclass(Py<PyAny>),
}

#[cfg(feature = "python")]
#[allow(dead_code)]
impl DataInterfaceHandle {
    /// Build a handle from an explicit Python `DataInterface` object.
    pub fn from_interface(interface: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        macro_rules! extract_interface {
            ($type:ty, $variant:ident) => {
                if interface.is_instance_of::<$type>() {
                    let value = interface.extract::<PyRef<'_, $type>>()?;
                    return Ok(Self::$variant(value.clone_for_handle(interface.py())));
                }
            };
        }

        extract_interface!(PandasInterface, Pandas);
        extract_interface!(PolarsInterface, Polars);
        extract_interface!(ArrowInterface, Arrow);
        extract_interface!(ParquetInterface, Parquet);
        extract_interface!(NumpyInterface, Numpy);
        extract_interface!(TorchInterface, Torch);
        extract_interface!(SqlInterface, Sql);
        extract_interface!(JsonlInterface, Jsonl);
        extract_interface!(ImageInterface, Image);
        extract_interface!(TextInterface, Text);
        extract_interface!(HuggingfaceInterface, Huggingface);
        extract_interface!(CustomDataInterface, Custom);

        if interface.is_instance_of::<DataInterface>() {
            return Ok(Self::Subclass(interface.clone().unbind()));
        }

        Err(WyrdPyError::validation(
            "DataCard requires a supported data interface",
        ))
    }

    /// Detect and build a default interface holder from raw Python data.
    pub fn from_raw(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        match dtype::detect_data_source(py, data)? {
            DataSourceKind::Pandas => Ok(Self::Pandas(PandasInterface {
                data: Some(data.clone().unbind()),
                compression: "snappy".to_string(),
            })),
            DataSourceKind::Polars => Ok(Self::Polars(PolarsInterface {
                data: Some(data.clone().unbind()),
                compression: "snappy".to_string(),
            })),
            DataSourceKind::Arrow => Ok(Self::Arrow(ArrowInterface {
                data: Some(data.clone().unbind()),
                format: "parquet".to_string(),
            })),
            DataSourceKind::ParquetPath => Ok(Self::Parquet(ParquetInterface {
                data: Some(data.clone().unbind()),
                compression: "snappy".to_string(),
                row_group_size: None,
            })),
            DataSourceKind::Numpy => Ok(Self::Numpy(NumpyInterface {
                data: Some(data.clone().unbind()),
                dtype: None,
                shape: None,
                format: "npy".to_string(),
            })),
            DataSourceKind::Torch => Ok(Self::Torch(TorchInterface {
                data: Some(data.clone().unbind()),
                save_format: "safetensors".to_string(),
            })),
            DataSourceKind::Sql => Ok(Self::Sql(SqlInterface {
                data: Some(data.clone().unbind()),
                dialect: "sql".to_string(),
                connection_hint: None,
            })),
            DataSourceKind::JsonlPath => Ok(Self::Jsonl(JsonlInterface {
                data: Some(data.clone().unbind()),
                compression: dtype::jsonl_compression_from_path(data)?,
                lines_per_file: None,
            })),
            DataSourceKind::ImageDirectory => Ok(Self::Image(ImageInterface {
                data: Some(data.clone().unbind()),
                format: "mixed".to_string(),
                color_mode: "rgb".to_string(),
                manifest_ref: None,
            })),
            DataSourceKind::TextDirectory => Ok(Self::Text(TextInterface {
                data: Some(data.clone().unbind()),
                encoding: "utf-8".to_string(),
                manifest_ref: None,
            })),
            DataSourceKind::Huggingface => Ok(Self::Huggingface(HuggingfaceInterface {
                data: Some(data.clone().unbind()),
                dataset_id: huggingface_dataset_id(data)?,
                revision: huggingface_optional_attr(data, &["revision"]),
                split: huggingface_optional_attr(data, &["split"]),
                config: huggingface_optional_attr(data, &["config_name", "config"]),
            })),
        }
    }

    /// Convert this holder into Rust-only interface metadata.
    pub fn to_spec_interface(&self, py: Python<'_>) -> CardPyResult<RustDataInterface> {
        match self {
            Self::Pandas(value) => value.to_spec_interface(py),
            Self::Polars(value) => value.to_spec_interface(py),
            Self::Arrow(value) => value.to_spec_interface(py),
            Self::Parquet(value) => value.to_spec_interface(py),
            Self::Numpy(value) => value.to_spec_interface(py),
            Self::Torch(value) => value.to_spec_interface(py),
            Self::Sql(value) => value.to_spec_interface(py),
            Self::Jsonl(value) => value.to_spec_interface(py),
            Self::Image(value) => value.to_spec_interface(py),
            Self::Text(value) => value.to_spec_interface(py),
            Self::Huggingface(value) => value.to_spec_interface(py),
            Self::Custom(value) => value.to_spec_interface(py),
            Self::Subclass(_) => Ok(RustDataInterface::Custom(CustomDataMeta {
                loader_module: String::new(),
                loader_class: String::new(),
                extra: BTreeMap::new(),
            })),
        }
    }

    /// Convert this handle back into a Python interface object.
    pub fn into_py_any(self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        macro_rules! into_py {
            ($value:expr, $kind:literal) => {
                Ok(Py::new(py, ($value, DataInterface::marker($kind)))?.into_any())
            };
        }
        match self {
            Self::Pandas(value) => into_py!(value, "Pandas"),
            Self::Polars(value) => into_py!(value, "Polars"),
            Self::Arrow(value) => into_py!(value, "Arrow"),
            Self::Parquet(value) => into_py!(value, "Parquet"),
            Self::Numpy(value) => into_py!(value, "Numpy"),
            Self::Torch(value) => into_py!(value, "Torch"),
            Self::Sql(value) => into_py!(value, "Sql"),
            Self::Jsonl(value) => into_py!(value, "Jsonl"),
            Self::Image(value) => into_py!(value, "Image"),
            Self::Text(value) => into_py!(value, "Text"),
            Self::Huggingface(value) => into_py!(value, "Huggingface"),
            Self::Custom(value) => into_py!(value, "Custom"),
            Self::Subclass(value) => Ok(value),
        }
    }

    /// Take the held Python source object, if one exists.
    pub fn take_source(&mut self) -> Option<Py<PyAny>> {
        match self {
            Self::Pandas(value) => value.data.take(),
            Self::Polars(value) => value.data.take(),
            Self::Arrow(value) => value.data.take(),
            Self::Parquet(value) => value.data.take(),
            Self::Numpy(value) => value.data.take(),
            Self::Torch(value) => value.data.take(),
            Self::Sql(value) => value.data.take(),
            Self::Jsonl(value) => value.data.take(),
            Self::Image(value) => value.data.take(),
            Self::Text(value) => value.data.take(),
            Self::Huggingface(value) => value.data.take(),
            Self::Custom(value) => value.data.take(),
            Self::Subclass(_) => None,
        }
    }

    /// Borrow the held Python source object, if one exists.
    pub fn source_ref(&self) -> Option<&Py<PyAny>> {
        match self {
            Self::Pandas(value) => value.data.as_ref(),
            Self::Polars(value) => value.data.as_ref(),
            Self::Arrow(value) => value.data.as_ref(),
            Self::Parquet(value) => value.data.as_ref(),
            Self::Numpy(value) => value.data.as_ref(),
            Self::Torch(value) => value.data.as_ref(),
            Self::Sql(value) => value.data.as_ref(),
            Self::Jsonl(value) => value.data.as_ref(),
            Self::Image(value) => value.data.as_ref(),
            Self::Text(value) => value.data.as_ref(),
            Self::Huggingface(value) => value.data.as_ref(),
            Self::Custom(value) => value.data.as_ref(),
            Self::Subclass(_) => None,
        }
    }

    /// Infer a `DataSchema` from this holder's source object.
    pub fn infer_schema(
        &self,
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
    ) -> CardPyResult<DataSchema> {
        if matches!(self, Self::Subclass(_)) {
            return Ok(DataSchema::empty());
        }
        dtype::infer_schema_for_interface(py, data, self.kind())
    }

    /// Return the stable interface kind for this handle.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Pandas(_) => "Pandas",
            Self::Polars(_) => "Polars",
            Self::Arrow(_) => "Arrow",
            Self::Parquet(_) => "Parquet",
            Self::Numpy(_) => "Numpy",
            Self::Torch(_) => "Torch",
            Self::Sql(_) => "Sql",
            Self::Jsonl(_) => "Jsonl",
            Self::Image(_) => "Image",
            Self::Text(_) => "Text",
            Self::Huggingface(_) => "Huggingface",
            Self::Custom(_) => "Custom",
            Self::Subclass(_) => "Custom",
        }
    }
}

/// Parse a parquet compression token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_parquet_compression(value: &str) -> CardPyResult<ParquetCompression> {
    match normalize_option(value).as_str() {
        "none" => Ok(ParquetCompression::None),
        "snappy" => Ok(ParquetCompression::Snappy),
        "gzip" => Ok(ParquetCompression::Gzip),
        "zstd" => Ok(ParquetCompression::Zstd),
        "lz4" => Ok(ParquetCompression::Lz4),
        got => Err(WyrdPyError::invalid_interface_option(
            "compression",
            got,
            ["none", "snappy", "gzip", "zstd", "lz4"],
        )),
    }
}

/// Parse an Arrow serialization format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_arrow_format(value: &str) -> CardPyResult<ArrowFormat> {
    match normalize_option(value).as_str() {
        "ipc" => Ok(ArrowFormat::Ipc),
        "parquet" => Ok(ArrowFormat::Parquet),
        got => Err(WyrdPyError::invalid_interface_option(
            "format",
            got,
            ["ipc", "parquet"],
        )),
    }
}

/// Parse a `NumPy` serialization format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_numpy_format(value: &str) -> CardPyResult<NumpyFormat> {
    match normalize_option(value).as_str() {
        "npy" => Ok(NumpyFormat::Npy),
        "npz" => Ok(NumpyFormat::Npz),
        got => Err(WyrdPyError::invalid_interface_option(
            "format",
            got,
            ["npy", "npz"],
        )),
    }
}

/// Parse a Torch save format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_torch_save_format(value: &str) -> CardPyResult<TorchSaveFormat> {
    match normalize_option(value).as_str() {
        "safetensors" => Ok(TorchSaveFormat::Safetensors),
        "pickle" => Ok(TorchSaveFormat::Pickle),
        got => Err(WyrdPyError::invalid_interface_option(
            "save_format",
            got,
            ["safetensors", "pickle"],
        )),
    }
}

/// Parse a JSON Lines compression token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_jsonl_compression(value: &str) -> CardPyResult<JsonlCompression> {
    match normalize_option(value).as_str() {
        "none" => Ok(JsonlCompression::None),
        "gzip" => Ok(JsonlCompression::Gzip),
        "zstd" => Ok(JsonlCompression::Zstd),
        got => Err(WyrdPyError::invalid_interface_option(
            "compression",
            got,
            ["none", "gzip", "zstd"],
        )),
    }
}

/// Parse an image format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_image_format(value: &str) -> CardPyResult<ImageFormat> {
    match normalize_option(value).as_str() {
        "png" => Ok(ImageFormat::Png),
        "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::Webp),
        "mixed" => Ok(ImageFormat::Mixed),
        got => Err(WyrdPyError::invalid_interface_option(
            "format",
            got,
            ["png", "jpeg", "webp", "mixed"],
        )),
    }
}

/// Parse an image color mode token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_color_mode(value: &str) -> CardPyResult<ColorMode> {
    match normalize_option(value).as_str() {
        "rgb" => Ok(ColorMode::Rgb),
        "rgba" => Ok(ColorMode::Rgba),
        "grayscale" => Ok(ColorMode::Grayscale),
        got => Err(WyrdPyError::invalid_interface_option(
            "color_mode",
            got,
            ["rgb", "rgba", "grayscale"],
        )),
    }
}

#[cfg(feature = "python")]
fn interface_to_dict(
    py: Python<'_>,
    interface: RustDataInterface,
) -> CardPyResult<Bound<'_, PyDict>> {
    let value = serde_json::to_value(interface)?;
    let dict = PyDict::new(py);
    if let serde_json::Value::Object(values) = value {
        for (key, value) in values {
            dict.set_item(key, json_to_pyobject(py, &value)?)?;
        }
    }
    Ok(dict)
}

#[cfg(feature = "python")]
fn require_local_file(path: &Path) -> CardPyResult<()> {
    wyrd_utils::fs::require_local_file(path).map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
fn require_local_path(path: &Path) -> CardPyResult<()> {
    wyrd_utils::fs::require_local_path(path).map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
fn data_stats_for_file(path: &Path, schema: Option<&DataSchema>) -> CardPyResult<DataStats> {
    wyrd_utils::fs::data_stats_for_file(path, schema)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
fn data_stats_for_path(path: &Path, schema: Option<&DataSchema>) -> CardPyResult<DataStats> {
    wyrd_utils::fs::data_stats_for_path(path, schema)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
fn write_json_sorted<T: Serialize>(path: impl AsRef<Path>, value: &T) -> CardPyResult<()> {
    wyrd_utils::json::write_json_sorted(path, value)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
fn ensure_parent_dir(path: &Path) -> CardPyResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(feature = "python")]
fn bool_kwarg(kwargs: Option<&Bound<'_, PyDict>>, name: &str) -> CardPyResult<bool> {
    let Some(kwargs) = kwargs else {
        return Ok(false);
    };
    Ok(kwargs
        .get_item(name)?
        .map(|value| value.extract::<bool>())
        .transpose()?
        .unwrap_or(false))
}

#[cfg(feature = "python")]
trait FileManifest {
    fn files(&self) -> &[ManifestEntry];
}

#[cfg(feature = "python")]
impl FileManifest for ImageManifest {
    fn files(&self) -> &[ManifestEntry] {
        &self.files
    }
}

#[cfg(feature = "python")]
impl FileManifest for TextManifest {
    fn files(&self) -> &[ManifestEntry] {
        &self.files
    }
}

#[cfg(feature = "python")]
fn copy_manifest_files(manifest: &impl FileManifest, dest: &Path) -> CardPyResult<()> {
    fs::create_dir_all(dest)?;
    for entry in manifest.files() {
        let source = PathBuf::from(&entry.path);
        require_local_file(&source)?;
        let file_name = source.file_name().ok_or_else(|| {
            WyrdPyError::validation_with_details(
                "manifest file entries must include a file name",
                serde_json::json!({ "path": entry.path }),
            )
        })?;
        fs::copy(&source, dest.join(file_name))?;
    }
    Ok(())
}

#[cfg(feature = "python")]
fn torch_to_safetensor_map<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> CardPyResult<Bound<'py, PyDict>> {
    let values = PyDict::new(py);
    if data.hasattr("items")? {
        for item in data.call_method0("items")?.try_iter()? {
            let item = item?;
            let tuple = item.cast::<PyTuple>()?;
            if tuple.len() != 2 {
                return Err(WyrdPyError::validation(
                    "Torch tensor mappings must yield key/value pairs",
                ));
            }
            let key = tuple.get_item(0)?.extract::<String>()?;
            values.set_item(key, tuple.get_item(1)?)?;
        }
    } else {
        values.set_item("value", data)?;
    }
    Ok(values)
}

#[cfg(feature = "python")]
fn parse_card_ref(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<Option<CardRef>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(None);
    };
    let raw = pyobject_to_json(value)?;
    serde_json::from_value(raw).map(Some).map_err(Into::into)
}

#[cfg(feature = "python")]
#[allow(dead_code)]
fn huggingface_dataset_id(data: &Bound<'_, PyAny>) -> CardPyResult<String> {
    huggingface_optional_attr(data, &["dataset_id", "repo_id", "path"])
        .or_else(|| {
            data.getattr("info").ok().and_then(|info| {
                huggingface_optional_attr(&info, &["dataset_name", "builder_name"])
            })
        })
        .ok_or_else(|| {
            WyrdPyError::interface_metadata_required(
                "HuggingfaceInterface requires dataset_id when it cannot be inferred from data",
            )
        })
}

#[cfg(feature = "python")]
#[allow(dead_code)]
fn huggingface_optional_attr(data: &Bound<'_, PyAny>, names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(value) = data.getattr(*name) {
            if value.is_none() {
                continue;
            }
            if let Ok(text) = value.extract::<String>() {
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn normalize_option(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn parquet_compression_token(value: ParquetCompression) -> &'static str {
    match value {
        ParquetCompression::None => "none",
        ParquetCompression::Snappy => "snappy",
        ParquetCompression::Gzip => "gzip",
        ParquetCompression::Zstd => "zstd",
        ParquetCompression::Lz4 => "lz4",
    }
}

fn arrow_format_token(value: ArrowFormat) -> &'static str {
    match value {
        ArrowFormat::Ipc => "ipc",
        ArrowFormat::Parquet => "parquet",
    }
}

fn numpy_format_token(value: NumpyFormat) -> &'static str {
    match value {
        NumpyFormat::Npy => "npy",
        NumpyFormat::Npz => "npz",
    }
}

fn torch_save_format_token(value: TorchSaveFormat) -> &'static str {
    match value {
        TorchSaveFormat::Safetensors => "safetensors",
        TorchSaveFormat::Pickle => "pickle",
    }
}

fn jsonl_compression_token(value: JsonlCompression) -> &'static str {
    match value {
        JsonlCompression::None => "none",
        JsonlCompression::Gzip => "gzip",
        JsonlCompression::Zstd => "zstd",
    }
}

fn image_format_token(value: ImageFormat) -> &'static str {
    match value {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
        ImageFormat::Mixed => "mixed",
    }
}

fn color_mode_token(value: ColorMode) -> &'static str {
    match value {
        ColorMode::Rgb => "rgb",
        ColorMode::Rgba => "rgba",
        ColorMode::Grayscale => "grayscale",
    }
}

#[cfg(feature = "python")]
macro_rules! impl_clone_for_handle {
    ($type:ty { $($field:ident),+ $(,)? }) => {
        impl $type {
            #[allow(dead_code)]
            fn clone_for_handle(&self, py: Python<'_>) -> Self {
                Self {
                    data: self.data.as_ref().map(|data| data.clone_ref(py)),
                    $($field: self.$field.clone()),+
                }
            }
        }
    };
}

#[cfg(feature = "python")]
impl_clone_for_handle!(PandasInterface { compression });
#[cfg(feature = "python")]
impl_clone_for_handle!(PolarsInterface { compression });
#[cfg(feature = "python")]
impl_clone_for_handle!(ArrowInterface { format });
#[cfg(feature = "python")]
impl_clone_for_handle!(ParquetInterface {
    compression,
    row_group_size
});
#[cfg(feature = "python")]
impl_clone_for_handle!(NumpyInterface {
    dtype,
    shape,
    format
});
#[cfg(feature = "python")]
impl_clone_for_handle!(TorchInterface { save_format });
#[cfg(feature = "python")]
impl_clone_for_handle!(SqlInterface {
    dialect,
    connection_hint
});
#[cfg(feature = "python")]
impl_clone_for_handle!(JsonlInterface {
    compression,
    lines_per_file
});
#[cfg(feature = "python")]
impl_clone_for_handle!(ImageInterface {
    format,
    color_mode,
    manifest_ref
});
#[cfg(feature = "python")]
impl_clone_for_handle!(TextInterface {
    encoding,
    manifest_ref
});
#[cfg(feature = "python")]
impl_clone_for_handle!(HuggingfaceInterface {
    dataset_id,
    revision,
    split,
    config
});
#[cfg(feature = "python")]
impl_clone_for_handle!(CustomDataInterface {
    loader_module,
    loader_class,
    extra
});

/// Register data interface classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<DataInterface>()?;
    module.add_class::<PandasInterface>()?;
    module.add_class::<PolarsInterface>()?;
    module.add_class::<ArrowInterface>()?;
    module.add_class::<ParquetInterface>()?;
    module.add_class::<NumpyInterface>()?;
    module.add_class::<TorchInterface>()?;
    module.add_class::<SqlInterface>()?;
    module.add_class::<JsonlInterface>()?;
    module.add_class::<ImageInterface>()?;
    module.add_class::<TextInterface>()?;
    module.add_class::<HuggingfaceInterface>()?;
    module.add_class::<CustomDataInterface>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_arrow_format, parse_color_mode, parse_image_format, parse_jsonl_compression,
        parse_numpy_format, parse_parquet_compression, parse_torch_save_format,
    };
    use crate::error::WyrdPyError;
    use wyrd_spec::card::data::{
        ArrowFormat, ColorMode, ImageFormat, JsonlCompression, NumpyFormat, ParquetCompression,
        TorchSaveFormat,
    };

    #[test]
    fn parses_locked_interface_options() {
        assert_eq!(
            parse_parquet_compression("snappy").expect("valid compression"),
            ParquetCompression::Snappy
        );
        assert_eq!(
            parse_arrow_format("ipc").expect("valid arrow format"),
            ArrowFormat::Ipc
        );
        assert_eq!(
            parse_numpy_format("npz").expect("valid numpy format"),
            NumpyFormat::Npz
        );
        assert_eq!(
            parse_torch_save_format("pickle").expect("valid torch format"),
            TorchSaveFormat::Pickle
        );
        assert_eq!(
            parse_jsonl_compression("zstd").expect("valid jsonl compression"),
            JsonlCompression::Zstd
        );
        assert_eq!(
            parse_image_format("webp").expect("valid image format"),
            ImageFormat::Webp
        );
        assert_eq!(
            parse_color_mode("rgba").expect("valid color mode"),
            ColorMode::Rgba
        );
    }

    #[test]
    fn rejects_unknown_interface_options_with_wyrd_code() {
        assert_invalid_interface_option(
            parse_parquet_compression("brotli").expect_err("invalid compression"),
        );
        assert_invalid_interface_option(parse_arrow_format("feather").expect_err("invalid format"));
        assert_invalid_interface_option(parse_numpy_format("txt").expect_err("invalid format"));
        assert_invalid_interface_option(
            parse_torch_save_format("unsafe_pickle").expect_err("invalid format"),
        );
        assert_invalid_interface_option(
            parse_jsonl_compression("zip").expect_err("invalid compression"),
        );
        assert_invalid_interface_option(parse_image_format("tiff").expect_err("invalid format"));
        assert_invalid_interface_option(parse_color_mode("cmyk").expect_err("invalid color mode"));
    }

    fn assert_invalid_interface_option(error: WyrdPyError) {
        match error {
            WyrdPyError::Spec(error) => {
                assert_eq!(error.code(), "WYRD_DATA_400_INVALID_INTERFACE_OPTION");
            }
            other => panic!("expected Wyrd spec error, got {other:?}"),
        }
    }
}
