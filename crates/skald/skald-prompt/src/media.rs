//! Media references and path loaders for prompt media binding.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use skald_spec::{MediaKind, MediaRef, MediaSource, SkaldError, SkaldResult};

#[cfg(feature = "python")]
use pyo3::types::PyBytesMethods;

/// Maximum local media file size read by path helpers.
pub const MAX_MEDIA_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// Construct an image media reference by eagerly reading a local file.
pub fn image_path(path: impl AsRef<Path>) -> SkaldResult<MediaRef> {
    path_ref(path, MediaKind::Image)
        .map(|(mime_type, data)| MediaRef::image_base64(mime_type, data))
}

/// Construct a document media reference by eagerly reading a local file.
pub fn document_path(path: impl AsRef<Path>) -> SkaldResult<MediaRef> {
    path_ref(path, MediaKind::Document)
        .map(|(mime_type, data)| MediaRef::document_base64(mime_type, data))
}

fn path_ref(path: impl AsRef<Path>, kind: MediaKind) -> SkaldResult<(String, String)> {
    let path = path.as_ref();
    let path_string = path.display().to_string();
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| SkaldError::MediaIo(format!("{}: {}", path.display(), error)))?;
    if !metadata.file_type().is_file() {
        return Err(SkaldError::MediaNotRegularFile { path: path_string });
    }
    if metadata.len() > MAX_MEDIA_FILE_BYTES {
        return Err(SkaldError::MediaTooLarge {
            path: path_string,
            size: metadata.len(),
            limit: MAX_MEDIA_FILE_BYTES,
        });
    }
    let mime =
        mime_from_extension(path, kind).ok_or_else(|| SkaldError::MediaInvalidExtension {
            path: path_string.clone(),
            kind,
        })?;
    let bytes = std::fs::read(path)
        .map_err(|error| SkaldError::MediaIo(format!("{}: {}", path.display(), error)))?;
    Ok((mime.to_owned(), STANDARD.encode(bytes)))
}

fn mime_from_extension(path: &Path, kind: MediaKind) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match (kind, ext.as_str()) {
        (MediaKind::Image, "png") => Some("image/png"),
        (MediaKind::Image, "jpg" | "jpeg") => Some("image/jpeg"),
        (MediaKind::Image, "gif") => Some("image/gif"),
        (MediaKind::Image, "webp") => Some("image/webp"),
        (MediaKind::Document, "pdf") => Some("application/pdf"),
        (MediaKind::Document, "txt") => Some("text/plain"),
        (MediaKind::Document, "md") => Some("text/markdown"),
        (MediaKind::Document, "json") => Some("application/json"),
        (MediaKind::Document, "csv") => Some("text/csv"),
        (MediaKind::Document, "html" | "htm") => Some("text/html"),
        _ => None,
    }
}

/// Python-facing wrapper around a pure [`MediaRef`].
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.prompt", name = "MediaRef", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyMediaRef {
    pub(crate) inner: MediaRef,
}

impl PyMediaRef {
    /// Wrap a native media reference.
    pub const fn from_native(inner: MediaRef) -> Self {
        Self { inner }
    }

    /// Borrow the native media reference.
    pub const fn native(&self) -> &MediaRef {
        &self.inner
    }
}

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl PyMediaRef {
    /// Construct an image reference from a URL.
    #[staticmethod]
    #[pyo3(signature = (url, *, mime_type=None))]
    pub fn image_url(url: String, mime_type: Option<String>) -> Self {
        Self::from_native(MediaRef::image_url(url, mime_type))
    }

    /// Construct an image reference from bytes.
    #[staticmethod]
    pub fn image_bytes(mime_type: String, data: &pyo3::Bound<'_, pyo3::types::PyBytes>) -> Self {
        Self::from_native(MediaRef::image_base64(
            mime_type,
            STANDARD.encode(data.as_bytes()),
        ))
    }

    /// Construct an image reference from base64 bytes.
    #[staticmethod]
    pub fn image_base64(mime_type: String, data: String) -> Self {
        Self::from_native(MediaRef::image_base64(mime_type, data))
    }

    /// Construct an image reference from a provider file id or URI.
    #[staticmethod]
    #[pyo3(signature = (uri, *, mime_type=None))]
    pub fn image_file(uri: String, mime_type: Option<String>) -> Self {
        Self::from_native(MediaRef::image_file(uri, mime_type))
    }

    /// Construct an image reference by eagerly reading a local path.
    #[staticmethod]
    pub fn image_path(path: std::path::PathBuf) -> wyrd_interfaces::error::CardPyResult<Self> {
        Ok(Self::from_native(
            image_path(path).map_err(crate::error::PromptBuilderError::from)?,
        ))
    }

    /// Construct a document reference from a URL.
    #[staticmethod]
    #[pyo3(signature = (url, *, mime_type=None))]
    pub fn document_url(url: String, mime_type: Option<String>) -> Self {
        Self::from_native(MediaRef::document_url(url, mime_type))
    }

    /// Construct a document reference from bytes.
    #[staticmethod]
    pub fn document_bytes(mime_type: String, data: &pyo3::Bound<'_, pyo3::types::PyBytes>) -> Self {
        Self::from_native(MediaRef::document_base64(
            mime_type,
            STANDARD.encode(data.as_bytes()),
        ))
    }

    /// Construct a document reference from base64 bytes.
    #[staticmethod]
    pub fn document_base64(mime_type: String, data: String) -> Self {
        Self::from_native(MediaRef::document_base64(mime_type, data))
    }

    /// Construct a document reference from a provider file id or URI.
    #[staticmethod]
    #[pyo3(signature = (uri, *, mime_type=None))]
    pub fn document_file(uri: String, mime_type: Option<String>) -> Self {
        Self::from_native(MediaRef::document_file(uri, mime_type))
    }

    /// Construct a document reference by eagerly reading a local path.
    #[staticmethod]
    pub fn document_path(path: std::path::PathBuf) -> wyrd_interfaces::error::CardPyResult<Self> {
        Ok(Self::from_native(
            document_path(path).map_err(crate::error::PromptBuilderError::from)?,
        ))
    }

    /// Return `"image"` or `"document"`.
    #[getter]
    pub fn kind(&self) -> &'static str {
        match self.inner.kind {
            MediaKind::Image => "image",
            MediaKind::Document => "document",
        }
    }

    /// Return `"url"`, `"base64"`, or `"file"`.
    #[getter]
    pub fn source_type(&self) -> &'static str {
        match self.inner.source {
            MediaSource::Url { .. } => "url",
            MediaSource::Base64 { .. } => "base64",
            MediaSource::File { .. } => "file",
        }
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "MediaRef(kind={}, source_type={})",
            self.kind(),
            self.source_type()
        )
    }
}
