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
async fn register_oracle(fixture: &PgFixture, tenant: DataTenantId, node_id: NodeId) {
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
        .tenant_conn(tenant)
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
}

/// Canonical insertion is idempotent and conflicting pod configuration fails.
///
/// # Panics
///
/// Panics when the isolated PostgreSQL fixture or asserted SQL behavior fails.
#[tokio::test]
async fn canonical_policy_is_idempotent_and_conflict_fails() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(
            tenant,
            &format!("admission-{}", tenant.as_uuid().simple()),
        )
        .await
        .expect("tenant seeds");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_policy(None, QueryClass::Interactive, 4)
        .await
        .expect("global policy");
    blocks
        .ensure_policy(None, QueryClass::Interactive, 4)
        .await
        .expect("idempotent global policy");
    blocks
        .ensure_policy(Some(tenant.into()), QueryClass::Interactive, 2)
        .await
        .expect("tenant policy");
    assert!(
        blocks
            .ensure_policy(Some(tenant.into()), QueryClass::Interactive, 3)
            .await
            .is_err()
    );
}

/// Production startup initializes every active tenant atomically and idempotently.
///
/// # Panics
///
/// Panics when startup policy initialization, inspection, or conflict rollback fails.
#[tokio::test]
async fn startup_policies_cover_active_tenants_and_reject_conflict() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let second = fixture
        .seed_additional_tenant("startup-second")
        .await
        .expect("second tenant seeds");
    let blocks = OracleAdmissionBlocks::new(fixture.operator_pool());
    blocks
        .ensure_startup_policies(4, 2, 3, 1)
        .await
        .expect("first startup initializes policies");
    blocks
        .ensure_startup_policies(4, 2, 3, 1)
        .await
        .expect("repeated startup is idempotent");
    let active_tenants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.tenants WHERE status='active' AND deleted_at IS NULL",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("active tenants count");
    let tenant_policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_policies WHERE scope_kind='tenant'",
    )
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("tenant policy count");
    assert_eq!(tenant_policies, active_tenants * 2);
    assert!(
        blocks.ensure_startup_policies(5, 2, 3, 1).await.is_err(),
        "conflicting startup must fail readiness initialization"
    );
    let retained: i32 = sqlx::query_scalar(
        "SELECT capacity FROM vala.oracle_admission_policies \
         WHERE scope_kind='tenant' AND data_tenant_id=$1 AND query_class='interactive'",
    )
    .bind(second.as_uuid())
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("second tenant policy remains canonical");
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
        .ensure_policy(None, QueryClass::Interactive, 2)
        .await
        .expect("global policy");
    blocks
        .ensure_policy(Some(tenant.into()), QueryClass::Interactive, 2)
        .await
        .expect("tenant policy");
    let node_id = NodeId::new(uuid::Uuid::from_u128(1));
    register_oracle(&fixture, tenant, node_id).await;
    let demand = |node| OracleAdmissionDemand {
        tenant_id: tenant,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 2,
        holder_node_id: NodeId::new(uuid::Uuid::from_u128(node)),
        holder_fencing_token: 1,
    };
    let first = blocks
        .allocate(demand(1), Duration::seconds(15))
        .await
        .expect("first allocation");
    assert_eq!(first.len(), 3);
    assert!(first.iter().all(|row| row.units == 2));
    let second = blocks
        .allocate(demand(1), Duration::seconds(15))
        .await
        .expect("bounded second allocation");
    assert!(second.is_empty());
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
        .ensure_policy(None, QueryClass::Interactive, 1)
        .await
        .expect("global policy");
    blocks
        .ensure_policy(Some(tenant.into()), QueryClass::Interactive, 1)
        .await
        .expect("tenant policy");
    let node_id = NodeId::new(uuid::Uuid::from_u128(7));
    register_oracle(&fixture, tenant, node_id).await;
    let demand = OracleAdmissionDemand {
        tenant_id: tenant,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        query_class: QueryClass::Interactive,
        requested_units: 1,
        holder_node_id: node_id,
        holder_fencing_token: 1,
    };
    let allocated = blocks
        .allocate(demand, Duration::seconds(2))
        .await
        .expect("allocation");
    let allocation_id = allocated[0].allocation_id;
    let first = blocks
        .renew(allocation_id, node_id.as_uuid(), 1, Duration::seconds(10))
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
        .renew(allocation_id, node_id.as_uuid(), 1, Duration::seconds(10))
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
            .renew(allocation_id, node_id.as_uuid(), 1, Duration::seconds(10))
            .await
            .expect("late renewal query")
            .is_none()
    );
}
