//! Model autodetection placeholders.

use crate::error::{CardPyResult, WyrdPyError};

/// Report that model autodetection is intentionally unavailable in this stage.
///
/// # Errors
/// Always returns a validation error because autodetection is implemented by a
/// later `ModelCard` stage.
pub fn autodetect_not_available() -> CardPyResult<()> {
    Err(WyrdPyError::validation(
        "model interface autodetection is not available in this stage",
    ))
}
