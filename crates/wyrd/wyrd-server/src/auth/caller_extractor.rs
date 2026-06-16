// TODO(auth-wiring): remove when JWT verifier lands
// DEPRECATED(auth-wiring): x-wyrd-data-tenant-id will be removed when tenant comes from JWT claims.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use wyrd_spec::DataTenantId;
use wyrd_spec::authz::Principal;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::auth::AuthenticatedPrincipal;
use crate::error::WyrdErrorResponse;

/// Header used only by the skeleton stub until auth claims carry tenant data.
///
/// Expects a DataTenantId (UUIDv7) value. Temporary — will be removed when
/// tenant isolation comes from verified JWT claims.
pub const STUB_TENANT_HEADER: &str = "x-wyrd-data-tenant-id";

/// Authenticated caller context used by tenant-scoped service code.
#[derive(Debug, Clone)]
pub struct Caller {
    /// Resolved tenant isolation key for data-plane access.
    pub data_tenant_id: DataTenantId,
    /// Authenticated principal.
    pub principal: Principal,
    /// Request correlation ID.
    pub request_id: RequestId,
}

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let principal = AuthenticatedPrincipal::from_request_parts(parts, state)
            .await?
            .principal;
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .cloned()
            .ok_or_else(missing_request_id)
            .map_err(WyrdErrorResponse::from)?;
        let data_tenant_id = stub_tenant_id(parts).map_err(WyrdErrorResponse::from)?;

        Ok(Self {
            data_tenant_id,
            principal,
            request_id,
        })
    }
}

fn missing_request_id() -> WyrdError {
    WyrdError::Internal {
        message: "missing RequestId extension".to_owned(),
        details: serde_json::json!({ "extension": "RequestId" }),
    }
}

#[cfg(feature = "stub-auth")]
fn stub_tenant_id(parts: &Parts) -> Result<DataTenantId, WyrdError> {
    let Some(header) = parts.headers.get(STUB_TENANT_HEADER) else {
        return Err(WyrdError::InvalidToken {
            message: format!("missing required header: {STUB_TENANT_HEADER}"),
            details: serde_json::json!({ "header": STUB_TENANT_HEADER }),
        });
    };
    let value = header.to_str().map_err(|_| WyrdError::InvalidToken {
        message: "tenant claim is not valid UTF-8".to_owned(),
        details: serde_json::json!({ "header": STUB_TENANT_HEADER }),
    })?;
    value
        .parse::<DataTenantId>()
        .map_err(|error| WyrdError::InvalidToken {
            message: "tenant claim is not a valid DataTenantId".to_owned(),
            details: serde_json::json!({
                "header": STUB_TENANT_HEADER,
                "source": error.to_string(),
            }),
        })
}

#[cfg(not(feature = "stub-auth"))]
fn stub_tenant_id(_parts: &Parts) -> Result<DataTenantId, WyrdError> {
    Err(WyrdError::InvalidToken {
        message: "auth not configured: enable the stub-auth feature in non-production builds, or wire the JWT verifier".to_owned(),
        details: serde_json::json!({}),
    })
}
