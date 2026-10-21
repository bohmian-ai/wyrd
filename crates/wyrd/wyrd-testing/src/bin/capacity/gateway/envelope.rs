//! Judges each capacity window and finds each case's envelope.
//!
//! A rate row is INVALID when its evidence cannot judge it: an arrival the
//! driver missed or capped, send lateness p99 above 5 ms, a tenant/mode
//! with fewer than [`MIN_CALLS`] completions, unmatched upstream sends, or
//! missing backlog readings. Otherwise it is SUSTAINABLE when every call
//! answered and every tenant/mode distribution, and each mode's aggregate,
//! meets the declared p99 bounds (equality passes) with backlog growth from
//! the first to the last [`EDGE`] of at most one second of offered work;
//! else it is OUTSIDE-SLO, which is exploration, not failure. Integrity is
//! its own row: one staged capture per admitted call, no drop or loss,
//! durable rows within each tenant's own admitted calls, and memory
//! headroom. The overload row allows only configured admission refusals;
//! the recovery row reapplies the SLOs to its final minute. Every judgment
//! collects all of its misses and is a pure function of the measurements.

use std::collections::BTreeMap;

use num_traits::ToPrimitive;
use wyrd_testing::capacity::Percentiles;

use crate::gateway::capacity::{
    Arrived, Backlog, EDGE, LATENESS_LIMIT_US, Load, MIN_CALLS, Measured, Slo, TENANTS,
};
use crate::gateway::report::{
    MEMORY_HEADROOM_BYTES, Report, Row, Verdict, client, gib, integrity_misses, summary,
};
use crate::gateway::run::{BUFFERED_ANSWER, STREAM_END, micros, outcome_key};

/// Reads one backlog series, such as in-flight calls or outbox depth, from a sample.
type Series = fn(&Backlog) -> f64;

/// The capacity report table's header.
pub const CAPACITY_HEADER: &str = "| step | offered | scheduled/sent/missed/capped | completed | errors | server CPU | peak memory | driver+upstream CPU | verdict | evidence |\n|---|---|---|---|---|---|---|---|---|---|";

/// The admission refusals a configured gateway limit or budget answers;
/// the only errors an overload interval may see.
pub const ADMISSION_REFUSALS: [&str; 2] = [
    "WYRD_GATEWAY_429_LIMIT_EXCEEDED",
    "WYRD_GATEWAY_429_BUDGET_EXCEEDED",
];

/// One tenant's, or every tenant's, completed calls of one mode, measured
/// from each call's scheduled arrival, microseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Distribution {
    /// Tenant index, or `None` for the mode's aggregate.
    pub tenant: Option<usize>,
    /// Whether the calls streamed.
    pub stream: bool,
    /// Until the complete validated answer.
    pub completion_us: Vec<u64>,
    /// Streams: until the first complete data event.
    pub first_us: Vec<u64>,
    /// Streams: each data event's receipt behind its upstream send.
    pub lag_us: Vec<u64>,
    /// Streams whose data events and upstream sends did not pair one to one.
    pub unmatched: u64,
}

impl Distribution {
    /// The distribution's name: `<tenant>/<mode>` or `all/<mode>`.
    fn name(&self) -> String {
        let tenant = self.tenant.map_or("all", |tenant| TENANTS[tenant]);
        let mode = if self.stream { "stream" } else { "buffered" };
        format!("{tenant}/{mode}")
    }

    /// Adds one answered call.
    fn add(&mut self, call: &Arrived) {
        let since = |at: std::time::Instant| micros(at.saturating_duration_since(call.scheduled));
        self.completion_us.push(since(call.timed.done));
        if !self.stream {
            return;
        }
        if let Some(first) = call.timed.events.first() {
            self.first_us.push(since(*first));
        }
        if call.sends.len() != call.timed.events.len() {
            self.unmatched += 1;
        }
        for (received, sent) in call.timed.events.iter().zip(&call.sends) {
            self.lag_us
                .push(micros(received.saturating_duration_since(*sent)));
        }
    }

    /// Every p99 bound of `slo` this distribution misses, by name.
    fn misses(&self, slo: &Slo) -> Vec<String> {
        let checks: Vec<(&str, &[u64], Option<f64>)> = if self.stream {
            vec![
                ("first token", &self.first_us, slo.first_token),
                ("chunk lag", &self.lag_us, slo.chunk_lag),
                ("stream", &self.completion_us, slo.stream),
            ]
        } else {
            vec![("answer", &self.completion_us, slo.answer)]
        };
        let mut misses = Vec::new();
        for (what, samples, bound) in checks {
            let p99 = Percentiles::raw(samples, 1e-6).p99;
            match (p99, bound) {
                (Some(p99), Some(bound)) if p99 <= bound => {}
                (p99, bound) => misses.push(format!(
                    "{} {what} p99 {} ms > {} ms",
                    self.name(),
                    ms(p99),
                    ms(bound)
                )),
            }
        }
        misses
    }

    /// The distribution as `name: answer|first/lag/stream p50/p95/p99 ms xN`.
    fn render(&self) -> String {
        if self.stream {
            format!(
                "{}: first {} lag {} stream {} ms x{}",
                self.name(),
                client(&self.first_us),
                client(&self.lag_us),
                client(&self.completion_us),
                self.completion_us.len()
            )
        } else {
            format!(
                "{}: answer {} ms x{}",
                self.name(),
                client(&self.completion_us),
                self.completion_us.len()
            )
        }
    }
}

/// Every tenant/mode distribution of `load`'s answered `calls`, then each
/// mode's aggregate when several tenants sent.
pub fn distributions(load: Load, calls: &[Arrived]) -> Vec<Distribution> {
    let mut each: BTreeMap<(usize, bool), Distribution> = BTreeMap::new();
    let mut all: BTreeMap<bool, Distribution> = BTreeMap::new();
    for tenant in 0..load.tenants() {
        for stream in load.modes() {
            each.insert(
                (tenant, *stream),
                Distribution {
                    tenant: Some(tenant),
                    stream: *stream,
                    ..Distribution::default()
                },
            );
            all.entry(*stream).or_insert_with(|| Distribution {
                stream: *stream,
                ..Distribution::default()
            });
        }
    }
    for call in calls.iter().filter(|call| call.timed.outcome.answered()) {
        if let Some(distribution) = each.get_mut(&(call.tenant, call.stream)) {
            distribution.add(call);
        }
        if let Some(distribution) = all.get_mut(&call.stream) {
            distribution.add(call);
        }
    }
    let mut distributions: Vec<Distribution> = each.into_values().collect();
    if load.tenants() > 1 {
        distributions.extend(all.into_values());
    }
    distributions
}

/// The mean of each backlog series over the first and the last [`EDGE`]
/// of the window starting at `from` seconds: in-flight calls, the server's
/// active calls, and outbox items, as `(name, first, last)`; `None` when an
/// edge has no reading.
pub fn backlog_edges(measured: &Measured, from: f64) -> Option<Vec<(&'static str, f64, f64)>> {
    let edge = EDGE.as_secs_f64();
    let until = from + measured.seconds;
    let mean = |low: f64, high: f64, read: Series| {
        let values: Vec<f64> = measured
            .backlog
            .iter()
            .filter(|reading| reading.offset >= low && reading.offset < high)
            .map(read)
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let series: [(&'static str, Series); 3] = [
        ("in-flight", |reading| reading.in_flight),
        ("active", |reading| reading.active),
        ("outbox", |reading| reading.pending),
    ];
    series
        .iter()
        .map(|(name, read)| {
            Some((
                *name,
                mean(from, from + edge, *read)?,
                mean(until - edge, until + 1.0, *read)?,
            ))
        })
        .collect()
}

/// The window's start, seconds from its schedule's start: its earliest
/// scheduled call.
fn start(measured: &Measured) -> f64 {
    measured
        .calls
        .iter()
        .map(|call| call.offset.as_secs_f64())
        .fold(None, |earliest: Option<f64>, offset| {
            Some(earliest.map_or(offset, |earliest| earliest.min(offset)))
        })
        .unwrap_or(0.0)
        .floor()
}

/// Every reason the driver's evidence cannot judge `measured`: a missed or
/// capped arrival, send lateness p99 above [`LATENESS_LIMIT_US`], nothing
/// sent, a tenant/mode with fewer than [`MIN_CALLS`] completions, unmatched
/// upstream sends, or missing backlog readings.
pub fn evidence_misses(load: Load, measured: &Measured) -> Vec<String> {
    let driven = &measured.driven;
    let mut misses = Vec::new();
    if driven.missed > 0 {
        misses.push(format!("{} arrivals missed by the driver", driven.missed));
    }
    if driven.capped > 0 {
        misses.push(format!(
            "{} arrivals capped at the in-flight bound (server active calls peaked at {:.0})",
            driven.capped,
            measured
                .backlog
                .iter()
                .map(|reading| reading.active)
                .fold(0.0, f64::max)
        ));
    }
    match Percentiles::raw(&driven.lateness_us, 1.0).p99 {
        Some(p99) if p99 <= LATENESS_LIMIT_US as f64 => {}
        Some(p99) => misses.push(format!("send lateness p99 {:.1} ms > 5 ms", p99 / 1e3)),
        None => misses.push("nothing was sent".to_owned()),
    }
    for distribution in distributions(load, &measured.calls) {
        if distribution.completion_us.len() < MIN_CALLS {
            misses.push(format!(
                "{} completed {} < {MIN_CALLS} calls",
                distribution.name(),
                distribution.completion_us.len()
            ));
        }
        if distribution.tenant.is_some() && distribution.unmatched > 0 {
            misses.push(format!(
                "{} streams whose events did not pair with upstream sends ({})",
                distribution.unmatched,
                distribution.name()
            ));
        }
    }
    if backlog_edges(measured, start(measured)).is_none() {
        misses.push("backlog readings missing at an edge of the window".to_owned());
    }
    misses
}

/// Calls of `measured` that did not answer, by outcome key.
pub fn outcomes(measured: &Measured) -> BTreeMap<String, u64> {
    let mut outcomes = BTreeMap::new();
    for call in measured
        .calls
        .iter()
        .filter(|call| !call.timed.outcome.answered())
    {
        *outcomes
            .entry(outcome_key(&call.timed.outcome))
            .or_default() += 1;
    }
    outcomes
}

/// Every reason valid evidence is outside `slo`: any call that did not
/// answer, a tenant/mode or aggregate p99 above its bound, or backlog
/// growth above one second of offered work.
pub fn slo_misses(load: Load, measured: &Measured, slo: &Slo) -> Vec<String> {
    let mut misses: Vec<String> = outcomes(measured)
        .iter()
        .map(|(outcome, count)| format!("{count}x {outcome}"))
        .collect();
    for distribution in distributions(load, &measured.calls) {
        misses.extend(distribution.misses(slo));
    }
    misses.extend(growth_misses(measured));
    misses
}

/// Every backlog series that grew from the first to the last [`EDGE`] by
/// more than one second of offered work.
fn growth_misses(measured: &Measured) -> Vec<String> {
    backlog_edges(measured, start(measured))
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, first, last)| last - first > measured.rate)
        .map(|(name, first, last)| {
            format!(
                "{name} backlog grew {first:.0} -> {last:.0} (> {:.0}, 1 s of offered work)",
                measured.rate
            )
        })
        .collect()
}

/// Every reason `measured`'s captures and memory are not intact: per
/// [`integrity_misses`] against the server's admitted calls, durable rows
/// outside a tenant's own answered-to-admissible calls, rows not summing to
/// the staged captures, or peak memory within [`MEMORY_HEADROOM_BYTES`] of
/// `memory_limit`.
pub fn capture_misses(measured: &Measured, memory_limit: u64) -> Vec<String> {
    let mut misses = Vec::new();
    if !measured.admitted.is_finite() || measured.admitted < 0.0 {
        misses.push("admitted calls are not evidence".to_owned());
    }
    misses.extend(integrity_misses(
        &measured.integrity,
        measured.admitted.to_u64().unwrap_or(0),
    ));
    let mut rows = 0;
    for (tenant, owned) in measured.ownership.iter().enumerate() {
        rows += owned.rows;
        let answered = i64::try_from(owned.answered).unwrap_or(i64::MAX);
        let admissible = i64::try_from(owned.admissible).unwrap_or(i64::MAX);
        if owned.rows < answered || owned.rows > admissible {
            misses.push(format!(
                "{} reads {} capture rows for {answered}..={admissible} of its calls",
                TENANTS[tenant], owned.rows
            ));
        }
    }
    if measured.ownership.is_empty() {
        misses.push("no tenant capture reads".to_owned());
    } else if (rows as f64 - measured.integrity.staged).abs() > 0.5 {
        misses.push(format!(
            "tenants read {rows} rows for {} staged captures",
            measured.integrity.staged
        ));
    }
    let peak = measured
        .traffic
        .peak_memory_bytes
        .max(measured.drained.peak_memory_bytes);
    if peak + MEMORY_HEADROOM_BYTES >= memory_limit {
        misses.push(format!(
            "peak memory >= {} GiB",
            memory_limit.saturating_sub(MEMORY_HEADROOM_BYTES) >> 30
        ));
    }
    misses
}

/// A rate row's verdict and its misses: INVALID on evidence misses,
/// OUTSIDE-SLO on SLO misses, else SUSTAINABLE.
pub fn classify(load: Load, measured: &Measured, slo: &Slo) -> (Verdict, Vec<String>) {
    let invalid = evidence_misses(load, measured);
    if !invalid.is_empty() {
        return (Verdict::Invalid, invalid);
    }
    let outside = slo_misses(load, measured, slo);
    if outside.is_empty() {
        (Verdict::Sustainable, outside)
    } else {
        (Verdict::OutsideSlo, outside)
    }
}

/// The overload row's misses and whether saturation was observed. Only
/// [`ADMISSION_REFUSALS`] may end a call unanswered, the driver's evidence
/// must hold (sample sufficiency aside), and captures and memory must be
/// intact; saturation is a refusal or backlog growth above one second of
/// offered work.
pub fn overload_misses(measured: &Measured, memory_limit: u64) -> (Vec<String>, bool) {
    let driven = &measured.driven;
    let mut misses = Vec::new();
    if driven.missed + driven.capped > 0 {
        misses.push(format!(
            "{} missed and {} capped arrivals",
            driven.missed, driven.capped
        ));
    }
    if Percentiles::raw(&driven.lateness_us, 1.0)
        .p99
        .is_none_or(|p99| p99 > LATENESS_LIMIT_US as f64)
    {
        misses.push("send lateness p99 above 5 ms or nothing sent".to_owned());
    }
    let outcomes = outcomes(measured);
    let mut refused = 0;
    for (outcome, count) in &outcomes {
        if ADMISSION_REFUSALS.contains(&outcome.as_str()) {
            refused += count;
        } else {
            misses.push(format!("{count}x {outcome}"));
        }
    }
    misses.extend(capture_misses(measured, memory_limit));
    let saturated = refused > 0 || !growth_misses(measured).is_empty();
    (misses, saturated)
}

/// The recovery row's misses: valid evidence (with [`MIN_CALLS`] per
/// tenant/mode), every SLO with zero refusals, bounded backlog, and intact
/// captures and memory.
pub fn recovery_misses(
    load: Load,
    measured: &Measured,
    slo: &Slo,
    memory_limit: u64,
) -> Vec<String> {
    let mut misses = evidence_misses(load, measured);
    misses.extend(slo_misses(load, measured, slo));
    misses.extend(capture_misses(measured, memory_limit));
    misses
}

/// The envelope sentence of `rates`, each tested rate's verdict in run
/// order, and whether a sustainable rate was found.
pub fn envelope(load: Load, rates: &[(f64, Verdict)]) -> (String, bool) {
    let highest = rates
        .iter()
        .filter(|(_, verdict)| *verdict == Verdict::Sustainable)
        .map(|(rate, _)| *rate)
        .fold(None, |high: Option<f64>, rate| {
            Some(high.map_or(rate, |high| high.max(rate)))
        });
    let failing = rates
        .iter()
        .find(|(_, verdict)| *verdict != Verdict::Sustainable)
        .map(|(rate, verdict)| format!("{rate}/s ({verdict:?})"));
    let text = match (highest, failing) {
        (None, _) => format!("{}: no measured sustainable rate", load.name()),
        (Some(high), None) => format!(
            "{}: sustainable at {high}/s, the highest tested rate: a lower bound, ceiling unlocated",
            load.name()
        ),
        (Some(high), Some(failing)) => format!(
            "{}: highest sustainable tested rate {high}/s; first failing rate {failing}",
            load.name()
        ),
    };
    (text, highest.is_some())
}

/// Shared row columns of a capacity window.
fn cells(measured: &Measured) -> String {
    let driven = &measured.driven;
    let seconds = measured.traffic.seconds.max(f64::EPSILON);
    let errors = outcomes(measured);
    let errors = if errors.is_empty() {
        "0".to_owned()
    } else {
        errors
            .iter()
            .map(|(outcome, count)| format!("{count}x {outcome}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{}/s | {}/{}/{}/{} | {} | {errors} | {:.2} cores | {:.2} GiB | {:.2} cores",
        measured.rate,
        driven.scheduled,
        driven.sent,
        driven.missed,
        driven.capped,
        measured
            .calls
            .iter()
            .filter(|call| call.timed.outcome.answered())
            .count(),
        measured.traffic.server_cpu_seconds / seconds,
        gib(measured.traffic.peak_memory_bytes),
        measured.traffic.driver_cpu_seconds / seconds,
    )
}

/// The evidence column of a capacity window: every distribution, driver
/// lateness and peak in flight, backlog edges, server counts, diagnostic
/// server overhead, capture integrity and ownership, and profile gaps.
fn evidence(load: Load, measured: &Measured) -> String {
    let mut parts: Vec<String> = distributions(load, &measured.calls)
        .iter()
        .map(Distribution::render)
        .collect();
    parts.push(format!(
        "send lateness p50/p95/p99 {} ms, peak in flight {}",
        client(&measured.driven.lateness_us),
        measured.driven.peak_in_flight
    ));
    if let Some(edges) = backlog_edges(measured, start(measured)) {
        parts.push(
            edges
                .iter()
                .map(|(name, first, last)| format!("{name} {first:.1} -> {last:.1}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    let refused = measured
        .calls
        .iter()
        .filter(|call| call.timed.outcome.refused())
        .count();
    parts.push(format!(
        "server arrived {}, refused {refused}, admitted {}",
        measured.server_arrived, measured.admitted
    ));
    for point in [BUFFERED_ANSWER, STREAM_END] {
        let overhead = measured.telemetry.histogram(point.overhead);
        if overhead.count > 0.0 {
            parts.push(format!(
                "diagnostic {} overhead {}",
                point.name,
                summary(&overhead)
            ));
        }
    }
    let integrity = &measured.integrity;
    parts.push(format!(
        "captures staged {}/{} admitted, dropped {}, lost {}, scribe writes {}",
        integrity.staged, measured.admitted, integrity.dropped, integrity.lost, integrity.written
    ));
    parts.extend(
        measured
            .ownership
            .iter()
            .enumerate()
            .map(|(tenant, owned)| format!("{} capture rows {}", TENANTS[tenant], owned.rows)),
    );
    if let Some(gaps) = &measured.profile_gaps {
        parts.push(if gaps.is_empty() {
            "profile complete".to_owned()
        } else {
            format!("profile INCOMPLETE: {}", gaps.join(", "))
        });
    }
    parts.join("; ")
}

/// Records `measured`'s raw per-call samples in the report.
fn samples(report: &mut Report, load: Load, measured: &Measured) {
    let calls: Vec<serde_json::Value> = measured
        .calls
        .iter()
        .map(|call| {
            let since =
                |at: std::time::Instant| micros(at.saturating_duration_since(call.scheduled));
            serde_json::json!({
                "tenant": TENANTS[call.tenant],
                "stream": call.stream,
                "offset_us": micros(call.offset),
                "send_late_us": since(call.timed.sent),
                "headers_us": call.timed.headers.map(since),
                "first_event_us": call.timed.events.first().copied().map(since),
                "done_us": since(call.timed.done),
                "lag_us": call.timed.events.iter().zip(&call.sends)
                    .map(|(received, sent)| micros(received.saturating_duration_since(*sent)))
                    .collect::<Vec<_>>(),
                "outcome": outcome_key(&call.timed.outcome),
            })
        })
        .collect();
    report.sample(&serde_json::json!({
        "step": measured.name,
        "case": load.name(),
        "rate": measured.rate,
        "profiled": measured.profile_gaps.is_some(),
        "scheduled": measured.driven.scheduled,
        "sent": measured.driven.sent,
        "missed": measured.driven.missed,
        "capped": measured.driven.capped,
        "lateness_us": measured.driven.lateness_us,
        "backlog": measured.backlog.iter().map(|reading| serde_json::json!({
            "offset": reading.offset,
            "in_flight": reading.in_flight,
            "active": reading.active,
            "pending": reading.pending,
        })).collect::<Vec<_>>(),
        "calls": calls,
    }));
}

impl Report {
    /// Adds a rate row classified by [`classify`], its integrity row, and
    /// its drain row, and returns the rate's verdict.
    pub fn rate(&mut self, load: Load, measured: &Measured, slo: &Slo) -> Verdict {
        samples(self, load, measured);
        let (verdict, misses) = classify(load, measured, slo);
        self.push(Row::new(
            measured.name.clone(),
            cells(measured),
            verdict,
            &misses,
            evidence(load, measured),
        ));
        self.integrity(measured);
        verdict
    }

    /// Adds the overload row per [`overload_misses`], with a note when no
    /// saturation was observed, then the recovery row per
    /// [`recovery_misses`] and the sequence's integrity and drain rows.
    pub fn overload(&mut self, load: Load, overload: &Measured, recovery: &Measured, slo: &Slo) {
        let limit = self.memory_limit_bytes();
        samples(self, load, overload);
        samples(self, load, recovery);
        let (misses, saturated) = overload_misses(overload, limit);
        let observed = if saturated {
            "refusal or saturation observed".to_owned()
        } else {
            "no refusal or saturation observed: overload safety not demonstrated".to_owned()
        };
        self.push(Row::judged(
            overload.name.clone(),
            cells(overload),
            &misses,
            format!("{observed}; {}", evidence(load, overload)),
        ));
        self.push(Row::judged(
            recovery.name.clone(),
            cells(recovery),
            &recovery_misses(load, recovery, slo, limit),
            evidence(load, recovery),
        ));
        self.integrity(recovery);
    }

    /// Adds `measured`'s capture integrity row and its drain row.
    fn integrity(&mut self, measured: &Measured) {
        let blank = self.blank_cells();
        self.push(Row::judged(
            format!("{} integrity", measured.name),
            blank.clone(),
            &capture_misses(measured, self.memory_limit_bytes()),
            format!(
                "captures staged {}/{} admitted, dropped {}, lost {}",
                measured.integrity.staged,
                measured.admitted,
                measured.integrity.dropped,
                measured.integrity.lost
            ),
        ));
        let drain = measured.drain.map_or_else(
            || vec!["did not drain within 60 s".to_owned()],
            |_| Vec::new(),
        );
        self.push(Row::judged(
            format!("{} drain", measured.name),
            blank,
            &drain,
            format!(
                "drain {}, {:.2} cores",
                measured
                    .drain
                    .map_or("timed out".to_owned(), |seconds| format!("{seconds:.1} s")),
                measured.drained.server_cpu_seconds / measured.drained.seconds.max(f64::EPSILON)
            ),
        ));
    }
}

/// Milliseconds of `seconds`, or `-`.
fn ms(seconds: Option<f64>) -> String {
    seconds.map_or("-".to_owned(), |seconds| format!("{:.1}", seconds * 1e3))
}
