//! Tenant-independent workflow guidance advertised through MCP.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Closed set of workflows an authenticated agent can ask Wyrd to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuideTopic {
    /// Discovery and entry points across the shipped tool catalog.
    Overview,
    /// Card selection and exact relationship navigation.
    Cards,
    /// Binding, run, and judgment interpretation.
    Verification,
    /// Bounded SQL over discovered Bifrost tables.
    BifrostSql,
}

/// Closed input of the read-only `wyrd.guide` tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideRequest {
    /// Workflow to explain; never a tenant or resource selector.
    pub topic: GuideTopic,
}

/// Bounded guidance containing no caller or tenant data.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideResponse {
    /// Requested workflow.
    pub topic: GuideTopic,
    /// Ordered instructions, with permission and evidence limits stated in place.
    pub steps: Vec<String>,
    /// Tool calls with explicit identifier placeholders where caller data is needed.
    pub examples: Vec<GuideExample>,
}

/// One illustrative call; its arguments are never executed by the guide.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideExample {
    /// Exact wire name from the shipped MCP catalog.
    pub tool: String,
    /// What evidence or context the call supplies.
    pub purpose: String,
    /// Argument object accepted by the named tool after replacing placeholders.
    pub arguments: Map<String, Value>,
}
