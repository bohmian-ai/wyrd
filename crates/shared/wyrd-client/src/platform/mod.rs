//! Platform control-plane administration from a client.
//!
//! The other client surfaces speak to a tenant. This one speaks to the
//! deployment: creating tenants, recovering a tenant's administration, and
//! managing who may administer the platform itself.
//!
//! It is a separate handle rather than another method on the tenant client
//! because the two identities are separate: a platform session names a
//! principal with no tenant, and a tenant token names one with no platform
//! authority. Both travel on `X-Wyrd-Access-Token`, the one header every Wyrd
//! plane authenticates on — the header is not what keeps them apart, since any
//! client can set any header. What keeps them apart is the scope marker the
//! server requires of a platform session and the extractor each route declares:
//! a tenant token presented to a platform route carries the wrong scope and is
//! refused. Holding the two sessions on two handles means a caller does not
//! reach for a platform route with a tenant credential in the first place.

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
