//! GitHub delivery of one `pull_request` execution: its one Check and its
//! one `COMMENT` review, rendered from retained evidence by
//! [`Presentation::render`].
//!
//! Delivery runs in the background once the runner settles or parks an
//! execution, and again for every current PR execution when the server
//! starts. It reads only the execution's accepted state, its selected
//! result and the extraction its source was pinned by; it never recomputes
//! verification and never changes the verdict.
//!
//! GitHub REST has no idempotency key, so every write is lookup-before-
//! create: the Check is found by name, head and `external_id` and updated in
//! place, the review by its execution marker and never created twice once
//! found. A waiting delivery only creates the Check and never updates an
//! existing one, so a completed Check never returns to `in_progress`. After an unanswered write the next attempt looks up again first.
//! A write whose answer is lost and which is not visible yet may land twice;
//! that rare duplicate is accepted. Before each attempt the repository's
//! current access and the PR's current generation are rechecked: an
//! execution whose access was removed or whose generation was superseded is
//! recorded `skipped` and never written again.
//!
//! One outcome is recorded per accepted state revision as a `github_delivery`
//! row of `bifrost.verification.execution_events`, under a batch id derived
//! from the event, execution and revision, so a re-park at the same revision
//! delivers nothing and the first recorded outcome is final. Cancellation
//! settles only the queue row and writes no revision, so a cancelled
//! execution's outcome is a separate `github_delivery_cancelled` row at the
//! same revision: cancelling delivers once more and updates the Check to
//! `cancelled`. Public cancellation starts that delivery; a principal
//! revocation's is delivered by restart recovery. Inspection reads the
//! outcome the current status and revision need; without one it is
//! `pending`.

use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use chrono::Utc;
use secrecy::SecretString;
use serde::Deserialize as _;
use sha2::{Digest as _, Sha256};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::tables::DomainTable;
use vala_bifrost_redux::tables::staged::{Cell, StagedRow};
use vala_bifrost_redux::tables::verification::ExecutionEventsTable;
use wyrd_queue::variant::EncodedVariant;
use wyrd_spec::DataTenantId;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::managed_columns::{RUN_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID};
use wyrd_spec::verification::VerificationExecutionStatus;
use wyrd_spec::verification::execution::{
    GithubDelivery, GithubDeliveryReason, GithubDeliveryStatus, PullRequestActivation, StepResult,
};
use wyrd_spec::verification::report::{ReportSelection, select_report};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_executions::{PullRequestKey, VerifierExecutionQueue};

use super::app::{DeliveryFailure, GithubApp};
use super::extraction::output::PullRequestExtraction;
use super::presentation::{CheckRun, Completed, Presentation, Review, Settled, review_marker};
use super::service::GithubConnections;
use crate::components::verification::service::{Presented, VerificationControl};
use crate::scribe_outbox::{ScribeTable, ScribeWrite};
use crate::state::{AppState, registry_db_error};
use crate::verification::inputs::InputResolver;
use crate::verification::state::STATE_TABLE;

/// The `event` of a delivery outcome row.
const EVENT: &str = "github_delivery";

/// The `event` of a cancelled execution's delivery outcome row.
const CANCELLED_EVENT: &str = "github_delivery_cancelled";

/// Attempts of one Check or review write, and of one whole delivery whose
/// reads are unavailable.
const ATTEMPTS: u32 = 5;

/// Wait after the first unavailable attempt; doubles after each later one.
const BACKOFF: Duration = Duration::from_secs(1);

/// How long recording an outcome may wait for its Scribe acknowledgement.
const RECORD_DEADLINE: Duration = Duration::from_secs(30);

/// How long restart recovery waits before scanning again after a failed
/// scan.
const RECOVERY_RETRY: Duration = Duration::from_secs(60);

/// The schema every outcome row shares: the table's fields, then the
/// workload, subject, and run columns Scribe attributes rows by.
static SCHEMA: LazyLock<SchemaRef> = LazyLock::new(|| {
    let mut fields = ExecutionEventsTable::arrow_fields();
    fields.extend(
        [WORKLOAD_CARD_UID, SUBJECT_CARD_UID, RUN_ID]
            .map(|name| Field::new(name, DataType::Utf8, true)),
    );
    Arc::new(Schema::new(fields))
});

/// Why one delivery attempt did not finish.
enum Halt {
    /// Stop without writing again, recording this reason as `skipped`.
    Skipped(GithubDeliveryReason),
    /// Stop, recording this reason as `failed`.
    Failed(GithubDeliveryReason),
    /// Try again after the failure's wait.
    Again(DeliveryFailure),
}

impl From<DeliveryFailure> for Halt {
    /// A refusal fails delivery for good; every other failure is tried
    /// again.
    fn from(failure: DeliveryFailure) -> Self {
        match failure {
            DeliveryFailure::Refused(status) => {
                tracing::warn!(status, "GitHub refused a delivery write");
                Self::Failed(GithubDeliveryReason::GithubRefused)
            }
            failure => Self::Again(failure),
        }
    }
}

/// One GitHub write of a delivery.
#[derive(Clone, Copy)]
enum Write<'p> {
    /// The execution's Check, updated in place once found.
    Check(&'p CheckRun),
    /// The execution's review, created only while none carries its marker.
    Review(&'p Review),
}

impl Write<'_> {
    /// Look the write's target up with `token` and make the write unless
    /// it is already there, for `execution_id` on PR `number` of
    /// `repository`. A waiting Check is only created, never updated.
    ///
    /// Dropping the future after the write was sent does not undo it; the
    /// next attempt's lookup finds whatever landed.
    ///
    /// # Errors
    /// Returns the [`DeliveryFailure`] of the lookup or the write.
    async fn send(
        self,
        app: &GithubApp,
        token: &SecretString,
        execution_id: ExecutionId,
        (repository, number): (u64, u64),
    ) -> Result<(), DeliveryFailure> {
        match self {
            Self::Check(check) => {
                let external = execution_id.to_string();
                let found = app
                    .check_runs(token, repository, &check.head_sha, &check.name)
                    .await?
                    .into_iter()
                    .find(|run| run.external_id.as_deref() == Some(external.as_str()));
                match found {
                    // Waiting only creates the Check: an update could land
                    // after a terminal one and set it back to in_progress.
                    Some(_) if check.status != "completed" => Ok(()),
                    Some(run) => app.update_check_run(token, repository, run.id, check).await,
                    None => app
                        .create_check_run(token, repository, check)
                        .await
                        .map(drop),
                }
            }
            Self::Review(review) => {
                let marker = review_marker(execution_id);
                let bodies = app.review_bodies(token, repository, number).await?;
                if bodies.iter().any(|body| body.contains(&marker)) {
                    return Ok(());
                }
                app.create_review(token, repository, number, review).await
            }
        }
    }
}

/// One tenant's GitHub delivery.
pub(crate) struct GithubDeliveries<'a> {
    /// Process state: the App, the queue, the Scribe outbox and fences.
    app: &'a AppState,
    /// The tenant delivered for.
    tenant: DataTenantId,
}

impl<'a> GithubDeliveries<'a> {
    /// `tenant`'s delivery through `app`.
    pub(crate) const fn new(app: &'a AppState, tenant: DataTenantId) -> Self {
        Self { app, tenant }
    }

    /// Deliver `tenant`'s execution `execution_id` in the background,
    /// retrying a delivery whose reads are unavailable a bounded number of
    /// times. Does nothing when this server has no GitHub App.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime.
    pub(crate) fn spawn(app: AppState, tenant: DataTenantId, execution_id: ExecutionId) {
        if app.github.is_none() {
            return;
        }
        tokio::spawn(async move {
            GithubDeliveries::new(&app, tenant)
                .deliver_retried(execution_id)
                .await;
        });
    }

    /// Deliver every current PR execution of every tenant that has no
    /// outcome for its current state, once, retrying the scan until it
    /// succeeds, then idle until `shutdown`: a supervised worker runs until
    /// shutdown.
    // ponytail: one execution at a time; bound a concurrent pool if restart backlogs grow.
    pub(crate) async fn recover(app: AppState, shutdown: CancellationToken) {
        let Some(operator) = app.postgres.operator_pool() else {
            shutdown.cancelled().await;
            return;
        };
        let queue = VerifierExecutionQueue::default();
        loop {
            let scanned: Result<(), WyrdError> = async {
                let tenants = queue
                    .tenants_with_pull_requests(&operator)
                    .await
                    .map_err(registry_db_error)?;
                for tenant in tenants {
                    let mut conn = app.registry_tenant_conn(tenant).await?;
                    let executions = queue
                        .current_pull_request_executions(&mut conn)
                        .await
                        .map_err(registry_db_error)?;
                    conn.commit().await.map_err(registry_db_error)?;
                    for execution_id in executions {
                        if shutdown.is_cancelled() {
                            return Ok(());
                        }
                        GithubDeliveries::new(&app, tenant)
                            .deliver_retried(execution_id)
                            .await;
                    }
                }
                Ok(())
            }
            .await;
            match scanned {
                Ok(()) => break,
                Err(error) => {
                    tracing::warn!(%error, "GitHub delivery recovery scan failed; retrying");
                }
            }
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(RECOVERY_RETRY) => {}
            }
        }
        shutdown.cancelled().await;
    }

    /// [`Self::deliver`], tried again with backoff while it fails.
    async fn deliver_retried(&self, execution_id: ExecutionId) {
        let mut wait = BACKOFF;
        for attempt in 1..=ATTEMPTS {
            match self.deliver(execution_id).await {
                Ok(()) => break,
                Err(error) => {
                    tracing::warn!(%execution_id, attempt, %error, "GitHub delivery could not run");
                }
            }
            tokio::time::sleep(wait).await;
            wait = wait.saturating_mul(2);
        }
    }

    /// Deliver the current state of `execution_id` unless an outcome is
    /// already recorded for it, then record the outcome.
    ///
    /// Only a settled or waiting `pull_request` execution is delivered;
    /// anything else returns at once.
    ///
    /// # Errors
    /// Returns the read errors of [`VerificationControl::presented`] and
    /// [`Self::recorded`], a registry error when the Verifier cannot be
    /// read, and `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when the
    /// outcome is not acknowledged.
    #[tracing::instrument(skip(self), fields(tenant = %self.tenant))]
    pub(crate) async fn deliver(&self, execution_id: ExecutionId) -> Result<(), WyrdError> {
        let Some(presented) = VerificationControl::new(self.app)
            .presented(self.tenant, execution_id)
            .await?
        else {
            return Ok(());
        };
        if matches!(
            presented.status,
            VerificationExecutionStatus::Pending
                | VerificationExecutionStatus::Retrying
                | VerificationExecutionStatus::Running
        ) {
            return Ok(());
        }
        let records = InputResolver::new(&presented.query, None);
        if self
            .recorded(&records, execution_id, presented.revision, presented.status)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let presentation = self.render(execution_id, &presented).await?;
        let delivery = match self.send(execution_id, &presented, &presentation).await {
            Ok(()) => GithubDelivery {
                status: GithubDeliveryStatus::Delivered,
                reason: presentation.report_error,
                problems: presentation.problems,
            },
            Err(Halt::Skipped(reason)) => GithubDelivery {
                status: GithubDeliveryStatus::Skipped,
                reason: Some(reason),
                problems: Vec::new(),
            },
            Err(Halt::Failed(reason)) => GithubDelivery {
                status: GithubDeliveryStatus::Failed,
                reason: Some(reason),
                problems: presentation.problems,
            },
            Err(Halt::Again(_)) => GithubDelivery {
                status: GithubDeliveryStatus::Failed,
                reason: Some(GithubDeliveryReason::GithubUnavailable),
                problems: presentation.problems,
            },
        };
        tracing::info!(%execution_id, status = ?delivery.status, reason = ?delivery.reason, "GitHub delivery recorded");
        self.record(execution_id, &presented, &delivery).await
    }

    /// The outcome recorded for `execution_id` in `status` at `revision`,
    /// read through `records`: a cancelled execution's own outcome, else its
    /// revision's; `None` while none is recorded.
    ///
    /// Reads come only from Bifrost. An outcome Scribe accepted but no query
    /// sees yet is `None` here: delivery runs again, its lookup-before-create
    /// updates the same Check and review, and recording the outcome again is
    /// absorbed, or refused when different, under the outcome's batch id, so
    /// the first recorded outcome stays final.
    ///
    /// # Errors
    /// Returns the query error, or an internal error when the row is
    /// malformed.
    pub(crate) async fn recorded(
        &self,
        records: &InputResolver<'_>,
        execution_id: ExecutionId,
        revision: i64,
        status: VerificationExecutionStatus,
    ) -> Result<Option<GithubDelivery>, WyrdError> {
        records
            .delivery(execution_id, revision, event(status))
            .await
    }

    /// What `presented` shows on GitHub: a completed execution's report is
    /// selected from its final step results against its Verifier and
    /// rendered against the extraction among them.
    ///
    /// # Errors
    /// Returns a registry error when the Verifier cannot be read.
    async fn render(
        &self,
        execution_id: ExecutionId,
        presented: &Presented,
    ) -> Result<Presentation, WyrdError> {
        let mut conn = self.app.registry_tenant_conn(self.tenant).await?;
        let verifier = get_card_by_uid(&mut conn, &presented.queued.verifier_card_uid).await?;
        conn.commit().await.map_err(registry_db_error)?;
        let name = format!("wyrd/{}", verifier.name);
        let (verdict, steps): (_, &[StepResult]) = match &presented.outcome {
            Some((verdict, steps)) => (Some(*verdict), steps),
            None => (None, &[]),
        };
        let selection = match &verifier.spec {
            Spec::Verifier(spec) => select_report(spec, steps),
            _ => ReportSelection::None,
        };
        let extraction = steps
            .iter()
            .filter_map(|step| step.output.as_ref())
            .find_map(|output| PullRequestExtraction::deserialize(output).ok());
        Ok(Presentation::render(
            &name,
            execution_id,
            &presented.activation.head.sha,
            Settled {
                status: presented.status,
                verdict,
            },
            &presented.waiting,
            Some(Completed {
                selection: &selection,
                extraction: extraction.as_ref(),
            }),
        ))
    }

    /// Put `presentation` on GitHub: the Check, then the review when there
    /// is one, each lookup-before-create.
    ///
    /// # Errors
    /// Returns the [`Halt`] that stopped delivery.
    async fn send(
        &self,
        execution_id: ExecutionId,
        presented: &Presented,
        presentation: &Presentation,
    ) -> Result<(), Halt> {
        let activation = &presented.activation;
        self.attempt(execution_id, activation, Write::Check(&presentation.check))
            .await?;
        match &presentation.review {
            Some(review) => {
                self.attempt(execution_id, activation, Write::Review(review))
                    .await
            }
            None => Ok(()),
        }
    }

    /// Make `write` on the PR of `activation` at most [`ATTEMPTS`] times,
    /// with a token minted on current access, rechecking access and
    /// generation before each attempt and waiting out each cooldown or
    /// backoff between them.
    ///
    /// Dropping the future mid-attempt leaves any write already sent in
    /// place and records nothing; a later delivery looks it up first.
    ///
    /// # Errors
    /// Returns [`Halt::Skipped`] once access or generation is gone,
    /// [`Halt::Failed`] on a refusal, and the last [`Halt::Again`] once
    /// every attempt was unavailable.
    async fn attempt(
        &self,
        execution_id: ExecutionId,
        activation: &PullRequestActivation,
        write: Write<'_>,
    ) -> Result<(), Halt> {
        let app = self
            .app
            .github
            .as_deref()
            .ok_or(Halt::Skipped(GithubDeliveryReason::AccessRemoved))?;
        let repository = repository(activation)?;
        let mut wait = BACKOFF;
        let mut last = DeliveryFailure::Unavailable;
        for _ in 0..ATTEMPTS {
            let outcome = match self.allowed(activation, repository).await {
                Ok(token) => write
                    .send(app, &token, execution_id, (repository, activation.number))
                    .await
                    .map_err(Halt::from),
                Err(halt) => Err(halt),
            };
            let failure = match outcome {
                Ok(()) => return Ok(()),
                Err(Halt::Again(failure)) => failure,
                Err(halt) => return Err(halt),
            };
            let pause = if let DeliveryFailure::Cooldown(cooldown) = failure {
                cooldown
            } else {
                let pause = wait;
                wait = wait.saturating_mul(2);
                pause
            };
            last = failure;
            tokio::time::sleep(pause).await;
        }
        Err(Halt::Again(last))
    }

    /// A token for `repository`, the repository of `activation`, minted
    /// only while the tenant still has access to it and the activation is
    /// still its PR's current, active generation.
    ///
    /// # Errors
    /// Returns [`Halt::Skipped`] with `access_removed` or `superseded`, and
    /// [`Halt::Again`] when either cannot be read now.
    async fn allowed(
        &self,
        activation: &PullRequestActivation,
        repository: u64,
    ) -> Result<SecretString, Halt> {
        let again = |error: &dyn std::fmt::Display| {
            tracing::warn!(%error, "GitHub delivery access or generation cannot be read now");
            Halt::Again(DeliveryFailure::Unavailable)
        };
        let token = match GithubConnections::new(self.app)
            .installation_token(self.tenant, repository)
            .await
        {
            Ok(token) => token,
            Err(
                WyrdError::GithubNotConfigured { .. }
                | WyrdError::GithubNotConnected { .. }
                | WyrdError::GithubRepositoryInaccessible { .. },
            ) => return Err(Halt::Skipped(GithubDeliveryReason::AccessRemoved)),
            Err(error) => return Err(again(&error)),
        };
        let mut conn = self
            .app
            .registry_tenant_conn(self.tenant)
            .await
            .map_err(|error| again(&error))?;
        let history = VerifierExecutionQueue::default()
            .lock_pull_request(
                &mut conn,
                PullRequestKey {
                    host: &activation.host,
                    repository_id: &activation.repository_id,
                    number: activation.number,
                },
            )
            .await
            .map_err(|error| again(&error))?;
        conn.commit().await.map_err(|error| again(&error))?;
        let current = history.current.is_some_and(|generation| {
            !generation.deactivated && generation.activation.generation == activation.generation
        });
        if !current {
            return Err(Halt::Skipped(GithubDeliveryReason::Superseded));
        }
        Ok(token)
    }

    /// Record `delivery` as the outcome of `presented`'s current state and
    /// wait for its Scribe acknowledgement. A second record of the same
    /// revision is absorbed or refused by its batch fence; the first stays.
    ///
    /// Once staged, the row is flushed even when this future is dropped or
    /// times out waiting; a delivery that then runs again finds it through
    /// its fence or is absorbed by it.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` when the outcome
    /// gets no reserved slot or is not acknowledged in time, and an internal
    /// error when its row cannot be built.
    async fn record(
        &self,
        execution_id: ExecutionId,
        presented: &Presented,
        delivery: &GithubDelivery,
    ) -> Result<(), WyrdError> {
        let row = row(execution_id, presented, delivery)?;
        let writer = || {
            WyrdError::from(BifrostError::WriterUnavailable {
                table: STATE_TABLE.to_owned(),
            })
        };
        let deadline = Instant::now() + RECORD_DEADLINE;
        let slot = self
            .app
            .bifrost
            .scribe_outbox()
            .reserve_until(1, deadline)
            .await
            .map_err(|_| writer())?;
        let (sender, acked) = oneshot::channel();
        slot.stage(
            self.tenant,
            ScribeWrite::Fenced {
                table: ScribeTable::ExecutionEvents,
                batch: key(event(presented.status), execution_id, presented.revision),
                row,
                attribution: presented.queued.attribution,
                acked: Mutex::new(Some(sender)),
            },
        );
        match tokio::time::timeout_at(deadline.into(), acked).await {
            Ok(Ok(())) => Ok(()),
            _ => Err(writer()),
        }
    }
}

/// The outcome event of an execution in `status`: a cancelled execution
/// has its own, apart from its revision's.
fn event(status: VerificationExecutionStatus) -> &'static str {
    if status == VerificationExecutionStatus::Cancelled {
        CANCELLED_EVENT
    } else {
        EVENT
    }
}

/// The batch identity of the `event` outcome of `execution`'s state at
/// `revision`, apart from every state revision's and the other event's.
fn key(event: &str, execution: ExecutionId, revision: i64) -> Uuid {
    let digest = Sha256::new()
        .chain_update(event.as_bytes())
        .chain_update(execution.as_uuid().as_bytes())
        .chain_update(revision.to_be_bytes())
        .finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Builder::from_custom_bytes(bytes).into_uuid()
}

/// The numeric GitHub repository id of `activation`.
///
/// # Errors
/// Returns [`Halt::Skipped`] with `access_removed` for an id that is not
/// numeric, which no GitHub repository has.
fn repository(activation: &PullRequestActivation) -> Result<u64, Halt> {
    activation
        .repository_id
        .as_str()
        .parse()
        .map_err(|_| Halt::Skipped(GithubDeliveryReason::AccessRemoved))
}

/// `delivery` as the outcome row of `presented`'s current state.
///
/// # Errors
/// Returns an internal error when a value does not fit its column.
fn row(
    execution_id: ExecutionId,
    presented: &Presented,
    delivery: &GithubDelivery,
) -> Result<StagedRow, WyrdError> {
    let text = |value: serde_json::Value| value.as_str().map(str::to_owned);
    let status = text(serde_json::json!(delivery.status));
    let reason = delivery
        .reason
        .and_then(|reason| text(serde_json::json!(reason)));
    let mut value = if delivery.problems.is_empty() {
        None
    } else {
        Some(
            EncodedVariant::from_serialize(&delivery.problems)
                .map_err(|violation| WyrdError::from(violation.into_error("value", 0)))?,
        )
    };
    let (event_id, execution, verifier) = (
        Uuid::now_v7().to_string(),
        execution_id.to_string(),
        presented.queued.verifier_card_uid.to_string(),
    );
    let cells = SCHEMA
        .fields()
        .iter()
        .map(|field| match field.name().as_str() {
            "event_id" => Cell::text(Some(event_id.as_str())),
            "execution_id" => Cell::text(Some(execution.as_str())),
            "sequence" => Cell::Int(Some(presented.revision)),
            "event" => Cell::text(Some(event(presented.status))),
            "status" => Cell::text(status.as_deref()),
            "verifier_card_uid" => Cell::text(Some(verifier.as_str())),
            "reason" => Cell::text(reason.as_deref()),
            "value" => Cell::Variant(value.take()),
            "occurred_at" => Cell::Micros(Some(Utc::now().timestamp_micros())),
            "item_index" | "iteration_index" | "attempt" | "size_bytes" | "file_count" => {
                Cell::Int(None)
            }
            // Shared dimensions and the workload, subject, and run columns.
            _ => Cell::text(None::<&str>),
        })
        .collect();
    StagedRow::new(Arc::clone(&SCHEMA), cells)
        .map_err(|error| WyrdError::internal(format!("delivery row: {error}")))
}

/// Unit proofs of outcome identity.
#[cfg(test)]
mod tests {
    use wyrd_spec::ids::ExecutionId;

    use super::{CANCELLED_EVENT, EVENT, key};

    /// Proves an outcome's batch identity is stable for one event,
    /// execution and revision and differs for another of any of them.
    ///
    /// # Panics
    /// Panics when two identities that must match differ, or two that must
    /// differ match.
    #[test]
    fn outcome_identity_is_per_event_execution_and_revision() {
        let execution = ExecutionId::new_v7();
        assert_eq!(key(EVENT, execution, 3), key(EVENT, execution, 3));
        assert_ne!(key(EVENT, execution, 3), key(EVENT, execution, 4));
        assert_ne!(key(EVENT, execution, 3), key(CANCELLED_EVENT, execution, 3));
        assert_ne!(
            key(EVENT, execution, 3),
            key(EVENT, ExecutionId::new_v7(), 3)
        );
    }
}
