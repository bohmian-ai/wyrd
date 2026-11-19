//! Approval decisions: accepting a person's decision of one pending approval
//! occurrence of a queued execution and making the execution due.

use std::time::Instant;

use chrono::{SubsecRound, Utc};
use wyrd_runtime::permission::Action;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ApprovalId, ExecutionId};
use wyrd_spec::verification::execution::{ApprovalReceipt, ApprovalStatus, DecideApprovalRequest};
use wyrd_sql::queries::verifier_executions::VerifierExecutionQueue;

use super::inspection::execution_not_found;
use super::{DECIDE_APPROVAL, VerificationControl};
use crate::components::auth::Caller;
use crate::state::registry_db_error;
use crate::verification::approvals::ApprovalDecisions;
use crate::verification::cache::Db;
use crate::verification::inputs::InputResolver;

/// The longest reason, in characters, a decision may record.
const REASON_MAX_CHARS: usize = 4096;

impl VerificationControl<'_> {
    /// Accept `request` as the caller's decision of pending approval
    /// occurrence `approval_id` of queued execution `execution_id`, and
    /// return its receipt once the decision is durable.
    ///
    /// The execution is read as an inspection reads it, but authorized only
    /// by `verifier:approve` on its exact Verifier, audited; the occurrence
    /// must be one its accepted state recorded. A decision
    /// already accepted for the occurrence replays its receipt when the same
    /// actor sends the same decision and reason, and refuses otherwise; a
    /// replay makes the execution due again, as a decision does. An
    /// undecided occurrence whose step a speculative guard discarded or
    /// blocked is refused as expired, since no decision of it is used. A new
    /// decision is accepted only while the execution is unfinished and before
    /// its original deadline, the runtime timeout capped by the Verifier's
    /// authored timeout, both from admission, written through Scribe, and returned once
    /// acknowledged; the execution is then made due so it continues under
    /// the same id. A lost wake loses nothing: the parked execution rechecks
    /// its decisions when its bounded park ends.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationWrongTarget`] on a target that does
    /// not run queued work; [`WyrdError::VerificationInputInvalid`] for an
    /// oversized reason; [`WyrdError::PermissionDeniedRbac`] without `verifier:approve`
    /// on the execution's exact Verifier; the read errors of an inspection;
    /// [`WyrdError::VerificationApprovalNotFound`] when the execution
    /// recorded no such occurrence; [`WyrdError::VerificationTargetNotFound`] when the
    /// execution's exact Verifier and subject no longer resolve for a new
    /// decision; [`WyrdError::VerificationApprovalNotPending`] when another
    /// decision was accepted or the execution ended, expired, or was
    /// superseded; and [`WyrdError::ServiceUnavailable`] or
    /// `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when the decision cannot be
    /// read or acknowledged now, so the caller decides again.
    #[tracing::instrument(
        name = "verification.approval.decide",
        skip_all,
        fields(%execution_id, %approval_id, decision = ?request.decision)
    )]
    pub(crate) async fn decide_approval(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        approval_id: ApprovalId,
        request: DecideApprovalRequest,
    ) -> Result<ApprovalReceipt, WyrdError> {
        self.state.settings.role.require_queued(DECIDE_APPROVAL)?;
        if request
            .reason
            .as_ref()
            .is_some_and(|reason| reason.chars().count() > REASON_MAX_CHARS)
        {
            return Err(WyrdError::VerificationInputInvalid {
                message: format!("reason exceeds {REASON_MAX_CHARS} characters"),
                details: serde_json::json!({ "field": "reason", "max_chars": REASON_MAX_CHARS }),
            });
        }
        let retained = self
            .retained(caller, execution_id, DECIDE_APPROVAL, Action::Approve)
            .await?;
        let Some(recorded) = retained
            .state
            .approvals
            .iter()
            .find(|recorded| recorded.approval_id == approval_id)
        else {
            return Err(WyrdError::VerificationApprovalNotFound {
                message: "the execution records no approval occurrence with this ID".to_owned(),
                details: serde_json::json!({
                    "execution_id": execution_id,
                    "approval_id": approval_id,
                }),
            });
        };
        let identity = retained
            .identity()
            .ok_or_else(|| execution_not_found(execution_id))?;
        let verifier = identity
            .verifier
            .uid
            .clone()
            .ok_or_else(|| execution_not_found(execution_id))?;
        let subject = identity
            .subject
            .uid
            .clone()
            .ok_or_else(|| execution_not_found(execution_id))?;
        let tenant = caller.data_tenant_id;
        let custody = ApprovalDecisions::new(self.state, tenant);
        if let Some(held) = retained.decisions.get(&approval_id) {
            let receipt = replay(held, caller, &request)?;
            self.wake_decided(tenant, execution_id).await;
            return Ok(receipt);
        }
        if retained.withdrawn(recorded) {
            return Err(not_pending(approval_id, ApprovalStatus::Expired));
        }
        let mut conn = self
            .state
            .postgres
            .wyrd()
            .tenant_conn(tenant)
            .await
            .map_err(registry_db_error)?;
        // The runner's ceiling: the runtime timeout, capped by the exact
        // Verifier's authored timeout, both counted from admission. A target
        // that no longer resolves has no ceiling to decide under.
        let runtime = self.state.verification.execution_timeout();
        let target = self
            .state
            .verification
            .verifier_cards()
            .resolve(Db::Conn(conn.transaction()), tenant, &verifier, &subject)
            .await
            .map_err(registry_db_error)?
            .map_err(|refusal| {
                tracing::info!(?refusal, "the execution's exact target no longer resolves");
                WyrdError::VerificationTargetNotFound {
                    message:
                        "no active Verifier and subject Card with these UIDs in the caller's tenant"
                            .to_owned(),
                    details: serde_json::json!({
                        "verifier_card_uid": verifier,
                        "card_uid": subject,
                    }),
                }
            })?;
        let ceiling = target
            .authored_timeout()
            .map_or(runtime, |authored| authored.min(runtime));
        let timeout = chrono::Duration::from_std(ceiling)
            .map_err(|error| WyrdError::internal(format!("execution timeout: {error}")))?;
        let open = self
            .queue
            .open_for_decision(&mut conn, execution_id, timeout)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        if open != Some(true) {
            return Err(not_pending(approval_id, ApprovalStatus::Expired));
        }
        let receipt = ApprovalReceipt {
            approval_id,
            status: request.decision.into(),
            actor_id: Some(caller.attribution().act_principal_id),
            // Bifrost keeps microseconds; a fresh receipt states what a replay reads.
            decided_at: Some(Utc::now().trunc_subsecs(6)),
            reason: request.reason.clone(),
        };
        let deadline = Instant::now() + self.state.settings.limits.timeout / 2;
        let accepted = custody
            .accept(
                caller.attribution(),
                (execution_id, &verifier),
                &receipt,
                deadline,
            )
            .await;
        if let Err(error) = accepted {
            // A concurrent decider may have won the fence first; its
            // decision is the receipt this request replays or conflicts with.
            let records = InputResolver::new(&retained.query, None);
            let raced = custody
                .decided(&records, execution_id, &[approval_id])
                .await?;
            let receipt = raced
                .get(&approval_id)
                .ok_or(error)
                .and_then(|held| replay(held, caller, &request))?;
            self.wake_decided(tenant, execution_id).await;
            return Ok(receipt);
        }
        self.wake_decided(tenant, execution_id).await;
        Ok(receipt)
    }

    /// Make `execution_id` due now. A failure is logged: the parked
    /// execution rechecks its decisions when its bounded park ends.
    async fn wake_decided(&self, tenant: wyrd_spec::DataTenantId, execution_id: ExecutionId) {
        let woken = async {
            let mut conn = self.state.postgres.wyrd().tenant_conn(tenant).await?;
            let woken = VerifierExecutionQueue::default()
                .wake_decided(&mut conn, execution_id)
                .await?;
            conn.commit().await?;
            Ok::<_, wyrd_sql::SqlError>(woken)
        };
        match woken.await {
            Ok(woken) => tracing::info!(woken, "the decided execution was made due"),
            Err(error) => {
                tracing::warn!(%error, "waking the decided execution failed; its park recheck repairs it");
            }
        }
    }
}

/// The receipt of `held`, an accepted decision, when the actor who decided
/// it, `caller`'s acting principal rather than any subject it acts for, sends
/// the same decision and reason again.
///
/// # Errors
/// Returns [`WyrdError::VerificationApprovalNotPending`] for any other actor,
/// decision, or reason.
fn replay(
    held: &ApprovalReceipt,
    caller: &Caller,
    request: &DecideApprovalRequest,
) -> Result<ApprovalReceipt, WyrdError> {
    if held.actor_id == Some(caller.attribution().act_principal_id)
        && held.status == ApprovalStatus::from(request.decision)
        && held.reason == request.reason
    {
        Ok(held.clone())
    } else {
        Err(not_pending(held.approval_id, held.status))
    }
}

/// The refusal of a decision of `approval_id`, which is `status`, not pending.
fn not_pending(approval_id: ApprovalId, status: ApprovalStatus) -> WyrdError {
    WyrdError::VerificationApprovalNotPending {
        message: format!("approval {approval_id} is no longer pending"),
        details: serde_json::json!({ "approval_id": approval_id, "status": status }),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::{DelegationStep, Principal, PrincipalKind, PrincipalRef};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::ApprovalId;
    use wyrd_spec::verification::execution::{
        ApprovalDecision, ApprovalReceipt, ApprovalStatus, DecideApprovalRequest,
    };

    use super::replay;
    use crate::components::auth::Caller;

    /// A new user of `tenant` making a request of its own.
    fn person(tenant: DataTenantId) -> Caller {
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                Vec::new(),
                PermissionSet::default(),
            ),
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            delegation_chain: Vec::new(),
            executed_by: None,
        }
    }

    /// `actor` acting for the fixture user `subject`.
    fn acting(subject: &Caller, actor: &Caller) -> Caller {
        Caller {
            delegation_chain: vec![DelegationStep {
                principal: PrincipalRef::from_principal(&actor.principal),
            }],
            ..subject.clone()
        }
    }

    /// Proves a decision replays only for the actor who made it: another
    /// actor for the same subject, and the subject directly, are refused.
    ///
    /// # Panics
    /// Panics when the deciding actor's retry is refused, or another
    /// identity's identical decision replays.
    #[test]
    fn only_the_deciding_actor_replays_a_decision() {
        let tenant = DataTenantId::new_v7();
        let subject = person(tenant);
        let (first, second) = (person(tenant), person(tenant));
        let actor = acting(&subject, &first);
        let request = DecideApprovalRequest {
            decision: ApprovalDecision::Approved,
            reason: Some("reviewed".to_owned()),
        };
        let held = ApprovalReceipt {
            approval_id: ApprovalId::new_v7(),
            status: ApprovalStatus::Approved,
            actor_id: Some(actor.attribution().act_principal_id),
            decided_at: Some(Utc::now()),
            reason: request.reason.clone(),
        };
        assert_eq!(held.actor_id, Some(first.principal.id));
        assert_ne!(held.actor_id, Some(subject.principal.id));
        assert_eq!(
            replay(&held, &actor, &request).expect("the actor replays"),
            held
        );
        for other in [acting(&subject, &second), subject] {
            assert!(matches!(
                replay(&held, &other, &request),
                Err(WyrdError::VerificationApprovalNotPending { .. })
            ));
        }
    }
}
