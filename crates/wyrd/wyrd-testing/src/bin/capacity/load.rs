//! The open-loop load driver: arrivals at fixed instants, independent of
//! responses, through the public client.
//!
//! A [`Lane`] is one tenant's stream of one [`Work`] at one rate. Every
//! arrival is scheduled from the step start; it starts only when one of
//! [`MAX_IN_FLIGHT`] driver permits is free, and otherwise counts as missed,
//! so driver saturation shows as lost traffic instead of silently lowering
//! the offered rate. Requests rotate across the deployment's replicas,
//! standing in for its gateway.
//!
//! - A direct arrival is one `POST /v1/verification/execute` call whose
//!   verdict is checked against the input it sent.
//! - A queued Drift arrival is one manual `POST /v1/verification/runs` over the tenant's
//!   seeded window; a queued Eval arrival is one observation whose binding
//!   activates the run.
//! - An ingest arrival is one Drift observation of [`INGEST_FEATURES`]
//!   features admitted to the tenant's `WyrdState` queue.
//! - A query arrival is one Oracle SQL statement over the tenant's last
//!   [`QUERY_WINDOW`] of ingested Drift rows.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::Instant;
use wyrd_client::observe::EvalObservationOptions;
use wyrd_client::state::WyrdState;
use wyrd_client::{Bifrost, QueueConfig, WyrdClient};
use wyrd_spec::verification::{
    ExecuteVerificationRequest, Judgment, StartVerificationRunRequest,
    StartVerificationRunResponse, VerificationRunInput, VerificationRunTarget, VerificationVerdict,
};

use crate::Result;
use crate::fixture::{Kind, Tenant, context};

/// Driver permits: the most requests the driver keeps in flight at once.
pub const MAX_IN_FLIGHT: usize = 2_048;

/// Every this-many arrivals of a verification lane, the input fails its
/// judgment; that failed verdict is a correct judgment, not an error.
const FAIL_EVERY: u64 = 20;

/// Numeric features, and so durable rows, of one ingested observation
/// (REQ-171, AC-041).
pub const INGEST_FEATURES: usize = 100;

/// How far back an Oracle query reads (REQ-171).
const QUERY_WINDOW: chrono::TimeDelta = chrono::TimeDelta::minutes(5);

/// One golden-signal operation of the REQ-171 mix; the report judges each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub enum Op {
    /// Synchronous supplied-input verification.
    Direct,
    /// Durable verification runs.
    Queued,
    /// Drift observations through the client queue into Scribe.
    Ingest,
    /// Oracle SQL reads.
    Query,
}

impl Op {
    /// Every operation, in report order.
    pub const ALL: [Self; 4] = [Self::Direct, Self::Queued, Self::Ingest, Self::Query];

    /// The report name of this operation.
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "verification direct",
            Self::Queued => "verification queued",
            Self::Ingest => "scribe ingest",
            Self::Query => "oracle query",
        }
    }
}

/// What one lane repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Direct execution of one verifier kind.
    Direct(Kind),
    /// Queued execution of one verifier kind.
    Queued(Kind),
    /// One Drift observation of [`INGEST_FEATURES`] features.
    Ingest,
    /// The latest rows of one series: a selective lookup.
    Lookup,
    /// Row count and mean per series: a small aggregate.
    Aggregate,
}

impl Work {
    /// The operation this work counts toward.
    pub fn op(self) -> Op {
        match self {
            Self::Direct(_) => Op::Direct,
            Self::Queued(_) => Op::Queued,
            Self::Ingest => Op::Ingest,
            Self::Lookup | Self::Aggregate => Op::Query,
        }
    }
}

/// One arrival stream.
#[derive(Debug, Clone, Copy)]
pub struct Lane {
    /// Index of the tenant in the benchmark's tenant list.
    pub tenant: usize,
    /// The work.
    pub work: Work,
    /// Offered arrivals per second.
    pub rate: f64,
}

/// What one lane did during one step.
#[derive(Debug, Default, Clone)]
pub struct Tally {
    /// Arrivals scheduled inside the step.
    pub offered: u64,
    /// Arrivals that found no driver permit and were never sent.
    pub missed: u64,
    /// Requests sent.
    pub started: u64,
    /// Requests the server or client queue accepted: a judgment, an enqueued
    /// run, an admitted observation, or a query answer.
    pub accepted: u64,
    /// Refused or failed requests by stable error code.
    pub rejected: BTreeMap<String, u64>,
    /// Direct judgments that differ from the verdict the input implies.
    pub wrong_verdicts: u64,
    /// Raw client latency per sent request, microseconds: the judgment for
    /// direct, the enqueue acknowledgement or queue admission for queued and
    /// ingest, the whole answer for a query.
    pub latency_us: Vec<u64>,
    /// When each accepted request returned, from the step start.
    pub completed_at: Vec<Duration>,
    /// For an ingest lane, how long the tenant's client queues took to drain
    /// once its arrivals stopped.
    pub drain: Option<Duration>,
}

/// The public clients one tenant's lanes use, one of each per replica.
pub struct TenantClients {
    /// Administrator clients calling the Verification routes, with the
    /// direct-execution deadline.
    verification: Vec<WyrdClient>,
    /// Service lifetimes emitting observations, with the default queue.
    states: Vec<WyrdState>,
    /// Administrator Oracle query handles.
    oracle: Vec<Bifrost>,
}

impl TenantClients {
    /// Connects `tenant` to every replica in `urls`. Each administrator
    /// client exchanges its token here, so no step opens with every tenant
    /// authenticating at once.
    ///
    /// # Errors
    ///
    /// Returns a client, token exchange, bundle, or Bifrost start failure.
    ///
    /// # Cancellation
    ///
    /// Service lifetimes already started are dropped with nothing queued;
    /// exchanged tokens simply expire. Nothing durable is written, so a
    /// retry is safe.
    pub async fn connect(tenant: &Tenant, urls: &[String]) -> Result<Self> {
        let mut verification = Vec::new();
        let mut states = Vec::new();
        let mut oracle = Vec::new();
        for url in urls {
            let admin = tenant.admin(url)?;
            admin
                .auth()
                .bearer()
                .await
                .map_err(|error| format!("exchanging the administrator token: {error}"))?;
            oracle.push(Bifrost::query_only(&admin));
            let executor = tenant.executor(url)?;
            executor
                .auth()
                .bearer()
                .await
                .map_err(|error| format!("exchanging the executor token: {error}"))?;
            verification.push(executor);
            let state = WyrdState::from_path_with_client(&tenant.bundle, tenant.service(url)?)?;
            state
                .start_bifrost_with_config(None, QueueConfig::default())
                .await?;
            states.push(state);
        }
        Ok(Self {
            verification,
            states,
            oracle,
        })
    }

    /// Hands every queued observation to the server.
    ///
    /// # Errors
    ///
    /// Returns the first flush failure.
    ///
    /// # Cancellation
    ///
    /// Batches already handed to the server are durable; the rest stay
    /// queued in their Service lifetime for a later flush or shutdown.
    pub async fn flush(&self) -> Result<()> {
        for state in &self.states {
            state.flush().await?;
        }
        Ok(())
    }

    /// Drains and stops every Service lifetime.
    ///
    /// # Errors
    ///
    /// Returns the first shutdown failure.
    ///
    /// # Cancellation
    ///
    /// Lifetimes already stopped are drained and durable; observations of
    /// the rest are lost when the clients drop.
    pub async fn shutdown(&self) -> Result<()> {
        for state in &self.states {
            state.shutdown().await?;
        }
        Ok(())
    }
}

/// One prepared request a lane repeats.
#[derive(Clone)]
enum Request {
    /// Direct execution: the passing and the failing input.
    Execute(Arc<[ExecuteVerificationRequest; 2]>),
    /// Manual Drift run over the seeded window.
    Start(Arc<StartVerificationRunRequest>),
    /// Eval observation from the component `alias`.
    Observe(&'static str),
    /// Drift observation of the Model.
    Ingest,
    /// Oracle read of one shape.
    Query(Work),
}

/// How one request ended.
enum Outcome {
    /// Accepted; for a direct judgment, whether it is the verdict the input
    /// implies.
    Accepted { right: bool },
    /// Refused with a stable error code.
    Rejected(String),
}

impl Lane {
    /// Drives this lane from `start` for `window`, sending through `clients`
    /// with permits from `permits`, and returns its tally once every sent
    /// request has ended. An ingest lane then times the tenant's queue
    /// drain.
    ///
    /// # Errors
    ///
    /// Returns an error when the tenant does not run this lane's workload, a
    /// request task panicked, or the drain flush fails.
    ///
    /// # Cancellation
    ///
    /// Dropping it aborts every in-flight request task and discards the
    /// tally. Requests that reached a replica keep their effects: enqueued
    /// runs, admitted observations, and audit rows remain durable, and
    /// observations admitted to the client queue are flushed by its owner's
    /// later flush or shutdown. Arrivals are never resent.
    pub async fn drive(
        self,
        tenant: &Tenant,
        clients: Arc<TenantClients>,
        permits: Arc<Semaphore>,
        start: Instant,
        window: Duration,
    ) -> Result<Tally> {
        let request = self.request(tenant)?;
        let mut tally = Tally::default();
        let mut running = JoinSet::new();
        let replicas = clients.verification.len().max(1) as u64;
        let end = start + window;
        let mut sequence = 0_u64;
        loop {
            let due = start + Duration::from_secs_f64(sequence as f64 / self.rate);
            if due >= end {
                break;
            }
            tokio::time::sleep_until(due).await;
            tally.offered += 1;
            let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                tally.missed += 1;
                sequence += 1;
                continue;
            };
            tally.started += 1;
            let replica = usize::try_from(sequence % replicas).expect(
                "invariant: the replica index is below the replica count, which fits usize",
            );
            let (clients, request) = (Arc::clone(&clients), request.clone());
            running.spawn(async move {
                let sent = Instant::now();
                let outcome = request.send(&clients, replica, sequence).await;
                drop(permit);
                (sent.elapsed(), sent.duration_since(start), outcome)
            });
            sequence += 1;
        }
        while let Some(ended) = running.join_next().await {
            let (elapsed, sent, outcome) = ended?;
            tally
                .latency_us
                .push(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX));
            match outcome {
                Outcome::Accepted { right } => {
                    tally.accepted += 1;
                    tally.wrong_verdicts += u64::from(!right);
                    tally.completed_at.push(sent + elapsed);
                }
                Outcome::Rejected(code) => {
                    *tally.rejected.entry(code).or_default() += 1;
                }
            }
        }
        if self.work == Work::Ingest {
            let draining = Instant::now();
            clients.flush().await?;
            tally.drain = Some(draining.elapsed());
        }
        Ok(tally)
    }

    /// The request this lane repeats against `tenant`.
    ///
    /// # Errors
    ///
    /// Returns an error when `tenant` does not run the workload or has no
    /// seeded window for a queued Drift lane.
    fn request(&self, tenant: &Tenant) -> Result<Request> {
        Ok(match self.work {
            Work::Direct(kind) => {
                let (verifier, subject) = tenant.target(kind)?;
                let build = |failing| ExecuteVerificationRequest {
                    verifier_uid: verifier.clone(),
                    subject_card_uid: subject.clone(),
                    input: kind.input(0, failing),
                    run_id: None,
                };
                Request::Execute(Arc::new([build(false), build(true)]))
            }
            Work::Queued(kind) if kind.is_drift() => {
                let (verifier, subject) = tenant.target(kind)?;
                let (start, end) = tenant.window.ok_or("a queued Drift lane needs a window")?;
                Request::Start(Arc::new(StartVerificationRunRequest {
                    target: VerificationRunTarget::Verifier {
                        verifier_uid: verifier.clone(),
                        subject_card_uid: subject.clone(),
                    },
                    input: VerificationRunInput::DriftWindow { start, end },
                }))
            }
            Work::Queued(kind) => Request::Observe(kind.agent_alias()),
            Work::Ingest => Request::Ingest,
            Work::Lookup | Work::Aggregate => Request::Query(self.work),
        })
    }
}

impl Request {
    /// Sends arrival `sequence` to `replica`.
    ///
    /// # Cancellation
    ///
    /// A request dropped after it reached the replica may still take
    /// effect there (a judgment, an enqueued run, or audit rows); an
    /// observation is admitted to the client queue synchronously, so it is
    /// either admitted or not. The driver never resends an arrival.
    async fn send(&self, clients: &TenantClients, replica: usize, sequence: u64) -> Outcome {
        let failing = sequence % FAIL_EVERY == FAIL_EVERY - 1;
        let accepted = Outcome::Accepted { right: true };
        match self {
            Self::Execute(inputs) => {
                match clients.verification[replica]
                    .request_json::<_, Judgment>(
                        Method::POST,
                        "/v1/verification/execute",
                        Some(&inputs[usize::from(failing)]),
                    )
                    .await
                {
                    Ok(response) => {
                        let expected = if failing {
                            VerificationVerdict::Failed
                        } else {
                            VerificationVerdict::Passed
                        };
                        Outcome::Accepted {
                            right: response.verdict == expected,
                        }
                    }
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
            Self::Start(request) => {
                match clients.verification[replica]
                    .submit_idempotent::<_, StartVerificationRunResponse>(
                        Method::POST,
                        "/v1/verification/runs",
                        &**request,
                    )
                    .await
                {
                    Ok(_) => accepted,
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
            Self::Observe(alias) => {
                let kind = if *alias == "judge" {
                    Kind::Judge
                } else {
                    Kind::Assertion
                };
                let emitted = clients.states[replica]
                    .run()
                    .for_card(alias)
                    .and_then(|view| {
                        view.observe()
                            .eval(&context(kind, failing), EvalObservationOptions::default())
                    });
                match emitted {
                    Ok(()) => accepted,
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
            Self::Ingest => {
                let features: serde_json::Map<String, serde_json::Value> = (0..INGEST_FEATURES)
                    .map(|feature| {
                        let value = (sequence + feature as u64 * 7) % 100;
                        (format!("f{feature}"), (value as f64).into())
                    })
                    .collect();
                let emitted = clients.states[replica]
                    .run()
                    .for_card("model")
                    .and_then(|view| {
                        view.observe()
                            .drift(&serde_json::Value::Object(features), None)
                    });
                match emitted {
                    Ok(()) => accepted,
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
            Self::Query(shape) => {
                match clients.oracle[replica]
                    .sql(&query(*shape, sequence, chrono::Utc::now()), &[])
                    .await
                {
                    Ok(_) => accepted,
                    Err(error) => Outcome::Rejected(
                        wyrd_spec::error::WyrdError::from(&error).code().to_owned(),
                    ),
                }
            }
        }
    }
}

/// The SQL of query `shape` for arrival `sequence` at `now`: both read only
/// the last [`QUERY_WINDOW`] of the tenant's Drift rows. A lookup reads the
/// ten newest rows of one series, rotating through the ingested features; an
/// aggregate counts and averages every series.
fn query(shape: Work, sequence: u64, now: chrono::DateTime<chrono::Utc>) -> String {
    let since = (now - QUERY_WINDOW).format("%Y-%m-%d %H:%M:%S%.6f");
    if shape == Work::Lookup {
        format!(
            "SELECT record_id, num_value FROM vala.drift.observations \
             WHERE series = 'f{}' AND wyrd_event_time >= TIMESTAMP '{since}' \
             ORDER BY wyrd_event_time DESC LIMIT 10",
            sequence % INGEST_FEATURES as u64
        )
    } else {
        format!(
            "SELECT series, COUNT(*) AS n, AVG(num_value) AS mean \
             FROM vala.drift.observations \
             WHERE wyrd_event_time >= TIMESTAMP '{since}' GROUP BY series"
        )
    }
}

/// The REQ-171 mix at level `level`, spread evenly over `tenants` identical
/// tenants: direct and queued verification at `level / 2` each across every
/// [`Kind`] equally, ingest at `2.5 · level` observations, and queries at
/// `level / 2` split between [`Work::Lookup`] and [`Work::Aggregate`].
pub fn mix(level: f64, tenants: usize) -> Vec<Lane> {
    let share = level / tenants as f64;
    let verify = share / 2.0 / Kind::ALL.len() as f64;
    (0..tenants)
        .flat_map(|tenant| {
            let kinds = Kind::ALL.into_iter().flat_map(move |kind| {
                [Work::Direct(kind), Work::Queued(kind)].map(|work| Lane {
                    tenant,
                    work,
                    rate: verify,
                })
            });
            kinds.chain([
                Lane {
                    tenant,
                    work: Work::Ingest,
                    rate: 2.5 * share,
                },
                Lane {
                    tenant,
                    work: Work::Lookup,
                    rate: share / 4.0,
                },
                Lane {
                    tenant,
                    work: Work::Aggregate,
                    rate: share / 4.0,
                },
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Op, Work, mix, query};

    /// The mix offers exactly the REQ-171 rates per operation at a level,
    /// spread evenly over the tenants.
    ///
    /// # Panics
    ///
    /// Panics when a rate is wrong.
    #[test]
    fn mix_offers_the_required_rates() {
        let lanes = mix(200.0, 4);
        let rate = |op: Op| -> f64 {
            lanes
                .iter()
                .filter(|lane| lane.work.op() == op)
                .map(|lane| lane.rate)
                .sum()
        };
        assert!((rate(Op::Direct) - 100.0).abs() < 1e-9);
        assert!((rate(Op::Queued) - 100.0).abs() < 1e-9);
        assert!((rate(Op::Ingest) - 500.0).abs() < 1e-9);
        assert!((rate(Op::Query) - 100.0).abs() < 1e-9);
        for tenant in 0..4 {
            let per_tenant: f64 = lanes
                .iter()
                .filter(|lane| lane.tenant == tenant)
                .map(|lane| lane.rate)
                .sum();
            assert!((per_tenant - 200.0).abs() < 1e-9);
        }
    }

    /// Both query shapes read only the last five minutes; a lookup names one
    /// ingested series.
    ///
    /// # Panics
    ///
    /// Panics when the SQL is wrong.
    #[test]
    fn queries_read_the_last_five_minutes() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-03T12:00:00Z")
            .map(|time| time.to_utc())
            .unwrap_or_default();
        let lookup = query(Work::Lookup, 142, now);
        assert!(lookup.contains("series = 'f42'"));
        assert!(lookup.contains("TIMESTAMP '2026-10-03 11:55:00.000000'"));
        let aggregate = query(Work::Aggregate, 0, now);
        assert!(aggregate.contains("GROUP BY series"));
        assert!(aggregate.contains("TIMESTAMP '2026-10-03 11:55:00.000000'"));
    }
}
