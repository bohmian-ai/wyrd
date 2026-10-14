//! Jev `systemone` request preparation.
//!
//! A native `POST /v1/systemone` body decodes into the closed, typed Skald
//! [`JevRequest`], is bound-checked, and takes the deployment's native model.
//! Jev speaks only its own protocol: no other dialect is projected onto it,
//! and its typed answer returns to the caller as Jev's own response.

use serde::Deserialize;
use serde_json::Value;
use skald_spec::wire::jev::JevRequest;

use super::UnsupportedRequest;

/// Decodes and validates a native `systemone` `body`, rewriting its model to
/// the deployment's native `model`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it is not a Jev request
/// (unknown members or question forms included), or the member that breaks a
/// documented Jev bound.
pub(super) fn request(body: &Value, model: &str) -> Result<JevRequest, UnsupportedRequest> {
    let mut request = JevRequest::deserialize(body).map_err(|error| {
        UnsupportedRequest::new("body", format!("is not a valid Jev request: {error}"))
    })?;
    request
        .validate()
        .map_err(|error| UnsupportedRequest::new(error.field, error.reason))?;
    model.clone_into(&mut request.model);
    Ok(request)
}
