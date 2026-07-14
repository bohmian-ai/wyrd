//! Multi-pod Scribe stub-tier journey test.

use std::time::Duration;
use wyrd_spec::ids::DataTenantId;
use wyrd_testing::bifrost::{HarnessConfig, MultiScribeHarness};

#[tokio::test]
async fn multi_scribe_harness_three_pods_construct_and_teardown_stub() {
    // PR#1 stub tier: the harness constructs, pods are reachable, and teardown
    // completes cleanly. No real append/seal/drain behavior yet.

    let tenant_a = DataTenantId::SYSTEM_OWNER;
    let tenant_b = DataTenantId::new_v7();

    let cfg = HarnessConfig {
        pods: 3,
        tenants: vec![tenant_a, tenant_b],
        schema: format!("test_scribe_{}", uuid::Uuid::now_v7().simple()),
    };

    let harness = MultiScribeHarness::new(cfg)
        .await
        .expect("harness constructs");

    // Verify pod count
    assert_eq!(harness.pods().count(), 3);

    // Verify individual pod access
    let pod0 = harness.pod(0);
    let pod1 = harness.pod(1);
    let pod2 = harness.pod(2);

    // In PR#1, ScribeImpl::append is a stub — just verify it returns Ok
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;

    let principal_id = PrincipalId::new(uuid::Uuid::now_v7());
    let append_req = ScribeAppend {
        table_fqn: "vala.datasets.test_table".to_string(),
        schema_fingerprint: [0u8; 32],
        principal: Principal {
            id: principal_id,
            kind: PrincipalKind::User,
            tenant_id: tenant_a,
            roles: vec![],
            effective_permissions: PermissionSet::new(),
        },
        batch_data: vec![],
    };

    let ack0 = pod0.append(append_req.clone()).await.expect("pod0 append");
    let ack1 = pod1.append(append_req.clone()).await.expect("pod1 append");
    let ack2 = pod2.append(append_req.clone()).await.expect("pod2 append");

    // Verify acks are returned
    assert_eq!(ack0.batch_id, [0u8; 16]);
    assert_eq!(ack1.batch_id, [0u8; 16]);
    assert_eq!(ack2.batch_id, [0u8; 16]);

    // Stub-tier drain is a no-op; just verify it completes
    harness
        .wait_for_drain(Duration::from_secs(1))
        .await
        .expect("drain completes");

    // Teardown
    harness.shutdown().await.expect("shutdown completes");
}

#[tokio::test]
async fn harness_rejects_sql_injection_attempt() {
    let cfg = HarnessConfig {
        pods: 1,
        tenants: vec![],
        schema: "test; DROP SCHEMA wyrd CASCADE".to_string(),
    };
    let result = MultiScribeHarness::new(cfg).await;
    assert!(
        result.is_err(),
        "should reject schema name with SQL injection attempt"
    );
}

#[tokio::test]
async fn harness_rejects_oversized_schema_name() {
    let cfg = HarnessConfig {
        pods: 1,
        tenants: vec![],
        schema: "a".repeat(64), // 64 chars, exceeds PG limit of 63
    };
    let result = MultiScribeHarness::new(cfg).await;
    assert!(
        result.is_err(),
        "should reject schema name exceeding 63 characters"
    );
}

#[tokio::test]
async fn harness_rejects_sql_keyword_schema_name() {
    let cfg = HarnessConfig {
        pods: 1,
        tenants: vec![],
        schema: "DROP".to_string(),
    };
    let result = MultiScribeHarness::new(cfg).await;
    assert!(result.is_err(), "should reject SQL keyword as schema name");
}
