use crate::data::interfaces::DataInterface;
use crate::error::CardPyResult;
use wyrd_spec::reference::CardRef;

#[cfg(feature = "python")]
use {
    crate::data::interfaces::helpers::{interface_to_dict, parse_card_ref},
    crate::data::interfaces::options::{
        parse_arrow_format, parse_color_mode, parse_image_format, parse_jsonl_compression,
        parse_numpy_format, parse_parquet_compression, parse_sql_dialect, parse_torch_save_format,
    },
    crate::data::stats::PyDataStats,
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict},
    std::path::PathBuf,
    std::sync::Arc,
};

#[cfg(feature = "python")]
fn shared_py(value: Option<Py<PyAny>>) -> Option<Arc<Py<PyAny>>> {
    value.map(Arc::new)
}

macro_rules! interface_struct {
    ($(#[$meta:meta])+ $name:ident { $($field:ident : $field_ty:ty),+ $(,)? }) => {
        $(#[$meta])+
        #[cfg_attr(feature = "python", pyclass(module = "wyrd.data", extends = DataInterface))]
        pub struct $name {
            $(pub(super) $field: $field_ty),+
        }
    };
}

interface_struct!(
/// Data interface for pandas `DataFrame` values.
///
/// `save` validates a pandas `DataFrame`, writes `data/data.parquet` with the
/// configured parquet compression, and returns deterministic `DataStats`.
/// `load` reads the local parquet artifact back into a pandas `DataFrame`.
PandasInterface {
    data: Option<Arc<Py<PyAny>>>,
    compression: String,
});

interface_struct!(
/// Data interface for polars `DataFrame` values.
///
/// `save` validates a polars `DataFrame`, writes `data/data.parquet` with the
/// configured parquet compression, and returns deterministic `DataStats`.
/// `load` reads the local parquet artifact back into a polars `DataFrame`.
PolarsInterface {
    data: Option<Arc<Py<PyAny>>>,
    compression: String,
});

interface_struct!(
/// Data interface for `PyArrow` table values.
///
/// `save` validates a `pyarrow.Table` and writes either `data/data.parquet` or
/// `data/data.arrow` based on `format`. `load` restores the table from the
/// matching local artifact.
ArrowInterface {
    data: Option<Arc<Py<PyAny>>>,
    format: String,
});

interface_struct!(
/// Data interface for an existing parquet file or table-like parquet source.
///
/// `save` copies a path-like parquet file or writes a table-like source to
/// `data/data.parquet`. `load` restores the local artifact as a `PyArrow` table.
ParquetInterface {
    data: Option<Arc<Py<PyAny>>>,
    compression: String,
    row_group_size: Option<u32>,
});

interface_struct!(
/// Data interface for `NumPy` ndarray values.
///
/// `save` validates a `NumPy` array and writes `data/data.npy` or
/// `data/data.npz` based on `format`. `load` restores the saved array with
/// pickle loading disabled.
NumpyInterface {
    data: Option<Arc<Py<PyAny>>>,
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
    data: Option<Arc<Py<PyAny>>>,
    save_format: String,
});

interface_struct!(
/// Data interface for SQL query bundles.
///
/// `save` serializes SQL logic to `data/sql.json` with deterministic key
/// ordering. `load` reads that JSON artifact back into a Python object.
SqlInterface {
    data: Option<Arc<Py<PyAny>>>,
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
    data: Option<Arc<Py<PyAny>>>,
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
    data: Option<Arc<Py<PyAny>>>,
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
    data: Option<Arc<Py<PyAny>>>,
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
    data: Option<Arc<Py<PyAny>>>,
    dataset_id: String,
    revision: Option<String>,
    split: Option<String>,
    config: Option<String>,
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
            /// saved into the local `DataCard` artifact layout. Sourceless
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
            /// that will be stored in the `DataCard` spec.
            ///
            /// # Errors
            ///
            /// Returns a Wyrd data validation error when interface options such
            /// as compression, serialization format, or color mode are invalid.
            fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
                interface_to_dict(py, self.to_spec_interface(py)?)
            }

            /// Save this interface's source data into the local `DataCard` layout.
            ///
            /// The method validates the held Python object, materializes bytes
            /// under `path`, and returns deterministic byte statistics for the
            /// artifact that was written. The concrete interface controls the
            /// exact convention path, for example `data/data.parquet`,
            /// `data/data.npy`, or `data/manifest.json`.
            ///
            /// # Arguments
            ///
            /// * `path` - Directory containing the local `DataCard`
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

            /// Load this interface's source data from the local `DataCard` layout.
            ///
            /// The method reads the convention path for the concrete interface
            /// from `path` and attaches the loaded Python object back to the
            /// interface instance. For pinned remote Hugging Face pointers,
            /// callers must explicitly pass `allow_remote=true` in
            /// `load_kwargs`.
            ///
            /// # Arguments
            ///
            /// * `path` - Directory containing the local `DataCard`
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
    /// * `data` - Optional pandas `DataFrame` to materialize when saving.
    /// * `compression` - Parquet compression codec. Accepted values are
    ///   `none`, `snappy`, `gzip`, `zstd`, and `lz4`.
    ///
    /// # Returns
    ///
    /// A pandas interface with kind `Pandas`.
    #[new]
    #[pyo3(signature = (*, data=None, compression="snappy"))]
    fn __new__(data: Option<Py<PyAny>>, compression: &str) -> CardPyResult<(Self, DataInterface)> {
        parse_parquet_compression(compression)?;
        Ok((
            Self {
                data: shared_py(data),
                compression: compression.to_string(),
            },
            DataInterface::marker("Pandas"),
        ))
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(PolarsInterface {
    /// Create a polars data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional polars `DataFrame` to materialize when saving.
    /// * `compression` - Parquet compression codec. Accepted values are
    ///   `none`, `snappy`, `gzip`, `zstd`, and `lz4`.
    ///
    /// # Returns
    ///
    /// A polars interface with kind `Polars`.
    #[new]
    #[pyo3(signature = (*, data=None, compression="snappy"))]
    fn __new__(data: Option<Py<PyAny>>, compression: &str) -> CardPyResult<(Self, DataInterface)> {
        parse_parquet_compression(compression)?;
        Ok((
            Self {
                data: shared_py(data),
                compression: compression.to_string(),
            },
            DataInterface::marker("Polars"),
        ))
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(ArrowInterface {
    /// Create a `PyArrow` table interface.
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
    fn __new__(data: Option<Py<PyAny>>, format: &str) -> CardPyResult<(Self, DataInterface)> {
        parse_arrow_format(format)?;
        Ok((
            Self {
                data: shared_py(data),
                format: format.to_string(),
            },
            DataInterface::marker("Arrow"),
        ))
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
    ) -> CardPyResult<(Self, DataInterface)> {
        parse_parquet_compression(compression)?;
        Ok((
            Self {
                data: shared_py(data),
                compression: compression.to_string(),
                row_group_size,
            },
            DataInterface::marker("Parquet"),
        ))
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(NumpyInterface {
    /// Create a `NumPy` data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional `NumPy` ndarray to materialize when saving.
    /// * `dtype` - Optional declared dtype. When omitted, Wyrd attempts to
    ///   infer it from `data`.
    /// * `shape` - Optional declared array shape. When omitted, Wyrd attempts
    ///   to infer it from `data`.
    /// * `format` - Serialization format. Accepted values are `npy` and `npz`.
    ///
    /// # Returns
    ///
    /// A `NumPy` interface with kind `Numpy`.
    #[new]
    #[pyo3(signature = (*, data=None, dtype=None, shape=None, format="npy"))]
    fn __new__(
        data: Option<Py<PyAny>>,
        dtype: Option<String>,
        shape: Option<Vec<i64>>,
        format: &str,
    ) -> CardPyResult<(Self, DataInterface)> {
        parse_numpy_format(format)?;
        Ok((
            Self {
                data: shared_py(data),
                dtype,
                shape,
                format: format.to_string(),
            },
            DataInterface::marker("Numpy"),
        ))
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
    fn __new__(data: Option<Py<PyAny>>, save_format: &str) -> CardPyResult<(Self, DataInterface)> {
        parse_torch_save_format(save_format)?;
        Ok((
            Self {
                data: shared_py(data),
                save_format: save_format.to_string(),
            },
            DataInterface::marker("Torch"),
        ))
    }
});

#[cfg(feature = "python")]
impl_interface_methods!(SqlInterface {
    /// Create a SQL data interface.
    ///
    /// # Arguments
    ///
    /// * `data` - Optional SQL query bundle or JSON-compatible SQL logic.
    /// * `dialect` - SQL dialect label recorded in the `DataCard` spec.
    /// * `connection_hint` - Optional human-readable connection hint.
    ///
    /// # Returns
    ///
    /// A SQL interface with kind `Sql`.
    #[new]
    #[pyo3(signature = (*, data=None, dialect, connection_hint=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        dialect: &str,
        connection_hint: Option<String>,
    ) -> CardPyResult<(Self, DataInterface)> {
        let dialect = parse_sql_dialect(dialect)?;
        Ok((
            Self {
                data: shared_py(data),
                dialect,
                connection_hint,
            },
            DataInterface::marker("Sql"),
        ))
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
    ) -> CardPyResult<(Self, DataInterface)> {
        parse_jsonl_compression(compression)?;
        Ok((
            Self {
                data: shared_py(data),
                compression: compression.to_string(),
                lines_per_file,
            },
            DataInterface::marker("Jsonl"),
        ))
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
    /// * `manifest_ref` - Optional `CardRef` pointing at an external manifest
    ///   card.
    ///
    /// # Returns
    ///
    /// An image interface with kind `Image`.
    ///
    /// # Errors
    ///
    /// Returns a Wyrd error when `manifest_ref` cannot be parsed as a `CardRef`.
    #[new]
    #[pyo3(signature = (*, data=None, format="mixed", color_mode="rgb", manifest_ref=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        format: &str,
        color_mode: &str,
        manifest_ref: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<(Self, DataInterface)> {
        parse_image_format(format)?;
        parse_color_mode(color_mode)?;
        Ok((
            Self {
                data: shared_py(data),
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
    /// * `encoding` - Text encoding label recorded in the `DataCard` spec.
    /// * `manifest_ref` - Optional `CardRef` pointing at an external manifest
    ///   card.
    ///
    /// # Returns
    ///
    /// A text interface with kind `Text`.
    ///
    /// # Errors
    ///
    /// Returns a Wyrd error when `manifest_ref` cannot be parsed as a `CardRef`.
    #[new]
    #[pyo3(signature = (*, data=None, encoding="utf-8", manifest_ref=None))]
    fn __new__(
        data: Option<Py<PyAny>>,
        encoding: &str,
        manifest_ref: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<(Self, DataInterface)> {
        Ok((
            Self {
                data: shared_py(data),
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
    ) -> CardPyResult<(Self, DataInterface)> {
        if let Some(revision) = revision.as_deref()
            && (!(7..=40).contains(&revision.len())
                || !revision
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()))
        {
            return Err(crate::error::WyrdPyError::validation_with_details(
                "Huggingface revision must be a lowercase hex string between 7 and 40 characters",
                serde_json::json!({ "revision": revision }),
            ));
        }
        Ok((
            Self {
                data: shared_py(data),
                dataset_id,
                revision,
                split,
                config,
            },
            DataInterface::marker("Huggingface"),
        ))
    }
});
