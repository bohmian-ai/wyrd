//! Stage-walking driver for a Verifier's `ExecutionPlan`.
//!
//! [`walk`] is the one stage lifecycle the Verifier engine runs:
//!
//! 1. Walks the stages in topological order, settling every task into one
//!    [`RunLedger`].
//! 2. Per stage: the caller's `prepare` classifies each task synchronously
//!    (no IO) against the ledger of earlier stages, settling skips without
//!    running and handing back runnable futures.
//! 3. Fans the runnable tasks out concurrently on one `tokio::JoinSet`.
//! 4. At the stage barrier: appends the stage's settlements to the ledger in
//!    declaration order.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use tokio::task::{JoinError, JoinSet};
use tracing::Instrument as _;
use wyrd_spec::verification::{Stage, TaskId};

/// Per-run state shared by every stage-walking execution: each settled task
/// by id, in the order the stage walk settled it.
///
/// [`walk`] owns it and appends one stage at a time in declaration order, so
/// iteration order is stage order and, within a stage, declaration order.
/// Preparation of a later stage reads earlier settlements through
/// [`RunLedger::get`]; nothing mutates a settled entry.
#[derive(Debug, Clone)]
pub struct RunLedger<O> {
    /// Settled tasks in settlement order.
    entries: Vec<(TaskId, O)>,
    /// Position of each settled task in `entries`.
    index: HashMap<TaskId, usize>,
}

impl<O> Default for RunLedger<O> {
    /// An empty ledger: nothing has settled.
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<O> RunLedger<O> {
    /// The settlement of `task`, when an earlier stage settled it.
    #[must_use]
    pub fn get(&self, task: &TaskId) -> Option<&O> {
        self.index.get(task).map(|&at| &self.entries[at].1)
    }

    /// Record `task`'s settlement after every earlier one.
    fn push(&mut self, task: TaskId, settled: O) {
        self.index.insert(task.clone(), self.entries.len());
        self.entries.push((task, settled));
    }
}

impl<O> IntoIterator for RunLedger<O> {
    /// One owned settlement: the task id and what it settled as.
    type Item = (TaskId, O);
    /// Consumes the ledger, yielding settlements in settlement order.
    type IntoIter = std::vec::IntoIter<(TaskId, O)>;

    /// Every settlement, in settlement order.
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

/// A pending task future spawned by [`walk`].
pub(crate) type Running<O, E> = Pin<Box<dyn Future<Output = Result<O, E>> + Send>>;

/// How one planned task proceeds, decided before its stage fans out.
pub(crate) enum Prepared<O, E> {
    /// Settled without running, such as a skip.
    Settled(O),
    /// Runs concurrently with the rest of its stage.
    Run(Running<O, E>),
}

/// Walk `stages` in order, settling every task into one [`RunLedger`].
///
/// This is the single stage lifecycle of `vala-verify`: per stage, `prepare`
/// sees the ledger of every earlier stage and returns one [`Prepared`] per
/// stage task, in declaration order; settled tasks are recorded as is, and
/// runnable ones are spawned on one `JoinSet` inside the caller's current
/// span, so a task's work stays in the caller's trace. At the stage barrier
/// the settlements are appended in declaration order. Contract-specific
/// classification, budgets and results stay in `prepare`.
///
/// # Errors
/// Returns the first `prepare` error, the first task error in completion
/// order, or `joined` applied to a task that panicked or was aborted.
///
/// # Cancellation
/// Returning early or dropping the future drops the `JoinSet`, which aborts
/// every task of the stage still running; the stage yields no partial
/// settlements. Work an aborted task already performed is not rolled back.
pub(crate) async fn walk<'s, O, E>(
    stages: impl IntoIterator<Item = &'s Stage>,
    mut prepare: impl FnMut(&RunLedger<O>, &Stage) -> Result<Vec<Prepared<O, E>>, E>,
    joined: fn(JoinError) -> E,
) -> Result<RunLedger<O>, E>
where
    O: Send + 'static,
    E: Send + 'static,
{
    let mut ledger = RunLedger::default();
    for stage in stages {
        let mut settled = HashMap::new();
        let mut running = JoinSet::new();
        for (at, prepared) in prepare(&ledger, stage)?.into_iter().enumerate() {
            match prepared {
                Prepared::Settled(done) => {
                    settled.insert(at, done);
                }
                Prepared::Run(task) => {
                    running.spawn(
                        async move { (at, task.await) }.instrument(tracing::Span::current()),
                    );
                }
            }
        }
        while let Some(done) = running.join_next().await {
            let (at, done) = done.map_err(joined)?;
            settled.insert(at, done?);
        }
        for (at, task) in stage.tasks.iter().enumerate() {
            if let Some(done) = settled.remove(&at) {
                ledger.push(task.clone(), done);
            }
        }
    }
    Ok(ledger)
}
