//! Wyrd-native media references for `${media:name}` placeholders.
//!
//! `MediaRef` is pure data. It carries enough information for
//! [`crate::Prompt::bind_media`] to replace a media placeholder with the native
//! OpenAI, Anthropic, Gemini, or Vertex content block.

use serde::{Deserialize, Serialize};

/// Wyrd-native media reference for `${media:name}` placeholder substitution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MediaRef {
    /// Image vs document classification.
    pub kind: MediaKind,
    /// Concrete payload, such as URL, inline base64, or provider file URI.
    pub source: MediaSource,
}

/// Image vs document classification for a [`MediaRef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    /// Image media.
    Image,
    /// Document media.
    Document,
}

/// Concrete payload for a [`MediaRef`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaSource {
    /// Remote URL reference. Gemini and Vertex require `mime_type` and accept
    /// only `gs://` or Gemini file API URIs.
    Url {
        /// Remote URL passed to the provider.
        url: String,
        /// Optional MIME type.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    /// Inline base64 payload without a `data:` prefix.
    Base64 {
        /// Required MIME type.
        mime_type: String,
        /// Base64 payload.
        data: String,
    },
    /// Provider-managed file id or file URI.
    File {
        /// OpenAI/Anthropic file id, Gemini file API URI, or Vertex file URI.
        uri: String,
        /// Optional MIME type. Gemini and Vertex require this field.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
}

impl MediaRef {
    /// Construct an image reference from a URL.
    pub fn image_url(url: impl Into<String>, mime_type: Option<String>) -> Self {
        Self {
            kind: MediaKind::Image,
            source: MediaSource::Url {
                url: url.into(),
                mime_type,
            },
        }
    }

    /// Construct an image reference from an inline base64 payload.
    pub fn image_base64(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            kind: MediaKind::Image,
            source: MediaSource::Base64 {
                mime_type: mime_type.into(),
                data: data.into(),
            },
        }
    }

    /// Construct an image reference from a provider file id or URI.
    pub fn image_file(uri: impl Into<String>, mime_type: Option<String>) -> Self {
        Self {
            kind: MediaKind::Image,
            source: MediaSource::File {
                uri: uri.into(),
                mime_type,
            },
        }
    }

    /// Construct a document reference from a URL.
    pub fn document_url(url: impl Into<String>, mime_type: Option<String>) -> Self {
        Self {
            kind: MediaKind::Document,
            source: MediaSource::Url {
                url: url.into(),
                mime_type,
            },
        }
    }

    /// Construct a document reference from an inline base64 payload.
    pub fn document_base64(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            kind: MediaKind::Document,
            source: MediaSource::Base64 {
                mime_type: mime_type.into(),
                data: data.into(),
            },
        }
    }

    /// Construct a document reference from a provider file id or URI.
    pub fn document_file(uri: impl Into<String>, mime_type: Option<String>) -> Self {
        Self {
            kind: MediaKind::Document,
            source: MediaSource::File {
                uri: uri.into(),
                mime_type,
            },
        }
    }
}
