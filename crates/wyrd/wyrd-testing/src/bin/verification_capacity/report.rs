//! The capacity report: correctness, coexistence, fairness, and performance
//! verdicts over every step, rendered as Markdown and JSON.
//!
//! Correctness fails on loss, duplication, a wrong judgment, or a failed
//! durable run. Coexistence fails when a scored step does not complete every
//! kind of the noisy tenant in every progress slice, or a multi-replica step
//! does not execute queued work on every replica exactly once per run.
//! Fairness fails when the quiet tenant completes nothing during a
//! sustained step. Performance applies AC-040: in the
//! one-replica sustained step, which must itself be sustainable, each
//! non-judge kind's direct lane needs at least [`MIN_SAMPLES`]
//! engine-overhead samples with 95% at or below 9 ms. Missing evidence never
//! passes.

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

/// Whether `record` is sustainable (AC-040): the whole step achieved
/// [`SUSTAINED`] of its offered rate with no missed arrival or refusal, and
/// its runs drained within the drain budget.
pub fn sustainable(record: &Record) -> bool {
    let offered: f64 = record.lanes.iter().map(|lane| lane.rate).sum();
    let achieved: f64 = record.lanes.iter().map(|lane| achieved(record, lane)).sum();
    record.outstanding_at_deadline == 0
        && achieved >= SUSTAINED * offered
        && record
            .lanes
            .iter()
            .all(|lane| lane.tally.missed == 0 && lane.tally.rejected.is_empty())
}

/// The step's name and replica count, unique within one run.
fn label(record: &Record) -> String {
    format!("{} on {} replica(s)", record.plan.name(), record.replicas)
}

/// Whether `lane` belongs to the noisy tenant, which carries the measured mix.
fn noisy(lane: &LaneRecord) -> bool {
    lane.tenant == "m0"
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
                    name: format!("profile {} replica {}", label(record), profile.replica),
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

    /// AC-040 checks: one per non-judge kind, from the noisy tenant's direct
    /// lane in the one-replica sustained step.
    fn performance(&self) -> Vec<Check> {
        let step = self
            .records
            .iter()
            .find(|record| record.replicas == 1 && record.plan.path == "sustained");
        OBJECTIVE_KINDS
            .iter()
            .map(|kind| {
                let name = format!("AC-040 direct {kind} overhead p95 < 10 ms");
                let Some(record) = step else {
                    return Check {
                        name,
                        passed: false,
                        detail: "no sustained step ran: no ramp step was sustainable".to_owned(),
                    };
                };
                let Some(lane) = record
                    .lanes
                    .iter()
                    .find(|lane| noisy(lane) && lane.mode == "direct" && lane.kind == *kind)
                else {
                    return Check {
                        name,
                        passed: false,
                        detail: "the step has no direct lane of this kind".to_owned(),
                    };
                };
                let server = &lane.server;
                let share = server.overhead_within_bound.unwrap_or(0.0);
                Check {
                    name,
                    passed: sustainable(record)
                        && server.overhead_samples >= MIN_SAMPLES
                        && share >= 0.95,
                    detail: format!(
                        "at {}/s{}: {:.0} samples, {:.1}% ≤ 9 ms, p95 bucket {}",
                        record.plan.rate,
                        if sustainable(record) {
                            ""
                        } else {
                            " (not sustainable)"
                        },
                        server.overhead_samples,
                        share * 100.0,
                        seconds_ms(server.overhead.p95)
                    ),
                }
            })
            .collect()
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
            "## Steps\n\n| step | replicas | offered/s | achieved/s | sent/accepted/missed | refused | wrong verdicts | client p50/95/99 ms | backlog deadline/final | drain s | sustainable |\n|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for record in &self.records {
            let _ = writeln!(text, "{}", summary(record));
        }
        let _ = writeln!(
            text,
            "\n## Per-kind breakdown\n\nNoisy tenant m0 carries the mix; the quiet tenant m1 sends queued assertions.\n\n| step | replicas | tenant | kind | mode | offered/s | achieved/s | sent/accepted/missed | refused | verdicts (wrong) | client p50/95/99 ms | terminal p50/95/99 ms | run p95 s | phase p95 s | overhead p50/95/99 ms (n, ≤9 ms) | task-start p95 µs |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for record in &self.records {
            for lane in &record.lanes {
                let _ = writeln!(text, "{}", row(record, lane));
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
        "| {} | {} | {} | {} | {} | {:.1} | {:.1} | {}/{}/{} | {refused} | {verdicts} | {} | {} | {} | {} | {}/{}/{} ({:.0}, {}) | {} |",
        record.plan.name(),
        record.replicas,
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
    )
}

/// One summary row for `record`: every lane's counts, refusals, and raw
/// client samples together.
fn summary(record: &Record) -> String {
    let mut refused = BTreeMap::<&str, u64>::new();
    let mut samples = Vec::new();
    let (mut sent, mut accepted, mut missed, mut wrong) = (0, 0, 0, 0);
    for lane in &record.lanes {
        sent += lane.tally.started;
        accepted += lane.tally.accepted;
        missed += lane.tally.missed;
        wrong += lane.tally.wrong_verdicts;
        samples.extend_from_slice(&lane.tally.client_us);
        for (code, count) in &lane.tally.rejected {
            *refused.entry(code.as_str()).or_default() += count;
        }
    }
    let refused = if refused.is_empty() {
        "0".to_owned()
    } else {
        refused
            .iter()
            .map(|(code, count)| format!("{count}× {code}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "| {} | {} | {:.1} | {:.1} | {sent}/{accepted}/{missed} | {refused} | {wrong} | {} | {}/{} | {:.1} | {} |",
        record.plan.name(),
        record.replicas,
        record.lanes.iter().map(|lane| lane.rate).sum::<f64>(),
        record
            .lanes
            .iter()
            .map(|lane| achieved(record, lane))
            .sum::<f64>(),
        ms3(Percentiles::raw(&samples, 1e-3)),
        record.outstanding_at_deadline,
        record.outstanding_final,
        record.drain_seconds,
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
    let name = label(record);
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
    let name = label(record);
    let mut checks = Vec::new();
    if record.plan.path == "warmup" {
        return checks;
    }
    let stalled: Vec<String> = record
        .lanes
        .iter()
        .filter(|lane| noisy(lane))
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
    if record.replicas > 1 {
        // Exporter attempts are per kind across every tenant, so they are
        // compared with every tenant's runs of that kind.
        let mut kinds = BTreeMap::<&str, (Vec<f64>, f64, u64, u64)>::new();
        for lane in record.lanes.iter().filter(|lane| lane.mode == "queued") {
            let entry = kinds.entry(lane.kind).or_insert_with(|| {
                (
                    lane.server.attempts_per_replica.clone(),
                    lane.server.attempts,
                    0,
                    0,
                )
            });
            entry.2 += lane.runs.as_ref().map_or(0, |runs| runs.created);
            entry.3 += lane.runs.as_ref().map_or(0, |runs| runs.retried);
        }
        let mut problems = Vec::new();
        for (kind, (per_replica, attempts, runs, retried)) in &kinds {
            if per_replica.iter().any(|count| *count <= 0.0) {
                problems.push(format!("{kind}: attempts per replica {per_replica:?}"));
            }
            if (attempts - *runs as f64).abs() > *retried as f64 {
                problems.push(format!(
                    "{kind}: {attempts} attempts for {runs} runs ({retried} retried)"
                ));
            }
        }
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
    if record.plan.path == "sustained" {
        let idle: Vec<String> = record
            .lanes
            .iter()
            .filter(|lane| !noisy(lane))
            .filter(|lane| lane.progress.iter().sum::<u64>() == 0)
            .map(|lane| format!("{} {} {}", lane.tenant, lane.kind, lane.mode))
            .collect();
        checks.push(Check {
            name: format!("{name} quiet-tenant progress"),
            passed: idle.is_empty(),
            detail: if idle.is_empty() {
                "the quiet tenant completed work during noisy traffic".to_owned()
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

    /// A one-replica ramp step of `lanes` over a 10 s window.
    fn record(lanes: Vec<LaneRecord>) -> Record {
        Record {
            replicas: 1,
            plan: Plan {
                path: "ramp",
                rate: 1.0,
                seconds: 10.0,
                drain_budget: 10.0,
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

    /// A step is sustainable only when it completes at least 95% of its
    /// offered rate and nothing is outstanding at the deadline.
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
        assert_eq!(objectives.len(), 4);
        assert!(objectives.iter().all(|check| !check.passed));
        assert!(!report.passed());
    }
}
