//! Authentication request and response contracts.

mod admin;
mod card_scope;
mod issue_key;
mod oidc;
mod permission_scope;
mod platform_identity;
mod principal_id;
mod principal_kind;
mod revoke;
mod secret_bearer;
mod tenant_admin;
mod tenant_principals;
mod token;

pub use admin::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    IssuerTokenPolicy, TrustedIssuerView, WorkloadBindingView,
};
pub use card_scope::CardScope;
pub use issue_key::{IssueKeyRequest, IssueKeyResponse};
pub use oidc::{AbsoluteUrl, CallbackQuery, IssuerUrl, LoginInitResponse, UrlParseError};
pub use permission_scope::{
    BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope, PermissionScope,
    PermissionScopeError,
};
pub use platform_identity::{
    ConfigurePlatformOidcRequest, IssuePlatformCredentialRequest, PlatformCallbackRequest,
    PlatformClientAuth, PlatformLoginRequest, PlatformOidcConnectionView,
    PlatformPrincipalListResponse, PlatformPrincipalSummary, RegisterPlatformAdminRequest,
    RegisterPlatformAdminResponse, SetPlatformPrincipalStatusRequest,
};
pub use principal_id::{PLATFORM_AUDIT_PRINCIPAL, PrincipalId};
pub use principal_kind::PrincipalKindTag;
pub use revoke::{RevokePrincipalRequest, RevokePrincipalResponse};
pub use secret_bearer::SecretBearer;
pub use tenant_admin::{
    CreateTenantRequest, CreateTenantResponse, PlatformTokenRequest, PlatformTokenResponse,
    ProvisionedTenant, ProvisionedTenantAdmin, RecoverTenantAdminRequest,
};
pub use tenant_principals::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    CredentialMetadata, IssuedCredential,
};
pub use token::{RequestedSubject, SubjectTokenType, TokenRequest, TokenResponse, TokenType};
