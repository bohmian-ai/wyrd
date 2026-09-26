//! Verification control-plane operations shared by HTTP and MCP.
//!
//! [`VerificationControl`] is the one owner of binding status, manual run
//! requests, and run status. Both transports hand it an authenticated
//! [`Caller`] and typed input, so authorization, audit, tenancy, idempotency,
//! and error mapping have exactly one implementation.

use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, IdempotencyKey, VerificationRunId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_spec::verification::{
    StartVerificationRunRequest, VerificationBindingStatus, VerificationRunInput,
    VerificationRunStatus, VerificationRunTarget, VerifierReadiness,
};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_runs::{
    EnqueueRefusal, ManualEnqueueOutcome, RequestKey, VerifierRunQueue,
};

use crate::audit;
use crate::components::auth::Caller;
use crate::state::{AppState, registry_db_error};

/// Audit operation of a binding status read.
const GET_BINDING: &str = "verification.binding.read";

/// Audit operation of a manual run request.
const START_RUN: &str = "verification.run.start";

/// Audit operation of a run status read.
const GET_RUN: &str = "verification.run.read";

/// Decode a manual run request body with a precise refusal for each part.
///
/// A body that decodes whole is returned as is. Otherwise the `target` and
/// `input` members are decoded alone so a malformed target answers
/// [`WyrdError::VerificationInvalidTarget`] and a malformed input answers
/// [`WyrdError::VerificationInvalidWindow`]; anything else, such as a non-object
/// body or an unknown member, is [`WyrdError::Validation`]. HTTP and MCP both
/// decode through here so they refuse the same body identically.
///
/// # Errors
/// Returns the refusal named above for the first malformed part.
pub(crate) fn decode_start_request(
    body: JsonValue,
) -> Result<StartVerificationRunRequest, WyrdError> {
    let error = match serde_json::from_value::<StartVerificationRunRequest>(body.clone()) {
        Ok(request) => return Ok(request),
        Err(error) => error,
    };
    let part = |name: &str| body.get(name).cloned().unwrap_or(JsonValue::Null);
    if let Err(target) = serde_json::from_value::<VerificationRunTarget>(part("target")) {
        return Err(WyrdError::VerificationInvalidTarget {
            message: format!("target is invalid: {target}"),
            details: serde_json::json!({ "field": "target" }),
        });
    }
    if let Err(input) = serde_json::from_value::<VerificationRunInput>(part("input")) {
        return Err(WyrdError::VerificationInvalidWindow {
            message: format!("input is invalid: {input}"),
            details: serde_json::json!({ "field": "input" }),
        });
    }
    Err(WyrdError::Validation {
        message: format!("verification run request is invalid: {error}"),
        details: serde_json::json!({}),
    })
}

/// Audit resource naming one manual run target.
fn target_resource(target: &VerificationRunTarget) -> String {
    match target {
        VerificationRunTarget::Binding { binding_id } => {
            format!("verification.binding:{binding_id}")
        }
        VerificationRunTarget::Verifier {
            verifier_uid,
            subject_card_uid,
        } => format!("verifier:{verifier_uid}/subject:{subject_card_uid}"),
    }
}

/// Map a refused enqueue to its public error.
///
/// An unknown binding is not found; an unfitted baseline is not ready; every
/// other refusal means the target cannot run the requested Drift window.
fn refusal_error(refusal: EnqueueRefusal, target: &VerificationRunTarget) -> WyrdError {
    let details = serde_json::json!({ "target": target });
    match refusal {
        EnqueueRefusal::BindingNotFound => WyrdError::VerificationBindingNotFound {
            message: "no verification binding with this ID in the caller's tenant".to_owned(),
            details,
        },
        EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady) => {
            WyrdError::VerificationNotReady {
                message: "the Verifier's fitted baseline is not ready".to_owned(),
                details,
            }
        }
        EnqueueRefusal::NotReady(_) => WyrdError::VerificationInvalidTarget {
            message: "the target is not an active Verifier Card".to_owned(),
            details,
        },
        EnqueueRefusal::SubjectUnavailable => WyrdError::VerificationInvalidTarget {
            message: "the subject Card is not active in the caller's tenant".to_owned(),
            details,
        },
        EnqueueRefusal::InputMismatch => WyrdError::VerificationInvalidTarget {
            message: "the Verifier's implementation cannot analyze a Drift window".to_owned(),
            details,
        },
    }
}

/// Owner of the Verification control plane for one request.
///
/// Borrows the process [`AppState`] for its tenant connections, permission
/// checker, and audit path, and holds the queue policy every run it creates
/// freezes. It holds no connection of its own: each operation opens one
/// tenant transaction under forced RLS, so another tenant's bindings and runs
/// are indistinguishable from absent ones.
pub(crate) struct VerificationControl<'a> {
    /// Server state providing tenant connections, authorization, and audit.
    state: &'a AppState,
    /// Durable run queue whose policies manual runs freeze.
    queue: VerifierRunQueue,
}

impl<'a> VerificationControl<'a> {
    /// Build the control plane over this process's state.
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
            queue: VerifierRunQueue::default(),
        }
    }

    /// Read one binding's identities, activity gate, readiness, and cursor.
    ///
    /// Authorizes and audits `cards:read` standalone, then reads under the
    /// caller's tenant.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `cards:read`,
    /// [`WyrdError::VerificationBindingNotFound`] when the tenant has no such
    /// binding, [`WyrdError::AuditUnavailable`] when the decision cannot be
    /// recorded, and a registry unavailability error when the read fails.
    pub(crate) async fn get_binding(
        &self,
        caller: &Caller,
        binding_id: BindingId,
    ) -> Result<VerificationBindingStatus, WyrdError> {
        let resource = format!("verification.binding:{binding_id}");
        audit::authorize(
            self.state,
            caller,
            &Permission::card_read(),
            GET_BINDING,
            &resource,
        )
        .await?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let status = self
            .queue
            .binding_status(&mut conn, binding_id)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        status.ok_or_else(|| WyrdError::VerificationBindingNotFound {
            message: "no verification binding with this ID in the caller's tenant".to_owned(),
            details: serde_json::json!({ "binding_id": binding_id }),
        })
    }

    /// Read one run's execution status, requester, result pointer, and dispatches.
    ///
    /// Authorizes and audits `cards:read` standalone, then reads under the
    /// caller's tenant.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `cards:read`,
    /// [`WyrdError::VerificationRunNotFound`] when the tenant has no such run,
    /// [`WyrdError::AuditUnavailable`] when the decision cannot be recorded,
    /// and a registry unavailability error when the read fails.
    pub(crate) async fn get_run(
        &self,
        caller: &Caller,
        run_id: VerificationRunId,
    ) -> Result<VerificationRunStatus, WyrdError> {
        let resource = format!("verification.run:{run_id}");
        audit::authorize(
            self.state,
            caller,
            &Permission::card_read(),
            GET_RUN,
            &resource,
        )
        .await?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let status = self
            .queue
            .run_status(&mut conn, run_id)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        status.ok_or_else(|| WyrdError::VerificationRunNotFound {
            message: "no verification run with this ID in the caller's tenant".to_owned(),
            details: serde_json::json!({ "run_id": run_id }),
        })
    }

    /// Durably enqueue one manual Drift run for the authenticated caller.
    ///
    /// Validates the window, then evaluates `evals:run`; a denial is audited
    /// standalone. In one tenant transaction it then checks a Card-bound
    /// caller's signed scope over the exact subject, appends the single
    /// allow or deny decision, and enqueues the run with the caller as
    /// requester — never as a binding owner. A request carrying `key` replays
    /// the requester's earlier run for the same body and is refused for a
    /// different one. Refusals commit only their decision row. When the
    /// transaction fails before committing, the allow decision is recorded
    /// standalone so an evaluated permission is never lost.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInvalidWindow`] for an invalid window,
    /// [`WyrdError::PermissionDeniedRbac`] without `evals:run` or subject
    /// scope, [`WyrdError::VerificationBindingNotFound`] for an unknown
    /// binding, [`WyrdError::VerificationInvalidTarget`] for an unrunnable
    /// target, [`WyrdError::VerificationNotReady`] for an unfitted baseline,
    /// [`WyrdError::RegistryIdempotencyConflict`] when `key` was used for a
    /// different request, [`WyrdError::AuditUnavailable`] when a decision
    /// cannot be recorded, and a registry unavailability error when the
    /// transaction fails.
    pub(crate) async fn start_run(
        &self,
        caller: &Caller,
        request: &StartVerificationRunRequest,
        key: Option<&IdempotencyKey>,
    ) -> Result<VerificationRunId, WyrdError> {
        request.validate()?;
        let resource = target_resource(&request.target);
        let allowed = audit::authorize_recording_denial(
            self.state,
            caller,
            &Permission::eval_run(),
            START_RUN,
            &resource,
        )
        .await?;
        match self
            .enqueue(caller, request, key, &allowed, &resource)
            .await
        {
            Ok(committed) => committed,
            Err(uncommitted) => {
                audit::record_audit(
                    self.state.postgres.vala_pool(),
                    caller.data_tenant_id,
                    &allowed,
                )
                .await?;
                Err(uncommitted)
            }
        }
    }

    /// Run the manual request transaction and commit its decision.
    ///
    /// The outer result is `Err` only when nothing committed; the inner
    /// result is the committed answer, including committed refusals.
    ///
    /// # Errors
    /// Returns the registry or audit error of a transaction that did not commit.
    async fn enqueue(
        &self,
        caller: &Caller,
        request: &StartVerificationRunRequest,
        key: Option<&IdempotencyKey>,
        allowed: &AuditEvent,
        resource: &str,
    ) -> Result<Result<VerificationRunId, WyrdError>, WyrdError> {
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        if !self
            .subject_in_scope(&mut conn, caller, &request.target)
            .await?
        {
            let denied = audit::audit_event(
                caller,
                START_RUN,
                resource,
                &Permission::eval_run().to_string(),
                AuditOutcome::Denied,
            );
            audit::append_on(&mut conn, &denied).await?;
            conn.commit().await.map_err(registry_db_error)?;
            return Ok(Err(WyrdError::PermissionDeniedRbac {
                message: "the caller's Card scope does not cover the verified subject".to_owned(),
                details: serde_json::json!({ "resource": resource }),
            }));
        }
        audit::append_on(&mut conn, allowed).await?;
        let digest = serde_json::to_vec(request)
            .map(|body| Sha256::digest(body).to_vec())
            .map_err(registry_db_error)?;
        let request_key = key.map(|key| RequestKey {
            key: key.as_str(),
            request_sha256: &digest,
        });
        let outcome = self
            .queue
            .enqueue_manual(
                &mut conn,
                caller.principal.id,
                &request.target,
                request.input.drift_window(),
                request_key,
            )
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok(match outcome {
            ManualEnqueueOutcome::Enqueued(run_id) | ManualEnqueueOutcome::Replayed(run_id) => {
                Ok(run_id)
            }
            ManualEnqueueOutcome::KeyReused(_) => Err(WyrdError::RegistryIdempotencyConflict {
                message: "this Idempotency-Key was already used for a different run request"
                    .to_owned(),
                details: serde_json::json!({ "header": "Idempotency-Key" }),
            }),
            ManualEnqueueOutcome::Refused(refusal) => Err(refusal_error(refusal, &request.target)),
        })
    }

    /// Whether the caller may request verification of the target's subject.
    ///
    /// Users, tenant administrators, and Card-free services act on RBAC alone.
    /// A Card-bound principal must hold signed scope over the exact subject
    /// Card. An unknown binding or absent subject has nothing to scope, so it
    /// passes here and enqueue refuses it with its own error.
    ///
    /// # Errors
    /// Returns a registry unavailability error when a read fails.
    async fn subject_in_scope(
        &self,
        conn: &mut TenantConn<'_>,
        caller: &Caller,
        target: &VerificationRunTarget,
    ) -> Result<bool, WyrdError> {
        if caller.principal.card_ref().is_none() {
            return Ok(true);
        }
        let Some(subject) = self
            .queue
            .target_subject(conn, target)
            .await
            .map_err(registry_db_error)?
        else {
            return Ok(true);
        };
        let row = match get_card_by_uid(conn, &subject).await {
            Ok(row) => row,
            Err(WyrdError::RegistryCardNotFound { .. }) => return Ok(true),
            Err(error) => return Err(error),
        };
        Ok(caller.principal.authorizes_card(&CardRef {
            kind: row.kind,
            name: row.name,
            version: row.version,
            space: Some(row.space),
            uid: Some(row.card_uid),
        }))
    }
}

#[cfg(test)]
mod tests {
    //! Pure request decoding and refusal mapping.

    use super::*;

    /// A well-formed body decodes; a malformed target, a malformed input, and
    /// an unknown member each map to their own stable error code.
    ///
    /// # Panics
    /// Panics when any body decodes differently than its documented refusal.
    #[test]
    fn decode_start_request_names_the_malformed_part() {
        let target = serde_json::json!({ "kind": "binding", "binding_id": BindingId::new_v7() });
        let input = serde_json::json!({
            "kind": "drift_window", "start": "2026-09-17T00:00:00Z", "end": "2026-09-17T01:00:00Z"
        });
        assert!(
            decode_start_request(serde_json::json!({ "target": target, "input": input })).is_ok()
        );
        let code = |body: JsonValue| {
            decode_start_request(body)
                .expect_err("malformed body is refused")
                .code()
        };
        assert_eq!(
            code(serde_json::json!({ "target": { "kind": "nope" }, "input": input })),
            "WYRD_VERIFICATION_400_INVALID_TARGET"
        );
        assert_eq!(
            code(serde_json::json!({ "target": target, "input": { "kind": "drift_window" } })),
            "WYRD_VERIFICATION_400_INVALID_WINDOW"
        );
        assert_eq!(
            code(serde_json::json!({ "target": target, "input": input, "extra": 1 })),
            "WYRD_SPEC_400_VALIDATION"
        );
    }

    /// Only an unfitted baseline is "not ready"; an unknown binding is not
    /// found and every other refusal is an invalid target.
    ///
    /// # Panics
    /// Panics when a refusal maps to the wrong public code.
    #[test]
    fn refusals_map_to_stable_codes() {
        let target = VerificationRunTarget::Binding {
            binding_id: BindingId::new_v7(),
        };
        let cases = [
            (
                EnqueueRefusal::BindingNotFound,
                "WYRD_VERIFICATION_404_BINDING_NOT_FOUND",
            ),
            (
                EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady),
                "WYRD_VERIFICATION_409_VERIFIER_NOT_READY",
            ),
            (
                EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable),
                "WYRD_VERIFICATION_400_INVALID_TARGET",
            ),
            (
                EnqueueRefusal::SubjectUnavailable,
                "WYRD_VERIFICATION_400_INVALID_TARGET",
            ),
            (
                EnqueueRefusal::InputMismatch,
                "WYRD_VERIFICATION_400_INVALID_TARGET",
            ),
        ];
        for (refusal, code) in cases {
            assert_eq!(refusal_error(refusal, &target).code(), code);
        }
    }
}
