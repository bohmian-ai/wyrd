# TASK-003-R1: review fixes

Implement with `$wyrd-implement`. This task is self-contained, and every
decision in it is already made. Do your own research to find exact lines.
Line numbers are from candidate `861a0342f` and may have moved. Paths are
relative to `crates/vala/vala-bifrost-redux/src` unless stated.

## Background

- **What TASK-003 did.** It moved Variant querying into a DataFusion fork
  (the `variant` feature) and rebuilt Bifrost's Analytical (distributed)
  query path on native `datafusion-distributed`.
- **What was deleted.** Wyrd's own Variant SQL, leaf-path pushdown, pruning,
  scan assignment and cache code. Do not bring any of it back.
- **The ownership rule.** If logic is not Wyrd-specific and needs to change
  DataFusion, arrow-rs or `datafusion-distributed` internals, it belongs in
  the fork. Wyrd-specific logic stays in Wyrd/Bifrost.
- **Inputs:**
  - spec: `changes/active/bifrost-variant/spec.md`, plus
    `spec-revision-26-datafusion-variant.md`, which wins where they differ;
  - task plan: `changes/active/bifrost-variant/tasks/TASK-003-native-datafusion-read-path-plan.md`;
  - review: `verdict.md` and `findings-validation.md` in this directory.
- **Forks**, published to github.com/bohmian-ai, with local clones in
  `/home/thorrester/Documents/GitHub/`. They are pinned in the root
  `Cargo.toml` `[patch.crates-io]`:
  - `datafusion` `a4ff145509b0…`;
  - `arrow-rs` `0d0eb247d802…`;
  - `datafusion-distributed` `0b81b127164a…`, branch `wyrd-arrow60`.

## Rules

- **Wyrd rules:** follow `AGENTS.md`.
  - No `#[allow]`. Don't weaken, delete or `#[ignore]` tests.
  - Every touched Rust item gets rustdoc, with `# Errors` and `# Panics`
    sections where they apply.
- **Diagnosing failures:**
  - Rerun with `WYRD_LOG` tracing on and record Symptom / Evidence / Cause /
    Fix site.
  - Before changing a test, assertion, timeout or retry to clear a failure,
    get a fresh read-only diagnostician. Give it only the command, the trace
    and the diff.
- **Builds:**
  - Run heavy builds one at a time under `nice`, with `-j 12`.
  - Use toolchain `cargo +1.98.1` for the forks; Wyrd's `mise.toml` pins the
    same version.
- **Fork work:**
  - Write it as the fork's maintainer: upstream quality, no Wyrd names, and
    the fork's own `AGENTS.md` and contributor guide apply.
  - Every fork commit passes `cargo fmt --all`,
    `cargo clippy --all-targets --all-features -- -D warnings`, and the
    changed crates' tests.
  - Push to bohmian-ai only after telling the user what will be pushed.
  - Then bump the rev in Wyrd's root `Cargo.toml`.
- **Git:**
  - Commit each completed group with the local identity only. No
    `git config`, co-author trailers, stash, rebase or force-push.
  - Wyrd commits use `--no-verify`.
  - Never commit `changes/active/bifrost-variant/spec.md` or
    `tasks/TASK-004-forge-parallelism.md`.
  - End every commit message with:
    `Claude-Session: https://claude.ai/code/session_014nCbNQCtTnQdNzP4JwrG3i`
- **Already decided — don't reopen:**
  - **SPEC-1:** Bifrost's write path keeps using the public
    `parquet-variant*` APIs, and revision 26 is reworded to match. The
    plan's line "drop `parquet-variant*` from `vala-bifrost-redux`" is
    superseded.
  - **FIND-003-12:** settled by a spec wording change. There is no code
    change.

## Fixes

### Group 1: move generic distributed logic into `datafusion-distributed` (user-directed)

Do this group first: it changes code that findings 11 and 7 also touch.

**D1. Plan-wide metrics.**
- **Problem:** Wyrd walks the distributed plan itself to add up scan
  metrics:
  - `oracle/exec.rs` `record_distributed_scan_metrics` and
    `fold_distributed_scan_metrics`;
  - the `Stage::Local` branches in `OracleQueryScanStats::visit` and
    `PhysicalEvidenceWalk::visit`.

  A network boundary's `children()` already returns its local stage plan,
  so following `input_stage()` as well walks every stage twice. That
  double-counts.
- **Fork change:**
  - Add a public function beside `rewrite_distributed_plan_with_metrics` in
    `src/metrics/`, exported from `lib.rs`:
    `pub fn aggregate_plan_metrics(plan: &Arc<dyn ExecutionPlan>) -> MetricsSet`.
  - It visits every node of the plan exactly once through `children()`,
    merges each node's metrics, and returns them aggregated by name.
  - It works on any plan. Pass it the plan returned by
    `rewrite_distributed_plan_with_metrics` to get the executed stages.
  - Unit tests:
    - a two-stage plan whose scan counters are counted once;
    - a plain single-stage plan.
- **Wyrd change:**
  - `record_distributed_scan_metrics` calls the rewrite, then
    `aggregate_plan_metrics`, and reads `bytes_scanned` and the pruning
    metrics by name.
  - Delete `fold_distributed_scan_metrics`.
  - Delete every `Stage::Local` branch in Wyrd's plan walks. The
    output-sort and join evidence walk stays in Wyrd, as a plain
    `children()` walk.

**D2. Release worker partition streams when the query ends.**
- **Problem:** Wyrd wraps every partition stream returned by
  `datafusion-distributed` so it can drop them when a graph settles. The
  wrapper is `oracle/analytical_transport.rs` `AnalyticalGraphExchanges`,
  with `AnalyticalExchangeStream`, `ExchangeSlot` and `retain`/`close`, used
  through `measured_exchange` and `close_exchanges`.
  - Upstream keeps one reader task per `execute_task` alive until every
    stream it returned is dropped.
  - A consumer that stops polling (a finished hash-join build side, an
    early-finished aggregate) holds its stream forever.
  - Its buffered bytes stay charged to the query's memory pool.
- **Fork change:**
  - In `src/worker/worker_connection_pool.rs`, the pool hands out proxy
    streams. Each one is an `Arc<Mutex<Option<inner>>>` slot that the pool
    also retains.
  - Add a `close()` that drops every retained inner stream, taken or not.
    It is idempotent, and holds the lock only for one inner poll.
  - `DistributedExec`'s output stream calls `close()` on every connection
    pool in its plan when it reaches end-of-stream or is dropped.
  - Dropping the inner gRPC partition receivers fires the existing
    `CancellationToken` path in `src/protocol/grpc/worker_client.rs`. That
    ends the reader task and cancels the remote work.
  - Fork test: a join whose build side is held but not polled.
    - After the root stream ends, the reader tasks have exited and the
      memory pool's `reserved()` is 0.
    - The same holds after dropping the root stream mid-query.
- **Wyrd change:**
  - Delete `AnalyticalGraphExchanges`, `AnalyticalExchangeStream`,
    `ExchangeSlot`, `close_exchanges` and every `exchanges.close()` call (in
    `analytical.rs` and `analytical_transport.rs`).
  - Keep the per-batch telemetry, which is Wyrd's: `execute_task` in
    `AnalyticalWorkerChannel` still applies `record_exchange_transfer`
    through a plain `inspect`.
  - In `AnalyticalGraphLifecycle::settle`, the "close the graph's exchanges"
    step goes away. Keep the rest of the order: drop the unfolded plan and
    the participant grants, wait for the envelope to go idle, release the
    graph, then release the admission owner. Update its rustdoc to match.

### Group 2: correctness

**FIND-003-1. Peer contract does not bind the fork revision.**
- **Problem:** `ORACLE_VARIANT_SQL_VERSION` was deleted from
  `oracle/peer.rs` `stage_body_digest` and `oracle/codec.rs`
  `physical_plan_fingerprint`, and nothing replaced it. Workers on different
  DataFusion builds exchange plans silently.
- **Fix:**
  - Add `const ORACLE_DATAFUSION_REVISION: &str` in `codec.rs`, holding the
    full pinned datafusion rev.
  - Hash it into both digests through one private helper.
  - Do not bump the protocol versions.
- **Tests:**
  - A different revision gives a different digest.
  - Follower preflight refuses the mismatch.
  - The constant equals the root `Cargo.toml` datafusion rev, read with
    `include_str!` and a plain string search, with no new dependency.

**FIND-003-11. Analytical scan stats mix units.**
- **Problem:** after D1 removes the double walk, one problem remains.
  "Files scanned" counts files, while DataFusion's
  `files_ranges_pruned_statistics` counts byte ranges.
- **Fix:**
  - Count `PartitionedFile` ranges on both sides of the subtraction,
    replacing `distinct_files`.
  - For a `DistributedExec` root, take bytes and pruning only from the D1
    aggregate.
- **Test:** `oracle::exec::tests::distributed_scan_stats_count_each_range_once`,
  built next to `physical_root_alone_selects_query_class`. Over two files, a
  plain scan and a GROUP BY report the same counts, with each range counted
  once.

**FIND-003-13. Planning ignores the query deadline.**
- **Problem:** `plan_physical` (`oracle/mod.rs` ~2859-2902) reads manifests
  and footers with no deadline.
- **Fix:** wrap it in `timeout_at(deadline)`, the same pattern used near
  `oracle/mod.rs` ~3702, and map the timeout to `QueryTimeout`. Planning
  before admission stays as it is (the plan approves it).
- **Test:** an Oracle journey that stalls a planning read with `arm_stall()`
  (in `crates/wyrd/wyrd-testing`) under a short deadline, and asserts
  `QueryTimeout`.

**FIND-003-14. Recovery re-uploads already-published files.**
- **Problem:** `parquet/object_uploader.rs` `converge` always PUTs, and is
  reached from scribe persistence recovery. Spec REQ-034 says a published
  key is never uploaded again.
- **Fix:**
  - Call `verify_remote` first; on a match, return with no PUT.
  - If the object is missing or its content contradicts, fall through to
    the existing put-and-verify loop.
- **Test:** three tests with a PUT-counting object store (reuse an existing
  test store wrapper if there is one) for a matching, an absent and a
  contradicting key. They expect 0, 1 and 1 PUTs.

**FIND-003-9. Dead error variants hide a broken missing-file check.**
- **Problem:** in `storage/error.rs`, `from_object_store`,
  `PermissionDenied` and `Backend` are only constructed in tests. Because of
  that, `is_missing_object` never matches a real missing file, so the
  planner's retry on a missing file (`oracle/planner.rs` ~266) never fires.
- **Fix:**
  - Delete the three test-only items.
  - Make `is_missing_object` match the error production actually returns
    for a missing object.
- **Test:** assert that it matches that real error, and that the planner
  retry fires.

### Group 3: missing or weakened proof

**FIND-003-2. AC-007 nested-leaf reads are unproven for lists, `unnest` and
distributed queries.**
- **Unit tests** in `oracle/nested_pushdown.rs`, on the existing
  `HotTable`/`events` fixture. Assert that only the leaf is decoded for:
  - `events[1]['attributes'] ->> 'k'`;
  - `unnest(events)['attributes'] ->> 'k'`.
- **Journey:** in
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs`
  (the distributed nested journey), assert that the bytes scanned for the
  nested-leaf query are lower than for `SELECT events`.

**FIND-003-3. The late-failure proof was weakened (REQ-019 ¶2).**
- **Problem:** the journey proving "rows stream out, then a late error
  arrives after 8192 rows" was loosened. The Rust SDK case was deleted from
  `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` (~2858), and the Python
  and TypeScript assertions were weakened. Use `git diff b145767dd 861a0342f`
  to find the originals.
- **Cause:** the fixture's 16384-row inexact row count makes DataFusion add a
  round-robin repartition, so the failing row can arrive first.
- **Fix:**
  - Change the fixture (`crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs`
    ~932 and its builder) so the read is one ordered, single partition.
  - Restore the mandatory late error after `delivered == 8192` on all four
    surfaces: Rust Oracle, Rust SDK, Python and TypeScript.

### Group 4: generated files and checks

- **FIND-003-5:**
  - Regenerate the stale `wyrd.v1.bin` proto snapshot with the
    `WYRD_PROTO_SNAPSHOT=1` rebuild that `check:proto-drift` documents in
    `mise.toml`.
  - Run `mise exec -- cargo hakari generate && mise exec -- cargo hakari manage-deps`.
- **FIND-003-6:** add `wyrd-types` to `FAMILY_SHARED` in
  `scripts/test-families.sh`.

### Group 5: delete dead code

- **FIND-003-8:** delete `reconciliation_limit_bytes`:
  - the field in `oracle/mod.rs`;
  - in `crates/wyrd/wyrd-server/src/boot/mod.rs`, its computation, the boot
    refusal when it is zero, and the argument.
- **FIND-003-10:** delete `PublishedAssignment` from `oracle/exec.rs`, with
  its `assigned` parameter and the `Some` branch. Its only caller passes
  `None`.
- **FIND-003-20:**
  - In `crates/shared/wyrd-queue/src/variant.rs` `mask_placeholders`,
    replace the hand-written `ShreddedResiduals` walk with
    `parquet_variant_compute::unshred_variant`, followed by the existing
    `validate`.
  - Delete `ShreddedResiduals`.
  - The fixture for `variant::tests::shredded_residuals_pass_the_read_gate`
    needs field `a` in its metadata to pass after this change.
  - If a depth limit must run before unshredding, it belongs in the arrow-rs
    fork, not in Wyrd.

### Group 6: docs and style

- **FIND-003-4:** fixture comments and the plan give the wrong reason a
  one-object table stays Interactive.
  - Sites:
    - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_lifecycle.rs`
      ~105;
    - `analytical_activation.rs` ~744;
    - `capacity.rs` ~758, ~820, ~1815;
    - the plan ~618-627.
  - The correct reason: one object is one file group and one partition. With
    an inexact row count and row estimates trusted, no repartition is added,
    so there is no stage boundary. Two objects do distribute.
- **FIND-003-7:** fix docs that no longer match the code.
  - `architecture/bifrost-design.md`:
    - Delete the metric `bifrost_oracle_file_pruning_total` (~1221); nothing
      emits it.
    - Rewrite ~715-718 ("no precharge") to describe the footer-cache charge
      taken when the cache is set up, as `with_metadata_cache` does.
  - `oracle/exec.rs`: fix the rustdoc on `hot_parquet_scan`, `row_count` and
    `scan()`, plus the module doc.
  - `abort_selected_owners` and `abort` are async but never await. Make both
    plain functions, drop the `.await` at their callers
    (`crates/wyrd/wyrd-server/src/app/server.rs`, `boot/mod.rs`), and remove
    the false shutdown claims in their docs.
  - Fix the stale rustdoc in `analytical.rs`, `follower.rs`, `session.rs`,
    `bench_support.rs`, the `benches/bifrost_reads` module doc,
    `resources.rs` (the "classifier" sentence) and
    `tests/bifrost/oracle/peer_network/analytical.rs` (lines 36-39).
  - Update the plan's status table (top of the plan file).
- **FIND-003-15:** remove `REQ-0xx` and `ponytail:` references that this task
  added to code and manifests: root `Cargo.toml`, `object_uploader.rs` and
  `forge/compact.rs`. Older ones not added by this task stay.
- **FIND-003-16:** in the items this task added (`bench_support.rs`,
  `oracle/bindings.rs`, `resources.rs`, `oracle/mod.rs`, forge test
  support), move function-scoped `use` statements to the top of the module
  (cfg-gated where needed) and shorten fully qualified paths.
- **FIND-003-17:** `crates/wyrd/wyrd-storage/src/signer.rs` `BackendSigner`
  defines each operation twice, once per cfg. Write one method per operation
  that returns the crate's existing boxed `SignerFuture` and matches on
  `self` inside it. Delete the copies. No `#[allow]`; clippy must pass with
  and without `--all-features`.
- **FIND-003-18:** add `# Panics` sections to the new helpers that can panic:
  - `forge/compact.rs`;
  - `forge/orphan_gc.rs`;
  - three test helpers in `oracle/exec.rs`;
  - `parquet/object_uploader.rs`;
  - two in `crates/wyrd-spec/src/vala/assignment_authority.rs`;
  - two in `benches/bifrost_reads/main.rs`.

  Also add rustdoc to `HOUR_MICROS` in `oracle/live.rs`.
- **FIND-003-19:** add `--profile dist` to the `bench:bifrost:reads` task in
  `mise.toml`. The Step 5 numbers in the plan that are labelled `dist` were
  actually built with the release profile, so relabel them.

## Done when

- Every new test above passes, with output shown in the exact
  `mise exec -- cargo nextest run --locked -p <crate> --lib -E 'test(=…)'`
  form.
- The fork's checks pass and the new fork commit is pushed and pinned in
  Wyrd.
- `git grep` returns nothing for:
  - `AnalyticalGraphExchanges`
  - `fold_distributed_scan_metrics`
  - `reconciliation_limit_bytes`
  - `PublishedAssignment`
  - `ShreddedResiduals`
  - `bifrost_oracle_file_pruning_total`
- These pass:
  - `mise run test:bifrost`, including the Oracle journeys, the
    analytical lifecycle and the MCP leak journey;
  - `mise run fmt`
  - `mise run lints`
  - `mise run codegen:check`
  - `mise run check:proto-drift`
  - `mise run check:workspace-hack`
  - `mise run check:test-coverage`
  - `mise run docs:check`
  - `git diff --check`
- Last, `mise run gate`, since the toolchain and the root patch set changed.
- Evidence (an acceptance table and any diagnoses) is appended to this file.
  Then route to `$wyrd-task-review` r2.
