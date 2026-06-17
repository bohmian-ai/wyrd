//! Shared authz-check mechanism primitives.

#![deny(missing_docs)]

pub mod context;
pub mod hook;
pub mod request;

pub use context::{AuthzCheckContext, AuthzCheckContextError, is_delegated_token};
pub use hook::{DenyAllPolicyHook, PolicyHook, StubAllowPolicyHook};
pub use request::{AuthzCheckRequest, AuthzCheckRequestError};
