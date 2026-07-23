//! PgFixture coverage for composite card-registration persistence primitives.

use std::env;

use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::envelope::Card;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::{CardRef, scope_child_card_refs};
use wyrd_spec::registry::{ArtifactManifestEntry, RegistrationOperationId};
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_artifact_manifest_rows, insert_card_row,
    insert_registration_operation, persist_outbound_relationships, recheck_active_card_refs,
};
use wyrd_sql::row_types::cards::CardStatus;

/// Return whether live Postgres registration tests are enabled.
fn enabled() -> bool {
    env::var("WYRD_REG_E2E").as_deref() == Ok("1")
}

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
    if !enabled() {
        return;
    }
    let fixture = PgFixture::start().await.expect("fixture starts");
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'wyrd' AND table_name = 'cards' \
           AND column_name IN ('registration_operation_id', 'pending_since', 'finalized_at', \
                               'card_blob_uri', 'blob_failed_at', 'blob_status') \
         ORDER BY column_name",
    )
    .fetch_all(fixture.platform_admin_pool())
    .await
    .expect("card columns read");

    assert_eq!(
        columns,
        vec![
            "blob_failed_at",
            "card_blob_uri",
            "finalized_at",
            "pending_since",
            "registration_operation_id",
        ]
    );
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'wyrd' \
           AND table_name IN ('card_registration_operations', 'card_artifact_manifest', 'card_relationships') \
         ORDER BY table_name",
    )
    .fetch_all(fixture.platform_admin_pool())
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
    if !enabled() {
        return;
    }
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

/// Persist a UID-bearing outbound edge atomically with its source Card row.
#[tokio::test]
async fn relationship_rows_preserve_exact_target_identity() {
    if !enabled() {
        return;
    }
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
    if !enabled() {
        return;
    }
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
