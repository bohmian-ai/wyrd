use serde::{Deserialize, Serialize};

use crate::ids::MediaBindingId;

pub use skald_spec::MediaKind;

/// A reference to a media artifact stored in object storage.
///
/// Carries the URI and optional MIME type — no inline binary data. The client
/// queues only this descriptor; the server resolves and reads the authorized
/// object at judge execution, so an emitting process never uploads bytes
/// through the observation path.
///
/// `id` names an existing `${media:id}` binding slot in the resolved judge
/// Prompt, which is what lets the scoring chain attach the real bytes to the
/// exact placeholder the Prompt declares. `kind` selects the supported Skald
/// media kind explicitly rather than inferring it from the URI's extension,
/// because the provider content block depends on it and a filename is not a
/// contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaRef {
    /// `${media:id}` binding slot this artifact fills in the judge Prompt.
    pub id: MediaBindingId,
    /// Supported Skald media kind for the provider content block.
    pub kind: MediaKind,
    /// Object-storage URI pointing to the media artifact.
    pub uri: String,
    /// IANA media type (e.g. `"image/png"`, `"video/mp4"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}
