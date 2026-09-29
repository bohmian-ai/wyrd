//! Images and Audio request shape.
//!
//! A media call names its `OpenAI` route and carries any uploaded files beside
//! the JSON or multipart text members of its body. Validation here is
//! adapter-independent: the route must belong to the requested operation, a
//! media call never streams, and every file must be a field the route accepts
//! in the number it accepts. Provider-specific support is decided by
//! [`super::prepare`].

use serde_json::Value;
use skald_providers::{OpenAiMediaRoute, UploadFile};
use wyrd_spec::gateway::GatewayOperation;

use super::UnsupportedRequest;

/// Route and uploaded files of one Images or Audio call.
#[derive(Debug, Clone)]
pub struct MediaRequest {
    /// `OpenAI` media route the call targets.
    pub route: OpenAiMediaRoute,
    /// Uploaded files in submission order; empty for JSON routes.
    pub files: Vec<UploadFile>,
}

/// Largest number of images one edit accepts, the `OpenAI` maximum.
const MAX_EDIT_IMAGES: usize = 16;

/// Operation family a media `route` belongs to.
const fn family(route: OpenAiMediaRoute) -> GatewayOperation {
    match route {
        OpenAiMediaRoute::ImageGenerations
        | OpenAiMediaRoute::ImageEdits
        | OpenAiMediaRoute::ImageVariations => GatewayOperation::Images,
        OpenAiMediaRoute::AudioSpeech
        | OpenAiMediaRoute::AudioTranscriptions
        | OpenAiMediaRoute::AudioTranslations => GatewayOperation::Audio,
    }
}

/// Checks that `media` matches `operation` and that `body` and the files fit
/// the route.
///
/// Images and Audio require a route of their own family; every other operation
/// must carry none.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `operation` when the route is missing,
/// present for a non-media operation, or of another family; `stream` when the
/// body carries a stream member; a file's field when the route does not take
/// it; and the route's file field when it carries too few or too many files.
pub(crate) fn validate(
    operation: GatewayOperation,
    media: Option<&MediaRequest>,
    body: &Value,
) -> Result<(), UnsupportedRequest> {
    let media_operation = matches!(
        operation,
        GatewayOperation::Images | GatewayOperation::Audio
    );
    let media = match media {
        None if !media_operation => return Ok(()),
        Some(media) if media_operation && family(media.route) == operation => media,
        _ => {
            return Err(UnsupportedRequest::new(
                "operation",
                format!("{operation:?} requires a route of its own operation family"),
            ));
        }
    };
    if body.get("stream").is_some() {
        return Err(UnsupportedRequest::new(
            "stream",
            "media operations do not stream through the gateway",
        ));
    }
    let rules: &[(&[&str], usize, usize)] = match media.route {
        OpenAiMediaRoute::ImageGenerations | OpenAiMediaRoute::AudioSpeech => &[],
        OpenAiMediaRoute::ImageEdits => &[
            (&["image", "image[]"], 1, MAX_EDIT_IMAGES),
            (&["mask"], 0, 1),
        ],
        OpenAiMediaRoute::ImageVariations => &[(&["image"], 1, 1)],
        OpenAiMediaRoute::AudioTranscriptions | OpenAiMediaRoute::AudioTranslations => {
            &[(&["file"], 1, 1)]
        }
    };
    if let Some(file) = media.files.iter().find(|file| {
        !rules
            .iter()
            .any(|(fields, ..)| fields.contains(&file.field.as_str()))
    }) {
        return Err(UnsupportedRequest::new(
            file.field.clone(),
            "is not a file field of this route",
        ));
    }
    for (fields, min, max) in rules {
        let count = media
            .files
            .iter()
            .filter(|file| fields.contains(&file.field.as_str()))
            .count();
        if !(*min..=*max).contains(&count) {
            return Err(UnsupportedRequest::new(
                fields[0],
                format!("must carry between {min} and {max} files"),
            ));
        }
    }
    Ok(())
}
