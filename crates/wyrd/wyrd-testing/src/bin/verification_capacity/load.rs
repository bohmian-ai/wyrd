//! The open-loop load driver: arrivals at fixed instants, independent of
//! responses, through the public client.
//!
//! A [`Lane`] is one tenant's stream of one workload on one path at one rate.
//! Every arrival is scheduled from the step start; it starts only when one of
//! [`MAX_IN_FLIGHT`] driver permits is free, and otherwise counts as missed,
//! so driver saturation is reported instead of silently lowering the offered
//! rate. A direct arrival is one `verification.execute` call whose verdict is
//! checked against the input it sent; a queued Drift arrival is one manual
//! `start_run` over the tenant's seeded window, labelled `manual`; a queued
//! Eval arrival is one observation whose binding activates the run. Requests
//! rotate across the deployment's replicas, standing in for its gateway.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::Instant;
use wyrd_client::observe::EvalObservationOptions;
use wyrd_client::state::WyrdState;
use wyrd_client::{QueueConfig, Verification};
use wyrd_spec::verification::{
    ExecuteVerificationRequest, StartVerificationRunRequest, VerificationRunInput,
    VerificationRunTarget, VerificationVerdict,
};

use crate::Result;
use crate::fixture::{Kind, Tenant, context};

/// Driver permits: the most requests the driver keeps in flight at once.
pub const MAX_IN_FLIGHT: usize = 2_048;

/// Every this-many arrivals of a lane, the input fails its judgment.
const FAIL_EVERY: u64 = 20;

/// Execution path of a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    /// Durable verification run, judged by the queued runtime.
    Queued,
    /// Synchronous supplied-input execution.
    Direct,
}

impl Mode {
    /// The server's closed `mode` metric label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Direct => "direct",
        }
    }
}

/// One arrival stream.
#[derive(Debug, Clone, Copy)]
pub struct Lane {
    /// Index of the tenant in the benchmark's tenant list.
    pub tenant: usize,
    /// The workload.
    pub kind: Kind,
    /// The path.
    pub mode: Mode,
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
    /// Requests the server accepted: a judgment, an enqueued run, or an
    /// observation handed to the client queue.
    pub accepted: u64,
    /// Accepted requests whose input was built to fail its judgment.
    pub failing_accepted: u64,
    /// Refused or failed requests by stable error code.
    pub rejected: BTreeMap<String, u64>,
    /// Direct judgments by verdict.
    pub verdicts: BTreeMap<String, u64>,
    /// Direct judgments that differ from the verdict the input implies.
    pub wrong_verdicts: u64,
    /// Raw client latency per sent request, microseconds: the judgment for
    /// direct, the enqueue acknowledgement or client-queue hand-off for
    /// queued.
    pub latency_us: Vec<u64>,
    /// When each accepted direct judgment returned, from the step start.
    pub completed_at: Vec<Duration>,
}

/// The public clients one tenant's lanes use, one of each per replica.
pub struct TenantClients {
    /// Administrator Verification handles.
    verification: Vec<Verification>,
    /// Service lifetimes emitting Eval observations.
    states: Vec<WyrdState>,
}

impl TenantClients {
    /// Connects `tenant` to every replica in `urls`; Service lifetimes start
    /// only when `observes`.
    ///
    /// # Errors
    ///
    /// Returns a client, bundle, or Bifrost start failure.
    pub async fn connect(tenant: &Tenant, urls: &[String], observes: bool) -> Result<Self> {
        let mut verification = Vec::new();
        let mut states = Vec::new();
        for url in urls {
            verification.push(Verification::with_client(tenant.admin(url)?));
            if observes {
                let state = WyrdState::from_path(&tenant.bundle)?;
                state
                    .start_bifrost_with_config(&tenant.service(url)?, None, QueueConfig::default())
                    .await?;
                states.push(state);
            }
        }
        Ok(Self {
            verification,
            states,
        })
    }

    /// Hands every queued observation to the server.
    ///
    /// # Errors
    ///
    /// Returns the first flush failure.
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
}

/// How one request ended.
enum Outcome {
    /// Accepted; for a direct judgment, with its verdict and whether it is
    /// the verdict the input implies.
    Accepted(Option<(VerificationVerdict, bool)>),
    /// Refused with a stable error code.
    Rejected(String),
}

impl Lane {
    /// Drives this lane from `start` for `window`, sending through `clients`
    /// with permits from `permits`, and returns its tally once every sent
    /// request has ended.
    ///
    /// # Errors
    ///
    /// Returns an error when the tenant does not run this lane's workload or
    /// a request task panicked.
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
            let failing = sequence % FAIL_EVERY == FAIL_EVERY - 1;
            let replica = (sequence % replicas) as usize;
            let (clients, request) = (Arc::clone(&clients), request.clone());
            running.spawn(async move {
                let sent = Instant::now();
                let outcome = request.send(&clients, replica, failing).await;
                drop(permit);
                (sent.elapsed(), sent.duration_since(start), failing, outcome)
            });
            sequence += 1;
        }
        while let Some(ended) = running.join_next().await {
            let (elapsed, sent, failing, outcome) = ended?;
            tally
                .latency_us
                .push(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX));
            match outcome {
                Outcome::Accepted(verdict) => {
                    tally.accepted += 1;
                    tally.failing_accepted += u64::from(failing);
                    if let Some((verdict, expected)) = verdict {
                        let name: &'static str = verdict.into();
                        *tally.verdicts.entry(name.to_owned()).or_default() += 1;
                        tally.wrong_verdicts += u64::from(!expected);
                        tally.completed_at.push(sent + elapsed);
                    }
                }
                Outcome::Rejected(code) => {
                    *tally.rejected.entry(code).or_default() += 1;
                }
            }
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
        let (verifier, subject) = tenant.target(self.kind)?;
        Ok(match (self.mode, self.kind.is_drift()) {
            (Mode::Direct, _) => {
                let build = |failing| ExecuteVerificationRequest {
                    verifier_uid: verifier.clone(),
                    subject_card_uid: subject.clone(),
                    input: self.kind.input(0, failing),
                };
                Request::Execute(Arc::new([build(false), build(true)]))
            }
            (Mode::Queued, true) => {
                let (start, end) = tenant.window.ok_or("a queued Drift lane needs a window")?;
                Request::Start(Arc::new(StartVerificationRunRequest {
                    target: VerificationRunTarget::Verifier {
                        verifier_uid: verifier.clone(),
                        subject_card_uid: subject.clone(),
                    },
                    input: VerificationRunInput::DriftWindow { start, end },
                }))
            }
            (Mode::Queued, false) => Request::Observe(self.kind.agent_alias()),
        })
    }
}

impl Request {
    /// Sends one request to `replica`, with the failing input when `failing`.
    async fn send(&self, clients: &TenantClients, replica: usize, failing: bool) -> Outcome {
        match self {
            Self::Execute(inputs) => {
                match clients.verification[replica]
                    .execute(&inputs[usize::from(failing)])
                    .await
                {
                    Ok(response) => {
                        let expected = if failing {
                            VerificationVerdict::Failed
                        } else {
                            VerificationVerdict::Passed
                        };
                        Outcome::Accepted(Some((response.verdict, response.verdict == expected)))
                    }
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
            Self::Start(request) => {
                match clients.verification[replica].start_run(request, None).await {
                    Ok(_) => Outcome::Accepted(None),
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
            Self::Observe(alias) => {
                let Some(state) = clients.states.get(replica) else {
                    return Outcome::Rejected("no_service_lifetime".to_owned());
                };
                let kind = if *alias == "judge" {
                    Kind::Judge
                } else {
                    Kind::Assertion
                };
                let emitted = state.run().for_card(alias).and_then(|view| {
                    view.observe()
                        .eval(&context(kind, failing), EvalObservationOptions::default())
                });
                match emitted {
                    Ok(()) => Outcome::Accepted(None),
                    Err(error) => Outcome::Rejected(error.code().to_owned()),
                }
            }
        }
    }
}
