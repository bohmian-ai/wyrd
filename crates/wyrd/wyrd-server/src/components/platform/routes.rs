//! Platform control-plane routes.
//!
//! Every route here takes [`PlatformCaller`], so a tenant identity cannot reach
//! them: the extractor is the only producer of a platform-scoped context and it
//! accepts only a platform session. Authorization and its audit record happen
//! inside each operation, in the transaction that performs it.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use wyrd_spec::auth::{CreateTenantRequest, CreateTenantResponse};
use wyrd_spec::error::WyrdError;

use crate::components::auth::PlatformCaller;
use crate::components::platform::provisioning::{ProvisionError, TenantProvisioning};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build the platform control-plane routes for the `/v1` group.
pub fn platform_router() -> Router<AppState> {
    Router::new().route("/tenants", post(create_tenant))
}

/// Provision a tenant and return its one-time administrative credential.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the slug is
/// taken, the platform plane is unconfigured, or provisioning fails.
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn create_tenant(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<CreateTenantRequest>,
) -> Result<Json<CreateTenantResponse>, WyrdErrorResponse> {
    let Some(operator) = state.postgres.operator_pool() else {
        return Err(WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform control plane is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        }));
    };
    let provisioning = TenantProvisioning::new(operator, state.postgres.app_pool().clone());

    provisioning
        .provision(&caller, request)
        .await
        .map(Json)
        .map_err(provision_error)
}

/// Project a provisioning failure onto the stable public error catalog.
///
/// A denial and a taken slug are caller-correctable and say so; everything else
/// is internal, because the caller can do nothing with a store failure's
/// details and they may name infrastructure.
fn provision_error(error: ProvisionError) -> WyrdErrorResponse {
    match error {
        ProvisionError::Denied => WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
            message: "not authorized to provision tenants".to_owned(),
            details: serde_json::json!({ "permission": "tenants:write" }),
        }),
        ProvisionError::SlugTaken => WyrdErrorResponse::from(WyrdError::Conflict {
            message: "tenant slug is already in use".to_owned(),
            details: serde_json::json!({ "field": "slug" }),
        }),
        ProvisionError::AuditUnavailable(reason) | ProvisionError::Store(reason) => {
            WyrdErrorResponse::from(WyrdError::Internal {
                message: "tenant provisioning failed".to_owned(),
                details: serde_json::json!({ "error": reason }),
            })
        }
    }
}
