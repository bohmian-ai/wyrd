//! Authentication request and response contracts.

mod admin;
mod card_scope;
mod cli_handoff;
mod human_connection;
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
pub use cli_handoff::{
    CliHandoff, CliHandoffClaim, CliHandoffProof, CliLogin, CreateCliHandoff, RevokeRefreshToken,
};
pub use human_connection::{
    ConnectionActivate, ConnectionInput, ConnectionTestRequest, ConnectionTestResponse,
    HUMAN_SUBJECT_CLAIM, HumanClientAuth, HumanConnectionState, HumanConnectionView,
    HumanConnectionsResponse, refuse_human_trusted_issuer,
};
pub use issue_key::{IssueKeyRequest, IssueKeyResponse};
pub use oidc::{
    AbsoluteUrl, BeginLogin, BeginLoginResponse, CallbackQuery, ConnectionTester, IssuerUrl,
    LoginInitResponse, LoginInitiation, Sha256Hex, Sha256HexError, UrlParseError,
};
pub use permission_scope::{
    BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope, GatewayAccess, PermissionScope,
    PermissionScopeError,
};
pub use platform_identity::{
    ConfigurePlatformOidcRequest, IssuePlatformCredentialRequest, PlatformCallbackRequest,
    PlatformClientAuth, PlatformLoginRequest, PlatformOidcConnectionView,
    PlatformPrincipalListResponse, PlatformPrincipalSummary, RegisterPlatformAdminRequest,
    RegisterPlatformAdminResponse, SetPlatformPrincipalStatusRequest,
};
pub use principal_id::{GATEWAY_CAPTURE_PRINCIPAL, PLATFORM_AUDIT_PRINCIPAL, PrincipalId};
pub use principal_kind::PrincipalKindTag;
pub use revoke::{REASON_MAX_BYTES, RevokePrincipalRequest};
pub use secret_bearer::SecretBearer;
pub use tenant_admin::{
    CreateTenantRequest, CreateTenantResponse, PlatformTokenRequest, PlatformTokenResponse,
    ProvisionedTenant, ProvisionedTenantAdmin, RecoverTenantAdminRequest, SetTenantStatusRequest,
    TenantListResponse,
};
pub use tenant_principals::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    CredentialMetadata, CredentialRevoked, IssuedCredential, ListCredentialsArgs,
    RevokeCredentialArgs,
};
pub use token::{ExchangeTokenType, TokenAudience, TokenRequest, TokenResponse, TokenType};
