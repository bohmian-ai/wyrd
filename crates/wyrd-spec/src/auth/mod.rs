//! Authentication request and response contracts.

mod admin;
mod card_scope;
mod issue_key;
mod oidc;
mod principal_id;
mod principal_kind;
mod revoke;
mod secret_bearer;
mod token;

pub use admin::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    PrincipalKindPayload, TrustedIssuerView, WorkloadBindingView,
};
pub use card_scope::CardScope;
pub use issue_key::{IssueKeyRequest, IssueKeyResponse};
pub use oidc::{AbsoluteUrl, CallbackQuery, IssuerUrl, LoginInitResponse, UrlParseError};
pub use principal_id::PrincipalId;
pub use principal_kind::{CardKindMismatch, PrincipalKind, PrincipalKindTag};
pub use revoke::{RevokePrincipalRequest, RevokePrincipalResponse};
pub use secret_bearer::SecretBearer;
pub use token::{RequestedSubject, SubjectTokenType, TokenRequest, TokenResponse, TokenType};
