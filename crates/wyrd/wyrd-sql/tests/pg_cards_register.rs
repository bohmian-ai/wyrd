//! PgFixture coverage for composite card-registration persistence primitives.

use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::Card;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::{CardRef, scope_child_card_refs};
use wyrd_spec::registry::{ArtifactManifestEntry, RegistrationOperationId};
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, RECONCILE_KIND_REGISTRATION, claim_card_reconciliation,
    insert_artifact_manifest_rows, insert_card_row, insert_registration_operation,
    manifest_completion_rows, persist_outbound_relationships, recheck_active_card_refs,
    record_card_reconciliation_failure, soft_delete_card_by_ref, soft_delete_card_with_state,
};
use wyrd_sql::row_types::cards::CardStatus;

/// Build one resolved Prompt card fixture.
fn prompt_card(name: &str) -> Card {
    serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Prompt",
        "metadata": { "name": name, "version": "1.0.0", "space": "default" },
        "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
        "relationships": { "outbound": [], "inbound": [] }
    }))
    .expect("card fixture must deserialize")
}

/// Build one artifact manifest fixture.
fn artifact() -> ArtifactManifestEntry {
    serde_json::from_value(serde_json::json!({
        "relative_path": "prompt.txt",
        "sha256": "YQ==",
        "size_bytes": 1,
        "content_type": "text/plain"
    }))
    .expect("artifact fixture must deserialize")
}

/// Prove the consolidated migration contains every PR-one lifecycle column and table.
#[tokio::test]
async fn consolidated_registration_migration_has_locked_shape() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'wyrd' AND table_name = 'cards' \
           AND column_name IN ('registration_operation_id', 'pending_since', 'finalized_at', \
                               'card_blob_uri', 'blob_failed_at', \
                               'reconcile_kind', 'reconcile_status', 'reconcile_attempts', \
                               'reconcile_next_attempt_at', 'reconcile_lease_owner', \
                               'reconcile_lease_expires_at', 'reconcile_last_error_code', \
                               'reconcile_last_error_message', 'reconcile_dead_lettered_at') \
         ORDER BY column_name",
    )
    .fetch_all(fixture.operator_pool().pool())
    .await
    .expect("card columns read");

    assert_eq!(
        columns,
        vec![
            "blob_failed_at",
            "card_blob_uri",
            "finalized_at",
            "pending_since",
            "reconcile_attempts",
            "reconcile_dead_lettered_at",
            "reconcile_kind",
            "reconcile_last_error_code",
            "reconcile_last_error_message",
            "reconcile_lease_expires_at",
            "reconcile_lease_owner",
            "reconcile_next_attempt_at",
            "reconcile_status",
            "registration_operation_id",
        ]
    );
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'wyrd' \
           AND table_name IN ('card_registration_operations', 'card_artifact_manifest', 'card_relationships') \
         ORDER BY table_name",
    )
    .fetch_all(fixture.operator_pool().pool())
    .await
    .expect("registration tables read");
    assert_eq!(
        tables,
        vec![
            "card_artifact_manifest",
            "card_registration_operations",
            "card_relationships",
        ]
    );
}

/// Persist an operation, pending card, and manifest atomically through TenantConn.
#[tokio::test]
async fn pending_card_and_manifest_share_registration_transaction() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let principal_id = PrincipalId::new(Uuid::now_v7());
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let card_uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
    let card = prompt_card("sql-registration");
    let manifest = vec![artifact()];
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");

    assert!(
        insert_registration_operation(
            &mut conn,
            NewRegistrationOperation {
                operation_id,
                principal_id,
                idempotency_key: "sql-registration-001",
                request_hash: "request-hash",
            },
        )
        .await
        .expect("operation inserts")
    );
    let row = insert_card_row(
        &mut conn,
        NewCardRow {
            card: &card,
            card_uid,
            principal_id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "spec-hash",
            artifact_hash: Some("artifact-hash"),
        },
    )
    .await
    .expect("card inserts");
    insert_artifact_manifest_rows(&mut conn, &row.card_uid, &manifest)
        .await
        .expect("manifest inserts");
    conn.commit()
        .await
        .expect("registration transaction commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let stored: (
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, card_blob_uri, blob_failed_at FROM wyrd.cards WHERE card_uid = $1",
    )
    .bind(row.card_uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("stored card reads");
    assert_eq!(stored.0, "pending");
    assert_eq!(stored.1, None);
    assert_eq!(stored.2, None);
    let manifest_state: (String, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT upload_status, upload_id, verified_at \
             FROM wyrd.card_artifact_manifest WHERE card_uid = $1 AND relative_path = $2",
        )
        .bind(row.card_uid.as_uuid())
        .bind("prompt.txt")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("manifest row reads");
    assert_eq!(manifest_state.0, "awaiting_init");
    assert_eq!(manifest_state.1, None);
    assert_eq!(manifest_state.2, None);
    conn.commit().await.expect("assertion transaction commits");
}

/// Claims are serialized by `SKIP LOCKED`, recover after lease expiry, and stop
/// after the third failed attempt without a fourth claim.
#[tokio::test]
async fn reconciliation_claims_are_bounded_and_lease_safe() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let principal_id = PrincipalId::new(Uuid::now_v7());
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let card_uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
    let card = prompt_card("reconcile-bounded");
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    insert_registration_operation(
        &mut conn,
        NewRegistrationOperation {
            operation_id,
            principal_id,
            idempotency_key: "reconcile-bounded-001",
            request_hash: "reconcile-bounded-hash",
        },
    )
    .await
    .expect("operation inserts");
    insert_card_row(
        &mut conn,
        NewCardRow {
            card: &card,
            card_uid: card_uid.clone(),
            principal_id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "reconcile-bounded-spec",
            artifact_hash: None,
        },
    )
    .await
    .expect("card inserts");
    conn.commit().await.expect("setup commits");

    // A misleading host-derived reporting timestamp must not be able to defer
    // eligibility: registration seeds it from PostgreSQL's own clock.
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (pending_since_recorded, eligible_now, eligibility_is_independent) =
        sqlx::query_as::<_, (bool, bool, bool)>(
            "SELECT pending_since IS NOT NULL, \
                    reconcile_next_attempt_at <= statement_timestamp(), \
                    reconcile_next_attempt_at IS DISTINCT FROM pending_since \
               FROM wyrd.cards WHERE card_uid = $1",
        )
        .bind(card_uid.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("registration seeding is observable");
    conn.commit().await.expect("seed assertion commits");
    assert!(
        pending_since_recorded,
        "pending_since remains reporting data"
    );
    assert!(
        eligible_now,
        "a pending Card is immediately eligible in database time"
    );
    assert!(
        eligibility_is_independent,
        "eligibility must not be seeded from the host-derived pending_since"
    );

    let operator = fixture.operator_pool().clone();
    let (left, right) = tokio::join!(
        claim_card_reconciliation(&operator, 30, 32),
        claim_card_reconciliation(&operator, 30, 32),
    );
    let left = left.expect("left concurrent claim succeeds");
    let right = right.expect("right concurrent claim succeeds");
    assert_eq!(
        left.len() + right.len(),
        1,
        "one concurrent worker wins the claim"
    );
    let first = if left.is_empty() { right } else { left };
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].reconcile_kind, RECONCILE_KIND_REGISTRATION);
    assert_eq!(first[0].reconcile_attempts, 1);
    assert!(
        first[0].lease_remaining_seconds > 0.0 && first[0].lease_remaining_seconds <= 30.0,
        "the claim statement reports a positive lease remainder: {}",
        first[0].lease_remaining_seconds
    );

    let immediate = claim_card_reconciliation(&operator, 30, 32)
        .await
        .expect("second claim succeeds");
    assert!(immediate.is_empty(), "live lease must exclude the Card");

    expire_lease(&fixture, &card_uid).await;
    let crash_recovered = claim_card_reconciliation(&operator, 30, 32)
        .await
        .expect("expired lease is recoverable");
    assert_eq!(crash_recovered.len(), 1);
    assert_eq!(crash_recovered[0].reconcile_attempts, 2);
    let owner = crash_recovered[0].reconcile_lease_owner;

    // A requested retry delay, not a caller-supplied absolute instant, decides
    // when the next attempt becomes eligible.
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let dead = record_card_reconciliation_failure(
        &mut conn,
        &card_uid,
        owner,
        600,
        "WYRD_STORAGE_500_BACKEND",
        "Card blob persistence failed; retry the storage transition",
    )
    .await
    .expect("second failure records");
    assert!(!dead);
    conn.commit().await.expect("failure commits");
    assert!(
        claim_card_reconciliation(&operator, 30, 32)
            .await
            .expect("delayed claim succeeds")
            .is_empty(),
        "the requested retry delay must defer the next attempt"
    );

    make_retry_due(&fixture, &card_uid).await;
    let third = claim_card_reconciliation(&operator, 30, 32)
        .await
        .expect("third claim succeeds");
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].reconcile_attempts, 3);
    let owner = third[0].reconcile_lease_owner;

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let dead = record_card_reconciliation_failure(
        &mut conn,
        &card_uid,
        owner,
        1,
        "WYRD_STORAGE_500_BACKEND",
        "Card blob persistence failed; retry the storage transition",
    )
    .await
    .expect("third failure records");
    assert!(dead);
    conn.commit().await.expect("dead letter commits");

    let fourth = claim_card_reconciliation(&operator, 30, 32)
        .await
        .expect("bounded claim succeeds");
    assert!(
        fourth.is_empty(),
        "dead-lettered Card must not be claimed again"
    );
}

/// Age a Card's reconciliation lease in database time so a crashed worker's
/// claim becomes recoverable without sleeping or trusting the host clock.
async fn expire_lease(fixture: &PgFixture, card_uid: &CardUid) {
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE wyrd.cards \
            SET reconcile_lease_expires_at = statement_timestamp() - interval '1 second' \
          WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("lease ages");
    conn.commit().await.expect("lease aging commits");
}

/// Bring a deferred retry forward in database time so the next attempt is due.
async fn make_retry_due(fixture: &PgFixture, card_uid: &CardUid) {
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE wyrd.cards \
            SET reconcile_next_attempt_at = statement_timestamp() - interval '1 second' \
          WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("retry deadline ages");
    conn.commit().await.expect("retry aging commits");
}

/// `PostgreSQL`, not the Rust host clock, decides whether a manifest's upload
/// session is still live and resumable.
#[tokio::test]
async fn manifest_upload_liveness_is_decided_by_postgres() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let principal_id = PrincipalId::new(Uuid::now_v7());
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let card_uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
    let card = prompt_card("manifest-liveness");
    let upload_id = Uuid::now_v7();

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    insert_registration_operation(
        &mut conn,
        NewRegistrationOperation {
            operation_id,
            principal_id,
            idempotency_key: "manifest-liveness-001",
            request_hash: "manifest-liveness-hash",
        },
    )
    .await
    .expect("operation inserts");
    insert_card_row(
        &mut conn,
        NewCardRow {
            card: &card,
            card_uid: card_uid.clone(),
            principal_id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "manifest-liveness-spec",
            artifact_hash: None,
        },
    )
    .await
    .expect("card inserts");
    insert_artifact_manifest_rows(&mut conn, &card_uid, &[artifact()])
        .await
        .expect("manifest inserts");
    sqlx::query(
        "INSERT INTO wyrd.storage_multipart_uploads \
             (id, data_tenant_id, card_uid, relative_path, storage_path, backend, \
              wire_protocol, expected_sha256, expected_size_bytes, part_count_planned, \
              part_size_bytes, status, expires_at) \
         VALUES ($1, wyrd.current_tenant(), $2, 'prompt.txt', 'objects/prompt.txt', 'local', \
                 'local_fs_v1', 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=', \
                 1, 1, 1, 'pending', \
                 statement_timestamp() + interval '1 hour')",
    )
    .bind(upload_id)
    .bind(card_uid.as_str())
    .execute(&mut **conn.transaction())
    .await
    .expect("upload row inserts");
    sqlx::query(
        "UPDATE wyrd.card_artifact_manifest SET upload_id = $2, upload_status = 'pending' \
          WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .bind(upload_id)
    .execute(&mut **conn.transaction())
    .await
    .expect("manifest links the upload");
    conn.commit().await.expect("setup commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let live = manifest_completion_rows(&mut conn, &card_uid)
        .await
        .expect("manifest projection loads");
    conn.commit().await.expect("live read commits");
    assert_eq!(live.len(), 1);
    assert!(
        live[0].storage_upload_live,
        "an unexpired pending upload is live"
    );

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE wyrd.storage_multipart_uploads \
            SET expires_at = statement_timestamp() - interval '1 second' WHERE id = $1",
    )
    .bind(upload_id)
    .execute(&mut **conn.transaction())
    .await
    .expect("upload session ages");
    let expired = manifest_completion_rows(&mut conn, &card_uid)
        .await
        .expect("manifest projection reloads");
    conn.commit().await.expect("expired read commits");
    assert!(
        !expired[0].storage_upload_live,
        "an expired upload session is not live"
    );
}

/// Persist a UID-bearing outbound edge atomically with its source Card row.
#[tokio::test]
async fn relationship_rows_preserve_exact_target_identity() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let principal_id = PrincipalId::new(Uuid::now_v7());
    let target_operation = RegistrationOperationId::new(Uuid::now_v7());
    let source_operation = RegistrationOperationId::new(Uuid::now_v7());
    let target_uid = CardUid::from_uuid(Uuid::now_v7()).expect("target UUIDv7 is valid");
    let source_uid = CardUid::from_uuid(Uuid::now_v7()).expect("source UUIDv7 is valid");
    let target = prompt_card("relationship-target");
    let source: Card = serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Agent",
        "metadata": { "name": "relationship-source", "version": "1.0.0", "space": "default" },
        "spec": { "prompt": {
            "kind": "Prompt",
            "name": "relationship-target",
            "version": "1.0.0",
            "space": "default",
            "uid": target_uid
        }}
    }))
    .expect("agent fixture must deserialize");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    for (operation_id, key) in [
        (target_operation, "relationship-target-operation"),
        (source_operation, "relationship-source-operation"),
    ] {
        assert!(
            insert_registration_operation(
                &mut conn,
                NewRegistrationOperation {
                    operation_id,
                    principal_id,
                    idempotency_key: key,
                    request_hash: key,
                },
            )
            .await
            .expect("operation inserts")
        );
    }
    insert_card_row(
        &mut conn,
        NewCardRow {
            card: &target,
            card_uid: target_uid.clone(),
            principal_id,
            operation_id: target_operation,
            status: CardStatus::Active,
            spec_hash: "target-spec-hash",
            artifact_hash: None,
        },
    )
    .await
    .expect("target card inserts");
    insert_card_row(
        &mut conn,
        NewCardRow {
            card: &source,
            card_uid: source_uid.clone(),
            principal_id,
            operation_id: source_operation,
            status: CardStatus::Pending,
            spec_hash: "source-spec-hash",
            artifact_hash: None,
        },
    )
    .await
    .expect("source card inserts");
    persist_outbound_relationships(&mut conn, &source_uid, &scope_child_card_refs(&source.spec))
        .await
        .expect("relationship inserts");
    conn.commit()
        .await
        .expect("registration transaction commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let stored: (String, String, String, String, Uuid) = sqlx::query_as(
        "SELECT target_kind, target_space, target_name, target_version, target_uid \
         FROM wyrd.card_relationships WHERE card_uid = $1",
    )
    .bind(source_uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("relationship row reads");
    assert_eq!(stored.0, "Prompt");
    assert_eq!(stored.1, "default");
    assert_eq!(stored.2, "relationship-target");
    assert_eq!(stored.3, "1.0.0");
    assert_eq!(stored.4, target_uid.as_uuid());
    conn.commit().await.expect("assertion transaction commits");
}

/// Hold the target lifecycle row lock until the relationship transaction commits.
#[tokio::test]
async fn relationship_recheck_blocks_target_lifecycle_race() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let principal_id = PrincipalId::new(Uuid::now_v7());
    let target_operation = RegistrationOperationId::new(Uuid::now_v7());
    let source_operation = RegistrationOperationId::new(Uuid::now_v7());
    let target_uid = CardUid::from_uuid(Uuid::now_v7()).expect("target UUIDv7 is valid");
    let source_uid = CardUid::from_uuid(Uuid::now_v7()).expect("source UUIDv7 is valid");
    let target = prompt_card("race-target");
    let source: Card = serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Agent",
        "metadata": { "name": "race-source", "version": "1.0.0", "space": "default" },
        "spec": { "prompt": {
            "kind": "Prompt",
            "name": "race-target",
            "version": "1.0.0",
            "space": "default"
        }}
    }))
    .expect("agent fixture must deserialize");

    let mut setup_conn = fixture.tenant_conn().await.expect("setup connection opens");
    for (operation_id, key) in [
        (target_operation, "race-target-operation"),
        (source_operation, "race-source-operation"),
    ] {
        assert!(
            insert_registration_operation(
                &mut setup_conn,
                NewRegistrationOperation {
                    operation_id,
                    principal_id,
                    idempotency_key: key,
                    request_hash: key,
                },
            )
            .await
            .expect("operation inserts")
        );
    }
    insert_card_row(
        &mut setup_conn,
        NewCardRow {
            card: &target,
            card_uid: target_uid.clone(),
            principal_id,
            operation_id: target_operation,
            status: CardStatus::Active,
            spec_hash: "race-target-spec-hash",
            artifact_hash: None,
        },
    )
    .await
    .expect("target card inserts");
    insert_card_row(
        &mut setup_conn,
        NewCardRow {
            card: &source,
            card_uid: source_uid.clone(),
            principal_id,
            operation_id: source_operation,
            status: CardStatus::Pending,
            spec_hash: "race-source-spec-hash",
            artifact_hash: None,
        },
    )
    .await
    .expect("source card inserts");
    setup_conn.commit().await.expect("setup commits");

    let target_ref: CardRef = serde_json::from_value(serde_json::json!({
        "kind": "Prompt",
        "name": "race-target",
        "version": "1.0.0",
        "space": "default"
    }))
    .expect("target reference decodes");
    let mut registration_conn = fixture
        .tenant_conn()
        .await
        .expect("registration connection opens");
    let resolved = recheck_active_card_refs(&mut registration_conn, &[target_ref])
        .await
        .expect("active target rechecks");
    assert_eq!(resolved[0].1, target_uid);
    let resolved_refs = resolved
        .iter()
        .map(|(card_ref, _)| card_ref.clone())
        .collect::<Vec<_>>();
    persist_outbound_relationships(&mut registration_conn, &source_uid, &resolved_refs)
        .await
        .expect("relationship inserts");

    let mut lifecycle_conn = fixture
        .tenant_conn()
        .await
        .expect("lifecycle connection opens");
    sqlx::query("SET LOCAL lock_timeout = '100ms'")
        .execute(&mut **lifecycle_conn.transaction())
        .await
        .expect("lock timeout configures");
    let lock_error = sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(target_uid.as_uuid())
        .execute(&mut **lifecycle_conn.transaction())
        .await
        .expect_err("target lifecycle update waits for registration lock");
    assert_eq!(
        lock_error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref(),
        Some("55P03")
    );

    drop(lifecycle_conn);
    registration_conn
        .commit()
        .await
        .expect("registration commits before lifecycle update");
    let mut lifecycle_conn = fixture
        .tenant_conn()
        .await
        .expect("lifecycle retry connection opens");
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(target_uid.as_uuid())
        .execute(&mut **lifecycle_conn.transaction())
        .await
        .expect("lifecycle update proceeds after registration commit");
    lifecycle_conn.commit().await.expect("lifecycle commits");

    let mut verify_conn = fixture
        .tenant_conn()
        .await
        .expect("verification connection opens");
    let status: String = sqlx::query_scalar("SELECT status FROM wyrd.cards WHERE card_uid = $1")
        .bind(target_uid.as_uuid())
        .fetch_one(&mut **verify_conn.transaction())
        .await
        .expect("target status reads");
    let relationship_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.card_relationships \
         WHERE card_uid = $1 AND target_uid = $2",
    )
    .bind(source_uid.as_uuid())
    .bind(target_uid.as_uuid())
    .fetch_one(&mut **verify_conn.transaction())
    .await
    .expect("relationship reads");
    assert_eq!(status, "deleted");
    assert_eq!(relationship_count, 1);
    verify_conn.commit().await.expect("verification commits");
}

/// Exact delete blocks visible inbound references, preserves tenant parity, and retries idempotently.
#[tokio::test]
async fn exact_delete_enforces_inbound_references_and_is_idempotent() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let other_tenant = fixture
        .seed_additional_tenant("delete-other-tenant")
        .await
        .expect("second tenant seeds");
    let principal = Principal::new(
        PrincipalId::new(Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    );
    let target_operation = RegistrationOperationId::new(Uuid::now_v7());
    let source_operation = RegistrationOperationId::new(Uuid::now_v7());
    let target_uid = CardUid::from_uuid(Uuid::now_v7()).expect("target UID is valid");
    let source_uid = CardUid::from_uuid(Uuid::now_v7()).expect("source UID is valid");
    let target = prompt_card("delete-target");
    let source: Card = serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Agent",
        "metadata": { "name": "delete-source", "version": "1.0.0", "space": "default" },
        "spec": { "prompt": {
            "kind": "Prompt",
            "name": "delete-target",
            "version": "1.0.0",
            "space": "default",
            "uid": target_uid
        }}
    }))
    .expect("source fixture deserializes");
    let target_ref = CardRef {
        kind: wyrd_spec::envelope::CardKind::Prompt,
        name: CardName::new("delete-target").expect("target name is valid"),
        version: VersionBlock::parse("1.0.0").expect("target version is valid"),
        space: Some(SpaceName::new("default").expect("target space is valid")),
        uid: Some(target_uid.clone()),
    };
    let target_spec_hash = "a".repeat(64);
    let source_spec_hash = "b".repeat(64);

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    for (operation_id, key) in [
        (target_operation, "delete-target-operation"),
        (source_operation, "delete-source-operation"),
    ] {
        assert!(
            insert_registration_operation(
                &mut conn,
                NewRegistrationOperation {
                    operation_id,
                    principal_id: principal.id,
                    idempotency_key: key,
                    request_hash: key,
                },
            )
            .await
            .expect("operation inserts")
        );
    }
    insert_card_row(
        &mut conn,
        NewCardRow {
            card: &target,
            card_uid: target_uid.clone(),
            principal_id: principal.id,
            operation_id: target_operation,
            status: CardStatus::Active,
            spec_hash: target_spec_hash.as_str(),
            artifact_hash: None,
        },
    )
    .await
    .expect("target card inserts");
    insert_card_row(
        &mut conn,
        NewCardRow {
            card: &source,
            card_uid: source_uid.clone(),
            principal_id: principal.id,
            operation_id: source_operation,
            status: CardStatus::Active,
            spec_hash: source_spec_hash.as_str(),
            artifact_hash: None,
        },
    )
    .await
    .expect("source card inserts");
    persist_outbound_relationships(&mut conn, &source_uid, std::slice::from_ref(&target_ref))
        .await
        .expect("inbound relationship inserts");
    conn.commit().await.expect("setup commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let error = soft_delete_card_with_state(&mut conn, &target_uid)
        .await
        .expect_err("visible inbound reference blocks delete");
    assert_eq!(error.status(), 409);
    drop(conn);

    let mut conn = fixture
        .tenant_conn_for(other_tenant)
        .await
        .expect("other tenant connection opens");
    let error = soft_delete_card_with_state(&mut conn, &target_uid)
        .await
        .expect_err("cross-tenant UID must not resolve");
    assert_eq!(error.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    drop(conn);

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(source_uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("source tombstone commits");
    conn.commit().await.expect("source tombstone commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let deleted = soft_delete_card_by_ref(&mut conn, &target_ref)
        .await
        .expect("exact reference deletes target");
    assert!(deleted.deleted);
    conn.commit().await.expect("delete commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let replay = soft_delete_card_with_state(&mut conn, &target_uid)
        .await
        .expect("repeated delete is idempotent");
    assert!(!replay.deleted);
    conn.commit().await.expect("replay commits");

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let status: String = sqlx::query_scalar("SELECT status FROM wyrd.cards WHERE card_uid = $1")
        .bind(target_uid.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("deleted card reads");
    assert_eq!(status, "deleted");
    conn.commit().await.expect("assertion transaction commits");
}
