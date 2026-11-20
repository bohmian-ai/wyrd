//! Approval decisions: the durable acceptance of a person's decision of one
//! approval occurrence, and the read-back the runner and inspection share.
//!
//! One decision is one `bifrost.verification.approvals` row, written through
//! Scribe as its own batch under a batch id its approval id derives. The
//! batch fence, committed before Scribe acknowledges, is the authoritative
//! record that the occurrence was decided: an unfenced decision is absent,
//! and a fenced one whose row a query cannot see yet is unavailable, never
//! absent. The first accepted decision is final. Nothing is mirrored in
//! PostgreSQL; the queue only learns that the execution is due.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use tokio::sync::oneshot;
use uuid::Uuid;
use vala_bifrost_redux::contracts::committed_batches;
use vala_bifrost_redux::tables::DomainTable;
use vala_bifrost_redux::tables::staged::{Cell, StagedRow};
use vala_bifrost_redux::tables::verification::ApprovalsTable;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ApprovalId, CardUid, ExecutionId};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::managed_columns::{RUN_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID};
use wyrd_spec::verification::execution::{ApprovalReceipt, ApprovalStatus};

use super::inputs::InputResolver;
use crate::scribe_outbox::{ScribeTable, ScribeWrite, fenced_batch_id};
use crate::state::AppState;

/// The table every decision is a row of, and the logical table of every
/// decision's batch fence.
pub(crate) const APPROVALS_TABLE: &str = "bifrost.verification.approvals";

/// The one schema every staged decision row shares: the table's fields,
/// then the workload, subject, and run columns Scribe attributes rows by.
static SCHEMA: LazyLock<SchemaRef> = LazyLock::new(|| {
    let mut fields = ApprovalsTable::arrow_fields();
    fields.extend(
        [WORKLOAD_CARD_UID, SUBJECT_CARD_UID, RUN_ID]
            .map(|name| Field::new(name, DataType::Utf8, true)),
    );
    Arc::new(Schema::new(fields))
});

/// One tenant's approval decision custody.
pub(crate) struct ApprovalDecisions<'a> {
    /// Process state: the Scribe outbox and the fence store.
    app: &'a AppState,
    /// The tenant the decisions belong to.
    tenant: DataTenantId,
}

impl<'a> ApprovalDecisions<'a> {
    /// `tenant`'s custody through `app`.
    pub(crate) const fn new(app: &'a AppState, tenant: DataTenantId) -> Self {
        Self { app, tenant }
    }

    /// The accepted decision of each of `execution_id`'s `approvals` that
    /// has one, read through `records`; an undecided occurrence is absent.
    ///
    /// # Errors
    /// Returns [`WyrdError::ServiceUnavailable`] when the fences cannot be
    /// read, or an accepted decision is not visible yet, so a decision is
    /// never mistaken for absence; the query error or an internal error when
    /// a row cannot be read or is malformed.
    pub(crate) async fn decided(
        &self,
        records: &InputResolver<'_>,
        execution_id: ExecutionId,
        approvals: &[ApprovalId],
    ) -> Result<BTreeMap<ApprovalId, ApprovalReceipt>, WyrdError> {
        if approvals.is_empty() {
            return Ok(BTreeMap::new());
        }
        let fences: Vec<Uuid> = approvals.iter().map(|id| self.fence(*id)).collect();
        let mut conn = self
            .app
            .postgres
            .vala()
            .tenant_conn(self.tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let fenced = committed_batches(&mut conn, APPROVALS_TABLE, &fences)
            .await
            .map_err(|error| unavailable(&error))?;
        drop(conn);
        let held: Vec<ApprovalId> = approvals
            .iter()
            .zip(&fences)
            .filter(|(_, fence)| fenced.contains(fence))
            .map(|(id, _)| *id)
            .collect();
        if held.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut rows = records.decisions(execution_id).await?;
        held.into_iter()
            .map(|id| {
                let decided = rows
                    .remove(&id)
                    .ok_or_else(|| unavailable(&"an accepted decision is not visible yet"))?;
                Ok((id, decided))
            })
            .collect()
    }

    /// Accept `receipt`, a decision of `execution_id`'s approval occurrence
    /// whose Verifier version is `verifier`, retained under the execution's
    /// exact `subject`, under `attribution`, and wait
    /// for its own Scribe acknowledgement until `deadline`. A resubmission
    /// is absorbed by its batch fence; a different decision is refused.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when the decision
    /// gets no reserved slot, or is rejected, dropped, or unacknowledged,
    /// before `deadline`; the outcome is unknown and the caller rechecks the
    /// fence. Returns an internal error when the receipt is not a decision
    /// or its row cannot be built.
    pub(crate) async fn accept(
        &self,
        attribution: Attribution,
        (execution_id, verifier, subject): (ExecutionId, &CardUid, &CardUid),
        receipt: &ApprovalReceipt,
        deadline: Instant,
    ) -> Result<(), WyrdError> {
        let row = row((execution_id, verifier, subject), receipt)?;
        let writer = || {
            WyrdError::from(BifrostError::WriterUnavailable {
                table: APPROVALS_TABLE.to_owned(),
            })
        };
        let outbox = self.app.bifrost.scribe_outbox();
        let slot = outbox.reserve_until(1, deadline).await.map_err(|full| {
            tracing::warn!(
                admitted = full.admitted,
                limit = full.limit,
                "an approval decision got no reserved slot"
            );
            writer()
        })?;
        let (sender, acked) = oneshot::channel();
        slot.stage(
            self.tenant,
            ScribeWrite::Fenced {
                table: ScribeTable::Approvals,
                batch: receipt.approval_id.as_uuid(),
                row,
                attribution,
                acked: Mutex::new(Some(sender)),
            },
        );
        match tokio::time::timeout_at(deadline.into(), acked).await {
            Ok(Ok(())) => Ok(()),
            outcome => {
                tracing::warn!(
                    timed_out = outcome.is_err(),
                    "an approval decision was not acknowledged"
                );
                Err(writer())
            }
        }
    }

    /// The batch fence of the decision of `approval_id`.
    fn fence(&self, approval_id: ApprovalId) -> Uuid {
        fenced_batch_id(self.tenant, ScribeTable::Approvals, approval_id.as_uuid())
    }
}

/// `receipt` as its staged row.
///
/// # Errors
/// Returns an internal error when the receipt is pending, expired, or lacks
/// its actor or time, or a value does not fit its column.
fn row(
    (execution_id, verifier, subject): (ExecutionId, &CardUid, &CardUid),
    receipt: &ApprovalReceipt,
) -> Result<StagedRow, WyrdError> {
    let decision = match receipt.status {
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Rejected => "rejected",
        ApprovalStatus::Pending | ApprovalStatus::Expired => {
            return Err(WyrdError::internal(
                "an approval decision is approved or rejected",
            ));
        }
    };
    let (Some(actor), Some(decided_at)) = (receipt.actor_id, receipt.decided_at) else {
        return Err(WyrdError::internal(
            "an approval decision names its actor and time",
        ));
    };
    let (approval, execution, verifier, actor) = (
        receipt.approval_id.to_string(),
        execution_id.to_string(),
        verifier.to_string(),
        actor.to_string(),
    );
    let cells = SCHEMA
        .fields()
        .iter()
        .map(|field| match field.name().as_str() {
            "approval_id" => Cell::text(Some(approval.as_str())),
            "execution_id" => Cell::text(Some(execution.as_str())),
            "verifier_card_uid" => Cell::text(Some(verifier.as_str())),
            "decision" => Cell::text(Some(decision)),
            "actor_id" => Cell::text(Some(actor.as_str())),
            "reason" => Cell::text(receipt.reason.as_deref()),
            "decided_at" => Cell::Micros(Some(decided_at.timestamp_micros())),
            SUBJECT_CARD_UID => Cell::text(Some(subject.as_str())),
            // Shared dimensions and the workload and run columns.
            _ => Cell::text(None::<&str>),
        })
        .collect();
    StagedRow::new(Arc::clone(&SCHEMA), cells)
        .map_err(|error| WyrdError::internal(format!("approval row: {error}")))
}

/// An unreadable fence or invisible accepted decision as a retryable
/// unavailability. The cause is traced here and never reaches the caller.
fn unavailable(error: &dyn std::fmt::Display) -> WyrdError {
    tracing::warn!(cause = %error, "approval decisions cannot be read");
    WyrdError::ServiceUnavailable {
        message: "approval decisions cannot be read now; try again".to_owned(),
        details: serde_json::json!({ "table": APPROVALS_TABLE }),
    }
}

#[cfg(test)]
mod tests {
    use wyrd_spec::error::WyrdError;

    use super::{APPROVALS_TABLE, unavailable};

    /// Proves an unreadable decision answers a retryable unavailability that
    /// names the table but never the internal cause.
    ///
    /// # Panics
    /// Panics when the error is not `ServiceUnavailable`, loses its table
    /// detail, or carries the cause.
    #[test]
    fn an_unavailable_read_hides_its_cause() {
        let WyrdError::ServiceUnavailable { message, details } =
            unavailable(&"connection to 10.0.0.7:5432 refused")
        else {
            panic!("an unreadable decision is unavailable");
        };
        assert!(!message.contains("10.0.0.7"), "{message}");
        assert_eq!(details["table"], APPROVALS_TABLE);
    }
}
