# TASK-003 r1: behavior review (`behavior-rev`)

- Subject: base `b145767dd` → candidate `861a0342f`. The authority is the working-tree `spec.md` plus `spec-revision-26-datafusion-variant.md`. The governing plan is `tasks/TASK-003-native-datafusion-read-path-plan.md`.
- Lens: the path from caller to result.
  - SQL goes through Gate/Oracle, then the Interactive or Analytical path, then the result.
  - The SDK, MCP, and HTTP surfaces.
  - Variant and Struct queries, live Scribe routes, ceilings and cancellation.
  - Every acceptance criterion, negative flow, and non-goal.
- Sources: `git show` and `git diff` only. I ran `cargo hakari generate --diff` on the candidate tree; it exits 1. No source was modified.

## Acceptance matrix

| Requirement / criterion / constraint / non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Rev 26 Oracle registration: every session is built from a `SessionState` with `variant` on. Wyrd has no Variant function, planner, adapter, or codec. | `oracle/session.rs` template (`with_default_features`). `resources.rs:4262` and `oracle/mod.rs:2883` use it. The analytical planning session and the worker session use default-featured builders. `variant_sql.rs` and `leaf_paths.rs` are deleted. | Journey `published::variant_sql_registry_covers_every_session`. `oracle::` 174/174. | PASS |
| Rev 26: `ORACLE_VARIANT_SQL_VERSION` is deleted, **the peer contract fingerprint includes the DataFusion fork revision, and a worker on a different revision refuses before decoding a plan** | The constant is removed from `stage_body_digest` (`oracle/peer.rs:316-320`) and from `physical_plan_fingerprint` (`oracle/codec.rs:27-33`). Nothing replaces it. `PEER_PROTOCOL_VERSION` (5) and `STAGE_PROTOCOL_VERSION` (2) are unchanged. | No test. | **FAIL (BEH-1)** |
| Rev 26 non-goal / INV-008: no Wyrd DataFusion or `datafusion-distributed` logic; the deleted Wyrd analytical and Variant code does not return | There is no `ExprPlanner`, adapter factory, Variant UDF, `ScanPredicate`, task-count handler, or leaf split in production `oracle/**`. The only `DesiredTaskCountHandler` is under `#[cfg(test)]` (`exec.rs:3680`). Distributed hooks use upstream seams (`set_distributed_worker_plan_rewrite_handler`, `set_distributed_children_isolator_unions`). | Source grep. | PASS |
| REQ-025 / plan items 2 and 4: one `DataSourceExec`, shipped by `datafusion-proto`, with the tenant footer proof re-attached on the worker | `TenantWarehouseReads::bind` (`exec.rs:907-990`) refuses a non-Parquet scan or a non-warehouse scan. The tenant comes from the recorded stage identity (`analytical.rs:537-556`). | `exec::tests::decoded_stage_scans_read_only_as_the_bound_tenant`; the `prove_worker_tenant_refusal` journey. | PASS |
| Plan item 5: event-time file pruning from `PartitionedFile` statistics, and an aggregate never answers without a footer proof | `HotFileSource::statistics` gives exact per-file min/max and an inexact row count; the merged statistics are inexact (`exec.rs:1083-1095`). | `event_time_predicate_skips_disjoint_hot_files_before_any_footer_open`. | PASS |
| Plan item 6 / AC-007 live part: the live fragment carries the pushed filter; route pruning is kept | `LiveScribeExec::handle_child_pushdown_result`, `fragment()`, and `LiveTableRoutes::selected_by` (`live.rs`). The leader keeps its `FilterExec`; dynamic filters are not shipped. | `live::tests::pushed_filter_ships_on_the_fragment_and_stays_on_the_leader`, `event_time_predicate_selects_only_overlapping_routes`, journeys `live_query_routes_only_relevant_scribes` and `peer_network::security::leaf_predicates_round_trip_and_execute`. | PASS |
| Plan item 7: distributed scan metrics count files and partitions, net of `FilePruner` skips | `OracleQueryScanStats`, `RemoteScanMetrics::record_pruned_files`. | `exec::tests::scan_stats_exclude_files_distributed_tasks_pruned`; the filtering journeys. | PASS |
| MCP Analytical refusal leaves clean nodes (the `settle` fix) | `analytical.rs:2621-2625` drops the unfolded metric fold before the release. The cause is recorded with Symptom, Evidence, Cause, and Fix site, and an independent diagnostician confirmed it. | `mcp::agent_runs_three_table_analytical_query_through_mcp` in 53/53. | PASS |
| AC-007 (rev 26): Rust Bifrost tests show DataFusion reads only declared leaves **through Structs, List element access, and `unnest`, on hot, published, and distributed scans** | Wyrd tests cover only top-level Struct and Variant leaves. The `events` List<Struct{attributes: Variant}> fixture in `nested_pushdown.rs:196-345` is never asserted on. `published::prove_pushdown_cut` checks row-group pruning, not decoded leaves. No distributed journey reads a nested leaf. | None for List, `unnest`, or distributed nested leaves. | **FAIL (BEH-2)** |
| REQ-019 ¶2 (unchanged by rev 26): a late failure keeps the catalog code after the first batch, on every SDK | All four surfaces now also accept a pre-stream failure, and the Rust SDK late `parse_json` case is deleted. | The journeys pass with no late terminal observed. | **FAIL (BEH-3)** |
| REQ-019 ¶1 (amended): query-side Variant errors are DataFusion's | `parse_json` failures map to `WYRD_VALA_500_QUERY_EXECUTION_FAILED` in the MCP, Rust, Python, and TS journeys. `bifrost-design.md` is updated. | Journeys. | PASS |
| Analytical classification of the journey fixtures ("Group A") | The fixtures were reshaped to two objects (`analytical_lifecycle.rs:105-110`, `analytical_activation.rs`, `capacity.rs`) on the recorded cause "one published object is one task". | The diagnosis contradicts upstream `file_scan_config_desired_task_count` under the harness's `bytes_per_partition = 1`. | **FAIL (BEH-4)** |
| `published_workers_and_live_scribes_share_one_plan`: the lease assertion moves from exactly `[1,2]` to non-empty | `distributed.rs:1336-1340`. No spec or design text requires a single destination. | Recorded reason (plan 651-654). | PASS |
| Removal of the `bifrost_oracle_file_pruning_total` assertion | No spec obligation covers it; the ranged-read exclusion is still asserted. | — | PASS (the doc residue is in BEH-8) |
| Rev 26 material constraint: **no `parquet-variant*` dependency in `vala-bifrost-redux`** | `crates/vala/vala-bifrost-redux/Cargo.toml:60-61` keeps `parquet-variant` and `parquet-variant-compute`. | — | **FAIL (BEH-6)** |
| Rev 26: every DataFusion crate at one fork revision, including `datafusion-functions-variant` | Cargo.lock: every datafusion-* crate at `a4ff1455`, `datafusion-distributed` at `0b81b12`, arrow-rs at `0d0eb247`. | — | PASS |
| Public contracts regenerate cleanly (AGENTS §12) | `wyrd.v1.proto` dropped messages, but the committed `wyrd.v1.bin` was not regenerated. Eight `strings` hits for deleted names remain at the candidate. | `check:proto-drift` was not run, and it would fail. | **FAIL (BEH-5)** |
| `mise run check` / gate: the workspace-hack matches the feature union | The candidate's arrow-rs git pins and the new `object_store` cloud features are not reflected in the workspace-hack. | `cargo hakari generate --diff` exits 1 on the candidate. | **FAIL (BEH-5)** |
| AC-010: format, lints, `codegen:check`, `test:bifrost`, `docs:check`, `check:docs`, `check:examples`, and the boundary checks pass | The diff touches `wyrd-spec` wire types, the `wyrd-tonic` proto, `docs/…/schema.svx`, and the Python and TS tests. | Only fmt, lints, and `test:bifrost` ran (navigation map). | **FAIL (BEH-7)** |
| AC-010: fork tests with and without `variant` | `verify6.sh` reports `ALL-OK`; each phase's per-crate runs are recorded. | Recorded output (plan 686-697). | PASS |
| Rev 26 latency table: non-Variant, Struct, and unshredded Variant unchanged; shredded much faster | Fork LTO base→on is within noise. The `parquet_variant_query` key reads are 3–20x faster, and whole-Variant reads stay under 2x. | Plan Step 5 evidence. The Wyrd `bifrost_reads` bench compares native with Wyrd, not before with after. The hot `select_wide` "predates this task" claim cites numbers recorded inside TASK-003, not at base; this is not material because the table's "before" is defined on the fork. | PASS |
| Documentation of touched surfaces matches behavior (AGENTS §16) | `bifrost-design.md:1221-1223` documents a deleted metric. `exec.rs:1007` (new rustdoc) says Wyrd adds a "Variant expression adapter". The plan's status table contradicts its own evidence. | — | **FAIL (BEH-8)** |
| New production config `bifrost.oracle.file_scan_bytes_per_task` | `config.rs:486-491` and validation at `:3128`. Every journey forces `1` (`wyrd-testing/src/bifrost/cluster.rs:1829`). | The default (`None`) is never exercised on the Analytical path, and the key is undocumented. | **FAIL (BEH-9)** |
| AC-008: sensitive data stays gated | The logical column gate (`oracle/mod.rs:3498-3522`) does not depend on the deleted Variant code. | Existing journeys in `test:bifrost`. | PASS |
| No test, timeout, sleep, retry, `#[ignore]`, `#[allow]`, or skip was added to clear a failure | None added. The assertion changes are BEH-3 and BEH-4. | Diff audit. | PASS except BEH-3 and BEH-4 |

## Findings

### BEH-1: MISSING. Peer contract fingerprint does not bind the DataFusion fork revision

- **Obligation:** spec revision 26, "Oracle registration and distributed wire": "`ORACLE_VARIANT_SQL_VERSION` is deleted; the peer contract fingerprint includes the DataFusion fork revision, and a worker on a different revision refuses before decoding a plan."
- **Location:**
  - `crates/vala/vala-bifrost-redux/src/oracle/peer.rs:316-320` (`stage_body_digest`)
  - `crates/vala/vala-bifrost-redux/src/oracle/codec.rs:27-33` (`physical_plan_fingerprint`)
  - `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:46` (`PEER_PROTOCOL_VERSION = 5`, unchanged)
- **Evidence:** the diff removes `digest.update(ORACLE_VARIANT_SQL_VERSION…)` from both digests and adds nothing in its place. A grep of `oracle/**` and `wyrd-server/src` finds no fork revision or DataFusion version constant. The peer wire also changed: `FollowerScanAssignment.persisted`, `.predicates`, and `RemoteScanPayload.assignment_json` were removed. Even so, neither protocol version was bumped.
- **Consequence:** a worker or Scribe follower built on a different DataFusion, Variant, or proto revision decodes and runs the stage. A function, adapter, or proto change between revisions then yields wrong results or opaque decode failures instead of a pre-decode refusal.
- **Correction:**
  - Bind a build-time DataFusion fork revision identity into the peer contract digest(s) that are checked before decode.
  - Bump the protocol version for the changed peer wire.
  - Add a unit test showing that a stage minted under a different revision is refused before `datafusion-proto` decode.

### BEH-2: MISSING. AC-007 has no Wyrd evidence for List element access, `unnest`, or distributed nested leaf reads

- **Obligation:** AC-007 as amended by revision 26: "Rust Bifrost tests on both read paths show that Struct uses `get_field` and Variant uses `variant_get`, and that DataFusion reads only the declared leaves through Structs, List element access, and `unnest`, on hot, published, and distributed scans. The distributed case uses no Wyrd codec."
- **Location:**
  - `crates/vala/vala-bifrost-redux/src/oracle/nested_pushdown.rs`: the `events` fixture at 196-345 is unused by any assertion.
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1338-1420` (`prove_pushdown_cut`)
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs`, which has no nested-leaf case.
- **Evidence:**
  - Every Wyrd leaf assertion is on top-level `s['a']` or `v ->> k`.
  - The published and promoted journeys assert files and row groups pruned, not decoded leaves or bytes.
  - No Wyrd test issues `events[i]['attributes'] ->> k`, `unnest(events)…`, or a distributed query on a Struct or Variant leaf.
  - The fork's DataFusion tests prove each generic piece. Rev 26 assigns that to "DataFusion's own tests", in addition to the Wyrd obligation, not instead of it.
- **Consequence:**
  - Span `events` and `links` attribute reads, which are a primary OTel use, have no proof that Oracle's scan, with the tenant reader factory, `FilterPushdown`, and the worker plan rewrite, keeps the List and `unnest` leaf narrowing.
  - Nothing proves that a decoded worker scan keeps it either.
- **Correction:**
  - Add hot and published assertions of decoded leaves or bytes for `events[1]['attributes'] ->> 'k'` and `unnest(events)['attributes'] ->> 'k'`, reusing the existing `events` fixture or the filtering journey.
  - Add one Analytical journey whose distributed `DataSourceExec` reads a Struct or Variant leaf, asserting `oracle_query_bytes_scanned_total` against a whole-column read.

### BEH-3: MISSING. The late-failure path (REQ-019 ¶2) is no longer proven on any SDK surface

- **Obligation:** REQ-019 ¶2, unchanged by revision 26: a failure keeps the same catalog code and details whether it occurs before or after the first result batch, for every SDK. Also the rule in AGENTS §11 that journeys cover negative flows.
- **Location:**
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:989` (`if let Some(terminal)`, which previously had to exist with `row_count > 0`)
  - `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2868`; the late `parse_json` case is deleted.
  - `sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py:592` (`delivered in (0, 8192)`)
  - `sdks/wyrd-sdk-ts/wyrd/tests/integration/oracle-query.test.ts:596`
- **Evidence:** the cause is recorded in `TASK-003-revision-query-latency.md` at about line 819. Round-robin repartition under `ProjectionExec` lets the failing batch overtake the first. That is a fixture-ordering cause, but the fix loosened the assertion. No diagnostician report is cited.
- **Consequence:** all four surfaces pass when only the pre-stream failure occurs. The late-terminal encoding of the catalog error in Rust, Python, TS, and Oracle has no journey proof. A regression that drops the code after the first batch would pass every lane.
- **Correction:**
  - Make the fixture deterministic. Options are a single-partition read or an order-preserving plan, for example a failing expression evaluated after an ordered `SortPreservingMerge`, or `target_partitions = 1` for that table.
  - Restore `terminal.row_count > 0` and the exact-problem assertion on all four surfaces, and restore the Rust SDK late `parse_json` case.

### BEH-4: INCORRECT. The "Group A" diagnosis does not explain the trace, and the fixtures were reshaped on it

- **Obligation:** standing constraint, plan 886 ("Diagnoses use Symptom / Evidence / Cause / Fix site … Do not change code to make an old test pass"). A diagnosis must explain the trace, and the fix must sit at the cause.
- **Location:**
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_lifecycle.rs:105-110` (`FIXTURE_OBJECTS = 2`, documented as "the harness plans one task per published object")
  - the matching reshapes in `analytical_activation.rs` and `capacity.rs`
  - plan lines 625-634
- **Evidence:**
  - The harness sets `file_scan_bytes_per_task = 1` (`wyrd-testing/src/bifrost/cluster.rs:1829`).
  - Upstream `file_scan_config_desired_task_count` (datafusion-distributed `0b81b12`, `src/events/defaults/file_scan_config.rs:13-33`) desires `total_bytes.div_ceil(1).div_ceil(target_partitions)` tasks.
  - So a one-object table of several KB desires many tasks, not one. "One published object is one task" is not upstream's mechanism.
  - No annotated plan and no diagnostician report are recorded.
- **Consequence:**
  - The real reason a one-object scan stays Interactive is unknown. Candidates include the budget, union isolation, `LiveScribeExec`'s `Maximum(1)`, and leaf scale-up.
  - The fixture rustdoc and the plan now state a false rule.
  - Production class selection for small tables is unproven, and BEH-9 compounds this.
- **Correction:**
  - Capture the annotated distributed plan for a one-object and a two-object table under the harness config, and record the actual cause.
  - Then either keep the fixtures with true rustdoc or fix the cause, and update plan lines 625-634.

### BEH-5: REGRESSION. Two repository gates are red at the candidate: proto snapshot drift and workspace-hack drift

- **Obligations:**
  - AGENTS §12: "Public contracts regenerate cleanly when touched", and "A red gate blocks the change".
  - `check:proto-drift` (`scripts/checks/proto-drift.sh`).
  - `check:workspace-hack` (`mise.toml:1410`, part of `check`, part of `gate`).
- **Location:**
  - `crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin` (not in the diff, though `wyrd.v1.proto` is)
  - the hakari workspace-hack manifest
- **Evidence:**
  - `git show 861a0342f:…/wyrd.v1.bin | strings` still has 8 hits for `ScanPredicate`, `HotFileDescriptor`, and `PersistedFileAssignment`, which the `.proto` removed. The reflection descriptor therefore advertises deleted messages.
  - `mise exec -- cargo hakari generate --diff` exits 1 on the candidate: arrow* moved to the `bohmian-ai/arrow-rs` git rev, and there is a new `object_store` 0.14 aws/azure/gcp entry.
- **Consequence:**
  - gRPC reflection and MCP schema generation see stale messages.
  - `mise run check` and `mise run gate` fail.
- **Correction:**
  - Regenerate `wyrd.v1.bin` (`WYRD_PROTO_SNAPSHOT=1` build) and commit it.
  - Run `cargo hakari generate` and `cargo hakari manage-deps`, then commit.
  - Show `check:proto-drift` and `check:workspace-hack` green.

### BEH-6: INCORRECT. Approved constraint "no `parquet-variant*` dependency in `vala-bifrost-redux`" is violated

- **Obligation:** spec revision 26, Material constraints. The plan's "Wyrd switch" also says: "drop `parquet-variant*` from `vala-bifrost-redux`".
- **Location:** `crates/vala/vala-bifrost-redux/Cargo.toml:60-61`.
- **Evidence:**
  - `parquet-variant` and `parquet-variant-compute` remain.
  - Only `-json` was dropped.
  - The plan's progress note justifies keeping them for the write side, but neither the spec nor the plan directive was amended or approved.
- **Consequence:** the shipped dependency graph contradicts the approved spec. Either the constraint or the code is wrong, and the review cannot tell which is authoritative.
- **Correction:** either obtain a human-approved spec amendment that permits the write-side use, or move the write-side Variant code out of `vala-bifrost-redux`, for example to its write owner (`wyrd-queue` or `wyrd-types`), and drop the dependencies.

### BEH-7: MISSING. AC-010 verification lanes were not run for touched surfaces

- **Obligation:** AC-010 ("Format, lints, `codegen:check`, `test:bifrost` …, `docs:check`, `check:docs`, `check:examples`, and the boundary checks for the touched surfaces pass"), and AGENTS §11 verification scope.
- **Location:** the navigation map's "Not run" list, which covers `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `docs:check`, `check:docs`, `check:examples`, and `gate`.
- **Evidence:**
  - The diff changes `wyrd-spec/src/vala/{api,assignment_authority}.rs`, `wyrd-tonic` proto, `docs/src/content/docs/bifrost/schema.svx`, Python and TS integration tests, `wyrd-client` tests, and the root `Cargo.toml` patch set.
  - BEH-5 shows that unrun lanes already hide real failures.
- **Consequence:** there is no evidence of contract, docs, examples, or boundary health.
- **Correction:** run those lanes, or `mise run gate`, since the change crosses many owners and changes the workspace patch set. Record the output.

### BEH-8: INCORRECT. Touched documentation contradicts behavior

- **Obligation:** AGENTS §16 (rustdoc correctness is a hard blocker), and the design doc as active authority.
- **Locations and evidence:**
  - `architecture/bifrost-design.md:1221-1223` still documents `bifrost_oracle_file_pruning_total{outcome}`. No emitter exists, and the plan (618-621) says it is gone.
  - `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:1007` is rustdoc newly written in this diff. It says the hot scan adds "the tenant proof of every footer and the Variant expression adapter". `hot_parquet_scan` installs no adapter, and INV-008 (rev 26) forbids one.
  - Plan status table (plan lines 18-25): step 3 is "in progress", step 4 is "not started", and step 5 says "final LTO run not started". Steps 4 and 5 of the same file record completed evidence.
  - `exec.rs:3466` `projected_leaf_union_preserves_predicate_columns` applies no predicate any more.
- **Consequence:** operators look for a metric that does not exist, and maintainers are told Wyrd owns a Variant adapter. The governing plan misstates task status to the next agent.
- **Correction:**
  - Replace the metric sentence with DataFusion's `files_ranges_pruned_statistics` and scan counters.
  - Fix the rustdoc.
  - Update the status table.
  - Rename the test to what it proves.

### BEH-9: MISSING. The production default of the new Analytical task-size knob is never exercised

- **Obligation:** AGENTS §11 (journeys cover edge flows of shipped capability), and the plan item 2 design, "`None` means the upstream default (production)".
- **Location:**
  - `crates/wyrd/wyrd-server/src/config.rs:486-491`
  - `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:1829` (`get_or_insert(1)` for every cluster)
- **Evidence:**
  - Every Oracle journey runs with 1 byte per task, so no journey plans an Analytical query under the production default.
  - The key is absent from `architecture/bifrost-design.md`, which documents its sibling `max_workers_per_query`.
- **Consequence:** whether production classifies and distributes Bifrost scans at all under upstream's default is unproven. BEH-4 shows that the class rule the fixtures encode is not understood.
- **Correction:**
  - Add one journey, or an integration test with a planning-only session, that leaves the key unset and asserts the class of a realistic multi-object table.
  - Document the key beside `max_workers_per_query`.

## Overall

**FAIL**

- MISSING: BEH-1, BEH-2, BEH-3, BEH-7, BEH-9.
- INCORRECT: BEH-4, BEH-6, BEH-8.
- REGRESSION: BEH-5.

The core rebuild matches the approved design: native `datafusion-distributed`, the worker tenant rewrite, live filter shipping, and the `settle` fix. No deleted Wyrd analytical or Variant code returned, and the fork latency gates hold. Completion fails on the unimplemented fork-revision refusal, AC-007 coverage gaps, a weakened late-failure proof, an unexplained fixture diagnosis, a spec-constraint conflict, and two red repository gates.
