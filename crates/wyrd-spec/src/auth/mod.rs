//! Authentication request and response contracts.

mod access;
mod admin;
mod attribution;
mod browser;
mod device;
mod human_connection;
mod oidc;
mod permission;
mod permission_scope;
mod platform_identity;
mod principal_id;
mod principal_kind;
mod revoke;
mod secret_bearer;
mod social;
mod tenant_admin;
mod tenant_principals;
mod token;
mod workload;

pub use access::{
    AccessGrant, AccessSummary, BindingSubject, CreateGroupRequest, CreateRoleBindingRequest,
    DeleteGroupArgs, DeleteRoleArgs, GrantSource, GroupList, GroupMember, GroupMemberArgs,
    GroupView, ListAccessArgs, NoApplicableOperation, Operation, PrincipalAccessArgs, PutRoleArgs,
    PutRoleRequest, RoleBindingList, RoleBindingQuery, RoleBindingView, RoleList, RoleView,
    ScopedRoleAssignment, UnbindArgs, compile_binding,
};
pub use admin::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    IssuerTokenPolicy, ReplaceTrustedIssuerSecretRequest, TrustedIssuerView, WorkloadBindingView,
};
pub use attribution::Attribution;
pub use browser::{
    BrowserApiKeyRequest, BrowserCredential, BrowserLoginRequest, BrowserLoginResponse,
    BrowserLogoutRequest, BrowserSessionQuery, BrowserSessionView, BrowserSessionsResponse,
    ContactRequest, ContactVerifyRequest, DiscoveredWorkspace, DiscoveryPage, DiscoveryQuery,
    DiscoveryRequest, DiscoveryVerifyRequest, LocalCredentials, PasswordChangeRequest,
    PasswordLoginRequest, PasswordResetCodeRequest, PasswordResetRequest, PasswordSignupRequest,
    ProofStatus, ProofStatusResponse, SignupBeginRequest, SignupCompleteRequest,
    SignupCompleteResponse, SignupVerifyRequest,
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
pub use permission::{
    Action, Permission, PermissionParseError, PermissionSet, Resource, UnrepresentableIntersection,
};
pub use permission_scope::{
    BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope, CardPermissionScope,
    GatewayAccess, PermissionScope, PermissionScopeError,
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
pub use social::{
    ConfigureMailRelayRequest, ConfigureSocialProviderRequest, CreationMethods, EmailMethods,
    EnableSocialProviderRequest, LoginMethodsQuery, LoginMethodsResponse, LoginSettings,
    MailRelayView, MailSettings, MailTestRequest, MailTestResponse, SignInMethods, SignupChallenge,
    SmtpTls, SocialProvider, SocialProviderState, SocialProviderTestRequest,
    SocialProviderTestResponse, SocialProviderView, SocialProvidersResponse,
};
pub use tenant_admin::{
    CreateTenantRequest, CreateTenantResponse, ProvisionedHumanAdmin, ProvisionedTenant,
    ProvisionedTenantAdmin, RecoverTenantAdminRequest, SetTenantStatusRequest, TenantHumanAdmin,
    TenantListResponse,
};
pub use tenant_principals::{
    AccountAuthentication, CREDENTIAL_CURSOR_MAX_BYTES, CREDENTIAL_PAGE_DEFAULT,
    CREDENTIAL_PAGE_MAX, CreateServicePrincipalRequest, CreateServicePrincipalResponse,
    CreateUserCredentialRequest, CreateUserInvitationRequest, CreateUserRequest, CreatedUser,
    CredentialListQuery, CredentialListResponse, CredentialMetadata, CredentialRevoked,
    CurrentUserArgs, CurrentUserProfile, INVITATION_RECIPIENTS_MAX, InvitationDelivery,
    IssuedCredential, IssuedUserInvitation, IssuedUserInvitations, IssuedUserPasswordReset,
    ListCredentialsArgs, PrincipalPage, PrincipalQuery, PrincipalStatus, PrincipalSummary,
    RevokeCredentialArgs, RoleSource, SetWorkloadRestrictionArgs,
    USER_CREDENTIAL_MAX_LIFETIME_SECONDS, USER_CREDENTIAL_NAME_MAX_CHARS, UserInvitationList,
    UserInvitationView, UserPasswordResetView,
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
