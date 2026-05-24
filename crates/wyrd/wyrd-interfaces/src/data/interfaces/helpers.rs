use std::fs;
use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};
use serde::Serialize;
use wyrd_utils::py::{json_to_pyobject, pyobject_to_json};

use crate::data::dtype;
use crate::data::interfaces::kinds::{
    ArrowInterface, CustomDataInterface, HuggingfaceInterface, ImageInterface, JsonlInterface,
    NumpyInterface, PandasInterface, ParquetInterface, PolarsInterface, SqlInterface,
    TextInterface, TorchInterface,
};
use crate::data::interfaces::options::parquet_compression_token;
use crate::data::io::{ImageManifest, ManifestEntry, TextManifest};
use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::{
    DataInterface as RustDataInterface, DataSchema, DataStats, ParquetCompression,
};
use wyrd_spec::reference::CardRef;

#[cfg(feature = "python")]
pub(super) fn interface_to_dict(
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
pub(super) fn require_local_file(path: &Path) -> CardPyResult<()> {
    wyrd_utils::fs::require_local_file(path).map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
pub(super) fn require_local_path(path: &Path) -> CardPyResult<()> {
    wyrd_utils::fs::require_local_path(path).map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
pub(super) fn data_stats_for_file(
    path: &Path,
    schema: Option<&DataSchema>,
) -> CardPyResult<DataStats> {
    wyrd_utils::fs::data_stats_for_file(path, schema)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
pub(super) fn data_stats_for_path(
    path: &Path,
    schema: Option<&DataSchema>,
) -> CardPyResult<DataStats> {
    wyrd_utils::fs::data_stats_for_path(path, schema)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
pub(super) fn write_json_sorted<T: Serialize>(
    path: impl AsRef<Path>,
    value: &T,
) -> CardPyResult<()> {
    wyrd_utils::json::write_json_sorted(path, value)
        .map_err(|error| WyrdPyError::Io(error.to_string()))
}

#[cfg(feature = "python")]
pub(super) fn pyarrow_engine_kwargs<'py>(py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("engine", "pyarrow")?;
    Ok(kwargs)
}

#[cfg(feature = "python")]
pub(super) fn pandas_to_parquet_kwargs<'py>(
    py: Python<'py>,
    compression: &'static str,
) -> CardPyResult<Bound<'py, PyDict>> {
    let kwargs = pyarrow_engine_kwargs(py)?;
    kwargs.set_item("compression", compression)?;
    Ok(kwargs)
}

#[cfg(feature = "python")]
pub(super) fn parquet_compression_kwargs<'py>(
    py: Python<'py>,
    compression: ParquetCompression,
) -> CardPyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("compression", parquet_compression_token(compression))?;
    Ok(kwargs)
}

#[cfg(feature = "python")]
pub(super) fn numpy_no_pickle_kwargs<'py>(py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("allow_pickle", false)?;
    Ok(kwargs)
}

#[cfg(feature = "python")]
pub(super) fn numpy_npz_value_kwargs<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> CardPyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("value", data)?;
    Ok(kwargs)
}

#[cfg(feature = "python")]
pub(super) fn torch_weights_only_kwargs<'py>(py: Python<'py>) -> CardPyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("weights_only", true)?;
    Ok(kwargs)
}

#[cfg(feature = "python")]
pub(super) fn optional_schema_for_interface(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    kind: &str,
) -> DataSchema {
    dtype::infer_schema_for_interface(py, data, kind).unwrap_or_else(|_| DataSchema::empty())
}

#[cfg(feature = "python")]
pub(super) fn ensure_parent_dir(path: &Path) -> CardPyResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(feature = "python")]
pub(super) fn bool_kwarg(kwargs: Option<&Bound<'_, PyDict>>, name: &str) -> CardPyResult<bool> {
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
pub(super) trait FileManifest {
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
pub(super) fn copy_manifest_entries(manifest: &impl FileManifest, dest: &Path) -> CardPyResult<()> {
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
pub(super) fn torch_to_safetensor_map<'py>(
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
pub(super) fn parse_card_ref(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<Option<CardRef>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(None);
    };
    let raw = pyobject_to_json(value)?;
    serde_json::from_value(raw).map(Some).map_err(Into::into)
}

#[cfg(feature = "python")]
#[allow(dead_code)]
pub(super) fn huggingface_dataset_id(data: &Bound<'_, PyAny>) -> CardPyResult<String> {
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
pub(super) fn huggingface_optional_attr(data: &Bound<'_, PyAny>, names: &[&str]) -> Option<String> {
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
#[cfg(feature = "python")]
macro_rules! impl_clone_for_handle {
    ($type:ty { $($field:ident),+ $(,)? }) => {
        impl $type {
            #[allow(dead_code)]
            pub(super) fn clone_for_handle(&self, py: Python<'_>) -> Self {
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
