use serde::{Deserialize, Serialize};

/// A reference to a media artifact stored in object storage.
///
/// Carries the URI and optional MIME type — no inline binary data.
/// The server stores URIs; callers fetch content separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct MediaRef {
    /// Object-storage URI pointing to the media artifact.
    pub uri: String,
    /// IANA media type (e.g. `"image/png"`, `"video/mp4"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}
