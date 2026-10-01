# TASK-004 domain review: concurrency and resource ownership

Reviewer role: domain-rev (concurrency and resource ownership: memory governor, query slots, storage permits, async lifetimes). Independent and static.

## Reviewed boundary

- Subject: base `a56ab7569` to candidate `990803fc0`. Source was read at the candidate with `git show 990803fc0:<path>`.
- Task: `changes/active/bifrost-scribe-live-reads/tasks/TASK-004-integrate-eval-server-and-unify-bifrost-memory.md`.
- Spec: `changes/active/bifrost-scribe-live-reads/spec.md` (rev 24).
- Seams traced:
  - Governor synchronization: `BifrostResourceGovernor`, `charge_locked`, `release_locked`, poisoning, and `lock_state`.
  - The DataFusion pools on the shared root: `GovernedMemoryRoot`, `GovernedMemoryView`, `GovernedMemoryLedger`, and the Oracle, Forge and follower views.
  - Oracle query resources: `OracleQueryResources::release`.
  - Transport body leases: HTTP `WyrdBodyLimit`; gRPC `GrpcFirstFrame`; and `BifrostTransportAdmission` and its lease.
  - The OTLP decode transfer into Scribe ingress: `reserve_otlp_decode`, `OtlpDecodeOwner::complete`, and `admit_transport_frame`.
  - Storage permits: `acquire_request`, `governed_decode`, `attempt_once`, and the cache loader and waiter.
  - Receiver slot reservation: `OraclePeerWorker::reserve`, `ReservationRegistry`, and `PendingGraphActivation`.
  - The leader retry loop: `AnalyticalGraphLifecycle::reserve` / `reserve_round` and `dispatcher::wait_for_peer_capacity`.
  - D13: `release_physical_projections` and `retain_admission`.
  - D16: the separate scanner and publisher loops in `app/server.rs`.
  - D17: the `OnceCell` reader in `oracle/exec.rs`.

## Authority and source coverage

- Authorities applied:
  - The task's "One memory charge contract" (Transport row, task:289) and Revision 13 (task:62, "Charge the encoded body while held").
  - R13-A, R13-C and R13-D (task:99, task:202, task:761).
  - D1, D10-D13, D16 and D17.
  - The admission and resources sections of `architecture/bifrost-design.md`.
  - AGENTS.md §5 and §6.
- Main sources reviewed at the candidate:
  - `crates/vala/vala-bifrost-redux/src/resources.rs`
  - `crates/vala/vala-bifrost-redux/src/storage/{mod.rs,cache.rs}`
  - `crates/vala/vala-bifrost-redux/src/gate/limits.rs`
  - `crates/vala/vala-bifrost-redux/src/contracts.rs`
  - `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs`
  - `crates/vala/vala-bifrost-redux/src/oracle/{dispatcher.rs,analytical.rs,admission.rs,analytical_supervisor.rs,exec.rs,query_stream.rs}`
  - `crates/wyrd/wyrd-server/src/http/{middleware/body_limit.rs,router.rs,otlp.rs}`
  - `crates/wyrd/wyrd-server/src/grpc/mod.rs`
  - `crates/wyrd/wyrd-server/src/app/server.rs`
  - `crates/wyrd/wyrd-server/src/config.rs`
  - `crates/wyrd/wyrd-server/src/verification/observations.rs`
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs`
- History checked:
  - `61018283a` and `10f1d7189`: removal of `try_acquire_unknown`.
  - `1a6ec2554`: D10 removal of the fragment path and its dispatcher test.

## Verification limits

- This is a static review only.
- No mise lanes, nextest runs, builds, Postgres wrappers or benchmarks were run.
- The task's PASS rows and benchmark numbers were treated as claims. They were checked against the source and test inventory at the candidate, not re-executed.
- Liveness under real contention, for example the two-leader retry, was judged from the code and the stated bounds, not observed.

## Findings

### CONC-1 — REGRESSION: undeclared-length HTTP bodies are buffered with no governed transport charge

- **Obligation:**
  - Task:289, Transport row: "Validate the declared/decoded message against that ceiling before retaining it; charge the actual encoded bytes held … to the shared root. HTTP and gRPC body owners return that charge when the body is consumed or dropped, including cancellation…"
  - Task:62, Revision 13 "Before ACK": "Charge the encoded body while held."
  - The `LimitsConfig` contract at `crates/wyrd/wyrd-server/src/config.rs:1798`: "worst-case process memory is `body_bytes × concurrency`".
- **Location:**
  - `crates/wyrd/wyrd-server/src/http/middleware/body_limit.rs:136-171` at `990803fc0`.
  - At :136, the lease is taken only when `declared_bytes` is `Some`.
  - At :148-149, `Limited::new(body, max_bytes + 1)` is followed by `limited.collect().await`, which buffers the whole body.
  - Only at :162-165 does `admission.try_acquire(bytes.len())` run, and only for an undeclared body, after it has been fully collected.
- **Evidence:**
  - At the base `a56ab7569`, the same middleware charged before buffering:
    ```rust
    declared_bytes.map_or_else(|| admission.try_acquire_unknown(), |bytes| admission.try_acquire(bytes))
    ```
  - Commit `61018283a` replaced this with a post-collection charge. Its comment reads: "an undeclared body is charged its exact collected length below".
  - In the window between the first chunk and the end of `collect()`, the bytes are held by the process, but neither `BifrostResourceGovernor` nor any lease accounts for them.
  - The edge concurrency default is 1024 (`config.rs:2336`), and the Scribe OTLP route ceiling is 16 MiB.
  - There is no request-decompression layer in front of this middleware, so the collected bytes are the encoded bytes the task requires to be charged.
- **Observable consequence:**
  - A chunked or streamed `Transfer-Encoding` client, with no `Content-Length`, can hold up to `concurrency × max_bytes` of buffered ingest bodies. With the defaults that is about 16 GiB.
  - None of it appears in `used_bytes`, so the shared cap neither sees nor refuses it. Oracle, Scribe and Forge keep charging against capacity that is already consumed, and the process can exceed cap plus server headroom and be OOM-killed.
  - A request cancelled during collection also never appears in governor accounting. Before this change it was precharged and released.
  - This is the producer of the problem. Downstream governor consumers are correct, but they are fed an under-count.
- **Required correction:**
  - Charge an undeclared body incrementally, before each frame is retained. Grow the transport lease by `frame.len()` as each `Frame` is polled, and refuse with the existing `transport_occupied` 503 as soon as a charge fails.
  - Alternatively, use the declared-path rule: reject the undeclared body, or precharge the route ceiling, before any buffering.
  - The lease must be released on cancellation, on a length-limit error, and on decode error.
  - Required tests:
    - A unit test in `body_limit.rs` showing the governor's `used_bytes` is non-zero while a chunked body is still being collected.
    - A unit test showing that a nearly full root refuses an undeclared body before it buffers more than the remaining capacity.
    - A unit test showing that a dropped, mid-collection request returns `used_bytes` to zero.

### CONC-2 — MISSING: the required R13-D dispatcher test no longer exists, and cancellation during the retry wait is unproven

- **Obligation:**
  - Task:202 makes `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::dispatcher::tests::leader_retries_only_preaccept_peer_capacity)'` a required command.
  - The acceptance row at task:761 claims PASS for R13-D on that selector.
  - Task:99 requires: "Cancellation/deadline stops retry."
  - AGENTS.md §11 says every named test must be run by its exact focused command, and an exact `=` selector that matches nothing is not proof.
- **Location:**
  - The test is absent at `990803fc0`: `git grep leader_retries_only_preaccept_peer_capacity 990803fc0 -- crates` returns no matches.
  - It was deleted in `1a6ec2554` (D10) together with the fragment leader path.
  - The retry loop under review now lives in `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs` (`reserve` around 3067 and `reserve_round` around 3141), with the wait at `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:68` (`wait_for_peer_capacity`).
- **Evidence:**
  - The acceptance row cites a selector that selects nothing, so the R13-D PASS evidence is stale.
  - The surviving coverage is:
    - `participant_cut_is_reserved_once_immediately_before_dispatch` and `assert_refused_round_retries_within_deadline` in `analytical.rs`, which cover release-before-wait, the exact retry hint, deadline stop, and transport loss;
    - the journey `peer_network::analytical::two_leaders_retry_preaccept_capacity` (`crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs:1482`).
  - None of these cancels the owner while the leader is parked inside `wait_for_peer_capacity`.
  - `assert_cancellation_interrupts_a_pending_reservation` cancels a hanging peer `reserve`, not the backoff wait.
  - The code path appears correct: the wait `select!`s on cancellation, and the round is released before waiting. But the "Cancellation stops retry" obligation has no test proving it.
- **Observable consequence:**
  - The R13-D acceptance claim cannot be reproduced from the task's own command.
  - A future regression that makes the backoff ignore cancellation would hold an analytical query slot and its leader until the deadline, and no lane would fail.
- **Required correction:**
  - Replace the stale selector at task:202 and task:761 with the real graph-leader test names.
  - Add a focused `vala-bifrost-redux` lib test for this case: a peer returns `Rejected { retry_after_ms }` with a long hint, the owner is cancelled during the wait, and the test asserts that `reserve` returns promptly, makes no second reservation round, and leaves no provisional reservation or leader charge held.

## Clean seams (no finding)

- **Governor lock:**
  - One `Mutex<ResourceState>`, never held across `.await`.
  - The admission charge is dropped outside the lock in `OracleQueryResources::release`.
  - There is no lock-order inversion with `GovernedMemoryRoot::operation` or the `graphs` lock in `retain_admission`.
  - Poisoning is sticky and surfaces through `BifrostResourceHealth`.
- **`GovernedMemoryView`:**
  - `try_grow` follows ceiling, then governor charge, then `pool.try_grow`, and rolls back on failure.
  - The overshoot from an infallible `grow` is booked as headroom and released first on `shrink`.
  - A view dropped with live bytes poisons rather than leaking silently.
  - Forge and the follower views (D1) reuse the same mechanics as the base Oracle view.
- **gRPC:** the declared encoded length is charged at the first frame, and `_lease` lives until `inner.call(...).await` completes, so it is released on cancellation and decode error. No client-streaming RPC exists.
- **OTLP decode transfer:** the lease moves into `OtlpDecodeOwner` and is returned by `complete()` into Scribe ingress, with no double charge. The JSON scratch buffer is dropped after decode.
- **Storage permits (R13-C):**
  - `acquire_request` is a biased `select!` over owner cancellation, the deadline and `acquire_owned`.
  - The per-attempt timeout starts after the permit is acquired, and the permit is dropped each attempt.
  - The cache loader races the deadline, and abandoned waiters cancel it.
- **Receiver slots:** the pending entry is charged via `try_acquire_query`. `PendingGraphActivation` commits, rolls back, or restores on `Drop`. Running slots are never exceeded.
- **Leader retry:**
  - The round is released before waiting, the wait is bounded by `now + retry_after_ms < deadline`, and ambiguous `Err(None)` is not retried.
  - Two-leader contention is bounded by the deadline, as acknowledged in D6.
- **D13:** the production caller of `release_physical_projections` is the terminal `settle_analytical` at `query_stream.rs:815`. The `mod.rs` caller is `test-support` only.
- **D16:**
  - The scanner loop runs synchronous `check_age`.
  - The publisher loop runs `publish_due`, which is documented as cancel-safe, under a nested shutdown `select!`.
  - There is no shared lock between the two loops.
- **D17:** `OnceCell::get_or_init` is infallible, so no error is cached. The retained first-partition footer loader is per-exec and per-tenant, with no cross-tenant sharing.

## Residuals (not findings)

- Expired pending reservations are reclaimed only lazily, and any receiver error maps to `Rejected { retry_after_ms: 1000 }`. Both behave the same at the base.
- `verification/observations.rs`: `PENDING_LIMIT = 256` tracked tasks retain frame `Bytes` uncharged. This sits within server headroom by design and was not changed by this task's memory contract.

## Overall result

FAIL
