//! Data source detection and Python type guards.

use std::borrow::Cow;
use std::path::Path;

#[cfg(feature = "python")]
use {
    pyo3::exceptions::PyModuleNotFoundError,
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict, PyString, PyTuple},
    std::collections::BTreeMap,
    std::path::PathBuf,
    wyrd_spec::card::data::DataSchema,
    wyrd_spec::card::field::{Dim, FieldSpec},
    wyrd_spec::ids::ColumnName,
};

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

/// Infer the canonical dtype for a `NumPy` object when present.
#[cfg(feature = "python")]
pub fn numpy_dtype(py: Python<'_>, data: Option<&Py<PyAny>>) -> Option<CardPyResult<String>> {
    data.map(|value| {
        let bound = value.bind(py);
        let dtype = numpy_dtype_string(bound)?;
        normalize_dtype("numpy", &dtype)
    })
}

/// Infer the shape for a `NumPy` object when present.
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

fn normalize_pandas_dtype(value: &str) -> CardPyResult<String> {
    let value = value.to_ascii_lowercase();
    match value.as_str() {
        "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64"
        | "float16" | "float32" | "float64" | "bool" => Ok(value.clone()),
        "object" | "string[python]" | "string[pyarrow]" => Ok("utf8".to_string()),
        "category" => Ok("dictionary<int32, utf8>".to_string()),
        "datetime64[ns]" => Ok("timestamp[ns]".to_string()),
        "datetime64[ns,utc]" => Ok("timestamp[ns, tz=UTC]".to_string()),
        "timedelta64[ns]" => Ok("duration[ns]".to_string()),
        _ => Err(unknown_dtype("pandas", &value)),
    }
}

fn normalize_polars_dtype(value: &str) -> CardPyResult<String> {
    let lower = value.to_ascii_lowercase();
    match value {
        "Int8" | "Int16" | "Int32" | "Int64" | "UInt8" | "UInt16" | "UInt32" | "UInt64"
        | "Float32" | "Float64" => return Ok(lower),
        "Boolean" => return Ok("bool".to_string()),
        "Utf8" | "String" => return Ok("utf8".to_string()),
        "Categorical" => return Ok("dictionary<int32, utf8>".to_string()),
        "Date" => return Ok("date32".to_string()),
        _ => {}
    }

    if let Some((unit, timezone)) = parse_polars_datetime(value) {
        return Ok(match timezone {
            Some(timezone) => format!("timestamp[{unit}, tz={timezone}]"),
            None => format!("timestamp[{unit}]"),
        });
    }
    if let Some(unit) = parse_call_one_arg(value, "Duration") {
        return Ok(format!("duration[{unit}]"));
    }
    if let Some(inner) = parse_call_one_arg(value, "List") {
        return Ok(format!("list<{}>", normalize_polars_dtype(&inner)?));
    }
    if let Some(fields) = parse_wrapped(value, "Struct", '(', ')') {
        return normalize_struct_fields(fields, "polars");
    }
    Err(unknown_dtype("polars", value))
}

fn normalize_pyarrow_dtype(value: &str) -> CardPyResult<String> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "int8" | "int8type" => Ok("int8".to_string()),
        "int16" | "int16type" => Ok("int16".to_string()),
        "int32" | "int32type" => Ok("int32".to_string()),
        "int64" | "int64type" => Ok("int64".to_string()),
        "uint8" | "uint8type" => Ok("uint8".to_string()),
        "uint16" | "uint16type" => Ok("uint16".to_string()),
        "uint32" | "uint32type" => Ok("uint32".to_string()),
        "uint64" | "uint64type" => Ok("uint64".to_string()),
        "float16" | "float16type" => Ok("float16".to_string()),
        "float32" | "float32type" => Ok("float32".to_string()),
        "float64" | "double" | "float64type" => Ok("float64".to_string()),
        "bool" | "booltype" => Ok("bool".to_string()),
        "string" | "utf8" | "utf8type" | "large_string" | "largestringtype" | "string_view"
        | "utf8viewtype" => Ok("utf8".to_string()),
        "binary" | "binarytype" | "large_binary" | "largebinarytype" => Ok("binary".to_string()),
        "date32" | "date32[day]" | "date32type" => Ok("date32".to_string()),
        "date64" | "date64[ms]" | "date64type" => Ok("date64".to_string()),
        _ if lower.starts_with("timestamp[") => normalize_arrow_timestamp(value),
        _ if lower.starts_with("timestamp(") => normalize_arrow_timestamp_call(value),
        _ if lower.starts_with("dictionary<") => normalize_arrow_dictionary(value),
        _ if lower.starts_with("dictionarytype(") => normalize_arrow_dictionary_call(value),
        _ if lower.starts_with("list<") || lower.starts_with("large_list<") => {
            normalize_arrow_list(value)
        }
        _ if lower.starts_with("listtype(") || lower.starts_with("largelisttype(") => {
            normalize_arrow_list_call(value)
        }
        _ if lower.starts_with("struct<") => normalize_arrow_struct(value),
        _ if lower.starts_with("structtype(") => normalize_arrow_struct_call(value),
        _ => Err(unknown_dtype("pyarrow", value)),
    }
}

fn normalize_numpy_dtype(value: &str) -> CardPyResult<String> {
    match value {
        "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64"
        | "float16" | "float32" | "float64" => Ok(value.to_string()),
        "bool_" | "bool" => Ok("bool".to_string()),
        "bytes_" | "bytes" => Ok("binary".to_string()),
        "str_" | "str" => Ok("utf8".to_string()),
        _ if value.to_ascii_lowercase().starts_with("|s") => Ok("binary".to_string()),
        _ if value.to_ascii_lowercase().starts_with("<u")
            || value.to_ascii_lowercase().starts_with("|u") =>
        {
            Ok("utf8".to_string())
        }
        _ if value.starts_with("datetime64[") && value.ends_with(']') => {
            let unit = &value["datetime64[".len()..value.len() - 1];
            Ok(format!("timestamp[{unit}]"))
        }
        _ => Err(unknown_dtype("numpy", value)),
    }
}

fn normalize_torch_dtype(value: &str) -> CardPyResult<String> {
    let value = value.strip_prefix("torch.").unwrap_or(value);
    match value {
        "int8" | "int16" | "int32" | "int64" | "uint8" | "float16" | "bfloat16" | "float32"
        | "float64" | "bool" => Ok(value.to_string()),
        _ => Err(unknown_dtype("torch", value)),
    }
}

fn normalize_token(value: &str) -> Cow<'_, str> {
    Cow::Owned(value.trim().replace(' ', ""))
}

fn unknown_dtype(source: &str, value: &str) -> WyrdPyError {
    WyrdPyError::unknown_data_type(format!("unsupported {source} dtype: {value}"))
}

fn parse_call_one_arg(value: &str, name: &str) -> Option<String> {
    parse_wrapped(value, name, '(', ')').map(|inner| trim_quotes(inner).to_string())
}

fn parse_wrapped<'a>(value: &'a str, name: &str, open: char, close: char) -> Option<&'a str> {
    value
        .strip_prefix(name)?
        .strip_prefix(open)?
        .strip_suffix(close)
}

fn parse_polars_datetime(value: &str) -> Option<(String, Option<String>)> {
    let inner = parse_wrapped(value, "Datetime", '(', ')')?;
    let parts = split_top_level(inner, ',');
    let unit = parts
        .first()
        .map(|part| trim_key_value(part, "time_unit"))?;
    let unit = trim_quotes(unit).to_string();
    let timezone = parts.get(1).map(|part| trim_key_value(part, "time_zone"));
    let timezone = timezone
        .map(trim_quotes)
        .filter(|value| !value.eq_ignore_ascii_case("none") && !value.is_empty())
        .map(str::to_string);
    Some((unit, timezone))
}

fn normalize_arrow_timestamp(value: &str) -> CardPyResult<String> {
    let inner =
        parse_square_inner(value, "timestamp").ok_or_else(|| unknown_dtype("pyarrow", value))?;
    Ok(format!("timestamp[{}]", normalize_timestamp_inner(inner)))
}

fn normalize_arrow_timestamp_call(value: &str) -> CardPyResult<String> {
    let inner = parse_wrapped(value, "Timestamp", '(', ')')
        .or_else(|| parse_wrapped(value, "TimestampType", '(', ')'))
        .or_else(|| parse_wrapped(value, "timestamptype", '(', ')'))
        .ok_or_else(|| unknown_dtype("pyarrow", value))?;
    Ok(format!("timestamp[{}]", normalize_timestamp_inner(inner)))
}

fn normalize_timestamp_inner(inner: &str) -> String {
    let mut unit = "";
    let mut timezone = None;
    for part in split_top_level(inner, ',') {
        if part.contains("tz=") {
            timezone = Some(trim_quotes(trim_key_value(part, "tz")).to_string());
        } else if part.contains("unit=") {
            unit = trim_quotes(trim_key_value(part, "unit"));
        } else if unit.is_empty() {
            unit = trim_quotes(part);
        }
    }
    match timezone.filter(|value| !value.eq_ignore_ascii_case("none") && !value.is_empty()) {
        Some(timezone) => format!("{unit}, tz={timezone}"),
        None => unit.to_string(),
    }
}

fn normalize_arrow_dictionary(value: &str) -> CardPyResult<String> {
    let inner =
        parse_bracket_inner(value, "dictionary").ok_or_else(|| unknown_dtype("pyarrow", value))?;
    let parts = split_top_level(inner, ',');
    let mut index = None;
    let mut values = None;
    for part in parts {
        if part.contains("indices=") {
            index = Some(trim_key_value(part, "indices"));
        } else if part.contains("values=") {
            values = Some(trim_key_value(part, "values"));
        }
    }
    let index = index.unwrap_or_else(|| split_top_level(inner, ',').first().copied().unwrap_or(""));
    let values =
        values.unwrap_or_else(|| split_top_level(inner, ',').get(1).copied().unwrap_or(""));
    Ok(format!(
        "dictionary<{}, {}>",
        normalize_pyarrow_dtype(index)?,
        normalize_pyarrow_dtype(values)?
    ))
}

fn normalize_arrow_dictionary_call(value: &str) -> CardPyResult<String> {
    let inner = parse_wrapped(value, "DictionaryType", '(', ')')
        .or_else(|| parse_wrapped(value, "dictionarytype", '(', ')'))
        .ok_or_else(|| unknown_dtype("pyarrow", value))?;
    let parts = split_top_level(inner, ',');
    if parts.len() < 2 {
        return Err(unknown_dtype("pyarrow", value));
    }
    Ok(format!(
        "dictionary<{}, {}>",
        normalize_pyarrow_dtype(parts[0])?,
        normalize_pyarrow_dtype(parts[1])?
    ))
}

fn normalize_arrow_list(value: &str) -> CardPyResult<String> {
    let inner = parse_bracket_inner(value, "list")
        .or_else(|| parse_bracket_inner(value, "large_list"))
        .ok_or_else(|| unknown_dtype("pyarrow", value))?;
    let dtype = inner.rsplit_once(':').map_or(inner, |(_, dtype)| dtype);
    Ok(format!("list<{}>", normalize_pyarrow_dtype(dtype)?))
}

fn normalize_arrow_list_call(value: &str) -> CardPyResult<String> {
    let inner = parse_wrapped(value, "ListType", '(', ')')
        .or_else(|| parse_wrapped(value, "LargeListType", '(', ')'))
        .or_else(|| parse_wrapped(value, "listtype", '(', ')'))
        .or_else(|| parse_wrapped(value, "largelisttype", '(', ')'))
        .ok_or_else(|| unknown_dtype("pyarrow", value))?;
    Ok(format!("list<{}>", normalize_pyarrow_dtype(inner)?))
}

fn normalize_arrow_struct(value: &str) -> CardPyResult<String> {
    let inner =
        parse_bracket_inner(value, "struct").ok_or_else(|| unknown_dtype("pyarrow", value))?;
    normalize_struct_fields(inner, "pyarrow")
}

fn normalize_arrow_struct_call(value: &str) -> CardPyResult<String> {
    let inner = parse_wrapped(value, "StructType", '(', ')')
        .or_else(|| parse_wrapped(value, "structtype", '(', ')'))
        .ok_or_else(|| unknown_dtype("pyarrow", value))?;
    normalize_struct_fields(inner, "pyarrow")
}

fn normalize_struct_fields(fields: &str, source: &str) -> CardPyResult<String> {
    let fields = fields
        .trim()
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(fields);
    let values = split_top_level(fields, ',')
        .into_iter()
        .filter(|field| !field.is_empty())
        .map(|field| {
            let (name, dtype) = field
                .split_once(':')
                .ok_or_else(|| unknown_dtype(source, fields))?;
            Ok(format!(
                "{}:{}",
                trim_quotes(name),
                normalize_dtype(source, dtype)?
            ))
        })
        .collect::<CardPyResult<Vec<String>>>()?;
    Ok(format!("struct<{}>", values.join(", ")))
}

fn parse_bracket_inner<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .strip_prefix(prefix)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

fn parse_square_inner<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .strip_prefix(prefix)?
        .strip_prefix('[')?
        .strip_suffix(']')
}

fn split_top_level(value: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (index, ch) in value.char_indices() {
        match ch {
            '<' | '[' | '(' | '{' => depth += 1,
            '>' | ']' | ')' | '}' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                parts.push(value[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

fn trim_key_value<'a>(value: &'a str, key: &str) -> &'a str {
    value
        .strip_prefix(key)
        .and_then(|value| value.strip_prefix('='))
        .unwrap_or(value)
}

fn trim_quotes(value: &str) -> &str {
    value.trim().trim_matches('\'').trim_matches('"')
}

#[cfg(feature = "python")]
fn infer_pandas_schema(data: &Bound<'_, PyAny>) -> CardPyResult<DataSchema> {
    let items = data.getattr("dtypes")?.call_method0("items")?;
    schema_from_items(&items, "pandas")
}

#[cfg(feature = "python")]
fn infer_polars_schema(data: &Bound<'_, PyAny>) -> CardPyResult<DataSchema> {
    let items = data.getattr("schema")?.call_method0("items")?;
    schema_from_items(&items, "polars")
}

#[cfg(feature = "python")]
fn infer_arrow_schema(data: &Bound<'_, PyAny>) -> CardPyResult<DataSchema> {
    let schema = data.getattr("schema")?;
    let names = schema.getattr("names")?.extract::<Vec<String>>()?;
    let types = schema.getattr("types")?;
    let mut fields = Vec::new();
    for (name, dtype) in names.iter().zip(types.try_iter()?) {
        let dtype = dtype?.str()?.extract::<String>()?;
        fields.push(field_spec(
            name,
            normalize_dtype("pyarrow", &dtype)?,
            Vec::new(),
        )?);
    }
    Ok(DataSchema::new(fields))
}

#[cfg(feature = "python")]
fn infer_numpy_schema(data: &Bound<'_, PyAny>) -> CardPyResult<DataSchema> {
    let dtype = numpy_dtype_string(data)?;
    Ok(DataSchema::new(vec![field_spec(
        "value",
        normalize_dtype("numpy", &dtype)?,
        shape_dims(data)?,
    )?]))
}

#[cfg(feature = "python")]
fn infer_torch_schema(data: &Bound<'_, PyAny>) -> CardPyResult<DataSchema> {
    let dtype = data.getattr("dtype")?.str()?.extract::<String>()?;
    Ok(DataSchema::new(vec![field_spec(
        "value",
        normalize_dtype("torch", &dtype)?,
        shape_dims(data)?,
    )?]))
}

#[cfg(feature = "python")]
fn schema_from_items(items: &Bound<'_, PyAny>, source: &str) -> CardPyResult<DataSchema> {
    let mut fields = Vec::new();
    for item in items.try_iter()? {
        let item = item?;
        let tuple = item.cast::<PyTuple>()?;
        let name = tuple.get_item(0)?.str()?.extract::<String>()?;
        let dtype = tuple.get_item(1)?.str()?.extract::<String>()?;
        fields.push(field_spec(
            &name,
            normalize_dtype(source, &dtype)?,
            Vec::new(),
        )?);
    }
    Ok(DataSchema::new(fields))
}

#[cfg(feature = "python")]
fn shape_dims(data: &Bound<'_, PyAny>) -> CardPyResult<Vec<Dim>> {
    Ok(shape_values(data)?
        .into_iter()
        .map(Dim::Fixed)
        .collect::<Vec<Dim>>())
}

#[cfg(feature = "python")]
fn shape_values(data: &Bound<'_, PyAny>) -> CardPyResult<Vec<i64>> {
    let shape = data.getattr("shape")?;
    let mut values = Vec::new();
    for dim in shape.try_iter()? {
        values.push(dim?.extract::<i64>()?);
    }
    Ok(values)
}

#[cfg(feature = "python")]
fn numpy_dtype_string(data: &Bound<'_, PyAny>) -> CardPyResult<String> {
    let dtype = data.getattr("dtype")?;
    let dtype_string = dtype.str()?.extract::<String>()?;
    if dtype_string.starts_with("datetime64[") {
        return Ok(dtype_string);
    }
    Ok(dtype
        .getattr("type")?
        .getattr("__name__")?
        .extract::<String>()?)
}

#[cfg(feature = "python")]
fn field_spec(name: &str, dtype: String, shape: Vec<Dim>) -> CardPyResult<FieldSpec> {
    let name = ColumnName::new(name).map_err(|source| {
        WyrdPyError::validation_with_details(
            "DataCard schema column name is invalid",
            serde_json::json!({
                "column": name,
                "source": source.to_string(),
            }),
        )
    })?;
    Ok(FieldSpec {
        name,
        dtype,
        shape,
        nullable: false,
        extra: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{is_parquet_path, jsonl_compression_for_path, normalize_dtype};
    use crate::error::WyrdPyError;
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

    #[test]
    fn normalize_pandas_dtypes_from_locked_table() {
        for dtype in [
            "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64", "float16",
            "float32", "float64", "bool",
        ] {
            assert_eq!(normalize_dtype("pandas", dtype).unwrap(), dtype);
        }
        assert_eq!(normalize_dtype("pandas", "object").unwrap(), "utf8");
        assert_eq!(normalize_dtype("pandas", "string[python]").unwrap(), "utf8");
        assert_eq!(
            normalize_dtype("pandas", "string[pyarrow]").unwrap(),
            "utf8"
        );
        assert_eq!(
            normalize_dtype("pandas", "category").unwrap(),
            "dictionary<int32, utf8>"
        );
        assert_eq!(
            normalize_dtype("pandas", "datetime64[ns]").unwrap(),
            "timestamp[ns]"
        );
        assert_eq!(
            normalize_dtype("pandas", "datetime64[ns, UTC]").unwrap(),
            "timestamp[ns, tz=UTC]"
        );
        assert_eq!(
            normalize_dtype("pandas", "timedelta64[ns]").unwrap(),
            "duration[ns]"
        );
    }

    #[test]
    fn normalize_polars_dtypes_from_locked_table() {
        for (source, canonical) in [
            ("Int8", "int8"),
            ("Int16", "int16"),
            ("Int32", "int32"),
            ("Int64", "int64"),
            ("UInt8", "uint8"),
            ("UInt16", "uint16"),
            ("UInt32", "uint32"),
            ("UInt64", "uint64"),
            ("Float32", "float32"),
            ("Float64", "float64"),
            ("Boolean", "bool"),
            ("Utf8", "utf8"),
            ("String", "utf8"),
            ("Categorical", "dictionary<int32, utf8>"),
            ("Date", "date32"),
            ("Datetime(ns,UTC)", "timestamp[ns, tz=UTC]"),
            ("Datetime(us,None)", "timestamp[us]"),
            ("Duration(ms)", "duration[ms]"),
            ("List(Int64)", "list<int64>"),
            ("Struct(a:Int64,b:Utf8)", "struct<a:int64, b:utf8>"),
        ] {
            assert_eq!(normalize_dtype("polars", source).unwrap(), canonical);
        }
    }

    #[test]
    fn normalize_pyarrow_dtypes_from_locked_table() {
        for (source, canonical) in [
            ("Int8Type", "int8"),
            ("Int16Type", "int16"),
            ("Int32Type", "int32"),
            ("Int64Type", "int64"),
            ("UInt8Type", "uint8"),
            ("UInt16Type", "uint16"),
            ("UInt32Type", "uint32"),
            ("UInt64Type", "uint64"),
            ("Float16Type", "float16"),
            ("Float32Type", "float32"),
            ("Float64Type", "float64"),
            ("BoolType", "bool"),
            ("Utf8Type", "utf8"),
            ("LargeStringType", "utf8"),
            ("Utf8ViewType", "utf8"),
            ("BinaryType", "binary"),
            ("LargeBinaryType", "binary"),
            ("Date32Type", "date32"),
            ("Date64Type", "date64"),
            ("timestamp[ns, tz=UTC]", "timestamp[ns, tz=UTC]"),
            ("DictionaryType(int32, string)", "dictionary<int32, utf8>"),
            ("ListType(int64)", "list<int64>"),
            ("LargeListType(string)", "list<utf8>"),
            ("StructType(a:int64,b:string)", "struct<a:int64, b:utf8>"),
        ] {
            assert_eq!(normalize_dtype("pyarrow", source).unwrap(), canonical);
        }
    }

    #[test]
    fn normalize_numpy_dtypes_from_locked_table() {
        for dtype in [
            "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64", "float16",
            "float32", "float64",
        ] {
            assert_eq!(normalize_dtype("numpy", dtype).unwrap(), dtype);
        }
        assert_eq!(normalize_dtype("numpy", "bool_").unwrap(), "bool");
        assert_eq!(normalize_dtype("numpy", "bytes_").unwrap(), "binary");
        assert_eq!(normalize_dtype("numpy", "str_").unwrap(), "utf8");
        assert_eq!(
            normalize_dtype("numpy", "datetime64[ns]").unwrap(),
            "timestamp[ns]"
        );
    }

    #[test]
    fn normalize_torch_dtypes_from_locked_table() {
        for dtype in [
            "int8", "int16", "int32", "int64", "uint8", "float16", "bfloat16", "float32",
            "float64", "bool",
        ] {
            assert_eq!(normalize_dtype("torch", dtype).unwrap(), dtype);
            assert_eq!(
                normalize_dtype("torch", &format!("torch.{dtype}")).unwrap(),
                dtype
            );
        }
    }

    #[test]
    fn normalize_unknown_dtype_returns_wyrd_error_code() {
        let error = normalize_dtype("numpy", "complex64").unwrap_err();
        let WyrdPyError::Spec(error) = error else {
            panic!("unknown dtype should map to a public WyrdError");
        };
        assert_eq!(error.code(), "WYRD_DATA_400_UNKNOWN_DATA_TYPE");
    }
}
