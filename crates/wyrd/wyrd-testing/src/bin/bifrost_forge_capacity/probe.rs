//! The live leader's decision latency under open-loop peer pulls.
//!
//! Before any compactor starts, the benchmark plays the production fleet of
//! [`FLEET_WORKERS`] compactors against the running leader over the same
//! private peer route a dedicated worker dials: each pulls [`PULL_LIMIT`]
//! tasks on a fixed, staggered arrival schedule and reports every dispatched
//! task `NotStarted`, which returns the table to due with its commits intact,
//! so the probe consumes no compaction and leaves the backlog as it found it.
//! The leader's own `bifrost_forge_leader_decision_seconds` summary is scraped
//! at the end of each step; the probe also records the client-observed pull
//! round trip.

use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::PgPool;
use tokio::task::JoinSet;
use tokio::time;
use vala_bifrost_redux::forge::{ForgeCompactionOutcome, outcome_to_wire};
use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
use vala_sql::OperatorPool;
use vala_sql::queries::forge_leader::ForgeLeaderElection;
use wyrd_testing::capacity::Percentiles;
use wyrd_testing::release_server::LocalServer;
use wyrd_testing::server::TestBifrostPeerTls;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::forge_leader_peer_service_client::ForgeLeaderPeerServiceClient;
use wyrd_tonic::wyrd::v1::{PullForgeCompactionRequest, ReportForgeCompactionRequest};

use crate::Result;
use crate::deployment::LeaderDecisions;
use crate::schedule::{FLEET_WORKERS, PULL_LIMIT, production_pulls_per_second};

/// The peer client the probe dials the elected leader with.
pub struct LeaderProbe {
    /// Reads the current term's fencing token and peer address.
    election: ForgeLeaderElection,
    /// The probe's own peer identity, issued by the deployment's peer CA.
    tls: BifrostPeerTls,
}

/// One offered rate against the live leader.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeStep {
    /// Offered rate as a multiple of the production rate.
    pub multiplier: f64,
    /// Offered pulls per second.
    pub offered_pulls_per_second: f64,
    /// Pulls answered per second.
    pub achieved_pulls_per_second: f64,
    /// Pulls answered.
    pub pulls: usize,
    /// Pulls answered with [`PULL_LIMIT`] tasks.
    pub full_pulls: usize,
    /// Pulls or reports the leader refused.
    pub errors: usize,
    /// Client-observed pull round trip, microseconds.
    pub round_trip_us: Percentiles,
    /// The leader's own decisions over the step.
    pub leader: LeaderDecisions,
}

/// One simulated compactor's results.
#[derive(Default)]
struct Tally {
    /// Pull round trips, nanoseconds.
    round_trips: Vec<u64>,
    /// Pulls answered in full.
    full: usize,
    /// Refused calls.
    errors: usize,
}

impl LeaderProbe {
    /// Reads the probe's peer identity from `peer` and the term from the
    /// database `owner` reaches.
    ///
    /// # Errors
    ///
    /// Returns an unreadable certificate, key, or CA file.
    pub fn new(owner: PgPool, peer: &TestBifrostPeerTls) -> Result<Self> {
        Ok(Self {
            election: ForgeLeaderElection::new(OperatorPool::from(owner)),
            tls: BifrostPeerTls::new(
                std::fs::read(&peer.ca_path)?,
                peer.server_name.clone(),
                std::fs::read(&peer.certificate_path)?,
                secrecy::SecretString::from(std::fs::read_to_string(&peer.private_key_path)?),
            ),
        })
    }

    /// Offers `multiplier` times the production pull rate to the elected
    /// leader for `seconds`, then reads `leader`'s decision summary.
    ///
    /// # Errors
    ///
    /// Returns no elected leader, a leader without a peer address, a dial
    /// failure, or a scrape failure.
    pub async fn step(
        &self,
        leader: &LocalServer,
        multiplier: f64,
        seconds: f64,
    ) -> Result<ProbeStep> {
        let term = self
            .election
            .current()
            .await?
            .ok_or("no Forge leader holds the term")?;
        let uri = term
            .peer_uri
            .ok_or("the Forge leader publishes no peer address")?;
        let channel = self.tls.endpoint(uri)?.connect().await?;
        let client = ForgeLeaderPeerServiceClient::new(channel);
        let before = leader.metrics().await?;
        let period = vala_bifrost_redux::forge::ForgeWorkerConfig::default()
            .pull_interval
            .div_f64(multiplier);
        let started = time::Instant::now();
        let end = started + Duration::from_secs_f64(seconds);
        let mut compactors = JoinSet::new();
        for worker in 0..FLEET_WORKERS {
            let first = started + period.mul_f64(worker as f64 / FLEET_WORKERS as f64);
            compactors.spawn(compactor(
                client.clone(),
                term.fencing_token,
                first,
                period,
                end,
            ));
        }
        let mut tallies = Vec::new();
        while let Some(tally) = compactors.join_next().await {
            tallies.push(tally?);
        }
        let elapsed = started.elapsed().as_secs_f64().max(seconds);
        let after = leader.metrics().await?;
        let round_trips: Vec<u64> = tallies
            .iter()
            .flat_map(|tally| tally.round_trips.iter().copied())
            .collect();
        Ok(ProbeStep {
            multiplier,
            offered_pulls_per_second: production_pulls_per_second() * multiplier,
            achieved_pulls_per_second: round_trips.len() as f64 / elapsed,
            pulls: round_trips.len(),
            full_pulls: tallies.iter().map(|tally| tally.full).sum(),
            errors: tallies.iter().map(|tally| tally.errors).sum(),
            round_trip_us: Percentiles::raw(&round_trips, 1e-3),
            leader: LeaderDecisions::read(&before, &after, elapsed),
        })
    }
}

/// Plays one compactor against the leader: pulls at `first` and every
/// `period` after it until `end`, and reports each dispatched task
/// `NotStarted` so its table is due again.
async fn compactor(
    mut client: ForgeLeaderPeerServiceClient<Channel>,
    fencing_token: i64,
    first: time::Instant,
    period: Duration,
    end: time::Instant,
) -> Tally {
    let mut tally = Tally::default();
    let mut arrival = first;
    while arrival < end {
        time::sleep_until(arrival).await;
        arrival += period;
        let timer = Instant::now();
        let pulled = client
            .pull_compaction(PullForgeCompactionRequest {
                fencing_token,
                limit: u32::try_from(PULL_LIMIT).unwrap_or(u32::MAX),
            })
            .await;
        let elapsed = u64::try_from(timer.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let Ok(response) = pulled else {
            tally.errors += 1;
            continue;
        };
        tally.round_trips.push(elapsed);
        let tasks = response.into_inner().tasks;
        tally.full += usize::from(tasks.len() == PULL_LIMIT);
        for task in tasks {
            let reported = client
                .report_compaction(ReportForgeCompactionRequest {
                    fencing_token,
                    task_id: task.task_id,
                    tenant_id: task.tenant_id,
                    namespace: task.namespace,
                    table: task.table,
                    outcome: outcome_to_wire(ForgeCompactionOutcome::NotStarted) as i32,
                })
                .await;
            tally.errors += usize::from(reported.is_err());
        }
    }
    tally
}
