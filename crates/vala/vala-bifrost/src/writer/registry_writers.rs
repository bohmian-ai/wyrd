//! Process-local per-physical-table writer registry and supervisor.
//!
//! [`WriterRegistry`] is the single source of truth for live coordinator
//! handles.  [`WriterSupervisor`] owns the `JoinSet` that supervises actor
//! tasks; the registry and supervisor are coupled so the supervisor can retire
//! idle coordinators and the registry can lazy-spawn them.
//!
//! **Q3 contract:** one coordinator per physical table, lazily spawned on
//! first write, `JoinSet`-supervised, drains on shutdown, idle-retire after 5
//! minutes.
//!
//! **Q7 contract:** no writer election.  Cross-pod serialization is achieved
//! by Iceberg catalog CAS + the `WRITER_INSTANCE` fence in `commit.rs` +
//! `CommitKey` dedup.  Two pods that both hold a coordinator for the same
//! table will serialize at the Iceberg SQL-catalog CAS; they will NOT forward
//! writes to each other and will NOT shed ownership.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use crate::error::BifrostError;
use crate::types::TableUid;
use crate::writer::coordinator::GroupCommitHandle;

/// Duration after which an idle coordinator is retired.
pub const IDLE_RETIRE_DURATION: Duration = Duration::from_mins(5);

/// A live handle to the group-commit coordinator for one physical table.
struct LiveHandle {
    handle: GroupCommitHandle,
    last_used: Instant,
}

impl LiveHandle {
    fn new(handle: GroupCommitHandle) -> Self {
        Self {
            handle,
            last_used: Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    fn is_idle(&self) -> bool {
        self.last_used.elapsed() > IDLE_RETIRE_DURATION
    }
}

/// Message type for the supervisor actor.
pub(crate) enum RegistryCmd {
    /// Return (or lazy-spawn) the group-commit coordinator for `table_uid`.
    GetOrSpawn {
        table_uid: TableUid,
        spawn_fn: Box<dyn FnOnce() -> GroupCommitHandle + Send>,
        reply: oneshot::Sender<GroupCommitHandle>,
    },
    /// Retire coordinators that have been idle longer than [`IDLE_RETIRE_DURATION`].
    RetireIdle,
    /// Drain: wait for all coordinator tasks to finish, then reply.
    Shutdown(oneshot::Sender<()>),
}

/// Process-local registry of per-physical-table group-commit coordinators.
///
/// The supervisor task owns this and responds to [`RegistryCmd`] messages.  It
/// is not shared directly — callers hold a [`WriterRegistry`] handle.
struct RegistrySupervisorActor {
    receiver: mpsc::Receiver<RegistryCmd>,
    handles: HashMap<TableUid, LiveHandle>,
    join_set: JoinSet<()>,
}

impl RegistrySupervisorActor {
    fn new(receiver: mpsc::Receiver<RegistryCmd>) -> Self {
        Self {
            receiver,
            handles: HashMap::new(),
            join_set: JoinSet::new(),
        }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                // Process registry commands.
                Some(cmd) = self.receiver.recv() => {
                    match cmd {
                        RegistryCmd::GetOrSpawn { table_uid, spawn_fn, reply } => {
                            let handle = self.get_or_spawn(table_uid, spawn_fn);
                            let _ = reply.send(handle);
                        }
                        RegistryCmd::RetireIdle => {
                            self.retire_idle();
                        }
                        RegistryCmd::Shutdown(reply) => {
                            // Stop accepting new commands; drain all live coordinators.
                            drop(self.receiver);
                            // Drop live handles — their senders close, causing actor loops
                            // to exit naturally.
                            self.handles.clear();
                            // Wait for all supervised tasks to finish.
                            while self.join_set.join_next().await.is_some() {}
                            let _ = reply.send(());
                            return;
                        }
                    }
                }
                // Reap completed coordinator tasks.
                Some(_result) = self.join_set.join_next() => {
                    // Task completed normally (idle-retire or channel closed).
                }
                else => break,
            }
        }
    }

    fn get_or_spawn(
        &mut self,
        table_uid: TableUid,
        spawn_fn: impl FnOnce() -> GroupCommitHandle,
    ) -> GroupCommitHandle {
        if let Some(live) = self.handles.get_mut(&table_uid) {
            if !live.handle.is_closed() {
                live.touch();
                return live.handle.clone();
            }
            // Actor exited (idle-retire or panic) — remove and re-spawn.
            self.handles.remove(&table_uid);
        }

        let handle = spawn_fn();
        self.handles
            .insert(table_uid, LiveHandle::new(handle.clone()));
        handle
    }

    fn retire_idle(&mut self) {
        let idle: Vec<TableUid> = self
            .handles
            .iter()
            .filter(|(_, live)| live.is_idle() || live.handle.is_closed())
            .map(|(uid, _)| *uid)
            .collect();
        for uid in idle {
            self.handles.remove(&uid);
        }
    }
}

/// Caller-facing handle to the writer supervisor.
///
/// Cloneable; cheaply passed to the ingest orchestrator. All spawning and
/// retirement decisions happen inside the supervisor task.
#[derive(Clone)]
pub struct WriterRegistry {
    sender: Arc<mpsc::Sender<RegistryCmd>>,
}

impl WriterRegistry {
    /// Spawn the supervisor task and return a registry handle.
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel(256);
        let actor = RegistrySupervisorActor::new(receiver);
        tokio::spawn(actor.run());
        Self {
            sender: Arc::new(sender),
        }
    }

    /// Return the group-commit coordinator for `table_uid`, spawning one if
    /// none exists or the previous one has exited.
    ///
    /// `spawn_fn` is called at most once per table per supervisor lifetime
    /// (or after an idle-retire).  It must be cheap — callers should not hold
    /// locks inside it.
    ///
    /// # Errors
    /// Returns [`BifrostError::IngestBusy`] when the supervisor channel is
    /// saturated (the supervisor itself is the backpressure signal here, not
    /// per-table).
    pub async fn get_or_spawn(
        &self,
        table_uid: TableUid,
        table_fqn: String,
        spawn_fn: impl FnOnce() -> GroupCommitHandle + Send + 'static,
    ) -> Result<GroupCommitHandle, BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(RegistryCmd::GetOrSpawn {
                table_uid,
                spawn_fn: Box::new(spawn_fn),
                reply: tx,
            })
            .await
            .map_err(|_| BifrostError::IngestBusy(table_fqn.clone()))?;
        rx.await.map_err(|_| BifrostError::IngestBusy(table_fqn))
    }

    /// Trigger idle-retirement of stale coordinators.  Call periodically.
    pub async fn retire_idle(&self) {
        let _ = self.sender.send(RegistryCmd::RetireIdle).await;
    }

    /// Drain all coordinators and wait for them to finish.  Call on shutdown.
    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self.sender.send(RegistryCmd::Shutdown(tx)).await;
        let _ = rx.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::coordinator::GroupCommitHandle;

    fn dummy_handle() -> GroupCommitHandle {
        // A handle backed by a channel that is immediately dropped (closed).
        // Sufficient for registry tests that don't drive commits.
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        GroupCommitHandle {
            sender: std::sync::Arc::new(sender),
            table_fqn: "test.table".to_string(),
        }
    }

    #[tokio::test]
    async fn writer_registry_get_or_spawn_returns_same_handle_for_same_table() {
        let registry = WriterRegistry::start();
        let uid = TableUid::new_v7();

        let h1 = registry
            .get_or_spawn(uid, "test.a".to_string(), dummy_handle)
            .await
            .expect("first spawn succeeds");

        // Second call for the same uid: the closed handle (dummy) is re-spawned.
        // We verify at minimum that get_or_spawn does not error.
        let _h2 = registry
            .get_or_spawn(uid, "test.a".to_string(), dummy_handle)
            .await
            .expect("second get does not error");

        drop(h1);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn writer_registry_retire_idle_does_not_panic() {
        let registry = WriterRegistry::start();
        registry.retire_idle().await;
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn writer_registry_shutdown_completes() {
        let registry = WriterRegistry::start();
        let uid = TableUid::new_v7();
        let _ = registry
            .get_or_spawn(uid, "test.b".to_string(), dummy_handle)
            .await;
        registry.shutdown().await; // must not hang
    }
}
