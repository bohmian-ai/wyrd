use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use wyrd_testing::WyrdTestServer;

use super::support::{append_values, register_table, sorted_values, tenant_client, unique_table};

/// AC22/AC28 Tier-2 owner: failure, retry, and replay never change row identity.
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
        .flush_bifrost_for_tenant(tenant)
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
        .flush_bifrost_for_tenant(tenant)
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
    faults.fail_next_post_commit_response();
    let ambiguous = server.flush_bifrost_for_tenant(tenant).await;
    let mut expected: Vec<i64> = first.iter().chain(second.iter()).copied().collect();
    expected.sort_unstable();
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "an ambiguous publication may not change which rows are readable"
    );
    if ambiguous.is_err() {
        server
            .flush_bifrost_for_tenant(tenant)
            .await
            .expect("the retry reconciles the ambiguous publication");
    }
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        expected.len() as u64,
        "an ambiguous publication and its retry must publish each row exactly once"
    );

    // Replaying the very first batch after it was published adds nothing. The
    // durable batch fence already owns that identity, so the append is either
    // acknowledged as a duplicate or refused; neither may write the rows again.
    let replayed = append_values(&client, &table, first_batch, &first).await;
    server
        .flush_bifrost_for_tenant(tenant)
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
        .flush_bifrost_for_tenant(tenant)
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

/// Returns how many rows one tenant's published hot objects account for.
async fn published_rows(
    server: &WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
) -> u64 {
    server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), name)
        .await
        .expect("published hot files are inspectable")
        .iter()
        .map(|file| file.row_count)
        .sum()
}
