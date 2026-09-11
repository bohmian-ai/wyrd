//! Data-specific local IO helpers for data interfaces.

use std::fs;
use std::path::{Path, PathBuf};
use wyrd_spec::error::WyrdError;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use wyrd_spec::card::data::{ColorMode, DataSchema, ImageFormat, JsonlCompression};
use wyrd_spec::card::field::FieldSpec;
use wyrd_spec::ids::ColumnName;

#[cfg(feature = "python")]
use wyrd_utils::py::WyrdPyResult;

#[cfg(feature = "python")]
use {
    crate::data::stats::PyDataStats,
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict},
    serde_json::json,
    std::collections::{BTreeMap, HashMap},
    wyrd_spec::card::data::{DataStats, SqlLogic},
    wyrd_spec::ids::QueryName,
};

/// Manifest entry for image and text datasets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Source file path captured in the manifest.
    pub path: String,
}

/// Image dataset manifest written under the local `DataCard` convention path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageManifest {
    /// Ordered image file entries.
    pub files: Vec<ManifestEntry>,
    /// Declared image format family.
    pub format: ImageFormat,
    /// Declared image color mode.
    pub color_mode: ColorMode,
}

/// Text dataset manifest written under the local `DataCard` convention path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextManifest {
    /// Ordered text file entries.
    pub files: Vec<ManifestEntry>,
    /// Declared text encoding.
    pub encoding: String,
}

/// Pinned Hugging Face dataset pointer written for remote-only local materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HuggingfacePointer {
    /// Dataset identifier passed to `datasets.load_dataset`.
    pub dataset_id: String,
    /// Pinned git revision.
    pub revision: String,
    /// Optional dataset split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Optional dataset config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

/// Return the convention-relative JSONL artifact path for a compression mode.
#[must_use]
pub fn jsonl_relative_path_for(compression: JsonlCompression) -> PathBuf {
    match compression {
        JsonlCompression::None => PathBuf::from("data/data.jsonl"),
        JsonlCompression::Gzip => PathBuf::from("data/data.jsonl.gz"),
        JsonlCompression::Zstd => PathBuf::from("data/data.jsonl.zst"),
    }
}

/// Build the JSON value for a pinned Hugging Face dataset pointer.
#[must_use]
pub fn huggingface_pointer(
    dataset_id: &str,
    revision: &str,
    split: Option<&str>,
    config: Option<&str>,
) -> Value {
    let mut pointer = Map::new();
    pointer.insert(
        "dataset_id".to_string(),
        Value::String(dataset_id.to_string()),
    );
    pointer.insert("revision".to_string(), Value::String(revision.to_string()));
    if let Some(split) = split {
        pointer.insert("split".to_string(), Value::String(split.to_string()));
    }
    if let Some(config) = config {
        pointer.insert("config".to_string(), Value::String(config.to_string()));
    }
    Value::Object(pointer)
}

/// Read a pinned Hugging Face dataset pointer from a local JSON file.
///
/// # Errors
/// Returns an error when the file cannot be read or does not match the pointer shape.
pub fn read_huggingface_pointer(path: &Path) -> Result<HuggingfacePointer, WyrdError> {
    wyrd_utils::fs::require_local_file(path).map_err(|error| crate::error::io_error(&error))?;
    let contents = fs::read_to_string(path).map_err(|error| crate::error::io_error(&error))?;
    serde_json::from_str(&contents).map_err(|error| crate::error::json_error(&error))
}

/// Return the fixed image manifest schema.
pub fn image_manifest_schema() -> Result<DataSchema, WyrdError> {
    Ok(DataSchema::new(vec![
        field("path", "utf8")?,
        field("format", "utf8")?,
        field("color_mode", "utf8")?,
    ]))
}

/// Return the fixed text manifest schema.
pub fn text_manifest_schema() -> Result<DataSchema, WyrdError> {
    Ok(DataSchema::new(vec![
        field("path", "utf8")?,
        field("encoding", "utf8")?,
    ]))
}

#[cfg(feature = "python")]
/// Normalize a Python JSONL source into the configured local artifact file.
///
/// # Errors
/// Returns an error when the source cannot be read or serialized as JSONL.
pub fn write_jsonl_normalized(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    path: &Path,
    compression: JsonlCompression,
) -> WyrdPyResult<()> {
    let bytes = if crate::data::dtype::is_path_like(py, data)? {
        let source = crate::data::dtype::extract_pathbuf(data)?;
        wyrd_utils::fs::require_local_file(&source)
            .map_err(|error| crate::error::io_error(&error))?;
        // BLOCKING: synchronous filesystem I/O; do not call from an async executor
        // without tokio::task::spawn_blocking.
        let source_bytes = fs::read(&source)?;
        decode_jsonl_bytes(
            &source_bytes,
            compression_from_path(&source).unwrap_or(compression),
        )?
    } else {
        let mut payload = Vec::new();
        for item in data.try_iter()? {
            let value = wyrd_utils::py::pyobject_to_json(&item?)?;
            payload.extend_from_slice(serde_json::to_string(&value)?.as_bytes());
            payload.push(b'\n');
        }
        payload
    };

    let encoded = encode_jsonl_bytes(&bytes, compression)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // BLOCKING: synchronous filesystem I/O; do not call from an async executor
    // without tokio::task::spawn_blocking.
    fs::write(path, encoded)?;
    Ok(())
}

#[cfg(feature = "python")]
/// Read a local JSONL artifact into a Python list of JSON objects.
///
/// # Errors
/// Returns an error when the file cannot be read, decoded, or parsed.
pub fn read_jsonl_to_py<'py>(
    py: Python<'py>,
    path: &Path,
    compression: Option<&str>,
    _load_kwargs: Option<&Bound<'_, PyDict>>,
) -> WyrdPyResult<Bound<'py, PyAny>> {
    wyrd_utils::fs::require_local_file(path).map_err(|error| crate::error::io_error(&error))?;
    // BLOCKING: synchronous filesystem I/O; do not call from an async executor
    // without tokio::task::spawn_blocking.
    let bytes = fs::read(path)?;
    let compression = compression
        .map(parse_jsonl_compression_label)
        .transpose()?
        .or_else(|| compression_from_path(path))
        .unwrap_or(JsonlCompression::None);
    let decoded = decode_jsonl_bytes(&bytes, compression)?;
    let contents = String::from_utf8(decoded).map_err(|error| crate::error::json_error(&error))?;
    let mut values = Vec::new();
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        values.push(serde_json::from_str::<Value>(line)?);
    }
    Ok(wyrd_utils::py::json_to_pyobject(py, &Value::Array(values))?.into_bound(py))
}

#[cfg(feature = "python")]
/// Build an image manifest from a Python path, iterable, or manifest-like value.
///
/// # Errors
/// Returns an error when the source cannot be normalized into file entries.
pub fn image_manifest_from_data(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    format: ImageFormat,
    color_mode: ColorMode,
) -> WyrdPyResult<ImageManifest> {
    Ok(ImageManifest {
        files: manifest_entries_from_data(py, data)?,
        format,
        color_mode,
    })
}

#[cfg(feature = "python")]
/// Build a text manifest from a Python path, iterable, or manifest-like value.
///
/// # Errors
/// Returns an error when the source cannot be normalized into file entries.
pub fn text_manifest_from_data(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    encoding: &str,
) -> WyrdPyResult<TextManifest> {
    Ok(TextManifest {
        files: manifest_entries_from_data(py, data)?,
        encoding: encoding.to_string(),
    })
}

#[cfg(feature = "python")]
/// Read a manifest JSON file into a Python object.
///
/// # Errors
/// Returns an error when the file cannot be read or parsed as JSON.
pub fn manifest_json_to_py<'py>(py: Python<'py>, path: &Path) -> WyrdPyResult<Bound<'py, PyAny>> {
    serde_json_file_to_py(py, path)
}

#[cfg(feature = "python")]
/// Convert Python SQL data into Wyrd SQL logic.
///
/// # Errors
/// Returns a validation error when query keys or values are invalid.
pub fn sql_logic_from_data(py: Python<'_>, data: Option<&Py<PyAny>>) -> WyrdPyResult<SqlLogic> {
    let Some(data) = data else {
        return Ok(SqlLogic {
            queries: HashMap::new(),
            default_query: None,
        });
    };

    let value = wyrd_utils::py::pyobject_to_json(data.bind(py))?;
    sql_logic_from_value(&value)
}

#[cfg(feature = "python")]
/// Read a JSON file and convert it into a Python object.
///
/// # Errors
/// Returns an error when the file cannot be read or parsed as JSON.
pub fn serde_json_file_to_py<'py>(py: Python<'py>, path: &Path) -> WyrdPyResult<Bound<'py, PyAny>> {
    wyrd_utils::fs::require_local_file(path).map_err(|error| crate::error::io_error(&error))?;
    let contents = fs::read_to_string(path)?;
    let value = serde_json::from_str::<Value>(&contents)?;
    Ok(wyrd_utils::py::json_to_pyobject(py, &value)?.into_bound(py))
}

#[cfg(feature = "python")]
/// Convert a Hugging Face pointer into `datasets.load_dataset` keyword arguments.
///
/// # Errors
/// Returns an error when Python dictionary creation fails.
pub fn pointer_to_kwargs(
    py: Python<'_>,
    pointer: HuggingfacePointer,
) -> WyrdPyResult<Bound<'_, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("path", pointer.dataset_id)?;
    kwargs.set_item("revision", pointer.revision)?;
    if let Some(split) = pointer.split {
        kwargs.set_item("split", split)?;
    }
    if let Some(config) = pointer.config {
        kwargs.set_item("name", config)?;
    }
    Ok(kwargs)
}

#[cfg(feature = "python")]
/// Convert a Python JSON-like object into a string-keyed JSON map.
///
/// # Errors
/// Returns a validation error when the value is not a JSON object.
pub fn py_json_to_value_map(value: &Bound<'_, PyAny>) -> WyrdPyResult<BTreeMap<String, Value>> {
    match wyrd_utils::py::pyobject_to_json(value)? {
        Value::Object(values) => Ok(values.into_iter().collect()),
        _ => Err(crate::error::validation("expected a JSON object with string keys").into()),
    }
}

#[cfg(feature = "python")]
/// Dispatch to a Python data interface `save` method and extract returned stats.
///
/// # Errors
/// Returns an error when `save` fails or does not return `DataStats`.
pub fn save_data(
    interface: &Bound<'_, PyAny>,
    path: &Path,
    save_kwargs: Option<&Bound<'_, PyDict>>,
) -> WyrdPyResult<DataStats> {
    let stats = interface.call_method("save", (path.to_path_buf(), save_kwargs), None)?;
    Ok(stats
        .extract::<PyRef<'_, PyDataStats>>()?
        .clone()
        .into_inner())
}

#[cfg(feature = "python")]
/// Dispatch to a Python data interface `load` method.
///
/// # Errors
/// Returns an error when no local artifact path is supplied or `load` fails.
pub fn load_data(
    interface: &Bound<'_, PyAny>,
    path: Option<PathBuf>,
    load_kwargs: Option<&Bound<'_, PyDict>>,
) -> WyrdPyResult<()> {
    let path = path.ok_or_else(|| crate::error::validation("local artifact path is required"))?;
    interface.call_method("load", (path, load_kwargs), None)?;
    Ok(())
}

fn field(name: &str, dtype: &str) -> Result<FieldSpec, WyrdError> {
    let name = ColumnName::new(name).map_err(|error| {
        crate::error::validation_with_details(
            "invalid static manifest schema field name",
            serde_json::json!({
                "field": name,
                "source": error.to_string(),
            }),
        )
    })?;
    Ok(FieldSpec::new(name, dtype))
}

#[cfg(feature = "python")]
fn encode_jsonl_bytes(bytes: &[u8], compression: JsonlCompression) -> WyrdPyResult<Vec<u8>> {
    match compression {
        JsonlCompression::None => Ok(bytes.to_vec()),
        JsonlCompression::Gzip => Ok(wyrd_utils::codec::gzip_encode(bytes)
            .map_err(|error| crate::error::io_error(&error))?),
        JsonlCompression::Zstd => Ok(wyrd_utils::codec::zstd_encode(bytes)
            .map_err(|error| crate::error::io_error(&error))?),
    }
}

#[cfg(feature = "python")]
fn decode_jsonl_bytes(bytes: &[u8], compression: JsonlCompression) -> WyrdPyResult<Vec<u8>> {
    match compression {
        JsonlCompression::None => Ok(bytes.to_vec()),
        JsonlCompression::Gzip => Ok(wyrd_utils::codec::gzip_decode(bytes)
            .map_err(|error| crate::error::io_error(&error))?),
        JsonlCompression::Zstd => Ok(wyrd_utils::codec::zstd_decode(bytes)
            .map_err(|error| crate::error::io_error(&error))?),
    }
}

#[cfg(feature = "python")]
fn compression_from_path(path: &Path) -> Option<JsonlCompression> {
    crate::data::dtype::jsonl_compression_for_path(path).and_then(|value| {
        parse_jsonl_compression_label(value)
            .ok()
            .or(Some(JsonlCompression::None))
    })
}

#[cfg(feature = "python")]
fn parse_jsonl_compression_label(value: &str) -> WyrdPyResult<JsonlCompression> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "none" => Ok(JsonlCompression::None),
        "gzip" => Ok(JsonlCompression::Gzip),
        "zstd" => Ok(JsonlCompression::Zstd),
        got => Err(crate::error::invalid_interface_option(
            "compression",
            got,
            ["none", "gzip", "zstd"],
        )
        .into()),
    }
}

#[cfg(feature = "python")]
fn manifest_entries_from_data(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
) -> WyrdPyResult<Vec<ManifestEntry>> {
    if crate::data::dtype::is_path_like(py, data)? {
        return manifest_entries_from_path(&crate::data::dtype::extract_pathbuf(data)?);
    }

    let value = wyrd_utils::py::pyobject_to_json(data)?;
    match value {
        Value::Object(object) => {
            if let Some(files) = object.get("files") {
                entries_from_json(files)
            } else if let Some(path) = object.get("path").and_then(Value::as_str) {
                manifest_entries_from_path(Path::new(path))
            } else {
                Err(crate::error::validation("manifest object must contain files or path").into())
            }
        }
        Value::Array(_) => entries_from_json(&value),
        Value::String(path) => manifest_entries_from_path(Path::new(&path)),
        _ => Err(crate::error::validation(
            "manifest data must be a path, sequence of paths, or manifest object",
        )
        .into()),
    }
}

#[cfg(feature = "python")]
fn manifest_entries_from_path(path: &Path) -> WyrdPyResult<Vec<ManifestEntry>> {
    wyrd_utils::fs::require_local_path(path).map_err(|error| crate::error::io_error(&error))?;

    if path.is_file() {
        return Ok(vec![entry_for_path(path)]);
    }

    let mut files = Vec::new();
    collect_files(path, &mut files)?;
    files.sort();
    Ok(files.iter().map(|path| entry_for_path(path)).collect())
}

#[cfg(feature = "python")]
fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> WyrdPyResult<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(feature = "python")]
fn entries_from_json(value: &Value) -> WyrdPyResult<Vec<ManifestEntry>> {
    let values = value.as_array().ok_or_else(|| {
        crate::error::validation("manifest files must be a JSON array of paths or objects")
    })?;
    let mut entries = Vec::with_capacity(values.len());
    for value in values {
        match value {
            Value::String(path) => entries.push(ManifestEntry { path: path.clone() }),
            Value::Object(object) => {
                let path = object.get("path").and_then(Value::as_str).ok_or_else(|| {
                    crate::error::validation("manifest file objects must contain a path string")
                })?;
                entries.push(ManifestEntry {
                    path: path.to_string(),
                });
            }
            _ => {
                return Err(crate::error::validation(
                    "manifest file entries must be paths or objects",
                )
                .into());
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

#[cfg(feature = "python")]
fn entry_for_path(path: &Path) -> ManifestEntry {
    ManifestEntry {
        path: path.to_string_lossy().into_owned(),
    }
}

#[cfg(feature = "python")]
fn sql_logic_from_value(value: &Value) -> WyrdPyResult<SqlLogic> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::error::validation("SQL data must be a JSON object"))?;
    let queries_value = object.get("queries").unwrap_or(value);
    let query_object = queries_value
        .as_object()
        .ok_or_else(|| crate::error::validation("SQL queries must be a JSON object"))?;

    let mut queries = HashMap::new();
    for (name, query) in query_object {
        if name == "default_query" && !object.contains_key("queries") {
            continue;
        }
        let query = query.as_str().ok_or_else(|| {
            crate::error::validation_with_details(
                "SQL query values must be strings",
                json!({ "query": name }),
            )
        })?;
        let name = QueryName::new(name).map_err(|error| {
            crate::error::validation_with_details(
                "invalid SQL query name",
                json!({ "query": name, "source": error.to_string() }),
            )
        })?;
        queries.insert(name, query.to_string());
    }

    let default_query = object
        .get("default_query")
        .and_then(Value::as_str)
        .map(QueryName::new)
        .transpose()
        .map_err(|error| {
            crate::error::validation_with_details(
                "invalid default SQL query name",
                json!({ "source": error.to_string() }),
            )
        })?;

    Ok(SqlLogic {
        queries,
        default_query,
    })
}

#[cfg(test)]
mod tests {
    use super::{image_manifest_schema, jsonl_relative_path_for, text_manifest_schema};
    use wyrd_spec::card::data::JsonlCompression;

    #[test]
    fn jsonl_relative_path_tracks_compression() {
        assert_eq!(
            jsonl_relative_path_for(JsonlCompression::None),
            std::path::PathBuf::from("data/data.jsonl")
        );
        assert_eq!(
            jsonl_relative_path_for(JsonlCompression::Gzip),
            std::path::PathBuf::from("data/data.jsonl.gz")
        );
        assert_eq!(
            jsonl_relative_path_for(JsonlCompression::Zstd),
            std::path::PathBuf::from("data/data.jsonl.zst")
        );
    }

    #[test]
    fn manifest_schemas_use_documented_fields() {
        let image = image_manifest_schema().expect("image manifest schema should build");
        let text = text_manifest_schema().expect("text manifest schema should build");

        assert_eq!(image.columns.len(), 3);
        assert_eq!(image.columns[0].name.as_str(), "path");
        assert_eq!(image.columns[1].name.as_str(), "format");
        assert_eq!(image.columns[2].name.as_str(), "color_mode");
        assert_eq!(text.columns.len(), 2);
        assert_eq!(text.columns[0].name.as_str(), "path");
        assert_eq!(text.columns[1].name.as_str(), "encoding");
    }
}
