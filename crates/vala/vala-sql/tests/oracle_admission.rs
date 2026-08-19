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
