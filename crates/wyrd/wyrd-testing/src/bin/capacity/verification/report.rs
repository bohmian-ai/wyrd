//! Judges each measured step against AC-009 and renders the report.
//!
//! A direct `verify` row is PASS or FAIL: the server's verification
//! overhead p99 at or under 500 µs from at least [`MIN_SAMPLES`]
//! observations, with no refusal, no wrong verdict, and peak server memory
//! below the limit less [`MEMORY_HEADROOM_BYTES`]. A `submit` row only
//! reports its latencies (`-`), and fails only on a refusal, a backlog that
//! did not drain, or memory near the limit. The judgments are pure
//! functions of the measurements, so the tests below pin every rule.

use std::fmt::Write as _;
use std::path::Path;

use wyrd_testing::capacity::Percentiles;

use crate::verification::Result;
use crate::verification::run::{
    DIRECT, Histogram, Measured, OVERHEAD, QUEUE_WAIT, QUEUED, RUN, STAGED, SUBMIT, SUBMITTED,
    TRIGGER_TO_TERMINAL, VERIFIER,
};

/// The server's 1-GiB default headroom: every step's peak server memory must
/// stay below the server's memory limit less this.
const MEMORY_HEADROOM_BYTES: u64 = 1 << 30;

/// Direct overhead p99 every direct window must stay at or under; a p99 in
/// the `le="0.0005"` bucket proves it and any higher bucket cannot.
const P99_LIMIT_SECONDS: f64 = 0.0005;

/// Direct overhead observations a window needs before its p99 judges.
const MIN_SAMPLES: f64 = 1_000.0;

/// The report table's header.
const HEADER: &str = "| step | clients | throughput | client p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | server evidence |\n|---|---|---|---|---|---|---|---|---|---|";

/// A row's judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Met every target.
    Pass,
    /// Missed a target or saw an error.
    Fail,
    /// Reported only.
    Measured,
}

/// One report line.
struct Row {
    /// Step name.
    step: String,
    /// The rest of the rendered columns before the verdict, or `-` cells
    /// for a row with no window.
    cells: String,
    /// Judgement.
    verdict: Verdict,
    /// Why, and the server's evidence.
    reason: String,
}

impl Row {
    /// Renders the row as one Markdown table line.
    fn render(&self) -> String {
        let verdict = match self.verdict {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Measured => "-",
        };
        format!(
            "| {} | {} | {verdict} | {} |",
            self.step, self.cells, self.reason
        )
    }
}

/// Every judged row, in the order the benchmark ran them.
pub struct Report {
    /// Where the server ran and the run's settings.
    label: String,
    /// The server's memory limit, which sets the peak-memory ceiling.
    memory_limit_bytes: u64,
    /// Rows so far.
    rows: Vec<Row>,
    /// One JSON line per measured step: its name, clients, and every latency.
    samples: String,
}

impl Report {
    /// Starts an empty report titled with `label` (host and settings) on a
    /// server whose memory limit is `memory_limit_bytes`, and prints the
    /// title and table header; each row prints as it is judged.
    pub fn new(label: String, memory_limit_bytes: u64) -> Self {
        let report = Self {
            label,
            memory_limit_bytes,
            rows: Vec::new(),
            samples: String::new(),
        };
        println!("{}\n\n{HEADER}", report.title());
        report
    }

    /// Adds a direct `verify` window, judged by [`direct_miss`], with the
    /// server's overhead, Verifier time, and the client's extra time over
    /// the server's whole execution.
    pub fn direct(&mut self, step: &str, measured: &Measured) {
        let usage = &measured.usage;
        let overhead = usage.histogram(OVERHEAD, DIRECT);
        let run = usage.histogram(RUN, DIRECT);
        let reason = format!(
            "overhead {}; verifier {}; server total {}; client extra mean {} ms",
            summary(&overhead),
            summary(&usage.histogram(VERIFIER, DIRECT)),
            summary(&run),
            client_extra(measured, &run),
        );
        let miss = direct_miss(measured, self.memory_limit_bytes);
        self.add(step, measured, Verdict::Pass, miss, reason);
    }

    /// Adds a background `submit` window, reported only; it fails only per
    /// [`queued_miss`].
    pub fn queued(&mut self, step: &str, measured: &Measured) {
        let usage = &measured.usage;
        let submit = usage.histogram(SUBMIT, STAGED);
        let reason = format!(
            "submit {}; client extra mean {} ms; queue wait {}; run overhead {}; verifier {}; trigger->terminal {}; {}",
            summary(&submit),
            client_extra(measured, &submit),
            summary(&usage.histogram(QUEUE_WAIT, SUBMITTED)),
            summary(&usage.histogram(OVERHEAD, QUEUED)),
            summary(&usage.histogram(VERIFIER, QUEUED)),
            summary(&usage.histogram(TRIGGER_TO_TERMINAL, SUBMITTED)),
            measured
                .drain
                .map_or("did not drain within 60 s".to_owned(), |seconds| {
                    format!("drained in {seconds:.1} s")
                }),
        );
        let miss = queued_miss(measured, self.memory_limit_bytes);
        self.add(step, measured, Verdict::Measured, miss, reason);
    }

    /// Adds the shutdown step: after `SIGTERM` the server must exit cleanly
    /// within its grace period; `stopped` is its exit time or the failure.
    pub fn shutdown(&mut self, stopped: &Result<f64>) {
        let (verdict, reason) = match stopped {
            Ok(seconds) => (Verdict::Pass, format!("exited cleanly in {seconds:.1}s")),
            Err(error) => (Verdict::Fail, error.to_string()),
        };
        let row = Row {
            step: "shutdown".to_owned(),
            cells: "- | - | - | - | - | - | -".to_owned(),
            verdict,
            reason,
        };
        println!("{}", row.render());
        self.rows.push(row);
    }

    /// Whether every judged row passed.
    pub fn passed(&self) -> bool {
        self.rows.iter().all(|row| row.verdict != Verdict::Fail)
    }

    /// Writes every row, as printed, into `output/report.md` and the raw
    /// latencies into `output/samples.jsonl`.
    ///
    /// # Errors
    ///
    /// Returns the file write failure.
    pub fn write_to(&self, output: &Path) -> Result<()> {
        let mut text = format!("{}\n\n{HEADER}\n", self.title());
        for row in &self.rows {
            let _ = writeln!(text, "{}", row.render());
        }
        std::fs::write(output.join("report.md"), text)?;
        std::fs::write(output.join("samples.jsonl"), &self.samples)?;
        Ok(())
    }

    /// The report's title line: where the server ran and the run's settings,
    /// so only reports from matching hosts are compared.
    fn title(&self) -> String {
        format!("# Verification capacity: {}", self.label)
    }

    /// Records the step's raw latencies, then appends and prints its row:
    /// FAIL with `miss` ahead of `reason` when there is one, else `clean`.
    fn add(
        &mut self,
        step: &str,
        measured: &Measured,
        clean: Verdict,
        miss: Option<String>,
        reason: String,
    ) {
        let _ = writeln!(
            self.samples,
            "{}",
            serde_json::json!({
                "step": step,
                "clients": measured.clients,
                "latencies_us": measured.latencies_us,
            })
        );
        let latency = measured.percentiles();
        let errors = if measured.errors.is_empty() {
            "0".to_owned()
        } else {
            measured
                .errors
                .iter()
                .map(|(code, count)| format!("{count}× {code}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let usage = &measured.usage;
        let seconds = usage.seconds.max(f64::EPSILON);
        let cells = format!(
            "{} | {:.0}/s | {} | {errors} | {:.2} cores | {:.2} GiB | {:.2} cores",
            measured.clients,
            measured.throughput(),
            match (latency.p50, latency.p95, latency.p99) {
                (Some(p50), Some(p95), Some(p99)) => format!("{p50:.2}/{p95:.2}/{p99:.2}"),
                _ => "-".to_owned(),
            },
            usage.server_cpu_seconds / seconds,
            usage.peak_memory_bytes as f64 / f64::from(1 << 30),
            usage.driver_cpu_seconds / seconds,
        );
        let row = Row {
            step: step.to_owned(),
            cells,
            verdict: if miss.is_some() { Verdict::Fail } else { clean },
            reason: miss.map_or(reason.clone(), |miss| format!("{miss}; {reason}")),
        };
        println!("{}", row.render());
        self.rows.push(row);
    }
}

/// Why a direct window fails: any refusal or wrong verdict, memory near the
/// limit, fewer than [`MIN_SAMPLES`] overhead observations, or an overhead
/// p99 bucket bound above [`P99_LIMIT_SECONDS`].
fn direct_miss(measured: &Measured, memory_limit: u64) -> Option<String> {
    let overhead = measured.usage.histogram(OVERHEAD, DIRECT);
    let p99 = overhead.percentiles().p99;
    correctness_miss(measured, memory_limit).or_else(|| {
        if overhead.count < MIN_SAMPLES {
            Some(format!(
                "{} overhead samples < {MIN_SAMPLES}",
                overhead.count
            ))
        } else {
            p99.is_none_or(|p99| p99 > P99_LIMIT_SECONDS)
                .then(|| format!("overhead p99 <= {} ms exceeds 0.5 ms", bound_ms(p99)))
        }
    })
}

/// Why a `submit` window fails: any refusal, memory near the limit, or a
/// backlog that did not drain within the limit. Its latencies never fail it.
fn queued_miss(measured: &Measured, memory_limit: u64) -> Option<String> {
    correctness_miss(measured, memory_limit).or_else(|| {
        measured
            .drain
            .is_none()
            .then(|| "backlog did not drain within 60 s".to_owned())
    })
}

/// Why any window fails whatever its latency: a refusal, a wrong verdict, or
/// peak server memory within [`MEMORY_HEADROOM_BYTES`] of `memory_limit`.
fn correctness_miss(measured: &Measured, memory_limit: u64) -> Option<String> {
    if measured.error_count() > 0 {
        Some(format!("{} refusals", measured.error_count()))
    } else if measured.wrong > 0 {
        Some(format!("{} wrong verdicts", measured.wrong))
    } else {
        (measured.usage.peak_memory_bytes + MEMORY_HEADROOM_BYTES >= memory_limit).then(|| {
            format!(
                "peak memory >= {} GiB",
                memory_limit.saturating_sub(MEMORY_HEADROOM_BYTES) >> 30
            )
        })
    }
}

/// One histogram as `p50/p95/p99 <= a/b/c ms, mean m ms, xN`.
fn summary(histogram: &Histogram) -> String {
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

/// The client's mean latency less the server's mean `server` time, in
/// milliseconds: what the client, transport, and HTTP stack add.
fn client_extra(measured: &Measured, server: &Histogram) -> String {
    match (measured.mean_seconds(), server.mean()) {
        (Some(client), Some(server)) => format!("{:.4}", (client - server) * 1e3),
        _ => "-".to_owned(),
    }
}

/// A bucket bound in milliseconds, or `-`.
fn bound_ms(bound: Option<f64>) -> String {
    bound.map_or("-".to_owned(), |bound| format!("{}", bound * 1e3))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{direct_miss, queued_miss};
    use crate::verification::run::{HISTOGRAMS, Histogram, Measured, Usage};

    /// A memory limit far above every test's peak.
    const LIMIT: u64 = 64 << 30;

    /// A window whose every [`HISTOGRAMS`] entry holds `count` observations,
    /// `slow` of them above `slow_bound` seconds.
    fn window(count: f64, slow_bound: f64, slow: f64) -> Measured {
        let bounds = [0.0005, 0.0009, 0.001, 0.009, 0.01, f64::INFINITY];
        let histogram = Histogram {
            buckets: bounds
                .iter()
                .map(|bound| {
                    (
                        *bound,
                        if *bound < slow_bound {
                            count - slow
                        } else {
                            count
                        },
                    )
                })
                .collect(),
            sum: count * 0.0004,
            count,
        };
        Measured {
            clients: 4,
            seconds: 1.0,
            latencies_us: vec![1_000; 10],
            drain: Some(1.0),
            usage: Usage {
                seconds: 1.0,
                histograms: vec![histogram; HISTOGRAMS.len()],
                ..Usage::default()
            },
            ..Measured::default()
        }
    }

    /// A direct window passes only with p99 overhead at or under 500 µs from
    /// at least 1,000 samples and no refusal or wrong verdict; the next
    /// histogram bucket above 500 µs fails, and p95 is reported, never judged.
    ///
    /// # Panics
    ///
    /// Panics when a rule judges the wrong way.
    #[test]
    fn direct_overhead_judges_p99_only() {
        assert_eq!(
            direct_miss(&window(1_000.0, 0.0009, 5.0), LIMIT),
            None,
            "p99 in le=0.0005 passes"
        );
        assert!(
            direct_miss(&window(999.0, 0.0009, 0.0), LIMIT).is_some(),
            "999 samples cannot judge"
        );
        assert!(
            direct_miss(&window(1_000.0, 0.0009, 20.0), LIMIT).is_some(),
            "p99 in le=0.0009, the next bucket, fails"
        );
        assert!(
            direct_miss(&Measured::default(), LIMIT).is_some(),
            "no data fails"
        );
        let mut refused = window(1_000.0, 0.0009, 5.0);
        refused.errors = BTreeMap::from([("refused".to_owned(), 1)]);
        assert!(direct_miss(&refused, LIMIT).is_some(), "a refusal fails");
        let mut wrong = window(1_000.0, 0.0009, 5.0);
        wrong.wrong = 1;
        assert!(
            direct_miss(&wrong, LIMIT).is_some(),
            "a wrong verdict fails"
        );
    }

    /// A `submit` window is reported whatever its latency, and fails only on
    /// a refusal or a backlog that did not drain.
    ///
    /// # Panics
    ///
    /// Panics when a rule judges the wrong way.
    #[test]
    fn queued_latency_is_reported_not_judged() {
        assert_eq!(
            queued_miss(&window(10.0, 0.0005, 10.0), LIMIT),
            None,
            "slow overhead from few samples is only reported"
        );
        let mut undrained = window(1_000.0, 0.0009, 5.0);
        undrained.drain = None;
        assert!(
            queued_miss(&undrained, LIMIT).is_some(),
            "an undrained backlog fails"
        );
        let mut refused = window(1_000.0, 0.0009, 5.0);
        refused.errors = BTreeMap::from([("refused".to_owned(), 1)]);
        assert!(queued_miss(&refused, LIMIT).is_some(), "a refusal fails");
    }
}
