//! Stage-walking driver for a Verifier's `ExecutionPlan`.
//!
//! [`walk`] is the one stage lifecycle the Verifier engine runs:
//!
//! 1. Walks the stages in topological order, settling every task into one
//!    [`RunLedger`].
//! 2. Per stage: the caller's `prepare` classifies each task synchronously
//!    (no IO) against the ledger of earlier stages, settling skips without
//!    running and handing back runnable futures.
//! 3. Polls the runnable tasks concurrently inside the caller's task, so
//!    each stage costs no runtime spawn or wake-up hop.
//! 4. At the stage barrier: appends the stage's settlements to the ledger in
//!    declaration order.

use std::any::Any;
use std::collections::HashMap;
use std::future::{Future, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::Poll;

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

/// A pending task future polled by [`walk`].
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
/// runnable ones are polled together inside the caller's task and span, so
/// their waits overlap, their work stays in the caller's trace, and no task
/// is spawned or woken through the runtime. At the stage barrier
/// the settlements are appended in declaration order. Contract-specific
/// classification, budgets and results stay in `prepare`.
///
/// # Errors
/// Returns the first `prepare` error, the first task error in completion
/// order, or `panicked` applied to the message of a task that panicked.
///
/// # Cancellation
/// Returning early or dropping the future drops every task of the stage
/// still running; the stage yields no partial settlements. Work a dropped
/// task already performed is not rolled back.
pub(crate) async fn walk<'s, O, E>(
    stages: impl IntoIterator<Item = &'s Stage>,
    mut prepare: impl FnMut(&RunLedger<O>, &Stage) -> Result<Vec<Prepared<O, E>>, E>,
    panicked: fn(String) -> E,
) -> Result<RunLedger<O>, E>
where
    O: Send + 'static,
    E: Send + 'static,
{
    let mut ledger = RunLedger::default();
    for stage in stages {
        let mut settled = HashMap::new();
        let mut running = Vec::new();
        for (at, prepared) in prepare(&ledger, stage)?.into_iter().enumerate() {
            match prepared {
                Prepared::Settled(done) => {
                    settled.insert(at, done);
                }
                Prepared::Run(task) => running.push((at, task)),
            }
        }
        while !running.is_empty() {
            let (at, done) = poll_fn(|cx| {
                let ready = running
                    .iter_mut()
                    .enumerate()
                    .find_map(|(index, (_, task))| {
                        match catch_unwind(AssertUnwindSafe(|| task.as_mut().poll(cx))) {
                            Ok(Poll::Pending) => None,
                            Ok(Poll::Ready(done)) => Some((index, Ok(done))),
                            Err(payload) => Some((index, Err(message(payload.as_ref())))),
                        }
                    });
                ready.map_or(Poll::Pending, |(index, done)| {
                    Poll::Ready((running.swap_remove(index).0, done))
                })
            })
            .await;
            settled.insert(at, done.map_err(panicked)??);
        }
        for (at, task) in stage.tasks.iter().enumerate() {
            if let Some(done) = settled.remove(&at) {
                ledger.push(task.clone(), done);
            }
        }
    }
    Ok(ledger)
}

/// The text of a panic `payload`, or a fixed note when it carries none.
fn message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a verification block panicked".to_owned())
}
