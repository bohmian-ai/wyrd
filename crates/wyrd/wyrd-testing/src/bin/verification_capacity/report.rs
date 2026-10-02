//! The capacity report: correctness, coexistence, fairness, and performance
//! verdicts over every step, rendered as Markdown and JSON.
//!
//! Correctness fails on loss, duplication, a wrong judgment, or a failed
//! durable run. Coexistence fails when a mixed step does not complete every
//! kind in every progress slice, or a multi-replica queued step does not
//! execute on every replica exactly once per run. Fairness fails when the
//! quiet tenant or any background tenant completes nothing during noisy
//! traffic. Performance applies AC-040: at the highest sustainable direct
//! step of each non-judge case, solo and mixed, on one replica, at least
//! [`MIN_SAMPLES`] engine-overhead samples with 95% at or below 9 ms. Missing
//! evidence never passes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::Result;
use crate::collector::TraceSummary;
use crate::evidence::Percentiles;
use crate::step::{LaneRecord, Record};

/// Engine-overhead samples a performance verdict needs (AC-040).
pub const MIN_SAMPLES: f64 = 1_000.0;

/// Achieved share of offered work a sustainable step reaches (AC-040).
const SUSTAINED: f64 = 0.95;

/// Workloads the AC-040 overhead objective covers.
const OBJECTIVE_KINDS: [&str; 4] = ["drift_psi", "drift_spc", "drift_custom", "eval_assertion"];

/// One named check.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// Whether it held.
    pub passed: bool,
    /// The evidence behind the verdict.
    pub detail: String,
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
    /// What the sampled server traces show.
    pub traces: TraceSummary,
    /// How each replica stopped.
    pub shutdown: Vec<std::result::Result<f64, String>>,
}

/// Completions per second a lane achieved inside its arrival window.
fn achieved(record: &Record, lane: &LaneRecord) -> f64 {
    lane.progress.iter().sum::<u64>() as f64 / record.window_seconds
}

/// Whether every lane of `record` achieved [`SUSTAINED`] of its offered rate
/// with no missed arrival or refusal, and its runs drained within one step.
pub fn sustainable(record: &Record) -> bool {
    record.outstanding_at_deadline == 0
        && record.lanes.iter().all(|lane| {
            lane.tally.missed == 0
                && lane.tally.rejected.is_empty()
                && achieved(record, lane) >= SUSTAINED * lane.rate
        })
}

impl Report {
    /// Every check over every step.
    pub fn checks(&self) -> Vec<Check> {
        let mut checks = Vec::new();
        for record in &self.records {
            checks.extend(correctness(record));
            checks.extend(coexistence(record));
        }
        checks.extend(self.performance());
        checks.push(Check {
            name: "trace scrubbing".to_owned(),
            passed: self.traces.leaks.is_empty(),
            detail: if self.traces.leaks.is_empty() {
                format!(
                    "{} spans, {} sampled attempts, {} correlated",
                    self.traces.spans, self.traces.attempts, self.traces.correlated
                )
            } else {
                self.traces.leaks.join(", ")
            },
        });
        for (ordinal, shutdown) in self.shutdown.iter().enumerate() {
            checks.push(Check {
                name: format!("replica {ordinal} shutdown"),
                passed: shutdown.is_ok(),
                detail: match shutdown {
                    Ok(seconds) => format!("clean exit in {seconds:.1}s"),
                    Err(error) => error.clone(),
                },
            });
        }
        for record in &self.records {
            for profile in &record.profiles {
                checks.push(Check {
                    name: format!("profile {} replica {}", record.plan.name(), profile.replica),
                    passed: profile.failure.is_none(),
                    detail: profile
                        .failure
                        .clone()
                        .unwrap_or_else(|| profile.directory.display().to_string()),
                });
            }
        }
        checks
    }

    /// Whether every check held.
    pub fn passed(&self) -> bool {
        self.checks().iter().all(|check| check.passed)
    }

    /// AC-040 checks: one per non-judge kind and case on one direct replica.
    fn performance(&self) -> Vec<Check> {
        let mut checks = Vec::new();
        for case in ["solo", "mixed"] {
            for kind in OBJECTIVE_KINDS {
                let name = format!("AC-040 direct {case} {kind} overhead p95 < 10 ms");
                let candidates = self.records.iter().filter(|record| {
                    record.replicas == 1
                        && record.plan.path == "direct"
                        && if case == "solo" {
                            record.plan.case == kind
                        } else {
                            record.plan.case == "mixed"
                        }
                });
                let best = candidates
                    .filter(|record| sustainable(record))
                    .max_by(|a, b| a.plan.rate.total_cmp(&b.plan.rate));
                let Some(record) = best else {
                    checks.push(Check {
                        name,
                        passed: false,
                        detail: "no sustainable step was measured".to_owned(),
                    });
                    continue;
                };
                let Some(lane) = record.lanes.iter().find(|lane| lane.kind == kind) else {
                    checks.push(Check {
                        name,
                        passed: false,
                        detail: "the step has no lane of this kind".to_owned(),
                    });
                    continue;
                };
                let server = &lane.server;
                let share = server.overhead_within_bound.unwrap_or(0.0);
                checks.push(Check {
                    name,
                    passed: server.overhead_samples >= MIN_SAMPLES && share >= 0.95,
                    detail: format!(
                        "at {}/s: {:.0} samples, {:.1}% ≤ 9 ms, p95 bucket {}",
                        record.plan.rate,
                        server.overhead_samples,
                        share * 100.0,
                        seconds_ms(server.overhead.p95)
                    ),
                });
            }
        }
        checks
    }

    /// Writes `report{suffix}.md` and `report{suffix}.json` into `output`.
    ///
    /// # Errors
    ///
    /// Returns the encoding or write failure.
    pub fn write_to(&self, output: &Path, suffix: &str) -> Result<String> {
        let rendered = self.render();
        std::fs::write(output.join(format!("report{suffix}.md")), &rendered)?;
        let json = serde_json::json!({ "report": self, "checks": self.checks() });
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
            "# Verification capacity{}\n\nSetup {:.0}s, total {:.0}s. Envelope: `{}`.\n\nClient percentiles are raw samples. Server columns are bucket estimates from exporter deltas summed across replicas; phases overlap and never add. Overhead is the server's paired per-execution engine time minus its measured wait union. Queued terminal latency is PostgreSQL creation-to-settlement time per run. Queued Drift runs are manual activations, not scheduler throughput.\n",
            if self.profiled {
                " (profiled: diagnostic, not authoritative)"
            } else {
                ""
            },
            self.setup_seconds,
            self.total_seconds,
            self.envelope
        );
        let _ = writeln!(
            text,
            "| replicas | path | case | step/s | tenant | kind | mode | offered/s | achieved/s | sent/accepted/missed | refused | verdicts (wrong) | client p50/95/99 ms | terminal p50/95/99 ms | run p95 s | phase p95 s | overhead p50/95/99 ms (n, ≤9 ms) | task-start p95 µs | backlog deadline/final | sustainable |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for record in &self.records {
            for lane in &record.lanes {
                if record.plan.path == "fairness" && lane.tenant.starts_with("bg") {
                    continue;
                }
                let _ = writeln!(text, "{}", row(record, lane));
            }
            if record.plan.path == "fairness" {
                let background: Vec<&LaneRecord> = record
                    .lanes
                    .iter()
                    .filter(|lane| lane.tenant.starts_with("bg"))
                    .collect();
                let progressed = background
                    .iter()
                    .filter(|lane| lane.progress.iter().sum::<u64>() > 0)
                    .count();
                let latency: Vec<u64> = background
                    .iter()
                    .filter_map(|lane| lane.runs.as_ref())
                    .flat_map(|runs| runs.latency_us.iter().copied())
                    .collect();
                let terminal = Percentiles::raw(&latency, 1e-3);
                let _ = writeln!(
                    text,
                    "| {} | fairness | background | - | {} tenants | eval_assertion | queued | - | - | - | - | - | - | {} | - | - | - | - | - | {progressed}/{} progressed |",
                    record.replicas,
                    background.len(),
                    ms3(terminal),
                    background.len()
                );
            }
        }
        let _ = writeln!(
            text,
            "\n| replicas | step | window s | drain s | replica cores (CPU s) / peak GiB | driver cores | judge calls | profiles |\n|---|---|---|---|---|---|---|---|"
        );
        for record in &self.records {
            let resources: Vec<String> = record
                .resources
                .iter()
                .map(|resources| {
                    format!(
                        "r{} {:.2} ({:.1}) / {:.2}",
                        resources.replica,
                        resources.cores,
                        resources.cpu_seconds,
                        resources.peak_memory as f64 / f64::from(1 << 30)
                    )
                })
                .collect();
            let profiles: Vec<String> = record
                .profiles
                .iter()
                .map(|profile| {
                    format!(
                        "r{} {}",
                        profile.replica,
                        profile.failure.as_deref().unwrap_or("ok")
                    )
                })
                .collect();
            let _ = writeln!(
                text,
                "| {} | {} | {:.1} | {:.1} | {} | {:.2} | {} | {} |",
                record.replicas,
                record.plan.name(),
                record.window_seconds,
                record.drain_seconds,
                resources.join("; "),
                record.driver_cores,
                record.judge_calls,
                if profiles.is_empty() {
                    "-".to_owned()
                } else {
                    profiles.join("; ")
                }
            );
        }
        let hotspots: Vec<String> = self
            .records
            .iter()
            .flat_map(|record| {
                record
                    .profiles
                    .iter()
                    .filter(|profile| profile.failure.is_none())
                    .map(move |profile| {
                        format!(
                            "### {} replica {}\n\n```text\n{}\n```\n",
                            record.plan.name(),
                            profile.replica,
                            profile.hotspots.join("\n")
                        )
                    })
            })
            .collect();
        if !hotspots.is_empty() {
            let _ = writeln!(
                text,
                "\n## Hotspots\n\nSelf-time frames per step and replica; a mixed step's profile is process-wide, not per kind.\n\n{}",
                hotspots.join("\n")
            );
        }
        let _ = writeln!(
            text,
            "\n## Checks\n\n| check | verdict | evidence |\n|---|---|---|"
        );
        for check in self.checks() {
            let _ = writeln!(
                text,
                "| {} | {} | {} |",
                check.name,
                if check.passed { "PASS" } else { "FAIL" },
                check.detail
            );
        }
        text
    }
}

/// One Markdown table row for `lane` of `record`.
fn row(record: &Record, lane: &LaneRecord) -> String {
    let tally = &lane.tally;
    let refused = if tally.rejected.is_empty() {
        "0".to_owned()
    } else {
        tally
            .rejected
            .iter()
            .map(|(code, count)| format!("{count}× {code}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let verdicts = if tally.verdicts.is_empty() {
        lane.runs.as_ref().map_or("-".to_owned(), |runs| {
            runs.statuses
                .iter()
                .map(|(status, count)| format!("{count} {status}"))
                .collect::<Vec<_>>()
                .join(", ")
        })
    } else {
        format!(
            "{} ({})",
            tally
                .verdicts
                .iter()
                .map(|(verdict, count)| format!("{count} {verdict}"))
                .collect::<Vec<_>>()
                .join(", "),
            tally.wrong_verdicts
        )
    };
    let server = &lane.server;
    let phases: Vec<String> = server
        .phase_p95
        .iter()
        .map(|(phase, p95)| format!("{phase} {}", seconds(*p95)))
        .collect();
    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.1} | {}/{}/{} | {refused} | {verdicts} | {} | {} | {} | {} | {}/{}/{} ({:.0}, {}) | {} | {}/{} | {} |",
        record.replicas,
        record.plan.path,
        record.plan.case,
        record.plan.rate,
        lane.tenant,
        lane.kind,
        lane.mode,
        lane.rate,
        achieved(record, lane),
        tally.started,
        tally.accepted,
        tally.missed,
        ms3(tally.client_ms),
        ms3(lane.terminal_ms),
        seconds(server.run_duration.p95),
        if phases.is_empty() {
            "-".to_owned()
        } else {
            phases.join(", ")
        },
        seconds_ms(server.overhead.p50),
        seconds_ms(server.overhead.p95),
        seconds_ms(server.overhead.p99),
        server.overhead_samples,
        server
            .overhead_within_bound
            .map_or("-".to_owned(), |share| format!("{:.1}%", share * 100.0)),
        lane.task_start_us
            .p95
            .map_or("-".to_owned(), |p95| format!("{p95:.0}")),
        record.outstanding_at_deadline,
        record.outstanding_final,
        if sustainable(record) { "yes" } else { "no" }
    )
}

/// `p50/p95/p99` milliseconds of raw percentiles already in milliseconds.
fn ms3(percentiles: Percentiles) -> String {
    let one = |value: Option<f64>| value.map_or("-".to_owned(), |ms| format!("{ms:.1}"));
    format!(
        "{}/{}/{}",
        one(percentiles.p50),
        one(percentiles.p95),
        one(percentiles.p99)
    )
}

/// A bucket bound in seconds, `>max` for the overflow bucket.
fn seconds(bound: Option<f64>) -> String {
    match bound {
        None => "-".to_owned(),
        Some(bound) if bound.is_infinite() => ">max".to_owned(),
        Some(bound) => format!("{bound}"),
    }
}

/// A bucket bound in seconds, printed in milliseconds.
fn seconds_ms(bound: Option<f64>) -> String {
    match bound {
        None => "-".to_owned(),
        Some(bound) if bound.is_infinite() => ">max".to_owned(),
        Some(bound) => format!("{}", bound * 1e3),
    }
}

/// Reconciliation checks of one step.
fn correctness(record: &Record) -> Vec<Check> {
    let name = record.plan.name();
    let mut wrong = 0;
    let mut lost = Vec::new();
    let mut failed = BTreeMap::<String, u64>::new();
    for lane in &record.lanes {
        wrong += lane.tally.wrong_verdicts;
        let ended = lane.tally.accepted + lane.tally.rejected.values().sum::<u64>();
        if ended != lane.tally.started {
            lost.push(format!(
                "{} {} {}: {} sent, {ended} ended",
                lane.tenant, lane.kind, lane.mode, lane.tally.started
            ));
        }
        if let Some(runs) = &lane.runs {
            if record.outstanding_final == 0 && runs.created != lane.tally.accepted {
                lost.push(format!(
                    "{} {} queued: {} accepted, {} runs",
                    lane.tenant, lane.kind, lane.tally.accepted, runs.created
                ));
            }
            for (status, count) in &runs.statuses {
                if !matches!(
                    status.as_str(),
                    "completed" | "pending" | "running" | "retrying"
                ) {
                    *failed.entry(format!("{} {status}", lane.kind)).or_default() += count;
                }
            }
        }
    }
    vec![
        Check {
            name: format!("{name} reconciliation"),
            passed: lost.is_empty(),
            detail: if lost.is_empty() {
                "every request ended once and every accepted queued request made one run".to_owned()
            } else {
                lost.join("; ")
            },
        },
        Check {
            name: format!("{name} judgments"),
            passed: wrong == 0 && failed.is_empty(),
            detail: format!("{wrong} wrong direct verdicts; failed runs {failed:?}"),
        },
    ]
}

/// Coexistence, claim, and fairness checks of one step.
fn coexistence(record: &Record) -> Vec<Check> {
    let name = record.plan.name();
    let mut checks = Vec::new();
    if record.plan.case == "mixed" && record.plan.path != "fairness" {
        let stalled: Vec<String> = record
            .lanes
            .iter()
            .filter(|lane| lane.progress.is_empty() || lane.progress.contains(&0))
            .map(|lane| format!("{} {} {:?}", lane.kind, lane.mode, lane.progress))
            .collect();
        checks.push(Check {
            name: format!("{name} overlap"),
            passed: stalled.is_empty(),
            detail: if stalled.is_empty() {
                "every kind completed work in every 5 s slice".to_owned()
            } else {
                format!("slices without a completion: {}", stalled.join("; "))
            },
        });
    }
    if record.replicas > 1 && record.plan.path != "fairness" {
        let mut problems = Vec::new();
        for lane in record.lanes.iter().filter(|lane| lane.mode == "queued") {
            let runs = lane.runs.as_ref().map_or(0, |runs| runs.created);
            let retried = lane.runs.as_ref().map_or(0, |runs| runs.retried);
            if lane
                .server
                .attempts_per_replica
                .iter()
                .any(|count| *count <= 0.0)
            {
                problems.push(format!(
                    "{}: attempts per replica {:?}",
                    lane.kind, lane.server.attempts_per_replica
                ));
            }
            if (lane.server.attempts - runs as f64).abs() > retried as f64 {
                problems.push(format!(
                    "{}: {} attempts for {runs} runs ({retried} retried)",
                    lane.kind, lane.server.attempts
                ));
            }
        }
        if record.lanes.iter().any(|lane| lane.mode == "queued") {
            checks.push(Check {
                name: format!("{name} cross-replica claims"),
                passed: problems.is_empty(),
                detail: if problems.is_empty() {
                    "every replica executed queued work, one attempt per run".to_owned()
                } else {
                    problems.join("; ")
                },
            });
        }
    }
    if record.plan.path == "fairness" {
        let idle: Vec<String> = record
            .lanes
            .iter()
            .filter(|lane| !lane.tenant.starts_with("m0"))
            .filter(|lane| lane.progress.iter().sum::<u64>() == 0)
            .map(|lane| format!("{} {} {}", lane.tenant, lane.kind, lane.mode))
            .collect();
        checks.push(Check {
            name: format!("{name} quiet and background progress"),
            passed: idle.is_empty(),
            detail: if idle.is_empty() {
                "the quiet tenant and every background tenant completed work during noisy traffic"
                    .to_owned()
            } else {
                format!("no completion during noisy traffic: {}", idle.join(", "))
            },
        });
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::{Report, correctness, sustainable};
    use crate::evidence::RunTally;
    use crate::step::{LaneRecord, Plan, Record, TallyRecord};

    /// A lane of `mode` that sent `started`, had `accepted` accepted, and
    /// completed one execution per accepted request in its only slice.
    fn lane(mode: &'static str, started: u64, accepted: u64, runs: Option<u64>) -> LaneRecord {
        LaneRecord {
            tenant: "m0".to_owned(),
            kind: "drift_psi",
            mode,
            rate: 1.0,
            tally: TallyRecord {
                offered: started,
                started,
                accepted,
                ..TallyRecord::default()
            },
            runs: runs.map(|created| RunTally {
                created,
                ..RunTally::default()
            }),
            terminal_ms: Default::default(),
            server: Default::default(),
            task_start_us: Default::default(),
            progress: vec![accepted],
        }
    }

    /// A one-replica direct solo step of `lanes` over a 10 s window.
    fn record(lanes: Vec<LaneRecord>) -> Record {
        Record {
            replicas: 1,
            plan: Plan {
                path: "direct",
                case: "drift_psi".to_owned(),
                rate: 1.0,
                seconds: 10.0,
                lanes: Vec::new(),
                profiled: false,
            },
            window_seconds: 10.0,
            drain_seconds: 0.0,
            outstanding_at_deadline: 0,
            outstanding_final: 0,
            lanes,
            resources: Vec::new(),
            driver_cores: 0.0,
            judge_calls: 0,
            profiles: Vec::new(),
        }
    }

    /// A request that never ended, and an accepted queued request without
    /// exactly one run, each fail reconciliation; matching counts pass.
    ///
    /// # Panics
    ///
    /// Panics when a verdict is wrong.
    #[test]
    fn reconciliation_refuses_every_mismatch() {
        let passed = |lanes| correctness(&record(lanes))[0].passed;
        assert!(passed(vec![lane("direct", 10, 10, None)]));
        assert!(passed(vec![lane("queued", 10, 10, Some(10))]));
        assert!(!passed(vec![lane("direct", 10, 9, None)]));
        assert!(!passed(vec![lane("queued", 10, 10, Some(9))]));
        assert!(!passed(vec![lane("queued", 10, 10, Some(11))]));
    }

    /// A step is sustainable only when every lane completes at least 95% of
    /// its offered rate and nothing is outstanding at the deadline.
    ///
    /// # Panics
    ///
    /// Panics when a verdict is wrong.
    #[test]
    fn sustainability_needs_throughput_and_drain() {
        assert!(sustainable(&record(vec![lane("direct", 10, 10, None)])));
        assert!(!sustainable(&record(vec![lane("direct", 10, 9, None)])));
        let mut backlog = record(vec![lane("direct", 10, 10, None)]);
        backlog.outstanding_at_deadline = 1;
        assert!(!sustainable(&backlog));
    }

    /// A report without measured steps fails every AC-040 check.
    ///
    /// # Panics
    ///
    /// Panics when missing evidence passes.
    #[test]
    fn missing_evidence_never_passes() {
        let report = Report {
            profiled: false,
            setup_seconds: 0.0,
            total_seconds: 0.0,
            envelope: serde_json::Value::Null,
            records: Vec::new(),
            traces: Default::default(),
            shutdown: Vec::new(),
        };
        let objectives: Vec<_> = report
            .checks()
            .into_iter()
            .filter(|check| check.name.starts_with("AC-040"))
            .collect();
        assert_eq!(objectives.len(), 8);
        assert!(objectives.iter().all(|check| !check.passed));
        assert!(!report.passed());
    }
}
