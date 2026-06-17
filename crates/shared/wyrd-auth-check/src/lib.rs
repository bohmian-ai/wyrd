//! Shared authz-check mechanism primitives.

#![deny(missing_docs)]

pub mod context;
pub mod hook;
pub mod request;

pub use context::{AuthzCheckContext, AuthzCheckContextError, is_delegated_token};
#[cfg(feature = "test-helpers")]
pub use hook::{DenyAllPolicyHook, RecordingPolicyHook};
pub use hook::{PolicyHook, StubAllowPolicyHook};
pub use request::{AuthzCheckRequest, AuthzCheckRequestError};
