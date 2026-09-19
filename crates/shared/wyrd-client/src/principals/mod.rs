//! Tenant principal and credential administration from a client.
//!
//! The shared implementation every first-class SDK projects. It owns no durable
//! state: the server decides who a principal is, what it may do, and whether a
//! credential is live, and this only carries typed requests to it and typed
//! answers back.
//!
//! One rule shapes the whole surface. A credential's plaintext crosses here
//! exactly once, in the response that created it, and is never written to disk,
//! cached, or logged by the client. Everything afterwards works from the
//! credential's id and its non-secret metadata.

mod handle;

pub use handle::Principals;

// The wire contract this handle speaks, re-exported so an SDK user reaches one
// module for the capability and its types rather than depending on `wyrd-spec`
// directly.
pub use wyrd_spec::auth::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    CredentialMetadata, IssuedCredential, PrincipalId,
};
pub use wyrd_spec::error::WyrdError;
