//! Accumulation of the managed core's physical events for Forge.
//!
//! The core reports what it physically did — an object opened, a roll decided,
//! a close settled, memory peaked, scratch measured, and exactly one terminal
//! event. Forge needs one thing from that stream: the set of objects the
//! attempt may have produced, plus the two peaks, so a failed or cancelled
//! attempt is reclaimable and its resource lease is auditable.
//!
//! Everything else is deliberately dropped. The observer never influences the
//! rewrite: [`RewriteObserver::on_event`] returns nothing, and this
//! implementation holds no channel, no error slot, and no cancellation
//! authority through which it could.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use iceberg_compaction_core::managed::{OutputIdentity, RewriteEvent, RewriteObserver};

/// Forge's one observer of a managed rewrite attempt.
///
/// Owns the attempt-scoped accumulators the executor reads back after the core
/// returns: the possibly-produced output set, and the two peak measurements
/// that make the attempt's resource lease auditable against what it actually
/// used. All three are shared with the executor rather than returned, because
/// the observer is handed to the core and only the core calls it.
pub(crate) struct ForgeRewriteObserver {
    /// Objects the attempt opened, settled or not, in open order.
    outputs: std::sync::Mutex<Vec<OutputIdentity>>,
    /// Highest reservation the core observed against the leased pool.
    peak_memory_bytes: Arc<AtomicU64>,
    /// Highest byte usage the core measured under the leased scratch root.
    peak_scratch_bytes: Arc<AtomicU64>,
}

/// Reports only the observer's accumulators.
///
/// Written by hand because the interior mutex has no useful derived shape;
/// printing the accumulators is what a maintainer inspecting a stuck attempt
/// actually wants.
impl std::fmt::Debug for ForgeRewriteObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForgeRewriteObserver")
            .field(
                "outputs",
                &self.outputs.lock().map(|guard| guard.len()).ok(),
            )
            .field("peak_memory_bytes", &self.peak_memory_bytes())
            .field("peak_scratch_bytes", &self.peak_scratch_bytes())
            .finish_non_exhaustive()
    }
}

impl ForgeRewriteObserver {
    /// Creates one observer for a single rewrite attempt.
    pub(crate) fn new() -> Self {
        Self {
            outputs: std::sync::Mutex::new(Vec::new()),
            peak_memory_bytes: Arc::new(AtomicU64::new(0)),
            peak_scratch_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns every object this attempt opened, settled or not.
    ///
    /// A caller reclaiming after a failure must treat an unsettled entry as
    /// possibly-existing: the close was outstanding when the attempt ended, so
    /// the object may or may not be in storage.
    ///
    /// # Panics
    ///
    /// Panics only if the accumulator lock was poisoned by a panicking
    /// observer callback, which cannot happen because no callback can panic.
    pub(crate) fn outputs(&self) -> Vec<OutputIdentity> {
        self.outputs
            .lock()
            .expect("invariant: the rewrite observer never panics while holding its accumulator")
            .clone()
    }

    /// Returns the highest reservation the core observed, in bytes.
    pub(crate) fn peak_memory_bytes(&self) -> u64 {
        self.peak_memory_bytes.load(Ordering::Acquire)
    }

    /// Returns the highest scratch usage the core measured, in bytes.
    pub(crate) fn peak_scratch_bytes(&self) -> u64 {
        self.peak_scratch_bytes.load(Ordering::Acquire)
    }

    /// Merges the possibly-produced output set carried by a terminal event.
    ///
    /// One attempt runs every admitted plan under one observer, so each plan
    /// contributes its own terminal event. The accumulator is therefore merged
    /// rather than replaced: replacing it would let a later cancelled or failed
    /// plan erase the objects an earlier successful plan already produced,
    /// leaving unreclaimable orphans behind.
    ///
    /// `logical_ordinal` is the attempt-global reservation the core issues
    /// before an object opens, so it is the merge key. An identity already
    /// recorded from its `OutputOpened` event is updated in place, and `settled`
    /// is sticky: once the core reports an object closed it never reverts, so a
    /// later terminal report cannot unsettle it. Entries stay ordered by
    /// ordinal, which is open order.
    fn merge_outputs(&self, outputs: &[OutputIdentity]) {
        if let Ok(mut retained) = self.outputs.lock() {
            for output in outputs {
                match retained
                    .iter_mut()
                    .find(|existing| existing.logical_ordinal == output.logical_ordinal)
                {
                    Some(existing) => {
                        existing.path.clone_from(&output.path);
                        existing.settled |= output.settled;
                    }
                    None => retained.push(output.clone()),
                }
            }
            retained.sort_by_key(|output| output.logical_ordinal);
        }
    }
}

impl RewriteObserver for ForgeRewriteObserver {
    /// Folds any measurement one physical event carries.
    ///
    /// Every branch is O(1) and lock-free apart from the terminal output merge,
    /// which happens once per plan the attempt executes.
    fn on_event(&self, event: RewriteEvent) {
        match event {
            RewriteEvent::OutputOpened {
                logical_ordinal,
                path,
                ..
            } => {
                if let Ok(mut outputs) = self.outputs.lock() {
                    outputs.push(OutputIdentity {
                        logical_ordinal,
                        path,
                        settled: false,
                    });
                }
            }
            RewriteEvent::PeakMemory { peak_bytes, .. } => {
                self.peak_memory_bytes.fetch_max(
                    u64::try_from(peak_bytes).unwrap_or(u64::MAX),
                    Ordering::AcqRel,
                );
            }
            RewriteEvent::ScratchSpill { peak_bytes, .. } => {
                self.peak_scratch_bytes
                    .fetch_max(peak_bytes, Ordering::AcqRel);
            }
            RewriteEvent::Succeeded { outputs, .. }
            | RewriteEvent::Failed { outputs, .. }
            | RewriteEvent::Cancelled { outputs, .. } => self.merge_outputs(&outputs),
            RewriteEvent::RollDecided { .. }
            | RewriteEvent::OutputClosed { .. }
            | RewriteEvent::OperatorSpill { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use iceberg_compaction_core::managed::AttemptId;

    use super::*;

    /// One instance of every closed core event, in the order the core emits them.
    fn every_event(attempt_id: AttemptId) -> Vec<RewriteEvent> {
        vec![
            RewriteEvent::OutputOpened {
                attempt_id,
                logical_ordinal: 0,
                path: "out-0.parquet".to_owned(),
            },
            RewriteEvent::RollDecided {
                attempt_id,
                logical_ordinal: 0,
                path: "out-0.parquet".to_owned(),
                reason: iceberg::writer::file_writer::rolling_writer::RollingCloseReason::Threshold,
                target_file_size_bytes: 1024,
                written_size_estimate_bytes: 1024,
            },
            RewriteEvent::OutputClosed {
                attempt_id,
                logical_ordinal: 0,
                completion_ordinal: 0,
                path: "out-0.parquet".to_owned(),
                reason: iceberg::writer::file_writer::rolling_writer::RollingCloseReason::Threshold,
                output_files: Some(1),
            },
            RewriteEvent::PeakMemory {
                attempt_id,
                peak_bytes: 4096,
                pool_capacity_bytes: Some(8192),
            },
            RewriteEvent::OperatorSpill {
                attempt_id,
                spill_count: 1,
                spilled_bytes: 512,
                spilled_rows: 8,
            },
            RewriteEvent::ScratchSpill {
                attempt_id,
                current_bytes: 256,
                peak_bytes: 512,
            },
            RewriteEvent::Succeeded {
                attempt_id,
                outputs: vec![OutputIdentity {
                    logical_ordinal: 0,
                    path: "out-0.parquet".to_owned(),
                    settled: true,
                }],
                output_bytes: 1024,
            },
            RewriteEvent::Failed {
                attempt_id,
                message: "boom".to_owned(),
                outputs: Vec::new(),
            },
            RewriteEvent::Cancelled {
                attempt_id,
                outputs: vec![OutputIdentity {
                    logical_ordinal: 1,
                    path: "out-1.parquet".to_owned(),
                    settled: false,
                }],
            },
        ]
    }

    /// The projection is total, bounded, reachable, and cannot alter a rewrite.
    ///
    /// *Cumulative*: the terminal events of one attempt describe different
    /// plans, so their possibly-produced sets are merged by attempt-global
    /// logical ordinal rather than replaced — the observer projects one
    /// attempt, not one plan.
    ///
    /// *Total and reachable*: every one of the core's nine events maps to a
    /// distinct Forge label, and the nine labels are exactly the registered
    /// inventory — so no event is dropped and no label exists that nothing can
    /// emit. *Bounded*: the labels are compile-time constants with no path,
    /// attempt, or tenant in them, which is what keeps one counter family from
    /// becoming one series per table. *Non-semantic*: the observer's only
    /// method returns nothing and the module names no mutation of a rewrite
    /// result, so observation cannot change what a rewrite produces — the
    /// source assertion is what keeps that true as the module grows.
    #[test]
    fn forge_managed_observer_projection_is_bounded_reachable_and_non_semantic() {
        const SOURCE: &str = include_str!("observer.rs");

        let attempt_id = AttemptId::new();
        let events = every_event(attempt_id);
        let projected: Vec<ForgeRewriteEventKind> =
            events.iter().map(ForgeRewriteObserver::project).collect();
        assert_eq!(
            projected,
            ForgeRewriteEventKind::ALL.to_vec(),
            "every core event maps to exactly one registered label, in order"
        );
        let mut distinct = projected.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            ForgeRewriteEventKind::ALL.len(),
            "no two core events collapse onto one label"
        );
        for kind in ForgeRewriteEventKind::ALL {
            let label = kind.as_str();
            assert!(
                !label.contains('/') && !label.contains('-') && !label.is_empty(),
                "{label} must be a stable identifier, never a path fragment"
            );
        }

        let observer = ForgeRewriteObserver::new();
        for event in events {
            observer.on_event(event);
        }
        assert_eq!(
            observer.peak_memory_bytes(),
            4096,
            "the leased pool's peak is folded from the core's own measurement"
        );
        assert_eq!(
            observer.peak_scratch_bytes(),
            512,
            "the leased scratch peak is folded from the core's own measurement"
        );
        assert_eq!(
            observer
                .outputs()
                .into_iter()
                .map(|output| (output.logical_ordinal, output.path, output.settled))
                .collect::<Vec<_>>(),
            vec![
                (0, "out-0.parquet".to_owned(), true),
                (1, "out-1.parquet".to_owned(), false),
            ],
            "possibly-produced outputs accumulate across every terminal event of \
             the attempt, ordered by the attempt-global logical ordinal: a later \
             cancelled plan must not hide an earlier succeeded plan's objects"
        );

        let body = SOURCE
            .split_once("#[cfg(test)]")
            .expect("the module has a test section")
            .0;
        for forbidden in [
            "RewriteHandoff",
            "DataFile",
            "CancellationToken",
            "Result<",
            "Catalog",
        ] {
            assert!(
                !body.contains(forbidden),
                "an observer that could reach '{forbidden}' could change a rewrite"
            );
        }
    }
}
