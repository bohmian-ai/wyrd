//! Focused production-composition proof for the Task 3B follower boundary.

use wyrd_testing::load::{BifrostClusterLoad, ClusterLoadProfile};

/// Boots the real mixed-role peer composition and proves every follower owner
/// starts clean and settles through the production cluster shutdown path.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn follower_ticket_source_stream_and_cleanup_are_exact() {
    let mut profile = ClusterLoadProfile::multi_tenant_three_server_three_worker();
    profile.tenants = 1;
    vala_bifrost_redux::oracle::reset_remote_partition_attempts_for_test();
    let summary = BifrostClusterLoad::start(profile)
        .await
        .expect("three-Server physical follower cluster")
        .run()
        .await
        .expect("one-tenant physical follower journey");
    assert!(
        vala_bifrost_redux::oracle::remote_partition_attempts_for_test() > 0,
        "public query must reach authenticated follower execution"
    );
    println!("BIFROST_PARITY_CASE=P27:PASS");
    assert!(
        summary
            .tenants
            .values()
            .all(|tenant| tenant.completed_reads >= 2)
    );
    println!("BIFROST_PARITY_CASE=P28:PASS");
    assert!(
        summary
            .tenants
            .values()
            .all(|tenant| tenant.final_terminal_count == 1)
    );
    println!("BIFROST_PARITY_CASE=P29:PASS");
    assert_eq!(summary.cleanup.oracle_tail_fences, 0);
    assert_eq!(summary.cleanup.gate_active_streams, 0);
    assert_eq!(summary.cleanup.supervised_tasks, 0);
    assert!(summary.cleanup.listeners_stopped && summary.cleanup.servers_stopped);
    println!("BIFROST_PARITY_CASE=P35:PASS");
}
