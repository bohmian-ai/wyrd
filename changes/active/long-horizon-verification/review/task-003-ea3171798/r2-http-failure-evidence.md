# Recovered failing output: R1 HTTP pressure test (authoring-time red runs)

These were recovered verbatim from the implementation session's tool output. The run logs were overwritten by later passing runs, so only the excerpts printed during the session survive.

Command (both runs):

```sh
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity) | test(=pg_verification_routes::enqueued_executions_replay_conflict_and_read_back)"'
```

## Failure 1 (first run)

Test body at the time (excerpt):

```rust
    let (status, refused) = enqueue(&server, &jwt, &body, Some("vr-pressure-0001")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{refused}");
    assert_eq!(refused["code"], "WYRD_VERIFICATION_429_STAGING_FULL");
    assert_eq!(refused["details"]["retry_after_seconds"], json!(1));
    assert_eq!(fixture.queued().await, 2, "the refusal queued nothing");

    stall.release();
    stall.wait_completed().await;
    server.flush_bifrost().await.expect("the share drains");
```

Output:

```text
ERROR ...enqueue{operation="verification.enqueue"}: wyrd_runtime::outbox: outbox is full; the staged item was lost outbox="scribe" tenant=01a14c8f-7dde-78a0-bcee-12030fe77c28 capacity=3
ERROR ...enqueue{operation="verification.enqueue"}: wyrd_runtime::outbox: outbox is full; the staged item was lost outbox="scribe" tenant=01a14c8f-7dde-78a0-bcee-12030fe77c28 capacity=3
 WARN ...enqueue{operation="verification.enqueue"}: wyrd_server::components::verification::service: verification staging refused outbox="scribe" admitted=2 limit=2
 WARN wyrd_testing::server: WyrdTestServer dropped on an active Tokio runtime without explicit shutdown(); aborting its lifecycle owners. Prefer `srv.shutdown().await` in async tests.
 WARN wyrd_testing::server: Bifrost did not drain within the teardown budget; awaiting abort
thread 'pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity' panicked at crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:518:5:
assertion `left == right` failed: the refusal queued nothing
  left: 0
 right: 2
```

## Failure 2 (second run, after the queue assertion moved below the drain)

Helper at the time:

```rust
    let rows = server
        .retained_records(
            server.data_tenant_id(),
            "SELECT CAST(execution_id AS VARCHAR) FROM vala.verification.execution_inputs"
                .to_owned(),
        )
        .await
        .expect("inputs rows read");
```

Output:

```text
thread 'pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity' panicked at crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:475:10:
inputs rows read: Audit("retained audit projection is neither text nor Variant")
```

The first passing version is commit `ba12fe8f1`, where `git show ba12fe8f1` shows the test as committed.
