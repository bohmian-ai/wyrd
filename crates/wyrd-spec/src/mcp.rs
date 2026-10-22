//! Tenant-independent workflow guidance advertised through MCP.
//!
//! [`SETUP_PROMPT`] is the one source of the guided setup instructions that
//! `wyrd init`, the MCP `setup` prompt, and the documentation project.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Closed set of workflows an authenticated agent can ask Wyrd to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuideTopic {
    /// Onboarding an unfamiliar coding agent to Declare, Observe, and Verify
    /// for an application it owns, across the CLI, SDKs, docs, and MCP.
    Start,
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

/// MCP prompt name under which the server advertises [`SETUP_PROMPT`].
pub const SETUP_PROMPT_NAME: &str = "setup";

/// Release-independent body of the guided setup prompt.
///
/// The documentation embeds this file directly; executable surfaces project it
/// through [`setup_prompt`], which adds the release heading. It holds no
/// credential, path, or repository task, and asks the reader's agent to
/// resolve the exact SDK release from the installed `wyrd --version`.
pub const SETUP_PROMPT: &str = include_str!("setup-prompt.md");

/// The setup prompt labelled with the `release` that projects it.
///
/// Every surface of one release returns the same bytes for the same
/// `release`, so a version mismatch between a CLI and a server is visible in
/// the heading rather than in a silent content difference.
#[must_use]
pub fn setup_prompt(release: &str) -> String {
    format!("# Wyrd {release} setup\n\n{SETUP_PROMPT}")
}

/// Unit checks of the release-labelled setup prompt projection.
#[cfg(test)]
mod tests {
    use super::setup_prompt;

    /// The projected prompt names its release and carries no credential.
    ///
    /// # Panics
    /// Panics when the heading is missing or a credential-shaped token appears.
    #[test]
    fn setup_prompt_names_its_release_and_holds_no_secret() {
        let prompt = setup_prompt("9.8.7");
        assert!(prompt.starts_with("# Wyrd 9.8.7 setup\n\n"));
        for secret in ["wyrd_sk_", "wyrd_global_", "postgres://", "postgresql://"] {
            assert!(!prompt.contains(secret), "{secret}");
        }
    }
}
