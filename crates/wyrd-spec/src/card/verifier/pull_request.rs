//! The `extractor` field of an `extract` action.
//!
//! `extractor: pull_request` makes an `extract` step read the pinned source
//! of the pull request a queued `pull_request` execution was admitted for,
//! instead of a media slot. Only the field an author writes is spec; the
//! extraction's output shape is owned by the server's extractor.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The extractor an `extract` action runs instead of reading a media slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Extractor {
    /// The pinned source of the pull request a queued `pull_request`
    /// execution was admitted for.
    PullRequest,
}

/// The input slot naming the engine-owned original source of a successful
/// `pull_request` extraction.
///
/// Every output derived from the extraction reaches this slot, so an Agent or
/// code check can cite the reviewed pull request as
/// `{"slot": "pull_request", "locator": {"json_pointer": "/pr/head/sha"}}`,
/// with the pointer resolved against that execution's retained extraction.
/// A Verifier using the extractor cannot declare an input of this name, so a
/// caller never supplies or replaces it.
pub const PULL_REQUEST_SOURCE: &str = "pull_request";
