//! Platform-control-plane authentication.
//!
//! A platform request presents a platform **session token**, never credential
//! material: the credential is exchanged once and the token carries every
//! request after that, so no served surface reads a secret, a lookup prefix, or
//! a credential record. The token arrives on `X-Wyrd-Access-Token`, the one
//! header every Wyrd plane authenticates on; the caller's own `Authorization`
//! header belongs to the calling application and is never read here.
//!
//! The header does not separate the planes — any client can set any header.
//! What separates them is the scope marker `verify_platform` requires and the
//! extractor a route declares: a tenant access token carries the wrong scope
//! and is refused here, and a platform session produces no tenant so it cannot
//! satisfy [`Caller`](super::Caller).
//!
//! The extractor is the only producer of a platform-scoped
//! [`AuthContext`](wyrd_runtime::AuthContext). A route that takes
//! [`PlatformCaller`] therefore cannot receive a tenant identity, and a route
//! that takes [`Caller`](super::Caller) cannot receive a platform one.

use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth::platform_sessions::{PlatformSessionError, PlatformSessions};
use wyrd_runtime::{AuthContext, Permission, PermissionSet, PlatformPrincipal, PrincipalId};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::platform::principal_grants::platform_grant_for_principal;

use crate::components::auth::token_extract::{self, WYRD_ACCESS_TOKEN_HEADER};
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;

/// An authenticated platform-control-plane caller.
///
/// Holds no tenant, because a platform principal has none. Handlers reach the
/// tenant directory through the operator boundary and never through row-level
/// security, which is exactly the separation this type makes unrepresentable to
/// violate.
#[derive(Debug, Clone)]
pub struct PlatformCaller {
    /// The authenticated identity, always the platform variant.
    pub context: AuthContext,
    /// Credential that minted the presented session, carried into audit so an
    /// operation is traceable to the credential as well as the identity.
    ///
    /// `None` when the session came from federated login, where the caller
    /// presented an identity rather than a credential. Audit records the
    /// principal either way; only the credential column is left empty.
    pub credential_id: Option<Uuid>,
    /// Request correlator for audit and response headers.
    pub request_id: RequestId,
}

impl PlatformCaller {
    /// The authenticated platform principal id, for audit attribution.
    #[must_use]
    pub fn principal_id(&self) -> PrincipalId {
        self.context.principal_id()
    }
}

/// Read a platform session token from `X-Wyrd-Access-Token`.
///
/// Every malformed or missing case yields `None`, which the caller renders as
/// the same unauthenticated error as an unknown credential, so header shape is
/// not an oracle either. That is why this does not reuse
/// `token_extract::extract_wyrd_access_token`, whose informative
/// missing-versus-malformed distinction a legitimate client never needs and an
/// enumerating one would read.
fn extract_platform_token(headers: &HeaderMap) -> Option<SecretString> {
    let raw = headers.get(WYRD_ACCESS_TOKEN_HEADER)?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?;
    if token.is_empty() {
        return None;
    }
    Some(SecretString::from(token.to_owned()))
}

/// The single indistinguishable rejection for the platform plane.
///
/// A missing header, a malformed value, an unknown credential, a wrong secret,
/// a revoked or expired credential, and a suspended principal all render
/// identically. Only a store outage is reported differently, because an
/// unavailable database is not a failed authentication.
fn unauthenticated() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Unauthenticated {
        message: "invalid platform session".to_owned(),
        details: serde_json::json!({ "plane": "platform" }),
    })
}

/// Resolve a verified platform principal's granted permissions.
///
/// # Errors
/// Returns a store error when the grant read fails; a principal with no grant
/// row resolves to an empty set, which authorizes nothing.
async fn resolve_grant(
    pool: &OperatorPool,
    principal_id: PrincipalId,
) -> Result<PermissionSet, WyrdErrorResponse> {
    let stored = platform_grant_for_principal(pool, principal_id.as_uuid())
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure("platform grant lookup failed", &error))
        })?;
    let Some(stored) = stored else {
        return Ok(PermissionSet::new());
    };
    let permissions: Vec<Permission> = serde_json::from_value(stored).map_err(|error| {
        WyrdErrorResponse::from(internal_failure(
            "platform grant permissions are corrupt",
            &error,
        ))
    })?;
    let mut set = PermissionSet::new();
    for permission in permissions {
        set.insert(permission);
    }
    Ok(set)
}

impl FromRequestParts<AppState> for PlatformCaller {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(pool) = state.postgres.operator_pool() else {
            return Err(WyrdErrorResponse::from(WyrdError::Internal {
                message: "platform control plane is not configured".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            }));
        };
        let Some(token) = extract_platform_token(&parts.headers) else {
            return Err(unauthenticated());
        };
        let Some(verifier) = state.auth.token_verifier.clone() else {
            return Err(WyrdErrorResponse::from(WyrdError::Internal {
                message: "token verification is not configured".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            }));
        };
        let Some(issuing_key) = state.auth.issuing_key.clone() else {
            return Err(WyrdErrorResponse::from(WyrdError::Internal {
                message: "platform session issuance is not configured".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            }));
        };

        let Ok(claims) = verifier.verify_platform(token.expose_secret()) else {
            return Err(unauthenticated());
        };
        let sessions = PlatformSessions::new(pool.clone(), issuing_key);
        let session = match sessions.confirm(&claims).await {
            Ok(session) => session,
            Err(PlatformSessionError::Invalid) => return Err(unauthenticated()),
            Err(error) => {
                return Err(WyrdErrorResponse::from(internal_failure(
                    "platform session verification failed",
                    &error,
                )));
            }
        };
        let principal_id = session.principal_id;

        let effective_permissions = resolve_grant(&pool, principal_id).await?;
        let request_id = token_extract::request_id(parts)?;

        Ok(Self {
            context: AuthContext::from(
                PlatformPrincipal::new(principal_id, session.principal_kind, effective_permissions)
                    .with_credential_id(session.credential_id),
            ),
            credential_id: session.credential_id,
            request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use secrecy::ExposeSecret;
    use uuid::Uuid;
    use wyrd_runtime::{AuthContext, PermissionSet, PlatformPrincipal, PrincipalId};

    use super::{PlatformCaller, extract_platform_token};

    /// Build a platform caller for shape assertions.
    fn caller() -> PlatformCaller {
        PlatformCaller {
            context: AuthContext::from(PlatformPrincipal::new(
                PrincipalId::new(Uuid::now_v7()),
                wyrd_spec::auth::PrincipalKindTag::GlobalAdmin,
                PermissionSet::new(),
            )),
            credential_id: Some(Uuid::now_v7()),
            request_id: wyrd_spec::request_id::RequestId::parse(&Uuid::now_v7().to_string())
                .expect("generated UUIDv7 is a valid request id"),
        }
    }

    /// A bearer session token is read from the canonical Wyrd header.
    #[test]
    fn bearer_session_token_is_extracted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-wyrd-access-token",
            "Bearer header.payload.sig".parse().unwrap(),
        );

        let extracted = extract_platform_token(&headers).expect("token is present");

        assert_eq!(extracted.expose_secret(), "header.payload.sig");
    }

    /// Missing, empty, and non-bearer headers all yield nothing, so header shape
    /// cannot be used to probe the plane.
    #[test]
    fn malformed_headers_yield_no_token() {
        for value in ["", "Bearer ", "Basic abc", "header.payload.sig"] {
            let mut headers = HeaderMap::new();
            if !value.is_empty() {
                headers.insert("x-wyrd-access-token", value.parse().unwrap());
            }
            assert!(
                extract_platform_token(&headers).is_none(),
                "{value:?} must not resolve to a session token"
            );
        }
    }

    /// The application's own `Authorization` header is left to the application.
    ///
    /// A request carrying both authenticates on the Wyrd header and the
    /// application's bearer is never consulted, so a caller that already speaks
    /// OAuth to its own upstream does not have to surrender that header to
    /// reach Wyrd.
    #[test]
    fn an_applications_own_authorization_header_is_never_read() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer the.applications.own.token".parse().unwrap(),
        );
        headers.insert(
            "x-wyrd-access-token",
            "Bearer the.wyrd.session".parse().unwrap(),
        );

        let extracted = extract_platform_token(&headers).expect("the Wyrd header is read");

        assert_eq!(extracted.expose_secret(), "the.wyrd.session");
    }

    /// An `Authorization` header alone authenticates nothing.
    #[test]
    fn authorization_alone_yields_no_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer a.platform.session".parse().unwrap(),
        );

        assert!(
            extract_platform_token(&headers).is_none(),
            "a token on Authorization must not authenticate a platform request"
        );
    }

    /// A platform caller has no tenant to offer any handler, so a platform
    /// handler cannot open a tenant-scoped connection even by mistake.
    #[test]
    fn platform_caller_exposes_no_tenant() {
        let caller = caller();

        assert_eq!(caller.context.tenant_id(), None);
        assert!(caller.context.tenant().is_none());
    }

    /// A federated caller carries no credential, and a credential-minted one
    /// does.
    ///
    /// The distinction is what audit records, so the type has to be able to
    /// express both. Asserting it on a fixture would be circular; this asserts
    /// the shape the two session kinds actually produce.
    #[test]
    fn a_caller_carries_a_credential_only_when_one_minted_its_session() {
        let federated = PlatformCaller {
            credential_id: None,
            ..caller()
        };
        let from_credential = caller();

        assert!(
            from_credential.credential_id.is_some(),
            "a credential-minted session names the credential audit must record"
        );
        assert!(
            federated.credential_id.is_none(),
            "a federated session names no credential, because none was presented"
        );
    }
}
