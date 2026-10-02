//! Reconciles what the clients offered against what the server
//! acknowledged, activated, settled, and still holds, and renders the report.
//!
//! Every check is PASS or FAIL with a reason; the run fails on any FAIL.
//! Capacity is labelled, never judged: a run that settles less than it was
//! offered says so in its capacity line and its backlog column, and still
//! passes when every row is accounted for.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use wyrd_testing::release_server::{CPUS, MEMORY_BYTES, Metrics};

use crate::Result;
use crate::collector::TraceSummary;
use crate::traffic::{Evidence, Flush, Phase, PhaseLoad};

/// Run statuses that still hold work.
const BACKLOG: [&str; 3] = ["pending", "running", "retrying"];

/// Run statuses that settled the run.
const TERMINAL: [&str; 4] = ["completed", "cancelled", "timed_out", "errored"];

/// Run origins, in report order.
const ORIGINS: [&str; 3] = ["observation", "schedule", "manual"];

/// Server-measured attempt phases, in report order.
const ATTEMPT_PHASES: [&str; 4] = ["load", "engine", "publication", "settlement"];

/// Fraction of the offered rate a phase must settle to be labelled as
/// sustaining it.
const SUSTAINED: f64 = 0.95;

/// One `/metrics` scrape plus the server cgroup's CPU counter.
pub struct Snapshot {
    /// The scrape.
    pub metrics: Metrics,
    /// Server `cpu.stat usage_usec`.
    pub cpu_us: u64,
}

/// One phase as driven and observed.
pub struct PhaseRecord {
    /// The profile phase.
    pub phase: Phase,
    /// Client tally merged across clients.
    pub load: PhaseLoad,
    /// Measured wall-clock seconds.
    pub seconds: f64,
    /// Peak server cgroup memory during the phase.
    pub peak_memory: u64,
}

/// Everything one tenant's reconciliation reads.
#[derive(Debug, Default, Clone)]
pub struct TenantTally {
    /// Tenant slug.
    pub slug: String,
    /// Run iterations the profile offered this tenant.
    pub offered: u64,
    /// Iterations its clients started.
    pub submitted: u64,
    /// Evidence its clients handed over and saw drained.
    pub acked: Evidence,
    /// Evidence the tenant reads back; `failing_eval` is its failed Eval
    /// verdicts.
    pub stored: Evidence,
    /// Run rows by `(origin, status)`.
    pub runs: BTreeMap<(String, String), u64>,
    /// Distinct result runs the tenant reads back.
    pub results: u64,
    /// Results naming a run or subject that is not this tenant's.
    pub foreign_results: u64,
    /// Completed runs in the queue.
    pub completed: u64,
    /// Operator dispatch rows by status.
    pub dispatches: BTreeMap<String, u64>,
    /// Requests the tenant's local Operator endpoint received.
    pub operator_posts: u64,
    /// Manual Drift runs the administrator requested.
    pub manual_requested: u64,
}

impl TenantTally {
    /// Run rows of `origin` whose status is in `statuses`.
    fn runs_in(&self, origin: &str, statuses: &[&str]) -> u64 {
        self.runs
            .iter()
            .filter(|((o, status), _)| o == origin && statuses.contains(&status.as_str()))
            .map(|(_, count)| count)
            .sum()
    }

    /// Every run row of `origin`.
    fn created(&self, origin: &str) -> u64 {
        self.runs
            .iter()
            .filter(|((o, _), _)| o == origin)
            .map(|(_, count)| count)
            .sum()
    }

    /// Settled runs of every origin.
    fn terminal(&self) -> u64 {
        ORIGINS
            .iter()
            .map(|origin| self.runs_in(origin, &TERMINAL))
            .sum()
    }
}

/// Server-wide counters the reconciliation reads.
#[derive(Debug, Default, Clone, Copy)]
pub struct Totals {
    /// `verification_observation_enqueue_failures_total` over the run.
    pub enqueue_failures: u64,
    /// Settled runs counted by `wyrd_verification_trigger_to_terminal_seconds`.
    pub terminal_metric: u64,
}

/// One judged reconciliation check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What was compared.
    pub name: String,
    /// Whether it held.
    pub passed: bool,
    /// The compared numbers or the failure.
    pub detail: String,
}

impl Check {
    /// A check holding when `passed`.
    fn new(name: impl Into<String>, passed: bool, detail: String) -> Self {
        Self {
            name: name.into(),
            passed,
            detail,
        }
    }
}

/// Judges every tenant and the server-wide totals.
///
/// Per tenant: every acknowledged signal is stored exactly once (missing
/// evidence or duplicates fail); every stored Eval observation activated one
/// run, a shortfall failing as silent loss unless the enqueue-failure counter
/// moved, when it is reported as explicit loss; every completed run has
/// exactly one result and no result names another tenant's run or subject;
/// every failed Eval verdict has one Operator dispatch and every delivered
/// dispatch reached the endpoint; scheduled Drift ran; and each requested
/// manual Drift run exists. Server-wide: the queue's settled runs equal the
/// trigger-to-terminal series' count.
pub fn reconcile(tenants: &[TenantTally], totals: Totals) -> Vec<Check> {
    let mut checks = Vec::new();
    for tenant in tenants {
        let slug = &tenant.slug;
        let signals = [
            ("drift runs", tenant.acked.drift, tenant.stored.drift),
            ("eval observations", tenant.acked.eval, tenant.stored.eval),
            ("custom rows", tenant.acked.custom, tenant.stored.custom),
            ("otlp spans", tenant.acked.spans, tenant.stored.spans),
            ("otlp logs", tenant.acked.logs, tenant.stored.logs),
            ("otlp points", tenant.acked.points, tenant.stored.points),
        ];
        for (signal, acked, stored) in signals {
            let detail = match stored.cmp(&acked) {
                std::cmp::Ordering::Equal => format!("{acked} acknowledged = {stored} stored"),
                std::cmp::Ordering::Less => format!(
                    "missing evidence: {} of {acked} acknowledged not stored",
                    acked - stored
                ),
                std::cmp::Ordering::Greater => {
                    format!("{stored} stored > {acked} acknowledged: duplicate rows")
                }
            };
            checks.push(Check::new(
                format!("{slug} {signal}"),
                acked > 0 && stored == acked,
                if acked == 0 {
                    format!("no {signal} acknowledged")
                } else {
                    detail
                },
            ));
        }

        let activated = tenant.created("observation");
        let (passed, detail) = if activated > tenant.stored.eval {
            (
                false,
                format!(
                    "{activated} runs > {} observations: duplicate activation",
                    tenant.stored.eval
                ),
            )
        } else if activated == tenant.stored.eval {
            (true, format!("{activated} runs = {activated} observations"))
        } else if totals.enqueue_failures == 0 {
            (
                false,
                format!(
                    "silent loss: {} observations never activated and no enqueue failure was counted",
                    tenant.stored.eval - activated
                ),
            )
        } else {
            (
                true,
                format!(
                    "explicit loss: {} observations not activated; {} enqueue failures counted",
                    tenant.stored.eval - activated,
                    totals.enqueue_failures
                ),
            )
        };
        checks.push(Check::new(
            format!("{slug} eval activation"),
            passed,
            detail,
        ));

        checks.push(Check::new(
            format!("{slug} results"),
            tenant.results == tenant.completed && tenant.foreign_results == 0,
            if tenant.foreign_results > 0 {
                format!("wrong-tenant results: {}", tenant.foreign_results)
            } else {
                format!(
                    "{} results for {} completed runs",
                    tenant.results, tenant.completed
                )
            },
        ));

        let dispatched: u64 = tenant.dispatches.values().sum();
        let delivered = tenant.dispatches.get("delivered").copied().unwrap_or(0);
        checks.push(Check::new(
            format!("{slug} operator"),
            dispatched == tenant.stored.failing_eval && tenant.operator_posts >= delivered,
            format!(
                "{} failed verdicts, {dispatched} dispatches, {delivered} delivered, {} received",
                tenant.stored.failing_eval, tenant.operator_posts
            ),
        ));

        let scheduled = tenant.created("schedule");
        checks.push(Check::new(
            format!("{slug} scheduled drift"),
            scheduled > 0,
            format!("{scheduled} scheduled runs"),
        ));
        let manual = tenant.created("manual");
        checks.push(Check::new(
            format!("{slug} manual drift"),
            manual == tenant.manual_requested,
            format!("{manual} runs for {} requests", tenant.manual_requested),
        ));
    }
    let terminal: u64 = tenants.iter().map(TenantTally::terminal).sum();
    checks.push(Check::new(
        "settled runs",
        terminal == totals.terminal_metric,
        format!(
            "{terminal} terminal in the queue, {} in wyrd_verification_trigger_to_terminal_seconds",
            totals.terminal_metric
        ),
    ));
    checks
}

/// The bound of the cumulative bucket holding quantile `q` of the
/// observations between two scrapes of one histogram, or `None` when none
/// were recorded.
fn quantile(before: &[(f64, f64)], after: &[(f64, f64)], q: f64) -> Option<f64> {
    let prior = |bound: f64| {
        before
            .iter()
            .find(|(b, _)| b.to_bits() == bound.to_bits())
            .map_or(0.0, |(_, count)| *count)
    };
    let deltas: Vec<(f64, f64)> = after
        .iter()
        .map(|(bound, count)| (*bound, count - prior(*bound)))
        .collect();
    let total = deltas.last().map_or(0.0, |(_, count)| *count);
    if total <= 0.0 {
        return None;
    }
    deltas
        .iter()
        .find(|(_, count)| *count >= q * total)
        .map(|(bound, _)| *bound)
}

/// The finished benchmark: inputs to every table and check.
pub struct Report {
    /// Seconds from server start to the first offered iteration.
    pub setup_seconds: f64,
    /// Tenants and clients.
    pub shape: (usize, usize),
    /// Every phase as driven.
    pub phases: Vec<PhaseRecord>,
    /// Scrapes at each phase boundary; one more than phases.
    pub snapshots: Vec<Snapshot>,
    /// Every client's graceful drain.
    pub flushes: Vec<Flush>,
    /// Every tenant's reconciliation inputs.
    pub tenants: Vec<TenantTally>,
    /// Server-wide counters.
    pub totals: Totals,
    /// Sampled server traces.
    pub traces: TraceSummary,
    /// Server sample ratio for its own traces.
    pub sample_ratio: &'static str,
    /// The server's exit, or the stop failure.
    pub shutdown: std::result::Result<f64, String>,
}

impl Report {
    /// Every check: reconciliation, traffic drains, traces, and shutdown.
    pub fn checks(&self) -> Vec<Check> {
        let mut checks = reconcile(&self.tenants, self.totals);
        let failed: Vec<&str> = self
            .flushes
            .iter()
            .filter_map(|flush| flush.error.as_deref())
            .collect();
        checks.push(Check::new(
            "client drains",
            failed.is_empty(),
            if failed.is_empty() {
                format!("{} clients drained", self.flushes.len())
            } else {
                failed.join("; ")
            },
        ));
        checks.push(Check::new(
            "attempt traces",
            self.traces.correlated > 0,
            format!(
                "{} of {} sampled attempts carry claim, load, engine, and settle spans; {} carry an evidence read",
                self.traces.correlated, self.traces.attempts, self.traces.evidence_reads
            ),
        ));
        checks.push(Check::new(
            "trace hygiene",
            self.traces.leaks.is_empty(),
            if self.traces.leaks.is_empty() {
                format!(
                    "{} spans carry no tenant id on verification spans, credential, or evidence",
                    self.traces.spans
                )
            } else {
                self.traces.leaks.join(", ")
            },
        ));
        checks.push(match &self.shutdown {
            Ok(seconds) => Check::new("shutdown", true, format!("exited cleanly in {seconds:.1}s")),
            Err(error) => Check::new("shutdown", false, error.clone()),
        });
        checks
    }

    /// Renders the whole report as Markdown.
    pub fn render(&self) -> String {
        let mut text = String::new();
        let (tenants, clients) = self.shape;
        let _ = writeln!(
            text,
            "# Verification journey: {tenants} tenants, {clients} clients, {CPUS} CPU / {} GiB\n",
            MEMORY_BYTES >> 30
        );
        let _ = writeln!(
            text,
            "Measurement boundaries: client latency is one iteration as the client sees it \
             (two enqueued observations, one awaited dataset write, three OTLP/HTTP exports); \
             acknowledged evidence is what each client's graceful drain confirmed; queue wait \
             is the server's claim time minus the run's due time on the PostgreSQL clock; \
             trigger-to-terminal is the run's age at claim plus the settling attempt; server \
             percentiles are histogram bucket bounds between phase-boundary scrapes. Setup \
             took {:.1}s. Server traces sampled at {}.\n",
            self.setup_seconds, self.sample_ratio
        );
        self.render_phases(&mut text);
        self.render_server(&mut text);
        self.render_tenants(&mut text);
        let _ = writeln!(text, "\n{}\n", self.capacity());
        let _ = writeln!(text, "| check | verdict | detail |\n|---|---|---|");
        for check in self.checks() {
            let verdict = if check.passed { "PASS" } else { "FAIL" };
            let _ = writeln!(text, "| {} | {verdict} | {} |", check.name, check.detail);
        }
        text
    }

    /// Whether every check passed.
    pub fn passed(&self) -> bool {
        self.checks().iter().all(|check| check.passed)
    }

    /// Writes the rendered report into `output/report.md` and its
    /// machine-readable counts and verdicts into `output/report.json`.
    ///
    /// # Errors
    ///
    /// Returns the serialization or file write failure.
    pub fn write_to(&self, output: &Path) -> Result<()> {
        std::fs::write(output.join("report.md"), self.render())?;
        std::fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&self.summary())?,
        )?;
        Ok(())
    }

    /// The offered profile, per-tenant accounting, capacity line, and every
    /// check as JSON.
    fn summary(&self) -> serde_json::Value {
        let phases: Vec<serde_json::Value> = self
            .phases
            .iter()
            .map(|record| {
                serde_json::json!({
                    "phase": record.phase.name,
                    "offered_per_second": record.phase.rate,
                    "seconds": record.seconds,
                    "planned": record.load.planned,
                    "submitted": record.load.submitted,
                    "late": record.load.late,
                    "errors": record.load.errors,
                    "peak_memory_bytes": record.peak_memory,
                })
            })
            .collect();
        let tenants: Vec<serde_json::Value> = self
            .tenants
            .iter()
            .map(|tenant| {
                serde_json::json!({
                    "tenant": tenant.slug,
                    "offered": tenant.offered,
                    "submitted": tenant.submitted,
                    "eval_acknowledged": tenant.acked.eval,
                    "eval_stored": tenant.stored.eval,
                    "activated": tenant.created("observation"),
                    "settled": tenant.runs_in("observation", &TERMINAL),
                    "backlog": tenant.runs_in("observation", &BACKLOG),
                    "results": tenant.results,
                    "failed_verdicts": tenant.stored.failing_eval,
                    "dispatches": tenant.dispatches,
                    "operator_received": tenant.operator_posts,
                    "scheduled_drift": tenant.created("schedule"),
                    "manual_drift": tenant.created("manual"),
                })
            })
            .collect();
        let checks: Vec<serde_json::Value> = self
            .checks()
            .into_iter()
            .map(|check| {
                serde_json::json!({
                    "check": check.name,
                    "passed": check.passed,
                    "detail": check.detail,
                })
            })
            .collect();
        serde_json::json!({
            "setup_seconds": self.setup_seconds,
            "tenants_count": self.shape.0,
            "clients": self.shape.1,
            "phases": phases,
            "tenants": tenants,
            "enqueue_failures": self.totals.enqueue_failures,
            "capacity": self.capacity(),
            "passed": self.passed(),
            "checks": checks,
        })
    }

    /// The capacity line for the steady phase, labelled from its settled
    /// Eval rate and the drain's leftover backlog.
    fn capacity(&self) -> String {
        let Some(index) = self
            .phases
            .iter()
            .position(|record| record.phase.name == "steady")
        else {
            return "Capacity: no steady phase ran.".to_owned();
        };
        let record = &self.phases[index];
        let settled = self.delta(
            index,
            "wyrd_verification_trigger_to_terminal_seconds_count",
            &["origin=\"observation\""],
        );
        let rate = settled / record.seconds.max(f64::EPSILON);
        let offered = record.phase.rate as f64;
        let backlog: u64 = self
            .tenants
            .iter()
            .map(|tenant| tenant.runs_in("observation", &BACKLOG))
            .sum();
        let submitted = record.load.submitted as f64 / record.load.planned.max(1) as f64;
        if rate >= SUSTAINED * offered && submitted >= SUSTAINED && backlog == 0 {
            format!(
                "Capacity: steady settled {rate:.1} Eval runs/s at {offered:.0} offered; the drain left no backlog."
            )
        } else {
            format!(
                "Capacity: BELOW TARGET. Steady settled {rate:.1} Eval runs/s of {offered:.0} offered \
                 ({:.0}% of planned iterations submitted); {backlog} observation runs were still \
                 queued after the drain.",
                submitted * 100.0
            )
        }
    }

    /// Increase of the series sum between phase `index`'s boundary scrapes.
    fn delta(&self, index: usize, family: &str, labels: &[&str]) -> f64 {
        self.snapshots[index + 1].metrics.sum(family, labels)
            - self.snapshots[index].metrics.sum(family, labels)
    }

    /// p50/p95/p99 of histogram `family` over phase `index`.
    fn percentiles(&self, index: usize, family: &str, labels: &[&str], scale: f64) -> String {
        let before = self.snapshots[index].metrics.buckets(family, labels);
        let after = self.snapshots[index + 1].metrics.buckets(family, labels);
        let values: Vec<String> = [0.5, 0.95, 0.99]
            .iter()
            .map(|q| {
                quantile(&before, &after, *q).map_or("-".to_owned(), |bound| {
                    if bound.is_infinite() {
                        ">max".to_owned()
                    } else {
                        format!("{}", bound * scale)
                    }
                })
            })
            .collect();
        values.join("/")
    }

    /// The client-side phase table.
    fn render_phases(&self, text: &mut String) {
        let _ = writeln!(
            text,
            "| phase | offered/s | planned | submitted | late | errors | iteration p50/p95/p99 ms | server CPU | peak memory |\n|---|---|---|---|---|---|---|---|---|"
        );
        for (index, record) in self.phases.iter().enumerate() {
            let load = &record.load;
            let latency = if load.latency_us.is_empty() {
                "-".to_owned()
            } else {
                format!(
                    "{:.1}/{:.1}/{:.1}",
                    load.latency_us.value_at_quantile(0.5) as f64 / 1e3,
                    load.latency_us.value_at_quantile(0.95) as f64 / 1e3,
                    load.latency_us.value_at_quantile(0.99) as f64 / 1e3
                )
            };
            let errors = if load.errors.is_empty() {
                "0".to_owned()
            } else {
                load.errors
                    .iter()
                    .map(|(code, count)| format!("{count}× {code}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let cpu = self.snapshots[index + 1]
                .cpu_us
                .saturating_sub(self.snapshots[index].cpu_us) as f64
                / 1e6
                / record.seconds.max(f64::EPSILON);
            let _ = writeln!(
                text,
                "| {} | {} | {} | {} | {} | {errors} | {latency} | {cpu:.2} cores | {:.2} GiB |",
                record.phase.name,
                record.phase.rate,
                load.planned,
                load.submitted,
                load.late,
                record.peak_memory as f64 / f64::from(1 << 30)
            );
        }
        let flushes: Vec<f64> = self.flushes.iter().map(|flush| flush.seconds).collect();
        let slowest = flushes.iter().copied().fold(0.0, f64::max);
        let _ = writeln!(
            text,
            "\nClient drain after the burst: {} clients, slowest {slowest:.2}s.\n",
            flushes.len()
        );
    }

    /// The server-side phase table.
    fn render_server(&self, text: &mut String) {
        let _ = writeln!(
            text,
            "| phase | attempts | settled obs/sched/manual | queue wait p50/p95/p99 s | eval trigger→terminal p50/p95/p99 s | load/engine/publication/settlement p95 s | runs pending/running/retrying at end | operator attempts | enqueue failures |\n|---|---|---|---|---|---|---|---|---|"
        );
        for (index, record) in self.phases.iter().enumerate() {
            let settled: Vec<String> = ORIGINS
                .iter()
                .map(|origin| {
                    format!(
                        "{:.0}",
                        self.delta(
                            index,
                            "wyrd_verification_trigger_to_terminal_seconds_count",
                            &[&format!("origin=\"{origin}\"")],
                        )
                    )
                })
                .collect();
            let phases: Vec<String> = ATTEMPT_PHASES
                .iter()
                .map(|phase| {
                    let label = format!("phase=\"{phase}\"");
                    let before = self.snapshots[index]
                        .metrics
                        .buckets("wyrd_verification_phase_duration_seconds", &[&label]);
                    let after = self.snapshots[index + 1]
                        .metrics
                        .buckets("wyrd_verification_phase_duration_seconds", &[&label]);
                    quantile(&before, &after, 0.95)
                        .map_or("-".to_owned(), |bound| format!("{bound}"))
                })
                .collect();
            let end = &self.snapshots[index + 1].metrics;
            let depth: Vec<String> = BACKLOG
                .iter()
                .map(|status| {
                    format!(
                        "{:.0}",
                        end.sum(
                            "wyrd_verification_queue_depth",
                            &["queue=\"runs\"", &format!("status=\"{status}\"")],
                        )
                    )
                })
                .collect();
            let _ = writeln!(
                text,
                "| {} | {:.0} | {} | {} | {} | {} | {} | {:.0} | {:.0} |",
                record.phase.name,
                self.delta(index, "wyrd_verification_run_attempts_total", &[]),
                settled.join("/"),
                self.percentiles(index, "wyrd_verification_queue_wait_seconds", &[], 1.0),
                self.percentiles(
                    index,
                    "wyrd_verification_trigger_to_terminal_seconds",
                    &["origin=\"observation\""],
                    1.0
                ),
                phases.join("/"),
                depth.join("/"),
                self.delta(index, "wyrd_operator_dispatch_attempts_total", &[]),
                self.delta(
                    index,
                    "verification_observation_enqueue_failures_total",
                    &[]
                ),
            );
        }
    }

    /// The per-tenant reconciliation table.
    fn render_tenants(&self, text: &mut String) {
        let _ = writeln!(
            text,
            "\n| tenant | offered | submitted | eval acked/stored | activated | settled | backlog | results | failed verdicts | dispatches delivered/total | operator received | scheduled/manual drift | custom acked/stored | otlp spans/logs/points stored |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
        );
        for tenant in &self.tenants {
            let dispatched: u64 = tenant.dispatches.values().sum();
            let _ = writeln!(
                text,
                "| {} | {} | {} | {}/{} | {} | {} | {} | {} | {} | {}/{dispatched} | {} | {}/{} | {}/{} | {}/{}/{} |",
                tenant.slug,
                tenant.offered,
                tenant.submitted,
                tenant.acked.eval,
                tenant.stored.eval,
                tenant.created("observation"),
                tenant.runs_in("observation", &TERMINAL),
                tenant.runs_in("observation", &BACKLOG),
                tenant.results,
                tenant.stored.failing_eval,
                tenant.dispatches.get("delivered").copied().unwrap_or(0),
                tenant.operator_posts,
                tenant.created("schedule"),
                tenant.created("manual"),
                tenant.acked.custom,
                tenant.stored.custom,
                tenant.stored.spans,
                tenant.stored.logs,
                tenant.stored.points,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TenantTally, Totals, quantile, reconcile};
    use crate::traffic::Evidence;

    /// A fully reconciled tenant: 100 iterations, five failing.
    fn clean() -> TenantTally {
        let evidence = Evidence {
            drift: 100,
            eval: 100,
            failing_eval: 5,
            custom: 100,
            spans: 100,
            logs: 100,
            points: 100,
        };
        let runs = [
            (("observation", "completed"), 98),
            (("observation", "pending"), 2),
            (("schedule", "completed"), 3),
            (("manual", "completed"), 1),
        ];
        TenantTally {
            slug: "t0".to_owned(),
            offered: 100,
            submitted: 100,
            acked: evidence,
            stored: evidence,
            runs: runs
                .iter()
                .map(|((origin, status), count)| {
                    (((*origin).to_owned(), (*status).to_owned()), *count)
                })
                .collect(),
            results: 102,
            foreign_results: 0,
            completed: 102,
            dispatches: [("delivered".to_owned(), 5)].into_iter().collect(),
            operator_posts: 5,
            manual_requested: 1,
        }
    }

    /// The failed check names.
    fn failures(tenant: &TenantTally, totals: Totals) -> Vec<String> {
        reconcile(std::slice::from_ref(tenant), totals)
            .into_iter()
            .filter(|check| !check.passed)
            .map(|check| check.name)
            .collect()
    }

    /// A reconciled tenant passes; missing evidence, silent activation loss,
    /// a wrong-tenant result, an undispatched failed verdict, missing
    /// scheduled Drift, and a settled-count mismatch each fail, while a
    /// shortfall the enqueue-failure counter explains is explicit loss.
    ///
    /// # Panics
    ///
    /// Panics when a check's verdict is wrong.
    #[test]
    fn reconciliation_refuses_every_mismatch() {
        let totals = Totals {
            enqueue_failures: 0,
            terminal_metric: 102,
        };
        assert!(failures(&clean(), totals).is_empty());

        let mut missing = clean();
        missing.stored.eval = 99;
        missing
            .runs
            .insert(("observation".to_owned(), "pending".to_owned()), 1);
        assert_eq!(failures(&missing, totals), ["t0 eval observations"]);

        let mut lost = clean();
        lost.runs
            .insert(("observation".to_owned(), "pending".to_owned()), 1);
        assert_eq!(failures(&lost, totals), ["t0 eval activation"]);
        let explained = Totals {
            enqueue_failures: 1,
            ..totals
        };
        assert!(failures(&lost, explained).is_empty());

        let mut foreign = clean();
        foreign.foreign_results = 1;
        assert_eq!(failures(&foreign, totals), ["t0 results"]);

        let mut undispatched = clean();
        undispatched.dispatches.insert("delivered".to_owned(), 4);
        assert_eq!(failures(&undispatched, totals), ["t0 operator"]);

        let mut unscheduled = clean();
        unscheduled
            .runs
            .remove(&("schedule".to_owned(), "completed".to_owned()));
        assert_eq!(
            failures(
                &unscheduled,
                Totals {
                    terminal_metric: 99,
                    ..totals
                }
            ),
            ["t0 scheduled drift"]
        );

        assert_eq!(
            failures(
                &clean(),
                Totals {
                    terminal_metric: 101,
                    ..totals
                }
            ),
            ["settled runs"]
        );
    }

    /// A quantile is the first cumulative bucket bound covering it among the
    /// observations between two scrapes.
    ///
    /// # Panics
    ///
    /// Panics when a bound is wrong.
    #[test]
    fn quantile_reads_bucket_deltas() {
        let before = [(0.1, 10.0), (1.0, 10.0), (f64::INFINITY, 10.0)];
        let after = [(0.1, 15.0), (1.0, 105.0), (f64::INFINITY, 110.0)];
        assert_eq!(quantile(&before, &after, 0.05), Some(0.1));
        assert_eq!(quantile(&before, &after, 0.5), Some(1.0));
        assert_eq!(quantile(&before, &after, 0.99), Some(f64::INFINITY));
        assert_eq!(quantile(&after, &after, 0.5), None);
    }
}
