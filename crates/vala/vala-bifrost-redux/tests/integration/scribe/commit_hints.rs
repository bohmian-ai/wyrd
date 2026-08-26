//! Which commit outcomes publish a hint, and how an unknown commit
//! reconciles to exactly one generation.
//!
//! Module of the `scribe` group; shared fixtures live in `persistence_support.rs`.

use super::persistence_support::*;

/// Poll the fixture's bounded wake-up inbox after persistence settles.
///
/// # Errors
/// Returns the channel's empty or closed status when no advisory signal is available.
fn hint_outcome(
    fixture: &mut PersistenceFixture,
) -> Result<
    vala_bifrost_redux::maintenance::StagingFileCommitted,
    tokio::sync::mpsc::error::TryRecvError,
> {
    fixture.hint_inbox.try_recv_for_test()
}

/// Returns ordered audit operation, request, resource, and principal identities.
///
/// # Panics
///
/// Panics when the repository-managed tenant connection or audit query fails.
async fn audit_identities(
    fixture: &PersistenceFixture,
) -> Vec<(String, String, String, uuid::Uuid)> {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_as(
        "SELECT operation, request_id, resource, principal_id
           FROM vala.audit_outbox
          WHERE operation IN ('bifrost.append', 'bifrost.scribe.visibility.publish')
          ORDER BY seq",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("ordered audit identities")
}

/// A confirmed file-list transaction publishes exactly one advisory wake-up.
#[tokio::test]
async fn confirmed_commit_publishes_hint() {
    let mut fixture = PersistenceFixture::start().await;
    append_one(&fixture, "confirmed_hint_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("confirmed flush");
    wait_for_state(&fixture, 0).await;

    assert!(hint_outcome(&mut fixture).is_ok());
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    fixture.stop().await;
}

/// A lost automatic COMMIT response transfers the exact generation into the
/// runtime reconciler and completes without duplicate rows, audit operations,
/// or objects.
#[tokio::test]
async fn pg_multi_candidate_unknown_commit_reconciles_one_generation() {
    let mut fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_post_commit_response();
    append_one(&fixture, "automatic_ambiguous_commit_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("ambiguous automatic flush");
    wait_for_state(&fixture, 0).await;

    assert_eq!(
        rows_for_table(&fixture, "automatic_ambiguous_commit_events")
            .await
            .len(),
        1,
        "fenced retry validates the already-committed row"
    );
    let artifacts = artifact_rows_for_table(&fixture, "automatic_ambiguous_commit_events").await;
    assert_eq!(artifacts.len(), 1, "retry preserves one artifact set");
    assert_catalog_object_parity(&fixture, &artifacts).await;
    assert_eq!(
        audit_counts(&fixture).await,
        (1, 1),
        "retry preserves one ingest audit and one publication audit"
    );
    let audits = audit_identities(&fixture).await;
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0].0, "bifrost.append");
    assert_eq!(audits[1].0, "bifrost.scribe.visibility.publish");
    assert_eq!(
        (&audits[0].1, &audits[0].2, &audits[0].3),
        (&audits[1].1, &audits[1].2, &audits[1].3),
        "publication retry must retain the ingest request, resource, and principal identity"
    );
    assert_eq!(object_paths(&fixture).await.len(), 1);
    assert!(hint_outcome(&mut fixture).is_ok());
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    fixture.stop().await;
}

/// Pre-commit object and SQL failures publish no wake-up signal.
#[tokio::test]
async fn precommit_and_commit_failure_publish_no_hint() {
    let mut fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_object_write();
    append_one(&fixture, "precommit_hint_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("object failure flush");
    wait_for_state(&fixture, 1).await;
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));

    retry_and_wait(&fixture).await;
    let _ = hint_outcome(&mut fixture);
    fixture.faults.fail_next_sql_commit();
    append_one(&fixture, "commit_hint_events", 2).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("SQL failure flush");
    wait_for_state(&fixture, 1).await;
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    fixture.stop().await;
}

/// A manifest failure after SQL commit still publishes the durable-row hint.
#[tokio::test]
async fn manifest_failure_after_commit_still_publishes_hint() {
    let mut fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_manifest_publication();
    append_one(&fixture, "manifest_hint_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("manifest failure flush");
    wait_for_state(&fixture, 1).await;
    assert_eq!(
        rows_for_table(&fixture, "manifest_hint_events").await.len(),
        1
    );
    assert!(hint_outcome(&mut fixture).is_ok());
    fixture.stop().await;
}

/// A post-commit manifest failure retains exactly one ingest and publication audit.
#[tokio::test]
async fn manifest_failure_retains_retryable_front() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_manifest_publication();
    append_one(&fixture, "manifest_failure_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("manifest failure flush");
    wait_for_state(&fixture, 1).await;
    assert_eq!(
        rows(&fixture).await.len(),
        1,
        "SQL commits before manifest failure"
    );
    assert_eq!(audit_counts(&fixture).await, (1, 1));

    retry_and_wait(&fixture).await;
    assert_eq!(
        rows(&fixture).await.len(),
        1,
        "manifest retry must dedupe SQL"
    );
    assert_eq!(
        audit_counts(&fixture).await,
        (1, 1),
        "manifest retry must not duplicate either audit operation"
    );
    fixture.stop().await;
}
