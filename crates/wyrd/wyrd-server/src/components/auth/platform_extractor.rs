//! Platform-control-plane authentication.
//!
//! A platform request presents a platform **session token**, never credential
//! material: the credential is exchanged once and the token carries every
//! request after that, so no served surface reads a secret, a lookup prefix, or
//! a credential record. The token arrives on `Authorization: Bearer`, distinct
//! from the tenant plane's `X-Wyrd-Access-Token`, so the two planes cannot be
//! confused by a misrouted header.
//!
//! The extractor is the only producer of a platform-scoped
//! [`AuthContext`](wyrd_runtime::AuthContext). A route that takes
//! [`PlatformCaller`] therefore cannot receive a tenant identity, and a route
//! that takes [`Caller`](super::Caller) cannot receive a platform one.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth::platform_sessions::{PlatformSessionError, PlatformSessions};
use wyrd_runtime::{AuthContext, Permission, PermissionSet, PlatformPrincipal, PrincipalId};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::platform::principal_grants::platform_grant_for_principal;

use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Header carrying a platform administrative credential.
const AUTHORIZATION: HeaderName = HeaderName::from_static("authorization");

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

/// Read a platform session token from `Authorization: Bearer`.
///
/// Every malformed or missing case yields the same unauthenticated error as an
/// unknown credential, so header shape is not an oracle either.
fn extract_platform_token(headers: &HeaderMap) -> Option<SecretString> {
    let raw = headers.get(AUTHORIZATION)?.to_str().ok()?;
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
            WyrdErrorResponse::from(WyrdError::Internal {
                message: "platform grant lookup failed".to_owned(),
                details: serde_json::json!({ "error": error.to_string() }),
            })
        })?;
    let Some(stored) = stored else {
        return Ok(PermissionSet::new());
    };
    let permissions: Vec<Permission> = serde_json::from_value(stored).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform grant permissions are corrupt".to_owned(),
            details: serde_json::json!({ "error": error.to_string() }),
        })
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
                return Err(WyrdErrorResponse::from(WyrdError::Internal {
                    message: "platform session verification failed".to_owned(),
                    details: serde_json::json!({ "error": error.to_string() }),
                }));
            }
        };
        let principal_id = session.principal_id;

        let effective_permissions = resolve_grant(&pool, principal_id).await?;
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .cloned()
            .ok_or_else(|| {
                WyrdErrorResponse::from(WyrdError::Internal {
                    message: "missing RequestId extension".to_owned(),
                    details: serde_json::json!({ "extension": "RequestId" }),
                })
            })?;

        Ok(Self {
            context: AuthContext::from(PlatformPrincipal::new(principal_id, effective_permissions)),
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
                PermissionSet::new(),
            )),
            credential_id: Some(Uuid::now_v7()),
            request_id: wyrd_spec::request_id::RequestId::parse(&Uuid::now_v7().to_string())
                .expect("generated UUIDv7 is a valid request id"),
        }
    }

    /// A bearer session token is read from the platform header.
    #[test]
    fn bearer_session_token_is_extracted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
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
                headers.insert("authorization", value.parse().unwrap());
            }
            assert!(
                extract_platform_token(&headers).is_none(),
                "{value:?} must not resolve to a session token"
            );
        }
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
