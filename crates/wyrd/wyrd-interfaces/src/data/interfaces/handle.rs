use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::data::dtype::{self, DataSourceKind};
use crate::data::interfaces::base::DataInterface;
use crate::data::interfaces::helpers::{huggingface_dataset_id, huggingface_optional_attr};
use crate::data::interfaces::kinds::{
    ArrowInterface, HuggingfaceInterface, ImageInterface, JsonlInterface, NumpyInterface,
    PandasInterface, ParquetInterface, PolarsInterface, SqlInterface, TextInterface,
    TorchInterface,
};
use wyrd_spec::card::data::{CustomDataMeta, DataInterface as RustDataInterface, DataSchema};
#[cfg(feature = "python")]
use wyrd_utils::py::WyrdPyResult;

/// Python-compatible dispatch wrapper for local data interface holders.
#[cfg(feature = "python")]
pub enum DataInterfaceHandle {
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
    /// Python subclass of the base `DataInterface`.
    Subclass(Py<PyAny>),
}

#[cfg(feature = "python")]
impl DataInterfaceHandle {
    /// Build a handle from an explicit Python `DataInterface` object.
    pub fn from_interface(interface: &Bound<'_, PyAny>) -> WyrdPyResult<Self> {
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

        if interface.is_instance_of::<DataInterface>() {
            return Ok(Self::Subclass(interface.clone().unbind()));
        }

        Err(crate::error::validation("DataCard requires a supported data interface").into())
    }

    /// Detect and build a default interface holder from raw Python data.
    pub fn from_raw(py: Python<'_>, data: &Bound<'_, PyAny>) -> WyrdPyResult<Self> {
        match dtype::detect_data_source(py, data)? {
            DataSourceKind::Pandas => Ok(Self::Pandas(PandasInterface {
                data: Some(Arc::new(data.clone().unbind())),
                compression: "snappy".to_string(),
            })),
            DataSourceKind::Polars => Ok(Self::Polars(PolarsInterface {
                data: Some(Arc::new(data.clone().unbind())),
                compression: "snappy".to_string(),
            })),
            DataSourceKind::Arrow => Ok(Self::Arrow(ArrowInterface {
                data: Some(Arc::new(data.clone().unbind())),
                format: "parquet".to_string(),
            })),
            DataSourceKind::ParquetPath => Ok(Self::Parquet(ParquetInterface {
                data: Some(Arc::new(data.clone().unbind())),
                compression: "snappy".to_string(),
                row_group_size: None,
            })),
            DataSourceKind::Numpy => Ok(Self::Numpy(NumpyInterface {
                data: Some(Arc::new(data.clone().unbind())),
                dtype: None,
                shape: None,
                format: "npy".to_string(),
            })),
            DataSourceKind::Torch => Ok(Self::Torch(TorchInterface {
                data: Some(Arc::new(data.clone().unbind())),
                save_format: "safetensors".to_string(),
            })),
            DataSourceKind::Sql => Ok(Self::Sql(SqlInterface {
                data: Some(Arc::new(data.clone().unbind())),
                dialect: "sql".to_string(),
                connection_hint: None,
            })),
            DataSourceKind::JsonlPath => Ok(Self::Jsonl(JsonlInterface {
                data: Some(Arc::new(data.clone().unbind())),
                compression: dtype::jsonl_compression_from_path(data)?,
                lines_per_file: None,
            })),
            DataSourceKind::ImageDirectory => Ok(Self::Image(ImageInterface {
                data: Some(Arc::new(data.clone().unbind())),
                format: "mixed".to_string(),
                color_mode: "rgb".to_string(),
                manifest_ref: None,
            })),
            DataSourceKind::TextDirectory => Ok(Self::Text(TextInterface {
                data: Some(Arc::new(data.clone().unbind())),
                encoding: "utf-8".to_string(),
                manifest_ref: None,
            })),
            DataSourceKind::Huggingface => Ok(Self::Huggingface(HuggingfaceInterface {
                data: Some(Arc::new(data.clone().unbind())),
                dataset_id: huggingface_dataset_id(data)?,
                revision: huggingface_optional_attr(data, &["revision"]),
                split: huggingface_optional_attr(data, &["split"]),
                config: huggingface_optional_attr(data, &["config_name", "config"]),
            })),
        }
    }

    /// Convert this holder into Rust-only interface metadata.
    pub fn to_spec_interface(&self, py: Python<'_>) -> WyrdPyResult<RustDataInterface> {
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
            Self::Subclass(_) => Ok(RustDataInterface::Custom(CustomDataMeta {
                loader_module: String::new(),
                loader_class: String::new(),
                extra: BTreeMap::new(),
            })),
        }
    }

    /// Convert this handle back into a Python interface object.
    pub fn into_py_any(self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
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
            Self::Subclass(value) => Ok(value),
        }
    }

    /// Take the held Python source object, if one exists.
    pub fn take_source(&mut self) -> Option<Arc<Py<PyAny>>> {
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
            Self::Subclass(_) => None,
        }
    }

    /// Borrow the held Python source object, if one exists.
    pub fn source_ref(&self) -> Option<&Py<PyAny>> {
        match self {
            Self::Pandas(value) => value.data.as_deref(),
            Self::Polars(value) => value.data.as_deref(),
            Self::Arrow(value) => value.data.as_deref(),
            Self::Parquet(value) => value.data.as_deref(),
            Self::Numpy(value) => value.data.as_deref(),
            Self::Torch(value) => value.data.as_deref(),
            Self::Sql(value) => value.data.as_deref(),
            Self::Jsonl(value) => value.data.as_deref(),
            Self::Image(value) => value.data.as_deref(),
            Self::Text(value) => value.data.as_deref(),
            Self::Huggingface(value) => value.data.as_deref(),
            Self::Subclass(_) => None,
        }
    }

    /// Infer a `DataSchema` from this holder's source object.
    pub fn infer_schema(
        &self,
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<DataSchema> {
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
            Self::Subclass(_) => "Custom",
        }
    }
}
