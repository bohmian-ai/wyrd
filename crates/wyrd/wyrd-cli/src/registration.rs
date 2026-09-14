//! Terminal-owned Card registration presentation.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use wyrd_client::cards::RegistrationReceipt;
use wyrd_client::cards::{
    Cards, RegistrationPhase, RegistrationProgressEvent, RegistrationProgressSink,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::RelativeArtifactPath;

use crate::load::RegistrationInput;

/// Run Card registration with terminal progress presentation.
///
/// The shared registry emits typed events; this module owns all `indicatif`
/// state and terminal policy. The returned result is ready for a caller to
/// print after the progress display has been cleared.
///
/// # Errors
/// Returns the registration error produced by the shared registry saga.
pub async fn register(
    cards: &Cards,
    input: &RegistrationInput,
) -> Result<RegistrationReceipt, WyrdError> {
    let renderer = RegistrationProgressRenderer::new();
    let result = cards.register_with_progress(input, renderer.sink()).await;
    renderer.finish_and_clear();
    result
}

/// `indicatif` renderer for one registration lifecycle.
#[derive(Clone)]
pub struct RegistrationProgressRenderer {
    state: Arc<Mutex<RendererState>>,
}

impl RegistrationProgressRenderer {
    /// Create a renderer using stderr when it is interactive and a hidden draw
    /// target otherwise.
    #[must_use]
    pub fn new() -> Self {
        Self::with_interactive(std::io::stderr().is_terminal())
    }

    /// Create a renderer with an explicit terminal mode, useful for tests and
    /// callers that already know whether output is interactive.
    #[must_use]
    pub fn with_interactive(interactive: bool) -> Self {
        let draw_target = if interactive {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        };
        let multi = MultiProgress::with_draw_target(draw_target);
        Self {
            state: Arc::new(Mutex::new(RendererState {
                multi,
                aggregate: None,
                spinner: None,
                active_artifacts: BTreeMap::new(),
                positions: BTreeMap::new(),
                totals: BTreeMap::new(),
            })),
        }
    }

    /// Build the thread-safe sink consumed by `Cards::register_with_progress`.
    #[must_use]
    pub fn sink(&self) -> RegistrationProgressSink {
        let state = Arc::clone(&self.state);
        Arc::new(move |event| handle_event(&state, event))
    }

    /// Remove every spinner and progress bar from the terminal.
    pub fn finish_and_clear(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.clear_all();
    }
}

impl Default for RegistrationProgressRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RegistrationProgressRenderer {
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) == 1 {
            self.finish_and_clear();
        }
    }
}

struct RendererState {
    multi: MultiProgress,
    aggregate: Option<ProgressBar>,
    spinner: Option<ProgressBar>,
    active_artifacts: BTreeMap<RelativeArtifactPath, ProgressBar>,
    positions: BTreeMap<RelativeArtifactPath, u64>,
    totals: BTreeMap<RelativeArtifactPath, Option<u64>>,
}

impl RendererState {
    fn clear_all(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_and_clear();
        }
        for (_, bar) in std::mem::take(&mut self.active_artifacts) {
            bar.finish_and_clear();
        }
        if let Some(aggregate) = self.aggregate.take() {
            aggregate.finish_and_clear();
        }
    }

    fn clear_spinner(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_and_clear();
        }
    }

    fn clear_artifacts(&mut self) {
        for (_, bar) in std::mem::take(&mut self.active_artifacts) {
            bar.finish_and_clear();
        }
    }

    fn ensure_artifact_bar(
        &mut self,
        artifact: &RelativeArtifactPath,
        total_bytes: Option<u64>,
    ) -> ProgressBar {
        if let Some(bar) = self.active_artifacts.get(artifact) {
            return bar.clone();
        }
        let bar = ProgressBar::new(total_bytes.unwrap_or(u64::MAX).max(1));
        let _ = self.multi.clear();
        let bar = self.multi.add(bar);
        bar.set_style(artifact_style(total_bytes.is_some()));
        bar.set_message(artifact.to_string());
        self.active_artifacts.insert(artifact.clone(), bar.clone());
        bar
    }

    fn update_aggregate(&self) {
        let uploaded = self.positions.values().sum();
        let Some(aggregate) = &self.aggregate else {
            return;
        };
        let known_total = self.totals.values().all(Option::is_some);
        aggregate.set_style(aggregate_style(known_total));
        if known_total {
            let total: u64 = self.totals.values().flatten().sum();
            aggregate.set_length(total.max(1));
        } else {
            aggregate.set_length(u64::MAX);
        }
        aggregate.set_position(uploaded);
    }

    fn ensure_aggregate(&mut self) {
        if self.aggregate.is_none() {
            let aggregate = self.multi.add(ProgressBar::new(u64::MAX));
            aggregate.set_style(aggregate_style(false));
            aggregate.set_message("uploading artifacts");
            self.aggregate = Some(aggregate);
        }
    }
}

fn handle_event(state: &Arc<Mutex<RendererState>>, event: RegistrationProgressEvent) {
    let Ok(mut state) = state.lock() else {
        return;
    };
    match event {
        RegistrationProgressEvent::Phase(phase) => handle_phase(&mut state, phase),
        RegistrationProgressEvent::ArtifactStarted {
            artifact,
            total_bytes,
        } => {
            state.clear_spinner();
            state.ensure_aggregate();
            state.positions.insert(artifact.clone(), 0);
            state.totals.insert(artifact.clone(), total_bytes);
            state.ensure_artifact_bar(&artifact, total_bytes);
            state.update_aggregate();
        }
        RegistrationProgressEvent::ArtifactProgress {
            artifact,
            uploaded_bytes,
            total_bytes,
        } => {
            state.clear_spinner();
            state.ensure_aggregate();
            state.positions.insert(artifact.clone(), uploaded_bytes);
            state.totals.insert(artifact.clone(), total_bytes);
            let bar = state.ensure_artifact_bar(&artifact, total_bytes);
            if let Some(total_bytes) = total_bytes {
                bar.set_style(artifact_style(true));
                bar.set_length(total_bytes.max(1));
            }
            bar.set_position(uploaded_bytes);
            state.update_aggregate();
        }
        RegistrationProgressEvent::ArtifactFinished { artifact, .. } => {
            if let Some(bar) = state.active_artifacts.remove(&artifact) {
                bar.finish_and_clear();
            }
        }
    }
}

fn handle_phase(state: &mut RendererState, phase: RegistrationPhase) {
    if matches!(phase, RegistrationPhase::Uploading) {
        state.clear_spinner();
        return;
    }
    if matches!(
        phase,
        RegistrationPhase::Completing | RegistrationPhase::CleaningUp
    ) {
        state.clear_artifacts();
        if let Some(aggregate) = state.aggregate.take() {
            aggregate.finish_and_clear();
        }
    }
    if state.spinner.is_none() {
        let spinner = state.multi.add(ProgressBar::new_spinner());
        spinner.set_style(spinner_style());
        spinner.enable_steady_tick(Duration::from_millis(100));
        state.spinner = Some(spinner);
    }
    let spinner = state
        .spinner
        .as_ref()
        .expect("invariant: spinner exists after initialization");
    spinner.set_message(phase_message(phase));
}

fn phase_message(phase: RegistrationPhase) -> &'static str {
    match phase {
        RegistrationPhase::Preparing => "preparing registration",
        RegistrationPhase::Submitting => "submitting registration",
        RegistrationPhase::Uploading => "uploading artifacts",
        RegistrationPhase::Completing => "completing registration",
        RegistrationPhase::Verifying => "verifying registration",
        RegistrationPhase::CleaningUp => "cleaning up failed registration",
    }
}

fn aggregate_style(known_total: bool) -> ProgressStyle {
    let template = if known_total {
        "{bar:40.cyan/blue} {bytes}/{total_bytes} {msg}"
    } else {
        "{bar:40.cyan/blue} {bytes} {msg}"
    };
    ProgressStyle::with_template(template)
        .expect("invariant: aggregate progress template is valid")
        .progress_chars("=>-")
}

fn artifact_style(known_total: bool) -> ProgressStyle {
    let template = if known_total {
        "{bar:28.green/blue} {bytes}/{total_bytes} {msg}"
    } else {
        "{bar:28.green/blue} {bytes} {msg}"
    };
    ProgressStyle::with_template(template)
        .expect("invariant: artifact progress template is valid")
        .progress_chars("=>-")
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.yellow} {msg}")
        .expect("invariant: registration spinner template is valid")
}

#[cfg(test)]
mod tests {
    use super::{RegistrationProgressRenderer, handle_event};
    use wyrd_client::cards::{RegistrationPhase, RegistrationProgressEvent};
    use wyrd_spec::registry::RelativeArtifactPath;

    #[test]
    fn hidden_renderer_clears_active_artifacts() {
        let renderer = RegistrationProgressRenderer::with_interactive(false);
        let artifact = RelativeArtifactPath::new("weights.bin").expect("test path");
        let sink = renderer.sink();
        sink(RegistrationProgressEvent::Phase(
            RegistrationPhase::Uploading,
        ));
        sink(RegistrationProgressEvent::ArtifactStarted {
            artifact: artifact.clone(),
            total_bytes: Some(10),
        });
        sink(RegistrationProgressEvent::ArtifactProgress {
            artifact: artifact.clone(),
            uploaded_bytes: 10,
            total_bytes: Some(10),
        });
        sink(RegistrationProgressEvent::ArtifactFinished {
            artifact,
            success: false,
        });
        renderer.finish_and_clear();
        drop(sink);
    }

    #[test]
    fn renderer_handles_phase_cleanup_without_a_live_upload_bar() {
        let renderer = RegistrationProgressRenderer::with_interactive(false);
        let state = renderer.state.clone();
        handle_event(
            &state,
            RegistrationProgressEvent::Phase(RegistrationPhase::CleaningUp),
        );
        renderer.finish_and_clear();
    }

    #[test]
    fn upload_bars_do_not_start_in_finished_state() {
        let renderer = RegistrationProgressRenderer::with_interactive(false);
        let state = renderer.state.clone();
        let artifact = RelativeArtifactPath::new("weights.bin").expect("test path");

        handle_event(
            &state,
            RegistrationProgressEvent::ArtifactStarted {
                artifact,
                total_bytes: Some(10),
            },
        );

        let state = state.lock().expect("test mutex is not poisoned");
        assert!(
            !state
                .aggregate
                .as_ref()
                .expect("aggregate bar is created")
                .is_finished()
        );
        assert!(
            state
                .active_artifacts
                .values()
                .all(|bar| !bar.is_finished())
        );
        drop(state);
        renderer.finish_and_clear();
    }
}
