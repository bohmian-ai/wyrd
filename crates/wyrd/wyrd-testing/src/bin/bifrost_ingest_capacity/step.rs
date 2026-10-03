//! One measured step: open-loop Drift emission through a fresh `WyrdState`,
//! then drain, durable read-back, and the server's own accounting.
//!
//! Every observation carries [`FEATURES`] numeric features and so lands
//! [`FEATURES`] rows under one `record_id`. Admission is all-or-none per
//! observation, so a `QUEUE_FULL` refusal admitted no row; open-loop pacing
//! counts it and moves on rather than resubmitting.

use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_client::QueueConfig;
use wyrd_client::state::WyrdState;
use wyrd_testing::release_server::{LocalServer, Metrics};

use crate::Result;
use crate::fixture::{Tenant, window};
use crate::proxy::DelayProxy;

/// Numeric features, and so durable rows, of one observation (AC-041).
pub const FEATURES: usize = 100;

/// Client-owned byte sampling interval.
const SAMPLE_EVERY: Duration = Duration::from_millis(250);

/// Trailing window whose byte slope decides growth.
const SLOPE_WINDOW: f64 = 10.0;

/// The longest drain AC-041 accepts after load stops.
const DRAIN_LIMIT: f64 = 1.0;

/// How long admitted rows may take to become queryable before the step
/// records what it found; generous because Scribe publishes on its own
/// cadence, and a miss is reported, never hidden.
const DURABLE_WAIT: Duration = Duration::from_secs(300);

/// Refusal code of a saturated producer.
const QUEUE_FULL: &str = "WYRD_CLIENT_429_QUEUE_FULL";

/// What one step offers.
pub struct Plan {
    /// Report label.
    pub name: String,
    /// Offered observations per second.
    pub rate: f64,
    /// Arrival window, seconds.
    pub seconds: f64,
    /// The producer's in-flight sends; the queue default otherwise.
    pub max_in_flight: usize,
    /// Injected server→client delay, when the step runs through the proxy.
    pub ack_delay: Option<Duration>,
    /// Whether the step's AC-041 checks decide the run.
    pub gating: bool,
}

/// One step's measurements and the AC-041 checks applied to them.
#[derive(Debug, Serialize)]
pub struct Record {
    /// Report label.
    pub name: String,
    /// Whether the checks decide the run.
    pub gating: bool,
    /// Offered observations per second.
    pub rate: f64,
    /// Arrival window, seconds.
    pub seconds: f64,
    /// The producer's in-flight sends.
    pub max_in_flight: usize,
    /// Injected server→client delay, milliseconds; zero without the proxy.
    pub ack_delay_ms: u64,
    /// Observations offered.
    pub offered: u64,
    /// Observations admitted.
    pub admitted: u64,
    /// Observations refused with `QUEUE_FULL`.
    pub refusals: u64,
    /// Admitted observations per second of emission.
    pub achieved_rate: f64,
    /// Largest sampled client-owned byte count.
    pub max_client_bytes: usize,
    /// Least-squares client-owned byte slope over the trailing window, bytes/s.
    pub client_bytes_slope: f64,
    /// Seconds `flush` took once load stopped.
    pub drain_seconds: f64,
    /// Rows the admitted observations land.
    pub expected_rows: u64,
    /// Rows durable in the step's window.
    pub durable_rows: u64,
    /// `record_id`s in the window whose row count is not [`FEATURES`].
    pub uneven_records: u64,
    /// Frames the server's Gate accepted during the step.
    pub batches: f64,
    /// Mean durable rows per accepted frame.
    pub mean_batch_rows: Option<f64>,
    /// Mean Arrow IPC bytes per frame the Gate received.
    pub mean_batch_bytes: Option<f64>,
    /// Gate write duration p50 bucket bound, seconds.
    pub send_p50_seconds: Option<f64>,
    /// Gate write duration p99 bucket bound, seconds.
    pub send_p99_seconds: Option<f64>,
    /// Client process CPU per durable row, microseconds.
    pub client_cpu_us_per_row: Option<f64>,
    /// Server cgroup CPU per durable row, microseconds.
    pub server_cpu_us_per_row: Option<f64>,
    /// Every failed AC-041 check, empty when the step passes.
    pub failures: Vec<String>,
}

/// The Gate counters one `/metrics` scrape and the server cgroup report.
struct ServerSnapshot {
    /// Frames the Gate accepted.
    frames: f64,
    /// Arrow IPC bytes the Gate received.
    frame_bytes: f64,
    /// Cumulative Gate write-duration buckets.
    buckets: Vec<(f64, f64)>,
    /// Server cgroup CPU, microseconds.
    cpu_usec: u64,
}

impl ServerSnapshot {
    /// Scrapes `server`.
    ///
    /// # Errors
    ///
    /// Returns the scrape failure.
    async fn take(server: &LocalServer) -> Result<Self> {
        let metrics: Metrics = server.metrics().await?;
        Ok(Self {
            frames: metrics.sum("bifrost_gate_frames_total", &["status=\"accepted\""]),
            frame_bytes: metrics.sum("bifrost_gate_frame_bytes_total", &[]),
            buckets: metrics.buckets(
                "bifrost_gate_request_duration_seconds",
                &["operation=\"write\""],
            ),
            cpu_usec: server.cgroup_stat("cpu.stat", "usage_usec"),
        })
    }

    /// The smallest bucket bound holding quantile `q` of the writes between
    /// `before` and `self`.
    fn quantile(&self, before: &Self, q: f64) -> Option<f64> {
        let delta: Vec<(f64, f64)> = self
            .buckets
            .iter()
            .map(|(bound, count)| {
                let earlier = before
                    .buckets
                    .iter()
                    .find(|(other, _)| other == bound)
                    .map_or(0.0, |(_, count)| *count);
                (*bound, count - earlier)
            })
            .collect();
        let total = delta.last()?.1;
        (total > 0.0)
            .then(|| delta.iter().find(|(_, count)| *count >= q * total))
            .flatten()
            .map(|(bound, _)| *bound)
    }
}

/// The server and tenant every step measures.
pub struct Bench<'a> {
    /// The release server.
    pub server: &'a LocalServer,
    /// The provisioned tenant.
    pub tenant: &'a Tenant,
}

impl Bench<'_> {
    /// Runs `plan`: emits, drains, waits for durability, and judges.
    ///
    /// # Errors
    ///
    /// Returns a client start, emit (other than `QUEUE_FULL`), flush,
    /// shutdown, query, or scrape failure.
    pub async fn run(&self, plan: &Plan) -> Result<Record> {
        let proxy = match plan.ack_delay {
            Some(delay) => Some(DelayProxy::start(grpc_target(), delay).await?),
            None => None,
        };
        let client = self
            .tenant
            .service(proxy.as_ref().map(DelayProxy::url).as_deref())?;
        let state = WyrdState::from_path(&self.tenant.bundle)?;
        state
            .start_bifrost_with_config(
                &client,
                None,
                QueueConfig {
                    max_in_flight: plan.max_in_flight,
                    ..QueueConfig::default()
                },
            )
            .await?;
        let model = state.run_for_card("model")?;

        let start = Utc::now() - chrono::TimeDelta::milliseconds(500);
        let server_before = ServerSnapshot::take(self.server).await?;
        let client_cpu_before = client_cpu_seconds()?;
        let stop = CancellationToken::new();
        let sampler = tokio::spawn(sample(state.clone(), stop.clone()));

        let offered = (plan.rate * plan.seconds).round() as u64;
        let mut features: serde_json::Map<String, serde_json::Value> = (0..FEATURES)
            .map(|feature| (format!("f{feature}"), 0.0.into()))
            .collect();
        let (mut admitted, mut refusals) = (0_u64, 0_u64);
        let began = Instant::now();
        for sequence in 0..offered {
            let due = began + Duration::from_secs_f64(sequence as f64 / plan.rate);
            if Instant::now() < due {
                tokio::time::sleep_until(due).await;
            }
            for (feature, value) in features.values_mut().enumerate() {
                *value = (((sequence as usize + feature * 7) % 100) as f64).into();
            }
            match model.observe().drift(&features, None) {
                Ok(()) => admitted += 1,
                Err(error) if error.code() == QUEUE_FULL => refusals += 1,
                Err(error) => return Err(error.into()),
            }
        }
        let emitted = began.elapsed().as_secs_f64();
        stop.cancel();
        let samples = sampler.await?;

        let draining = Instant::now();
        state.flush().await?;
        let drain_seconds = draining.elapsed().as_secs_f64();
        let client_cpu = client_cpu_seconds()? - client_cpu_before;
        state.shutdown().await?;
        drop(proxy);

        let end = Utc::now() + chrono::TimeDelta::milliseconds(500);
        let expected_rows = admitted * FEATURES as u64;
        let durable_rows = self.durable(&window(start, end), expected_rows).await?;
        let uneven_records = self
            .tenant
            .count(&format!(
                "SELECT COUNT(*) AS n FROM (SELECT record_id FROM vala.drift.observations \
                 WHERE {} GROUP BY record_id HAVING COUNT(*) <> {FEATURES}) AS uneven",
                window(start, end)
            ))
            .await?;
        let server_after = ServerSnapshot::take(self.server).await?;

        let batches = server_after.frames - server_before.frames;
        let per_row = |total: f64| (durable_rows > 0).then(|| total / durable_rows as f64);
        let mut record = Record {
            name: plan.name.clone(),
            gating: plan.gating,
            rate: plan.rate,
            seconds: plan.seconds,
            max_in_flight: plan.max_in_flight,
            ack_delay_ms: plan.ack_delay.map_or(0, |delay| delay.as_millis() as u64),
            offered,
            admitted,
            refusals,
            achieved_rate: admitted as f64 / emitted,
            max_client_bytes: samples.iter().map(|(_, bytes)| *bytes).max().unwrap_or(0),
            client_bytes_slope: slope(&samples),
            drain_seconds,
            expected_rows,
            durable_rows,
            uneven_records,
            batches,
            mean_batch_rows: (batches > 0.0).then(|| durable_rows as f64 / batches),
            mean_batch_bytes: (batches > 0.0)
                .then(|| (server_after.frame_bytes - server_before.frame_bytes) / batches),
            send_p50_seconds: server_after.quantile(&server_before, 0.5),
            send_p99_seconds: server_after.quantile(&server_before, 0.99),
            client_cpu_us_per_row: per_row(client_cpu * 1e6),
            server_cpu_us_per_row: per_row(
                server_after.cpu_usec.saturating_sub(server_before.cpu_usec) as f64,
            ),
            failures: Vec::new(),
        };
        record.failures = record.judge();
        Ok(record)
    }

    /// Polls the rows durable in `filter` until they reach `expected` or
    /// [`DURABLE_WAIT`] passes, returning the last count.
    ///
    /// # Errors
    ///
    /// Returns the query failure.
    async fn durable(&self, filter: &str, expected: u64) -> Result<u64> {
        let sql = format!("SELECT COUNT(*) AS n FROM vala.drift.observations WHERE {filter}");
        let deadline = Instant::now() + DURABLE_WAIT;
        loop {
            let rows = self.tenant.count(&sql).await?;
            if rows >= expected || Instant::now() > deadline {
                return Ok(rows);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

impl Record {
    /// The AC-041 checks this step fails: any refusal, client bytes growing
    /// over the trailing window by more than one message, a drain over one
    /// second, or durable rows that are not exactly [`FEATURES`] per admitted
    /// observation.
    fn judge(&self) -> Vec<String> {
        let mut failures = Vec::new();
        if self.refusals > 0 {
            failures.push(format!("{} QUEUE_FULL refusals", self.refusals));
        }
        // Sealing works in message-sized steps, so growth below one message
        // over the window is batching noise, not accumulation.
        let growth = self.client_bytes_slope * SLOPE_WINDOW;
        if growth > QueueConfig::default().max_message_bytes as f64 {
            failures.push(format!(
                "client bytes grew {growth:.0} B over the last {SLOPE_WINDOW} s"
            ));
        }
        if self.drain_seconds > DRAIN_LIMIT {
            failures.push(format!("drain took {:.3} s", self.drain_seconds));
        }
        if self.durable_rows != self.expected_rows {
            failures.push(format!(
                "{} durable rows, expected {}",
                self.durable_rows, self.expected_rows
            ));
        }
        if self.uneven_records > 0 {
            failures.push(format!(
                "{} record_ids without {FEATURES} rows",
                self.uneven_records
            ));
        }
        failures
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
        .map(|(time, bytes)| (*time, *bytes as f64))
        .collect();
    let count = tail.len() as f64;
    if tail.len() < 2 {
        return 0.0;
    }
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

/// This process's user plus system CPU, seconds, from `/proc/self/stat`.
///
/// # Errors
///
/// Returns an error when the file is unreadable or malformed.
fn client_cpu_seconds() -> Result<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat")?;
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .ok_or("malformed /proc/self/stat")?
        .1
        .split_whitespace()
        .collect();
    let ticks = |index: usize| -> Result<u64> {
        Ok(fields.get(index).ok_or("short /proc/self/stat")?.parse()?)
    };
    // ponytail: assumes USER_HZ = 100, Linux's fixed userspace tick on every
    // mainstream architecture; read sysconf(_SC_CLK_TCK) if that ever differs.
    Ok((ticks(11)? + ticks(12)?) as f64 / 100.0)
}

/// The server's default gRPC listener, which the delay proxy fronts.
fn grpc_target() -> std::net::SocketAddr {
    std::net::SocketAddr::from(([127, 0, 0, 1], 50051))
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
        assert_eq!(slope(&[(0.0, 7)]), 0.0);
    }
}
