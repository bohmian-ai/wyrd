//! Runtime singleton and cross-cutting behavior shells.

#![deny(missing_docs)]

use std::sync::OnceLock;

use tokio::runtime::Runtime;

pub mod audit;
pub mod builtin_roles;
pub mod otel;
pub mod permission;
pub mod permission_check;
pub mod principal;
pub mod redaction;
pub mod request_context;
pub mod request_id;

pub use permission::{Action, Permission, PermissionSet, Resource};
pub use permission_check::{PermissionCheck, PermissionDenyReason, PermissionVerdict, RbacCheck};
pub use principal::{
    InvalidRoleName, Principal, PrincipalId, PrincipalKind, PrincipalRef, RoleRef,
};
pub use request_context::{DelegationStep, RequestContext, TraceParent};
pub use wyrd_spec::reference::CardRefScope;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Borrow the process-wide Tokio runtime singleton.
///
/// Initialized on first access with Tokio's multi-thread scheduler. Runtime
/// construction failure is treated as process-start failure rather than a
/// recoverable application error.
///
/// # Panics
/// Panics if Tokio cannot create the runtime.
#[must_use]
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        build_runtime().unwrap_or_else(|error| panic!("failed to build tokio runtime: {error}"))
    })
}

fn build_runtime() -> std::io::Result<Runtime> {
    Runtime::new()
}
