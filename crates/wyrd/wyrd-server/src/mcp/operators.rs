//! Operator connection management over MCP.
//!
//! Typed projections of the same [`OperatorConnectionControl`] operations
//! HTTP serves. Reads are advertised to every caller; the three writes appear
//! only to a caller holding `operators:write`, and the operation authorizes
//! and audits that permission again when a write is named directly. Write
//! arguments are decoded by the HTTP body decoder, which reports only a
//! position, so no secret value is ever echoed.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map as JsonMap, Value as JsonValue};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::OperatorConnectionId;
use wyrd_spec::operator_connection::{
    CreateOperatorConnectionRequest, OperatorConnectionView, UpdateOperatorConnectionRequest,
};

use super::principals::{parse_args, tool};
use super::{WyrdMcpHandler, structured};
use crate::components::auth::Caller;
use crate::components::operators::routes::decode_body;
use crate::components::operators::service::OperatorConnectionControl;

/// Wire name of the connection listing.
pub(super) const LIST: &str = "operator_connections.list";

/// Wire name of the single-connection read.
pub(super) const GET: &str = "operator_connections.get";

/// Wire name of connection creation.
pub(super) const CREATE: &str = "operator_connections.create";

/// Wire name of the connection patch.
pub(super) const UPDATE: &str = "operator_connections.update";

/// Wire name of connection disable.
pub(super) const DISABLE: &str = "operator_connections.disable";

/// Arguments of the listing: none.
#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListArgs {}

/// Arguments addressing one connection.
#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    /// Connection ID from `operator_connections.list`.
    connection_id: OperatorConnectionId,
}

/// Advertised arguments of `operator_connections.update`: the connection ID
/// plus the HTTP PATCH body under `request`.
#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    /// Connection to patch.
    connection_id: OperatorConnectionId,
    /// Provider-tagged patch; omitted fields are preserved.
    request: UpdateOperatorConnectionRequest,
}

/// Tool listing shape: a JSON array of redacted views.
#[derive(Debug, Serialize, JsonSchema)]
struct ListAnswer {
    /// The tenant's connections, redacted.
    connections: Vec<OperatorConnectionView>,
}

/// The read tools every authenticated caller may be shown.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    vec![
        tool::<ListArgs, ListAnswer>(
            LIST,
            "List Operator connections",
            "List this tenant's Operator provider connections as redacted metadata: ID, \
             provider, name, status, and nonsecret config. Secrets are never returned. Requires \
             operators:read.",
            true,
        ),
        tool::<IdArgs, OperatorConnectionView>(
            GET,
            "Read an Operator connection",
            "Read one Operator connection's redacted metadata by ID. Requires operators:read.",
            true,
        ),
    ]
}

/// The write tools shown only to a caller holding `operators:write`.
#[must_use]
pub(super) fn write_descriptors() -> Vec<Tool> {
    vec![
        tool::<CreateOperatorConnectionRequest, OperatorConnectionView>(
            CREATE,
            "Create an Operator connection",
            "Create one provider-tagged Slack, PagerDuty, or HTTP connection. The secret is \
             encrypted at rest and never returned. Requires operators:write.",
            false,
        ),
        tool::<UpdateArgs, OperatorConnectionView>(
            UPDATE,
            "Update an Operator connection",
            "Patch one connection: omitted fields are preserved, a supplied secret replaces the \
             stored one, and status disables or re-enables it. Provider and name never change. \
             Requires operators:write.",
            false,
        ),
        tool::<IdArgs, OperatorConnectionView>(
            DISABLE,
            "Disable an Operator connection",
            "Disable one connection; it is kept and can be re-enabled with \
             operator_connections.update. Requires operators:write.",
            false,
        ),
    ]
}

/// Whether this caller's token carries `operators:write`.
pub(super) fn may_manage(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::operators_write())
}

/// Decode secret-bearing tool arguments without echoing any value.
///
/// # Errors
/// Returns [`WyrdError::OperatorConnectionInvalid`] naming only a position.
fn decode_secret_args<T: DeserializeOwned>(
    arguments: Option<JsonMap<String, JsonValue>>,
) -> Result<T, WyrdError> {
    let arguments = JsonValue::Object(arguments.unwrap_or_default());
    decode_body(arguments.to_string().as_bytes())
}

impl WyrdMcpHandler {
    /// Dispatch one Operator connection tool by wire name.
    ///
    /// `name` is one of this module's five wire names; the caller routes only
    /// those here, so the final arm is [`DISABLE`].
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid or the owning
    /// [`OperatorConnectionControl`] operation refuses or fails.
    pub(super) async fn mcp_operator_connections(
        &self,
        name: &str,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let control = OperatorConnectionControl::new(&self.state);
        match name {
            LIST => {
                let _: ListArgs = parse_args(Some(arguments.unwrap_or_default()), LIST)?;
                structured(&ListAnswer {
                    connections: control.list(&caller).await?,
                })
            }
            GET => {
                let args: IdArgs = parse_args(arguments, GET)?;
                structured(&control.get(&caller, args.connection_id).await?)
            }
            CREATE => structured(
                &control
                    .create(&caller, decode_secret_args(arguments)?)
                    .await?,
            ),
            UPDATE => {
                let args: UpdateArgs = decode_secret_args(arguments)?;
                structured(
                    &control
                        .update(&caller, args.connection_id, args.request)
                        .await?,
                )
            }
            _ => {
                let args: IdArgs = parse_args(arguments, DISABLE)?;
                structured(&control.disable(&caller, args.connection_id).await?)
            }
        }
    }
}
