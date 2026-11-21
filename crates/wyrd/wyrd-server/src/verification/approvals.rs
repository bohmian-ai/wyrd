//! Approval decisions: the durable acceptance of a person's decision of one
//! approval occurrence, and the read-back the runner and inspection share.
//!
//! One decision is one `bifrost.verification.approvals` row, written through
//! Scribe as its own batch under a batch id its approval id derives. The
//! row is the one record that the occurrence was decided, and reads come
//! only from Bifrost. A decision Scribe accepted but no query sees yet reads
//! as undecided: the execution stays parked until a later read sees it, and
//! a different decision for it is refused under the same batch id, so the
//! first accepted decision is final. Nothing is mirrored in PostgreSQL; the
//! queue only learns that the execution is due.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use tokio::sync::oneshot;
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
use crate::scribe_outbox::{ScribeTable, ScribeWrite};
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
    /// A decision no query sees yet is absent here; see the module docs.
    ///
    /// # Errors
    /// Returns the query error, or an internal error when a row is
    /// malformed.
    pub(crate) async fn decided(
        &self,
        records: &InputResolver<'_>,
        execution_id: ExecutionId,
        approvals: &[ApprovalId],
    ) -> Result<BTreeMap<ApprovalId, ApprovalReceipt>, WyrdError> {
        if approvals.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut rows = records.decisions(execution_id).await?;
        Ok(approvals
            .iter()
            .filter_map(|id| rows.remove_entry(id))
            .collect())
    }

    /// Accept `receipt`, a decision of `execution_id`'s approval occurrence
    /// whose Verifier version is `verifier`, under `attribution`, and wait
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
        (execution_id, verifier): (ExecutionId, &CardUid),
        receipt: &ApprovalReceipt,
        deadline: Instant,
    ) -> Result<(), WyrdError> {
        let row = row(execution_id, verifier, receipt)?;
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
}

/// `receipt` as its staged row.
///
/// # Errors
/// Returns an internal error when the receipt is pending, expired, or lacks
/// its actor or time, or a value does not fit its column.
fn row(
    execution_id: ExecutionId,
    verifier: &CardUid,
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
            // Shared dimensions and the workload, subject, and run columns.
            _ => Cell::text(None::<&str>),
        })
        .collect();
    StagedRow::new(Arc::clone(&SCHEMA), cells)
        .map_err(|error| WyrdError::internal(format!("approval row: {error}")))
}
