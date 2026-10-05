//! Accumulation of the managed core's physical events for Forge.
//!
//! The core reports what it physically did — an object opened, a roll decided,
//! a close settled, and exactly one terminal event per plan. Forge needs one
//! thing from that stream: the set of objects the attempt may have produced, so
//! a failed or cancelled attempt is reclaimable.
//!
//! Everything else is deliberately dropped. The observer never influences the
//! rewrite: [`RewriteObserver::on_event`] returns nothing, and this
//! implementation holds no channel, no error slot, and no cancellation
//! authority through which it could.

use iceberg_compaction_core::managed::{OutputIdentity, RewriteEvent, RewriteObserver};

/// Forge's one observer of a managed rewrite attempt.
///
/// Owns the attempt-scoped possibly-produced output set the executor reads back
/// after the core returns. It is shared with the executor rather than returned,
/// because the observer is handed to the core and only the core calls it.
pub(crate) struct ForgeRewriteObserver {
    /// Objects the attempt opened, settled or not, in open order.
    outputs: std::sync::Mutex<Vec<OutputIdentity>>,
}

/// Reports only the observer's accumulator size.
///
/// Written by hand because the interior mutex has no useful derived shape;
/// the output count is what a maintainer inspecting a stuck attempt actually
/// wants.
impl std::fmt::Debug for ForgeRewriteObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForgeRewriteObserver")
            .field(
                "outputs",
                &self.outputs.lock().map(|guard| guard.len()).ok(),
            )
            .finish_non_exhaustive()
    }
}

impl ForgeRewriteObserver {
    /// Creates one observer for a single rewrite attempt.
    pub(crate) fn new() -> Self {
        Self {
            outputs: std::sync::Mutex::new(Vec::new()),
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
    /// Folds the output identity one physical event carries.
    ///
    /// An open appends one identity; a terminal event merges its output set,
    /// once per plan the attempt executes. Every other event is dropped.
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
            RewriteEvent::Succeeded { outputs, .. }
            | RewriteEvent::Failed { outputs, .. }
            | RewriteEvent::Cancelled { outputs, .. } => self.merge_outputs(&outputs),
            RewriteEvent::RollDecided { .. } | RewriteEvent::OutputClosed { .. } => {}
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

    /// Observation folds every possibly-produced output and cannot alter a
    /// rewrite.
    ///
    /// *Cumulative*: the terminal events of one attempt describe different
    /// plans, so their possibly-produced sets are merged by attempt-global
    /// logical ordinal rather than replaced — the observer projects one
    /// attempt, not one plan.
    ///
    /// *Non-semantic*: the observer's only method returns nothing and the
    /// module names no mutation of a rewrite result, so observation cannot
    /// change what a rewrite produces — the source assertion is what keeps
    /// that true as the module grows.
    #[test]
    fn forge_managed_observer_folds_outputs_without_semantics() {
        const SOURCE: &str = include_str!("observer.rs");

        let attempt_id = AttemptId::new();
        let events = every_event(attempt_id);
        let observer = ForgeRewriteObserver::new();
        for event in events {
            observer.on_event(event);
        }
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
