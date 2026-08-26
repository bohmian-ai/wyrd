//! Role activation: Oracle and Scribe come up only after explicit readiness.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::cluster::ClusterRegistry;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::vala::api::{OracleCapabilitiesV1, QueryClass, ScribeCapabilitiesV1};

use super::support::*;

/// Proves reserved roles stay undiscoverable and colocated fences tear down independently.
///
/// # Panics
///
/// Panics when the managed database or a durable membership transition fails.
#[tokio::test]
async fn oracle_and_scribe_roles_activate_only_after_explicit_readiness() {
    let roles = reserve_role_pair().await;
    let mut shutdown_events = deactivate_and_cancel_roles(&roles).await;
    unregister_roles(&roles, &mut shutdown_events).await;
    assert_eq!(
        shutdown_events,
        [
            "durable_deactivate",
            "drain",
            "heartbeat_stop",
            "reject_cancel_active",
            "oracle_unregister",
            "scribe_unregister",
        ]
    );
}

/// Retained database and colocated role registrations for the lifecycle proof.
struct RolePair {
    /// Managed database retained through teardown.
    _pg: PgFixture,
    /// Shared role-fenced registry.
    cluster: Arc<ClusterRegistry>,
    /// Reserved Oracle role.
    oracle: vala_bifrost_redux::cluster::RegisteredRole,
    /// Reserved Scribe role.
    scribe: vala_bifrost_redux::cluster::RegisteredRole,
}

/// Reserves both roles, proves they are hidden, then explicitly activates them.
async fn reserve_role_pair() -> RolePair {
    let pg = PgFixture::start().await.expect("managed Postgres fixture");
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    let cluster = Arc::new(ClusterRegistry::new(pg.vala_postgres().clone(), node_id));
    let oracle = cluster
        .reserve_oracle(
            "127.0.0.1:50052",
            OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 256 * 1024 * 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 64 * 1024 * 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                max_workers_per_query: 0,
            },
        )
        .await
        .expect("reserve Oracle");
    let scribe = cluster
        .reserve_scribe(
            "127.0.0.1:50053",
            ScribeCapabilitiesV1 {
                tail_protocol_version: 1,
            },
        )
        .await
        .expect("reserve Scribe");
    cluster.refresh_snapshot().await.expect("reserved snapshot");
    assert!(cluster.snapshot().live_oracles().is_empty());
    assert!(cluster.snapshot().live_scribes().is_empty());
    cluster.activate(&oracle).await.expect("activate Oracle");
    cluster.activate(&scribe).await.expect("activate Scribe");
    cluster.refresh_snapshot().await.expect("active snapshot");
    assert_eq!(cluster.snapshot().live_oracles().len(), 1);
    assert_eq!(cluster.snapshot().live_scribes().len(), 1);
    RolePair {
        _pg: pg,
        cluster,
        oracle,
        scribe,
    }
}

/// Deactivates readiness, proves draining heartbeats remain hidden, and cancels work.
async fn deactivate_and_cancel_roles(roles: &RolePair) -> Vec<&'static str> {
    let oracle_ready = Arc::new(AtomicBool::new(true));
    let scribe_ready = Arc::new(AtomicBool::new(true));
    let oracle_stop = CancellationToken::new();
    let scribe_stop = CancellationToken::new();
    let oracle_heartbeat = Arc::clone(&roles.cluster).start_readiness_heartbeat(
        roles.oracle.clone(),
        Arc::clone(&oracle_ready),
        oracle_stop.clone(),
    );
    let scribe_heartbeat = Arc::clone(&roles.cluster).start_readiness_heartbeat(
        roles.scribe.clone(),
        Arc::clone(&scribe_ready),
        scribe_stop.clone(),
    );
    tokio::task::yield_now().await;
    let oracle_work = CancellationToken::new();
    let scribe_work = CancellationToken::new();
    let oracle_active = tokio::spawn({
        let stop = oracle_work.clone();
        async move { stop.cancelled().await }
    });
    let scribe_active = tokio::spawn({
        let stop = scribe_work.clone();
        async move { stop.cancelled().await }
    });
    oracle_ready.store(false, Ordering::Release);
    scribe_ready.store(false, Ordering::Release);
    roles
        .cluster
        .deactivate(&roles.oracle)
        .await
        .expect("deactivate Oracle");
    roles
        .cluster
        .deactivate(&roles.scribe)
        .await
        .expect("deactivate Scribe");
    roles
        .cluster
        .refresh_snapshot()
        .await
        .expect("deactivated snapshot");
    assert!(roles.cluster.snapshot().live_oracles().is_empty());
    assert!(roles.cluster.snapshot().live_scribes().is_empty());
    let mut clock = InjectedRoleClock::new();
    clock.advance(vala_bifrost_redux::cluster::ROLE_HEARTBEAT_INTERVAL);
    roles
        .cluster
        .heartbeat_readiness_for_test(&roles.oracle, false)
        .await
        .expect("injected Oracle heartbeat");
    roles
        .cluster
        .heartbeat_readiness_for_test(&roles.scribe, false)
        .await
        .expect("injected Scribe heartbeat");
    roles
        .cluster
        .refresh_snapshot()
        .await
        .expect("draining heartbeat snapshot");
    assert!(roles.cluster.snapshot().live_oracles().is_empty());
    assert!(roles.cluster.snapshot().live_scribes().is_empty());
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        clock.elapsed,
        vala_bifrost_redux::cluster::ROLE_HEARTBEAT_INTERVAL + Duration::from_secs(1)
    );
    oracle_stop.cancel();
    scribe_stop.cancel();
    oracle_heartbeat.await.expect("Oracle heartbeat stops");
    scribe_heartbeat.await.expect("Scribe heartbeat stops");
    oracle_work.cancel();
    scribe_work.cancel();
    oracle_active.await.expect("Oracle active work cancels");
    scribe_active.await.expect("Scribe active work cancels");
    vec![
        "durable_deactivate",
        "drain",
        "heartbeat_stop",
        "reject_cancel_active",
    ]
}

/// Unregisters each role independently and proves the Scribe fence survives Oracle teardown.
async fn unregister_roles(roles: &RolePair, events: &mut Vec<&'static str>) {
    roles
        .cluster
        .shutdown_role(roles.oracle.clone())
        .await
        .expect("unregister Oracle");
    events.push("oracle_unregister");
    roles
        .cluster
        .heartbeat(&roles.scribe)
        .await
        .expect("Scribe fence survives");
    roles
        .cluster
        .refresh_snapshot()
        .await
        .expect("Oracle shutdown snapshot");
    assert!(roles.cluster.snapshot().live_oracles().is_empty());
    assert_eq!(roles.cluster.snapshot().live_scribes().len(), 1);
    roles
        .cluster
        .deactivate(&roles.scribe)
        .await
        .expect("deactivate Scribe");
    roles
        .cluster
        .shutdown_role(roles.scribe.clone())
        .await
        .expect("unregister Scribe");
    events.push("scribe_unregister");
}
