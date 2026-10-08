//! Verification control-plane operations shared by HTTP and MCP.
//!
//! [`VerificationControl`] is the one owner of binding status, manual run
//! requests, run status, and direct execution. Both transports hand it an authenticated
//! [`Caller`] and typed input, so authorization, audit, tenancy, idempotency,
//! and error mapping have exactly one implementation.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use tracing::Instrument as _;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::VerificationResultId;
use wyrd_spec::ids::{
    BindingId, CardUid, IdempotencyKey, VerificationExecutionId, VerificationRunId,
};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::AuditOutcome;
use wyrd_spec::verification::DriftWindow;
use wyrd_spec::verification::{
    ExecuteVerificationRequest, Judgment, StartVerificationRunRequest, VerificationBindingStatus,
    VerificationRunInput, VerificationRunStatus, VerificationRunTarget, VerifierKind,
    VerifierReadiness,
};
use wyrd_sql::queries::auth::system_principal_id;
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_runs::RunInput;
use wyrd_sql::queries::verifier_runs::{
    EnqueueRefusal, ManualEnqueueOutcome, RequestKey, VerifierRunQueue,
};
use wyrd_sql::{CardStatus, ParsedCardRow, TenantConn};

use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::http::middleware::edge_timeout::EdgeTimer;
use crate::scribe_outbox::{ScribeWrite, VerifierAttribution};
use crate::state::{AppState, registry_db_error};
use crate::verification::direct::{self, DirectExecutor, EXECUTION_DEADLINE};
use crate::verification::engines::VerifierReport;
use crate::verification::results::{ResultPayloadBuilder, ResultRun};
use crate::verification::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};

/// Audit operation of a binding status read.
const GET_BINDING: &str = "verification.binding.read";

/// Audit operation of a manual run request.
const START_RUN: &str = "verification.run.start";

/// Audit operation of a run status read.
const GET_RUN: &str = "verification.run.read";

/// Audit operation of a direct execution.
const EXECUTE: &str = "verification.execute";

/// The exact Verifier and subject a direct execution resolved.
struct DirectTarget {
    /// Exact Verifier reference.
    verifier: CardRef,
    /// The Verifier's implementation.
    implementation: VerifierImplementation,
    /// Exact subject reference.
    subject: CardRef,
    /// The tenant's active SYSTEM principal the result is attributed to.
    system_principal: Option<PrincipalId>,
}

/// The exact reference of a stored Card row.
fn exact_ref(row: &ParsedCardRow) -> CardRef {
    CardRef {
        kind: row.kind.clone(),
        name: row.name.clone(),
        version: row.version.clone(),
        space: Some(row.space.clone()),
        uid: Some(row.card_uid.clone()),
    }
}

/// Whether a stored Card may be verified or verify now.
fn available(row: &ParsedCardRow) -> bool {
    matches!(row.status, CardStatus::Active | CardStatus::Deprecated)
}

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
    /// binding, and a registry unavailability error when the read fails.
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
        )?;
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
        )?;
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
    /// Validates the window, then evaluates `evals:run`. In one tenant
    /// transaction it then checks a Card-bound caller's signed scope over the
    /// exact subject and enqueues the run with the caller as requester —
    /// never as a binding owner. The single allow or deny decision is staged
    /// on [`AppState::scribe_outbox`] as soon as it is known, so the enqueue
    /// transaction never touches the tenant audit chain and the request never
    /// waits for the audit commit. A request carrying `key` replays the
    /// requester's earlier run for the same body and is refused for a
    /// different one.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInvalidWindow`] for an invalid window,
    /// [`WyrdError::PermissionDeniedRbac`] without `evals:run` or subject
    /// scope, [`WyrdError::VerificationBindingNotFound`] for an unknown
    /// binding, [`WyrdError::VerificationInvalidTarget`] for an unrunnable
    /// target, [`WyrdError::VerificationNotReady`] for an unfitted baseline,
    /// [`WyrdError::RegistryIdempotencyConflict`] when `key` was used for a
    /// different request, and a registry unavailability error when the
    /// transaction fails.
    pub(crate) async fn start_run(
        &self,
        caller: &Caller,
        request: &StartVerificationRunRequest,
        key: Option<&IdempotencyKey>,
    ) -> Result<VerificationRunId, WyrdError> {
        request.validate()?;
        let resource = target_resource(&request.target);
        if let Err(reason) = self
            .state
            .authz
            .permission_check
            .check(&caller.principal, &Permission::eval_run())
            .into_result()
        {
            self.stage_decision(caller, START_RUN, &resource, AuditOutcome::Denied);
            return Err(permission_deny_reason_to_wyrd(reason));
        }
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        if !self
            .subject_in_scope(&mut conn, caller, &request.target)
            .await?
        {
            drop(conn);
            self.stage_decision(caller, START_RUN, &resource, AuditOutcome::Denied);
            return Err(Self::scope_denied(&resource));
        }
        self.stage_decision(caller, START_RUN, &resource, AuditOutcome::Allowed);
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
        match outcome {
            ManualEnqueueOutcome::Enqueued(run_id) | ManualEnqueueOutcome::Replayed(run_id) => {
                Ok(run_id)
            }
            ManualEnqueueOutcome::KeyReused(_) => Err(WyrdError::RegistryIdempotencyConflict {
                message: "this Idempotency-Key was already used for a different run request"
                    .to_owned(),
                details: serde_json::json!({ "header": "Idempotency-Key" }),
            }),
            ManualEnqueueOutcome::Refused(refusal) => Err(refusal_error(refusal, &request.target)),
        }
    }

    /// Execute one exact Verifier over supplied input and return its judgment.
    ///
    /// Checks input bounds, then evaluates `evals:run`, then resolves both
    /// Cards and checks a Card-bound caller's signed scope over the exact
    /// subject. Permission blocks; audit does not: the single allow or deny
    /// decision is staged on [`AppState::scribe_outbox`] and the request never
    /// waits for its commit. The engine runs under [`EXECUTION_DEADLINE`] as
    /// a `direct` execution on its own telemetry; nothing else is persisted.
    /// Dropping the future cancels the execution. An HTTP caller passes its
    /// [`EdgeTimer`], handed off once the target is authorized and resolved
    /// so the deadline, not the generic edge timeout, governs the engine; MCP
    /// passes `None`.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInputTooLarge`] for an exceeded bound,
    /// [`WyrdError::PermissionDeniedRbac`] without `evals:run` or subject
    /// scope, [`WyrdError::VerificationTargetNotFound`] for an unknown,
    /// inactive, or non-Verifier target,
    /// [`WyrdError::VerificationExecutionTimedOut`] past the deadline, a
    /// registry unavailability error when a read fails, and every engine
    /// refusal of [`DirectExecutor::execute`].
    #[tracing::instrument(
        name = "verification.execute",
        skip_all,
        fields(execution_id, kind, mode = "direct", outcome)
    )]
    pub(crate) async fn execute(
        &self,
        caller: &Caller,
        request: &ExecuteVerificationRequest,
        edge_timer: Option<&EdgeTimer>,
    ) -> Result<Judgment, WyrdError> {
        request.validate()?;
        let execution_id = VerificationExecutionId::new_v7();
        let span = tracing::Span::current();
        span.record("execution_id", tracing::field::display(execution_id));
        let resource = format!(
            "verifier:{}/subject:{}/execution:{execution_id}",
            request.verifier_uid, request.subject_card_uid
        );
        if let Err(reason) = self
            .state
            .authz
            .permission_check
            .check(&caller.principal, &Permission::eval_run())
            .into_result()
        {
            self.stage_decision(caller, EXECUTE, &resource, AuditOutcome::Denied);
            return Err(permission_deny_reason_to_wyrd(reason));
        }
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct);
        let target = telemetry
            .phase(
                Phase::Load,
                self.resolve_direct(caller, request, &resource)
                    .instrument(tracing::info_span!("verification.load")),
            )
            .await?;
        if let Some(edge_timer) = edge_timer {
            edge_timer.hand_off();
        }
        let kind = VerifierKind::of(&target.implementation);
        telemetry.classify(kind);
        let started_at = Utc::now();
        let executed = telemetry
            .phase(
                Phase::Engine,
                tokio::time::timeout(
                    EXECUTION_DEADLINE,
                    DirectExecutor::new(self.state).execute(
                        caller.data_tenant_id,
                        execution_id,
                        &request.verifier_uid,
                        &target.implementation,
                        &request.input,
                        &telemetry,
                    ),
                )
                .instrument(tracing::info_span!("verification.engine")),
            )
            .await;
        let (outcome, result) = match executed {
            Err(_) => (
                "timed_out",
                Err(WyrdError::VerificationExecutionTimedOut {
                    message: "the Verifier exceeded the direct execution deadline".to_owned(),
                    details: serde_json::json!({ "deadline_seconds": EXECUTION_DEADLINE.as_secs() }),
                }),
            ),
            Ok(Err(error)) => ("errored", Err(error)),
            Ok(Ok(report)) => ("completed", Ok(report)),
        };
        span.record("outcome", outcome);
        telemetry.finish(outcome, outcome != "completed");
        let report = result?;
        self.stage_result(
            caller.data_tenant_id,
            execution_id,
            &target,
            &report,
            started_at,
        );
        Ok(Judgment {
            execution_id,
            verifier: target.verifier,
            subject: target.subject,
            kind,
            verdict: report.verdict(),
            summary: report.summary(),
            counts: report.counts(),
            detail: direct::detail(&report)?,
        })
    }

    /// Stage a completed direct execution's result on the Scribe outbox.
    ///
    /// The result takes the execution's identity, so the response's
    /// `execution_id` is the `result_id` of its rows. It is attributed to the
    /// tenant SYSTEM principal and the exact Verifier exactly like a queued
    /// result, with no run, owner, binding, or Trigger. A Drift result records
    /// the execution interval as its window; an Eval result records the
    /// execution's synthetic record. The verdict is already decided, so a
    /// missing SYSTEM principal or an unbuildable payload is logged and the
    /// result is not recorded.
    fn stage_result(
        &self,
        tenant: DataTenantId,
        execution_id: VerificationExecutionId,
        target: &DirectTarget,
        report: &VerifierReport,
        started_at: DateTime<Utc>,
    ) {
        let Some(principal) = target.system_principal else {
            tracing::warn!(%execution_id, "the tenant has no SYSTEM principal; the direct result is not recorded");
            return;
        };
        let (Some(subject), Ok(result_id)) = (
            target.subject.uid.as_ref(),
            VerificationResultId::new(execution_id.as_uuid()),
        ) else {
            return;
        };
        let ended_at = Utc::now();
        let input = match &target.implementation {
            VerifierImplementation::Drift(_) => RunInput::DriftWindow(DriftWindow {
                start: started_at,
                end: ended_at,
            }),
            VerifierImplementation::Eval(_) => RunInput::EvalRecord {
                record_id: execution_id.as_uuid().to_string(),
                event_time: started_at,
            },
        };
        let verifier_ref = CardRef {
            uid: None,
            ..target.verifier.clone()
        }
        .to_string();
        let verifier_version = target.verifier.version.to_string();
        let built = ResultPayloadBuilder::new(
            ResultRun {
                run_id: None,
                verifier_version: &verifier_version,
                subject_card_uid: subject,
                owner_card_uid: None,
                binding_id: None,
                trigger: None,
                input: &input,
            },
            &verifier_ref,
            result_id,
            ended_at,
            started_at,
            ended_at,
        )
        .build(report);
        match built {
            Ok(payload) => self.state.scribe_outbox.stage(
                tenant,
                ScribeWrite::Result {
                    payload,
                    attribution: VerifierAttribution {
                        verifier: target.verifier.clone(),
                        principal,
                    },
                },
            ),
            Err(error) => {
                tracing::warn!(%execution_id, %error, "the direct result cannot be encoded; it is not recorded");
            }
        }
    }

    /// Resolve a direct execution's Cards, check subject scope, and stage
    /// its decision.
    ///
    /// Reads both Cards in one read-only tenant transaction. A Card-bound
    /// caller without signed scope over the exact subject stages a denial;
    /// every other caller stages the allowed decision before the target is
    /// judged usable, so an unknown target still records who asked.
    ///
    /// # Errors
    /// Returns a registry error when a read fails,
    /// [`WyrdError::PermissionDeniedRbac`] without subject scope, and
    /// [`WyrdError::VerificationTargetNotFound`] for an unknown, inactive, or
    /// non-Verifier target.
    async fn resolve_direct(
        &self,
        caller: &Caller,
        request: &ExecuteVerificationRequest,
        resource: &str,
    ) -> Result<DirectTarget, WyrdError> {
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let verifier = Self::card(&mut conn, &request.verifier_uid).await?;
        let subject = Self::card(&mut conn, &request.subject_card_uid).await?;
        let system_principal = system_principal_id(&mut conn)
            .await
            .map_err(registry_db_error)?
            .map(PrincipalId::new);
        drop(conn);
        if let Some(subject) = &subject
            && caller.principal.card_ref().is_some()
            && !caller.principal.authorizes_card(&exact_ref(subject))
        {
            self.stage_decision(caller, EXECUTE, resource, AuditOutcome::Denied);
            return Err(Self::scope_denied(resource));
        }
        self.stage_decision(caller, EXECUTE, resource, AuditOutcome::Allowed);
        match (verifier, subject) {
            (Some(verifier), Some(subject)) if available(&verifier) && available(&subject) => {
                let reference = exact_ref(&verifier);
                match verifier.spec {
                    Spec::Verifier(spec) => Ok(DirectTarget {
                        verifier: reference,
                        implementation: spec.implementation,
                        subject: exact_ref(&subject),
                        system_principal,
                    }),
                    _ => Err(Self::target_not_found(request)),
                }
            }
            _ => Err(Self::target_not_found(request)),
        }
    }

    /// Stage one `evals:run` decision for `operation` on the process audit
    /// outbox without waiting for its commit.
    ///
    /// A decision the outbox cannot commit is logged and counted by the
    /// writer, never returned: audit does not block a run start or execution.
    fn stage_decision(
        &self,
        caller: &Caller,
        operation: &str,
        resource: &str,
        outcome: AuditOutcome,
    ) {
        self.state.scribe_outbox.stage(
            caller.data_tenant_id,
            audit::audit_event(
                caller,
                operation,
                resource,
                &Permission::eval_run().to_string(),
                outcome,
            ),
        );
    }

    /// The refusal of a Card-bound caller without scope over the subject.
    fn scope_denied(resource: &str) -> WyrdError {
        WyrdError::PermissionDeniedRbac {
            message: "the caller's Card scope does not cover the verified subject".to_owned(),
            details: serde_json::json!({ "resource": resource }),
        }
    }

    /// Read one Card of the caller's tenant, or `None` when it is absent.
    ///
    /// # Errors
    /// Returns a registry error other than not-found.
    async fn card(
        conn: &mut TenantConn<'_>,
        uid: &CardUid,
    ) -> Result<Option<ParsedCardRow>, WyrdError> {
        match get_card_by_uid(conn, uid).await {
            Ok(row) => Ok(Some(row)),
            Err(WyrdError::RegistryCardNotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The refusal of a target this tenant cannot execute.
    fn target_not_found(request: &ExecuteVerificationRequest) -> WyrdError {
        WyrdError::VerificationTargetNotFound {
            message: "no active Verifier and subject Card with these UIDs in the caller's tenant"
                .to_owned(),
            details: serde_json::json!({
                "verifier_uid": request.verifier_uid,
                "subject_card_uid": request.subject_card_uid,
            }),
        }
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
        Ok(caller.principal.authorizes_card(&exact_ref(&row)))
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
