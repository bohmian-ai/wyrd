//! Python-facing data interface holders and Rust metadata conversion.

use std::collections::BTreeMap;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyAny, PyDict, PyModule};
#[cfg(feature = "python")]
use wyrd_utils::py::{json_to_pyobject, module_version, pyobject_to_json};

#[cfg(feature = "python")]
use crate::dtype::{self, DataSourceKind};
use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::{
    ArrowFormat, ArrowMeta, ColorMode, CustomDataMeta, DataInterface as RustDataInterface,
    HuggingfaceMeta, ImageFormat, ImageMeta, JsonlCompression, JsonlMeta, NumpyFormat, NumpyMeta,
    PandasMeta, ParquetCompression, ParquetMeta, PolarsMeta, SqlMeta, TextMeta, TorchMeta,
    TorchSaveFormat,
};
use wyrd_spec::reference::CardRef;

/// Python-visible abstract base class for local data interface holders.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", subclass))]
pub struct DataInterface {
    kind: String,
}

#[cfg(feature = "python")]
#[pymethods]
impl DataInterface {
    #[new]
    fn __new__() -> CardPyResult<Self> {
        Err(WyrdPyError::validation(
            "DataInterface is an abstract base class; use PandasInterface, PolarsInterface, ArrowInterface, ParquetInterface, NumpyInterface, TorchInterface, SqlInterface, JsonlInterface, ImageInterface, TextInterface, HuggingfaceInterface, or CustomDataInterface",
        ))
    }

    #[getter]
    fn kind(&self) -> &str {
        &self.kind
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
    ($name:ident { $($field:ident : $field_ty:ty),+ $(,)? }) => {
        #[doc = concat!("Python-facing local data holder for `", stringify!($name), "`.")]
        #[cfg_attr(feature = "python", pyclass(module = "wyrd.data", extends = DataInterface))]
        pub struct $name {
            $($field: $field_ty),+
        }
    };
}

interface_struct!(PandasInterface {
    data: Option<Py<PyAny>>,
    compression: String,
});

interface_struct!(PolarsInterface {
    data: Option<Py<PyAny>>,
    compression: String,
});

interface_struct!(ArrowInterface {
    data: Option<Py<PyAny>>,
    format: String,
});

interface_struct!(ParquetInterface {
    data: Option<Py<PyAny>>,
    compression: String,
    row_group_size: Option<u32>,
});

interface_struct!(NumpyInterface {
    data: Option<Py<PyAny>>,
    dtype: Option<String>,
    shape: Option<Vec<i64>>,
    format: String,
});

interface_struct!(TorchInterface {
    data: Option<Py<PyAny>>,
    save_format: String,
});

interface_struct!(SqlInterface {
    data: Option<Py<PyAny>>,
    dialect: String,
    connection_hint: Option<String>,
});

interface_struct!(JsonlInterface {
    data: Option<Py<PyAny>>,
    compression: String,
    lines_per_file: Option<u64>,
});

interface_struct!(ImageInterface {
    data: Option<Py<PyAny>>,
    format: String,
    color_mode: String,
    manifest_ref: Option<CardRef>,
});

interface_struct!(TextInterface {
    data: Option<Py<PyAny>>,
    encoding: String,
    manifest_ref: Option<CardRef>,
});

interface_struct!(HuggingfaceInterface {
    data: Option<Py<PyAny>>,
    dataset_id: String,
    revision: Option<String>,
    split: Option<String>,
    config: Option<String>,
});

interface_struct!(CustomDataInterface {
    data: Option<Py<PyAny>>,
    loader_module: String,
    loader_class: String,
    extra: BTreeMap<String, String>,
});

#[cfg(feature = "python")]
#[pymethods]
impl PandasInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PolarsInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl ArrowInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl ParquetInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl NumpyInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl TorchInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl SqlInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl JsonlInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl ImageInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl TextInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl HuggingfaceInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl CustomDataInterface {
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

    #[getter]
    fn has_source(&self) -> bool {
        self.data.is_some()
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
        interface_to_dict(py, self.to_spec_interface(py)?)
    }
}

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
}

#[cfg(feature = "python")]
#[allow(dead_code)]
impl DataInterfaceHandle {
    /// Build a handle from an explicit Python `DataInterface` object.
    pub fn from_interface(interface: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        if !interface.is_instance_of::<DataInterface>() {
            return Err(WyrdPyError::validation(
                "DataCard requires a supported data interface",
            ));
        }

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
        }
    }

    /// Infer a `DataSchema` from this holder's source object.
    pub fn infer_schema(
        &self,
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
    ) -> CardPyResult<wyrd_spec::card::data::DataSchema> {
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
