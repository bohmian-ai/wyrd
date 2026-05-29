//! Prompt reference type.

use serde::{Deserialize, Serialize};

use crate::card::prompt::PromptSpec;
use crate::reference::CardRef;

/// Reference to a registered or inline PromptCard spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PromptRef {
    /// Reference to a registered Prompt Card.
    Card(CardRef),
    /// Inline prompt spec.
    Inline(Box<PromptSpec>),
}
