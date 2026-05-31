use serde::{Deserialize, Serialize};

use crate::wire::google_generate::GoogleGenerateContentRequest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct VertexGenerateContentRequest(pub GoogleGenerateContentRequest);
