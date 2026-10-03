//! The capacity report: the REQ-171 golden-signal SLOs judged per step and
//! rendered as one Markdown table plus JSON.
//!
//! A step passes only when every judged cell passes:
//! - traffic: every operation completed at least [`TRAFFIC`] of what it
//!   offered inside the arrival window;
//! - errors: none — no refusal, loss, duplicate, wrong judgment, or failed
//!   run (intentionally failing inputs judged failed are correct);
//! - overhead: each non-judge kind's direct paired engine overhead p95
//!   bucket below 10 ms, from at least [`MIN_SAMPLES`] samples in the
//!   one-replica sustained step (AC-040);
//! - ingest drain: every tenant's client queue drained within
//!   [`INGEST_DRAIN`] with no `QUEUE_FULL` (AC-041, counted as an error);
//! - backlogs: run queue, Scribe, audit outbox, and Forge demand empty
//!   within [`DRAIN_LIMIT`] of load stopping.
//!
//! Client latency and replica CPU and memory are reported, never judged.
//! The run passes when the one-replica sustained step and both two-replica
//! steps pass. Missing evidence never passes.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::Result;
use crate::load::Op;
use crate::step::{DRAIN_LIMIT, OpRecord, Record};

/// Achieved share of offered traffic a step needs (REQ-171).
const TRAFFIC: f64 = 0.95;

/// Overhead samples per kind the one-replica sustained step needs (AC-040).
const MIN_SAMPLES: f64 = 1_000.0;

/// Exclusive p95 overhead bound, seconds (AC-040).
const OVERHEAD_LIMIT: f64 = 0.010;

/// Longest client-queue drain after load stops, seconds (AC-041).
const INGEST_DRAIN: f64 = 1.0;

/// Kinds the overhead SLO covers; the judge kind is reported only.
const OBJECTIVE_KINDS: [&str; 4] = ["drift_psi", "drift_spc", "drift_custom", "eval_assertion"];

/// One judged or reported cell.
#[derive(Debug, Clone, Serialize)]
pub struct Cell {
    /// `Some(true)` PASS, `Some(false)` FAIL, `None` reported only.
    pub pass: Option<bool>,
    /// The evidence shown.
    pub text: String,
}

impl Cell {
    /// A judged cell.
    fn judged(pass: bool, text: String) -> Self {
        Self {
            pass: Some(pass),
            text,
        }
    }

    /// A reported cell.
    fn reported(text: String) -> Self {
        Self { pass: None, text }
    }

    /// The cell as Markdown.
    fn render(&self) -> String {
        match self.pass {
            Some(true) => format!("PASS {}", self.text),
            Some(false) => format!("**FAIL** {}", self.text),
            None => self.text.clone(),
        }
    }
}

/// The SLI columns of one row, in table order.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    /// Traffic: achieved ÷ offered.
    pub traffic: Cell,
    /// Errors.
    pub errors: Cell,
    /// Direct paired engine overhead p95, non-judge kinds.
    pub overhead: Cell,
    /// Scribe ingest client queue drain.
    pub ingest_drain: Cell,
    /// Every server backlog drained.
    pub backlog: Cell,
    /// Client latency p50/p95/p99.
    pub latency: Cell,
    /// Replica CPU cores and peak memory.
    pub resources: Cell,
}

impl Row {
    /// Whether every judged cell passed.
    pub fn passed(&self) -> bool {
        [
            &self.traffic,
            &self.errors,
            &self.overhead,
            &self.ingest_drain,
            &self.backlog,
        ]
        .iter()
        .all(|cell| cell.pass != Some(false))
    }

    /// The SLI cells as Markdown table cells.
    fn render(&self) -> String {
        [
            &self.traffic,
            &self.errors,
            &self.overhead,
            &self.ingest_drain,
            &self.backlog,
            &self.latency,
            &self.resources,
        ]
        .iter()
        .map(|cell| cell.render())
        .collect::<Vec<_>>()
        .join(" | ")
    }
}

/// The share of `record`'s offered arrivals it completed, or `None` when
/// nothing was offered.
fn achieved(record: &OpRecord) -> Option<f64> {
    (record.offered > 0).then(|| record.completed as f64 / record.offered as f64)
}

/// The traffic cell of one operation.
fn traffic(record: &OpRecord, seconds: f64) -> Cell {
    match achieved(record) {
        Some(share) => Cell::judged(
            share >= TRAFFIC,
            format!(
                "{:.1}% of {:.0}/s",
                share * 100.0,
                record.offered as f64 / seconds
            ),
        ),
        None => Cell::judged(false, "nothing offered".to_owned()),
    }
}

/// The errors cell of one operation.
fn errors(record: &OpRecord) -> Cell {
    let total: u64 = record.errors.values().sum();
    Cell::judged(
        total == 0,
        if total == 0 {
            "0".to_owned()
        } else {
            record
                .errors
                .iter()
                .map(|(cause, count)| format!("{count} {cause}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}

/// The client latency cell of one operation, milliseconds.
fn latency(record: &OpRecord) -> Cell {
    let one = |value: Option<f64>| value.map_or("-".to_owned(), |ms| format!("{ms:.1}"));
    Cell::reported(format!(
        "{}/{}/{} ms",
        one(record.client_ms.p50),
        one(record.client_ms.p95),
        one(record.client_ms.p99)
    ))
}

/// A cell that does not apply to the row.
fn none() -> Cell {
    Cell::reported("-".to_owned())
}

/// The overhead cell of `record`: every non-judge kind below the bound,
/// with the sample floor in the one-replica sustained step; the judge kind
/// is shown for diagnosis.
fn overhead(record: &Record) -> Cell {
    let mut pass = true;
    let mut parts = Vec::new();
    for overhead in &record.overhead {
        let judged = OBJECTIVE_KINDS.contains(&overhead.kind);
        let ok = overhead.p95.is_some_and(|p95| p95 < OVERHEAD_LIMIT)
            && (!record.plan.sample_floor || overhead.samples >= MIN_SAMPLES);
        pass &= !judged || ok;
        parts.push(format!(
            "{}{} {} (n {:.0})",
            if judged && !ok { "✗ " } else { "" },
            overhead.kind,
            overhead
                .p95
                .map_or("-".to_owned(), |p95| if p95.is_infinite() {
                    ">max".to_owned()
                } else {
                    format!("≤{} ms", p95 * 1e3)
                }),
            overhead.samples
        ));
    }
    Cell::judged(pass, parts.join(", "))
}

/// The ingest drain cell of an operation record, when it measured one.
fn ingest_drain(record: &OpRecord) -> Cell {
    match record.drain_seconds {
        Some(seconds) => Cell::judged(seconds <= INGEST_DRAIN, format!("{seconds:.3} s")),
        None => Cell::judged(false, "not measured".to_owned()),
    }
}

/// The backlog cell of `record`.
fn backlog(record: &Record) -> Cell {
    let left = record.backlog;
    match record.backlog_drain_seconds {
        Some(seconds) => Cell::judged(true, format!("{seconds:.1} s")),
        None => Cell::judged(
            false,
            format!(
                "after {} s: runs {}, scribe {}, audit {}, forge {}",
                DRAIN_LIMIT.as_secs(),
                left.runs,
                left.scribe,
                left.audit,
                left.forge
            ),
        ),
    }
}

/// The step row of `record`: each operation-level SLI holds for every
/// operation.
pub fn step_row(record: &Record) -> Row {
    let mut traffic_pass = true;
    let mut traffic_parts = Vec::new();
    let mut error_pass = true;
    let mut error_parts = Vec::new();
    let mut slowest: Option<(f64, Op)> = None;
    for op in Op::ALL {
        let Some(evidence) = record.op(op) else {
            traffic_pass = false;
            traffic_parts.push(format!("{} missing", op.label()));
            continue;
        };
        let cell = traffic(evidence, record.window_seconds);
        if cell.pass == Some(false) {
            traffic_pass = false;
            traffic_parts.push(format!("{} {}", op.label(), cell.text));
        }
        let cell = errors(evidence);
        if cell.pass == Some(false) {
            error_pass = false;
            error_parts.push(format!("{}: {}", op.label(), cell.text));
        }
        if let Some(p95) = evidence.client_ms.p95
            && slowest.is_none_or(|(worst, _)| p95 > worst)
        {
            slowest = Some((p95, op));
        }
    }
    let lowest = Op::ALL
        .iter()
        .filter_map(|op| record.op(*op).and_then(achieved))
        .fold(f64::INFINITY, f64::min);
    let ingest = record
        .op(Op::Ingest)
        .map_or_else(|| Cell::judged(false, "missing".to_owned()), ingest_drain);
    Row {
        traffic: Cell::judged(
            traffic_pass,
            if traffic_parts.is_empty() {
                format!("lowest {:.1}%", lowest * 100.0)
            } else {
                traffic_parts.join("; ")
            },
        ),
        errors: Cell::judged(
            error_pass,
            if error_parts.is_empty() {
                "0".to_owned()
            } else {
                error_parts.join("; ")
            },
        ),
        overhead: overhead(record),
        ingest_drain: ingest,
        backlog: backlog(record),
        latency: Cell::reported(slowest.map_or("-".to_owned(), |(p95, op)| {
            format!("slowest p95 {p95:.1} ms ({})", op.label())
        })),
        resources: Cell::reported(
            record
                .resources
                .iter()
                .map(|resources| {
                    format!(
                        "r{} {:.2} cores / {:.2} GiB",
                        resources.replica,
                        resources.cores,
                        resources.peak_memory as f64 / f64::from(1 << 30)
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        ),
    }
}

/// The per-operation row of `op` in `record`, for diagnosis: the same
/// columns, with those that do not apply to the operation left blank.
fn op_row(record: &Record, op: Op, evidence: &OpRecord) -> Row {
    Row {
        traffic: traffic(evidence, record.window_seconds),
        errors: errors(evidence),
        overhead: if op == Op::Direct {
            overhead(record)
        } else {
            none()
        },
        ingest_drain: if op == Op::Ingest {
            ingest_drain(evidence)
        } else {
            none()
        },
        backlog: match op {
            Op::Queued => Cell::reported(format!("runs left {}", record.backlog.runs)),
            Op::Ingest => Cell::reported(format!("scribe left {}", record.backlog.scribe)),
            _ => none(),
        },
        latency: latency(evidence),
        resources: if evidence.missed > 0 {
            Cell::reported(format!(
                "{} arrivals found no driver permit",
                evidence.missed
            ))
        } else {
            none()
        },
    }
}

/// The finished benchmark.
#[derive(Debug, Serialize)]
pub struct Report {
    /// Whether this run was profiled, and so is diagnostic only.
    pub profiled: bool,
    /// Seconds spent starting, provisioning, fitting, and seeding.
    pub setup_seconds: f64,
    /// Seconds the whole run took.
    pub total_seconds: f64,
    /// The deployment and driver settings every step ran under.
    pub envelope: serde_json::Value,
    /// Every step, in order.
    pub records: Vec<Record>,
    /// The knee `K`: the highest ramp level that passed.
    pub knee: Option<f64>,
    /// How each replica stopped.
    pub shutdown: Vec<std::result::Result<f64, String>>,
}

impl Report {
    /// The verdict steps: the one-replica sustained step and both
    /// two-replica steps; missing ones fail.
    fn verdict_steps(&self) -> [(&'static str, Option<&Record>); 3] {
        let find = |name: &str, replicas: usize| {
            self.records
                .iter()
                .find(|record| record.plan.name == name && record.replicas == replicas)
        };
        [
            ("sustained on 1 replica", find("sustained", 1)),
            ("sustained on 2 replicas", find("sustained", 2)),
            ("scale-out on 2 replicas", find("scale-out", 2)),
        ]
    }

    /// Whether the run passed: every verdict step passed and, when
    /// profiling, every capture is usable evidence.
    pub fn passed(&self) -> bool {
        self.verdict_steps()
            .iter()
            .all(|(_, record)| record.is_some_and(|record| step_row(record).passed()))
            && self
                .records
                .iter()
                .flat_map(|record| &record.profiles)
                .all(|profile| profile.failure.is_none())
    }

    /// Writes `report{suffix}.md` and `report{suffix}.json` into `output`.
    ///
    /// # Errors
    ///
    /// Returns the encoding or write failure.
    pub fn write_to(&self, output: &Path, suffix: &str) -> Result<String> {
        let rendered = self.render();
        std::fs::write(output.join(format!("report{suffix}.md")), &rendered)?;
        let rows: Vec<Row> = self.records.iter().map(step_row).collect();
        let json =
            serde_json::json!({ "report": self, "step_rows": rows, "passed": self.passed() });
        std::fs::write(
            output.join(format!("report{suffix}.json")),
            serde_json::to_vec(&json)?,
        )?;
        Ok(rendered)
    }

    /// The Markdown report.
    fn render(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "# Capacity{}\n\n**Verdict: {}.** Knee K = {}. Setup {:.0} s, total {:.0} s.\n",
            if self.profiled {
                " (profiled: diagnostic, not authoritative)"
            } else {
                ""
            },
            if self.passed() { "PASS" } else { "FAIL" },
            self.knee
                .map_or("none".to_owned(), |knee| format!("{knee}/s")),
            self.setup_seconds,
            self.total_seconds,
        );
        for (name, record) in self.verdict_steps() {
            let _ = writeln!(
                text,
                "- {name}: {}",
                match record {
                    None => "FAIL (did not run)",
                    Some(record) if step_row(record).passed() => "PASS",
                    Some(_) => "FAIL",
                }
            );
        }
        let _ = writeln!(
            text,
            "\nL is verification executions per second over four identical tenants; each step offers direct and queued verification at L/2 each, Scribe ingest at 2.5·L Drift observations of 100 features, and Oracle queries at L/2. SLOs: traffic ≥ 95% of offered per operation; 0 errors (refused, lost, duplicate, wrong judgment, failed run; QUEUE_FULL counts); direct overhead p95 < 10 ms for PSI, SPC, Custom and assertion Eval, from ≥ 1,000 samples each in the 1-replica sustained step; client queue drain ≤ 1 s; every backlog (run queue, Scribe, audit outbox, Forge demand) drained within 60 s of load stopping. Latency and CPU/memory are reported, not judged. Overhead figures are bucket upper bounds.\n"
        );
        let header = "| step | replicas | L | traffic | errors | direct overhead p95 | ingest drain | backlogs drained | client latency | CPU / memory | result |\n|---|---|---|---|---|---|---|---|---|---|---|";
        let _ = writeln!(text, "## Steps\n\n{header}");
        for record in &self.records {
            let row = step_row(record);
            let _ = writeln!(
                text,
                "| {} | {} | {} | {} | {} |",
                record.plan.name,
                record.replicas,
                record.plan.level,
                row.render(),
                result(record, &row)
            );
        }
        let _ = writeln!(
            text,
            "\n## Per operation\n\n| step | replicas | L | operation | traffic | errors | direct overhead p95 | ingest drain | backlog left | client p50/p95/p99 | driver |\n|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for record in &self.records {
            for (op, evidence) in &record.ops {
                let _ = writeln!(
                    text,
                    "| {} | {} | {} | {} | {} |",
                    record.plan.name,
                    record.replicas,
                    record.plan.level,
                    op.label(),
                    op_row(record, *op, evidence).render()
                );
            }
        }
        let _ = writeln!(
            text,
            "\nEnvelope: `{}`. Judge calls per step: {}. Replica shutdown: {}.",
            self.envelope,
            self.records
                .iter()
                .map(|record| record.judge_calls.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.shutdown
                .iter()
                .enumerate()
                .map(|(ordinal, stopped)| match stopped {
                    Ok(seconds) => format!("r{ordinal} clean in {seconds:.1} s"),
                    Err(error) => format!("r{ordinal} {error}"),
                })
                .collect::<Vec<_>>()
                .join("; ")
        );
        let profiles: Vec<String> = self
            .records
            .iter()
            .flat_map(|record| {
                record
                    .profiles
                    .iter()
                    .map(move |profile| match &profile.failure {
                        Some(failure) => format!(
                            "### {} {} r{}\n\n**FAIL** {failure}\n",
                            record.plan.name, record.replicas, profile.replica
                        ),
                        None => format!(
                            "### {} {} r{}\n\n```text\n{}\n```\n",
                            record.plan.name,
                            record.replicas,
                            profile.replica,
                            profile.hotspots.join("\n")
                        ),
                    })
            })
            .collect();
        if !profiles.is_empty() {
            let _ = writeln!(text, "\n## Profiles\n\n{}", profiles.join("\n"));
        }
        text
    }
}

/// The step's result cell: `not judged` for the warmup.
fn result(record: &Record, row: &Row) -> &'static str {
    if !record.plan.judged {
        "not judged"
    } else if row.passed() {
        "PASS"
    } else {
        "**FAIL**"
    }
}

#[cfg(test)]
mod tests {
    use super::{Report, step_row};
    use crate::evidence::{Backlog, Overhead};
    use crate::load::Op;
    use crate::step::{OpRecord, Plan, Record};

    /// A step whose every operation completed all it offered with no error,
    /// whose overhead holds with `samples` per kind, and whose backlogs
    /// drained.
    fn record(name: &'static str, replicas: usize, samples: f64) -> Record {
        let op = OpRecord {
            offered: 100,
            completed: 100,
            drain_seconds: Some(0.2),
            ..OpRecord::default()
        };
        Record {
            replicas,
            plan: Plan {
                name,
                level: 50.0,
                seconds: 10.0,
                judged: true,
                sample_floor: name == "sustained" && replicas == 1,
            },
            window_seconds: 10.0,
            ops: Op::ALL.into_iter().map(|op_| (op_, op.clone())).collect(),
            overhead: [
                "drift_psi",
                "drift_spc",
                "drift_custom",
                "eval_assertion",
                "eval_llm_judge",
            ]
            .into_iter()
            .map(|kind| Overhead {
                kind,
                samples,
                p95: Some(0.005),
            })
            .collect(),
            backlog_drain_seconds: Some(1.0),
            backlog: Backlog::default(),
            resources: Vec::new(),
            judge_calls: 0,
            profiles: Vec::new(),
        }
    }

    /// Every SLO failure fails its step: low traffic, any error, slow
    /// overhead, too few samples in the one-replica sustained step, a slow
    /// client drain, and an undrained backlog. The judge kind's overhead is
    /// reported only.
    ///
    /// # Panics
    ///
    /// Panics when a verdict is wrong.
    #[test]
    fn every_slo_failure_fails_the_step() {
        assert!(step_row(&record("ramp", 1, 10.0)).passed());
        let mut slow = record("ramp", 1, 10.0);
        slow.ops[1].1.completed = 94;
        assert!(!step_row(&slow).passed());
        let mut refused = record("ramp", 1, 10.0);
        refused.ops[2]
            .1
            .errors
            .insert("WYRD_CLIENT_429_QUEUE_FULL".to_owned(), 1);
        assert!(!step_row(&refused).passed());
        let mut overhead = record("ramp", 1, 10.0);
        overhead.overhead[0].p95 = Some(0.01);
        assert!(!step_row(&overhead).passed());
        let mut judge = record("ramp", 1, 10.0);
        judge.overhead[4].p95 = Some(0.5);
        assert!(step_row(&judge).passed());
        assert!(!step_row(&record("sustained", 1, 999.0)).passed());
        assert!(step_row(&record("sustained", 1, 1_000.0)).passed());
        assert!(step_row(&record("sustained", 2, 10.0)).passed());
        let mut drain = record("ramp", 1, 10.0);
        drain.ops[2].1.drain_seconds = Some(1.5);
        assert!(!step_row(&drain).passed());
        let mut backlog = record("ramp", 1, 10.0);
        backlog.backlog_drain_seconds = None;
        backlog.backlog.audit = 3;
        assert!(!step_row(&backlog).passed());
    }

    /// The run passes only with all three verdict steps passing; a missing
    /// one fails it.
    ///
    /// # Panics
    ///
    /// Panics when the verdict is wrong.
    #[test]
    fn verdict_needs_every_verdict_step() {
        let report = |records| Report {
            profiled: false,
            setup_seconds: 0.0,
            total_seconds: 0.0,
            envelope: serde_json::Value::Null,
            records,
            knee: Some(50.0),
            shutdown: Vec::new(),
        };
        let all = vec![
            record("sustained", 1, 1_000.0),
            record("sustained", 2, 1.0),
            record("scale-out", 2, 1.0),
        ];
        assert!(report(all).passed());
        assert!(!report(vec![record("sustained", 1, 1_000.0)]).passed());
        assert!(!report(Vec::new()).passed());
    }
}
