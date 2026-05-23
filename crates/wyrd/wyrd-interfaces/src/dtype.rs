//! Data source detection and Python type guards.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(feature = "python")]
use std::path::PathBuf;

#[cfg(feature = "python")]
use pyo3::exceptions::PyModuleNotFoundError;
#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyAny, PyDict, PyString, PyTuple};
use wyrd_spec::card::data::DataSchema;
use wyrd_spec::card::field::{Dim, FieldSpec};
use wyrd_spec::ids::ColumnName;

use crate::error::{CardPyResult, WyrdPyError};

/// Detected Python data source family used to build a `DataInterface`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSourceKind {
    /// A `pandas.DataFrame`.
    Pandas,
    /// A `polars.DataFrame`.
    Polars,
    /// A `pyarrow.Table`.
    Arrow,
    /// A parquet path.
    ParquetPath,
    /// A `numpy.ndarray`.
    Numpy,
    /// A `torch.Tensor`.
    Torch,
    /// A SQL query dictionary.
    Sql,
    /// A JSON Lines path.
    JsonlPath,
    /// An image directory.
    ImageDirectory,
    /// A text directory.
    TextDirectory,
    /// A `datasets.Dataset`.
    Huggingface,
}

/// Return a JSONL compression label from a path suffix.
#[must_use]
pub fn jsonl_compression_for_path(path: &Path) -> Option<&'static str> {
    if path_has_extension(path, "gz") && path_stem_has_extension(path, "jsonl") {
        Some("gzip")
    } else if path_has_extension(path, "zst") && path_stem_has_extension(path, "jsonl") {
        Some("zstd")
    } else if path_has_extension(path, "jsonl") {
        Some("none")
    } else {
        None
    }
}

/// Return true when a path suffix identifies parquet data.
#[must_use]
pub fn is_parquet_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("parquet"))
}

/// Normalize a source-library dtype token to the canonical Wyrd Arrow string.
///
/// # Errors
/// Returns `WYRD_DATA_400_UNKNOWN_DATA_TYPE` when the token is not in the
/// locked dtype table.
pub fn normalize_dtype(source: &str, value: &str) -> CardPyResult<String> {
    let source = source.trim().to_ascii_lowercase();
    let value = normalize_token(value);
    match source.as_str() {
        "pandas" => normalize_pandas_dtype(&value),
        "polars" => normalize_polars_dtype(&value),
        "pyarrow" | "arrow" | "parquet" => normalize_pyarrow_dtype(&value),
        "numpy" => normalize_numpy_dtype(&value),
        "torch" => normalize_torch_dtype(&value),
        _ => Err(unknown_dtype(&source, &value)),
    }
}

/// Infer a Wyrd `DataSchema` for a Python data object held by an interface.
///
/// # Errors
/// Returns a Wyrd Python-boundary error when the object cannot be inspected or
/// its dtype values are not in the locked normalization table.
#[cfg(feature = "python")]
pub fn infer_schema_for_interface(
    _py: Python<'_>,
    data: &Bound<'_, PyAny>,
    kind: &str,
) -> CardPyResult<DataSchema> {
    match kind {
        "Pandas" => infer_pandas_schema(data),
        "Polars" => infer_polars_schema(data),
        "Arrow" | "Parquet" => infer_arrow_schema(data),
        "Numpy" => infer_numpy_schema(data),
        "Torch" => infer_torch_schema(data),
        "Sql" | "Image" | "Text" | "Huggingface" => Ok(DataSchema::empty()),
        _ => Err(WyrdPyError::unknown_data_type(format!(
            "unknown DataCard interface kind: {kind}"
        ))),
    }
}

/// Infer the canonical dtype for a NumPy object when present.
#[cfg(feature = "python")]
pub fn numpy_dtype(py: Python<'_>, data: Option<&Py<PyAny>>) -> Option<CardPyResult<String>> {
    data.map(|value| {
        let bound = value.bind(py);
        let dtype = bound.getattr("dtype")?.str()?.extract::<String>()?;
        normalize_dtype("numpy", &dtype)
    })
}

/// Infer the shape for a NumPy object when present.
#[cfg(feature = "python")]
pub fn numpy_shape(py: Python<'_>, data: Option<&Py<PyAny>>) -> Option<CardPyResult<Vec<i64>>> {
    data.map(|value| shape_values(value.bind(py)))
}

/// Detect the source family for a raw Python data object.
///
/// Detection order is locked: pandas, polars, pyarrow, numpy, torch,
/// `HuggingFace` datasets, SQL dict, then path-like dispatch.
#[cfg(feature = "python")]
pub fn detect_data_source(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<DataSourceKind> {
    if is_pandas_dataframe(py, data)? {
        return Ok(DataSourceKind::Pandas);
    }
    if is_polars_dataframe(py, data)? {
        return Ok(DataSourceKind::Polars);
    }
    if is_pyarrow_table(py, data)? {
        return Ok(DataSourceKind::Arrow);
    }
    if is_numpy_array(py, data)? {
        return Ok(DataSourceKind::Numpy);
    }
    if is_torch_tensor(py, data)? {
        return Ok(DataSourceKind::Torch);
    }
    if is_huggingface_dataset(py, data)? {
        return Ok(DataSourceKind::Huggingface);
    }
    if data.is_instance_of::<PyDict>() {
        return Ok(DataSourceKind::Sql);
    }
    if is_path_like(py, data)? {
        let path = extract_pathbuf(data)?;
        if is_parquet_path(&path) {
            return Ok(DataSourceKind::ParquetPath);
        }
        if jsonl_compression_for_path(&path).is_some() {
            return Ok(DataSourceKind::JsonlPath);
        }
        if path.is_dir() {
            return if directory_contains_image_file(&path) {
                Ok(DataSourceKind::ImageDirectory)
            } else {
                Ok(DataSourceKind::TextDirectory)
            };
        }
    }
    Err(WyrdPyError::unknown_data_type(
        "DataCard requires a supported data object, SQL dictionary, or data path",
    ))
}

/// Return true when a Python object can be coerced through `os.fspath`.
#[cfg(feature = "python")]
pub fn is_path_like(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    if data.is_instance_of::<PyString>() {
        return Ok(true);
    }
    if !data.hasattr("__fspath__")? {
        return Ok(false);
    }
    let os = py.import("os")?;
    Ok(os.call_method1("fspath", (data,)).is_ok())
}

/// Coerce a Python string or path-like object into a local path.
#[cfg(feature = "python")]
pub fn extract_pathbuf(data: &Bound<'_, PyAny>) -> CardPyResult<PathBuf> {
    if data.is_instance_of::<PyString>() {
        return Ok(PathBuf::from(data.extract::<String>()?));
    }
    let os = data.py().import("os")?;
    let path = os.call_method1("fspath", (data,))?;
    Ok(PathBuf::from(path.extract::<String>()?))
}

/// Infer a JSONL compression label from a Python path-like object.
#[cfg(feature = "python")]
pub fn jsonl_compression_from_path(data: &Bound<'_, PyAny>) -> CardPyResult<String> {
    let path = extract_pathbuf(data)?;
    jsonl_compression_for_path(&path)
        .map(str::to_string)
        .ok_or_else(|| {
            WyrdPyError::validation_with_details(
                "JSONL data paths must end with .jsonl, .jsonl.gz, or .jsonl.zst",
                serde_json::json!({ "path": path.to_string_lossy() }),
            )
        })
}

/// Require a `pandas.DataFrame`.
#[cfg(feature = "python")]
pub fn ensure_pandas_dataframe(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<()> {
    ensure_kind(py, data, DataSourceKind::Pandas, "pandas.DataFrame")
}

/// Require a `polars.DataFrame`.
#[cfg(feature = "python")]
pub fn ensure_polars_dataframe(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<()> {
    ensure_kind(py, data, DataSourceKind::Polars, "polars.DataFrame")
}

/// Require a `pyarrow.Table`.
#[cfg(feature = "python")]
pub fn ensure_pyarrow_table(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<()> {
    ensure_kind(py, data, DataSourceKind::Arrow, "pyarrow.Table")
}

/// Require a `numpy.ndarray`.
#[cfg(feature = "python")]
pub fn ensure_numpy_array(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<()> {
    ensure_kind(py, data, DataSourceKind::Numpy, "numpy.ndarray")
}

/// Require a `torch.Tensor` or mapping of tensor values.
#[cfg(feature = "python")]
pub fn ensure_torch_tensor_or_mapping(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<()> {
    if is_torch_tensor(py, data)? {
        return Ok(());
    }
    if data.is_instance_of::<PyDict>() || (data.hasattr("keys")? && data.hasattr("items")?) {
        return Ok(());
    }
    Err(WyrdPyError::unknown_data_type(
        "expected a torch.Tensor or mapping of tensor values",
    ))
}

#[cfg(feature = "python")]
fn ensure_kind(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    kind: DataSourceKind,
    expected: &str,
) -> CardPyResult<()> {
    if detect_data_source(py, data)? == kind {
        Ok(())
    } else {
        Err(WyrdPyError::unknown_data_type(format!(
            "expected {expected}"
        )))
    }
}

#[cfg(feature = "python")]
fn is_pandas_dataframe(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    is_instance_of_optional(py, data, "pandas", "DataFrame")
}

#[cfg(feature = "python")]
fn is_polars_dataframe(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    is_instance_of_optional(py, data, "polars", "DataFrame")
}

#[cfg(feature = "python")]
fn is_pyarrow_table(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    is_instance_of_optional(py, data, "pyarrow", "Table")
}

#[cfg(feature = "python")]
fn is_numpy_array(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    is_instance_of_optional(py, data, "numpy", "ndarray")
}

#[cfg(feature = "python")]
fn is_torch_tensor(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    is_instance_of_optional(py, data, "torch", "Tensor")
}

#[cfg(feature = "python")]
fn is_huggingface_dataset(py: Python<'_>, data: &Bound<'_, PyAny>) -> CardPyResult<bool> {
    is_instance_of_optional(py, data, "datasets", "Dataset")
}

#[cfg(feature = "python")]
fn is_instance_of_optional(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    module_name: &str,
    class_name: &str,
) -> CardPyResult<bool> {
    let module = match py.import(module_name) {
        Ok(module) => module,
        Err(error) if error.is_instance_of::<PyModuleNotFoundError>(py) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let class = module.getattr(class_name)?;
    Ok(data.is_instance(&class)?)
}

#[cfg(feature = "python")]
fn directory_contains_image_file(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            directory_contains_image_file(&path)
        } else {
            is_image_path(&path)
        }
    })
}

#[cfg(feature = "python")]
fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff"
            )
        })
}

fn path_has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

fn path_stem_has_extension(path: &Path, expected: &str) -> bool {
    path.file_stem()
        .map(Path::new)
        .is_some_and(|stem| path_has_extension(stem, expected))
}

#[cfg(test)]
mod tests {
    use super::{is_parquet_path, jsonl_compression_for_path};
    use std::path::Path;

    #[test]
    fn jsonl_compression_maps_locked_suffixes() {
        assert_eq!(
            jsonl_compression_for_path(Path::new("data.jsonl")),
            Some("none")
        );
        assert_eq!(
            jsonl_compression_for_path(Path::new("data.jsonl.gz")),
            Some("gzip")
        );
        assert_eq!(
            jsonl_compression_for_path(Path::new("data.jsonl.zst")),
            Some("zstd")
        );
        assert_eq!(jsonl_compression_for_path(Path::new("data.json")), None);
    }

    #[test]
    fn parquet_path_is_case_insensitive() {
        assert!(is_parquet_path(Path::new("data.PARQUET")));
        assert!(!is_parquet_path(Path::new("data.jsonl")));
    }
}
