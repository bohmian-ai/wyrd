//! Group commit of completed Verifier runs.
//!
//! A run that completes queues its completion and waits for the reply. A
//! fixed set of [`FLUSHERS`] flush tasks each take everything queued, up to
//! [`BATCH`] runs, and settle each tenant's share in one tenant transaction
//! through [`VerifierExecutionQueue::complete_many`], staging each applied
//! run's result on its reserved slot before that transaction commits. Under
//! load one transaction settles many runs; an idle process settles a lone run
//! at once. A queued run is taken by the next free flush task, so no run waits
//! for another run to arrive, and settlement holds at most [`FLUSHERS`]
//! app-pool connections whatever the tenant count, leaving the rest of the
//! shared pool to Bifrost and the gateway.
//!
//! A flush that fails, or a run whose flush task is gone, gets no reply; that
//! run then settles alone through the runner's single settlement, which still
//! holds its result write unless the failed flush staged it before a failed
//! commit — the same state a failed single settlement leaves.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::{mpsc, oneshot};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_executions::{Completion, Settlement, VerifierExecutionQueue};
use wyrd_sql::{SqlError, WyrdPostgres};

use super::runner::StagedResult;

/// Flush tasks in the process, and so the most app-pool connections
/// settlement holds at once.
///
/// More than one lets the next batch gather and start while another commits;
/// the bound keeps settlement a fixed share of the pool every server
/// capability draws from.
const FLUSHERS: usize = 4;

/// Most queued runs one flush task takes at once, bounding one turn's work.
const BATCH: usize = 512;

/// A completed run's result write, shared between the run and its flush.
pub(super) type SharedResult = Arc<Mutex<Option<StagedResult>>>;

/// One run waiting for a flush to complete it.
struct Waiting {
    /// The run's tenant, whose transaction completes it.
    tenant: DataTenantId,
    /// The completion to apply.
    completion: Completion,
    /// The run's result write, staged when its completion applies.
    result: SharedResult,
    /// Receives the settlement once the flush commits.
    reply: oneshot::Sender<Settled>,
}

/// A committed flush's answer to one of its runs.
struct Settled {
    /// The run's settlement.
    settlement: Settlement,
    /// Duration of the shared batched statement, in microseconds.
    statement_us: u64,
    /// Runs the shared statement settled.
    batch: usize,
}

/// Handle every completing run queues its completion on.
///
/// Dropping it closes the queue; each flush task settles what is already
/// queued and then exits.
pub(super) struct Completions {
    /// Queue the flush tasks drain. Its length is bounded by the runner's
    /// execution slots, since each queued run holds one until it is answered.
    queue: mpsc::UnboundedSender<Waiting>,
}

/// One flush task's dependencies and its share of the queue.
struct Flusher {
    /// Wyrd Postgres owner that opens each tenant transaction.
    postgres: WyrdPostgres,
    /// Queue owning the batched completion statement.
    queue: VerifierExecutionQueue,
    /// The queue every flush task takes from; the task holding the lock waits
    /// for the next run and takes everything queued with it.
    waiting: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Waiting>>>,
}

impl Completions {
    /// Start [`FLUSHERS`] flush tasks over `postgres` and `queue`.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, which spawning requires.
    pub(super) fn new(postgres: &WyrdPostgres, queue: VerifierExecutionQueue) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let waiting = Arc::new(tokio::sync::Mutex::new(receiver));
        for _ in 0..FLUSHERS {
            let flusher = Flusher {
                postgres: postgres.clone(),
                queue,
                waiting: Arc::clone(&waiting),
            };
            tokio::spawn(flusher.run());
        }
        Self { queue: sender }
    }

    /// Complete one run of `tenant` in a group commit with the other
    /// completing runs, staging `result` before the commit when it applies.
    ///
    /// The shared statement is recorded on the caller's span as a
    /// `complete_many` child carrying its duration and batch size, as the
    /// claim statement is, so every attempt's trace still covers settlement.
    ///
    /// Returns the committed settlement, or `None` when no flush committed
    /// this run — its flush failed or no flush task remains — so the caller
    /// settles it alone.
    pub(super) async fn complete(
        &self,
        tenant: DataTenantId,
        completion: Completion,
        result: &SharedResult,
    ) -> Option<Settlement> {
        let (reply, settled) = oneshot::channel();
        self.queue
            .send(Waiting {
                tenant,
                completion,
                result: Arc::clone(result),
                reply,
            })
            .ok()?;
        let settled = settled.await.ok()?;
        drop(tracing::info_span!(
            "complete_many",
            operation = "verification.executions.complete_many",
            statement_us = settled.statement_us,
            batch = settled.batch,
        ));
        Some(settled.settlement)
    }
}

impl Flusher {
    /// Take everything queued, settle it one tenant transaction at a time,
    /// and repeat until the queue closes and empties.
    ///
    /// The lock is held only while waiting for and taking runs, so the other
    /// tasks flush meanwhile. A panic ends only this task; the runs it held
    /// get no reply and settle alone.
    async fn run(self) {
        let mut batch = Vec::with_capacity(BATCH);
        loop {
            if self.waiting.lock().await.recv_many(&mut batch, BATCH).await == 0 {
                return;
            }
            batch.sort_by_key(|waiting| waiting.tenant);
            while let Some(tenant) = batch.last().map(|waiting| waiting.tenant) {
                let first = batch.partition_point(|waiting| waiting.tenant < tenant);
                let runs = batch.split_off(first);
                self.flush(tenant, runs).await;
            }
        }
    }

    /// Settle one tenant's `runs` in one transaction and reply to each run.
    ///
    /// A failure is logged and replies to nobody, sending every run to its
    /// single settlement.
    async fn flush(&self, tenant: DataTenantId, runs: Vec<Waiting>) {
        let (completions, waiting): (Vec<_>, Vec<_>) = runs
            .into_iter()
            .map(|waiting| (waiting.completion, (waiting.result, waiting.reply)))
            .unzip();
        let batch = completions.len();
        match self.commit(tenant, &completions, &waiting).await {
            Ok((settlements, statement_us)) => {
                for ((_, reply), settlement) in waiting.into_iter().zip(settlements) {
                    let _ = reply.send(Settled {
                        settlement,
                        statement_us,
                        batch,
                    });
                }
            }
            Err(error) => {
                tracing::warn!(tenant_id = %tenant, runs = completions.len(), %error, "a batched verification settlement failed; settling each run alone");
            }
        }
    }

    /// Complete `completions` in one tenant transaction, staging each applied
    /// run's result write on its reserved slot before the commit, and return
    /// the settlements with the batched statement's duration in microseconds.
    ///
    /// A result already taken — staged by an earlier attempt whose commit
    /// failed — is not staged again.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the connection, statement, or commit fails;
    /// results staged before a failed commit stay staged.
    async fn commit(
        &self,
        tenant: DataTenantId,
        completions: &[Completion],
        waiting: &[(SharedResult, oneshot::Sender<Settled>)],
    ) -> Result<(Vec<Settlement>, u64), SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let started = std::time::Instant::now();
        let settlements = self.queue.complete_many(&mut conn, completions).await?;
        let statement_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        for (settlement, (result, _)) in settlements.iter().zip(waiting) {
            stage(tenant, *settlement, result);
        }
        conn.commit().await?;
        Ok((settlements, statement_us))
    }
}

/// Stage a run's result write on its reserved slot when its completion
/// `settlement` applied, inside the transaction that completes it.
///
/// The write is taken, so a later attempt of the same completion — after a
/// failed commit — completes the run without staging again; a stale
/// completion stages nothing.
pub(super) fn stage(tenant: DataTenantId, settlement: Settlement, result: &SharedResult) {
    if settlement == Settlement::Applied
        && let Some((slot, write)) = result.lock().unwrap_or_else(PoisonError::into_inner).take()
    {
        slot.stage(tenant, write);
    }
}
