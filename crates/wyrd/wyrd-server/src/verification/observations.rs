//! Post-acknowledgement Eval run requests, staged on the generic outbox.
//!
//! Gate hands every Eval observation frame Scribe has durably acknowledged to
//! [`ObservationEnqueue`], which derives each row's exact `record_id`,
//! subject, and committed `wyrd_event_time`, tags it with the Card the writing
//! principal is bound to, and stages one run request per record on the
//! [`ObservationRunOutbox`] without waiting. A Card-bound writer activates only
//! the bindings its Card owns on the subject; a writer bound to no Card, such
//! as a user, activates every `observations_ready` binding of the subject. Gate
//! hands over only the acknowledgement that first committed a batch, so a
//! suppressed replay never queues and its later receipt instant can never be
//! frozen as a run's event time.
//!
//! The generic [`Outbox`] owns the unbounded queue, per-tenant batching,
//! retry with backoff, and shutdown; [`ObservationRunSink`] is its one durable
//! call, [`VerifierRunQueue::enqueue_observation_batch`], a multi-row insert
//! keyed by tenant, binding, and record, so a repeated write inserts nothing.
//! Losses are counted in `outbox_events_lost_total{outbox="eval_run_requests"}`.

use std::sync::Arc;

use bytes::Bytes;
use vala_bifrost_redux::gate::{AuthContext, ObservationAck};
use vala_bifrost_redux::tables::EvalObservationsTable;
use wyrd_runtime::outbox::{Outbox, OutboxSink};
use wyrd_runtime::principal::Principal;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRefScope;
use wyrd_sql::queries::verifier_runs::{ObservationRecord, VerifierRunQueue};
use wyrd_sql::{SqlError, WyrdPostgres};

/// Tenants the run-request writer inserts for at once, which bounds the Wyrd
/// pool connections it may hold.
const OBSERVATION_RUN_WRITER_CONNECTIONS: usize = 4;

/// The process outbox of Eval run requests: the generic outbox over
/// [`ObservationRunSink`].
pub type ObservationRunOutbox = Outbox<ObservationRunSink>;

/// Run-request destination of the outbox: one tenant's records, one
/// transaction, one multi-row insert into `verifier_runs`.
pub struct ObservationRunSink {
    /// Wyrd Postgres owner each tenant write opens its transaction through.
    postgres: WyrdPostgres,
    /// The shared durable run queue.
    runs: VerifierRunQueue,
}

impl ObservationRunSink {
    /// Starts the process run-request outbox over `postgres`.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn outbox(postgres: WyrdPostgres) -> Arc<ObservationRunOutbox> {
        Outbox::new(
            Self {
                postgres,
                runs: VerifierRunQueue::default(),
            },
            OBSERVATION_RUN_WRITER_CONNECTIONS,
        )
    }
}

impl OutboxSink for ObservationRunSink {
    type Item = ObservationRecord;
    type Error = SqlError;
    const NAME: &'static str = "eval_run_requests";

    /// Inserts one run per `observations_ready` binding the record's writer
    /// owns on its subject in the tenant's transaction and commits it.
    ///
    /// Repeating the write is harmless: the insert is keyed by tenant,
    /// binding, and record with `ON CONFLICT DO NOTHING`.
    ///
    /// # Errors
    /// Returns the connection, insert, or commit failure; the outbox retries
    /// the batch.
    ///
    /// # Cancellation
    /// The outbox's shutdown deadline may drop this future mid-write.
    /// Cancelled before `commit` is sent, the transaction rolls back and no
    /// run exists; cancelled while `commit` resolves, the outcome is unknown
    /// and the runs may already be durable. Either way the outbox counts the
    /// batch in `outbox_events_lost_total`. A later repeat of the same
    /// records is safe, because the tenant/binding/record key absorbs any run
    /// that did commit.
    async fn write(
        &self,
        tenant: DataTenantId,
        records: &[ObservationRecord],
    ) -> Result<(), SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        self.runs
            .enqueue_observation_batch(&mut conn, records)
            .await
            .map_err(SqlError::Query)?;
        conn.commit().await
    }
}

/// Gate's observation acknowledgement hook: decodes each acknowledged Eval
/// frame and stages its run requests on the shared [`ObservationRunOutbox`].
pub struct ObservationEnqueue {
    /// Outbox the run requests are staged on.
    runs: Arc<ObservationRunOutbox>,
}

impl ObservationEnqueue {
    /// Builds the hook over the process run-request outbox.
    #[must_use]
    pub const fn new(runs: Arc<ObservationRunOutbox>) -> Self {
        Self { runs }
    }
}

impl ObservationAck for ObservationEnqueue {
    /// Decodes the acknowledged frame's records against `card_scope`, the
    /// scope Scribe stamped them with, and stages one run request per record,
    /// tagged with the Card `auth`'s principal is bound to as its writer,
    /// without waiting.
    ///
    /// A writer bound to no Card is staged without a writer, so its records
    /// activate every matching binding of their subject. A frame that does
    /// not decode is logged and counted as lost; the acknowledged
    /// observation is never affected.
    fn acknowledged(
        &self,
        auth: &AuthContext,
        card_scope: Option<&CardRefScope>,
        frame: Bytes,
        receipt_micros: i64,
    ) {
        let writer = bound_card(&auth.principal);
        match EvalObservationsTable::acknowledged(&frame, card_scope, receipt_micros) {
            Ok(keys) => {
                for key in keys {
                    self.runs.stage(
                        auth.tenant,
                        ObservationRecord {
                            subject: key.card_uid,
                            writer: writer.clone(),
                            record_id: key.record_id,
                            event_time: key.event_time,
                        },
                    );
                }
            }
            Err(error) => {
                tracing::error!(
                    tenant = %auth.tenant,
                    request_id = auth.request_id.as_str(),
                    %error,
                    "acknowledged Eval observation frame did not decode into run requests"
                );
                metrics::counter!("outbox_events_lost_total", "outbox" => ObservationRunSink::NAME)
                    .increment(1);
            }
        }
    }
}

/// The UID of the Card `principal` is bound to, read from the matching member
/// of its signed Card scope, which the token mint resolves with registry UIDs.
///
/// Returns `None` for a principal bound to no Card (humans, tenant
/// administrators, Card-free automation, SYSTEM) or whose root member carries
/// no UID; such a writer activates bindings of any owner.
fn bound_card(principal: &Principal) -> Option<CardUid> {
    let card = principal.card_ref()?;
    principal
        .card_ref_scope()?
        .as_slice()
        .iter()
        .find(|member| member.same_identity(card))?
        .uid
        .clone()
}

#[cfg(test)]
mod tests {
    //! Writer Card resolution for Eval run requests.

    use uuid::Uuid;
    use wyrd_runtime::PermissionSet;
    use wyrd_runtime::principal::{PrincipalId, PrincipalKind};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::*;

    /// A tenant principal of `kind` with no roles or permissions.
    fn principal(kind: PrincipalKind) -> Principal {
        Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            kind,
            DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::new(),
        )
    }

    /// A Service principal resolves to its root scope member's UID, while a
    /// User and a Card-free Service resolve to no writer Card, so their
    /// records activate the subject's bindings of every owner.
    #[test]
    fn writer_card_is_the_bound_cards_scope_uid() {
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid Card UID");
        let root: CardRef = "prod/Service/writer@1.0.0"
            .parse()
            .expect("static Card reference parses");
        let resolved = CardRef {
            uid: Some(uid.clone()),
            ..root.clone()
        };
        let bound = principal(PrincipalKind::Service {
            card_ref: Some(root),
            card_ref_scope: CardRefScope::own(&resolved),
        });
        assert_eq!(bound_card(&bound), Some(uid));
        assert_eq!(bound_card(&principal(PrincipalKind::User)), None);
        let card_free = principal(PrincipalKind::Service {
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        });
        assert_eq!(bound_card(&card_free), None);
    }
}
