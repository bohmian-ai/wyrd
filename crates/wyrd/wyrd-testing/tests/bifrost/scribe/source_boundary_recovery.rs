use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use wyrd_testing::WyrdTestServer;

use super::support::{
    append_values, published_rows, register_table, sorted_values, tenant_client, unique_table,
};

/// Failure, retry, and replay never change row identity.
///
/// Three things can make a Scribe pod write a row twice or lose one: a client
/// that resends the same batch, a publication that fails partway, and a
/// publication that actually committed while the caller was told it did not.
/// This owner drives all three against one table and holds the same invariant
/// across every one of them — a strict public read returns exactly the rows the
/// client sent, once each, and the published objects account for exactly those
/// rows.
///
/// The failure injection is at the two real publication seams: the object write
/// (nothing committed) and the response after `COMMIT` (committed, but the
/// caller cannot know it). The second is the harder one: a retry must reconcile
/// the already-identical publication rather than publish the same members
/// again.
///
/// # Panics
///
/// Panics when a public append or read fails, when a failed publication loses
/// rows or publishes anyway, when a retried publication duplicates rows, or
/// when a replayed batch adds rows a second time.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_failure_retry_replay_remain_atomic() {
    let faults = PersistenceFaults::default();
    let server = WyrdTestServer::builder()
        .with_scribe_persistence_faults_for_test(faults.clone())
        .start_bound()
        .await
        .expect("the Scribe production harness starts with publication faults");
    let tenant = server.data_tenant_id();
    let name = unique_table("retry_replay");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    // A client that resends the identical batch is retrying, not writing more.
    let first_batch = uuid::Uuid::now_v7();
    let first: Vec<i64> = (0..32).collect();
    append_values(&client, &table, first_batch, &first)
        .await
        .expect("the first append is acknowledged");
    append_values(&client, &table, first_batch, &first)
        .await
        .expect("an identical replayed batch is acknowledged");
    assert_eq!(
        sorted_values(&client, &table).await,
        first,
        "a replayed batch must not duplicate its rows at the live source"
    );

    // A publication whose object write fails must leave the rows exactly where
    // they were: readable, unpublished, and still owned by Scribe.
    faults.fail_next_object_write();
    server
        .flush_bifrost()
        .await
        .expect_err("a failed object write fails the publication");
    assert_eq!(
        sorted_values(&client, &table).await,
        first,
        "a failed publication may not lose or duplicate an acknowledged row"
    );
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        0,
        "a publication that failed its object write may not commit a hot object"
    );

    // The retry publishes those same rows exactly once.
    server
        .flush_bifrost()
        .await
        .expect("the retry publishes the retained members");
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        first.len() as u64,
        "the retry must publish the retained rows exactly once"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        first,
        "the published objects must read back exactly the acknowledged rows"
    );

    // Ambiguity: the fenced transaction commits and the caller is told it
    // failed. The rows are already published, so the retry must reconcile the
    // identical publication instead of committing it a second time.
    let second_batch = uuid::Uuid::now_v7();
    let second: Vec<i64> = (100..132).collect();
    append_values(&client, &table, second_batch, &second)
        .await
        .expect("the second append is acknowledged");
    let mut expected: Vec<i64> = first.iter().chain(second.iter()).copied().collect();
    expected.sort_unstable();

    let audits_before = publication_audits(&server, tenant, &table).await;
    faults.fail_next_post_commit_response();
    let ambiguous = server
        .flush_bifrost()
        .await
        .expect_err("the committed publication's lost response must surface as a failure");
    // The seam under test only exists when the injected fault actually fires:
    // an ordinary successful publication would satisfy every remaining
    // assertion in this block while proving nothing about uncertain commits.
    assert!(
        ambiguous
            .to_string()
            .contains("test client lost the committed publication response"),
        "the armed flush must return the injected unknown-commit outcome, not a \
         different failure: {ambiguous:?}"
    );

    // The transaction did commit. Its rows must already be durable, once each,
    // even though the caller was told the publication failed.
    let committed = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable after the lost response");
    assert_eq!(
        committed.iter().map(|file| file.row_count).sum::<u64>(),
        expected.len() as u64,
        "the committed-but-unacknowledged publication must be durably visible exactly once"
    );
    assert_eq!(
        publication_audits(&server, tenant, &table).await,
        audits_before + 1,
        "one committed publication is exactly one durable audit transition"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "an ambiguous publication may not change which rows are readable"
    );

    server
        .flush_bifrost()
        .await
        .expect("the retry reconciles the identical publication and succeeds");
    let reconciled = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable after reconciliation");
    assert_eq!(
        reconciled
            .iter()
            .map(|file| (file.id, file.object_key.clone(), file.file_checksum.clone()))
            .collect::<Vec<_>>(),
        committed
            .iter()
            .map(|file| (file.id, file.object_key.clone(), file.file_checksum.clone()))
            .collect::<Vec<_>>(),
        "reconciliation must retain the committed object identities rather than \
         publishing the same members again"
    );
    assert_eq!(
        publication_audits(&server, tenant, &table).await,
        audits_before + 1,
        "a reconciling retry inserts no artifact row and therefore emits no second audit"
    );
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        expected.len() as u64,
        "an ambiguous publication and its retry must publish each row exactly once"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "reconciliation must leave the public read exactly as the client was acknowledged"
    );
    assert_no_leaked_scribe_ownership(&server, "ambiguous publication reconciliation");

    // Replaying the very first batch after it was published adds nothing. The
    // durable batch fence already owns that identity, so the append is either
    // acknowledged as a duplicate or refused; neither may write the rows again.
    let replayed = append_values(&client, &table, first_batch, &first).await;
    server
        .flush_bifrost()
        .await
        .expect("the replayed batch flushes");
    // A refused duplicate may not cost the tenant its lane.
    let third_batch = uuid::Uuid::now_v7();
    let third: Vec<i64> = (200..204).collect();
    append_values(&client, &table, third_batch, &third)
        .await
        .unwrap_or_else(|error| {
            panic!("a new batch is still accepted after replay {replayed:?}: {error:?}")
        });
    expected.extend_from_slice(&third);
    expected.sort_unstable();
    server
        .flush_bifrost()
        .await
        .expect("the following batch publishes");
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "replaying a published batch must not resurrect its rows"
    );
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        expected.len() as u64,
        "replaying a published batch must not publish its rows again"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Counts the durable publication transitions recorded for one logical table.
///
/// Wraps the server-side audit probe so the case reads as a statement about the
/// table under test rather than about the outbox query.
async fn publication_audits(
    server: &WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    table_fqn: &str,
) -> i64 {
    server
        .scribe_publication_audit_count_for_test(tenant, table_fqn)
        .await
        .expect("Scribe publication audit rows are inspectable")
}

/// Asserts a settled Scribe retains no staged, claim, or scratch ownership.
///
/// After a publication settles — whether it committed cleanly or reconciled an
/// uncertain one — every generation the pod reserved must be released. Bytes
/// still charged to the generation ledger are a leak that no row-count
/// assertion can see, because the rows themselves are correct either way.
///
/// # Panics
///
/// Panics when the Scribe cannot be inspected or when any generation
/// reservation, active byte, or immutable byte survives the settled publication.
fn assert_no_leaked_scribe_ownership(server: &WyrdTestServer, boundary: &str) {
    let snapshot = server
        .scribe_inspection_snapshot()
        .expect("the production Scribe exposes its observation snapshot");
    let lifecycle = &snapshot.generation_lifecycle;
    assert_eq!(
        lifecycle.reservations, lifecycle.releases,
        "{boundary} must release every generation reservation it adopted: {lifecycle:?}"
    );
    assert_eq!(
        lifecycle.active_bytes, 0,
        "{boundary} must retain no active generation bytes: {lifecycle:?}"
    );
    assert_eq!(
        lifecycle.immutable_bytes, 0,
        "{boundary} must retain no immutable generation bytes: {lifecycle:?}"
    );
}
