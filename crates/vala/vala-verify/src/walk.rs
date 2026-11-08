//! The run ledger and the running-block set the Verifier engine drives.
//!
//! The engine releases each block as soon as the blocks it depends on are
//! complete, rather than at stage barriers:
//!
//! 1. Every settlement lands in one [`RunLedger`], in settlement order.
//! 2. Ready blocks are classified synchronously (no IO) against the ledger;
//!    blocks that settle without running are recorded at once, and runnable
//!    ones join one [`Tasks`] set.
//! 3. [`Tasks::next`] polls the whole set inside the caller's task, so the
//!    blocks' waits overlap, their work stays in the caller's trace, and no
//!    block is spawned or woken through the runtime.

use std::any::Any;
use std::collections::HashMap;
use std::future::{Future, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::Poll;

use wyrd_spec::verification::TaskId;

/// Per-run state of the Verifier engine: each settled block by id, in the
/// order it settled.
///
/// Readiness and scope resolution read earlier settlements through
/// [`RunLedger::get`]. A block's entry is replaced only when the engine
/// records a later decision about the same block, such as a speculative
/// step's disposition; its position stays that of the first settlement.
#[derive(Debug, Clone)]
pub struct RunLedger<O> {
    /// Settled blocks in settlement order.
    entries: Vec<(TaskId, O)>,
    /// Position of each settled block in `entries`.
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
    /// The settlement of `task`, when it settled.
    #[must_use]
    pub fn get(&self, task: &TaskId) -> Option<&O> {
        self.index.get(task).map(|&at| &self.entries[at].1)
    }

    /// Record `task`'s settlement after every earlier one, or replace the
    /// settlement it already has in place.
    pub(crate) fn insert(&mut self, task: TaskId, settled: O) {
        if let Some(&at) = self.index.get(&task) {
            self.entries[at].1 = settled;
        } else {
            self.index.insert(task.clone(), self.entries.len());
            self.entries.push((task, settled));
        }
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

/// A pending block future polled by [`Tasks`].
pub(crate) type Running<O, E> = Pin<Box<dyn Future<Output = Result<O, E>> + Send>>;

/// How one ready block proceeds, decided before it runs.
pub(crate) enum Prepared<O, E> {
    /// Settled without running, such as a skip.
    Settled(O),
    /// Runs concurrently with every other running block.
    Run(Running<O, E>),
}

/// The running blocks of one run, keyed by `K`, polled together.
///
/// Dropping the set, or cancelling one key, drops its futures: work a
/// dropped future already performed is not rolled back, and executors
/// release their resources on drop.
pub(crate) struct Tasks<K, O, E> {
    /// Running futures with their keys.
    running: Vec<(K, Running<O, E>)>,
}

impl<K, O, E> Default for Tasks<K, O, E> {
    /// No running work.
    fn default() -> Self {
        Self {
            running: Vec::new(),
        }
    }
}

impl<K: PartialEq, O, E> Tasks<K, O, E> {
    /// Start polling `task` under `key`.
    pub(crate) fn push(&mut self, key: K, task: Running<O, E>) {
        self.running.push((key, task));
    }

    /// Whether any future runs under `key`.
    pub(crate) fn contains(&self, key: &K) -> bool {
        self.running.iter().any(|(running, _)| running == key)
    }

    /// Drop every future running under `key`.
    pub(crate) fn cancel(&mut self, key: &K) {
        self.running.retain(|(running, _)| running != key);
    }

    /// Whether nothing is running.
    pub(crate) fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    /// Wait for every running future to finish and discard the results, so
    /// work already dispatched settles instead of being dropped; each future
    /// records its own settlement, and the caller already holds the error
    /// it returns.
    pub(crate) async fn drain(&mut self, panicked: fn(String) -> E) {
        while !self.is_empty() {
            let _finished = self.next(panicked).await;
        }
    }

    /// Wait for the next running future to finish, returning its key and
    /// result, or `panicked` applied to the message of a future that
    /// panicked.
    ///
    /// # Cancellation
    /// Dropping the returned future leaves every running future in the set.
    pub(crate) async fn next(&mut self, panicked: fn(String) -> E) -> (K, Result<O, E>) {
        poll_fn(|cx| {
            let ready =
                self.running.iter_mut().enumerate().find_map(
                    |(index, (_, task))| match catch_unwind(AssertUnwindSafe(|| {
                        task.as_mut().poll(cx)
                    })) {
                        Ok(Poll::Pending) => None,
                        Ok(Poll::Ready(done)) => Some((index, done)),
                        Err(payload) => Some((index, Err(panicked(message(payload.as_ref()))))),
                    },
                );
            ready.map_or(Poll::Pending, |(index, done)| {
                Poll::Ready((self.running.swap_remove(index).0, done))
            })
        })
        .await
    }
}

/// The text of a panic `payload`, or a fixed note when it carries none.
fn message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a verification block panicked".to_owned())
}
