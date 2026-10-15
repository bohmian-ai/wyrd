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

/// One shared file-format vocabulary for evidence slots, uploads, extraction,
/// and native media bindings.
///
/// Parsing accepts the canonical short token, common extension aliases, and
/// MIME types, case-insensitively; serialization always writes the canonical
/// token. Legacy binary Office formats stay distinct from their OOXML
/// successors so a `ppt` file is never treated as `pptx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MediaFormat {
    /// PNG image.
    Png,
    /// JPEG image.
    Jpeg,
    /// GIF image.
    Gif,
    /// WebP image.
    Webp,
    /// PDF document.
    Pdf,
    /// Plain text.
    Text,
    /// Markdown text.
    Markdown,
    /// Comma-separated values.
    Csv,
    /// JSON document.
    Json,
    /// HTML document.
    Html,
    /// OOXML Word document.
    Docx,
    /// OOXML Excel workbook.
    Xlsx,
    /// OOXML PowerPoint presentation.
    Pptx,
    /// Legacy binary Word document.
    Doc,
    /// Legacy binary Excel workbook.
    Xls,
    /// Legacy binary PowerPoint presentation.
    Ppt,
    /// Gzip-compressed tar archive.
    TarGz,
    /// Zip archive.
    Zip,
}

/// Error returned when a string names no [`MediaFormat`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown media format `{0}`")]
pub struct UnknownMediaFormat(pub String);

impl MediaFormat {
    /// Every format, in declaration order.
    pub const ALL: [Self; 18] = [
        Self::Png,
        Self::Jpeg,
        Self::Gif,
        Self::Webp,
        Self::Pdf,
        Self::Text,
        Self::Markdown,
        Self::Csv,
        Self::Json,
        Self::Html,
        Self::Docx,
        Self::Xlsx,
        Self::Pptx,
        Self::Doc,
        Self::Xls,
        Self::Ppt,
        Self::TarGz,
        Self::Zip,
    ];

    /// Returns the canonical short token written on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Pdf => "pdf",
            Self::Text => "text",
            Self::Markdown => "markdown",
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Html => "html",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Doc => "doc",
            Self::Xls => "xls",
            Self::Ppt => "ppt",
            Self::TarGz => "tar_gz",
            Self::Zip => "zip",
        }
    }

    /// Returns the canonical MIME type.
    #[must_use]
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Pdf => "application/pdf",
            Self::Text => "text/plain",
            Self::Markdown => "text/markdown",
            Self::Csv => "text/csv",
            Self::Json => "application/json",
            Self::Html => "text/html",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Self::Doc => "application/msword",
            Self::Xls => "application/vnd.ms-excel",
            Self::Ppt => "application/vnd.ms-powerpoint",
            Self::TarGz => "application/gzip",
            Self::Zip => "application/zip",
        }
    }

    /// Returns the native model media kind for formats providers accept as
    /// `${media:name}` content, or `None` when the format must be extracted
    /// or read through a workspace instead.
    #[must_use]
    pub const fn native_kind(self) -> Option<MediaKind> {
        match self {
            Self::Png | Self::Jpeg | Self::Gif | Self::Webp => Some(MediaKind::Image),
            Self::Pdf | Self::Text => Some(MediaKind::Document),
            _ => None,
        }
    }

    /// Returns whether the `extract` action produces text and structure for
    /// this format. Images, legacy Office formats, and archives are not
    /// extractable.
    #[must_use]
    pub const fn is_extractable(self) -> bool {
        matches!(
            self,
            Self::Pdf
                | Self::Text
                | Self::Markdown
                | Self::Csv
                | Self::Json
                | Self::Html
                | Self::Docx
                | Self::Xlsx
                | Self::Pptx
        )
    }

    /// Returns whether the format is an archive usable only as a workspace
    /// directory, never as a native model attachment.
    #[must_use]
    pub const fn is_archive(self) -> bool {
        matches!(self, Self::TarGz | Self::Zip)
    }
}

impl std::fmt::Display for MediaFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MediaFormat {
    type Err = UnknownMediaFormat;

    /// Parses a canonical token, extension alias, or MIME type.
    ///
    /// # Errors
    /// Returns [`UnknownMediaFormat`] when the input names no known format.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let lowered = value.trim().to_ascii_lowercase();
        let alias = match lowered.as_str() {
            "jpg" => Some(Self::Jpeg),
            "txt" => Some(Self::Text),
            "md" => Some(Self::Markdown),
            "htm" => Some(Self::Html),
            "tgz" | "tar.gz" | "application/x-gzip" | "application/x-tar+gzip" => Some(Self::TarGz),
            "application/x-zip-compressed" => Some(Self::Zip),
            _ => None,
        };
        alias
            .or_else(|| {
                Self::ALL
                    .into_iter()
                    .find(|format| format.as_str() == lowered || format.mime() == lowered)
            })
            .ok_or_else(|| UnknownMediaFormat(value.to_owned()))
    }
}

impl Serialize for MediaFormat {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MediaFormat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for MediaFormat {
    fn schema_name() -> String {
        "MediaFormat".to_owned()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject};

        SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(
                Self::ALL
                    .into_iter()
                    .map(|format| serde_json::Value::from(format.as_str()))
                    .collect(),
            ),
            ..SchemaObject::default()
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tokens, extension aliases, and MIME types parse case-insensitively to
    /// one canonical format.
    ///
    /// # Panics
    /// Panics when an accepted spelling does not parse to its format.
    #[test]
    fn media_format_parses_tokens_aliases_and_mime_types() {
        for (input, expected) in [
            ("pdf", MediaFormat::Pdf),
            ("PDF", MediaFormat::Pdf),
            ("application/pdf", MediaFormat::Pdf),
            ("jpg", MediaFormat::Jpeg),
            ("image/jpeg", MediaFormat::Jpeg),
            ("md", MediaFormat::Markdown),
            ("txt", MediaFormat::Text),
            ("tgz", MediaFormat::TarGz),
            ("tar.gz", MediaFormat::TarGz),
            (
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                MediaFormat::Pptx,
            ),
            ("application/vnd.ms-powerpoint", MediaFormat::Ppt),
        ] {
            assert_eq!(
                input.parse::<MediaFormat>().expect(input),
                expected,
                "{input}"
            );
        }
        assert!("bmp".parse::<MediaFormat>().is_err());
    }

    /// Serialization writes canonical tokens and deserialization accepts aliases.
    ///
    /// # Panics
    /// Panics when the wire form is not the canonical short token.
    #[test]
    fn media_format_serializes_canonical_tokens() {
        assert_eq!(
            serde_json::to_value(MediaFormat::Jpeg).expect("serialize"),
            "jpeg"
        );
        assert_eq!(
            serde_json::to_value(MediaFormat::TarGz).expect("serialize"),
            "tar_gz"
        );
        let parsed: MediaFormat = serde_json::from_str("\"text/markdown\"").expect("deserialize");
        assert_eq!(parsed, MediaFormat::Markdown);
        for format in MediaFormat::ALL {
            assert_eq!(
                format.as_str().parse::<MediaFormat>().expect("round trip"),
                format
            );
            assert_eq!(
                format
                    .mime()
                    .parse::<MediaFormat>()
                    .expect("mime round trip"),
                format
            );
        }
    }

    /// Legacy Office formats stay distinct from OOXML and are not extractable.
    ///
    /// # Panics
    /// Panics when a legacy format collapses into its OOXML successor.
    #[test]
    fn media_format_keeps_legacy_office_distinct() {
        for (legacy, ooxml) in [
            (MediaFormat::Ppt, MediaFormat::Pptx),
            (MediaFormat::Doc, MediaFormat::Docx),
            (MediaFormat::Xls, MediaFormat::Xlsx),
        ] {
            assert_ne!(
                legacy.as_str().parse::<MediaFormat>().expect("legacy"),
                ooxml
            );
            assert!(!legacy.is_extractable());
            assert!(ooxml.is_extractable());
        }
    }

    /// Only provider-native images, PDF, and plain text bind as model media;
    /// archives never do.
    ///
    /// # Panics
    /// Panics when a format reports the wrong native media kind.
    #[test]
    fn media_format_native_kinds() {
        assert_eq!(MediaFormat::Png.native_kind(), Some(MediaKind::Image));
        assert_eq!(MediaFormat::Pdf.native_kind(), Some(MediaKind::Document));
        assert_eq!(MediaFormat::Text.native_kind(), Some(MediaKind::Document));
        assert_eq!(MediaFormat::Docx.native_kind(), None);
        assert_eq!(MediaFormat::Zip.native_kind(), None);
        assert!(MediaFormat::Zip.is_archive());
        assert!(!MediaFormat::Zip.is_extractable());
    }
}
