//! PgFixture coverage for composite card-registration persistence primitives.

use std::env;

use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::envelope::Card;
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::{ArtifactManifestEntry, RegistrationOperationId};
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_artifact_manifest_rows, insert_card_row,
    insert_registration_operation,
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
           AND table_name IN ('card_registration_operations', 'card_artifact_manifest') \
         ORDER BY table_name",
    )
    .fetch_all(fixture.platform_admin_pool())
    .await
    .expect("registration tables read");
    assert_eq!(
        tables,
        vec!["card_artifact_manifest", "card_registration_operations"]
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
