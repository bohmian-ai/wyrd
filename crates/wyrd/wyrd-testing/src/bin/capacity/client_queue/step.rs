//! One client-queue lane: open-loop Drift emission through a fresh
//! `WyrdState` until a deadline, then drain, durable read-back, and the
//! client-owned byte trend.
//!
//! Every observation carries [`FEATURES`] numeric features and so lands
//! [`FEATURES`] rows under one `record_id`. Admission is all-or-none per
//! observation, so a `QUEUE_FULL` refusal admitted no row; open-loop pacing
//! records when it happened and moves on rather than resubmitting.

use std::time::Duration;

use num_traits::ToPrimitive;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_client::QueueConfig;
use wyrd_client::state::WyrdState;

use crate::client_queue::Result;
use crate::client_queue::fixture::{DRIFT_VERIFIER, Tenant};
use crate::client_queue::proxy::DelayProxy;

/// Numeric features, and so durable rows, of one observation.
pub const FEATURES: usize = 100;

/// Client-owned byte sampling interval.
const SAMPLE_EVERY: Duration = Duration::from_millis(250);

/// Trailing window whose byte slope decides growth, seconds.
const SLOPE_WINDOW: f64 = 10.0;

/// How long admitted rows may take to become queryable before the lane
/// records what it found; a shortfall is a mismatch, never hidden.
const DURABLE_WAIT: Duration = Duration::from_secs(60);

/// Refusal code of a saturated producer.
const QUEUE_FULL: &str = "WYRD_CLIENT_429_QUEUE_FULL";

/// The server's default gRPC listener, which the delay proxy fronts.
const GRPC_TARGET: ([u8; 4], u16) = ([127, 0, 0, 1], 50051);

/// One lane's declared shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lane {
    /// Stable lane id, used in metric ids.
    pub name: &'static str,
    /// Offered observations per second.
    pub rate: f64,
    /// The producer's in-flight sends, or `None` for the queue default.
    pub max_in_flight: Option<usize>,
    /// Injected server→client delay, when the lane runs through the proxy.
    pub ack_delay: Option<Duration>,
}

/// The lanes every L2 repetition runs: headroom above the sustained rate,
/// late acknowledgements, and a single in-flight send.
pub const LANES: [Lane; 3] = [
    Lane {
        name: "headroom",
        rate: 1_000.0,
        max_in_flight: None,
        ack_delay: None,
    },
    Lane {
        name: "ack-delay",
        rate: 500.0,
        max_in_flight: None,
        ack_delay: Some(Duration::from_millis(50)),
    },
    Lane {
        name: "in-flight",
        rate: 500.0,
        max_in_flight: Some(1),
        ack_delay: None,
    },
];

/// What one lane observed.
#[derive(Debug)]
pub struct Emitted {
    /// When each `QUEUE_FULL` refusal answered, for the caller's phase
    /// classification.
    pub refusals: Vec<Instant>,
    /// Admitted observations per second of emission.
    pub achieved_rate: f64,
    /// Least-squares growth of client-owned bytes over the trailing
    /// [`SLOPE_WINDOW`], bytes.
    pub bytes_growth: f64,
    /// Seconds `flush` took once emission stopped.
    pub drain_seconds: f64,
    /// `|durable − expected|` rows under the lane's `run_id`, plus every
    /// `record_id` without exactly [`FEATURES`] rows.
    pub row_mismatch: u64,
}

impl Lane {
    /// Emits at [`Self::rate`] through a fresh `WyrdState` of `tenant` until
    /// `until`, then flushes, shuts down, and reads the lane's rows back.
    ///
    /// # Errors
    ///
    /// Returns a proxy, client start, emit (other than `QUEUE_FULL`), flush,
    /// shutdown, or query failure.
    pub async fn run(&self, tenant: &Tenant, until: Instant) -> Result<Emitted> {
        let proxy = match self.ack_delay {
            Some(delay) => Some(DelayProxy::start(GRPC_TARGET.into(), delay).await?),
            None => None,
        };
        let client = tenant.service(proxy.as_ref().map(DelayProxy::url).as_deref())?;
        let state = WyrdState::from_path_with(&tenant.bundle, client)?;
        let defaults = QueueConfig::default();
        state
            .start_bifrost_with_config(
                None,
                QueueConfig {
                    max_in_flight: self.max_in_flight.unwrap_or(defaults.max_in_flight),
                    ..defaults
                },
            )
            .await?;
        let model = state.run_for_card("model")?;
        let stop = CancellationToken::new();
        let sampler = tokio::spawn(sample(state.clone(), stop.clone()));

        let mut features: serde_json::Map<String, serde_json::Value> = (0..FEATURES)
            .map(|feature| (format!("f{feature}"), 0.0.into()))
            .collect();
        let mut admitted = 0_u64;
        let mut refusals = Vec::new();
        let began = Instant::now();
        for sequence in 0_u64.. {
            let due =
                began + Duration::from_secs_f64(sequence.to_f64().unwrap_or(f64::MAX) / self.rate);
            if due >= until {
                break;
            }
            tokio::time::sleep_until(due).await;
            for (feature, value) in (0_u64..).zip(features.values_mut()) {
                *value = ((sequence + feature * 7) % 100)
                    .to_f64()
                    .unwrap_or(0.0)
                    .into();
            }
            match model
                .observe()
                .submit(DRIFT_VERIFIER, serde_json::Value::Object(features.clone()))
                .await
            {
                Ok(_) => admitted += 1,
                Err(error) if error.code() == QUEUE_FULL => refusals.push(Instant::now()),
                Err(error) => return Err(error.into()),
            }
        }
        let emitted = began.elapsed().as_secs_f64();
        stop.cancel();
        let samples = sampler.await?;

        let draining = Instant::now();
        state.flush().await?;
        let drain_seconds = draining.elapsed().as_secs_f64();
        state.shutdown().await?;
        drop(proxy);

        let own_rows = format!("run_id = '{}'", model.run_id().as_str());
        let expected = admitted * FEATURES as u64;
        let durable = durable(tenant, &own_rows, expected).await?;
        let uneven = tenant
            .count(&format!(
                "SELECT COUNT(*) AS n FROM (SELECT record_id FROM bifrost.drift.observations \
                 WHERE {own_rows} GROUP BY record_id HAVING COUNT(*) <> {FEATURES}) AS uneven"
            ))
            .await?;
        Ok(Emitted {
            refusals,
            achieved_rate: admitted.to_f64().unwrap_or(0.0) / emitted,
            bytes_growth: slope(&samples) * SLOPE_WINDOW,
            drain_seconds,
            row_mismatch: durable.abs_diff(expected) + uneven,
        })
    }
}

/// Polls the rows durable under `filter` until they reach `expected` or
/// [`DURABLE_WAIT`] passes, returning the last count.
///
/// # Errors
///
/// Returns the query failure.
async fn durable(tenant: &Tenant, filter: &str, expected: u64) -> Result<u64> {
    let sql = format!("SELECT COUNT(*) AS n FROM bifrost.drift.observations WHERE {filter}");
    let deadline = Instant::now() + DURABLE_WAIT;
    loop {
        let rows = tenant.count(&sql).await?;
        if rows >= expected || Instant::now() > deadline {
            return Ok(rows);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Samples the state's client-owned bytes every [`SAMPLE_EVERY`] until
/// `stop`, as `(seconds since the first sample, bytes)`.
async fn sample(state: WyrdState, stop: CancellationToken) -> Vec<(f64, usize)> {
    let mut samples = Vec::new();
    let mut tick = tokio::time::interval(SAMPLE_EVERY);
    let first = Instant::now();
    loop {
        tokio::select! {
            () = stop.cancelled() => return samples,
            _ = tick.tick() => {
                if let Ok(metrics) = state.bifrost_metrics() {
                    samples.push((first.elapsed().as_secs_f64(), metrics.owned_bytes));
                }
            }
        }
    }
}

/// Least-squares slope, bytes per second, of the samples in the trailing
/// [`SLOPE_WINDOW`]; zero with fewer than two.
fn slope(samples: &[(f64, usize)]) -> f64 {
    let Some(last) = samples.last().map(|(time, _)| *time) else {
        return 0.0;
    };
    let tail: Vec<(f64, f64)> = samples
        .iter()
        .filter(|(time, _)| *time >= last - SLOPE_WINDOW)
        .map(|(time, bytes)| (*time, bytes.to_f64().unwrap_or(f64::MAX)))
        .collect();
    if tail.len() < 2 {
        return 0.0;
    }
    let count = tail.len().to_f64().unwrap_or(f64::MAX);
    let mean_t = tail.iter().map(|(time, _)| time).sum::<f64>() / count;
    let mean_b = tail.iter().map(|(_, bytes)| bytes).sum::<f64>() / count;
    let covariance: f64 = tail
        .iter()
        .map(|(time, bytes)| (time - mean_t) * (bytes - mean_b))
        .sum();
    let variance: f64 = tail.iter().map(|(time, _)| (time - mean_t).powi(2)).sum();
    if variance == 0.0 {
        0.0
    } else {
        covariance / variance
    }
}

#[cfg(test)]
mod tests {
    use super::slope;

    /// Proves the slope reads a steady climb, ignores samples older than the
    /// trailing window, and reads a flat series as zero.
    ///
    /// # Panics
    ///
    /// Panics when a slope is wrong.
    #[test]
    fn slope_reads_the_trailing_window() {
        let climbing: Vec<(f64, usize)> = (0..20_u32)
            .map(|step| (f64::from(step), step as usize * 10))
            .collect();
        assert!((slope(&climbing) - 10.0).abs() < 1e-9);
        let settled: Vec<(f64, usize)> = (0..30_u32)
            .map(|step| {
                (
                    f64::from(step),
                    if step < 15 { step as usize * 1_000 } else { 5 },
                )
            })
            .collect();
        assert!(slope(&settled).abs() < 1e-9);
        assert_eq!(slope(&[(0.0, 7)]).to_bits(), 0.0_f64.to_bits());
    }
}
