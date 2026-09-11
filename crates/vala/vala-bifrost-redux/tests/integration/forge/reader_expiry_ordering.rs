//! Tier-2 coverage for the one table-local winner between reader widening and
//! prepared snapshot expiration.
//!
//! Both owners take the same `vala.bifrost_table_maintenance_authority` row.
//! Whichever takes it first wins for the snapshots it names: a prepared claim
//! makes the reader re-resolve before any source IO, and a surviving protection
//! header makes preparation refuse. Neither owner may widen the boundary to the
//! whole table, so an unrelated table and an unrelated cut still progress.

use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{
    ForgeClaimTable, ForgeExpirationAuthority, ForgeExpirationPreparation, ForgeOperationFamily,
};
use vala_sql::row_types::forge_tasks::ForgeTaskEvidence;
use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, ForgeSnapshotExpirePhase, StoragePath};

use crate::oracle::reader_authority::{AuthorityFixture, cut};

/// Canonical Forge resource string for one registered table.
fn resource_for(identity: &TableAuthorityIdentity) -> String {
    format!(
        "bifrost://{}/{}/{}",
        identity.tenant, identity.namespace_name, identity.table_name
    )
}

/// Seeds one live Forge lease and one running snapshot-expiry task.
///
/// # Panics
///
/// Panics when either seeding statement fails.
async fn seed_running_expiry_task(
    fixture: &AuthorityFixture,
    tenant: DataTenantId,
    identity: &TableAuthorityIdentity,
    base_snapshot_id: i64,
    plan_hash_byte: &str,
) -> ForgeExpirationAuthority {
    let authority = ForgeExpirationAuthority {
        task_id: Uuid::now_v7(),
        attempt_id: Uuid::now_v7(),
        worker_id: Uuid::now_v7(),
        lease_key: format!("forge:{}:{}", identity.table_name, base_snapshot_id),
        lease_fencing_token: base_snapshot_id,
    };
    let pool = fixture.database.operator_pool().pool();
    sqlx::query("INSERT INTO vala.maintenance_leases (lease_key,owner,fencing_token,expires_at,heartbeat_at) VALUES ($1,$2,$3,now()+interval '10 minutes',now())")
        .bind(&authority.lease_key)
        .bind(authority.worker_id)
        .bind(authority.lease_fencing_token)
        .execute(pool)
        .await
        .expect("lease seeds");
    sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry',$8,'{}'::jsonb,decode(repeat($9,32),'hex'),1,1,'running',$6,$7,now()+interval '10 minutes',$8,1,now())")
        .bind(authority.task_id)
        .bind(tenant.as_uuid())
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .bind(authority.attempt_id)
        .bind(authority.worker_id)
        .bind(base_snapshot_id)
        .bind(plan_hash_byte)
        .execute(pool)
        .await
        .expect("running task seeds");
    authority
}

/// Prepares one snapshot expiration for the exact selection.
///
/// # Errors
///
/// Returns the SQL owner's refusal when a surviving protection frontier covers
/// a selected snapshot or the claim collides with another operation.
///
/// # Panics
///
/// Panics when the fixed resource cannot construct the owner.
async fn prepare_expiration(
    fixture: &AuthorityFixture,
    identity: &TableAuthorityIdentity,
    authority: &ForgeExpirationAuthority,
    selected: &[i64],
) -> Result<(), vala_sql::SqlError> {
    let resource = resource_for(identity);
    let detail = AuditDetail::ForgeSnapshotExpire {
        operation_id: Uuid::now_v7(),
        phase: ForgeSnapshotExpirePhase::Prepared,
        group: resource.clone(),
        base_metadata_location: StoragePath::new("table/iceberg/metadata/00001-base.json")
            .expect("valid path"),
        current_snapshot_id: Some(90),
        retained_ref_heads: vec![90],
        cutoff_ms: 1_700_000_000_000,
        selected_snapshot_ids: selected.to_vec(),
    };
    let table = ForgeClaimTable {
        table_uid: identity.table_uid,
        catalog_name: identity.catalog_name.clone(),
        namespace_name: identity.namespace_name.clone(),
        table_name: identity.table_name.clone(),
        table_uuid: Uuid::now_v7(),
    };
    let evidence = ForgeTaskEvidence {
        prepared_candidate_index: None,
        version: 1,
        committed_snapshot_id: None,
        committed_metadata_location: None,
        committed_metadata_digest: None,
        cleanup_candidates: Vec::new(),
        deleted_candidate_count: 0,
    };
    ForgeOperations::new(&resource, ForgeOperationFamily::SnapshotExpire)
        .expect("valid Forge resource")
        .prepare_snapshot_expiration(
            fixture.database.operator_pool(),
            identity.tenant,
            &ForgeExpirationPreparation {
                authority,
                table: &table,
                evidence: &evidence,
                operation: "forge.snapshot_expire.prepared",
                detail: &detail,
            },
        )
        .await
        .map(|_| ())
}

/// Proves both lock winners, per-snapshot rather than per-table exclusion, and
/// that a surviving header protects even after its epoch lost its lease.
///
/// # Panics
///
/// Panics on any admission, refusal, or durable-state mismatch.
#[tokio::test]
async fn reader_and_expiration_claim_have_one_table_local_winner() {
    let fixture = AuthorityFixture::start().await;
    let (authority, _terminator) = fixture.authority(8).await;
    let tenant = fixture.tenant().await;
    let events = fixture.table(tenant, "events").await;
    let orders = fixture.table(tenant, "orders").await;

    // --- Forge takes the table row first --------------------------------
    let expiring = seed_running_expiry_task(&fixture, tenant, &events, 20, "00").await;
    prepare_expiration(&fixture, &events, &expiring, &[30])
        .await
        .expect("first preparation claims snapshot 30");

    // A cut whose frontier protects the claimed snapshot loses, and it loses
    // before any source IO.
    let refused = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await;
    assert!(
        refused.is_err(),
        "a claimed snapshot refuses reader widening"
    );
    assert!(
        fixture.header(&events).await.is_none(),
        "the refused admission published no protection header"
    );

    // The boundary is per protected snapshot, not per table: a newer cut whose
    // frontier does not protect the claimed snapshot, and an unrelated table,
    // both still progress.
    let _clear_cut = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(40, 400, &[40, 30]))])
        .await
        .expect("a cut that avoids the claim still admits");
    let _other_table = authority
        .acquire_guard_for_cuts(vec![(orders.clone(), cut(70, 700, &[70, 60]))])
        .await
        .expect("an unrelated table still admits");
    assert!(
        fixture.header(&events).await.is_some(),
        "the admitted cut published its protection header"
    );

    // --- the reader takes the table row first ---------------------------
    // `orders` widened first, so its surviving header refuses a preparation
    // over the exact snapshot it protects.
    let contested = seed_running_expiry_task(&fixture, tenant, &orders, 21, "11").await;
    let lost = prepare_expiration(&fixture, &orders, &contested, &[70]).await;
    assert!(
        lost.is_err(),
        "a surviving protection frontier refuses preparation over its snapshot"
    );

    // A header remains a root regardless of its epoch's liveness: only its
    // durable removal makes it absent.
    authority.collapse_lease_for_test();
    let still_lost = prepare_expiration(&fixture, &orders, &contested, &[70]).await;
    assert!(
        still_lost.is_err(),
        "an epoch that lost its lease still protects through its surviving header"
    );
    assert!(
        fixture.header(&orders).await.is_some(),
        "the surviving header was never removed"
    );
}
