//! Private stdout progress coordination for registry artifact transfers.

use std::collections::BTreeMap;
use std::sync::Mutex;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use wyrd_spec::registry::RelativeArtifactPath;

/// Owns one stdout progress operation and its active per-artifact bars.
///
/// The display is shared by bounded concurrent downloads, but all terminal
/// mutation is synchronous and protected by a short-lived mutex. It never
/// crosses an await while holding that mutex.
pub(crate) struct DownloadProgressDisplay {
    /// One stdout renderer for this artifact workflow.
    progress: MultiProgress,
    /// Active bars keyed by their server-authorized relative paths.
    bars: Mutex<BTreeMap<RelativeArtifactPath, ProgressBar>>,
}

impl DownloadProgressDisplay {
    /// Construct a display that always targets stdout for one download workflow.
    #[must_use]
    pub(crate) fn stdout() -> Self {
        Self::new(ProgressDrawTarget::stdout())
    }

    /// Start one artifact bar with the server-declared size when available.
    ///
    /// # Panics
    /// Panics only if the static progress-bar template is invalid, which is a
    /// source-code invariant.
    pub(crate) fn start(&self, path: &RelativeArtifactPath, total_bytes: Option<u64>) {
        let bar = self.progress.add(match total_bytes {
            Some(total_bytes) => ProgressBar::new(total_bytes),
            None => ProgressBar::new_spinner(),
        });
        bar.set_style(
            ProgressStyle::with_template("{msg} {wide_bar} {bytes}/{total_bytes}")
                .expect("progress template is a static invariant"),
        );
        bar.set_message(path.as_str().to_owned());
        self.bars_lock().insert(path.clone(), bar);
    }

    /// Advance one artifact bar to the bytes durably written by storage.
    pub(crate) fn set_position(
        &self,
        path: &RelativeArtifactPath,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) {
        if let Some(bar) = self.bars_lock().get(path) {
            if let Some(total_bytes) = total_bytes {
                bar.set_length(total_bytes);
            }
            bar.set_position(downloaded_bytes);
        }
    }

    /// Finish one artifact bar, removing it whether its transfer succeeded or failed.
    pub(crate) fn finish(&self, path: &RelativeArtifactPath, succeeded: bool) {
        if let Some(bar) = self.bars_lock().remove(path) {
            if !succeeded {
                bar.set_message(format!("{} failed", path.as_str()));
            }
            bar.finish_and_clear();
            self.progress.remove(&bar);
        }
    }

    /// Clear every remaining bar before the workflow returns its final result.
    pub(crate) fn clear(&self) {
        let bars = std::mem::take(&mut *self.bars_lock());
        for (_, bar) in bars {
            bar.finish_and_clear();
            self.progress.remove(&bar);
        }
        let _ = self.progress.clear();
    }

    /// Obtain active-bar state while recovering from a poisoned test or terminal lock.
    fn bars_lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<RelativeArtifactPath, ProgressBar>> {
        self.bars
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Construct a display with a selected draw target for production and tests.
    fn new(draw_target: ProgressDrawTarget) -> Self {
        Self {
            progress: MultiProgress::with_draw_target(draw_target),
            bars: Mutex::new(BTreeMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use indicatif::ProgressDrawTarget;
    use wyrd_spec::registry::RelativeArtifactPath;

    use super::DownloadProgressDisplay;

    /// Track every declared artifact to its final written byte position and clear all bars.
    #[test]
    fn tracks_multiple_artifacts_and_clears_after_success() {
        let display = DownloadProgressDisplay::new(ProgressDrawTarget::hidden());
        let first = RelativeArtifactPath::new("first.bin").expect("test path is valid");
        let second = RelativeArtifactPath::new("nested/second.bin").expect("test path is valid");
        display.start(&first, Some(3));
        display.start(&second, Some(5));
        display.set_position(&first, 3, Some(3));
        display.set_position(&second, 5, Some(5));
        assert_eq!(display.bars_lock()[&first].position(), 3);
        assert_eq!(display.bars_lock()[&second].position(), 5);
        display.finish(&first, true);
        display.finish(&second, true);
        display.clear();
        assert!(display.bars_lock().is_empty());
    }
}
