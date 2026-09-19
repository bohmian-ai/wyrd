//! Platform control-plane administration from a client.
//!
//! The other client surfaces speak to a tenant. This one speaks to the
//! deployment: creating tenants, recovering a tenant's administration, and
//! managing who may administer the platform itself.
//!
//! It is a separate handle rather than another method on the tenant client
//! because the two planes are separate all the way down to the wire. A platform
//! session arrives on `Authorization: Bearer`; a tenant access token arrives on
//! `X-Wyrd-Access-Token`. Neither header is read by the other plane, so a client
//! holding a tenant credential cannot reach a platform route by accident or by
//! construction — which is the property the server enforces and this surface
//! must not quietly paper over.

mod handle;

pub use handle::{Platform, PlatformSession};

// The wire contract this handle speaks, re-exported so a caller reaches one
// module for the capability and its types.
pub use wyrd_spec::auth::{
    ConfigurePlatformOidcRequest, CreateTenantRequest, CreateTenantResponse, PlatformClientAuth,
    PlatformOidcConnectionView, PlatformPrincipalListResponse, PlatformPrincipalSummary,
    ProvisionedTenant, ProvisionedTenantAdmin, RecoverTenantAdminRequest,
    RegisterPlatformAdminRequest, RegisterPlatformAdminResponse, SetPlatformPrincipalStatusRequest,
};
pub use wyrd_spec::error::WyrdError;
