//! Test-support MCP capability used to prove `/mcp` connectivity.
//!
//! This tool exists so a real MCP client journey can observe what the Wyrd edge
//! bound for a request — tenant, principal, delegation chain, roles, request ID
//! — and can drive request- and process-level cancellation deterministically,
//! before any production Bifrost tool exists. It is compiled only under the
//! `test-support` feature and, even then, joins the catalog only when a test
//! server explicitly opted in through
//! `WyrdTestServerBuilder::with_mcp_context_probe_for_test`.
//!
//! It is not an extension point: there is no registration hook, and the tool
//! name is a fixed constant.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use rmcp::RoleServer;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::RequestContext;
use serde::Deserialize;
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::components::auth::{AuthenticatedPrincipal, Caller};
use crate::query::service::authorize_audited;
use crate::state::AppState;

/// Wire name of the probe capability.
pub const TOOL_NAME: &str = "wyrd.mcp.test.context";

/// Audited operation name recorded for every probe invocation.
const OPERATION: &str = "wyrd.mcp.test.context";

/// Audited resource name recorded for every probe invocation.
const RESOURCE: &str = "mcp.test.context";

/// Arguments the probe accepts.
///
/// Deliberately carries no identity: tenant, principal, delegation, roles, and
/// execution path all come from the verified request context. Unknown keys are
/// ignored so a journey can prove that client-supplied identity claims have no
/// effect on what the probe reports.
#[derive(Debug, Default, Deserialize)]
struct ProbeArguments {
    /// When present, the probe parks on the matching [`ProbeLatches`] until it
    /// observes cancellation and the test releases cleanup.
    #[serde(default)]
    hold_id: Option<String>,
}

/// Deterministic rendezvous between a parked probe invocation and the journey
/// driving it.
///
/// Each latch is a zero-permit semaphore: the side that reaches a milestone
/// adds a permit, and the side waiting for it acquires one. That gives exact
/// happens-before ordering without sleeps or polling.
#[derive(Debug)]
pub struct ProbeLatches {
    /// Released by the probe once its invocation has started.
    entered: tokio::sync::Semaphore,
    /// Released by the probe once it has observed its request cancellation.
    cancelled: tokio::sync::Semaphore,
    /// Released by the test to let a cancelled invocation finish cleanup.
    cleanup_gate: tokio::sync::Semaphore,
    /// Released by the probe once cleanup has finished and it is about to drop
    /// its task-tracker token.
    cleanup_done: tokio::sync::Semaphore,
}

impl ProbeLatches {
    /// Construct a fully-closed set of latches.
    fn new() -> Self {
        Self {
            entered: tokio::sync::Semaphore::new(0),
            cancelled: tokio::sync::Semaphore::new(0),
            cleanup_gate: tokio::sync::Semaphore::new(0),
            cleanup_done: tokio::sync::Semaphore::new(0),
        }
    }

    /// Wait until the probe invocation has started on the server.
    ///
    /// # Panics
    ///
    /// Panics if the latch semaphore was closed, which never happens for a
    /// registry-owned latch set.
    pub async fn wait_entered(&self) {
        acquire(&self.entered).await;
    }

    /// Wait until the probe has observed its request context cancellation.
    ///
    /// # Panics
    ///
    /// Panics if the latch semaphore was closed.
    pub async fn wait_cancelled(&self) {
        acquire(&self.cancelled).await;
    }

    /// Let a cancelled probe invocation run its cleanup and release its token.
    pub fn release_cleanup(&self) {
        self.cleanup_gate.add_permits(1);
    }

    /// Wait until the probe has finished cleanup.
    ///
    /// # Panics
    ///
    /// Panics if the latch semaphore was closed.
    pub async fn wait_cleanup_done(&self) {
        acquire(&self.cleanup_done).await;
    }
}

/// Acquire and immediately consume one permit.
async fn acquire(semaphore: &tokio::sync::Semaphore) {
    semaphore
        .acquire()
        .await
        .expect("probe latch semaphores are never closed")
        .forget();
}

/// Process-global latch registry keyed by the caller-chosen `hold_id`.
///
/// The registry is what lets an in-process journey share a rendezvous with a
/// probe invocation that reaches the server over a real HTTP connection, where
/// no value can be handed across directly.
fn registry() -> &'static Mutex<HashMap<String, Arc<ProbeLatches>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<ProbeLatches>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get, creating if needed, the latch set for `hold_id`.
///
/// Journeys call this before issuing the request that carries the same
/// `hold_id`, so the latches already exist when the probe looks them up.
///
/// # Panics
///
/// Panics if the registry mutex was poisoned by a panicking test.
#[must_use]
pub fn latches(hold_id: &str) -> Arc<ProbeLatches> {
    let mut registry = registry().lock().expect("probe latch registry is healthy");
    Arc::clone(
        registry
            .entry(hold_id.to_owned())
            .or_insert_with(|| Arc::new(ProbeLatches::new())),
    )
}

/// The probe's advertised tool definition.
///
/// # Panics
///
/// Panics if the static input schema is not a JSON object, which it is.
#[must_use]
pub fn descriptor() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "hold_id": {
                "type": "string",
                "description": "Rendezvous key that parks this call until the test cancels and releases it."
            }
        }
    });
    let serde_json::Value::Object(schema) = schema else {
        unreachable!("probe input schema is a JSON object")
    };
    Tool::new(
        TOOL_NAME,
        "Test-only capability that reports the server-verified request context and can park \
         until cancelled.",
        Arc::new(schema),
    )
    .with_title("Wyrd MCP context probe")
}

/// Authorize, run, and report one probe invocation.
///
/// Authorization runs first and always: the probe requires
/// [`Permission::bifrost_query_read`] through the same
/// [`authorize_audited`] path every Bifrost read uses, so an under-scoped
/// principal is denied and audited before any context is disclosed.
///
/// The pending path takes an RAII token from the process MCP
/// [`tokio_util::task::TaskTracker`] and holds it across cancellation cleanup
/// and result construction, so a process shutdown that stops transport
/// admission still has to wait for this work to settle.
///
/// # Errors
///
/// Returns the permission denial from [`authorize_audited`], or
/// [`WyrdError::Validation`] when the arguments do not match the input schema.
///
/// # Cancellation
///
/// Only the probe's own parked work races `context.ct`. Either an rmcp request
/// cancellation or the server-configured [`AppState::shutdown_token`] reaches
/// that same branch; both then run the identical cleanup.
pub async fn invoke(
    state: &AppState,
    caller: Caller,
    principal: &AuthenticatedPrincipal,
    request: CallToolRequestParams,
    context: RequestContext<RoleServer>,
) -> Result<CallToolResult, WyrdError> {
    let request_id = caller.request_id.clone();
    let arguments: ProbeArguments = match request.arguments {
        Some(map) => serde_json::from_value(serde_json::Value::Object(map)).map_err(|error| {
            WyrdError::Validation {
                message: format!("invalid {TOOL_NAME} arguments: {error}"),
                details: serde_json::json!({ "tool": TOOL_NAME }),
            }
        })?,
        None => ProbeArguments::default(),
    };

    authorize_audited(
        state.clone(),
        caller.clone(),
        Permission::bifrost_query_read(),
        OPERATION,
        RESOURCE,
    )
    .await?;

    // Held for the whole invocation, including cancellation cleanup and result
    // construction, so process shutdown cannot report a clean drain while this
    // work is still settling.
    let _token = state.mcp_tasks.token();
    let cancelled = match arguments.hold_id.as_deref() {
        None => false,
        Some(hold_id) => {
            let latches = latches(hold_id);
            latches.entered.add_permits(1);
            context.ct.cancelled().await;
            latches.cancelled.add_permits(1);
            acquire(&latches.cleanup_gate).await;
            latches.cleanup_done.add_permits(1);
            true
        }
    };

    Ok(CallToolResult::structured(trusted_context(
        &caller,
        principal,
        &request_id,
        cancelled,
    )))
}

/// Project only server-verified request context into the probe's result.
fn trusted_context(
    caller: &Caller,
    principal: &AuthenticatedPrincipal,
    request_id: &RequestId,
    cancelled: bool,
) -> serde_json::Value {
    serde_json::json!({
        "tenant_id": caller.data_tenant_id.to_string(),
        "principal_id": caller.principal.id.to_string(),
        "principal_kind": format!("{:?}", caller.principal.kind),
        "roles": caller
            .principal
            .roles
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "delegation_chain": principal
            .delegation_chain()
            .iter()
            .map(|step| step.principal.id.to_string())
            .collect::<Vec<_>>(),
        "request_id": request_id.as_str(),
        "cancelled": cancelled,
    })
}
