//! Client query scheduling: fixed-rate open loop and fixed-concurrency closed
//! loop, both producing the same raw samples.
//!
//! [`FixedRateDriver`] launches on a wall-clock arrival schedule, never on the
//! completion of earlier requests, so a slow server shows up as latency and
//! refusals rather than as a silently lowered offered rate. In-flight client
//! work is bounded: an arrival that finds the bound full is counted as a
//! missed launch instead of queueing without limit.
//!
//! [`ClosedLoopDriver`] keeps a fixed number of clients busy, each issuing its
//! next query as soon as the previous one ends, which is how a concurrency
//! sweep finds the rate a given client count can sustain.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::Instant;

/// Terminal category of one short query, each reported separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortQueryOutcome {
    /// Terminal `Success` with exactly the expected IDs; the only outcome that
    /// counts toward successful throughput.
    Success,
    /// Terminal `Degraded`: a missing live tail is a different result.
    Degraded,
    /// Terminal `Failed` for any reason other than its deadline.
    Failed,
    /// Refused by Oracle admission before a stream opened.
    AdmissionRefused,
    /// Refused or failed because the query deadline elapsed.
    Deadline,
    /// The request or stream failed below the query contract.
    TransportError,
    /// Terminal `Success` whose IDs were not the expected ones.
    WrongResult,
    /// Refused by a peer security check; never expected under ordinary load.
    SecurityRefused,
}

impl ShortQueryOutcome {
    /// Every category, in report column order.
    pub const ALL: [Self; 8] = [
        Self::Success,
        Self::Degraded,
        Self::Failed,
        Self::AdmissionRefused,
        Self::Deadline,
        Self::TransportError,
        Self::WrongResult,
        Self::SecurityRefused,
    ];
}

/// What one issued query reports back to the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeResult {
    /// Terminal category.
    pub outcome: ShortQueryOutcome,
    /// When the first result row arrived, if one did.
    pub first_row_at: Option<Instant>,
}

/// One raw client sample, with times relative to the measured window's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ShortQuerySample {
    /// Arrival sequence number within the window.
    pub sequence: u64,
    /// Scheduled arrival.
    pub scheduled_us: u64,
    /// Actual send, after any driver lag.
    pub sent_us: u64,
    /// Send-to-first-row latency, when a row arrived.
    pub first_row_latency_us: Option<u64>,
    /// Send-to-terminal latency.
    pub terminal_latency_us: u64,
    /// Terminal category.
    pub outcome: ShortQueryOutcome,
    /// True when the terminal arrived after the measured window closed.
    pub drain_completion: bool,
}

/// Fixed-rate open-loop scheduler over a bounded in-flight budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedRateDriver {
    /// Offered arrivals per second.
    rate_per_second: u64,
    /// Most requests the driver keeps in flight at once.
    max_in_flight: usize,
}

/// Everything one measured window produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixedRateRun {
    /// Measured window length.
    pub window: Duration,
    /// Arrivals the schedule contained.
    pub scheduled: u64,
    /// Arrivals actually sent.
    pub sent: u64,
    /// Arrivals dropped because the in-flight bound was full.
    pub missed_launches: u64,
    /// Sent requests with no terminal by the end of drain, then aborted.
    pub abandoned: u64,
    /// Largest delay between a scheduled arrival and its launch decision.
    pub max_launch_lag: Duration,
    /// Every terminal sample, in completion order.
    pub samples: Vec<ShortQuerySample>,
}

impl FixedRateDriver {
    /// Offers `rate_per_second` arrivals with at most `max_in_flight` open.
    #[must_use]
    pub fn new(rate_per_second: u64, max_in_flight: usize) -> Self {
        Self {
            rate_per_second: rate_per_second.max(1),
            max_in_flight: max_in_flight.max(1),
        }
    }

    /// Runs one measured window, then drains.
    ///
    /// Arrival `n` is scheduled at `n / rate` seconds after the window starts.
    /// At each arrival the driver takes an in-flight permit without waiting; a
    /// full budget records a missed launch. A launched request records its send
    /// instant immediately before `issue(n)` is first polled, and its terminal
    /// when that future resolves. After the window, drain waits up to
    /// `drain_timeout` for outstanding requests; terminals that arrive then are
    /// kept but marked as drain completions, and requests still open are
    /// aborted and counted as abandoned.
    ///
    /// # Panics
    ///
    /// Never deliberately; a panicking `issue` future is counted as abandoned.
    pub async fn run<F, Fut>(
        &self,
        window: Duration,
        drain_timeout: Duration,
        issue: F,
    ) -> FixedRateRun
    where
        F: Fn(u64) -> Fut,
        Fut: Future<Output = ProbeResult> + Send + 'static,
    {
        let permits = Arc::new(Semaphore::new(self.max_in_flight));
        let start = Instant::now();
        let end = start + window;
        let mut tasks = JoinSet::new();
        let mut run = FixedRateRun {
            window,
            ..FixedRateRun::default()
        };
        // Measurement: launch on the wall-clock schedule, never on completions.
        for sequence in 0_u64.. {
            let offset =
                Duration::from_nanos(sequence.saturating_mul(1_000_000_000) / self.rate_per_second);
            let scheduled = start + offset;
            if scheduled >= end {
                break;
            }
            tokio::time::sleep_until(scheduled).await;
            run.scheduled += 1;
            run.max_launch_lag = run.max_launch_lag.max(Instant::now() - scheduled);
            let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                run.missed_launches += 1;
                continue;
            };
            run.sent += 1;
            let request = issue(sequence);
            tasks.spawn(async move {
                let sent = Instant::now();
                let probe = request.await;
                let terminal = Instant::now();
                drop(permit);
                (sequence, scheduled, sent, probe, terminal)
            });
            while let Some(done) = tasks.try_join_next() {
                run.record_joined(done, start, end);
            }
        }
        // Drain: collect what finishes in time, abort the rest.
        let drain_deadline = end + drain_timeout;
        while !tasks.is_empty() {
            match tokio::time::timeout_at(drain_deadline, tasks.join_next()).await {
                Ok(Some(done)) => run.record_joined(done, start, end),
                Ok(None) => break,
                Err(_) => {
                    run.abandoned += u64::try_from(tasks.len()).unwrap_or(u64::MAX);
                    tasks.abort_all();
                    while tasks.join_next().await.is_some() {}
                    break;
                }
            }
        }
        run
    }
}

/// Fixed-concurrency closed-loop scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedLoopDriver {
    /// Clients issuing queries back to back.
    concurrency: usize,
}

impl ClosedLoopDriver {
    /// Keeps `concurrency` clients, at least one, busy.
    #[must_use]
    pub fn new(concurrency: usize) -> Self {
        Self {
            concurrency: concurrency.max(1),
        }
    }

    /// Runs until `window` has elapsed or `limit` queries have been issued.
    ///
    /// Each client takes the next sequence number, issues `issue(sequence)`,
    /// waits for its terminal, and repeats; it starts nothing once the window
    /// has closed or the limit is reached. Terminals of queries still running
    /// when the window closes are kept but marked as drain completions. The
    /// returned run's window is the shorter of `window` and the time until the
    /// last terminal, so a count-limited run reports its real duration and
    /// every one of its terminals as measured. Every sample is sent on its own
    /// schedule, so scheduled and sent times are equal.
    ///
    /// # Panics
    ///
    /// Never deliberately; a panicking client is counted as abandoned.
    pub async fn run<F, Fut>(&self, window: Duration, limit: u64, issue: F) -> FixedRateRun
    where
        F: Fn(u64) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ProbeResult> + Send + 'static,
    {
        let issue = Arc::new(issue);
        let next = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let end = start + window;
        let mut clients = JoinSet::new();
        for _ in 0..self.concurrency {
            let (issue, next) = (Arc::clone(&issue), Arc::clone(&next));
            clients.spawn(async move {
                let mut done = Vec::new();
                while Instant::now() < end {
                    let sequence = next.fetch_add(1, Ordering::Relaxed);
                    if sequence >= limit {
                        break;
                    }
                    let sent = Instant::now();
                    let probe = issue(sequence).await;
                    done.push((sequence, sent, sent, probe, Instant::now()));
                }
                done
            });
        }
        let mut run = FixedRateRun::default();
        let mut last = start;
        while let Some(joined) = clients.join_next().await {
            let Ok(done) = joined else {
                run.abandoned += 1;
                continue;
            };
            for finished in done {
                last = last.max(finished.4);
                run.scheduled += 1;
                run.sent += 1;
                run.record(finished, start, end);
            }
        }
        run.window = window.min(last - start);
        run
    }
}

/// Microseconds from `from` to `to`, saturating at zero and `u64::MAX`.
fn micros(from: Instant, to: Instant) -> u64 {
    u64::try_from(to.saturating_duration_since(from).as_micros()).unwrap_or(u64::MAX)
}

impl FixedRateRun {
    /// Converts one joined request task into a sample.
    ///
    /// A request whose task panicked has no terminal and is counted as
    /// abandoned rather than guessed into a category.
    fn record_joined(
        &mut self,
        done: Result<(u64, Instant, Instant, ProbeResult, Instant), tokio::task::JoinError>,
        start: Instant,
        end: Instant,
    ) {
        match done {
            Ok(done) => self.record(done, start, end),
            Err(_) => self.abandoned += 1,
        }
    }

    /// Converts one finished request, `(sequence, scheduled, sent, probe,
    /// terminal)`, into a sample relative to the window `start..end`.
    fn record(
        &mut self,
        (sequence, scheduled, sent, probe, terminal): (u64, Instant, Instant, ProbeResult, Instant),
        start: Instant,
        end: Instant,
    ) {
        self.samples.push(ShortQuerySample {
            sequence,
            scheduled_us: micros(start, scheduled),
            sent_us: micros(start, sent),
            first_row_latency_us: probe.first_row_at.map(|first| micros(sent, first)),
            terminal_latency_us: micros(sent, terminal),
            outcome: probe.outcome,
            drain_completion: terminal > end,
        });
    }

    /// Terminals of `outcome` that arrived inside the measured window.
    #[must_use]
    pub fn measured(&self, outcome: ShortQueryOutcome) -> u64 {
        self.samples
            .iter()
            .filter(|sample| sample.outcome == outcome && !sample.drain_completion)
            .count() as u64
    }

    /// Terminals of any outcome that arrived during drain.
    #[must_use]
    pub fn drain_completions(&self) -> u64 {
        self.samples
            .iter()
            .filter(|sample| sample.drain_completion)
            .count() as u64
    }

    /// Measured-window `Success` terminals per second of window.
    #[must_use]
    pub fn successes_per_second(&self) -> f64 {
        let seconds = self.window.as_secs_f64();
        if seconds > 0.0 {
            self.measured(ShortQueryOutcome::Success) as f64 / seconds
        } else {
            0.0
        }
    }

    /// Nearest-rank p50/p95/p99 of measured-window `Success` latencies, in
    /// microseconds, chosen by `latency`.
    #[must_use]
    pub fn success_percentiles(
        &self,
        latency: impl Fn(&ShortQuerySample) -> Option<u64>,
    ) -> [Option<u64>; 3] {
        let values: Vec<u64> = self
            .samples
            .iter()
            .filter(|sample| {
                sample.outcome == ShortQueryOutcome::Success && !sample.drain_completion
            })
            .filter_map(latency)
            .collect();
        percentiles(values)
    }
}

/// Nearest-rank p50/p95/p99 of `values`; `None` when there are none.
///
/// The benchmark's one percentile rule, shared by query and write latencies.
#[must_use]
pub fn percentiles(mut values: Vec<u64>) -> [Option<u64>; 3] {
    values.sort_unstable();
    [0.50, 0.95, 0.99].map(|quantile| {
        let rank = (quantile * values.len() as f64).ceil() as usize;
        values.get(rank.saturating_sub(1)).copied()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed-rate scheduling ignores completions, bounds in-flight work, keeps
    /// categories separate, measures from send, and excludes drain terminals.
    ///
    /// A fake endpoint answers every request after 505 ms of paused Tokio time
    /// while the driver offers 100 requests/second for one second with at most
    /// 30 in flight. Arrivals 0–29 launch every 10 ms without waiting, arrivals
    /// 30–50 find the bound full and are missed, and arrivals 51–80 each reuse a
    /// permit freed 5 ms earlier until the bound is full again. Arrivals 81–99
    /// are missed because arrival 51 frees the next permit only at 1015 ms.
    /// Arrivals 0–29 finish inside the window; the 30 launched from 510 ms
    /// finish during drain and must not count toward the measured rate.
    ///
    /// # Panics
    ///
    /// Panics when any of those counts, categories, or latencies differ.
    #[tokio::test(start_paused = true)]
    async fn fixed_rate_driver_is_open_loop_bounded_and_window_exact() {
        let service = Duration::from_millis(505);
        let first_row = Duration::from_millis(5);
        let run = FixedRateDriver::new(100, 30)
            .run(
                Duration::from_secs(1),
                Duration::from_secs(5),
                |sequence| async move {
                    let sent = Instant::now();
                    tokio::time::sleep(service).await;
                    let outcome = match sequence % 4 {
                        0 => ShortQueryOutcome::Success,
                        1 => ShortQueryOutcome::Degraded,
                        2 => ShortQueryOutcome::AdmissionRefused,
                        _ => ShortQueryOutcome::WrongResult,
                    };
                    ProbeResult {
                        outcome,
                        first_row_at: Some(sent + first_row),
                    }
                },
            )
            .await;

        assert_eq!(run.scheduled, 100, "the schedule is fixed by the rate");
        assert_eq!(
            run.missed_launches, 40,
            "arrivals 30..=50 and 81..=99 met a full bound"
        );
        assert_eq!(run.sent, 60, "every other arrival launched on time");
        assert_eq!(run.abandoned, 0);
        assert_eq!(run.max_launch_lag, Duration::ZERO, "launches never waited");
        assert_eq!(run.samples.len(), 60);
        assert_eq!(
            run.drain_completions(),
            30,
            "arrivals 51..=80 ended in drain"
        );

        // Arrivals 0..=29 end in the window: sequence % 4 splits them 8/8/7/7.
        assert_eq!(run.measured(ShortQueryOutcome::Success), 8);
        assert_eq!(run.measured(ShortQueryOutcome::Degraded), 8);
        assert_eq!(run.measured(ShortQueryOutcome::AdmissionRefused), 7);
        assert_eq!(run.measured(ShortQueryOutcome::WrongResult), 7);
        assert_eq!(run.measured(ShortQueryOutcome::Failed), 0);
        assert!(
            (run.successes_per_second() - 8.0).abs() < f64::EPSILON,
            "only in-window Success terminals count toward throughput"
        );

        for sample in &run.samples {
            assert_eq!(sample.sent_us, sample.scheduled_us, "sent on schedule");
            assert_eq!(
                sample.terminal_latency_us, 505_000,
                "latency starts at send"
            );
            assert_eq!(sample.first_row_latency_us, Some(5_000));
        }
        assert_eq!(
            run.success_percentiles(|sample| Some(sample.terminal_latency_us)),
            [Some(505_000); 3]
        );
    }

    /// A closed loop keeps exactly its client count busy, and a count-limited
    /// run reports its real duration with every terminal measured.
    ///
    /// Four clients answering in 100 ms of paused time finish 40 queries in a
    /// one-second window; one client limited to three queries finishes them
    /// in 300 ms.
    ///
    /// # Panics
    ///
    /// Panics when the counts, measured terminals, or window differ.
    #[tokio::test(start_paused = true)]
    async fn closed_loop_driver_bounds_clients_and_counts() {
        let answer = |_| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            ProbeResult {
                outcome: ShortQueryOutcome::Success,
                first_row_at: None,
            }
        };
        let windowed = ClosedLoopDriver::new(4)
            .run(Duration::from_secs(1), u64::MAX, answer)
            .await;
        assert_eq!(windowed.sent, 40);
        assert_eq!(windowed.measured(ShortQueryOutcome::Success), 40);
        assert_eq!(windowed.window, Duration::from_secs(1));

        let limited = ClosedLoopDriver::new(1)
            .run(Duration::from_secs(3_600), 3, answer)
            .await;
        assert_eq!(limited.measured(ShortQueryOutcome::Success), 3);
        assert_eq!(limited.window, Duration::from_millis(300));
    }
}
