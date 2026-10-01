# TASK-004 behavior review (behavior-rev)

- Subject: base `a56ab7569` .. candidate `990803fc0` (worktree HEAD `78bd1049b`
  differs only by `changes/active/opitimization-and-benchmarks/spec.md`;
  verified).
- Authorities: `changes/active/bifrost-scribe-live-reads/spec.md` rev 24; task
  `tasks/TASK-004-integrate-eval-server-and-unify-bifrost-memory.md` (cited
  below as `T:<line>`); AGENTS.md.
- Method: static review only. I traced client → server → client paths in
  source and read every selector named in the proof matrix at the candidate. I
  ran no lanes, builds or benchmarks. The benchmark reports under
  `target/bifrost-query-capacity/{standard,heavy}/report.md` were read, not
  re-run.

## Findings

### B-1 — INCORRECT: R13-D evidence cites a deleted test, and the retry wait has no cancellation coverage

- **Obligation.**
  - R13-D (`T:761`) says only the leader retries pre-accept peer capacity,
    within the deadline and cancellation.
  - The proof matrix and command list (`T:202`) name
    `oracle::dispatcher::tests::leader_retries_only_preaccept_peer_capacity`.
  - AGENTS §12 does not allow a deleted test without a recorded reason.
- **Location.**
  - The test was added in `65da92a03` and deleted in `1a6ec2554` (the D10
    fragment-worker deletion).
  - Production code lives at `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:68`
    (`wait_for_peer_capacity`) and in the reserve loop at
    `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs` around
    lines 3058-3120.
- **Evidence.**
  - The selector does not exist at the candidate.
  - The task acceptance row (`T:761`) still marks it PASS, and D10
    (`T:815-856`) does not mention deleting it.
  - The substitute unit coverage is
    `analytical.rs::participant_cut_is_reserved_once_immediately_before_dispatch`
    (~7099). It uses `assert_refused_round_retries_within_deadline` (~6964) and
    `assert_partial_reservation_releases_exactly` (~6921). These cover
    retry-within-deadline and "lost peer is not retried".
  - Neither that unit nor the journey
    `peer_network/analytical.rs::two_leaders_retry_preaccept_capacity` (1482)
    cancels a query while it is parked in `wait_for_peer_capacity`.
- **Consequence.**
  - The recorded R13-D evidence points at a test that cannot run, so the
    command at `T:202` fails or selects nothing.
  - Cancellation during the leader retry wait is unproven. A regression that
    keeps a cancelled leader parked, or keeps peer reservations held after
    cancellation, would pass.
- **Required correction.**
  - Update `T:202`/`T:761` to the real selectors and record the deletion under
    D10.
  - Add a focused test that cancels a leader during the pre-accept capacity
    wait. It must assert a prompt terminal and zero retained peer
    reservations.

### B-2 — INCORRECT: a Scribe-only target stays ready after a WAL fault

- **Obligation.** `T:307-325` (Scribe failure boundary): "A Scribe-only target
  is unready."
- **Location.** `crates/wyrd/wyrd-server/src/components/health/mod.rs:195-197`:

  ```rust
  scribe_fault_is_role_local: state.bifrost.oracle().is_some()
      || state.bifrost.forge().is_some()
      || state.verification.is_composed(),
  ```

  `all_ok` (mod.rs:~150) excuses the Scribe fault whenever this flag is true.
- **Evidence.**
  - `BifrostTarget::Scribe` serves the API (`crates/wyrd/wyrd-server/src/config.rs:200`).
  - `app/server.rs:651` composes the verification runtime for API targets when
    `verification.enabled`, which is true by default (config.rs ~1638).
  - `VerificationRuntimeBuilder::build` composes its workers whenever an
    operator pool exists.
  - So with `WYRD_TARGET=scribe` and the default verification config,
    `verification.is_composed()` is true.
  - A WAL fault then leaves `/readyz` at 200, even though this pod's only
    Bifrost role (Scribe) is faulted.
  - The only journey, `server/owner_inspection.rs::scribe_wal_fault_is_role_local`
    (151), covers the combined target. No test covers a Scribe-only target.
- **Consequence.**
  - Kubernetes keeps routing ingest to a Scribe pod that refuses every write.
  - Fault isolation for a dedicated Scribe deployment is defeated: the
    readiness gate the boundary requires never trips.
- **Required correction.**
  - Derive role-locality from whether a *Bifrost* role other than Scribe
    (Oracle/Forge) is hosted. Alternatively, explicitly treat a target whose
    only Bifrost role is Scribe as not role-local.
  - Add a test (unit on `compute_snapshot`/`all_ok`, or a journey with
    `BifrostTarget::Scribe` plus default verification) that asserts `/readyz`
    is unready after a WAL fault.

### B-3 — MISSING: no TASK-006 replay ledger

- **Obligation.**
  - AC1 and approach step 1 require a source-to-destination replay ledger
    (`T:39`, `T:269`, `T:648`): "Record command exits, an acceptance-to-evidence
    table, the replay ledger, …".
  - `T:266-269` says any carried gateway or Card closure must be "record[ed]
    … in the replay ledger".
- **Location.**
  - The task packet `changes/active/bifrost-scribe-live-reads/tasks/TASK-004-…md`.
  - Nothing under `changes/active/` outside `review/` contains a ledger.
- **Evidence.**
  - The only "replay ledger" text is the instructions themselves.
  - Replay commit `4ae6a1992` deleted, with no ledger entry:
    - the authz check route;
    - `PolicyHook` and its tests;
    - the public codes `WYRD_AUTHZ_403_POLICY_DENIED` and
      `…REQUIRES_DELEGATED_TOKEN`.
  - VCC rev 44 REQ-166 approves those deletions, but the ledger that must
    attribute them to that approval does not exist.
- **Consequence.**
  - AC1 cannot be verified.
  - Public error-code removals and route deletions have no recorded
    source-to-destination justification, so a reviewer cannot tell an
    approved carry from accidental import.
- **Required correction.** Add the ledger to the task evidence. For each
  replayed source symbol or commit, give the destination file, a status
  (replayed / already present / superseded / deleted), and the approving
  requirement. Include the authz/PolicyHook/error-code deletions with
  REQ-166.

### B-4 — MISSING: no Python or TypeScript proof of the resource-exhaustion terminal

- **Obligation.**
  - `T:343` requires that "Rust/Python/TypeScript preserve the same stable code
    and do not auto retry", and that a client rejects the whole stream,
    including rows already received.
  - AGENTS §11 requires journeys on every user-facing SDK surface the
    capability ships.
- **Location.**
  - The Rust test exists:
    `crates/shared/wyrd-client/src/bifrost/query.rs:2663`
    (`resource_terminal_rejects_partial_rows`).
  - `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` adds the code.
  - No Python or TypeScript test references
    `WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED` or a post-stream `Failed`
    terminal.
- **Evidence.** I searched the Python and TypeScript test trees. Neither has a
  case that receives this code before the stream starts or after rows have
  arrived.
- **Consequence.**
  - The language projections could surface partial rows as success, or fail to
    project the stable code, and no test would notice.
  - The negative flow "post-stream Failed terminal rejects partial rows in the
    Python/TypeScript clients" is unproven.
- **Required correction.** Add Python (`integration` marker) and TypeScript
  integration cases, or record a specific justified reason under AGENTS §11.
  Each case must drive a resource-exhaustion terminal after at least one row
  and assert:
  - the typed error carries `WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED`;
  - no rows are returned as success;
  - there is no retry.

### B-5 — INCORRECT: the Forge unit test is weaker than its matrix claim

- **Obligation.** `T:571`: "Hold Oracle/Scribe charges; run Forge bounded
  DataFusion growth and spill, force exhaustion/cancel, prove root returns
  bytes only after work ends; assert normal temporary-file release under
  `forge-spill`."
- **Location.**
  `crates/vala/vala-bifrost-redux/src/forge/managed/executor.rs:627`
  (`rewrite_pool_charges_root_and_releases_on_cancel`).
- **Evidence.** The test composes Forge alone:
  - it holds no Oracle or Scribe charge;
  - it creates no spill file and checks no `forge-spill` file release;
  - it drops the reservation before asserting, so it never shows that bytes
    stay charged while work is still running.

  The journey `forge/live_rewrite.rs::failed_memory_attempt_retries_without_partial_publication`
  (2072) covers retry and no partial publish, but not the shared-root,
  spill-release or held-until-end properties.
- **Consequence.** The following MEM-002 failure modes would pass the named
  test:
  - Forge releasing root bytes before its execution ends;
  - spill files leaking under `forge-spill`;
  - Forge growth ignoring concurrent Oracle/Scribe charges.
- **Required correction.** Extend the unit test, or add one, to:
  - hold Scribe and Oracle charges on the same governor;
  - run a spilling rewrite to exhaustion or cancellation;
  - assert the root charge is still held while the plan is live and returns to
    baseline only after it ends;
  - assert that `forge-spill` is empty afterwards.

### B-6 — MISSING: the R13-B journey does not prove a live read under memory pressure

- **Obligation.** `T:140-150`: "Under memory pressure it stays readable from
  live authority; a failed stage attempt retries …". The RED test must
  "assert ACK identity, live read, retry …" (`T:146-149`, `T:575`).
- **Location.**
  `crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs:1141-1160`
  (`acknowledged_rows_survive_stage_pressure_and_restart`).
- **Evidence.**
  - The live read `sorted_values(&client, &table)` at line 1157 runs *after*
    `drop(occupant)` at line 1156, which releases the root pressure first.
  - The only check during pressure is `published_rows == 0` (1151-1155). That
    is also true if no stage attempt ran at all, so the test never shows that
    an attempt failed and was retried.
- **Consequence.** A regression where a live read is refused under memory
  pressure (Oracle charges the same root), or where staging is silently
  skipped rather than retried, still passes.
- **Required correction.**
  - Move a live read before `drop(occupant)` and assert it returns every
    acknowledged row.
  - Assert at least one recorded failed stage attempt, using an existing stage
    metric or test hook, before the retry flush.

### B-7 — DRIFT: dead production API left by the R13 producer-wait deletion

- **Obligation.**
  - R13-A deletes the producer byte wait.
  - The Ponytail ladder rung "delete" applies, as do AGENTS §15 ("remove or
    decline speculative work") and §5.
- **Location.**
  - `crates/vala/vala-bifrost-redux/src/resources.rs:1495-1513`:
    `ScribeResources::memory_epoch`, `wait_for_memory_change`.
  - The governor side at `resources.rs:2394-2416`.
  - The `memory_epoch` state at `resources.rs:829`, with its notify at
    ~line 1970.
- **Evidence.**
  - At the base, the only production caller was the producer byte wait in
    `persistence.rs`, which R13 deleted.
  - At the candidate, the only callers are the test at `resources.rs:5899-5911`
    (`accepted_replay_capacity_wait_is_bounded_and_cancellation_safe`).
- **Consequence.**
  - A public wait-for-bytes API survives that the approved design removed.
  - It invites a future reintroduction of the deleted future-byte hold-back,
    and every governor charge still pays epoch bookkeeping for no reader.
- **Required correction.**
  - Delete `memory_epoch`/`wait_for_memory_change` along with their state and
    notify.
  - Delete or retarget the test that exists only to exercise them.

### B-8 — DRIFT: unrecorded scope in TASK-004-attributed commit `1a6ec2554` (D10)

- **Obligation.**
  - AGENTS §15: inspect and record before adding or removing contracts and
    dependencies.
  - The task requires every deviation to carry a recorded diagnosis or
    approval.
  - D10 (`T:815-856`) lists exactly what that commit deletes.
- **Location.** Commit `1a6ec2554`, "refactor(oracle): one execution path and
  one slot unit per query". It touches:
  - `Cargo.toml`/`Cargo.lock`;
  - `.github/workflows/release.yml`;
  - `sdks/wyrd-sdk-python/Cargo.toml`;
  - `crates/wyrd-spec/src/vala/error.rs`;
  - `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts`;
  - `crates/vala/vala-bifrost-redux/src/{gate,storage}/error.rs`;
  - `crates/wyrd/wyrd-sql/src/{pool.rs,tenant_conn.rs}`.
- **Evidence.** None of the following appear in D10 or anywhere else in the
  task or spec outside `review/`:
  - git revision bumps of `iceberg`/`iceberg-catalog-sql`/`iceberg-datafusion`
    (`a2f7cd2`→`330fe33`) and `iceberg-compaction-core`
    (`74e4937`→`6773e19`);
  - the release workflow switched to `--profile dist`, and the per-crate
    `[profile.release]` removed from the Python SDK manifest;
  - deletion of the public error `IngestOversized` /
    `WYRD_VALA_413_INGEST_OVERSIZED` and of the `i32::MAX` row guard
    (`ScribeError::TooManyRows`);
  - the `TenantConn` begin-order change in `wyrd-sql`. This later caused the
    pool-poisoning bug that D19 had to fix.

  The `pg_router_smoke.rs` `oracle_authority_*` test deletions in the same
  commit do follow from D10's recorded `install_reader_authority` deletion and
  are not counted here.
- **Consequence.**
  - A public stable error code disappears from the wire and the TypeScript SDK
    with no approval trail.
  - Dependency and release-profile changes ship under an Oracle-slot refactor,
    with no diagnosis tying them to D10.
  - D19 shows one of these riders already caused a production-grade defect.
- **Required correction.** For each item, either:
  - record the approval or diagnosis in the task (D10 or a new D-entry) and
    cite the spec revision that removes `IngestOversized`; or
  - revert the unrelated rider.

  Restoring the removed code requires restoring its owner test.

## Acceptance matrix

| Requirement/criterion/constraint/non-goal | Implementation evidence | Verification evidence | PASS/FAIL |
|---|---|---|---|
| AC1 — replay TASK-006 Eval/server/boot with a replay ledger | Replay commit `4ae6a1992`; VCC spec rev 44 carried | No ledger exists (B-3) | FAIL |
| AC2 — one boot graph and one readiness snapshot; peer readiness after listener | `boot/mod.rs::compose_bifrost`, `components/health/mod.rs::compute_snapshot` | `peer_join_and_remote_query` (join.rs:58); readiness journeys | PASS (except Scribe boundary, see below) |
| AC3 — one memory governor, shared cap = limit − server minimum | `resources.rs` ~1983-2010 `shared_memory_cap`, governor ~2609-2830 | `shared_cap_defaults_overrides_and_concurrent_charges` (resources.rs:4993) | PASS |
| AC4 — typed QueryResourcesExhausted, query-local | `wyrd-spec/src/vala/error.rs:95-106`; `oracle/mod.rs:4305` `map_datafusion_error`; `query_stream.rs:1276` | `memory_failure_is_query_local_and_typed` (oracle/capacity.rs:1434); `resource_failure_has_no_retry_hint`; `resource_exhaustion_has_no_retry_after`; Rust `resource_terminal_rejects_partial_rows` | FAIL (Python/TS, B-4) |
| AC5 — Forge on the shared governor, forge-spill under the data root | `forge/managed/executor.rs`; `data_root.rs:107-164` | `prepare_clears_stale_forge_spill` (295); journey `failed_memory_attempt_retries_without_partial_publication` (2072); unit is weaker than claimed (B-5) | FAIL |
| AC6 — benchmarks and gate evidence | `target/bifrost-query-capacity/standard/report.md` (8 CPU/16 GiB, 10M rows, selective peak 1222.3 qps, one-client 6.8/8.2/8.9 ms, overflow = 429 QUERY_QUEUE_FULL); `heavy/report.md` peak 6.37 GiB, all PASS | Reports predate `31171d8d7`/`35343dd8f`/`990803fc0`, which are test/sql only. Gate is run by the orchestrator. | PASS (benchmark); gate pending orchestrator |
| R13-A — one Scribe held-byte owner, no second breaker | Deleted ScribeContentionLedger, ContentionReserveVector, ScribeGlobalCapacity, memory_breaker_bytes and others (sweep clean) | `one_root_charge_has_no_secondary_memory_ceiling` (scribe/admission.rs:350); `decode_to_memtable_transfers_one_charge` (scribe/ingress.rs:932) | PASS (dead epoch API remains, B-7) |
| R13-B — ACKed request readable under pressure; stage retry; restart; no duplicate | Producer wait deleted; stage retains generation | `acknowledged_rows_survive_stage_pressure_and_restart` (write_read.rs:1109): live read happens after pressure is released; failed attempt not proven (B-6). Restart, single publication and resend-dedup are asserted. | FAIL |
| R13-C — storage permit wait bounded by operation deadline; no fixed footer slot | `storage/mod.rs` `acquire_request` (~915) used by `governed_decode` (459) and `attempt_once` (857) | `occupied_storage_permit_waits_within_operation_deadline` (1493); `footer_decode_has_no_fixed_memory_slot` (1579) | PASS |
| R13-D — leader-only retry of pre-accept peer capacity | `dispatcher.rs:68`, `analytical.rs` reserve loop | Named selector deleted; cancellation-during-wait uncovered (B-1); substitute `participant_cut_is_reserved_once_immediately_before_dispatch`; journey `two_leaders_retry_preaccept_capacity` | FAIL |
| Scenario 1 — enqueue failure leaves ACK; replay gives no second enqueue | Eval ingest path | `integrated_enqueue_failure_preserves_ack` (eval_verification.rs:1252): trigger refusal, attempt count, observation stored once, no runs/results | PASS |
| Scenario 2 — Scribe WAL fault is role-local on the combined target | `wal.rs:1954` `mark_faulted`; `state.rs:931` monitor; health `probe_scribe` | `scribe_wal_fault_is_role_local` (owner_inspection.rs:151) | PASS (combined target) |
| Scenario 3 — Forge memory failure retries, no partial publish | executor + live rewrite | `failed_memory_attempt_retries_without_partial_publication` | PASS |
| Scenario 4 — peer mTLS typed context; wrong creds / stale fence / cross-tenant refused | `PeerContext`, `peer_service.rs` | `peer_context_refusals` (security.rs:27); `mise.toml:692` `test:server:peer` | PASS |
| Scenario 5 — memory failure is query-local and typed | Oracle mapping | `memory_failure_is_query_local_and_typed`: queued and sibling queries succeed, drained, health clean | PASS |
| Error table — queue full 429 with retry | `grpc/query.rs:260` `query_status`; HTTP `query/routes.rs:468-475` | `resource_failure_has_no_retry_hint` (also asserts 429 retryable) | PASS |
| Error table — resource exhausted 503 with no retry-after (pre-stream) | `http/error.rs:110`, `grpc/query.rs:260` | `resource_exhaustion_has_no_retry_after`, `resource_failure_has_no_retry_hint` | PASS |
| Error table — post-stream Failed terminal rejects partial rows (Rust/Python/TS), no auto retry | `query_stream.rs:1276`; Rust `collect_bounded`; no query auto-retry in client | Rust unit only (B-4) | FAIL |
| Scribe failure boundary — Scribe-only target unready | `health/mod.rs:195-197` | None; implementation contradicts it (B-2) | FAIL |
| Scribe failure boundary — fail-stop kept for poisoned governor | process resource-health watcher | WAL journey asserts the poison terminal contains `bifrost_resource_health` | PASS |
| Memory charge contract — infallible DataFusion growth as headroom; transport leases; OTLP decode transfer | governor `reserve_pool_memory_infallible`, `try_charge_transport` | `shared_cap_defaults_overrides_and_concurrent_charges`; `decode_to_memtable_transfers_one_charge` | PASS |
| Config `WYRD_SERVER_MEMORY_MIN_BYTES` / `bifrost.resources.server_memory_min_bytes` (1 GiB default) | `config.rs:2514-2520`; `resources.rs:41`; docs `kubernetes-production.svx:666-677`, `bifrost-design.md:475` | `server_memory_minimum_rejects_impossible_plan` (config.rs:3519); `scribe_boot_rejects_expanded_request_above_cap` (boot/mod.rs:2400) | PASS |
| Old unmanaged-reserve setting deleted with no alias | `deny_unknown_fields`; env var inert | Same config test asserts rejection and inertness; repo grep clean | PASS |
| Constraint — no recovery supervisor; no process exit on role fault | `mark_faulted` role-local cancel token | WAL journey restart path | PASS |
| Constraint — no compatibility aliases / legacy names | Sweep of deleted symbols clean | grep | PASS |
| Constraint — every test/timeout/ignore change has a diagnosis (D1-D19) | D2, D3, D6, D13 and D18 recorded; D19 cause and fix credible (`tenant_conn.rs` one-round-trip bind) | R13-D test deletion unrecorded (B-1); `1a6ec2554` riders unrecorded (B-8) | FAIL |
| Non-goal — no gateway/Card import beyond the closure | `4ae6a1992` deletions approved by REQ-166 | Not ledgered (B-3) | FAIL |
| Stop conditions — none triggered silently | Spec rev 24 node change (8/16) user-approved; D10/D14 user-approved | Task record | PASS |

## D1-D19 claim assessment (summary)

- **Credible, with the fix at the cause:**
  - D2, D3, D5, D6, D7, D13 and D18;
  - D14 (user decision);
  - D15, D16 and D17 (recorded cause and fix site);
  - D19. `begin_tenant_sql` binds the tenant before `BEGIN` in one round trip.
    Its Postgres test skips without `WYRD_DATABASE_URL`, which matches the
    vala-sql precedent; `test:sql` sets that variable.
- **Incomplete:**
  - D10 omits the deleted R13-D unit selector (B-1) and the commit's unrelated
    riders (B-8).
  - D9, D11 and D12 are consistent with the source I read.

## Overall result

FAIL
