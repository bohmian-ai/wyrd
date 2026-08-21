//! PostgreSQL integration proofs for hierarchical Oracle admission blocks.

use chrono::Duration;
use vala_sql::queries::cluster_nodes::ClusterNodes;
use vala_sql::queries::oracle_admission::OracleAdmissionBlocks;
use vala_sql::row_types::cluster_nodes::RoleRegistration;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterNodeKey, ClusterRole, OracleCapabilitiesV1,
};
use wyrd_spec::vala::api::{NodeId, OracleAdmissionDemand, QueryClass};

/// Registers and activates one exact Oracle role used by allocator proofs.
///
/// # Panics
///
/// Panics when the fixture cannot register or activate the role.
async fn register_oracle(fixture: &PgFixture, node_id: NodeId) -> u64 {
    let nodes = ClusterNodes::new(fixture.vala_postgres().clone());
    let capabilities = ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
        peer_protocol_version: 1,
        storage_protocol_version: 1,
        cpu_cores: 1.0,
        memory_budget_bytes: 1024,
        cpu_cores_per_slot: 1.0,
        memory_bytes_per_slot: 1024,
        raw_slots: 2,
        usable_slots: 2,
        supported_classes: vec![QueryClass::Interactive],
        max_workers_per_query: 1,
    });
    let key = ClusterNodeKey {
        node_id,
        role: ClusterRole::Oracle,
    };
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("tenant conn");
    let registered = nodes
        .register(
            &mut conn,
            &RoleRegistration {
                key,
                address: "http://oracle.test".to_owned(),
                capabilities: capabilities.clone(),
                started_at: chrono::Utc::now(),
            },
        )
        .await
        .expect("role registers");
    nodes
        .heartbeat(
            &mut conn,
            &key,
            registered.lease.fencing_token,
            true,
            &capabilities,
        )
        .await
        .expect("role activates");
    conn.commit().await.expect("role commits");
    registered.lease.fencing_token
}

/// Canonical insertion creates four rows and conflicting pod configuration fails.
///
/// # Panics
///
/// Panics when the isolated PostgreSQL fixture or asserted SQL behavior fails.
#[tokio::test]
async fn canonical_policy_is_idempotent_and_conflict_fails() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(4, 2, 3, 1)
        .await
        .expect("canonical policies");
    blocks
        .ensure_canonical_policies(4, 2, 3, 1)
        .await
        .expect("idempotent canonical policies");
    let policies: Vec<(String, Option<uuid::Uuid>, String, i32)> = sqlx::query_as(
        "SELECT scope_kind,data_tenant_id,query_class,capacity \
         FROM vala.oracle_admission_policies ORDER BY scope_kind,query_class",
    )
    .fetch_all(fixture.operator_pool().pool())
    .await
    .expect("canonical policy rows");
    assert_eq!(policies.len(), 4);
    assert!(
        policies
            .iter()
            .all(|(_, tenant_id, _, _)| tenant_id.is_none())
    );
    assert_eq!(
        policies
            .iter()
            .filter(|(scope, _, _, _)| scope == "global")
            .count(),
        2
    );
    assert_eq!(
        policies
            .iter()
            .filter(|(scope, _, _, _)| scope == "tenant_default")
            .count(),
        2
    );
    assert!(blocks.ensure_canonical_policies(5, 2, 3, 1).await.is_err());
}

/// Production startup is independent of the current and future tenant inventory.
///
/// # Panics
///
/// Panics when startup policy initialization, inspection, or conflict rollback fails.
#[tokio::test]
async fn startup_policies_never_materialize_tenants() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    fixture
        .seed_additional_tenant("startup-second")
        .await
        .expect("second tenant seeds");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(4, 2, 3, 1)
        .await
        .expect("first startup initializes policies");
    fixture
        .seed_additional_tenant("startup-late")
        .await
        .expect("late tenant seeds");
    blocks
        .ensure_canonical_policies(4, 2, 3, 1)
        .await
        .expect("repeated startup is idempotent");
    let tenant_policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_policies WHERE data_tenant_id IS NOT NULL",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("tenant policy count");
    assert_eq!(tenant_policies, 0);
    assert!(
        blocks.ensure_canonical_policies(5, 2, 3, 1).await.is_err(),
        "conflicting startup must fail readiness initialization"
    );
    let retained: i32 = sqlx::query_scalar(
        "SELECT capacity FROM vala.oracle_admission_policies \
         WHERE scope_kind='tenant_default' AND query_class='interactive'",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("tenant default remains canonical");
    assert_eq!(retained, 3);
}

/// Concurrent holders cannot exceed global or tenant canonical ceilings.
///
/// # Panics
///
/// Panics when the isolated PostgreSQL fixture or asserted allocation fails.
#[tokio::test]
async fn allocations_bind_three_scopes_and_respect_ceiling() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(tenant, &format!("blocks-{}", tenant.as_uuid().simple()))
        .await
        .expect("tenant seeds");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(2, 1, 2, 1)
        .await
        .expect("canonical policies");
    assert_ne!(tenant, DataTenantId::SYSTEM_OWNER);
    let first_node = NodeId::new(uuid::Uuid::from_u128(1));
    let second_node = NodeId::new(uuid::Uuid::from_u128(2));
    let first_fence = register_oracle(&fixture, first_node).await;
    let second_fence = register_oracle(&fixture, second_node).await;
    let demand = |node| OracleAdmissionDemand {
        tenant_id: tenant,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 2,
        holder_node_id: node,
        holder_fencing_token: if node == first_node {
            first_fence
        } else {
            second_fence
        },
    };
    let (first, second) = tokio::join!(
        blocks.allocate(demand(first_node), Duration::seconds(15)),
        blocks.allocate(demand(second_node), Duration::seconds(15))
    );
    let first = first.expect("first concurrent allocation");
    let second = second.expect("second concurrent allocation");
    assert_eq!(first.rows.len() + second.rows.len(), 3);
    assert!(
        first
            .rows
            .iter()
            .chain(&second.rows)
            .all(|row| row.units == 2)
    );
    assert!(first.rows.is_empty() || second.rows.is_empty());
}

/// Late tenants share the default ceiling while retaining exact usage isolation.
///
/// # Panics
///
/// Panics when a tenant created after policy initialization cannot allocate or
/// one tenant consumes another tenant's default capacity.
#[tokio::test]
async fn late_tenants_use_dynamic_default_with_isolated_usage() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(6, 1, 2, 1)
        .await
        .expect("canonical policies");
    let tenant_a = fixture
        .seed_additional_tenant("late-admission-a")
        .await
        .expect("first late tenant seeds");
    let tenant_b = fixture
        .seed_additional_tenant("late-admission-b")
        .await
        .expect("second late tenant seeds");
    let node_id = NodeId::new(uuid::Uuid::from_u128(21));
    let fence = register_oracle(&fixture, node_id).await;
    let demand = |tenant_id| OracleAdmissionDemand {
        tenant_id,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 2,
        holder_node_id: node_id,
        holder_fencing_token: fence,
    };
    let first = blocks
        .allocate(demand(tenant_a), Duration::seconds(10))
        .await
        .expect("first tenant allocates");
    let second = blocks
        .allocate(demand(tenant_b), Duration::seconds(10))
        .await
        .expect("second tenant allocates");
    let saturated = blocks
        .allocate(demand(tenant_a), Duration::seconds(10))
        .await
        .expect("first tenant saturation evaluates");
    assert_eq!(first.rows.len(), 3);
    assert_eq!(second.rows.len(), 3);
    assert!(saturated.rows.is_empty());
    let tenant_policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_policies WHERE data_tenant_id IS NOT NULL",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("tenant policy count");
    assert_eq!(tenant_policies, 0);
}

/// Exact fenced closure releases all three accounting rows immediately.
///
/// # Panics
///
/// Panics when closure is partial or closed rows continue consuming capacity.
#[tokio::test]
async fn exact_allocation_closure_frees_global_capacity() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(1, 1, 1, 1)
        .await
        .expect("canonical policies");
    let first_tenant = fixture
        .seed_additional_tenant("retire-first")
        .await
        .expect("first tenant seeds");
    let second_tenant = fixture
        .seed_additional_tenant("retire-second")
        .await
        .expect("second tenant seeds");
    let node_id = NodeId::new(uuid::Uuid::from_u128(31));
    let fence = register_oracle(&fixture, node_id).await;
    let demand = |tenant_id| OracleAdmissionDemand {
        tenant_id,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 1,
        holder_node_id: node_id,
        holder_fencing_token: fence,
    };
    let first = blocks
        .allocate(demand(first_tenant), Duration::seconds(15))
        .await
        .expect("first allocation");
    let allocation_id = first.rows[0].allocation_id;
    assert_eq!(
        blocks
            .close_allocation(allocation_id, node_id.as_uuid(), fence)
            .await
            .expect("exact allocation closes"),
        3
    );
    assert_eq!(
        blocks
            .allocate(demand(second_tenant), Duration::seconds(15))
            .await
            .expect("successor allocation")
            .rows
            .len(),
        3
    );
}

/// Two released tenants let a third allocate beneath a global ceiling of two.
///
/// # Panics
///
/// Panics when idle tenant-specific allocations pin reusable global capacity.
#[tokio::test]
async fn sequential_tenant_retirement_reuses_global_capacity() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(2, 1, 1, 1)
        .await
        .expect("canonical policies");
    let tenants = ["sequential-a", "sequential-b", "sequential-c"];
    let mut tenant_ids = Vec::new();
    for name in tenants {
        tenant_ids.push(
            fixture
                .seed_additional_tenant(name)
                .await
                .expect("tenant seeds"),
        );
    }
    let node_id = NodeId::new(uuid::Uuid::from_u128(32));
    let fence = register_oracle(&fixture, node_id).await;
    let demand = |tenant_id| OracleAdmissionDemand {
        tenant_id,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 1,
        holder_node_id: node_id,
        holder_fencing_token: fence,
    };
    let first = blocks
        .allocate(demand(tenant_ids[0]), Duration::seconds(15))
        .await
        .expect("first allocation");
    let second = blocks
        .allocate(demand(tenant_ids[1]), Duration::seconds(15))
        .await
        .expect("second allocation");
    assert_eq!(first.rows.len(), 3);
    assert_eq!(second.rows.len(), 3);
    assert!(
        blocks
            .allocate(demand(tenant_ids[2]), Duration::seconds(15))
            .await
            .expect("saturated allocation")
            .rows
            .is_empty()
    );
    for allocation_id in [first.rows[0].allocation_id, second.rows[0].allocation_id] {
        assert_eq!(
            blocks
                .close_allocation(allocation_id, node_id.as_uuid(), fence)
                .await
                .expect("idle allocation closes"),
            3
        );
    }
    assert_eq!(
        blocks
            .allocate(demand(tenant_ids[2]), Duration::seconds(15))
            .await
            .expect("third allocation after retirement")
            .rows
            .len(),
        3
    );
}

/// Inactive, deleted, and unknown tenants receive no delegated allocation.
///
/// # Panics
///
/// Panics when tenant validation fails open or leaves block residue.
#[tokio::test]
async fn inactive_deleted_and_unknown_tenants_receive_no_allocation() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(3, 1, 1, 1)
        .await
        .expect("canonical policies");
    let suspended = fixture
        .seed_additional_tenant("admission-suspended")
        .await
        .expect("suspended tenant seeds");
    let deleted = fixture
        .seed_additional_tenant("admission-deleted")
        .await
        .expect("deleted tenant seeds");
    sqlx::query("UPDATE platform.tenants SET status='suspended' WHERE data_tenant_id=$1")
        .bind(suspended.as_uuid())
        .execute(fixture.operator_pool().pool())
        .await
        .expect("tenant suspends");
    sqlx::query(
        "UPDATE platform.tenants SET status='deleted',deleted_at=statement_timestamp() \
         WHERE data_tenant_id=$1",
    )
    .bind(deleted.as_uuid())
    .execute(fixture.operator_pool().pool())
    .await
    .expect("tenant deletes");
    let node_id = NodeId::new(uuid::Uuid::from_u128(22));
    let fence = register_oracle(&fixture, node_id).await;
    for tenant_id in [suspended, deleted, DataTenantId::new_v7()] {
        let allocation = blocks
            .allocate(
                OracleAdmissionDemand {
                    tenant_id,
                    principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                    query_class: QueryClass::Interactive,
                    requested_units: 1,
                    holder_node_id: node_id,
                    holder_fencing_token: fence,
                },
                Duration::seconds(10),
            )
            .await
            .expect("inactive allocation evaluates");
        assert!(allocation.rows.is_empty());
    }
    let blocks_count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.oracle_admission_blocks")
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("block count");
    assert_eq!(blocks_count, 0);
}

/// Allocation serializes with a concurrent tenant suspension and fails closed.
///
/// # Panics
///
/// Panics when allocation bypasses the tenant row lock or persists capacity
/// after the concurrent lifecycle transition commits.
#[tokio::test]
async fn allocation_racing_tenant_suspension_fails_closed() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(1, 1, 1, 1)
        .await
        .expect("canonical policies");
    let tenant = fixture
        .seed_additional_tenant("admission-racing")
        .await
        .expect("racing tenant seeds");
    let node_id = NodeId::new(uuid::Uuid::from_u128(23));
    let fence = register_oracle(&fixture, node_id).await;
    let mut lifecycle = fixture
        .operator_pool()
        .pool()
        .begin()
        .await
        .expect("lifecycle transaction begins");
    sqlx::query("SELECT 1 FROM platform.tenants WHERE data_tenant_id=$1 FOR UPDATE")
        .bind(tenant.as_uuid())
        .fetch_one(&mut *lifecycle)
        .await
        .expect("tenant lifecycle lock");
    let operator_pool = fixture.operator_pool().clone();
    let mut allocation = tokio::spawn(async move {
        OracleAdmissionBlocks::new(&operator_pool)
            .allocate(
                OracleAdmissionDemand {
                    tenant_id: tenant,
                    principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                    query_class: QueryClass::Interactive,
                    requested_units: 1,
                    holder_node_id: node_id,
                    holder_fencing_token: fence,
                },
                Duration::seconds(10),
            )
            .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut allocation)
            .await
            .is_err(),
        "allocation must wait for the tenant lifecycle lock"
    );
    sqlx::query(
        "UPDATE platform.tenants SET status='suspended',updated_at=statement_timestamp() \
         WHERE data_tenant_id=$1",
    )
    .bind(tenant.as_uuid())
    .execute(&mut *lifecycle)
    .await
    .expect("tenant suspends");
    lifecycle.commit().await.expect("suspension commits");
    let allocation = allocation
        .await
        .expect("allocation task joins")
        .expect("allocation evaluates");
    assert!(allocation.rows.is_empty());
}

/// Expired capacity remains fenced while its system role is live, then reuses.
///
/// # Panics
///
/// Panics when data-tenant accounting is confused with system membership or
/// predecessor capacity is reused before the successor fence replaces it.
#[tokio::test]
async fn reuse_requires_expiry_and_non_live_system_owner_incarnation() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture
        .seed_additional_tenant("admission-reuse")
        .await
        .expect("data tenant seeds");
    assert_ne!(tenant, DataTenantId::SYSTEM_OWNER);
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(1, 1, 1, 1)
        .await
        .expect("canonical policies");
    let predecessor = NodeId::new(uuid::Uuid::from_u128(11));
    let contender = NodeId::new(uuid::Uuid::from_u128(12));
    let predecessor_fence = register_oracle(&fixture, predecessor).await;
    let contender_fence = register_oracle(&fixture, contender).await;
    let demand = |node_id, fence| OracleAdmissionDemand {
        tenant_id: tenant,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 1,
        holder_node_id: node_id,
        holder_fencing_token: fence,
    };
    let predecessor_allocation = blocks
        .allocate(
            demand(predecessor, predecessor_fence),
            Duration::seconds(10),
        )
        .await
        .expect("predecessor allocation");
    assert_eq!(predecessor_allocation.rows.len(), 3);
    sqlx::query(
        "UPDATE vala.oracle_admission_blocks \
         SET expires_at=statement_timestamp()-interval '1 millisecond' \
         WHERE allocation_id=$1",
    )
    .bind(predecessor_allocation.rows[0].allocation_id)
    .execute(fixture.operator_pool().pool())
    .await
    .expect("predecessor block expires");
    let refused = blocks
        .allocate(demand(contender, contender_fence), Duration::seconds(10))
        .await
        .expect("live-predecessor refusal query");
    assert!(refused.rows.is_empty());

    let successor_fence = register_oracle(&fixture, predecessor).await;
    assert!(successor_fence > predecessor_fence);
    let successor = blocks
        .allocate(demand(predecessor, successor_fence), Duration::seconds(10))
        .await
        .expect("successor allocation");
    assert_eq!(successor.rows.len(), 3);
    assert!(
        successor
            .rows
            .iter()
            .all(|row| row.holder_fencing_token == i64::try_from(successor_fence).expect("fence"))
    );
}

/// Fenced renewal returns one complete authoritative database-time expiry.
///
/// # Panics
///
/// Panics when allocation, repeated renewal, or late-renewal refusal diverges.
#[tokio::test]
async fn renewal_advances_authoritative_expiry_and_refuses_late_rows() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_canonical_policies(1, 1, 1, 1)
        .await
        .expect("canonical policies");
    let node_id = NodeId::new(uuid::Uuid::from_u128(7));
    assert_ne!(tenant, DataTenantId::SYSTEM_OWNER);
    let fence = register_oracle(&fixture, node_id).await;
    let demand = OracleAdmissionDemand {
        tenant_id: tenant,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 1,
        holder_node_id: node_id,
        holder_fencing_token: fence,
    };
    let allocated = blocks
        .allocate(demand, Duration::seconds(2))
        .await
        .expect("allocation");
    assert!(allocated.rows.iter().all(|row| {
        row.valid_from == allocated.database_now && row.expires_at > allocated.database_now
    }));
    let allocation_id = allocated.rows[0].allocation_id;
    let first = blocks
        .renew(
            allocation_id,
            node_id.as_uuid(),
            fence,
            Duration::seconds(10),
        )
        .await
        .expect("first renewal")
        .expect("live allocation renews");
    assert_eq!(first.rows.len(), 3);
    assert!(
        first
            .rows
            .iter()
            .all(|row| row.expires_at > first.database_now)
    );
    let second = blocks
        .renew(
            allocation_id,
            node_id.as_uuid(),
            fence,
            Duration::seconds(10),
        )
        .await
        .expect("second renewal")
        .expect("continuous allocation renews again");
    assert!(second.rows[0].expires_at > first.rows[0].expires_at);
    sqlx::query(
        "UPDATE vala.oracle_admission_blocks \
         SET expires_at=statement_timestamp()-interval '1 millisecond' \
         WHERE allocation_id=$1",
    )
    .bind(allocation_id)
    .execute(fixture.operator_pool().pool())
    .await
    .expect("allocation expires");
    assert!(
        blocks
            .renew(
                allocation_id,
                node_id.as_uuid(),
                fence,
                Duration::seconds(10),
            )
            .await
            .expect("late renewal query")
            .is_none()
    );
}
