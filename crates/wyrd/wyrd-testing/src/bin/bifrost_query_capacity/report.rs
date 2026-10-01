//! Judges each measured step against its target and renders the report.
//!
//! Every row is PASS, FAIL, or `-` (a sweep point that only locates
//! saturation) with a one-line reason. Every row fails on any error, on peak
//! server memory at or above [`MEMORY_CEILING_BYTES`], or on an OOM kill.

use std::fmt::Write as _;
use std::path::Path;

use crate::Result;
use crate::run::{Measured, Overload, QUEUE_FULL, QUEUE_PLACES, Usage};
use crate::workload::{Case, Fixture, Target};

/// Peak server memory every step must stay below: 7 GiB of the 8-GiB pod.
const MEMORY_CEILING_BYTES: u64 = 7 << 30;

/// Durable acknowledged rows per second every write step must reach.
const MIN_WRITE_ROWS_PER_SECOND: f64 = 100_000.0;

/// Largest p95 rise reads may take while writes run, against reads alone.
const MAX_P95_RISE: f64 = 1.2;

/// The report table's header.
const HEADER: &str = "| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | seals | driver CPU | verdict | reason |\n|---|---|---|---|---|---|---|---|---|---|---|";

/// A row's judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Met every target.
    Pass,
    /// Missed a target or saw an error.
    Fail,
    /// A sweep point: reported, judged by its sweep's summary row.
    Measured,
}

/// One report line.
struct Row {
    /// Step name.
    step: String,
    /// Clients, writers, or `-`.
    clients: String,
    /// Successful queries (or acknowledged rows) per second.
    rate: String,
    /// p50/p95/p99 in milliseconds.
    latency: String,
    /// Error count and codes.
    errors: String,
    /// Server usage over the window.
    usage: Option<(f64, u64, f64, f64)>,
    /// Judgement.
    verdict: Verdict,
    /// Why.
    reason: String,
}

impl Row {
    /// Renders the row as one Markdown table line.
    fn render(&self) -> String {
        let usage = self
            .usage
            .map_or("- | - | - | -".to_owned(), |(cpu, peak, seals, driver)| {
                format!(
                    "{cpu:.2} cores | {:.2} GiB | {seals:.0} | {driver:.2} cores",
                    peak as f64 / f64::from(1 << 30)
                )
            });
        let verdict = match self.verdict {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Measured => "-",
        };
        format!(
            "| {} | {} | {} | {} | {} | {usage} | {verdict} | {} |",
            self.step, self.clients, self.rate, self.latency, self.errors, self.reason
        )
    }
}

/// Every judged row, in the order the journey ran them.
pub struct Report {
    /// Fixture the report describes.
    fixture: Fixture,
    /// Rows so far.
    rows: Vec<Row>,
    /// One JSON line per judged step: its name, clients, and every latency.
    samples: String,
}

impl Report {
    /// Starts an empty report for `fixture` and prints the table header;
    /// each row prints as it is judged.
    pub fn new(fixture: Fixture) -> Self {
        println!("{}\n\n{HEADER}", Self::title(fixture));
        Self {
            fixture,
            rows: Vec::new(),
            samples: String::new(),
        }
    }

    /// Adds a write step: it must durably acknowledge at least
    /// [`MIN_WRITE_ROWS_PER_SECOND`]. `files` and `bytes` are the Parquet
    /// objects the table holds afterwards.
    pub fn write(&mut self, step: &str, measured: &Measured, files: u64, bytes: u64) {
        let rate = measured.rows as f64 / measured.usage.seconds.max(f64::EPSILON);
        let shape = format!(
            "{} rows in {:.1}s; {files} files, avg {:.1} MiB",
            measured.rows,
            measured.usage.seconds,
            bytes as f64 / files.max(1) as f64 / f64::from(1 << 20),
        );
        let missed = (rate < MIN_WRITE_ROWS_PER_SECOND)
            .then(|| format!("{rate:.0} rows/s < {MIN_WRITE_ROWS_PER_SECOND:.0}"));
        let row = self.judge(step, measured, format!("{rate:.0} rows/s"), missed, shape);
        self.add(row);
    }

    /// Adds the exact-answer check.
    pub fn answers(&mut self, measured: &Measured) {
        let row = self.judge(
            "exact answers",
            measured,
            "-".to_owned(),
            None,
            format!("{} statements", measured.latencies_us.len()),
        );
        self.add(row);
    }

    /// Adds one query step judged against `case`'s target.
    pub fn query(&mut self, case: Case, measured: &Measured) {
        let target = case.target(self.fixture);
        let missed = missed(&target, measured);
        let reason = if target == Target::default() {
            "no target on this fixture".to_owned()
        } else {
            "meets target".to_owned()
        };
        let row = self.judge(case.name(), measured, qps(measured), missed, reason);
        self.add(row);
    }

    /// Adds a concurrency sweep of `case`: one `-` row per point, then a
    /// summary that passes when some point meets the whole target.
    pub fn sweep(&mut self, case: Case, points: &[Measured]) {
        let target = case.target(self.fixture);
        for point in points {
            let step = format!("{} sweep", case.name());
            let mut row = self.judge(&step, point, qps(point), None, String::new());
            if row.verdict == Verdict::Pass {
                row.verdict = Verdict::Measured;
                row.reason = missed(&target, point).unwrap_or_else(|| "meets target".to_owned());
            }
            self.add(row);
        }
        let meeting: Vec<String> = points
            .iter()
            .filter(|point| missed(&target, point).is_none() && point.error_count() == 0)
            .map(|point| point.clients.to_string())
            .collect();
        self.add(Row {
            step: format!("{} target", case.name()),
            clients: "-".to_owned(),
            rate: "-".to_owned(),
            latency: "-".to_owned(),
            errors: "-".to_owned(),
            usage: None,
            verdict: if meeting.is_empty() {
                Verdict::Fail
            } else {
                Verdict::Pass
            },
            reason: if meeting.is_empty() {
                "no concurrency meets the target".to_owned()
            } else {
                format!("met at {} clients", meeting.join(", "))
            },
        });
    }

    /// Adds `case` read while writes run: it must meet its target and its p95
    /// may rise less than 20% against `alone`.
    pub fn loaded(&mut self, case: Case, alone: &Measured, loaded: &Measured) {
        let target = case.target(self.fixture);
        let (before, after) = (
            alone.percentile_ms(95.0).unwrap_or(f64::INFINITY),
            loaded.percentile_ms(95.0).unwrap_or(f64::INFINITY),
        );
        let missed = missed(&target, loaded).or_else(|| {
            (after >= before * MAX_P95_RISE)
                .then(|| format!("p95 {after:.1} ms rose >= 20% from {before:.1} ms"))
        });
        let step = format!("{} while writing", case.name());
        let reason = format!("p95 alone {before:.1} ms");
        let row = self.judge(&step, loaded, qps(loaded), missed, reason);
        self.add(row);
    }

    /// Adds the full-queue step: [`QUEUE_PLACES`] wait, the next query is
    /// refused as queue-full, and the queue empties once waiters leave.
    pub fn overload(&mut self, overload: Overload) {
        let missed = if overload.queued < QUEUE_PLACES {
            Some(format!(
                "queue reached {} of {QUEUE_PLACES}",
                overload.queued
            ))
        } else if overload.overflow != QUEUE_FULL {
            Some(format!("query into full queue: {}", overload.overflow))
        } else if overload.drained_seconds.is_none() {
            Some("queue did not empty after waiters left".to_owned())
        } else {
            None
        };
        let reason = format!(
            "{} queued, overflow {}, emptied in {:.1}s",
            overload.queued,
            overload.overflow,
            overload.drained_seconds.unwrap_or(f64::NAN)
        );
        let measured = Measured {
            clients: QUEUE_PLACES + 1,
            usage: overload.usage,
            ..Measured::default()
        };
        let row = self.judge("full queue", &measured, "-".to_owned(), missed, reason);
        self.add(row);
    }

    /// Whether every judged row passed.
    pub fn passed(&self) -> bool {
        self.rows.iter().all(|row| row.verdict != Verdict::Fail)
    }

    /// Adds the shutdown step: after `SIGTERM` the server must exit cleanly
    /// within its grace period; `stopped` is its exit time or the failure.
    pub fn shutdown(&mut self, stopped: &Result<f64>) {
        let (verdict, reason) = match stopped {
            Ok(seconds) => (Verdict::Pass, format!("exited cleanly in {seconds:.1}s")),
            Err(error) => (Verdict::Fail, error.to_string()),
        };
        self.add(Row {
            step: "shutdown".to_owned(),
            clients: "-".to_owned(),
            rate: "-".to_owned(),
            latency: "-".to_owned(),
            errors: "-".to_owned(),
            usage: None,
            verdict,
            reason,
        });
    }

    /// Writes every row, as printed, into `output/report.md` and the raw
    /// latencies into `output/samples.jsonl`.
    ///
    /// # Errors
    ///
    /// Returns the file write failure.
    pub fn write_to(&self, output: &Path) -> Result<()> {
        let mut text = format!("{}\n\n{HEADER}\n", Self::title(self.fixture));
        for row in &self.rows {
            let _ = writeln!(text, "{}", row.render());
        }
        std::fs::write(output.join("report.md"), text)?;
        std::fs::write(output.join("samples.jsonl"), &self.samples)?;
        Ok(())
    }

    /// The report's title line.
    fn title(fixture: Fixture) -> String {
        format!(
            "# Bifrost query capacity: {} rows, 4 CPU / 8 GiB",
            fixture.rows
        )
    }

    /// Appends `row` and prints it.
    fn add(&mut self, row: Row) {
        println!("{}", row.render());
        self.rows.push(row);
    }

    /// Judges one row: errors, memory, and OOM fail it first, then `missed`
    /// (a target miss) does. Records the step's raw latencies.
    fn judge(
        &mut self,
        step: &str,
        measured: &Measured,
        rate: String,
        missed: Option<String>,
        reason: String,
    ) -> Row {
        let _ = writeln!(
            self.samples,
            "{}",
            serde_json::json!({
                "step": step,
                "clients": measured.clients,
                "latencies_us": measured.latencies_us,
            })
        );
        let usage_ = &measured.usage;
        let failure = if measured.error_count() > 0 {
            Some(format!("{} errors", measured.error_count()))
        } else if usage_.oom_kills > 0 {
            Some(format!("{} OOM kills", usage_.oom_kills))
        } else if usage_.peak_memory_bytes >= MEMORY_CEILING_BYTES {
            Some("peak memory >= 7 GiB".to_owned())
        } else {
            missed
        };
        let latency = match (
            measured.percentile_ms(50.0),
            measured.percentile_ms(95.0),
            measured.percentile_ms(99.0),
        ) {
            (Some(p50), Some(p95), Some(p99)) => format!("{p50:.1}/{p95:.1}/{p99:.1}"),
            _ => "-".to_owned(),
        };
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
        Row {
            step: step.to_owned(),
            clients: measured.clients.to_string(),
            rate,
            latency,
            errors,
            usage: usage(usage_),
            verdict: if failure.is_some() {
                Verdict::Fail
            } else {
                Verdict::Pass
            },
            reason: failure.unwrap_or(reason),
        }
    }
}

/// Server CPU cores, peak memory, seals, and driver CPU cores of a window;
/// `None` for a window that never opened.
fn usage(usage: &Usage) -> Option<(f64, u64, f64, f64)> {
    let seconds = usage.seconds;
    (seconds > 0.0).then(|| {
        (
            usage.server_cpu_seconds / seconds,
            usage.peak_memory_bytes,
            usage.seals,
            usage.driver_cpu_seconds / seconds,
        )
    })
}

/// Successful queries per second, formatted.
fn qps(measured: &Measured) -> String {
    format!("{:.1} qps", measured.rate())
}

/// The first target `measured` misses, or `None`.
fn missed(target: &Target, measured: &Measured) -> Option<String> {
    let ceilings = [
        ("p50", 50.0, target.p50_ms),
        ("p95", 95.0, target.p95_ms),
        ("p99", 99.0, target.p99_ms),
    ];
    for (name, percentile, ceiling) in ceilings {
        if let Some(ceiling) = ceiling {
            let actual = measured.percentile_ms(percentile).unwrap_or(f64::INFINITY);
            if actual >= ceiling {
                return Some(format!("{name} {actual:.1} ms >= {ceiling} ms"));
            }
        }
    }
    if let Some(floor) = target.min_qps
        && measured.rate() < floor
    {
        return Some(format!("{:.1} qps < {floor}", measured.rate()));
    }
    if let Some(floor) = target.min_scan_bytes_per_second {
        let scanned = measured.usage.scanned_bytes / measured.usage.seconds.max(f64::EPSILON);
        if scanned < floor {
            return Some(format!(
                "scan {:.0} MB/s < {:.0} MB/s",
                scanned / 1e6,
                floor / 1e6
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Report, Verdict};
    use crate::run::{Measured, Usage};
    use crate::workload::{Case, Fixture};

    /// A point with the selective latencies and rate.
    fn point(clients: usize, latency_ms: u64, queries: u64) -> Measured {
        Measured {
            clients,
            latencies_us: vec![latency_ms * 1_000; queries as usize],
            usage: Usage {
                seconds: 1.0,
                ..Usage::default()
            },
            ..Measured::default()
        }
    }

    /// A sweep passes when one point meets the whole target, and an error
    /// fails a row whatever its latency.
    ///
    /// # Panics
    ///
    /// Panics when a verdict is wrong.
    #[test]
    fn sweep_and_errors_are_judged() {
        let mut report = Report::new(Fixture::standard());
        report.sweep(
            Case::Selective,
            &[point(1, 1, 500), point(8, 1, 2_000), point(64, 20, 3_000)],
        );
        assert_eq!(report.rows[3].verdict, Verdict::Pass);
        assert_eq!(report.rows[3].reason, "met at 8 clients");
        assert!(report.passed());

        let mut failing = point(1, 100, 10);
        failing.errors.insert("wrong-result".to_owned(), 1);
        report.query(Case::TableAggregate, &failing);
        assert_eq!(report.rows[4].verdict, Verdict::Fail);
        assert!(!report.passed());
    }
}
