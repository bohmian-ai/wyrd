//! Authentication request and response contracts.

mod admin;
mod attribution;
mod browser;
mod device;
mod human_connection;
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
mod workload;

pub use admin::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    IssuerTokenPolicy, TrustedIssuerView, WorkloadBindingView,
};
pub use attribution::Attribution;
pub use browser::{
    BrowserApiKeyRequest, BrowserCredential, BrowserLoginRequest, BrowserLoginResponse,
    BrowserLogoutRequest, BrowserSessionQuery, BrowserSessionView, BrowserSessionsResponse,
    PasswordChangeRequest, PasswordLoginRequest, PasswordResetRequest, PasswordSignupRequest,
};
pub use device::{DeviceAuthorization, DeviceAuthorizationRequest, TokenRevocationRequest};
pub use human_connection::{
    ConnectionActivate, ConnectionInput, ConnectionTestRequest, ConnectionTestResponse,
    HUMAN_SUBJECT_CLAIM, HumanClientAuth, HumanConnectionState, HumanConnectionView,
    HumanConnectionsResponse, refuse_human_trusted_issuer,
};
pub use oidc::{
    AbsoluteUrl, CallbackQuery, ConnectionTester, IssuerUrl, LoginInitResponse, LoginInitiation,
    ProviderResponse, Sha256Hex, Sha256HexError, UrlParseError,
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
pub use principal_id::{PLATFORM_AUDIT_PRINCIPAL, PrincipalId};
pub use principal_kind::PrincipalKindTag;
pub use revoke::{REASON_MAX_BYTES, RevokePrincipalRequest};
pub use secret_bearer::SecretBearer;
pub use tenant_admin::{
    CreateTenantRequest, CreateTenantResponse, ProvisionedTenant, ProvisionedTenantAdmin,
    RecoverTenantAdminRequest, SetTenantStatusRequest, TenantListResponse,
};
pub use tenant_principals::{
    AccountAuthentication, CREDENTIAL_CURSOR_MAX_BYTES, CREDENTIAL_PAGE_DEFAULT,
    CREDENTIAL_PAGE_MAX, CreateServicePrincipalRequest, CreateServicePrincipalResponse,
    CreateUserCredentialRequest, CreateUserInvitationRequest, CreateUserRequest, CreatedUser,
    CredentialListQuery, CredentialListResponse, CredentialMetadata, CredentialRevoked,
    CurrentUserArgs, CurrentUserProfile, IssuedCredential, IssuedUserInvitation,
    IssuedUserPasswordReset, ListCredentialsArgs, PrincipalPage, PrincipalQuery, PrincipalRoles,
    PrincipalStatus, PrincipalSummary, RevokeCredentialArgs, RoleAssignment, RoleAssignmentChange,
    RoleSource, SetWorkloadRestrictionArgs, USER_CREDENTIAL_MAX_LIFETIME_SECONDS,
    USER_CREDENTIAL_NAME_MAX_CHARS, UserInvitationView, UserPasswordResetView,
};
pub use token::{
    AuthorizationServerMetadata, ExchangeTokenType, OAuthClientId, OAuthErrorCode,
    OAuthErrorResponse, TokenAudience, TokenRequest, TokenResponse, TokenType,
};
pub use workload::{
    CardAccess, DEPLOYMENT_ENVIRONMENT_HEADER, FamilyRestriction, MAX_SCOPED_CARDS,
    WORKLOAD_UID_HEADER, WorkloadFamily, WorkloadKind, WorkloadRestriction,
    deployment_environment_selector, workload_uid_selector,
};
