//! Local saved-Card path resolution shared by Data and Model holders.

use std::path::{Path, PathBuf};

use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;

/// File name a holder's `save` writes inside its Card directory.
const SAVED_CARD_FILE: &str = "card.json";

/// Resolve the Card envelope file for a saved Card directory or a Card file.
///
/// A directory resolves to the `card.json` that holder `save` writes beside
/// the materialized artifacts; any other path is returned unchanged.
#[must_use]
fn card_file(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(SAVED_CARD_FILE)
    } else {
        path.to_path_buf()
    }
}

/// Read one complete Card envelope from a saved Card directory or a JSON or
/// YAML Card file.
///
/// JSON is parsed by the YAML reader because JSON is a YAML subset, so one
/// decoder serves both formats. The envelope is not kind-checked here; each
/// holder's `from_card` owns its kind and spec validation.
///
/// # Errors
/// Returns `WYRD_LOADER_400_IO` when the file cannot be read and
/// `WYRD_LOADER_400_INVALID_ENVELOPE` when its bytes are not a Card envelope.
pub(crate) fn read_card(path: &Path) -> Result<Card, WyrdError> {
    let file = card_file(path);
    let bytes = std::fs::read(&file).map_err(|error| WyrdError::LoaderIo {
        message: format!("failed to read Card file {}: {error}", file.display()),
        details: serde_json::json!({ "path": file.display().to_string() }),
    })?;
    serde_yaml::from_slice(&bytes).map_err(|error| WyrdError::LoaderInvalidEnvelope {
        message: format!("{} is not a Card envelope: {error}", file.display()),
        details: serde_json::json!({ "path": file.display().to_string() }),
    })
}
