//! Typed side-effect templates fired by Trigger cards.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::Ref;

/// A server-owned side-effect template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct OperatorSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The single action performed when this operator fires.
    pub action: OperatorAction,
    /// Optional execution limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<OperatorBudget>,
}

/// The closed set of actions an Operator can perform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperatorAction {
    /// Dispatch a registered Workflow.
    Workflow {
        /// Workflow to dispatch with the trigger context.
        workflow_ref: Ref,
    },
    /// Send a typed notification.
    Notify {
        /// Notification destination and payload.
        channel: NotifyChannel,
    },
    /// Perform an HTTP request.
    Http {
        /// HTTP method.
        method: HttpMethod,
        /// URL template.
        url: String,
        /// Optional request headers.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        /// Optional structured JSON body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<serde_json::Value>,
        /// Optional server-side authentication binding.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<HttpAuth>,
        /// Optional request timeout.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u32>,
        /// Optional expected response status.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_status: Option<u16>,
    },
}

/// Typed notification destinations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotifyChannel {
    /// PagerDuty event payload.
    PagerDuty {
        /// PagerDuty severity.
        severity: String,
        /// Human-readable event summary.
        summary: String,
        /// Optional de-duplication key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dedup_key: Option<String>,
    },
    /// Slack message payload.
    Slack {
        /// Message text.
        text: String,
    },
}

/// HTTP methods supported by an HTTP Operator action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    /// GET.
    Get,
    /// POST.
    Post,
    /// PUT.
    Put,
    /// PATCH.
    Patch,
    /// DELETE.
    Delete,
}

/// Server-side authentication sources for an HTTP action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "scheme", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpAuth {
    /// No authentication.
    None,
    /// Bearer token loaded from an environment variable.
    Bearer {
        /// Environment variable name.
        env: String,
    },
    /// `user:password` loaded from an environment variable.
    Basic {
        /// Environment variable name.
        env: String,
    },
    /// Custom header value loaded from an environment variable.
    Header {
        /// Header name.
        name: String,
        /// Environment variable name.
        env: String,
    },
}

/// Operator execution limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct OperatorBudget {
    /// Maximum wall-clock runtime in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_seconds: Option<u32>,
    /// Maximum tool calls allowed during invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
}
