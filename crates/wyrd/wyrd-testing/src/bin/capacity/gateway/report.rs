//! Judges each gateway step and renders the report.
//!
//! An overhead row is PASS or FAIL: every applicable handoff point's server
//! overhead p99 bucket bound strictly under 1 ms from at least
//! [`MIN_SAMPLES`] observations (`TARGET` at or under 500 us), no refusal or
//! invalid answer, one staged capture per arrival with no drop and no loss,
//! and peak server memory below the limit less [`MEMORY_HEADROOM_BYTES`].
//! Each step's drain and the run's shutdown get their own rows, each
//! conditioned on loss-free completion. Every judgment collects all of its
//! misses and is a pure function of the measurements, so the tests below pin
//! every rule.

use std::fmt::Write as _;
use std::path::Path;

use wyrd_testing::capacity::{Histogram, Percentiles};

use crate::gateway::Result;
use crate::gateway::run::{BUFFERED_ANSWER, Integrity, Point, STREAM_ANSWER, STREAM_END, Step};

/// The server's 1-GiB default headroom: every step's peak server memory must
/// stay below the server's memory limit less this.
pub const MEMORY_HEADROOM_BYTES: u64 = 1 << 30;

/// Overhead p99 every point must stay strictly under; a p99 in the
/// `le="0.001"` bucket cannot prove it.
const P99_LIMIT_SECONDS: f64 = 0.001;

/// Overhead p99 bound at or under which a passing point meets the target.
const P99_TARGET_SECONDS: f64 = 0.0005;

/// Overhead observations each point needs before its p99 judges.
const MIN_SAMPLES: f64 = 1_000.0;

/// Server log lines that report an outbox item lost.
pub const LOSS_MARKERS: [&str; 3] = ["was lost", "items unwritten", "write task failed"];

/// The overhead report table's header.
pub const OVERHEAD_HEADER: &str = "| step | clients | throughput | client p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | server evidence |\n|---|---|---|---|---|---|---|---|---|---|";

/// A row's judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Met every target.
    Pass,
    /// Missed a target or saw an error.
    Fail,
    /// A capacity rate that met every SLO with valid evidence.
    Sustainable,
    /// A capacity rate with valid evidence that missed an SLO: exploration,
    /// not a failure.
    OutsideSlo,
    /// A capacity window whose evidence cannot judge it.
    Invalid,
}

impl Verdict {
    /// Whether the row fails the command.
    pub const fn fails(self) -> bool {
        matches!(self, Self::Fail | Self::Invalid)
    }

    /// The rendered verdict.
    const fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Sustainable => "SUSTAINABLE",
            Self::OutsideSlo => "OUTSIDE-SLO",
            Self::Invalid => "INVALID",
        }
    }
}

/// One report line.
pub struct Row {
    /// Step name.
    step: String,
    /// The rendered columns before the verdict.
    cells: String,
    /// Judgement.
    verdict: Verdict,
    /// Why, and the server's evidence.
    reason: String,
}

impl Row {
    /// The row of `step`: FAIL with every miss ahead of `evidence` when
    /// `misses` is not empty.
    pub fn judged(step: String, cells: String, misses: &[String], evidence: String) -> Self {
        let verdict = if misses.is_empty() {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
        Self::new(step, cells, verdict, misses, evidence)
    }

    /// The row of `step` judged `verdict`, with `misses` ahead of
    /// `evidence`.
    pub fn new(
        step: String,
        cells: String,
        verdict: Verdict,
        misses: &[String],
        evidence: String,
    ) -> Self {
        let reason = if misses.is_empty() {
            evidence
        } else {
            format!("{}; {evidence}", misses.join("; "))
        };
        Self {
            step,
            cells,
            verdict,
            reason,
        }
    }

    /// Renders the row as one Markdown table line.
    fn render(&self) -> String {
        format!(
            "| {} | {} | {} | {} |",
            self.step,
            self.cells,
            self.verdict.label(),
            self.reason
        )
    }
}

/// What the run proved at shutdown.
#[derive(Debug)]
pub struct Shutdown {
    /// Seconds from `SIGTERM` to a clean exit, or why it was not clean.
    pub stopped: Result<f64>,
    /// Whether the pre-stop scrape showed no active call, nothing pending,
    /// and no loss, or why it could not.
    pub settled: Result<bool>,
    /// The retained server log, or why it could not be read.
    pub log: Result<String>,
}

/// Every judged row, in the order the benchmark ran them.
pub struct Report {
    /// The report's heading.
    title: String,
    /// The table's header lines.
    header: &'static str,
    /// The server's memory limit, which sets the peak-memory ceiling.
    memory_limit_bytes: u64,
    /// Rows so far.
    rows: Vec<Row>,
    /// One JSON line per measured step: its name, clients, and raw samples.
    samples: String,
    /// Summary lines after the table.
    notes: Vec<String>,
}

impl Report {
    /// Starts an empty report titled `title` (mode, host, build, and
    /// settings) with table `header` on a server whose memory limit is
    /// `memory_limit_bytes`, and prints the title and header; each row
    /// prints as it is judged.
    pub fn new(title: String, header: &'static str, memory_limit_bytes: u64) -> Self {
        let report = Self {
            title,
            header,
            memory_limit_bytes,
            rows: Vec::new(),
            samples: String::new(),
            notes: Vec::new(),
        };
        println!("# {}\n\n{header}", report.title);
        report
    }

    /// The server's memory limit.
    pub const fn memory_limit_bytes(&self) -> u64 {
        self.memory_limit_bytes
    }

    /// Appends `value` as one line of `samples.jsonl`.
    pub fn sample(&mut self, value: &serde_json::Value) {
        let _ = writeln!(self.samples, "{value}");
    }

    /// Prints and keeps `note`, rendered after the table.
    pub fn note(&mut self, note: String) {
        println!("{note}");
        self.notes.push(note);
    }

    /// `-` for every column between a row's step and its verdict.
    pub fn blank_cells(&self) -> String {
        let columns = self
            .header
            .lines()
            .next()
            .unwrap_or_default()
            .matches('|')
            .count();
        vec!["-"; columns.saturating_sub(4)].join(" | ")
    }

    /// Adds an overhead step, judged by [`overhead_misses`], and its drain,
    /// judged by [`drain_misses`].
    pub fn overhead(&mut self, step: &Step) {
        self.sample(&serde_json::json!({
                "step": step.name,
                "clients": step.clients,
                "profiled": step.profile_gaps.is_some(),
                "arrivals": step.samples.arrivals,
                "completion_us": step.samples.completion_us,
                "headers_us": step.samples.headers_us,
                "first_event_us": step.samples.first_event_us,
                "errors": step.samples.errors,
                "traffic_seconds": step.traffic.seconds,
                "drain_seconds": step.drain,
        }));
        let misses = overhead_misses(step, self.memory_limit_bytes);
        let evidence = overhead_evidence(step);
        self.push(Row::judged(
            step.name.clone(),
            cells(step),
            &misses,
            evidence,
        ));
        let drained = &step.drained;
        let drain_cells = format!(
            "{} | - | - | - | {:.2} cores | {:.2} GiB | -",
            step.clients,
            drained.server_cpu_seconds / drained.seconds.max(f64::EPSILON),
            gib(drained.peak_memory_bytes),
        );
        let evidence = format!(
            "drain {}; {}",
            step.drain
                .map_or("timed out".to_owned(), |seconds| format!("{seconds:.1} s")),
            integrity(&step.integrity, step.samples.arrivals),
        );
        self.push(Row::judged(
            format!("{} drain", step.name),
            drain_cells,
            &drain_misses(step),
            evidence,
        ));
    }

    /// Adds the shutdown row, judged by [`shutdown_misses`].
    pub fn shutdown(&mut self, shutdown: &Shutdown) {
        let evidence = shutdown
            .stopped
            .as_ref()
            .map_or_else(ToString::to_string, |seconds| {
                format!("exited cleanly in {seconds:.1} s")
            });
        self.push(Row::judged(
            "shutdown".to_owned(),
            self.blank_cells(),
            &shutdown_misses(shutdown),
            evidence,
        ));
    }

    /// Adds a row that failed for `reason` outside any measured window, such
    /// as a harness or evidence error.
    pub fn failed(&mut self, step: &str, reason: String) {
        self.push(Row::judged(
            step.to_owned(),
            self.blank_cells(),
            &[reason],
            "no evidence".to_owned(),
        ));
    }

    /// Whether every judged row passed.
    pub fn passed(&self) -> bool {
        self.rows.iter().all(|row| !row.verdict.fails())
    }

    /// Writes every row, as printed, into `output/report.md` and the raw
    /// samples into `output/samples.jsonl`.
    ///
    /// # Errors
    ///
    /// Returns the file write failure.
    pub fn write_to(&self, output: &Path) -> Result<()> {
        let mut text = format!("# {}\n\n{}\n", self.title, self.header);
        for row in &self.rows {
            let _ = writeln!(text, "{}", row.render());
        }
        for note in &self.notes {
            let _ = write!(text, "\n{note}\n");
        }
        std::fs::write(output.join("report.md"), text)?;
        std::fs::write(output.join("samples.jsonl"), &self.samples)?;
        Ok(())
    }

    /// Prints and appends `row`.
    pub fn push(&mut self, row: Row) {
        println!("{}", row.render());
        self.rows.push(row);
    }
}

/// The handoff points a step judges.
pub const fn points(stream: bool) -> &'static [Point] {
    if stream {
        &[STREAM_ANSWER, STREAM_END]
    } else {
        &[BUFFERED_ANSWER]
    }
}

/// Every reason an overhead step fails: per point, fewer than
/// [`MIN_SAMPLES`] observations or a p99 bound that is missing, non-finite,
/// or not strictly under [`P99_LIMIT_SECONDS`]; then every correctness,
/// capture, and memory miss of [`correctness_misses`].
pub fn overhead_misses(step: &Step, memory_limit: u64) -> Vec<String> {
    let mut misses = Vec::new();
    for point in points(step.stream) {
        let overhead = step.telemetry.histogram(point.overhead);
        if overhead.count.is_nan() || overhead.count < MIN_SAMPLES {
            misses.push(format!(
                "{} overhead samples {} < {MIN_SAMPLES}",
                point.name, overhead.count
            ));
        }
        match overhead.percentiles().p99 {
            Some(p99) if p99.is_finite() && p99 < P99_LIMIT_SECONDS => {}
            p99 => misses.push(format!(
                "{} overhead p99 <= {} ms is not < 1 ms",
                point.name,
                bound_ms(p99)
            )),
        }
    }
    misses.extend(correctness_misses(step, memory_limit));
    misses
}

/// Every reason any window fails whatever its latency: a refusal or invalid
/// answer, no arrivals, a capture count other than one staged per arrival,
/// a dropped capture, a lost outbox item, a counter reset, or peak server
/// memory within [`MEMORY_HEADROOM_BYTES`] of `memory_limit`.
pub fn correctness_misses(step: &Step, memory_limit: u64) -> Vec<String> {
    let mut misses = Vec::new();
    let errors = step.samples.error_count();
    if errors > 0 {
        misses.push(format!("{errors} refused or invalid calls"));
    }
    if step.samples.arrivals == 0 {
        misses.push("no arrivals".to_owned());
    }
    misses.extend(integrity_misses(&step.integrity, step.samples.arrivals));
    if step.traffic.peak_memory_bytes + MEMORY_HEADROOM_BYTES >= memory_limit {
        misses.push(format!(
            "peak memory >= {} GiB",
            memory_limit.saturating_sub(MEMORY_HEADROOM_BYTES) >> 30
        ));
    }
    misses
}

/// Every reason `integrity` cannot prove `arrivals` were each captured
/// without loss: a reset counter, a staged count other than `arrivals`, a
/// drop, a loss, or staged captures the Scribe writer never wrote.
pub fn integrity_misses(integrity: &Integrity, arrivals: u64) -> Vec<String> {
    if integrity.reset {
        return vec!["a counter reset or was invalid; no delta is evidence".to_owned()];
    }
    let mut misses = Vec::new();
    if (integrity.staged - arrivals as f64).abs() > 0.5 {
        misses.push(format!(
            "{} captures staged for {arrivals} arrivals",
            integrity.staged
        ));
    }
    if integrity.dropped != 0.0 {
        misses.push(format!("{} captures dropped", integrity.dropped));
    }
    if integrity.lost != 0.0 {
        misses.push(format!("{} outbox items lost", integrity.lost));
    }
    if integrity.staged > 0.0 && integrity.written <= 0.0 {
        misses.push("no Scribe outbox write completed".to_owned());
    }
    misses
}

/// Every reason a step's drain fails: it did not settle within the limit,
/// or its captures did not complete loss-free.
pub fn drain_misses(step: &Step) -> Vec<String> {
    let mut misses = Vec::new();
    if step.drain.is_none() {
        misses.push("did not drain within 60 s".to_owned());
    }
    misses.extend(integrity_misses(&step.integrity, step.samples.arrivals));
    misses
}

/// Every reason shutdown fails: an unclean or late exit, a pre-stop scrape
/// that was not settled or loss-free, or a log that is unreadable or reports
/// a lost item.
pub fn shutdown_misses(shutdown: &Shutdown) -> Vec<String> {
    let mut misses = Vec::new();
    if let Err(error) = &shutdown.stopped {
        misses.push(format!("unclean exit: {error}"));
    }
    match &shutdown.settled {
        Ok(true) => {}
        Ok(false) => misses.push("pending or lost items before stop".to_owned()),
        Err(error) => misses.push(format!("pre-stop scrape failed: {error}")),
    }
    match &shutdown.log {
        Ok(log) => {
            let lost = log
                .lines()
                .filter(|line| LOSS_MARKERS.iter().any(|marker| line.contains(marker)))
                .count();
            if lost > 0 {
                misses.push(format!("{lost} server log lines report lost items"));
            }
        }
        Err(error) => misses.push(format!("server log unreadable: {error}")),
    }
    misses
}

/// The columns of a measured step before its verdict.
fn cells(step: &Step) -> String {
    let usage = &step.traffic;
    let seconds = usage.seconds.max(f64::EPSILON);
    let errors = if step.samples.errors.is_empty() {
        "0".to_owned()
    } else {
        step.samples
            .errors
            .iter()
            .map(|(code, count)| format!("{count}× {code}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{} | {:.0}/s | {} | {errors} | {:.2} cores | {:.2} GiB | {:.2} cores",
        step.clients,
        step.samples.arrivals as f64 / seconds,
        client(&step.samples.completion_us),
        usage.server_cpu_seconds / seconds,
        gib(usage.peak_memory_bytes),
        usage.driver_cpu_seconds / seconds,
    )
}

/// The server evidence of an overhead step: each point's overhead, server
/// total, and upstream wait; a stream's client header and first-event
/// latencies; the client-added means; and capture integrity.
fn overhead_evidence(step: &Step) -> String {
    let mut evidence = Vec::new();
    for point in points(step.stream) {
        let overhead = step.telemetry.histogram(point.overhead);
        let target = overhead
            .percentiles()
            .p99
            .is_some_and(|p99| p99 <= P99_TARGET_SECONDS);
        evidence.push(format!(
            "{} overhead {}{}; server total {}; upstream {}",
            point.name,
            summary(&overhead),
            if target { " TARGET" } else { "" },
            summary(&step.telemetry.histogram(point.duration)),
            summary(&step.telemetry.histogram(point.upstream)),
        ));
    }
    let samples = &step.samples;
    if step.stream {
        evidence.push(format!(
            "client headers {} ms; first event {} ms",
            client(&samples.headers_us),
            client(&samples.first_event_us)
        ));
        evidence.push(format!(
            "client extra mean: headers {} ms, end {} ms",
            client_extra(
                &samples.headers_us,
                &step.telemetry.histogram(STREAM_ANSWER.duration)
            ),
            client_extra(
                &samples.completion_us,
                &step.telemetry.histogram(STREAM_END.duration)
            ),
        ));
    } else {
        evidence.push(format!(
            "client extra mean {} ms",
            client_extra(
                &samples.completion_us,
                &step.telemetry.histogram(BUFFERED_ANSWER.duration)
            )
        ));
    }
    evidence.push(integrity(&step.integrity, samples.arrivals));
    if let Some(gaps) = &step.profile_gaps {
        evidence.push(if gaps.is_empty() {
            "profile complete".to_owned()
        } else {
            format!("profile INCOMPLETE: {}", gaps.join(", "))
        });
    }
    evidence.join("; ")
}

/// Capture integrity as `staged s/a, dropped d, lost l`.
fn integrity(integrity: &Integrity, arrivals: u64) -> String {
    format!(
        "captures staged {}/{arrivals}, dropped {}, lost {}, scribe writes {}",
        integrity.staged, integrity.dropped, integrity.lost, integrity.written
    )
}

/// Raw client samples, microseconds, as `p50/p95/p99` milliseconds.
pub fn client(samples_us: &[u64]) -> String {
    match Percentiles::raw(samples_us, 1e-3) {
        Percentiles {
            p50: Some(p50),
            p95: Some(p95),
            p99: Some(p99),
        } => format!("{p50:.2}/{p95:.2}/{p99:.2}"),
        _ => "-".to_owned(),
    }
}

/// The client's mean less the server's mean `server` duration, in
/// milliseconds, only when both measured the same number of calls:
/// what the client, transport, and HTTP stack add.
fn client_extra(samples_us: &[u64], server: &Histogram) -> String {
    match server.mean() {
        Some(server_mean) if (samples_us.len() as f64 - server.count).abs() < 0.5 => {
            let client = samples_us.iter().sum::<u64>() as f64 / samples_us.len() as f64 * 1e-6;
            format!("{:.4}", (client - server_mean) * 1e3)
        }
        _ => "unavailable".to_owned(),
    }
}

/// One histogram as `p50/p95/p99 <= a/b/c ms, mean m ms, xN`.
pub fn summary(histogram: &Histogram) -> String {
    let Percentiles { p50, p95, p99 } = histogram.percentiles();
    format!(
        "p50/p95/p99 <= {}/{}/{} ms, mean {} ms, x{:.0}",
        bound_ms(p50),
        bound_ms(p95),
        bound_ms(p99),
        histogram
            .mean()
            .map_or("-".to_owned(), |mean| format!("{:.4}", mean * 1e3)),
        histogram.count,
    )
}

/// A bucket bound in milliseconds, or `-`.
fn bound_ms(bound: Option<f64>) -> String {
    bound.map_or("-".to_owned(), |bound| format!("{}", bound * 1e3))
}

/// Bytes in GiB.
pub fn gib(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1 << 30)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    use wyrd_testing::capacity::{Histogram, Telemetry, Usage};

    use super::{Shutdown, Verdict, drain_misses, overhead_misses, shutdown_misses};
    use crate::gateway::capacity::{Arrived, Backlog, Driven, Load, Measured, Ownership, Slo};
    use crate::gateway::envelope::{
        capture_misses, classify, envelope, overload_misses, recovery_misses,
    };
    use crate::gateway::run::{Integrity, SELECTION, Samples, Step};
    use crate::gateway::workload::{Outcome, Timed};

    /// A memory limit far above every test's peak.
    const LIMIT: u64 = 64 << 30;

    /// Bucket bounds every test histogram uses.
    const BOUNDS: [f64; 6] = [0.0005, 0.0009, 0.001, 0.009, 0.01, f64::INFINITY];

    /// A clean step of `arrivals` calls, streamed when `stream`, whose every
    /// selected histogram holds `count` observations, `slow` of them in the
    /// bucket `slow_bound`.
    fn step(stream: bool, arrivals: u64, count: f64, slow_bound: f64, slow: f64) -> Step {
        let histogram = Histogram {
            buckets: BOUNDS
                .iter()
                .map(|bound| {
                    let fast = count - slow;
                    (*bound, if *bound >= slow_bound { count } else { fast })
                })
                .collect(),
            sum: count * 0.0003,
            count,
        };
        let telemetry = Telemetry {
            selection: SELECTION,
            histograms: vec![histogram; SELECTION.histograms.len()],
            breakdown: BTreeMap::new(),
        };
        Step {
            name: "1 chat 16".to_owned(),
            clients: 16,
            stream,
            samples: Samples {
                arrivals,
                completion_us: vec![400; usize::try_from(arrivals).unwrap_or(0)],
                ..Samples::default()
            },
            traffic: Usage {
                seconds: 10.0,
                peak_memory_bytes: 1 << 30,
                ..Usage::default()
            },
            drain: Some(0.2),
            telemetry,
            integrity: Integrity {
                staged: arrivals as f64,
                written: 1.0,
                ..Integrity::default()
            },
            ..Step::default()
        }
    }

    /// The release-run SLO bounds, seconds.
    const SLO: Slo = Slo {
        answer: Some(0.2),
        first_token: Some(0.35),
        chunk_lag: Some(0.05),
        stream: Some(1.1),
    };

    /// One answered call of `tenant` completing `ms` after its scheduled
    /// arrival: a buffered body, or 32 events from 250 ms, 20 ms apart, each
    /// 5 ms behind its upstream send.
    fn arrived(tenant: usize, stream: bool, ms: u64) -> Arrived {
        let scheduled = Instant::now();
        let at = |ms: u64| scheduled + Duration::from_millis(ms);
        let (events, sends) = if stream {
            (
                (0..32).map(|k| at(250 + 20 * k)).collect(),
                (0..32).map(|k| at(245 + 20 * k)).collect(),
            )
        } else {
            (Vec::new(), vec![at(ms.saturating_sub(5))])
        };
        Arrived {
            tenant,
            stream,
            offset: Duration::ZERO,
            scheduled,
            timed: Timed {
                sent: scheduled,
                headers: Some(at(if stream { 250 } else { ms })),
                events,
                done: at(ms),
                outcome: Outcome::Answered,
            },
            sends,
        }
    }

    /// A clean 60 s capacity window at `rate` of `calls`, spread over the
    /// window, with flat backlog and every capture staged and owned.
    fn window(rate: f64, mut calls: Vec<Arrived>) -> Measured {
        let count = calls.len();
        for (index, call) in calls.iter_mut().enumerate() {
            call.offset = Duration::from_secs_f64(index as f64 * 60.0 / count as f64);
        }
        let mut ownership = vec![Ownership::default(); 4];
        for call in &calls {
            let owned = &mut ownership[call.tenant];
            owned.admissible += 1;
            if call.timed.outcome.answered() {
                owned.answered += 1;
                owned.rows += 1;
            }
        }
        let tenants = calls.iter().map(|call| call.tenant + 1).max().unwrap_or(1);
        ownership.truncate(tenants);
        let admitted = ownership.iter().map(|owned| owned.rows).sum::<i64>() as f64;
        Measured {
            name: format!("1 buffered {rate}/s"),
            rate,
            seconds: 60.0,
            driven: Driven {
                scheduled: count as u64,
                sent: count as u64,
                lateness_us: vec![100; count],
                ..Driven::default()
            },
            backlog: (0..60)
                .map(|second| Backlog {
                    offset: f64::from(second),
                    in_flight: 10.0,
                    active: 10.0,
                    pending: 0.0,
                })
                .collect(),
            calls,
            traffic: Usage {
                seconds: 60.0,
                peak_memory_bytes: 1 << 30,
                ..Usage::default()
            },
            drain: Some(0.5),
            integrity: Integrity {
                staged: admitted,
                written: 1.0,
                ..Integrity::default()
            },
            admitted,
            ownership,
            ..Measured::default()
        }
    }

    /// `count` answered calls of `tenant` in `stream` mode taking `ms`.
    fn calls(count: usize, tenant: usize, stream: bool, ms: u64) -> Vec<Arrived> {
        (0..count).map(|_| arrived(tenant, stream, ms)).collect()
    }

    /// Every overhead row requires each point's sample count and strict p99
    /// bound, correctness, one staged capture per arrival, no drop or loss,
    /// memory headroom, a bounded drain, and a loss-free shutdown; and it
    /// reports every miss at once.
    ///
    /// # Panics
    ///
    /// Panics when a rule judges the wrong way.
    #[test]
    fn every_row_requires_latency_correctness_and_zero_loss() {
        assert!(overhead_misses(&step(false, 1_000, 1_000.0, 0.0009, 10.0), LIMIT).is_empty());
        assert!(overhead_misses(&step(true, 1_000, 1_000.0, 0.0005, 0.0), LIMIT).is_empty());
        // Exactly 1 ms, above 1 ms, and an unbounded p99 fail.
        for slow_bound in [0.001, 0.009, f64::INFINITY] {
            let misses = overhead_misses(&step(false, 1_000, 1_000.0, slow_bound, 20.0), LIMIT);
            assert_eq!(misses.len(), 1, "{slow_bound}: {misses:?}");
        }
        // 999 samples fail; a stream fails on each of its two points.
        assert_eq!(
            overhead_misses(&step(false, 999, 999.0, 0.0005, 0.0), LIMIT).len(),
            1
        );
        assert_eq!(
            overhead_misses(&step(true, 999, 999.0, 0.0005, 0.0), LIMIT).len(),
            2
        );
        // Missing telemetry fails both its count and its p99.
        let mut missing = step(false, 1_000, 1_000.0, 0.0005, 0.0);
        missing.telemetry.histograms.clear();
        assert_eq!(overhead_misses(&missing, LIMIT).len(), 2);
        // Errors, capture mismatches, drops, losses, resets, and memory each
        // fail, all reported together.
        let mut broken = step(false, 1_000, 1_000.0, 0.0005, 0.0);
        broken.samples.errors.insert("rate_limited".to_owned(), 1);
        broken.integrity.staged = 999.0;
        broken.integrity.dropped = 1.0;
        broken.integrity.lost = 2.0;
        broken.traffic.peak_memory_bytes = LIMIT - (1 << 30);
        let misses = overhead_misses(&broken, LIMIT);
        assert_eq!(misses.len(), 5, "{misses:?}");
        let mut reset = step(false, 1_000, 1_000.0, 0.0005, 0.0);
        reset.integrity.reset = true;
        assert_eq!(overhead_misses(&reset, LIMIT).len(), 1);
        let mut unwritten = step(false, 1_000, 1_000.0, 0.0005, 0.0);
        unwritten.integrity.written = 0.0;
        assert_eq!(drain_misses(&unwritten).len(), 1);
        // An over-limit measurement stays FAIL; nothing relaxes it.
        assert!(!overhead_misses(&step(false, 5_000, 5_000.0, 0.009, 4_000.0), LIMIT).is_empty());
        // A drain that times out, or loses an item, fails.
        let mut slow = step(false, 1_000, 1_000.0, 0.0005, 0.0);
        slow.drain = None;
        assert_eq!(drain_misses(&slow).len(), 1);
        assert!(drain_misses(&step(false, 1_000, 1_000.0, 0.0005, 0.0)).is_empty());
        // Shutdown fails on an unclean exit, an unsettled stop, or a loss in
        // the log.
        let clean = || Shutdown {
            stopped: Ok(1.0),
            settled: Ok(true),
            log: Ok("INFO shutdown complete\n".to_owned()),
        };
        assert!(shutdown_misses(&clean()).is_empty());
        let lost = Shutdown {
            stopped: Err("did not exit within 45 s".into()),
            settled: Ok(false),
            log: Ok("ERROR outbox shutdown deadline passed with items unwritten\n".to_owned()),
        };
        assert_eq!(shutdown_misses(&lost).len(), 3);
        let unreadable = Shutdown {
            log: Err("missing".into()),
            ..clean()
        };
        assert_eq!(shutdown_misses(&unreadable).len(), 1);

        // Capacity: a declared bound passes at equality and fails above it.
        let buffered = Load::Buffered;
        let verdict = |measured: &Measured| classify(buffered, measured, &SLO).0;
        assert_eq!(
            verdict(&window(50.0, calls(200, 0, false, 150))),
            Verdict::Sustainable
        );
        assert_eq!(
            verdict(&window(50.0, calls(200, 0, false, 200))),
            Verdict::Sustainable
        );
        assert_eq!(
            verdict(&window(50.0, calls(200, 0, false, 201))),
            Verdict::OutsideSlo
        );
        // Fewer than 100 completions per tenant/mode is not evidence.
        assert_eq!(
            verdict(&window(50.0, calls(99, 0, false, 150))),
            Verdict::Invalid
        );
        // Streams judge first token, chunk lag, and the whole stream.
        let paced = |ms| {
            classify(
                Load::PacedStream,
                &window(50.0, calls(150, 0, true, ms)),
                &SLO,
            )
        };
        assert_eq!(paced(1_100).0, Verdict::Sustainable);
        assert_eq!(paced(1_101).1.len(), 1);
        // A quiet tenant's miss stays visible though the aggregate passes.
        let mut mixed = Vec::new();
        for tenant in 0..4 {
            mixed.extend(calls(150, tenant, true, 900));
            mixed.extend(calls(
                if tenant == 3 { 95 } else { 400 },
                tenant,
                false,
                100,
            ));
        }
        mixed.extend(calls(5, 3, false, 250));
        let (quiet, misses) = classify(Load::TenantContention, &window(100.0, mixed), &SLO);
        assert_eq!(quiet, Verdict::OutsideSlo);
        assert_eq!(misses.len(), 1, "{misses:?}");
        assert!(
            misses[0].starts_with("bench-d/buffered answer"),
            "{misses:?}"
        );
        // A missed or capped arrival, or late sends, invalidate the rate.
        let mut missed = window(50.0, calls(200, 0, false, 150));
        missed.driven.missed = 1;
        assert_eq!(verdict(&missed), Verdict::Invalid);
        let mut capped = window(50.0, calls(200, 0, false, 150));
        capped.driven.capped = 1;
        assert_eq!(verdict(&capped), Verdict::Invalid);
        let mut late = window(50.0, calls(200, 0, false, 150));
        late.driven.lateness_us = vec![6_000; 200];
        assert_eq!(verdict(&late), Verdict::Invalid);
        let mut blind = window(50.0, calls(200, 0, false, 150));
        blind.backlog.clear();
        assert_eq!(verdict(&blind), Verdict::Invalid);
        // Backlog may grow by one second of offered work, not more.
        let grown = |growth: f64| {
            let mut measured = window(50.0, calls(200, 0, false, 150));
            for reading in measured
                .backlog
                .iter_mut()
                .filter(|reading| reading.offset >= 50.0)
            {
                reading.pending += growth;
            }
            verdict(&measured)
        };
        assert_eq!(grown(50.0), Verdict::Sustainable);
        assert_eq!(grown(51.0), Verdict::OutsideSlo);
        // A refusal is outside the SLO but needs no capture: captures
        // reconcile against admitted calls, not offered ones.
        let mut offered = calls(200, 0, false, 150);
        offered[0].timed.outcome = Outcome::Refused("WYRD_GATEWAY_429_LIMIT_EXCEEDED".to_owned());
        let refused = window(50.0, offered);
        assert_eq!(verdict(&refused), Verdict::OutsideSlo);
        assert!(capture_misses(&refused, LIMIT).is_empty());
        // Lost, dropped, unstaged, and misattributed captures each fail.
        let mut lost = window(50.0, calls(200, 0, false, 150));
        lost.integrity.lost = 1.0;
        lost.integrity.dropped = 1.0;
        assert_eq!(capture_misses(&lost, LIMIT).len(), 2);
        let mut unstaged = window(50.0, calls(200, 0, false, 150));
        unstaged.integrity.staged -= 1.0;
        assert_eq!(capture_misses(&unstaged, LIMIT).len(), 2);
        let mut moved = window(
            100.0,
            [calls(200, 0, false, 150), calls(200, 1, false, 150)].concat(),
        );
        moved.ownership[0].rows -= 1;
        moved.ownership[1].rows += 1;
        assert_eq!(capture_misses(&moved, LIMIT).len(), 2);
        // The envelope reports the highest passing tested rate and the first
        // failing one, never an untested maximum.
        let (text, found) = envelope(
            buffered,
            &[(50.0, Verdict::Sustainable), (100.0, Verdict::OutsideSlo)],
        );
        assert!(
            found
                && text.contains("highest sustainable tested rate 50/s; first failing rate 100/s"),
            "{text}"
        );
        let (text, found) = envelope(
            buffered,
            &[(50.0, Verdict::Sustainable), (100.0, Verdict::Sustainable)],
        );
        assert!(
            found
                && text
                    .contains("100/s, the highest tested rate: a lower bound, ceiling unlocated"),
            "{text}"
        );
        let (text, found) = envelope(buffered, &[(50.0, Verdict::Invalid)]);
        assert!(
            !found && text.contains("no measured sustainable rate"),
            "{text}"
        );
        // Overload allows only configured admission refusals.
        let mut refusing = calls(200, 0, false, 150);
        for call in refusing.iter_mut().take(20) {
            call.timed.outcome = Outcome::Refused("WYRD_GATEWAY_429_LIMIT_EXCEEDED".to_owned());
        }
        let (misses, saturated) = overload_misses(&window(100.0, refusing.clone()), LIMIT);
        assert!(misses.is_empty() && saturated, "{misses:?}");
        let (misses, saturated) = overload_misses(&window(100.0, calls(200, 0, false, 150)), LIMIT);
        assert!(
            misses.is_empty() && !saturated,
            "unobserved saturation is not a failure"
        );
        for bad in [
            Outcome::TimedOut,
            Outcome::Refused("WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE".to_owned()),
            Outcome::Failed("stream ended without [DONE]".to_owned()),
        ] {
            let mut failing = refusing.clone();
            failing[100].timed.outcome = bad;
            assert_eq!(overload_misses(&window(100.0, failing), LIMIT).0.len(), 1);
        }
        let mut lossy = window(100.0, refusing);
        lossy.integrity.lost = 1.0;
        assert!(!overload_misses(&lossy, LIMIT).0.is_empty());
        // Recovery needs every SLO, enough calls, no refusal, and no loss.
        let recovered = |measured: &Measured| recovery_misses(buffered, measured, &SLO, LIMIT);
        assert!(recovered(&window(50.0, calls(200, 0, false, 150))).is_empty());
        assert!(!recovered(&window(50.0, calls(99, 0, false, 150))).is_empty());
        assert!(!recovered(&refused).is_empty());
        let mut lossy = window(50.0, calls(200, 0, false, 150));
        lossy.integrity.lost = 1.0;
        assert!(!recovered(&lossy).is_empty());
    }
}
