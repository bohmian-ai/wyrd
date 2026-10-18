# Supplemental diagnostician reports for TASK-003 (R1 / FIND-TASK-003-6)

Reports A and B in `diagnostician-reports.md` stay as they are. They close diagnoses 3 and 5.
This file adds the evidence for diagnoses 1, 2 and 4. Each report below is copied
verbatim from the implementation session transcript. Every diagnostician was a fresh
read-only agent and was given only the command, the trace or compiler output, and the diff.

| Diagnosis | Report | Provenance | Limits |
|---|---|---|---|
| 1. Rust admission ordering and share split (`outbox.rs` Admission/`Outbox::reserve`, direct and runner reservations) | Report C | Original. The prompt was sent at 2026-10-17T21:10:04Z, during implementation and before the fix. It was recovered from the session transcript; it was not kept in the review packet at review time. | The trace path it cites was a scratch log, and its excerpts are reproduced under Report C evidence. |
| 2. Python neighbor baseline and key (`conftest.py:275–290`, `support.py:144`, `server.rs:3885`) and the mirrored TS key (`assistant.ts:74–83`) | Report D | Retrospective. It was obtained after both fixes, from source, the recovered failing output and the cumulative diff. | It cannot show what the implementer knew before the fix, and it is not proof of compliance before the fix. No report was produced at the time of the fix. |
| 4. Clippy `large_futures` on `Assistant::start_on` (`verify_in_real_time.rs:76`, boxed callers `:68`, `:1217`) | Report E | Retrospective. It was obtained after the fix, from the compiler output and the diff. | Same as Report D. Its narrower-fix note (one `Box::pin` inside `start_on`) was not applied: the reviewer's finding says the boxing at the call sites is not wrong or unnecessary, and the current Clippy run passes. |

## Report C: admission ordering and share split (original)

PROMPT (sent 2026-10-17T21:10:04.426Z):
Read-only diagnosis. Do NOT edit any file, do not run tests, do not commit. Working directory: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003.

Failing command:
WYRD_LOG=info,wyrd_server=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::verification_staging_refuses_and_recovers)"'

Full trace/log: <implementation-session scratchpad>/rust-journey.log
(Panic: "the judge never received 1 calls" at sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:190; a WARN line "verification staging refused outbox=\"scribe\" admitted=4 limit=4" appears for the first verify.)

Diff under test: `git show HEAD` plus `git diff HEAD` (uncommitted). Key files: crates/shared/wyrd-runtime/src/outbox.rs (Outbox admit/reserve/stage/Reservation), crates/wyrd/wyrd-server/src/components/verification/service.rs (execute/enqueue/authorize_run/staging_full), crates/wyrd/wyrd-server/src/scribe_outbox.rs (ServerSpanSink, ScribeWrite), the journey verification_staging_refuses_and_recovers in sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs.

Return: (1) the code-level cause of the failure with file:line evidence, (2) the fix site (production owner vs test) and why, (3) every other caller affected by that fix site. Be concise (under 400 words).

REPORT (received 2026-10-17T21:11:56.556Z):
**(1) Cause.** The test as written can never pass against the current code. The reservation check counts every item in the outbox, including audit decisions and span copies that were never reserved. The test assumes it counts only reservations.

- `Outbox::reserve` (crates/shared/wyrd-runtime/src/outbox.rs:239-246) calls `admit(slots, capacity*3/4)`. `admit` (:260-264) compares against the single shared `admitted` counter.
- `Outbox::stage` (:220-221) adds to that same counter for unreserved items:
  - audit decisions, at crates/wyrd/wyrd-server/src/audit/mod.rs:129 and :163;
  - tenant span copies from `ServerSpanSink`, at crates/wyrd/wyrd-server/src/scribe_outbox.rs:627.
- `execute` runs `authorize_run` at service.rs:480, which stages the verify's own audit decision. Only then does it call `reserve(2)` at :482. So every verify charges its own audit row against the reservable share.
- The test sets capacity 6, so the reservable limit is 4. When the first verify arrived, its audit row, the neighbor's pending audit/inputs, and setup span copies made `admitted` = 4. `reserve(2)` was refused (the WARN `admitted=4 limit=4`), the judge was never called, and `await_judge_calls(1)` panicked at verify_in_real_time.rs:190.
- Flushing first would not save it. With verify1 holding 2 reserved slots, verify2's own audit row makes 1 + 2 + 2 = 5 &gt; 4, so verify2 is always refused too.
- The code also contradicts its own docs. The module doc (outbox.rs:25-27) says reserve refuses once "reservations would hold more than RESERVED_SHARE", and `RESERVED_SHARE` (:62-64) says the rest "stays free for unreserved staging". The code checks the total of everything admitted instead.

**(2) Fix site: production, outbox.rs.** Keep a separate count of reservation-held work: unspent slots plus items staged through a `Reservation` that are not yet written. `reserve` should check that count against the 3/4 limit, and `stage` should keep bounding the total against `capacity`. That makes refusals deterministic and matches the documented contract.

If only the test is changed (bigger capacity, flushing), it stays racy, because the background span and audit traffic varies in timing. The cost of the fix is that queued items must carry whether they came from a reservation, so the writer releases them from the right counter.

**(3) Affected callers and sites.**
- Inside outbox.rs:
  - `send` (:284);
  - the writer's release path (:624-628);
  - `shutdown`'s loss release (:340-341);
  - `Reservation::stage` (:374-385) and `Drop` (:389-395);
  - the unit test `bounded_staging_covers_retrying_and_inflight_work` (:1015-1037), which asserts `(admitted, limit) == (3, 3)` and needs its fields re-checked.
- The `OutboxFull { admitted, limit }` values reported through `staging_full` (service.rs:128).
- `reserve` callers:
  - `VerificationControl::execute`, service.rs:482 (Scribe outbox);
  - `enqueue`, service.rs:657 (`SubmissionOutbox` = `Outbox&lt;SubmissionSink&gt;`, submissions.rs:34) and :658 (Scribe outbox).
- Reserved-slot stagers: `stage_direct` (service.rs ~:674), `enqueue`'s `staged.stage` (:659), and the hand-off into the submissions outbox at scribe_outbox.rs:947.
- Unreserved `stage` callers keep their behaviour (still capped at `capacity`): audit/mod.rs:129 and :163, the `ServerSpanSink` path at scribe_outbox.rs:627, oracle/peer_audit.rs:84, and query/service.rs:150.

### Report C evidence (recovered failing output)

--- transcript line 709 (2026-10-17T21:05:57.843Z) ---
    2026-10-17T21:04:07.349429Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.regis
    2026-10-17T21:04:07.366863Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.regis
    2026-10-17T21:04:07.385687Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.regis
    2026-10-17T21:04:07.401734Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.regis
    2026-10-17T21:04:07.417452Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.regis
    2026-10-17T21:04:07.777498Z  WARN request{otel.kind="server" method=POST path="/v1/verification/execute" version=HTTP/1.1 http.request.method=POST http.route="/v1/verification/execute"}:verificati
    2026-10-17T21:04:08.059422Z  INFO wyrd_server::verification::fitter: drift baseline fitted tenant=01a14bad-d89c-7d12-9e9f-3400cb90a4bd verifier_uid=01a14bad-da2c-7d82-a16f-cd01cd5baec5
    test verify_in_real_time::verification_staging_refuses_and_recovers has been running for over 60 seconds
    2026-10-17T21:05:06.023112Z  INFO bifrost.forge.maintenance.pass{role="server"}: vala_bifrost_redux::forge::gc: Forge leader maintenance pass started manifest_rewrite_tables=0 snapshot_expiration_
    2026-10-17T21:05:06.023164Z  INFO bifrost.forge.maintenance.pass{role="server"}: vala_bifrost_redux::forge::gc: Forge leader maintenance pass completed
    2026-10-17T21:05:37.918758Z  WARN wyrd_testing::server: WyrdTestServer dropped on an active Tokio runtime without explicit shutdown(); aborting its lifecycle owners. Prefer `srv.shutdown().await`
    2026-10-17T21:05:37.919041Z  INFO vala_bifrost_redux::gate: bifrost gate closed
    2026-10-17T21:05:37.919104Z  INFO wyrd_storage::sweeper: storage sweeper shutting down
    2026-10-17T21:05:37.919180Z  INFO vala_bifrost_redux::forge::leadership: Forge leader term revoked fencing_token=1 reason="shutdown"
    2026-10-17T21:05:37.920463Z  WARN wyrd_testing::server: serve task did not drain within the teardown budget; aborting
    2026-10-17T21:05:37.920597Z  WARN wyrd_testing::server: Bifrost did not drain within the teardown budget; awaiting abort
    2026-10-17T21:05:37.920630Z  INFO vala_bifrost_redux::forge::worker: Forge worker stopped worker=05b2fd76-cb39-494a-8f23-2d3a672bf934
    test verify_in_real_time::verification_staging_refuses_and_recovers ... FAILED

    failures:

    failures:
        verify_in_real_time::verification_staging_refuses_and_recovers

    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 121 filtered out; finished in 93.20s

  stderr ───

    thread 'verify_in_real_time::verification_staging_refuses_and_recovers' (2144478) panicked at sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:190:13:
    the judge never received 1 calls
    stack backtrace:
       0: __rustc::rust_begin_unwind
                 at /rustc/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/std/src/panicking.rs:679:5
       1: core::panicking::panic_fmt
                 at /rustc/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/core/src/panicking.rs:80:14
       2: {async_fn#0}
                 at ./tests/integration/verify_in_real_time.rs:190:13
       3: {async_block#0}
                 at ./tests/integration/verify_in_real_time.rs:1242:48
       4: poll<&mut dyn core::future::future::Future<Output=()>>
                 at /home/thorrester/.rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/future/future.rs:133:9
})) operation="verification.execute"}:verification.execute{mode="direct" execution_id=01a14bad-dc61-71a1-b9c9-a533b1503efe}: wyrd_server::components::verification::service: verification staging refused outbox="scribe" admitted=4 limit=4
    /// Wait until the judge has received `calls` requests, so that many
    /// judged executions are in flight.
    ///
    /// # Panics
    /// Panics when the judge stops recording or the calls never arrive.
    async fn await_judge_calls(&self, calls: usize) {
        let deadline = std::time::Instant::now() + WAIT;
        while self
            .judge
            .received_requests()
            .await
            .expect("the judge records requests")
            .len()
            < calls
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the judge never received {calls} calls"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// The hydrated assistant, loaded with its server calls fixed to
    /// `client` and Bifrost not started.
    ///

--- transcript line 2035 (2026-10-18T01:02:33.425Z) ---
3396 2026-10-17T21:10:04.426Z 2026-10-17T21:11:56.556Z
PROMPT (sent 2026-10-17T21:10:04.426Z):
Read-only diagnosis. Do NOT edit any file, do not run tests, do not commit. Working directory: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003.

Failing command:
WYRD_LOG=info,wyrd_server=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::verification_staging_refuses_and_recovers)"'

Full trace/log: <implementation-session scratchpad>/rust-journey.log
(Panic: "the judge never received 1 calls" at sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:190; a WARN line "verification staging refused outbox=\"scribe\" admitted=4 limit=4" appears for the first verify.)

Diff under test: `git show HEAD` plus `git diff HEAD` (uncommitted). Key files: crates/shared/wyrd-runtime/src/outbox.rs (Outbox admit/reserve/stage/Reservation), crates/wyrd/wyrd-server/src/components/verification/service.rs (execute/enqueue/authorize_run/staging_full), crates/wyrd/wyrd-server/src/scribe_outbox.rs (ServerSpanSink, ScribeWrite), the journey verification_staging_refuses_and_recovers in sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs.

Return: (1) the code-level cause of the failure with file:line evidence, (2) the fix site (production owner vs test) and why, (3) every other caller affected by that fix site. Be concise (under 400 words).

REPORT (received 2026-10-17T21:11:56.556Z):
**(1) Cause.** The test as written can never pass against the current code. The reservation check counts every item in the outbox, including audit decisions and span copies that were never reserved. The test assumes it counts only reservations.

- `Outbox::reserve` (crates/shared/wyrd-runtime/src/outbox.rs:239-246) calls `admit(slots, capacity*3/4)`. `admit` (:260-264) compares against the single shared `admitted` counter.
- `Outbox::stage` (:220-221) adds to that same counter for unreserved items:
  - audit decisions, at crates/wyrd/wyrd-server/src/audit/mod.rs:129 and :163;
  - tenant span copies from `ServerSpanSink`, at crates/wyrd/wyrd-server/src/scribe_outbox.rs:627.
- `execute` runs `authorize_run` at service.rs:480, which stages the verify's own audit decision. Only then does it call `reserve(2)` at :482. So every verify charges its own audit row against the reservable share.
- The test sets capacity 6, so the reservable limit is 4. When the first verify arrived, its audit row, the neighbor's pending audit/inputs, and setup span copies made `admitted` = 4. `reserve(2)` was refused (the WARN `admitted=4 limit=4`), the judge was never called, and `await_judge_calls(1)` panicked at verify_in_real_time.rs:190.
- Flushing first would not save it. With verify1 holding 2 reserved slots, verify2's own audit row makes 1 + 2 + 2 = 5 &gt; 4, so verify2 is always refused too.
- The code also contradicts its own docs. The module doc (outbox.rs:25-27) says reserve refuses once "reservations would hold more than RESERVED_SHARE", and `RESERVED_SHARE` (:62-64) says the rest "stays free for unreserved staging". The code checks the total of everything admitted instead.

**(2) Fix site: production, outbox.rs.** Keep a separate count of reservation-held work: unspent slots plus items staged through a `Reservation` that are not yet written. `reserve` should check that count against the 3/4 limit, and `stage` should keep bounding the total against `capacity`. That makes refusals deterministic and matches the documented contract.

If only the test is changed (bigger capacity, flushing), it stays racy, because the background span and audit traffic varies in timing. The cost of the fix is that queued items must carry whether they came from a reservation, so the writer releases them from the right counter.

**(3) Affected callers and sites.**
- Inside outbox.rs:
  - `send` (:284);
  - the writer's release path (:624-628);
  - `shutdown`'s loss release (:340-341);
  - `Reservation::stage` (:374-385) and `Drop` (:389-395);
  - the unit test `bounded_staging_covers_retrying_and_inflight_work` (:1015-1037), which asserts `(admitted, limit) == (3, 3)` and needs its fields re-checked.
- The `OutboxFull { admitted, limit }` values reported through `staging_full` (service.rs:128).
- `reserve` callers:
  - `VerificationControl::execute`, service.rs:482 (Scribe outbox);
  - `enqueue`, service.rs:657 (`SubmissionOutbox` = `Outbox&lt;SubmissionSink&gt;`, submissions.rs:34) and :658 (Scribe outbox).
- Reserved-slot stagers: `stage_direct` (service.rs ~:674), `enqueue`'s `staged.stage` (:659), and the hand-off into the submissions outbox at scribe_outbox.rs:947.
- Unreserved `stage` callers keep their behaviour (still capped at `capacity`): audit/mod.rs:129 and :163, the `ServerSpanSink` path at scribe_outbox.rs:627, oracle/peer_audit.rs:84, and query/service.rs:150.
## Report D: neighbor baseline and key (retrospective)

Retrospective findings, from source only. They can't show what the implementer knew before either fix.

**Failure 1: baseline not ready (Python)**

(a) Cause: `WyrdTestServer::wait_for_baseline` (`crates/wyrd/wyrd-testing/src/server.rs:3885-3921`) always reads `DriftBaselineQueue::status` through `tenant_conn(self.data_tenant_id())` at line 3898, which is the fixture tenant. The neighbor's `latency-drift` Verifier lives in the `burst-neighbor` tenant, so tenant isolation hides its row, `last` stays `None`, and the call times out after 90s. The baseline itself was fitting normally. The `fitter: drift baseline fitted tenant=...` log line in the evidence shows that.

(b) Owner: the test helper. The neighbor only submits `answer-check` and never depends on the latency baseline. The committed fix makes the wait conditional (`fitted_by=None` for the neighbor, `conftest.py:275-291`). That matches the Rust mirror `in_tenant` (`verify_in_real_time.rs:157-177`), which never waits. Tenancy is not weakened, because the harness can still only see its own tenant, and no assertion is dropped. A per-tenant `wait_for_baseline` would only be needed if a neighbor assertion depended on drift.

(c) Callers: `assistant_bundle` (`conftest.py:270-272`) and the staging test's fixture-tenant assistant (`test_verify_in_real_time.py:466`) both pass `server` and keep the wait. The neighbor (`:473`) skips it. A separate fixture at `conftest.py:340` waits in the fixture tenant and is unaffected.

**Failure 2: Card scope does not cover the verified subject (Python, then TypeScript)**

(a) Cause: the neighbor's Service key was issued through the ambient client, which is the fixture-tenant admin. So the key was bound to the fixture tenant's `assistant` Card. It was then used with the neighbor's bundle, whose subject Card UIDs belong to the other tenant, and the server's scope check correctly refused it.
- Python: before the fix, `service_key` took no client (`support.py:144`, old form). The underlying `cli.issue_key` already accepted `client` (`_commands.py:90-118`).
- TypeScript: `cli.issueKey` was called without `{ client }` (old `assistant.ts` fixture body).

(b) Owner: the test helpers. The fixes are `service_key(service, client)` (`support.py:144-148`, used at `test_verify_in_real_time.py:474`) and `registerAssistant(client)` calling `issueKey(..., { client })` (`assistant.ts:74-92`, used at `verify-in-real-time.test.ts:443-444`). Both mirror Rust's `Some(admin)` (`verify_in_real_time.rs:167`). The server refusal was correct, and nothing in scope checks or assertions was relaxed.

(c) Fixture-tenant callers that still use the ambient default, which is correct for them:
- Python: `conftest.py:297`, `:313` and `:381`; `test_verify_in_real_time.py:108` and `:467`; `test_verifier_building_blocks.py:133` and `:158`.
- TypeScript: `assistantTest` (`registerAssistant(WyrdClient.connect())`) and `verify-in-real-time.test.ts:293`.

**Evidence limits (d)**

- The pre-fix test bodies were never committed. The cause of failure 2 is inferred from the old helper signatures plus the error text. No failing source line for the neighbor's key was recovered.
- The evidence doesn't map the failing Verifier UID to its tenant. Failure 1's cause is inferred from the hardcoded tenant on line 3898.
- Nothing was rerun.
- The TypeScript commit also changes the provider fixture's scope (`server.ts:157`). That fixes a separate Vitest fixture-sharing problem and is outside these three failures.

### Report D evidence (recovered failing output)

--- transcript line 1268 (2026-10-17T21:31:45.565Z) ---
52:tests/integration/test_verify_in_real_time.py:473:
70:E       wyrd._wyrd.WyrdError: baseline of verifier 01a14bc5-a91a-7802-98b4-2bca164502fa was not ready within 90s; last observed: None
72:tests/integration/conftest.py:285: WyrdError
Shell cwd was reset to /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003

--- transcript line 1272 (2026-10-17T21:31:55.675Z) ---
28:________________ test_verification_staging_refuses_and_recovers ________________
30:tmp_path = PosixPath('/tmp/pytest-of-thorrester/pytest-45/test_verification_staging_refu0')
32:    def test_verification_staging_refuses_and_recovers(tmp_path: Path) -> None:
38:                scribe_outbox_capacity=STAGING_CAPACITY,
57:destination = PosixPath('/tmp/pytest-of-thorrester/pytest-45/test_verification_staging_refu0/neighbor')
60:        """Register the ``assistant`` Service as ``client``, wait for its baseline, and download it.
66:        register(cards, "cards/latency_baseline/latency-baseline.yaml")
69:>       server.wait_for_baseline(str(registered["latency-drift"].uid), BASELINE_TIMEOUT_SECONDS)
70:E       wyrd._wyrd.WyrdError: baseline of verifier 01a14bc5-a91a-7802-98b4-2bca164502fa was not ready within 90s; last observed: None
72:tests/integration/conftest.py:285: WyrdError
91:2026-10-17T21:30:05.071163Z ERROR request{otel.kind="server" method=PUT path="/v1/admin/gateway/provider-credentials/openai-key" version=HTTP/1.1 http.request.method=PUT http.route="/v1/admin/gateway/provider-credentials/{name}"}:put_credential{operation="g
92:2026-10-17T21:30:05.087263Z ERROR request{otel.kind="server" method=PUT path="/v1/admin/gateway/provider-deployments/openai" version=HTTP/1.1 http.request.method=PUT http.route="/v1/admin/gateway/provider-deployments/{name}"}:put_deployment{operation="gatew
93:2026-10-17T21:30:05.106945Z ERROR request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}: wyrd_runtime::outbox: outbox is full; the s
96:2026-10-17T21:30:05.413614Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registra
97:2026-10-17T21:30:05.437891Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registra
98:2026-10-17T21:30:05.458446Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registra
99:2026-10-17T21:30:05.474706Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registra
100:2026-10-17T21:30:05.492648Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registr
101:2026-10-17T21:30:05.508708Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registr
102:2026-10-17T21:30:05.523733Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registr
103:2026-10-17T21:30:05.540727Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registr
104:2026-10-17T21:30:05.988193Z  INFO wyrd_server::verification::fitter: drift baseline fitted tenant=01a14bc5-996c-7f72-92fa-c0576413318c verifier_uid=01a14bc5-a0a9-7b53-8675-c94293d76a18
105:2026-10-17T21:30:06.133896Z ERROR request{otel.kind="server" method=GET path="/v1/cards/by-ref" version=HTTP/1.1 http.request.method=GET http.route="/v1/cards/by-ref"}:get_card_by_ref_http{query=CardRefQuery { kind: "Verifier", space: "default", name: "ans
106:2026-10-17T21:30:06.160227Z ERROR request{otel.kind="server" method=GET path="/v1/cards/by-ref" version=HTTP/1.1 http.request.method=GET http.route="/v1/cards/by-ref"}:get_card_by_ref_http{query=CardRefQuery { kind: "Agent", space: "default", name: "judge-
107:2026-10-17T21:30:06.187992Z ERROR request{otel.kind="server" method=GET path="/v1/cards/by-ref" version=HTTP/1.1 http.request.method=GET http.route="/v1/cards/by-ref"}:get_card_by_ref_http{query=CardRefQuery { kind: "Verifier", space: "default", name: "lat
108:2026-10-17T21:30:06.220372Z ERROR request{otel.kind="server" method=GET path="/v1/cards/download/local" version=HTTP/1.1 http.request.method=GET http.route="/v1/cards/download/local"}: wyrd_runtime::outbox: outbox is full; the staged item was lost outbox="
109:2026-10-17T21:30:06.226601Z ERROR request{otel.kind="server" method=GET path="/v1/cards/download/local" version=HTTP/1.1 http.request.method=GET http.route="/v1/cards/download/local"}: wyrd_runtime::outbox: outbox is full; the staged item was lost outbox="
110:2026-10-17T21:30:06.277842Z ERROR request{otel.kind="server" method=POST path="/auth/issue-key" version=HTTP/1.1 http.request.method=POST http.route="/auth/issue-key"}: wyrd_runtime::outbox: outbox is full; the staged item was lost outbox="scribe" tenant=0
111:2026-10-17T21:30:07.316659Z ERROR request{otel.kind="server" method=PUT path="/v1/admin/gateway/provider-credentials/openai-key" version=HTTP/1.1 http.request.method=PUT http.route="/v1/admin/gateway/provider-credentials/{name}"}:put_credential{operation="
112:2026-10-17T21:30:07.321823Z ERROR request{otel.kind="server" method=PUT path="/v1/admin/gateway/provider-deployments/openai" version=HTTP/1.1 http.request.method=PUT http.route="/v1/admin/gateway/provider-deployments

--- transcript line 1347 (2026-10-17T21:33:54.581Z) ---
<persisted-output>
Output too large (128.8KB). Full output saved to: /home/thorrester/.claude/projects/-home-thorrester-Documents-GitHub-wyrd-worktrees-long-horizon-verification-task-003/3d9ed8e8-ef54-480b-a2fe-2598087f67c5/tool-results/b4bvpub7s.txt

Preview (first 2KB):
60:>                       accepted = neighbor.run("agent").observe.submit("answer-check", "yes")
62:E                       wyrd._wyrd.WyrdError: the caller's Card scope does not cover the verified subject
64:tests/integration/test_verify_in_real_time.py:485: WyrdError
----
48:2026-10-17T21:33:44.157517Z  INFO request{otel.kind="server" method=POST path="/v1/cards" version=HTTP/1.1 http.request.method=POST http.route="/v1/cards"}:register_card_http{operation="card.registration.create"}:register_card{idempotency_key="card-registration-01a14bc8-f6e4-7d12-ab92-093e7379f43b" request=CreateCardRequest { submissions: [CardSubmission { api_version: ApiVersion("wyrd/v1"), kind: Prompt, metadata: Metadata { name: CardName("assistant-prompt"), version: Some(Pin(VersionBlock("1.0.0"))), bump: None, space: Some(SpaceName("default")), uid: None, labels: {}, annotations: {}, spec_hash: None, artifact_hash: None, origin: None }, spec: Object {"provider": String("openai"), "model": String("gpt-test"), "messages": Array [Object {"role": String("user"), "content": String("Answer the question.")}]}, artifacts: [] }, CardSubmission { api_version: ApiVersion("wyrd/v1"), kind: Prompt, metadata: Metadata { name: CardName("judge-prompt"), version: Some(Pin(VersionBlock("1.0.0"))), bump: None, space: Some(SpaceName("default")), uid: None, labels: {}, annotations: {}, spec_hash: None, artifact_hash: None, origin: None }, spec: Object {"provider": String("openai"), "model": String("gpt-4o"), "messages": Array [Object {"role": String("user"), "content": String("Claim: {{claim}}. Answer: {{answer}}. Cite the answer.")}], "response_format": Object {"type": String("object"), "properties": Object {"outcome": Object {"type": String("string"), "enum": Array [String("passed"), String("failed"), String("inconclusive")]}, "reason": Object {"type": String("string")}, "citations": Object {"type": String("array"), "items": Object {"type": String("object"), "properties": Object {"slot": Object {"type": St
...
</persisted-output>
Shell cwd was reset to /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003

--- transcript line 1376 (2026-10-17T21:34:14.432Z) ---
 ❯ tests/integration/verify-in-real-time.test.ts (20 tests | 1 failed | 19 skipped) 5909ms
   × verification_staging_refuses_and_recovers 5908ms
 FAIL  tests/integration/verify-in-real-time.test.ts > verification_staging_refuses_and_recovers
WyrdError: the caller's Card scope does not cover the verified subject
 ❯ projectedError dist/index.js:68:12
 ❯ nativeHandle dist/index.js:84:51
 ❯ Observe.submit dist/index.js:1620:27
 ❯ tests/integration/verify-in-real-time.test.ts:462:24
Shell cwd was reset to /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003
## Report E: Clippy large_futures (retrospective)

Retrospective diagnosis (read-only; nothing edited or committed).

**(a) Cause.** `clippy.toml` sets no `future-size-threshold`, so the default of 16384 bytes applies. `large_futures` is in the `pedantic` group, which `[workspace.lints.clippy]` sets to warn, and `-D warnings` turns that into an error. Before the task, `Assistant::start` built `WyrdTestServer::builder()` inline after the judge's awaits and moved it straight into `Deployment::start_with`, so the future held one builder. In `11c061743` the builder became a parameter of `start_on` (`verify_in_real_time.rs:76`). The `start_on` future now keeps the parameter alive across `MockServer::start().await` and `.mount().await`, and also holds the `Deployment::start_with` future. That future holds the `start_bound(self)` future, which holds the builder inline: its inner `Box::pin` at `server.rs:5178` only boxes `compose_in_process`. So the state carries about two builders. The result is 16760 bytes for `start_on` and 16768 for `start`, about 380 over the limit. The new `scribe_outbox_capacity: Option<usize>` field adds only 16 bytes.

**(b) Owner.** Boxing at the call sites is correct and follows existing precedent (`Box::pin(Local::start(...))` in `local_development.rs` and `signed_in_development.rs`). It is not the narrowest fix. One `Box::pin(Deployment::start_with(...))` inside `start_on` would cover all 22 sites, because the builder alone (about half of 16 KB, by inference) stays under the limit. Boxing `builder.start_bound()` inside `Deployment::start_with` (`support/mod.rs:65`) would be the broader shared-owner fix and would protect every `Deployment` user. Neither is required. The builder itself does not need to change.

**(c) Affected callers (22).**
- `verify_in_real_time.rs:68`: `start` calling `start_on`.
- 19 calls to `Assistant::start()` in `verify_in_real_time.rs`, lines 415 to 1162.
- `verify_in_real_time.rs:1217`: the direct `start_on` call in `verification_staging_refuses_and_recovers`.
- `action_timeline.rs:154`: `Assistant::start()`.

Boxing inside `start` fixes the 19 `start()` callers and `action_timeline.rs` without touching them. The evidence file shows only the first 5 of the 22 errors.

**(d) Necessity and limits.** The boxing is needed: without it the lint gate fails. On the current tree, `cargo clippy --locked -p wyrd-ai --test integration --all-features -- -D warnings` finishes clean, and the change has no behavioral effect. Limits:
- I did not measure the builder size or the pre-change future size; the "two builders" explanation comes from reading the source, not from `-Zprint-type-sizes`.
- I ran only the `wyrd-ai` integration target, not the full `mise run lints`.
- I did not run the journey tests (they are `#[ignore]` and need Postgres).

### Report E evidence (recovered compiler output)

--- transcript line 1494 (2026-10-17T21:48:08.265Z) ---
 * `test` with one real Wyrd server per story file, started with `options`.
 *
 * `provider` is the local upstream the server calls, started before the
 * server; with `options.provider` set, its address is the server's provider
 * base URL, and without it the server keeps its default upstreams. Each test starts
 * from the deployment environment, so a test that removes the ambient
 * credential with `vi.stubEnv` leaves the next test unaffected.
 */
export function serverTest(options: ServerOptions = {}) {
  return base.extend<{ provider: Receiver; server: NativeWyrdTestServer; deployment: void }>({
    provider: [
      async ({}, use) => {
        const provider = await Receiver.start(options.provider);
        await use(provider);
        await provider.close();
      },
      { scope: "file" },
    ],
    server: [
error: large future with a size of 16768 bytes
   --> sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs:154:21
    |
154 |     let assistant = Assistant::start().await;
    |                     ^^^^^^^^^^^^^^^^^^ help: consider `Box::pin` on it: `Box::pin(Assistant::start())`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#large_futures
    = note: `-D clippy::large-futures` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::large_futures)]`
--
error: large future with a size of 16760 bytes
  --> sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:68:9
   |
68 |         Self::start_on(WyrdTestServer::builder(), Duration::ZERO).await
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: consider `Box::pin` on it: `Box::pin(Self::start_on(WyrdTestServer::builder(), Duration::ZERO))`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#large_futures

error: large future with a size of 16768 bytes
   --> sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:415:21
    |
415 |     let assistant = Assistant::start().await;
    |                     ^^^^^^^^^^^^^^^^^^ help: consider `Box::pin` on it: `Box::pin(Assistant::start())`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#large_futures

error: large future with a size of 16768 bytes
   --> sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:447:21
    |
447 |     let assistant = Assistant::start().await;
    |                     ^^^^^^^^^^^^^^^^^^ help: consider `Box::pin` on it: `Box::pin(Assistant::start())`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#large_futures

error: large future with a size of 16768 bytes
   --> sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:479:21
    |
479 |     let assistant = Assistant::start().await;
    |                     ^^^^^^^^^^^^^^^^^^ help: consider `Box::pin` on it: `Box::pin(Assistant::start())`
    |