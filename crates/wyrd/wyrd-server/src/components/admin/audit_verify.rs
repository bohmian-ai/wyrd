//! `POST /v1/admin/audit/verify` — re-verify all seal checkpoints for the
//! caller's tenant against the Iceberg `audit_log`.
//!
//! Gated on `audit:read`. Returns a structured [`AuditVerifyResponse`] with the
//! outcome; a tampered or gapped range returns `409 Conflict`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use vala_bifrost::serving::audit_seal::verify::{VerifyOutcome, verify_checkpoint_walk};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;

use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Request body for `POST /v1/admin/audit/verify`.
///
/// Currently has no required fields; the caller is established from the bearer
/// token so `tenant_id` is derived from there. This struct is kept as a
/// placeholder for future parameters.
#[derive(Debug, Default, Deserialize)]
pub struct AuditVerifyRequest {}

/// Response from `POST /v1/admin/audit/verify`.
#[derive(Debug, Serialize)]
pub struct AuditVerifyResponse {
    /// The string representation of the verify outcome.
    pub outcome: String,
    /// Human-readable detail about the outcome.
    pub detail: String,
    /// First tampered checkpoint `seq_lo`, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tampered_seq_lo: Option<i64>,
    /// First tampered checkpoint `seq_hi`, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tampered_seq_hi: Option<i64>,
}

/// `POST /v1/admin/audit/verify` — verify all audit seal checkpoints for the
/// caller's tenant. Gated on `audit:read`.
///
/// Returns `200` on clean/no-checkpoints, `409` on tampered/gap.
pub async fn audit_verify(
    State(state): State<AppState>,
    caller: Caller,
    _body: Option<Json<AuditVerifyRequest>>,
) -> Result<(StatusCode, Json<AuditVerifyResponse>), WyrdErrorResponse> {
    if !caller
        .principal
        .effective_permissions
        .contains(&Permission::audit_read())
    {
        return Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
            message: "caller lacks audit:read".to_owned(),
            details: serde_json::json!({ "required": "audit:read" }),
        }));
    }

    let Some(seal_key) = state.auth.audit_seal_key.as_deref() else {
        return Err(WyrdErrorResponse::from(WyrdError::Internal {
            message: "audit seal key not configured".to_owned(),
            details: serde_json::Value::Null,
        }));
    };

    let tenant_id = caller.principal.tenant_id;
    let outcome = verify_checkpoint_walk(
        state.postgres.vala_pool(),
        &state.bifrost,
        seal_key,
        tenant_id,
    )
    .await
    .map_err(|e| {
        tracing::error!(
            error = %e,
            tenant_id = %tenant_id,
            "audit verify failed"
        );
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "audit verification failed; check server logs".to_owned(),
            details: serde_json::Value::Null,
        })
    })?;

    match outcome {
        VerifyOutcome::Clean => Ok((
            StatusCode::OK,
            Json(AuditVerifyResponse {
                outcome: "clean".to_owned(),
                detail: "all checkpoints verified".to_owned(),
                tampered_seq_lo: None,
                tampered_seq_hi: None,
            }),
        )),
        VerifyOutcome::NoCheckpoints => Ok((
            StatusCode::OK,
            Json(AuditVerifyResponse {
                outcome: "no_checkpoints".to_owned(),
                detail: "no seal checkpoints found; seal the audit log first".to_owned(),
                tampered_seq_lo: None,
                tampered_seq_hi: None,
            }),
        )),
        VerifyOutcome::Gap => Err(WyrdErrorResponse::from(WyrdError::AdminConflict {
            message: "audit gap: shipped rows found outside all sealed ranges".to_owned(),
            details: serde_json::json!({ "outcome": "gap" }),
        })),
        VerifyOutcome::Tampered { seq_lo, seq_hi } => {
            Err(WyrdErrorResponse::from(WyrdError::AdminConflict {
                message: format!(
                    "audit tamper detected: checkpoint [{seq_lo}, {seq_hi}] hash mismatch"
                ),
                details: serde_json::json!({
                    "outcome": "tampered",
                    "seq_lo": seq_lo,
                    "seq_hi": seq_hi,
                }),
            }))
        }
    }
}
