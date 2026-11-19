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
