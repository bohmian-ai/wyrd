//! Platform-control-plane authentication.
//!
//! The platform plane authenticates a credential directly rather than a Wyrd
//! access token: it exists before any tenant does, so there is no tenant to mint
//! a token against. The credential arrives on `Authorization: Bearer`, distinct
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
use secrecy::SecretString;
use wyrd_auth::platform_credentials::{PlatformCredentialError, PlatformCredentials};
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
    /// Request correlator for audit and response headers.
    pub request_id: RequestId,
}

impl PlatformCaller {
    /// The authenticated platform principal id, for audit attribution.
    #[must_use]
    pub fn principal_id(&self) -> PrincipalId {
        self.context.principal_id()
    }

    /// Authorize one platform-plane permission.
    ///
    /// Absence of a grant is denial: a platform principal that authenticates
    /// but holds no matching permission is refused exactly like an unknown one.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when the principal's grant does
    /// not cover `required`.
    pub fn authorize(&self, required: &Permission) -> Result<(), WyrdErrorResponse> {
        if self.context.effective_permissions().contains(required) {
            return Ok(());
        }
        Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
            message: "platform principal lacks the required permission".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        }))
    }
}

/// Read a platform credential from `Authorization: Bearer`.
///
/// Every malformed or missing case yields the same unauthenticated error as an
/// unknown credential, so header shape is not an oracle either.
fn extract_platform_credential(headers: &HeaderMap) -> Option<SecretString> {
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
        message: "invalid platform credential".to_owned(),
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
        let Some(presented) = extract_platform_credential(&parts.headers) else {
            return Err(unauthenticated());
        };

        let principal_id = match PlatformCredentials.authenticate(&pool, &presented).await {
            Ok(principal_id) => principal_id,
            Err(PlatformCredentialError::InvalidCredential) => return Err(unauthenticated()),
            Err(error) => {
                return Err(WyrdErrorResponse::from(WyrdError::Internal {
                    message: "platform credential verification failed".to_owned(),
                    details: serde_json::json!({ "error": error.to_string() }),
                }));
            }
        };

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
            request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use secrecy::ExposeSecret;
    use wyrd_runtime::{AuthContext, Permission, PermissionSet, PlatformPrincipal, PrincipalId};

    use super::{PlatformCaller, extract_platform_credential};

    /// Build a platform caller holding exactly `permissions`.
    fn caller(permissions: PermissionSet) -> PlatformCaller {
        PlatformCaller {
            context: AuthContext::from(PlatformPrincipal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                permissions,
            )),
            request_id: wyrd_spec::request_id::RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("generated UUIDv7 is a valid request id"),
        }
    }

    /// A bearer credential is read from the platform header.
    #[test]
    fn bearer_credential_is_extracted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer wyrd_global_abc_def".parse().unwrap(),
        );

        let extracted = extract_platform_credential(&headers).expect("credential is present");

        assert_eq!(extracted.expose_secret(), "wyrd_global_abc_def");
    }

    /// Missing, empty, and non-bearer headers all yield nothing, so header shape
    /// cannot be used to probe the plane.
    #[test]
    fn malformed_headers_yield_no_credential() {
        for value in ["", "Bearer ", "Basic abc", "wyrd_global_abc_def"] {
            let mut headers = HeaderMap::new();
            if !value.is_empty() {
                headers.insert("authorization", value.parse().unwrap());
            }
            assert!(
                extract_platform_credential(&headers).is_none(),
                "{value:?} must not resolve to a credential"
            );
        }
    }

    /// A platform caller authorizes only what its grant covers, and holding a
    /// platform permission never implies any tenant permission.
    #[test]
    fn authorization_follows_the_grant_and_never_reaches_a_tenant() {
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let caller = caller(permissions);

        assert!(caller.authorize(&Permission::tenant_create()).is_ok());
        assert!(caller.authorize(&Permission::tenant_suspend()).is_err());
        assert!(
            caller.authorize(&Permission::card_read()).is_err(),
            "platform authority never confers tenant data access"
        );
    }

    /// A platform caller has no tenant to offer any handler.
    #[test]
    fn platform_caller_exposes_no_tenant() {
        let caller = caller(PermissionSet::new());

        assert_eq!(caller.context.tenant_id(), None);
        assert!(caller.context.tenant().is_none());
    }

    /// An empty grant authenticates but authorizes nothing: absence is denial.
    #[test]
    fn empty_grant_authorizes_nothing() {
        let caller = caller(PermissionSet::new());

        assert!(caller.authorize(&Permission::tenant_create()).is_err());
        assert!(caller.authorize(&Permission::tenant_read()).is_err());
    }
}
