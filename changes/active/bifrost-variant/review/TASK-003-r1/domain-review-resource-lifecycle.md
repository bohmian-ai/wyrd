# TASK-003 r1 domain review: concurrency and resource lifecycle

Reviewer: `domain-rev` (resource lifecycle), fresh and independent, under
`$wyrd-task-review`. Subject: base `b145767dd` → candidate `861a0342f` on
`wyrd/bifrost-variant/TASK-003`. No source was modified.

## Reviewed boundary

Who owns Oracle query resources, and how they are cleaned up, under native
`datafusion-distributed`:

- admission guard and `LeaderStreamOwners` drop order;
- the query memory pool (`GovernedMemoryView` over `TrackConsumersPool`) and the
  footer-cache charge;
- leader graph lifecycle: `AnalyticalGraphLifecycle::settle`, `Drop`,
  `AnalyticalGraphRelease::{release, reclaim}`;
- metric fold retention (`AnalyticalGraphMetricFold`,
  `fold_physical_metrics`, `take_metric_fold`);
- closing `AnalyticalGraphExchanges`;
- follower settlement;
- worker plan rewrite (`TenantWarehouseReads`);
- the staged-lease lifetime in `hot_parquet_scan`;
- live Scribe fragment dispatch (`LiveScribeExec` pushdown and route pruning);
- cancellation and deadlines on every terminal (success, failed, cancelled,
  consumer drop mid-stream, deadline);
- ceiling refusals and readiness.

## Authority and source coverage

Authority read:
- `AGENTS.md` §§4–6, 11, 12.
- `architecture/bifrost-design.md`: Distributed analytical execution, Admission
  and memory, and Telemetry. Both candidate and base were compared.
- Spec REQ-032/033 (working tree).
- The governing plan's Step 4, including the settle-fix entry.
- The `TASK-003-revision-query-latency.md` A4 row on the footer cache.

Source read at the candidate:
- `oracle/analytical.rs`:
  - lifecycle, settle, `Drop`, release, reclaim and metric fold
    (2320–2690, 7340–7523);
  - session construction (6960–7200).
- `oracle/analytical_supervisor.rs`:
  - `finish_attempt`;
  - `AnalyticalAttemptGuard`;
  - `take_metric_fold`;
  - the fold test fixture.
- `oracle/analytical_transport.rs`: `AnalyticalGraphExchanges`.
- `oracle/query_stream.rs`:
  - `build_frames`;
  - `LeaderStreamOwners`;
  - `settle_and_finish_stream`.
- `oracle/mod.rs`:
  - `execute_retained_root`;
  - `settle_attempt_output`;
  - `settle_distributed_failure`;
  - `release_error`;
  - `build_physical_root`;
  - `run_sql_query` / `run_claimed_query` / `run_sql_attempt` /
    `classify_one_build`;
  - `plan_physical`.
- `oracle/exec.rs`:
  - `RemoteScanMetrics`;
  - `OracleQueryScanStats`;
  - `record_distributed_scan_metrics`;
  - `TenantWarehouseReads`;
  - `hot_parquet_scan`;
  - `cache_footers` / `cache_footer` / `persisted_input` / `scan`.
- `oracle/live.rs` diff.
- `oracle/follower.rs` diff, plus the staged-lease test.
- `oracle/bindings.rs` and `oracle/session.rs`.
- `resources.rs` diff.
- `storage/mod.rs` (`BifrostStorage::with_metadata_cache`).
- `wyrd-server/src/boot/mod.rs` diff.

Fork: in `datafusion-distributed`, `0b81b12` differs from the base pin
`22a395f` only by `8647526` (task count bounded to `1..=budget`) and the
snapshot refresh. Neither touches worker connections, reader tasks or metric
channels. Upstream `random_routing`, `routed_urls` (one routing per stage, used
for both plan push and execution), `DistributedExec::execute` (the
`end_query_guard` in the spawned task) and `rewrite_distributed_plan_with_metrics`
were read at `0b81b12`.

## Verification limits

- I ran no lanes. I relied on the recorded evidence: `test:bifrost` passed with
  Oracle journeys 53/53, and `oracle::` unit tests passed 174/174.
- Findings below rest on static reading of the candidate tree and the fork pin.
- RES-1 was not reproduced dynamically. No current test bounds planning time
  against the query deadline.

## Checked and found sound

These are not findings:

- **Settle fix ordering.**
  - Success: `fold_physical_metrics` takes the fold first (`analytical.rs:2598–2602`).
    Inside `AnalyticalGraphMetricFold::settle`, the executed plan and the
    rewritten plan drop before it returns. The `timeout` drops the fold when the
    deadline expires. Either way, the later
    `drop(self.supervisor.take_metric_fold(..))` at `:2625` is a no-op.
  - Non-success: the fold is dropped after `close_exchanges` and before
    `release().release()`. This removes the self-wait the plan diagnosed: a
    `CollectLeft` `OnceAsync` reservation held on the plan node inside the graph
    entry.
  - `Drop` (`:2876`) drops the fold after revoking grants and the attempt, and
    before `reclaim`.
  - `take_metric_fold` releases the supervisor mutex before the caller drops the
    plan. No `Drop` runs under the graph lock.
  - Leader attempts retain no production drivers. The only `retain_driver`
    caller is a test. So joining the attempt cannot wait on the fold's plan.
  - The MCP journey `agent_runs_three_table_analytical_query_through_mcp`
    exercises the fixed path end to end.
- **Leader stream drop order.**
  - `build_frames` drops `batches` before `settle_and_finish_stream`.
  - On a consumer drop, generator state drops `batches` (declared last) before
    `LeaderStreamOwners`. `LeaderStreamOwners::drop` drops admission (and the
    graph lifecycle) before the active-read claim.
  - An aborted `DistributedExec` driver task still holds `head_stage` until a
    later poll. The bounded poll loop in `AnalyticalGraphRelease::release`
    covers that.
- **First-batch failures.** `settle_distributed_failure` and `release_error`
  drop the admitted guard. Its lifecycle `Drop` then revokes, drops the fold and
  reclaims.
- **Task routing.**
  - Deleting `OracleRouteTasks` reintroduces no plan-push and execution split.
    Upstream calls `routed_urls` once per stage coordinator.
  - The worker resolver lists only remote participants, all of which hold
    grants.
  - The leader installs no `LocalWorkerContext`.
- **Staged lease.**
  - The lease rides as a `PartitionedFile` extension, so it lives exactly as
    long as the plan or stream.
  - It is pinned by `follower::tests::a_dropped_staged_scan_releases_its_lease_immediately`.
- **Worker scans.**
  - `TenantWarehouseReads` only swaps the reader factory.
  - Decoded stages run on `graph.runtime()`, which is the governed per-query
    pool.
- **Live pushdown.**
  - Route pruning only removes fragment dispatches.
  - `scan_metrics` stays one shared `Arc` across the rebuilt leaf.

## Material proposed findings

### RES-1: physical planning now does unbounded, pre-admission object IO outside the query deadline and outside public cancellation (INCORRECT)

- **Violated obligations:**
  - `architecture/bifrost-design.md:643`: "One pinned published cut, one
    discovered live route set, one deadline, cancellation tree … bound the
    complete stage graph."
  - `:804`: preparation "waits within the leader deadline".
  - `AGENTS.md` §6: "Use bounded concurrency and timeouts for external calls."
  - The provider's own contract at `oracle/exec.rs:1547–1549`: "Row IO remains
    lazy … Iceberg and hot Parquet bytes are first accessed only when DataFusion
    executes the already audited plan." This rustdoc is now false.
- **Location:**
  - `oracle/exec.rs:1489–1515` (`persisted_input`): `published_files(...)` runs
    Iceberg `plan_files()`, which reads the manifest list and manifests.
    `cache_footers` (`:1435–1450`) then fetches every kept file's footer and
    page index through `DFParquetMetadata`.
  - Both run from `TableProvider::scan` (`:1555–1568`) during
    `Oracle::plan_physical`.
  - `oracle/mod.rs:2898` calls `Self::plan_physical(&planning, context, sql)`
    with no `timeout_at(deadline, …)`. `build_physical_root` uses `deadline`
    only for live discovery (`:2869`).
  - No caller up the chain (`run_sql_attempt` → `run_claimed_query` →
    `run_sql_query` → `query_sql_with_deadline`) bounds it either.
- **Evidence:**
  - At base `b145767dd`, `persisted_inputs` built
    `OracleIcebergScanExec::from_plan(...).with_footers(...)`, and footers were
    read at execution. The candidate moved manifest listing and per-file footer
    and page-index GETs into planning.
  - This IO happens before admission, before the running-query registration
    that public cancellation targets, and before the audited read decision.
  - It runs `meta_fetch_concurrency` fetches at a time against the node-wide
    `LimitStore`. That store is the same permit pool admitted queries' scans
    use.
  - The local backend has no request timeout (`storage/mod.rs`, design
    REQ-033). Waiting for a `LimitStore` permit has no bound.
  - A `cache_footers` failure is swallowed (`let _ = …`), so only latency is
    affected and correctness is not.
- **Observable consequences:**
  - A query whose deadline expires during planning keeps issuing object-store
    GETs for every remaining file, then fails later at admission with
    `QueryTimeout`. The caller's deadline is overrun by up to the full prefetch
    time, which grows with file count (two ranged GETs per published file).
  - Until admission, the query cannot be cancelled through the running-query
    registry; only a client disconnect stops it.
  - Concurrent unadmitted queries consume `LimitStore` permits ahead of admitted
    queries' scans, which bypasses the tenant-fair admission queue for
    object-store IO.
  - The existing stall journey (`published.rs::prove_cancelled_read_terminates`,
    "Stalls one governed read") now arms a read that a published table reaches
    during planning. It still passes only because the process is cancelled
    underneath it.
- **Testable correction:**
  - Bound physical planning by the attempt deadline: wrap `plan_physical` (or
    the provider registration and planning block) in
    `tokio::time::timeout_at(deadline, …)`, mapping elapsed to
    `BifrostError::QueryTimeout`.
  - Correct the `scan` rustdoc to state that manifest and footer IO happens at
    planning.
  - Prove it with an Oracle journey using
    `server.object_reads_for_test().arm_stall()`:
    1. Publish a table.
    2. Arm the stall.
    3. Issue a query with a small `deadline_ms`.
    4. Assert a `QueryTimeout` terminal near the deadline while the stall is
       still held.
    5. Assert that `oracle_admission_total` and the running-query registry never
       saw the query.
    6. Release the stall and assert no further ranged reads come from that query.
- Whether pre-audit footer IO is acceptable is a security/audit question, not
  resource lifecycle. It is flagged here for that domain only.

### RES-2: the design's no-precharge memory rule is contradicted by the new composition-time footer-cache charge (DRIFT)

- **Violated obligation:** `architecture/bifrost-design.md:715–718` (Admission
  and memory) still states: "No role holds a fixed share or a precharge: an idle
  role holds nothing, and every charge returns when its owner ends."
- **Location:**
  - `storage/mod.rs:126–140` (`BifrostStorage::with_metadata_cache`) charges
    the whole `metadata_cache_bytes` once at boot (`wyrd-server/src/boot/mod.rs`)
    and holds it for the process lifetime.
  - By default that is `managed/20`, clamped to 32–512 MiB (`storage/policy.rs:154`).
  - The candidate also edited `bifrost-design.md:1231–1233`, which now says the
    budget is "charged to the Oracle memory root at composition".
- **Evidence:**
  - At base, `storage/cache.rs:827` reserved per loaded entry.
  - The approved revision plan (A4 row: "charged up front to the Oracle root")
    chose the up-front charge, but the governing memory paragraph was not
    updated.
  - The capacity journey was changed to subtract
    `idle.governed_memory_used_bytes`, which concedes that an idle Oracle now
    holds a charge.
- **Observable consequence:**
  - The design authority describes two incompatible memory rules.
  - An operator or reviewer reasoning from §Admission and memory expects an idle
    pod's governed usage to be 0. The pod actually reports up to 512 MiB, and
    every query ceiling refuses that much sooner.
- **Testable correction:**
  - Amend the §Admission and memory paragraph to name the footer cache's
    standing charge (its size and when it is taken) as the single exception.
    Keep `REQ-033`'s `metadata_cache_bytes` meaning.
  - Verify by reading both sections of the edited design; no code change is
    required.

## Overall

**FAIL.**
- RES-1 is a deadline and cancellation regression the candidate introduced on
  the Oracle read path.
- RES-2 is design-authority drift on memory accounting.
- The settle fix itself is ordered correctly on success, failure, consumer drop
  and `Drop`. No leaked tasks, held plan nodes or release races were found in
  the lifecycle, exchange, follower, lease or live-dispatch paths.
