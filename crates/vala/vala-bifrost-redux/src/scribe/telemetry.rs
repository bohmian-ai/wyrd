//! Optional stage measurements for real Scribe workload runs.

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One completed Scribe stage sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeStageSample {
    /// Stable stage name.
    pub stage: String,
    /// Stage duration in microseconds.
    pub duration_us: u64,
    /// Rows handled by the stage when known.
    pub rows: u64,
    /// Bytes handled by the stage when known.
    pub bytes: u64,
}

/// Opt-in, bounded-run recorder used by benchmark and journey harnesses.
#[derive(Debug, Clone, Default)]
pub struct ScribeTelemetry {
    samples: Arc<Mutex<Vec<ScribeStageSample>>>,
}

impl ScribeTelemetry {
    /// Record one completed stage without changing the production path when
    /// no recorder is attached.
    pub fn record(&self, stage: &str, elapsed: Duration, rows: usize, bytes: usize) {
        let sample = ScribeStageSample {
            stage: stage.to_owned(),
            duration_us: u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX),
            rows: u64::try_from(rows).unwrap_or(u64::MAX),
            bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
        };
        if let Ok(mut samples) = self.samples.lock() {
            samples.push(sample);
        }
    }

    /// Return a detached snapshot of all samples recorded so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ScribeStageSample> {
        self.samples
            .lock()
            .map(|samples| samples.clone())
            .unwrap_or_default()
    }
}
