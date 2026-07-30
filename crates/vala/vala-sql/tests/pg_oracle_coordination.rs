//! PostgreSQL integration coverage for Oracle membership and admission.

use chrono::{Duration, Utc};
use vala_sql::{
    queries::{cluster_nodes::ClusterNodes, oracle_admission::OracleAdmissionLeases},
    row_types::{
        cluster_nodes::{RoleMutation, RoleRegistration},
        oracle_admission::{
            AdmissionAcquire, AdmissionReconcileScope, AdmissionRequest, LeaseMutation, RoleFence,
        },
    },
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::{
    DataTenantId,
    vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, NodeId, OracleAdmissionLease, QueryClass,
        QueryId, ScribeCapabilitiesV1,
    },
};

/// Starts an isolated database and seeds one tenant.
async fn setup() -> (PgFixture, DataTenantId) {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(tenant, &format!("oracle-{}", tenant.as_uuid().simple()))
        .await
        .expect("tenant seeds");
    (fixture, tenant)
}

/// Builds a valid admission request with caller-selected timing and demand.
fn request(
    tenant: DataTenantId,
    query_id: QueryId,
    leader: NodeId,
    fence: u64,
    slots: u32,
    expires_at: chrono::DateTime<Utc>,
) -> AdmissionRequest {
    AdmissionRequest {
        lease: OracleAdmissionLease {
            query_id,
            data_tenant_id: tenant,
            query_class: QueryClass::Interactive,
            slot_units: slots,
            selected_node_ids: vec![leader],
            leader_node_id: leader,
            leader_fencing_token: fence,
            acquired_at: Utc::now(),
            expires_at,
        },
        cluster_limit: slots,
        class_limit: slots,
        tenant_limit: slots,
    }
}

/// Proves one physical node receives independent role fences.
#[tokio::test]
async fn same_node_roles_have_independent_fences() {
    let (fixture, _) = setup().await;
    let owner = ClusterNodes::new(fixture.operator_pool().clone());
    let node_id = NodeId::new(uuid::Uuid::now_v7());
    let now = Utc::now();
    let scribe = RoleRegistration {
        key: ClusterNodeKey {
            node_id,
            role: ClusterRole::Scribe,
        },
        address: "http://scribe:5001".into(),
        capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
            tail_protocol_version: 1,
        }),
        started_at: now,
    };
    let oracle = RoleRegistration {
        key: ClusterNodeKey {
            node_id,
            role: ClusterRole::Oracle,
        },
        address: "http://oracle:5002".into(),
        capabilities: ClusterCapabilities::OracleV1(wyrd_spec::vala::api::OracleCapabilitiesV1 {
            peer_protocol_version: 1,
            storage_protocol_version: 1,
            cpu_cores: 4.0,
            memory_budget_bytes: 4096,
            cpu_cores_per_slot: 1.0,
            memory_bytes_per_slot: 1024,
            raw_slots: 4,
            usable_slots: 3,
            supported_classes: vec![QueryClass::Interactive],
            max_workers_per_query: 3,
        }),
        started_at: now,
    };

    let first_scribe = owner.register(&scribe).await.expect("scribe registers");
    let first_oracle = owner.register(&oracle).await.expect("oracle registers");
    let second_scribe = owner.register(&scribe).await.expect("scribe re-registers");

    assert_eq!(first_scribe.lease.fencing_token, 1);
    assert_eq!(first_oracle.lease.fencing_token, 1);
    assert_eq!(second_scribe.lease.fencing_token, 2);
    assert_eq!(
        owner
            .unregister(&oracle.key, first_oracle.lease.fencing_token)
            .await
            .expect("operator role can delete matching membership"),
        RoleMutation::Applied
    );
    let scribe_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.cluster_nodes WHERE node_id=$1 AND role='scribe'",
    )
    .bind(node_id.as_uuid())
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("Scribe role reads");
    assert_eq!(scribe_rows, 1);
}

/// Proves stale mutations fail and discovery excludes unready roles.
#[tokio::test]
async fn stale_role_mutation_is_rejected() {
    let (fixture, _) = setup().await;
    let owner = ClusterNodes::new(fixture.operator_pool().clone());
    let registration = RoleRegistration {
        key: ClusterNodeKey {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            role: ClusterRole::Scribe,
        },
        address: "http://scribe:5001".into(),
        capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
            tail_protocol_version: 1,
        }),
        started_at: Utc::now(),
    };
    owner.register(&registration).await.expect("role registers");

    assert_eq!(
        owner
            .heartbeat(&registration.key, 0, true, &registration.capabilities)
            .await
            .expect("stale heartbeat is represented"),
        RoleMutation::StaleFence
    );
    assert!(
        owner
            .list_live(ClusterRole::Scribe, Utc::now() - Duration::minutes(1))
            .await
            .expect("discovery succeeds")
            .is_empty()
    );
}

/// Proves concurrent admission serializes the cluster counter.
#[tokio::test]
async fn concurrent_admission_never_exceeds_scope_limits() {
    let (fixture, tenant) = setup().await;
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let first = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let second = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let expiry = Utc::now() + Duration::minutes(5);
    let one = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    let two = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    let (a, b) = tokio::join!(first.acquire(&one), second.acquire(&two));
    let acquired = [a.expect("first admission"), b.expect("second admission")]
        .into_iter()
        .filter(|outcome| matches!(outcome, AdmissionAcquire::Acquired(_)))
        .count();

    assert_eq!(acquired, 1);
}

/// Proves release is idempotent while a mismatched fence remains stale.
#[tokio::test]
async fn release_is_idempotent_and_fenced() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    owner
        .acquire(&request(
            tenant,
            query_id,
            leader,
            7,
            1,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    assert!(matches!(
        owner
            .release(
                query_id,
                &RoleFence {
                    node_id: leader,
                    fencing_token: 6,
                },
            )
            .await
            .expect("stale release is represented"),
        LeaseMutation::StaleLeaderFence
    ));
    let fence = RoleFence {
        node_id: leader,
        fencing_token: 7,
    };
    assert!(matches!(
        owner
            .release(query_id, &fence)
            .await
            .expect("release succeeds"),
        LeaseMutation::Released
    ));
    assert!(matches!(
        owner
            .release(query_id, &fence)
            .await
            .expect("repeat release succeeds"),
        LeaseMutation::AlreadyReleased
    ));
}

/// Proves expiry reclaims one lease exactly once.
#[tokio::test]
async fn expiry_reclaims_once() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            NodeId::new(uuid::Uuid::now_v7()),
            1,
            2,
            Utc::now() + Duration::milliseconds(10),
        ))
        .await
        .expect("admission succeeds");
    let cutoff = Utc::now() + Duration::seconds(1);
    let first = owner
        .expire_batch(cutoff, 8)
        .await
        .expect("expiry succeeds");
    let second = owner
        .expire_batch(cutoff, 8)
        .await
        .expect("repeat expiry succeeds");

    assert_eq!((first.expired_leases, first.released_slots), (1, 2));
    assert_eq!((second.expired_leases, second.released_slots), (0, 0));
}

/// Proves reconciliation restores a corrupted authoritative counter.
#[tokio::test]
async fn reconciliation_repairs_interrupted_counter() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            NodeId::new(uuid::Uuid::now_v7()),
            1,
            2,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    sqlx::query(
        "UPDATE vala.oracle_admission_accounting SET used_slots=99 \
         WHERE scope_kind='cluster' AND scope_key='global' AND accounting_class='all'",
    )
    .execute(fixture.operator_pool().pool())
    .await
    .expect("counter corrupts");

    let report = owner
        .reconcile_scopes(&[AdmissionReconcileScope::Cluster], Utc::now())
        .await
        .expect("reconciliation succeeds");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE scope_kind='cluster' AND scope_key='global' AND accounting_class='all'",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("counter reads");

    assert_eq!(report.repaired_scopes, 1);
    assert_eq!(used, 2);
}

/// Proves reconciliation and acquisition serialize on the canonical counter.
#[tokio::test]
async fn concurrent_reconcile_and_acquire_preserve_accounting() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let mut first = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        Utc::now() + Duration::minutes(5),
    );
    first.cluster_limit = 10;
    first.class_limit = 10;
    first.tenant_limit = 10;
    owner.acquire(&first).await.expect("first lease acquires");

    let acquiring = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let reconciling = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let mut second = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        Utc::now() + Duration::minutes(5),
    );
    second.cluster_limit = 10;
    second.class_limit = 10;
    second.tenant_limit = 10;
    let (acquired, reconciled) = tokio::join!(
        acquiring.acquire(&second),
        reconciling.reconcile_scopes(&[AdmissionReconcileScope::Cluster], Utc::now())
    );
    assert!(matches!(
        acquired.expect("concurrent admission completes"),
        AdmissionAcquire::Acquired(_)
    ));
    reconciled.expect("concurrent reconciliation completes");

    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE scope_kind='cluster' AND scope_key='global' AND accounting_class='all'",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("counter reads");
    assert_eq!(used, 2);
}

/// Proves admission hints and bounded maintenance inputs remain closed.
#[tokio::test]
async fn admission_hints_and_maintenance_bounds_are_enforced() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let expiry = Utc::now() + Duration::minutes(5);
    owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            leader,
            1,
            1,
            expiry,
        ))
        .await
        .expect("first admission succeeds");
    let rejection = owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            leader,
            1,
            1,
            expiry,
        ))
        .await
        .expect("capacity rejection is represented");
    assert!(matches!(
        rejection,
        AdmissionAcquire::Rejected {
            retry_after_ms: 1_000,
            ..
        }
    ));
    assert!(owner.expire_batch(Utc::now(), 0).await.is_err());
    assert!(owner.expire_batch(Utc::now(), 129).await.is_err());
    assert!(owner.reconcile_scopes(&[], Utc::now()).await.is_err());
    assert!(
        owner
            .reconcile_scopes(
                &[
                    AdmissionReconcileScope::Cluster,
                    AdmissionReconcileScope::Cluster
                ],
                Utc::now()
            )
            .await
            .is_err()
    );
    let too_many = (0..65)
        .map(|_| AdmissionReconcileScope::Tenant {
            data_tenant_id: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
        })
        .collect::<Vec<_>>();
    assert!(owner.reconcile_scopes(&too_many, Utc::now()).await.is_err());
}

/// Proves leader-fence comparison precedes expiry during renewal.
#[tokio::test]
async fn expired_mismatched_renewal_is_stale() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    owner
        .acquire(&request(
            tenant,
            query_id,
            leader,
            7,
            1,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    sqlx::query(
        "UPDATE vala.oracle_admission_leases \
         SET acquired_at=now()-interval '2 seconds',expires_at=now()-interval '1 second'",
    )
    .execute(fixture.operator_pool().pool())
    .await
    .expect("lease expires");
    assert!(matches!(
        owner
            .renew(
                query_id,
                &RoleFence {
                    node_id: leader,
                    fencing_token: 6,
                },
                Utc::now() + Duration::minutes(5),
            )
            .await
            .expect("renewal outcome returns"),
        LeaseMutation::StaleLeaderFence
    ));
}

/// Proves ready but stale membership is excluded from discovery.
#[tokio::test]
async fn live_discovery_excludes_expired_heartbeat() {
    let (fixture, _) = setup().await;
    let owner = ClusterNodes::new(fixture.operator_pool().clone());
    let registration = RoleRegistration {
        key: ClusterNodeKey {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            role: ClusterRole::Scribe,
        },
        address: "http://scribe:5001".into(),
        capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
            tail_protocol_version: 1,
        }),
        started_at: Utc::now(),
    };
    let registered = owner.register(&registration).await.expect("role registers");
    owner
        .heartbeat(
            &registration.key,
            registered.lease.fencing_token,
            true,
            &registration.capabilities,
        )
        .await
        .expect("role becomes ready");
    sqlx::query(
        "UPDATE vala.cluster_nodes SET heartbeat_at=now()-interval '10 minutes' \
         WHERE node_id=$1 AND role='scribe'",
    )
    .bind(registration.key.node_id.as_uuid())
    .execute(fixture.operator_pool().pool())
    .await
    .expect("heartbeat ages");
    assert!(
        owner
            .list_live(ClusterRole::Scribe, Utc::now() - Duration::minutes(1))
            .await
            .expect("discovery succeeds")
            .is_empty()
    );
}

/// Proves concurrent release and expiry reclaim one lease only once.
#[tokio::test]
async fn concurrent_release_and_expiry_do_not_double_decrement() {
    let (fixture, tenant) = setup().await;
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    let owner = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    owner
        .acquire(&request(
            tenant,
            query_id,
            leader,
            3,
            1,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    sqlx::query(
        "UPDATE vala.oracle_admission_leases \
         SET acquired_at=now()-interval '2 seconds',expires_at=now()-interval '1 second'",
    )
    .execute(fixture.operator_pool().pool())
    .await
    .expect("lease expires");
    let releasing = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let expiring = OracleAdmissionLeases::new(fixture.operator_pool().clone());
    let fence = RoleFence {
        node_id: leader,
        fencing_token: 3,
    };
    let (released, expired) = tokio::join!(
        releasing.release(query_id, &fence),
        expiring.expire_batch(Utc::now(), 8)
    );
    released.expect("release completes");
    expired.expect("expiry completes");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE scope_kind='cluster' AND scope_key='global' AND accounting_class='all'",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("counter reads");
    assert_eq!(used, 0);
}
