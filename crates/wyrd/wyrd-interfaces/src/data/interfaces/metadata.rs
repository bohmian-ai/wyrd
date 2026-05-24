use crate::data::interfaces::kinds::{
    ArrowInterface, CustomDataInterface, HuggingfaceInterface, ImageInterface, JsonlInterface,
    NumpyInterface, PandasInterface, ParquetInterface, PolarsInterface, SqlInterface,
    TextInterface, TorchInterface,
};
use crate::data::interfaces::options::{
    arrow_format_token, color_mode_token, image_format_token, jsonl_compression_token,
    numpy_format_token, parquet_compression_token, parse_arrow_format, parse_color_mode,
    parse_image_format, parse_jsonl_compression, parse_numpy_format, parse_parquet_compression,
    parse_torch_save_format, torch_save_format_token,
};
use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::{
    ArrowMeta, CustomDataMeta, DataInterface as RustDataInterface, HuggingfaceMeta, ImageMeta,
    JsonlMeta, NumpyMeta, PandasMeta, ParquetMeta, PolarsMeta, SqlMeta, TextMeta, TorchMeta,
};

#[cfg(feature = "python")]
use {crate::data::dtype, pyo3::prelude::*, wyrd_utils::py::module_version};

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
