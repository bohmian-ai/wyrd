---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 4
requirements: [REQ-001, REQ-002, REQ-004, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-025, REQ-026, REQ-027]
primary_requirements: [REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-004, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-025, AC-026, AC-027, AC-028]
scenarios: [S01, S07, S08, S09, S10, S11, S12, S13, S14, S15, S16, S17, S18, S19, S21]
merged_from: [TASK-002, TASK-003, TASK-004, TASK-005]
depends_on: [TASK-001]
---

# Close the integration

## Outcome and Value

The integrated candidate runs S as the sole Bifrost implementation, with main surviving only as X1–X9, across every lifecycle owner, ingress path, verification result and public consumer. Every Bifrost-scope difference from S maps to an exception (AC-028).

This task merges former TASK-002 to TASK-005 (planning amendment, 2026-10-12, at the user's request). After spec revisions 3 and 4 the remaining work is one diff against S plus one list of failing lanes, and those failures cross the former task boundaries. The four former tasks are kept below, verbatim, as Parts A–D: every scenario block, acceptance criterion, recipe, stop condition and recorded result keeps its original ID and text. The former criterion IDs (`TASK-002-C01`, `TASK-003-C01`, …) remain the acceptance units. Only the grouping, the dependency chain and the review count change: four sequential reviews become one.

The AC-028 hunk ledger (P-028, formerly TASK-006-C02) moves here, because closing a divergence and mapping it are the same work. TASK-006 keeps the benchmark qualification and the review handoff.

Struck, with no obligation: REQ-005, REQ-006, REQ-007's settlement model, and REQ-024.

Use `$wyrd-implement`. An implementer's self-report does not count as independent task completion.

## Approach

1. Work the queue below, one failing behaviour at a time. Diagnose from the trace before editing. For every failing test (user rule, 2026-10-12):
   1. **Origin.** Establish which worktree the test and the failing code come from: S (`cfe21dc90`), main (`8a8813a74`), both, or this integration.
   2. **Relevance.** Decide whether the test still describes required behaviour under revision 4. A main test of behaviour S replaced, outside X1–X9, is obsolete; a test of behaviour X1–X9 keeps is relevant.
   3. **Fix by origin.** A relevant S test is fixed by restoring S's behaviour. A relevant main test is fixed by adapting main's code or test to S's contract (schema, wire, errors) while keeping its exception's behaviour. An obsolete test is removed only with the reason recorded, and never to turn a lane green. Record origin, relevance and fix in the evidence.
2. For each Part, complete its open scenarios and criteria, and rerun its exact recipes.
3. Build the P-028 ledger as divergences close: every hunk against S maps to an exception or is reverted to S.
4. Record evidence per original criterion ID, then hand the cumulative candidate to one independent task review.

## Work Queue

Known open items on 2026-10-12. A new failure found while working joins this queue with its diagnosis.

| # | Item | Lane / proof | Test origin | Diagnosis and fix | Status | Part |
| --- | --- | --- | --- | --- | --- | --- |
| Q1 | `owner_inspection::failed_wal_replay_is_role_local`: health reports `ok`, expected `scribe_wal_faulted` | exact selector below; `test:bifrost:journey:server` | S and main | Relevant (S makes a failed boot replay role-local, `boot/mod.rs` replay match). Two causes, both S-side. (1) Fixture: it flipped each segment's last byte, i.e. the record of the write whose sync failed. That record was never acknowledged, so S's later unacknowledged-tail rule (`scribe/wal.rs` `end_at_bad_record`, design §Startup replay) truncates it: trace `WAL recovery ended a shard chain at its first bad record, in the unacknowledged tail`. The acknowledged seed rows' segment is already retired, so no record corruption can fail replay. Fixture now corrupts each segment's prologue CRC, which is corruption wherever the segment lies. (2) Production: `WalWriter::new` → `initialize_existing_stream` opened every segment of every epoch, so a corrupt old-epoch prologue aborted boot (`Scribe runtime construction failed: segment header CRC mismatch`) before the role-local replay handler ran. Fix at the owner: counter recovery reads only the current stream directory (`WalWriter::stream_dir`, the one directory `open_segment` writes); older epochs are judged by replay | Closed | A |
| Q2 | Dedicated `GovernedMemoryView` drop/poison assertion (owed since the X8 port) | `resources::tests::oracle_view_dropped_holding_bytes_poisons_the_root` | — | Added. Charges the view behind its reservation's back, drops reservation, pool and query, asserts `Accounting` poison. RED by mutation: removing the poison call in `impl Drop for GovernedMemoryView` fails the test; restored | Closed | A |
| Q3 | Rerun S's Forge and Scribe lanes and the boundary checks after revision 4 | `test:bifrost:integration:redux`, `test:bifrost:journey:scribe`, `test:bifrost:journey:forge`; static lanes | S | Redux integration 901/901. Scribe and Forge each surfaced one failure (Q13, Q14). Static lanes green. After Q13/Q14: Scribe 29/29; Forge 22/23 (Q15, Q16, Q17). After Q16/Q17 (`a21f335ac`): redux integration 900/900, Forge 23/23 | Closed | A |
| Q4 | Record the 2026-10-12 future-size fix: `open_cancellable` borrows a pinned open; `phase`/`prepare`/`wait` are `map` combinators; the 11 `Box::pin` calls in the Verifier engines are removed | S07 (X8 single cancellation token); `mise run lints` (`large_futures`) | main (X8, Verifier) | rust-lang/rust#62958: an `async fn` taking a future by value stores it twice. Debug sizes after the fix: runner `process` 12.6 KB, Eval 10.4 KB, Drift 6.6 KB, `run_with` 4.5 KB. Lints, the affected unit tests and the MCP cancellation journeys pass | Closed | A |
| Q5 | Eval trace journeys fail with `eval_trace_unavailable` | `test:bifrost:journey:server` | S and main | Relevant. `verification/eval.rs` (main, X8 Verifier engine) decoded span and event times with a strict `Int64` downcast; S stores them as `Timestamp(Nanosecond, UTC)`. Fix: one `instant()` decoder over `TimestampNanosecondType` for span start/end and event time | Closed | C |
| Q6 | `direct_task_judgment_records_one_correlated_result` refused with invalid SQL | `test:bifrost:journey:server` | main only | Relevant under X9. The journey read `details`; the server writes the `assertion_result` Variant. A first rewrite used `assertion_result['task_id']`, which DataFusion plans as Struct access (`Field task_id not found in struct`). Shredded Variant paths use `->>`; the journey now reads `assertion_result ->> 'task_id'`, `'passed'`, `'actual'` | Closed | C |
| Q7 | `integrated_enqueue_outage_preserves_ack_and_recovers`: `permission denied for sequence eval_journey_attempts` | `test:bifrost:journey:server` | main only | Relevant. The superuser-created fault trigger ran as the inserting application role, which cannot use the superuser's sequence under X7. Fix: the fixture function is `SECURITY DEFINER` with a pinned `search_path`, so no role is named | Closed | C |
| Q8 | `agent_discovers_mcp_guide_and_sql_examples` refused with invalid SQL | `test:bifrost:journey:mcp` | main only | Relevant. `mcp/guide.rs` compared `start_time_unix_nano` (S: `Timestamp(ns)`) with integer literals: `Cannot infer common argument type ... Timestamp(ns, "+00:00") >= Int64`. Fix: `TIMESTAMP '2026-01-01T00:00:00Z'` / `'2027-01-01T00:00:00Z'` literals | Closed | D |
| Q9 | `agent_investigates_from_multiple_evidence_pivots`: status 400, expected 403 | `test:bifrost:journey:mcp` | main only | Relevant. The journey queried main's `request_payload_json`; S's column is `request_payload` (Variant), so planning refused first (`No field named request_payload_json`). Fix: query `request_payload`; the restricted agent now receives 403 | Closed | D |
| Q10 | `sensitive_variant_leaf_is_denied_before_io`: gateway calls not captured within 30 s | `test:bifrost:journey:mcp` | S only | Relevant. S's journey waited for `wyrd_gateway_capture_total{outcome="delivered"}`; under X2 capture stages on the Scribe outbox and counts `staged` (`components/gateway/capture.rs::record`). `flush_bifrost` settles the outbox before sealing, so the journey waits for both captures `staged` | Closed | B |
| Q11 | P-028 ledger | P-028 below | — | Ledger and the disposition of every flagged item recorded in `evidence.md` § P-028. Fixes it produced: `scribe_wal_newest_fence` operator raise/restore (`82c89ff94`), Gate and Oracle audit credential attribution (`f23eb1716`), restored S assertions (`ab56a47c9`, `3b91313c2`, `6bf7dee46`), `trace_export` timestamp literal (`dff848928`), stale docs (`6f370a205`), X3/X4/X6/X7 design prose (`fcc3aceb1`) | Closed | All |
| Q12 | `an_agent_runs_a_verifier_directly_and_reads_its_result`: summary query returned no rows (passed once earlier) | `test:bifrost:journey:mcp` | S and main | Relevant. Under X2 the judgment is staged on the Scribe outbox when the run completes, so `completed` does not imply the row is in Bifrost; the query raced the outbox writer (trace: the cut pinned zero hot files). Fix: the journey calls `server.flush_bifrost()` after the run settles, as the server Eval journeys do | Closed | C |
| Q13 | `describe::described_canonical_and_dynamic_schemas_reach_exact_physical_schema`: `attributes` carries field metadata | `test:bifrost:journey:scribe` | main only | Relevant. S describes Variant columns with the Arrow Variant extension type (`wyrd_types::spec_to_field`), which travels as field metadata; it is the type, not a field identity. Fix: the assertion admits only extension keys (`wyrd_types::is_extension_key`) | Closed | B |
| Q14 | `live_rewrite::failed_worker_restarts_while_the_api_serves`: rebuilt worker never becomes ready | `test:bifrost:journey:forge` | S and main | Diagnosed (read-only diagnostician). The leader dispatched the journey table and the tenant's `audit_log` rewrite in one pull; the injected lease-release fault ended `drive_event_loop` through `?`, and `join_spawned` settles nothing by S's design ("recovered by the next owner's startup drain"). The `audit_log` attempt stays `running` under owner `…0001`; the rebuilt worker reuses that owner, so `has_recoverable_work` holds until the 15-minute lease lapses, past the 2-minute bound. Forge code on the path is S's unchanged; X2 makes `audit_log` reliably due in the same pull. Decision: keep S's fatal-exit semantics; fix the journey. A first attempt that parked the rewrite at its lease release also stalled the sibling's settlement (settlement runs on the worker loop), so it was dropped. Fix: the journey opts the tenant's `audit_log` out of compaction (`wyrd.forge.enable-compaction=false`, so the leader keeps no track), following the `target_table` precedent; only the journey's rewrite meets the fault. Exact selector passes (13 s) | Closed | A |
| Q15 | `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass`: neighbour rewrite refused `Deadline` | `test:bifrost:journey:forge` | S | Passed in the previous lane run. The neighbour's trigger-count-1 rewrite was dispatched by the boot pass, and the journey advanced the Forge clock one day while it was in flight; the rewrite's publication deadline is taken from that clock (`worker.rs` `RewritePublicationDeadline::new` / `deadline_passed`). Diagnosed (read-only diagnostician): test sequencing. The deadline is deliberately an instant on the Forge clock (`publication.rs` deadline doc); a real clock never jumps a day, and the test's premise that the boot-pass cut is read "before any driven pass can rewrite it" is wrong, because the leader dispatches a due table at once. Fix: drain in-flight tasks (`drain_tasks`) before each clock jump that follows a boot pass, here and in the identical `ReaderCleanupJourney` setup (`advance_maintenance(hours(2))`). Production unchanged. Passes in the next `test:bifrost:journey:forge` run | Closed | A |
| Q16 | `production_closeout::empty_maintenance_restart_protects_orphans`: worker stops on `lost lease fence` for the tenant's `audit_log` | `test:bifrost:journey:forge` | S (X2 makes `audit_log` maintained) | Passed in the previous lane run. After the two-day advance, the successor's maintenance pass ran snapshot expiry while the leader dispatched an `audit_log` rewrite; the rewrite reported Failed and the worker exited fatally on the lost fence. Root cause (read-only diagnostician, then Codex 5-why `q16-codex.md`/`codex-5why.md`): S kept two Forge table authorities with separate lifetimes, the `forge_tasks` active slot and `vala.maintenance_leases`; expiry freed the slot while still holding the lease, so a successor's admitted rewrite lost the lease and was misclassified as a lost fence. Fix X10 (user-approved): the active task claim is the one authority (`ForgeLease` rebound to it; every fenced write opens through `begin_fenced`, which locks the live claim first, `3278de536`). The remaining instance of the same defect was several release points for one claim: the no-op compaction acknowledgement, manifest rewrite, orphan cleanup, and snapshot expiry committed their terminal transitions inside execution while the heartbeat still renewed, and prepared expiry reconciliation did the same. Fix `531ed0d26`: strategies return a decided `ForgeTerminal`; the settlement seam stops and joins the heartbeat, then commits it (a decided terminal commits through shutdown). The `Settled` early return, the self-settled dispatch variant, and the post-settlement heartbeat beat seam are deleted. Coverage: contention refused at claim time (`pg_forge_tasks::insert_claimed_records_one_active_attempt_per_table`, `fair_claim_yields_table_to_concurrent_accepted_attempt`, `active_table_generation_defers_same_table_and_claims_independent_work`); claims cleared at settlement (`snapshot_expiration` asserts `claimed_by IS NULL`, publication tests `live_claims()==0`); readiness before and after claim expiry (`compaction_admission::released_authority_gates_readiness_and_new_claims`); manifest rewrite as a task (`production_routes` timer tests); decided terminal through shutdown (`snapshot_expiration::decided_expiration_commits_through_shutdown`). Docs: `bifrost-design.md` Forge authority, `forge.svx`, `architecture.svx`, `data-plane-internals.svx`, `WyrdBifrostSystem.svelte` (`e01e60b3b`); spec X10 wording `f920318ee` | Closed | A |
| Q17 | `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass`: `compact()` times out after 600 s in the lane, passes alone | `test:bifrost:journey:forge` | S (Q15's drain made it certain) | Diagnosed (read-only diagnostician, `geometry-diagnosis.md`): test only. `enable_compaction` sets trigger-snapshot-count 1, so the boot-pass promotion makes the table due and a worker pull landing in that window rewrites it before `compact()` reads its entry head; the leader then owes nothing and no newer snapshot appears. Fix `a21f335ac`: `compact()` returns once the live head names only Forge outputs; the geometry and reader-cleanup journeys read inputs from the promotion (latest append) snapshot and count retained `Replace` snapshots. Production unchanged. Exact selectors for the three affected journeys pass; Forge lane 23/23 | Closed | A |
| Q18 | `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass`: `multiple inputs at the selected staging target required: [5823188]` (one hot file where two were expected) | `test:bifrost:journey:forge` | S (X2 widens the window) | Diagnosed (read-only diagnostician): production. `PersistenceWorker::publish_claims` in `ClaimSlotWait::Await` (the flush, `publish_residue`) ended its pass as soon as it had nothing left to take and no own claim in flight, unless every claim slot was full. A claim the 1 s tick (`publish_due`) was still driving was neither taken nor awaited, so the flush returned while those members were unpublished, and the round's rows landed in the next hot file. X2's outbox settle in `flush_bifrost` makes a tick-driven claim overlap the flush more often. Fix at the owner: an awaiting publisher waits on `claim_released` whenever another publisher drives a claim, and reports `BudgetExhausted` only when no claim is driven; `ClaimSlotWait::Await`, `publish_claims` and `publish_residue` docs state the contract. Every flush caller routes through `publish_residue`. Exact selector passes; redux integration 901/901, Forge 23/23, Scribe 29/29, MCP 19/19, fmt and lints green | Closed | A |
| Q19 | `components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired`: `audit settles` count mismatch; the test Scribe's writer task panicked `string array` in `recording::decode_audit` | Part B recipe; exact selector | main (X2 test recorder) | Relevant. The `RecordingScribe` audit decoder (test-support, `gateway/capture.rs`) read `detail` with `as_string`; S's `audit_log.detail` is a Variant (`tables/audit/audit_log.rs` `variant_field(DETAIL, true)`), so every acknowledged audit frame panicked and no decision was recorded. Fix: decode `detail` with `wyrd_queue::variant::variant_cell_to_json`, the decoder `verification/results.rs` already uses. Production unchanged. All 50 `components::gateway::` lib tests pass | Closed | B |
| Q20 | `wyrd-cli` `pg_tests::server_dev_*` and Python `test_installed_cli_bootstrap[*]` (rust, python, typescript, interrupt, exit): local startup refused `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE (unsafe_store): <home>/wyrd is accessible to other users` | Part D recipes; exact selectors | main | Relevant. The saved-login store requires the Wyrd config directory to be writable by no one else (`credentials_file.rs` `check_dir`, mask `0o022`). `ServerInstaller::install` created `<config>/server/versions` with `create_dir_all`, so a config directory it created took the process umask; under the common `002` umask it is `0775` and every later login is refused. The fixtures that simulate an installed server did the same. Fix at the owner: `install` creates its directories with a `0o700` `DirBuilder`; the Rust and Python fixtures create the config directory `0o700`. Regression: `server_install::server_install_selects_latest_stable` installs into a missing config directory and asserts it is private (RED by mutation: mode `40775`; GREEN after). Recipes 48–53 pass | Closed | D |
| Q21 | S Python Bifrost capability tests (`test_variant_tables`, `test_every_iceberg_column_type`, `test_register_a_table_from_a_model`, `test_three_timestamp_types`, `test_table_time_column`): 20 failed, 35 errors (`unexpected keyword argument 'server_url'`, `'str' object has no attribute 'serialize'`, `argument 'params': 'ModelMetaclass' object is not an instance of 'Sequence'`) | Part D recipe; `pytest -m integration` on the five files | S | Relevant. The files were ported verbatim, but TASK-001 accepted the Python SDK as `adapted` to main's surface: the ambient `WyrdClient` (no `server_url`/`credential` on `Bifrost`), `TableConfig.from_arrow(table, schema, ...)`, and `sql(query, params, *, model=)` with main's typed bind values. Fix: the tests call that surface; every assertion is unchanged. Dispositions for the five files now read `adapted`. SDK unchanged. 60 passed | Closed | D |
| Q22 | `mise run gate` → `check:ci-selection`: 5 of 53 `test-detect-changes.sh` scenarios fail (`missing rust_packages=wyrd-mcp`, `ci_lanes has test:shared`, `missing rust_client=false`, `missing package_python=false`) | `mise run check:ci-selection` | main | Relevant (the scenarios guard lane selection). Diagnosed (read-only diagnostician): fixture only. Main's `3ba325130` (2026-10-10, `wyrd mcp proxy`) made `wyrd-cli` normally depend on `wyrd-mcp`; every SDK normally depends on `wyrd-cli`, so `select-ci.py` `Workspace.closure` correctly reaches the CLI, the three SDKs and the examples from a `wyrd-mcp` change, and they really link it. The September scenarios still used `wyrd-mcp` as the leaf. No crate is both a leaf and in the journey sets, so the combined claim was split: the journey-lane scenario stays on `wyrd-mcp` and pins its consumers (`rust_packages~wyrd-cli`, `package_python=true`); the leaf scenarios (unrelated families, rename closure, Rust-only, packages nothing) use `crates/wyrd/wyrd`, whose only consumer is the examples crate. Selector and manifests unchanged. 53/53 and 7/7 pass | Closed | — |
| Q23 | `mise run gate` → `test:bifrost:gate`: `oracle mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp` — after the refused query pod 2 still holds 604 bytes of root query memory with `root_query_active: false`, and `await_baseline` times out | exact selector, `-P journey --run-ignored all` | S + X8 | Relevant; the assertion is correct. Diagnosed (read-only diagnostician): upstream `datafusion-distributed` `Worker::coordinator_channel` keeps a stage's `TaskData`, and every reservation in it, until the coordinator request body ends; `GraphLease::settle` cancelled the graph and dropped Wyrd's handle but nothing ended that body. S's `graph_children_idle`/`drain_children` wait hid it; X8 removed the wait. Fix at the owner, no wait and no spawned work: `AnalyticalConnectionLease` carries the graph's own cancellation token and `ReplayBody` ends its stream when it fires, so upstream reclaims through its own end-of-stream drop. Regression `oracle::analytical::tests::settled_graph_ends_its_open_coordinator_body` (RED by mutation: removing the settlement check leaves the body pending; GREEN after). `oracle::analytical*` 35/35; the MCP journey passes | Closed | A |
| Q24 | `mise run gate` → `test:wyrd`: `wyrd-cli::cli mcp_external_server_preserves_global_endpoint` refused the saved login (`<home>/wyrd is accessible to other users`) | exact selector, `-P journey --run-ignored all`, umask `002` | main | Relevant. Same cause as Q20 in a sibling fixture: `mcp_journey.rs` created the config directory with `create_dir_all`, so it took the gate shell's `002` umask (`0775`) and `credentials_file.rs` `check_dir` correctly refused it. Production already creates it `0o700`. Fix: the fixture creates the directory with a `0o700` `DirBuilder`, as `pg_tests.rs` does. Store check unchanged. Passes under umask `002` | Closed | C |
| Q25 | `mise run gate` → `test:shared`, `py:test:integration`, `ts:test:integration`: `wyrd-client::integration pg_bifrost_e2e::pg_tests::canonical_span_payload_reads_back_for_a_query_reader` (cast of `attributes` to Binary), Python `test_observation_surface.py::test_eval_carries_trace_session_and_media` and TS `observation-surface.test.ts > eval observation carries its session, media, and active span` (`context`/`media` compared as JSON text) | exact selectors | main tests over S storage | Relevant. S stores span `attributes` and eval `context`/`media` as Variant, and the SDKs decode Variant cells to native values; main's tests still read them as main's text/bytes columns. Fix: the Rust test decodes the cell with `wyrd_queue::variant::variant_cell_to_json` exactly as S's test does; the Python and TS row models type the columns as object/list and compare decoded values. Storage and SDKs unchanged. 1/1, 33/33 Python (with the touched S Bifrost files), 10/10 TS | Closed | B |
| Q26 | `mise run gate` → `py:typecheck`: 15 `ty` diagnostics in S Python code that main's wider `[tool.ty.src]` scope reaches | `mise run py:typecheck` | S | Relevant. (1) `types.py` imported the native `_wyrd.bifrost` submodule, which no generated stub declares; `timestamp_format` is now registered on the root native module beside `run_wyrd_cli` and declared in `stubs/header.pyi`, so the generated `_wyrd.pyi` covers it. No suppression. (2) `datetime.UTC` does not exist on the supported Python 3.10; tests use `timezone.utc`. (3) Plain `datetime` passed to Wyrd timestamp fields; the tests that prove Pydantic's conversion now call `model_validate`, keeping the conversion under test. (4) `Bifrost.table` is optional; `test_variant_tables.py` narrows it once in `table_of`. `ty` clean; `codegen:check` in sync; timestamp unit 4/4; touched S Bifrost integration files pass (Q25 run) | Closed | F |
| Q27 | `mise run test:bifrost:journey:oracle` (after Q23): `oracle peer_network::transport::peer_transport_uses_immutable_fenced_destinations` — `no live Oracle lease for 00000000-0000-0000-0000-000000000002` right after `PeerCluster::restart(1)` | exact selector, `-P journey --run-ignored all` | S (identical there) | Relevant. Diagnosed (read-only diagnostician; key lines verified): a restarted peer-mode Oracle reserves its fence unready and activates it in a task spawned after the peer plane serves (`server.rs` `activate_peer_roles`), but `Oracle::is_ready` used admission's `membership_available`, which is true when *any* Oracle is live, so beside a live pod it reported ready before its own lease was published, and `/readyz`, `wait_for_ready` and `await_ready` all trusted it. `Scribe::is_ready` had the same gap. Main's `535979176` (harness waits on `is_ready`) and `21d02b174` (pollers survive the drain) did not change the check. Fix at the owner, no wait: both `is_ready` also require the role's own lease at its exact fence in the live snapshot (`live_oracle_at_fence` / `live_scribe_at_fence`). Tests unchanged. The exact test passes; oracle 55/55, server 38/38, scribe 29/29 journeys; `wyrd-server` health/forwarding/state 20/20 | Closed | — |
| Q28 | Gate Cause D: GCS emulator lanes needed two emulators (OpenDAL JSON API vs object_store XML API) | `test:storage:gcs-emu`, `test:storage:e2e`, `test:storage:gcs:cloud` | main (storage stack) and S (object_store) | Relevant. Resolved by TASK-007: Wyrd storage consolidated on object_store, one `tustvold/fake-gcs-server` emulator. GCS XML multipart is unimplemented by every fake-gcs-server, so GCS multipart is proven against real GCS only (user decision); real GCS exposed the upload id persisted only for S3, now keyed on `MultipartV1`. All storage lanes green (see TASK-007 Evidence) | Closed | — |
| Q29 | Gate Cause E: `wyrd-testing::gateway native::anthropic_messages_ingress_is_native_governed_and_fail_closed` panics "the HTTP request span is captured" only when `WYRD_LOG`/`RUST_LOG` are set | exact selector under `test:gateway:native:inner`, with and without `WYRD_LOG` | main (wyrd-telemetry test capture) | Relevant. Diagnosed (read-only diagnostician): `init_test_capture` put one global `resolve_filter` in front of both the fmt and OTel capture layers, and `resolve_filter` lets `WYRD_LOG` replace the test's `config.filter`; the DEBUG `request` span (`wyrd-server/src/http/router.rs` `request_span`) was filtered before capture. Fix at the shared owner: per-layer filters. Logs keep env-wins `resolve_filter`; capture uses `config.filter` with the operator's directives appended, so test-named targets stay captured and `WYRD_LOG` can still add spans. Test unchanged. Exact test passes with and without `WYRD_LOG`; `fmt`, `lints` green | Closed | — |

## P-028 — Integrated AC-028 ledger (moved from TASK-006)

1. **Hunk ledger.** For every file in `git diff --stat cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- crates/vala/vala-bifrost-redux crates/vala/vala-sql crates/wyrd/wyrd-testing/src/bifrost crates/wyrd/wyrd-testing/src/bin/capacity crates/wyrd/wyrd-testing/tests/bifrost crates/wyrd/wyrd-server/src/otlp_decode.rs crates/wyrd/wyrd-server/src/bifrost crates/wyrd/wyrd-server/src/grpc crates/shared/wyrd-client/src/bifrost architecture/bifrost-design.md`, record in `evidence.md` the exception that each hunk maps to, or the non-Bifrost rename that an exception forces. An unmapped hunk is a finding against its owning task.
2. **Struck symbols absent.** The following must print nothing:

   ```bash
   rg -n 'OwnedMembers|owns_any|ForgeLoopQuiescence|PreparedExpiry|Unsubmitted|oracle_infallible_headroom_bytes|LiveConsumers|GRAPH_DRAIN|drain_children|audit_staging|AuditPublisher|AuditUnavailable|prev_hash|chain_hash' crates/vala crates/wyrd/wyrd-server/src crates/wyrd/wyrd-testing architecture/bifrost-design.md
   ```
3. **Migrations.** The diff `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- crates/vala/vala-sql/migrations` contains only in-place X2 edits (no audit staging), X7 edits (main role/RLS conventions and the operator raise/restore in definer functions) and X10 edits (no `vala.maintenance_leases` or lease fence, `manifest_rewrite` as a task strategy). `vala.dev.agent_traces` was never a Postgres object, so X5 has no migration edit. There are no added forward or drop migrations and no main-only Bifrost migration.
4. **Table schema.** Every difference in `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- crates/vala/vala-bifrost-redux/src/tables` maps to X1 (`card_uid` correlation columns), X2 (audit content), X5 (`vala.dev.agent_traces` removed) or X9 (`assertion_result`). The contract test must pass:

   ```bash
   WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
     mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/^tables::tests::/)'
   ```
5. **`agent_traces` wording.** Every hit of `rg -n 'agent_traces' crates architecture docs sdks` states that the table is removed.

## Closeout evidence (TASK-002-CLOSE-01, CLOSE-02, EVIDENCE), 2026-10-12

Candidate: `3def393ad` plus this record. Every command ran through `mise` with `WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1`. Recipes are the Parts' "Exact Selected Recipes", run once each in order (58 commands; P-groups deduplicated across Parts). Each recipe's nextest or runner summary was read: every one selected and ran tests, none passed empty. A recipe that failed was diagnosed, fixed and rerun; its queue item names the fix.

### Recipe results

| Group | Recipe content | Result |
| --- | --- | --- |
| P09 | membership lifetime and audit drain journeys | 2/2 |
| P10 | SQL role fences (3); peer listener, security, network journeys (3) | 3/3, 3/3 |
| P11 | shared outbox routing, retry, refusal | 6/6 |
| P12 | audit projection, observation keys, X9 results contract (4, then 5 with X9); Gate adapter | 4/4, 5/5, 1/1 |
| P13 | retained audit journeys; six-destination capture validator | 2/2, 1/1 |
| P14 | OTLP API key after token lifetime; UID entrances | 1/1, 2/2 |
| P15 | Eval/Task/trace/result layout journeys | 8/8 |
| P16 | verification queue/schedule/lease/judging (server); `wyrd-sql` verifier runs | 4/4, 5/5 |
| P17 | result authoring and schema-by-name | 4/4 |
| P18 | Rust SDK bind forwarding (4, 3, 3); `wyrd-client` bind values; Oracle payload authority; gateway payload objects | all pass; gateway 1/1 after Q19 |
| P19 | Rust query/observation/verification stories | as P18 SDK runs |
| P01 | declared schema and time conformance (5 lib selectors) | 5/5 |
| P03 | deadline projection; max query deadline config | 1/1, 1/1 |
| P20 | Workflow owner accounting | 2/2 |
| P21 | Workflow shutdown and forwarded query settlement | 1/1 |
| P22 | Oracle published Variant reads (3); Scribe recovery journey; recovery run shredding | 3/3, 1/1, 1/1 |
| P23 | Python query (6); TS query (11); S Python capability files (60); TS capability files (64); `wyrd-client` integration list | all pass; Python capability 60/60 after Q21 |
| P24 | boot auth handles (2); DSN (2); signing key file; platform initialization integration (3) | 2/2, 2/2, 1/1, 3/3 |
| P25 | build; installed CLI `server_dev_*` (3) | 3/3 after Q20 |
| P26 | installed CLI from Rust, Python, TS, interrupt, exit | 1, 1, 1, 2 pass after Q20 |
| P29 | mounted MCP query (3); compiled CLI query (2); late HTTP terminal | 3/3, 2/2, 1/1 |
| P30 | Service preflight and served OpenAPI journeys (1, 3) | 1/1, 3/3 |

### Lanes on the final code

After the last production change (`4066e46c3`, Q18) and the later fixes (Q19–Q21):

- `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`: pass.
- `mise run py:format`, `mise run py:lints`: pass.
- `mise run test:bifrost:integration:redux` 901/901; `test:bifrost:journey:forge` 23/23; `test:bifrost:journey:scribe` 29/29 (1 skipped); `test:bifrost:journey:otlp` 16/16; `test:bifrost:journey:oracle` 55/55; `test:bifrost:journey:server` 38/38; `test:bifrost:journey:mcp` 19/19.
- `git diff --check` is clean except `dispositions.tsv`, whose rows with an empty `note` column end in the field-separating tab (445 of 528 rows, the file's format since TASK-001). That is an empty TSV field, not stray whitespace, so the file is left as it is.

### Per-criterion results

| Criterion | Evidence | Result |
| --- | --- | --- |
| TASK-002-C01 (S07, X8) | Addendum above; `resources::tests::oracle_view_dropped_holding_bytes_poisons_the_root` (Q2); Oracle unit run 242/242; `test:bifrost:journey:oracle` passes | PASS |
| TASK-002-C02 (S08) | P09 2/2; P08 cluster drain journey (`server/query.rs`, `stop_node_restarting_another_during_drain`) in `test:bifrost:journey:server` | PASS |
| TASK-002-C03 (S09) | P10 3/3 + 3/3 (listener journey at 6,295,638 bytes); P11 6/6 | PASS |
| TASK-002-C04 (P-S) | `evidence.md` § P-028; struck symbols absent; redux integration 901/901, Forge 23/23, Scribe 29/29 | PASS |
| Part A evidence rows C01–C03 of the original numbering (S05, S06, S07 main settlement) | Superseded by revisions 3 and 4 (Addendum) | STRUCK |
| TASK-003-C01 | P11 6/6; P09 2/2 | PASS |
| TASK-003-C02 | P12 projection; P13 retained audit 2/2; Gate and Oracle credential attribution (`f23eb1716`) | PASS |
| TASK-003-C03 | P13 validator 1/1; P10; P18 gateway payload objects 1/1 (Q19) | PASS |
| TASK-003-C04 | P12 observation keys; P14 UID entrances 2/2 | PASS |
| TASK-003-C05 | P14 API key after token lifetime 1/1 | PASS |
| TASK-003-C06 | `evidence.md` § P-028 | PASS |
| TASK-004-C01 | P15 8/8; P16 `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown`; server lane `integrated_enqueue_outage_preserves_ack_and_recovers` (Q7) | PASS |
| TASK-004-C02 | P16 scheduler, renewal and cancellation selectors (4/4) and `wyrd-sql` verifier runs 5/5 | PASS |
| TASK-004-C03 | P16 `pg_verification_routes::direct_execution_calls_the_judge_provider` | PASS |
| TASK-004-C04 | P12 with X9 contract 5/5; P17 4/4; server lane Eval journeys (Q5, Q6) | PASS |
| TASK-004-C05 | P15 8/8; P17 4/4 | PASS |
| TASK-004-C06 | `evidence.md` § P-028 | PASS |
| TASK-005-C01 | P01 5/5; P23 S capability files Python 60/60 (Q21), TS 64/64 | PASS |
| TASK-005-C02 | P18 Oracle and gateway payload protections; server lane; `agent_traces` wording check (P-028 item 5) | PASS |
| TASK-005-C03 | P18/P19 Rust SDK and `wyrd-client` bind values; P23 Python 6/6, TS 11/11; P03 deadlines | PASS |
| TASK-005-C04 | P29 3/3, 2/2, 1/1 | PASS |
| TASK-005-C05 | P20 2/2; P21 1/1; P30 1/1 + 3/3 | PASS |
| TASK-005-C06 | P25 3/3 and P26 all pass after Q20 | PASS |
| TASK-005-C07 | `codegen:check`; P30 served OpenAPI; P23 runtime surfaces; P24 | PASS |
| TASK-005-C08 | `evidence.md` § P-028 | PASS |
| TASK-002-CLOSE-01 | This table | PASS |
| TASK-002-CLOSE-02 | Work Queue Q1–Q22 closed with diagnoses; server and MCP lanes pass in full (below) | PASS |
| TASK-002-CLOSE-03 | `evidence.md` § P-028 (Q11) | PASS |
| TASK-002-EVIDENCE | This section; `dispositions.tsv` names TASK-002 for this write set (`fcc3aceb1`, Q21 rows) | PASS |
| TASK-002-REVIEW | Not run; the user decides | OPEN |

## Acceptance Criteria

- **TASK-002-CLOSE-01.** Every criterion in Parts A–D passes under its original ID, with recorded evidence. A criterion superseded by spec revision 3 or 4 is recorded as struck, not passed.
- **TASK-002-CLOSE-02.** Every Work Queue item is closed with a diagnosis and a passing proof, including `mise run test:bifrost:journey:server` and `mise run test:bifrost:journey:mcp` passing in full.
- **TASK-002-CLOSE-03.** P-028: every Bifrost-scope hunk against S is mapped in `evidence.md`, the struck symbols are absent, and the migration and table-schema diffs contain only exception changes (formerly TASK-006-C02).
- **TASK-002-EVIDENCE.** Evidence is recorded per criterion ID with exact commands and results. `dispositions.tsv` rows for this write set name TASK-002.
- **TASK-002-REVIEW.** One fresh independent task review accepts the cumulative candidate. A missing proof or an unmapped divergence from S is not a PASS.

## Verification and Evidence

Every command uses `WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1`. Run each Part's exact recipes and the P-028 commands.

Static lanes, once over the whole write set:
- `mise run fmt:check`
- `mise run lints`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `mise run codegen:check`
- `git diff --check`

Do not run a broad aggregate. The final gate, geometry and parity runs belong to the change review.

## Material Stop Conditions

Every Part's stop conditions still apply. In addition, stop and return to the spec owner when closing a queue item would require a behaviour that is neither S's nor one of X1–X9.

## Authority Links

- [Spec revision 4](../spec.md)
- [Decomposition](../decomposition.md)
- [Shared implementation contract](../implementation-contract.md)
- [Proof recipes](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Repository standards](../../../../AGENTS.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)

## Part A — S lifecycle owners with X3, X4 and X8 (former TASK-002)

### Outcome and Value

S's staging, persistence, Forge and Oracle owners are restored unchanged. Main behaviour survives only as X3 (Scribe routing independent of Oracle), X4 (role heartbeat and one membership poller) and X8 (no-wait graph release with one cancellation signal), each adapted to S's owners.

Primary closure: REQ-007 / AC-007 (its X8 remainder), REQ-008 / AC-008 (X4), REQ-009 / AC-009 (X3). Supporting: REQ-001, REQ-002, REQ-004, REQ-010, REQ-023, REQ-027, and this task's share of AC-028. All of INV-001 to INV-012 apply.

Struck by spec revision 3, with no obligation here: REQ-005 (flush ownership repair), all of REQ-006 (Forge lease, term, quiescence, expiry, publication retry and their SQL), and REQ-007's main settlement model. S's own tests prove S's behaviour for those owners.

Use `$wyrd-implement`. An implementer's self-report does not count as independent task completion.

### Owners, Scope, Consumers, and Prohibited Changes

**Owners:** S's `scribe/{assembly,staging_runtime,persistence}.rs`, `forge/*`, `oracle/{admission,analytical,analytical_supervisor,mod,query_stream}.rs` and `resources.rs`; main's `cluster` tasks, the process membership poller, and Scribe peer routing.

**Consumers:**
- server role composition and boot;
- `wyrd-testing` oracle, scribe, forge and server journeys;
- `architecture/bifrost-design.md`.

**Prohibited:**
- Any main flush-ownership, Forge lease/term/quiescence/expiry/publication-retry or Oracle-headroom mechanism, or a test of one.
- Spawned graph-release work, a second cancellation signal, or a nested-child drain.
- A second peer protocol or a per-role poller.
- Changes to non-Bifrost code beyond renames that an exception forces.
- SQL changes, except S's in-place migration files (INV-009).

**Dependency:** depends on accepted TASK-001. TASK-003 reruns S08's drain proof after it changes audit projection.

#### Source-backed reuse

| Capability | Owner/symbol | Selected extension | Proof |
| --- | --- | --- | --- |
| REQ-005/006 struck | S `StagingAssembler`, `PersistenceWorker`, `forge::*` and their SQL | Restore S unchanged; delete main's ports and tests | S's lib, integration and journey lanes (Proof Strategy P-S) |
| REQ-007 / X8 | S `resources.rs` shared root and `GovernedMemoryView`; S analytical graph and supervisor; main c63e2ff46 | Settled or dropped graphs release inline; the late child returns its bytes through the root; the view's drop poisons on leftover bytes; the query cancel token ends both the first-batch wait and the stream | S07 |
| REQ-008 / X4 | main `ClusterTask`, `ClusterRegistry`, `BifrostComposition.membership_poller` | Main's final role tasks and one process poller around S's role engines | S08 |
| REQ-009 / X3 | main `ScribeRoute::select` and `ScribePeers::channel`; S `ScribeFragmentExecutor` | Main routing and transport limits; S's assignment and fencing | S09 |
| REQ-010 / X2 | main `ScribeOutbox` | Already retained by TASK-001; S08 uses the real outbox, not a temporary sink | S08 |

### Requirement Authority

[spec.md](../spec.md) revision 4 is the requirement text: the Revision 3 rule, exceptions X3, X4 and X8, REQ-007 to REQ-010, and AC-028. This task does not copy that text.

### Approach

1. Revert every remaining main hunk in S's staging, persistence, Forge and Oracle owners and their tests, unless it maps to an exception.
2. Keep X8 as ported from c63e2ff46, and add the one missing assertion (S07).
3. Keep X4 and X3 exactly as accepted (S08, S09).
4. Prove AC-028 for this write set: every hunk against S maps to X1–X9.

### Ordered Implementation Scenarios

#### S07 — No-wait graph release and one cancellation signal (X8)

**Behavior.**
- A settled or dropped analytical graph releases inline, without spawned release work and without waiting on nested children.
- A late child returns its bytes through the shared memory root.
- Dropping a query memory view that still holds bytes poisons the root.
- The query's one cancellation token ends both the leader's first-batch wait and an open stream.
- A cancel observed before the first batch is recorded as `cancelled`.

Mapped obligations: REQ-007 / AC-007 (X8).

**RED.** Every clause except the view-drop poison already has an assertion in a verified selector (see Proof). Add `resources::tests::oracle_view_dropped_holding_bytes_poisons_the_root` to the existing `resources::tests` module. It acquires an Oracle query, grows its memory view, drops the view while it still holds bytes, and asserts that `health().reason()` is `Some(BifrostResourcePoisonReason::Accounting)`. Prove RED by mutation: remove the poison call in `impl Drop for GovernedMemoryView`, see the test fail, then restore the call.

**GREEN.**
- Run the new test: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=resources::tests::oracle_view_dropped_holding_bytes_poisons_the_root)'`. If you choose a different name, confirm it with `mise exec -- cargo nextest list --locked -p vala-bifrost-redux --lib --features test-support` and record the command you actually ran.
- Then run the unit run and the Oracle journey below.

**REFACTOR.** None beyond the test. Do not add a release worker, drain loop or second token.

**Proof (verified selectors).**

| Clause | Test |
| --- | --- |
| Inline release; a graph that cannot release is retained until shutdown; shutdown result `(attempts_settled, graphs_retained) == (1, 0)` | `oracle::analytical::tests::follower_graph_release_waits_for_every_holder`, which calls `follower_retains_a_graph_it_cannot_release` |
| Late child returns bytes through the root | `resources::tests::oracle_release_paths_are_exact_and_idempotent` |
| Dropping the transport releases the graph | `analytical_activation::transport_drop_retires_and_releases_the_graph` |
| First-batch wait ends on timeout or ready | `oracle::tests::first_batch_timeout_releases_owner_and_preserves_query_timeout`, `oracle::tests::first_batch_ready_transfers_owner_without_cleanup` |
| One token ends an open stream; a drop records `cancelled` | `oracle::query_stream::tests::request_cancellation_interrupts_production_stream`, `oracle::query_stream::tests::gate_lifecycle_drop_records_cancelled_once` |
| A cancel before the first batch records `cancelled` | `workflow::workflow_forwarded_query_settles_before_the_run_ends` (the `Cancel` cause) |

```bash
WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
  mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(=oracle::analytical::tests::follower_graph_release_waits_for_every_holder) | test(=resources::tests::oracle_release_paths_are_exact_and_idempotent) | test(=oracle::tests::first_batch_timeout_releases_owner_and_preserves_query_timeout) | test(=oracle::tests::first_batch_ready_transfers_owner_without_cleanup) | test(=oracle::query_stream::tests::request_cancellation_interrupts_production_stream) | test(=oracle::query_stream::tests::gate_lifecycle_drop_records_cancelled_once)'

# Focused journeys: transport drop, and Workflow cancel before the first batch.
WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
  mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=analytical_activation::transport_drop_retires_and_releases_the_graph) | test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'

# Unit run of the touched owners (242 tests on 2026-10-11).
WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
  mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(/oracle::/) | test(/resources::tests::/) | test(/tables::tests::/)'

# Oracle journey lane, the whole binary (55 tests).
WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
  mise run test:bifrost:journey:oracle

# Static checks: each must print nothing.
rg -n 'LiveConsumers|GRAPH_DRAIN|drain_children|oracle_infallible_headroom_bytes' crates/vala/vala-bifrost-redux/src
rg -n 'tokio::spawn|spawn_blocking' crates/vala/vala-bifrost-redux/src/oracle/analytical.rs crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs | rg -i release
```

#### S08 — Withdrawn roles keep discovery alive until teardown (X4)

**Behavior.**
- Heartbeat advances after readiness is withdrawn, without readiness becoming true again.
- One membership poller stays current through a delayed remote-Scribe outbox drain.
- Shutdown, abort and failed boot end every owned task.

Mapped obligations: REQ-008 / AC-008.

**RED.** Run P08 and P09. The expected RED is any of: a frozen roster, a missed newly ready Scribe, re-advertised readiness, duplicate pollers, or a heartbeat leaked on teardown.

**GREEN.** Keep main's final `ClusterTask` and the single process poller around S's role engines. Rerun P09 after any outbox change.

**REFACTOR.** No per-role poller. Keep only the lifecycle owners that hold real tasks.

#### S09 — Independent fenced Scribe peers and configured transport (X3)

**Behavior.**
- Scribe-only fragments work under exact role, fence, tenant and body authority.
- Internal writes from the router reach ready Scribes without a local Oracle.
- Lost or silent peers retry within the configured timeout.
- Valid frames above tonic's default size decode under the configured ceiling.

Mapped obligations: REQ-009 / AC-009.

**RED.** Run P10 and P11. The listener journey submits and reads back a frame above tonic's default size. The expected RED is any of: an Oracle dependency, wrong sibling-role checks, a dropped valid frame, a missing deadline, or widened peer authority.

**GREEN.** Keep main's boot-selected route, TLS, discovery and transport limits, adapted to S's assignment codec and Scribe follower authority.

**REFACTOR.** No second peer service or transport. Keep mTLS-only mounts and one frozen endpoint and fence per query attempt.

### Acceptance Criteria

- **TASK-002-C01.** S07: every X8 clause has a passing, verified assertion, including the new view-drop poison test. The static checks print nothing, the unit run passes and the Oracle journey lane passes.
- **TASK-002-C02.** S08: one membership poller and the role heartbeat continue through the required drain; delayed remote-Scribe discovery is forced during the drain; shutdown, abort and failed construction settle all tasks.
- **TASK-002-C03.** S09: Scribe-only roles enforce frozen assignment, fence, tenant and body authority; valid frames above tonic's default but below the configured ceiling get an ACK and read back; silent or stale peers obey bounded retry and eviction.
- **TASK-002-C04.** P-S: each diff hunk against S maps to an exception; the struck symbols are absent; S's Forge and Scribe lib, integration and journey lanes pass.
- **TASK-002-EVIDENCE.** The Implementation Evidence section records exact commands and results. `dispositions.tsv` rows for this write set name TASK-002.
- **TASK-002-REVIEW.** A fresh independent task review accepts the checkpoint. A missing proof or an unmapped divergence from S is not a PASS.

### Write Set

- S's `scribe/{assembly,staging_runtime,persistence}.rs`, `forge/*` and `tests/integration/forge/*`;
- `oracle/{admission,analytical,analytical_supervisor,mod,planner,query_stream,running,telemetry}.rs` and `resources.rs`;
- `cluster`, server role composition and the poller;
- peer routing and listeners;
- `wyrd-testing` oracle and scribe journeys;
- `architecture/bifrost-design.md` prose for X8.

### Verification and Evidence

Run the commands in S07, S08, S09 and P-S, plus the recipes below. Every command uses `WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1`.

Static lanes:
- `mise run fmt:check`
- `mise run lints`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `git diff --check`

Do not run a broad aggregate. Final gate, geometry and parity runs belong to the change review.

#### Non-TDD proof P-S — S restored for struck REQ-005/006, and this task's AC-028 share

This proof is verification-only. It manufactures no RED, because the target is S's existing behaviour.

1. **Diff against S.** Map every hunk of `git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- crates/vala/vala-bifrost-redux/src/forge crates/vala/vala-bifrost-redux/src/scribe/assembly.rs crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs crates/vala/vala-bifrost-redux/src/scribe/persistence.rs crates/vala/vala-bifrost-redux/src/oracle crates/vala/vala-bifrost-redux/src/resources.rs crates/vala/vala-bifrost-redux/tests/integration/forge crates/wyrd/wyrd-testing/tests/bifrost/scribe crates/wyrd/wyrd-testing/tests/bifrost/oracle` to one of X1–X9, or to a non-Bifrost rename that an exception forces. Revert any hunk that maps to none. Mapped on 2026-10-11: `forge/compact.rs` deletes S's audit-staging name-ban test (X2: audit staging no longer exists); the table-scoped persistence faults (`target_table_for_test`) and staging backlog (`backlog_of`, `staging_backlog_for_test(Option<&str>)`) replace S's `without_audit_publication_for_test()` switch in the Scribe journeys (X2: audit decisions stage `audit_log` through Scribe on the same pod, and there is no publisher to turn off); `superuser_pool()` losing `.await` follows the non-Bifrost dev-fixture API.
2. **Struck symbols absent.** The following must print nothing:

   ```bash
   rg -n 'OwnedMembers|owns_any|ForgeLoopQuiescence|PreparedExpiry|Unsubmitted|oracle_shutdown_drains' crates/vala crates/wyrd/wyrd-testing
   ```
3. **S's tests pass:**

   ```bash
   WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
     mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/^forge::/) | test(/^scribe::/)'
   WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 mise run test:bifrost:integration:redux
   WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 mise run test:bifrost:journey:forge
   WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 mise run test:bifrost:journey:scribe
   ```

#### Exact recipes

##### P04 — S group/fence/crash recovery (regression)

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_syncs_once) | test(=scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once) | test(=scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain) | test(=scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed) | test(=scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays)'
```

##### P08 — Role task stop/abort/drop

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=cluster::tests::stop_joins_the_task_once) | test(=cluster::tests::stop_aborts_a_task_past_its_deadline) | test(=cluster::tests::abort_and_drop_end_the_task)'
```

##### P09 — Real membership lifetime and outbox drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

##### P10 — SQL role fences and private peer trust

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete) | test(=peer_network::security::peer_context_refusals) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations)"'
```

##### P11 — Shared outbox no-Oracle routing, bounded retry and refusal

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes) | test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'
```

### Material Stop Conditions

- A required outcome contradicts spec revision 4. Finish unaffected work, cite the exact conflict and request a revision.
- A hunk against S cannot map to X1–X9, but reverting it breaks a non-Bifrost owner. Report the hunk; do not keep it silently.
- The X8 port needs spawned release work, a second token or a drain to pass. Stop: that contradicts X8.

Compiler, lint, setup and test-name failures are ordinary diagnosis. A red check blocks completion. Do not hunt for authorship or check out a baseline.

### Authority Links

- [Spec revision 4](../spec.md)
- [Decomposition](../decomposition.md)
- [Implementation contract](../implementation-contract.md)
- [Proof recipes](../proof-recipes.md)
- [Repository standards](../../../../AGENTS.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)

### Implementation Evidence

Commits: `ba86fe129` (S07), `f10f735dd` + `53dfc2a1c` (S05), `136e460d6` (S06),
`3712b9ba3` (S08), `b7b2de753` (S09), `985aa12f8` (lint at release features).
All tests ran through `mise` with
`WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-002-C01 | `scribe/persistence.rs` flush proof: outside claim on the test table, audit staged at generation 2 under a 2-slot claim budget, fault scoped by `target_table_for_test` | P04 (6 lib selectors + Postgres crash replay) and P05 (3) pass; extended `flush_waits_for_an_outside_claim_over_members_durable_at_drain` asserts armed fault, scoped backlog `(audit 0, table 1)`, flush returns 1 | PASS |
| TASK-002-C02 | `forge/expire.rs` hold = `lease.remaining() - uncertainty_margin`; `expiry_gates.rs` post-preparation gate; `snapshot_expiration.rs` `Unsubmitted` variant plus `operation_id`/`operations` identity on retained and settled state | P06 3 lib + 7 integration + 1 SQL pass; `forge::snapshot_expiration::*` (8) and `forge::reader_expiry_ordering::*` pass; term replacement, revoked handlers, joined-loop reclaim, non-blocking cuts, v3 lineage covered by the P06 selectors | PASS |
| TASK-002-C03 | `resources.rs` shared-root `released` Notify and `oracle_infallible_headroom_bytes`; `oracle/admission.rs` drain over Oracle governed + headroom bytes, poisoned ledger reports `usize::MAX` | `oracle_release_paths_are_exact_and_idempotent`, `oracle_shutdown_drains_only_oracle_memory`, `oracle_shutdown_drains_oracle_infallible_headroom`; 119 owner tests and 187 `oracle::` lib tests pass | PASS |
| TASK-002-C04 | Main ClusterTask/single poller retained (no source per-role poller); `WyrdTestCluster::stop_node_restarting_another_during_drain` boots the Scribe only after drained roles are unready and the drain is still running | P08 (3) pass; P09 `unready_roles_heartbeat_until_teardown_ends_membership_tasks` and extended `oracle_only_pod_retains_audit_staged_before_its_drain` pass | PASS |
| TASK-002-C05 | Capture peer listener decodes at `transport.message_limit_bytes()` (`grpc/mod.rs`); listener journey submits a 6,295,638-byte audit frame as a member, requires ACK, reads back the exact row length through an Oracle | P10 SQL (3) and peer-network (3) pass; P11 outbox (6) pass, including silent-peer retry and terminal refusal | PASS |
| TASK-002-EVIDENCE | This section; `dispositions.tsv` rows for the write set set to TASK-002 | — | PASS |
| TASK-002-REVIEW | Independent review not run here; the user decides | — | OPEN |

#### Per-scenario RED/GREEN

- **S05.** Production behaviour was already ported in TASK-001, so this scenario only verifies it. The extended test was proven by a mutation RED: unscoping `take_claim_retirement` fails it, and the source was restored.
  - **Fixture diagnosis 1:** a backlog of `(1,1)` came from a one-slot claim budget held by the outside claim. Fixed with `start_with(faults, 2)`.
  - **Fixture diagnosis 2:** a "writer-v2 replay identity" mismatch came from both members using shard 0, generation 1. Fixed by staging the audit rows at generation 2.
  - **GREEN:** P04 and P05 commands as listed in the recipe.
- **S06.**
  - **RED:** `forge::snapshot_expiration::unsubmitted_expiry_at_the_lease_bound_is_released` failed with `Sql(InvariantViolation { "maintenance lease fence lost …" })`.
  - **Cause:** the hold timeout equalled `lease_ttl`, so the pre-submission reset always ran after the fence had lapsed.
  - **Fix site:** `run_snapshot_expiry_for_table_inner`. An independent read-only diagnostician confirmed this site.
  - **Source check:** main carries the same bound and the variant has no lease-bounded expiry, so this is a correction inside main's REQ-006 code, not a merge from either branch. `architecture/bifrost-design.md:1216` still holds, because the lease still bounds the hold.
  - **GREEN:** the same selector plus the accepted, rejected and retryable variants, then the full P06 set.
- **S07.**
  - **RED:** compile failure on the missing `oracle_infallible_headroom_bytes` field, then three behaviour failures:
    - late child: `Some(Accounting)` vs `None`;
    - headroom: 67112960 vs 4096;
    - drain reporting Forge bytes: 1052672 vs 4096.
  - **GREEN:** the same selectors plus the owner lanes.
  - **Removed:** source `LiveConsumers`, the graph child drain and its wall deadline, the supervisor child idle/debt refusal, and `a_registered_zero_byte_child_keeps_its_query_busy`. That test's property is now unreachable under main's settlement model.
  - **Replaced:** five source analytical tests, superseded by main's versions.
- **S08.** The P08 and P09 baselines passed before and after the change.
  - **Change:** the journey now boots the Scribe strictly after observing readiness withdrawn while the drain is still running. Before, the Scribe booted before the drain started.
  - **Result:** GREEN on the first run. Main's poller already keeps discovery alive, so the preserved behaviour is now proven.
- **S09.**
  - **RED:** a mutation removing `.max_decoding_message_size(...)` from the capture peer service failed the listener journey with `decoded message length too large: found 6295638 bytes, the limit is: 4194304 bytes`. The source was restored.
  - **GREEN:** the listener journey and P10/P11.

#### Static lanes

`mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation` and `git diff --check` all pass.

`lints` first failed twice, and both were fixed in `985aa12f8`:
- **Gate fields:** clippy's `struct_field_names` lint fired on the expiry gate fields; they were renamed.
- **Graph release:** `unused_async` fired on the analytical graph release in the release-feature binary pass. The release is now synchronous, and the test pause moved into its async callers, as on main.

#### Write set and exclusions

The write set is listed in `dispositions.tsv`. Two of its files are main-only and therefore have no B→S row:
- `forge/expiry_gates.rs` (test-support gate);
- `tests/bifrost/oracle/peer_network/listener.rs`.

Exclusions held:
- No SQL schema, public contract, generated artifact, dependency or configuration change.
- No compatibility shim.
- No `#[allow]`.
- No reader barrier restored.

Deviation for user decision: the S06 production correction above.

#### Addendum 2026-10-11 — spec revisions 3 and 4

Spec revision 3 struck REQ-005, all of REQ-006 and REQ-007's settlement model, and reverted their commits. The rows above for C01 (S05), C02 (S06) and C03 (S07, main settlement) are therefore superseded: `ba86fe129`, `f10f735dd`, `53dfc2a1c`, `136e460d6` and the Forge part of `985aa12f8`. Those rows record what was done, not current acceptance. The S06 "deviation for user decision" is moot, because the code it corrected is gone. The rows for S08 (`3712b9ba3`) and S09 (`b7b2de753`) stand.

Revision 4 ports main c63e2ff46 as X8. Files changed:
- `oracle/admission.rs`, `oracle/analytical.rs`, `oracle/analytical_supervisor.rs`, `oracle/mod.rs`, `oracle/query_stream.rs` and `resources.rs`;
- `architecture/bifrost-design.md`;
- the journeys `oracle/mcp.rs`, `oracle/workflow.rs` and `oracle/analytical_activation.rs`.

Current results:
- **Unit run:** `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/oracle::/) | test(/resources::tests::/) | test(/tables::tests::/)'` → 242/242 pass.
- **Oracle journey:** `mise run test:bifrost:journey:oracle` → 55/55 pass.
- **Static lanes:** Clippy and fmt are clean.

Diagnoses:
- **Retained-graph shutdown.** The shutdown assertion in `follower_retains_a_graph_it_cannot_release` is now `(attempts_settled, graphs_retained) == (1, 0)`. c63 deleted the stuck-child release refusal, so shutdown cancels the stray attempt and then releases the graph.
- **First-batch cancel telemetry.** A cancel before the first batch is marked through the existing telemetry cancellation marker. Main's `open_cancellable` cancels through the registry before a stream exists. The `Cancel` cause of `workflow::workflow_forwarded_query_settles_before_the_run_ends` asserts the `cancelled` outcome.
- **Activation journey rename.** The journey is now `transport_drop_retires_and_releases_the_graph`. The inline Drop release leaves no pause window to observe.
- **Workflow journey pause.** The test pause moved into `settle_distributed_failure`.
- **Deleted helpers.** `LiveConsumers` and the `nested_*` helpers are deleted. Dropping `GovernedMemoryView` is the leak check. A dedicated assertion for that drop is still owed under S07.

X9: `assertion_result` was added to the results contract test `tables::tests::verification_tables_match_their_approved_schemas`.

P-S mapping closed on 2026-10-11: every hunk against S in `forge/compact.rs`, `scribe/assembly.rs`, `scribe/persistence.rs` and `scribe/staging_runtime.rs` maps to X2 or to the dev-fixture API (see P-S step 1). `target_table_for_test` is the X2 replacement for S's `without_audit_publication_for_test()`.

## Part B — Audit and trusted ingress (former TASK-003)

### Outcome and Value

Authorization audit, gateway captures and Verifier result ingress use one non-blocking process outbox and the trusted six-destination peer path, with exact credentials, UID scopes and source time/Variant rows.

Primary closure ownership: REQ-010 / AC-010, REQ-011 / AC-011, REQ-012 / AC-012, REQ-013 / AC-013. Supporting clause contributions: REQ-004, REQ-008, REQ-009, REQ-017, REQ-022, REQ-023, REQ-027, and this write set's share of AC-028. Main behavior enters only as X1 (card_uid correlation and unbound-writer attribution), X2 (one shared outbox, audit content, six trusted destinations), X4 and X7 (main identity at the Bifrost entrance), adapted to S's owners. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md) and [approved spec revision 4](../spec.md); revision 4's Revision 3 rule overrides any conflicting sentence in the contract or evidence files. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 3 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

### Owners, Scope, Consumers, and Prohibited Changes

Own the existing shared outbox projection/batching/retry/loss contract, actual Gate/Oracle credential builders, private destination validation and whole-frame UID admission. Preserve main verified auth/API-key-before-decode boundaries. Source and main tests expressing partial sibling acceptance for forbidden correlation must be adapted to the approved complete-frame refusal; inherited behavior is not permission to weaken REQ-013. Prohibited (X2): no audit publisher, audit staging table or migration, relay, audit WAL, seq/hash chain, public internal scope or general API-key route may exist in code, SQL, tests or docs.

Depends on accepted TASK-002 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

#### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-004 / AC-004 — Scribe ACK/recovery | S shard group commit; NewestFence/FenceLookup; ScribePersistence recovery | P04/P22; SQL newest acknowledged fence | None: S owns it | S WAL/replay/SQL unchanged | Existing shard/recovery owners suffice | S04/S20 |
| REQ-008 / AC-008 — Membership lifetime | M ClusterTask; ClusterRegistry; BifrostComposition.membership_poller | P08/P09; Oracle/Scribe teardown and boot | S process token stops discovery before drain | Retain M final role tasks + one process poller | Existing concrete owners retain task lifetime | S08 |
| REQ-009 / AC-009 — Independent peers | M ScribeRoute::select/ScribePeers::channel; S ScribeFragmentExecutor | P10/P11; private grpc mounts and ingress | S capture requires local Oracle for remote Scribe TLS | Keep M routing + S assignment/fencing and limits | No additional peer service/transport | S09 |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S audit delivery differs from X2 | X2: M one process outbox; S row/frame projection. Prohibited: any publisher or staging path | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-011 / AC-011 — Audit schema/attribution | M project_audit_event; GateAudit blanket impl; Oracle query_audit_event; S AuditLogTable types | P12/P13; actual keyed Gate/Oracle -> retained history | M builders omit credential id | X2: M event content + S created_at/Variant; carry credential. Prohibited: chain columns or staging | Existing event/sink/projection; no chain/hash authority | S11 |
| REQ-012 / AC-012 — Trusted internal writes | M ScribeCapturePeerGrpc::batch; ScribeIngressFrame attribution | P13; six destinations and public Gate refusals | S older two-destination path loses result/audit attribution | Retain six-destination mTLS and exact trusted UID scope | Existing private route; no minting publisher | S10/S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

### Requirement Authority

[spec.md](../spec.md) revision 4 is the requirement text (Revision 3 rule, X1, X2, X4, X7, REQ-010–REQ-013, REQ-017 matrix, AC-028); it is not copied here.

### Approach

1. Preserve the main one-process outbox and audited permission boundary while updating source Arrow frame/time/Variant projections.
2. Carry actual credential_id and explicit decision created_at through Gate/Oracle builders and canonical retained audit content without seq/hash chaining.
3. Retain private mTLS audit/two-capture/three-result destination rules and exact trusted result UID attribution; prove public-write refusals.
4. Integrate signed bound scopes and unbound at-most-32 tenant-registry UID checks across native/OTLP/decode/ACK/private frames/client boundaries, refusing the entire frame on forbidden correlation.
5. Extend actual keyed allowed/denied audit, six-destination ACK/readback, trace/log/metric API-key and revoked-key journeys; rerun role-discovery drain and durable retry proofs.

### Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require Behavior/RED/GREEN/REFACTOR; unchanged S behavior uses verification-only proof.

#### S10 — One outbox delivers bounded identical private writes

**Behavior.** Audit/capture/results share one process writer, bounded destination frames and stable retry IDs. Terminal refusal consumes/counts its slice and later work progresses; private destinations/Verifier/system-owner rules and all public result-write refusals remain enforced. Mapped obligations: REQ-010 / AC-010, REQ-012 / AC-012.

**RED.** Run P11 and private validator in P13. Extend existing private/public journey owner to observe ACK/readback of all destination categories under source schema. Expected RED is lost category, changed retry frame, forbidden public write, or permissive internal attribution.

**GREEN.** Use main shared outbox and six-destination mTLS ingress with source frame/projection. Rerun S04/S09 exact retry and routing cases.

**REFACTOR.** Keep existing sink and permission surfaces; no publisher or staging owner may exist (X2).

#### S11 — Audit records real credentials and never changes authorization

**Behavior.** Allowed/denied keyed Gate and Oracle requests retain credential or null exactly, decision-created_at and logical Variant detail, with no sequence/hash columns; delivery failure never changes the request decision. Trusted system rejection records once. Mapped obligations: REQ-011 / AC-011, REQ-010 / AC-010, REQ-012 / AC-012.

**RED.** Run P12/P13. Extend existing retention journey to originate actual credential-bearing ingress/query decisions rather than only stage fabricated events, and extend production blanket adapter proof. Expected RED includes omitted credential_id, text/time mismatch, any chain column, or audit IO affecting authorization.

**GREEN.** Keep main event/stage/outbox authority, source explicit time/detail representation, and carry credential through actual builders. Rerun S08/S10 retention and private destination proof.

**REFACTOR.** No hash, chain or staging helper and no AuditUnavailable case may exist; do not change canonical CardRef content or batch dedup hashing.

#### S12 — UID and API-key scope are preserved at native and OTLP entrances

**Behavior.** Bound signed scope and unbound tenant registry admit valid optional subject UIDs; missing correlation stays null; forbidden/malformed/wrong-kind/foreign UID rejects the entire frame. Stock API-key OTLP authenticates before decode and works across token TTL without becoming a global API-key route. Mapped obligations: REQ-013 / AC-013, REQ-022 / AC-022.

**RED.** Run P14 and native scope selectors in P12; extend the API-key journey to trace/log/metric entrances and revoked/unknown key refusals where those branches changed. Expected RED is CardRef-only behavior, unbound denial/allow-all, attribution drift, or missing verified Gate entrance.

**GREEN.** Port main authorization/registry/verified entrance into source admit/conform/ACK and SDK/OTLP input. Rerun S01/S10/S11 for scope and retained attribution.

**REFACTOR.** Keep authentication and registry IO at the trusted boundary; no lookup in CPU decode or global CardRef rename.

### Acceptance Criteria

- **TASK-003-C01.** One bounded destination-grouped outbox retries identical content-derived batch IDs, progresses after terminal refusals and counts observable shutdown loss; audit delivery never changes authorization.
- **TASK-003-C02.** Real keyed/null Gate and Oracle decisions retain all 12 content fields, decision time and logical Variant detail through production adapters.
- **TASK-003-C03.** All six trusted destination categories are ACKed/read back with their exact system-owner/Verifier scopes; public gateway/result writes always refuse.
- **TASK-003-C04.** Native and OTLP valid optional UIDs are admitted/null as specified; malformed/missing/foreign/wrong-kind/forbidden IDs reject the whole frame before persistence, with no valid sibling prefix accepted.
- **TASK-003-C05.** API-key trace/log/metric requests authenticate before decode, work across token TTL and refuse unknown/revoked keys; other routes never gain API-key authentication.
- **TASK-003-C06.** AC-028 share: every hunk of this write set's Bifrost files against S (`git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- <Bifrost write-set paths>`) maps to X1, X2, X4 or X7; `rg -n 'audit_staging|AuditPublisher|AuditUnavailable|prev_hash|chain_hash' crates/vala crates/wyrd/wyrd-server/src crates/wyrd/wyrd-testing architecture/bifrost-design.md` prints nothing.
- **TASK-003-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-003-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

### Expected Write Set and Consumer Closure

ScribeOutbox/sink/batch and AuditStage; Gate/Oracle decision builders; audit table/projection; capture_peer/private grpc; verified auth and CardRegistry boundary; ingest conform/decode/frame and ACK correlation; immediately affected SDK encodings, real audit/OTLP/native/private destination tests, schemas and docs.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

### Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P09, P10, P11, P12, P13, P14, P19. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

#### Non-TDD proof

Relevant I18/I24/I26/I27 contributions (removal/history/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

#### Exact Selected Recipes

##### P09 — Real membership lifetime and audit drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

##### P10 — SQL role fences and private peer trust

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete) | test(=peer_network::security::peer_context_refusals) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations)"'
```

##### P11 — Shared outbox no-Oracle routing, bounded retry and refusal

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes) | test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'
```

##### P12 — Native UID/ACK/audit projection and actual Gate adapter

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_carry_subject_and_created_at) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

##### P13 — Real retained audit and six-destination private validator

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_retention::retained_history_carries_both_credential_shapes) | test(=audit_retention::system_owner_security_rejections_retain_once)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'
```

##### P14 — OTLP API-key and UID runtime entrances

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

##### P19 — Focused first-class Rust query/observation/verification stories

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

### Material Stop Conditions

- A required product/wire/persisted/security/lifecycle outcome contradicts
  approved revision 4 or needs a different decision. Finish unaffected work,
  cite the exact conflict and request a semantic revision; do not guess.
- Supplied compaction commit `22cd178d14009468bc951a14d7acf63237af808a`
  becomes unreachable or fails source Arrow60 compatibility or main no-REST/Ring
  narrowing in the combined graph. Report the concrete failure; do not replace
  the pin silently, waive checks, or publish externally without authorization.
- Work would reset/migrate retained old Bifrost objects or damage main control
  plane. Fresh-only scope does not authorize data deletion.
- A proposed fix requires a new public client/service, dependency family,
  cross-tenant/raw SQL boundary, scope widening, or optional feature not earned
  by the approved analytical graph/foreign-runtime boundary.
- User/main/source input pins change materially; record the delta and resolve
  its authority before extending this packet's scope.

Compiler, lint, setup, current test-name, private plumbing or sanctioned fixture
failures are ordinary implementation diagnosis, not material stop conditions.
A required red check blocks completion; no authorship hunt, older-code baseline
checkout, disabling assertion, extra sleep, or unsupported no-op substitute.

An unexplained S divergence outside X1–X9 is a material authority conflict, not permission for a local redesign. Preserve previous task checkpoints; do not edit their acceptance outcome to hide a regression.

### Authority Links

- [Spec revision 4](../spec.md)
- [Six-task traceability and dependency map](../decomposition.md)
- [Shared implementation contract](../implementation-contract.md)
- [Complete original task, verbatim historical source](../source/TASK-001-integrate-bifrost-variant-with-main.md)
- [Complete proof recipes and inspected selectors](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Reconciliation history](../reconciliation.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Part C — Observation and verification results (former TASK-004)

### Outcome and Value

Acknowledged source observations activate the correct main writer bindings once, and the retained verification runtime produces attributable typed Drift/Eval/Task evidence without coupling run settlement to result delivery.

Primary closure ownership: REQ-014 / AC-014, REQ-015 / AC-015, REQ-016 / AC-016. Supporting clause contributions: REQ-010, REQ-012, REQ-013, REQ-017, REQ-023, REQ-027, and this write set's share of AC-028. Inside Bifrost scope, main behavior enters only as X1, X2 and X9 (`assertion_result` nullable Variant in `vala.verification.results`); verification runtime, queues and result authoring are non-Bifrost main owners. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md) and [approved spec revision 4](../spec.md); revision 4's Revision 3 rule overrides any conflicting sentence in the contract or evidence files. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 4 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

### Owners, Scope, Consumers, and Prohibited Changes

Own ACK-to-ObservationRunOutbox/bulk queue/replay linkage, retained main claim/cache/baseline/lease/schedule/gateway authority, and typed result authoring/readback/application-Run lineage. Preserve source timestamp/Variant readers and result_id joins across detail days. Main judging authority, non-blocking result delivery and Task AssertionResult are approved exceptions; no new Task type/table, direct provider bypass or minted SYSTEM reader credential.

Depends on accepted TASK-003 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

#### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S audit delivery differs from X2 | X2: M one process outbox; S row/frame projection. Prohibited: any publisher or staging path | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-012 / AC-012 — Trusted internal writes | M ScribeCapturePeerGrpc::batch; ScribeIngressFrame attribution | P13; six destinations and public Gate refusals | S older two-destination path loses result/audit attribution | Retain six-destination mTLS and exact trusted UID scope | Existing private route; no minting publisher | S10/S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-014 / AC-014 — Observation activation | M ObservationRunOutbox/ObservationRunSink; enqueue_observation_batch | P15/P16; ACK decoder created_at and writer scope | S best-effort task and subject-only activation regress M | Retain M retry/bulk/writer/replay; adapt ACK fields | Existing outbox/SQL; no semaphore task replacement | S13 |
| REQ-015 / AC-015 — Verification runtime | M VerifierRunQueue::renew/tick_schedule; EvalEngine::execute; writer_caller; gateway caller | P15/P16; runner/cache/leases, direct routes | S removes renew/hydration/delay and direct-calls providers | Retain M runtime; port only S read/type projections | Existing verification owner; no second runner/provider path | S14 |
| REQ-016 / AC-016 — Result evidence | M ResultPayloadBuilder::build/summary; ResultRun; S typed result schemas | P15/P17; direct Task/continuous Eval/Drift results | S typed Drift/Eval summaries omit M Task and app Run meaning | X9: S typed structs + existing Task AssertionResult as the nullable assertion_result Variant column of S's results table | Existing AssertionResult; no new Task type/table | S15; `tables::tests::verification_tables_match_their_approved_schemas` |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

### Requirement Authority

[spec.md](../spec.md) revision 4 is the requirement text (Revision 3 rule, X1, X2, X9, REQ-014–REQ-017, AC-028); it is not copied here.

### Approach

1. Preserve main ACK-independent retry/bulk writer-aware enqueue, replay fence and lock order, using admitted original created_at rather than receipt time.
2. Retain main queue hydration, parsed/baseline caches, PostgreSQL leases/schedule clock, cancellation and direct invoke permission boundaries.
3. Route judging through the governed gateway with the correct writer/judging authority and preserve application Run lineage.
4. Compose source nullable typed Drift/Eval summaries and existing Task assertion_result Variant with result/outbox producers/readers and result_id joins.
5. Prove outage/next-day replay, denied/allowed judge calls, all terminal implementation payloads and split-day summary/details; rerun UID/audit/durable ACK regressions.

### Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require Behavior/RED/GREEN/REFACTOR; unchanged S behavior uses verification-only proof.

#### S13 — ACK-derived Eval activation survives outage and next-day replay

**Behavior.** Observation ACK remains independent of enqueue availability, queue retries retain work, matching writer bindings activate once, and later-day replay uses original created_at to read the same subject record. Mapped obligations: REQ-014 / AC-014, REQ-017 / AC-017.

**RED.** Run P15/P16 and ACK decoder in P12. Extend existing replay/outage assertions with source UID/Variant created_at and different receipt day. Expected RED is all-subject activation, lost enqueue, duplicate run, or record lookup under the receipt day.

**GREEN.** Retain main outbox/bulk writer-aware queue and replay fence; adapt admitted ACK fields/time. Rerun S04/S12 with changed frame identities.

**REFACTOR.** No best-effort semaphore task replacement or per-binding SQL loop; preserve lock order and batching.

#### S14 — Verification keeps leases, schedule clock and gateway authority

**Behavior.** The database owns claim/renew/schedule timing; expired/stale leases cannot revive and cancel their work; hydrated/cache baselines remain correct. Judge calls pass through gateway under the right authority and denial sends no provider request. Mapped obligations: REQ-015 / AC-015.

**RED.** Run P15/P16. Extend existing terminal/direct-judge matrix with explicit denied/allowed gateway assertions and retained writer/Run fields. Expected RED is direct provider bypass, missing renewal/cancel, missed delayed occurrence, or wrong attribution.

**GREEN.** Keep main queue/runtime/authority/cache/leases and gateway invocation; port source Variant/timestamp read decoding only. Rerun S13 and S11 decision/lineage proof.

**REFACTOR.** Keep existing verification owners; no duplicate runner, minted SYSTEM reader token or repeated per-run baseline lookup.

#### S15 — Every implementation retains its result and application lineage

**Behavior.** Drift/Eval use typed source summaries, Task uses existing AssertionResult in nullable assertion_result Variant, and exactly the matching implementation payload is populated. Application run_id stays distinct from verifier execution identity; detail joins survive a different ingestion day. Mapped obligations: REQ-016 / AC-016, REQ-017 / AC-017.

**RED.** Run P17, result/Task selectors in P15, and the X9 contract test in P12 (`tables::tests::verification_tables_match_their_approved_schemas` must pin `assertion_result` as a nullable Variant and no other S column change). Extend existing result-layout journey for ended_at versus detail ingestion midnight. Expected RED is lost Task payload/fields, text decode, duplicated summaries, verifier UUID as app Run, or pruned detail rows.

**GREEN.** Compose main result/outbox and source schema/projectors; use result_id joins and source nullable semantics. Rerun S11/S14 under combined schemas.

**REFACTOR.** Use existing AssertionResult and result contracts; no new Task struct/table or synthetic shared event time.

### Acceptance Criteria

- **TASK-004-C01.** ACK never waits for derived enqueue; outage/retry retains work; writer-matched bindings activate once on original created_at even when replay receipt occurs another day.
- **TASK-004-C02.** Main lease/cache/baseline/schedule/hydration behavior survives; stale/expired leases cannot revive and cancellation is owned.
- **TASK-004-C03.** Denied judging performs zero provider requests; allowed calls use governed gateway under exact authority, not direct ProviderRegistry.
- **TASK-004-C04.** Drift/Eval typed summaries and Task-only nullable AssertionResult Variant retain complete payload semantics; app run_id differs from Verifier execution UUID. X9: `tables::tests::verification_tables_match_their_approved_schemas` passes with `assertion_result` as the only difference from S's results schema.
- **TASK-004-C05.** Summary/detail joins work across midnight ingestion using result_id; delivery remains shared-outbox/non-blocking and settlement retains separate exact result/Verifier identities.
- **TASK-004-C06.** AC-028 share: every hunk of this write set's Bifrost files against S (`git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- <Bifrost write-set paths>`) maps to X1, X2 or X9.
- **TASK-004-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-004-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

### Expected Write Set and Consumer Closure

verification observations/outbox/ACK decoding; wyrd-sql verifier queues and binding/writer identity; verification runner/cache/leases/scheduler/authority/eval/drift/gateway callers; result builder and Vala typed summaries/detail schema/reader/query windows; real Eval/Task/Drift/direct-judge/lease/schedule/result-layout tests, affected generated/docs surfaces.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

### Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P12, P13, P15, P16, P17, P18. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

#### Non-TDD proof

Relevant I18/I24/I26/I27 contributions (removal/history/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

#### Exact Selected Recipes

##### P12 — Native UID/ACK/audit projection, X9 results contract and actual Gate adapter

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_carry_subject_and_created_at) | test(=tables::tests::verification_tables_match_their_approved_schemas) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

##### P13 — Real retained audit and six-destination private validator

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_retention::retained_history_carries_both_credential_shapes) | test(=audit_retention::system_owner_security_rejections_retain_once)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'
```

##### P15 — Evaluation/Task/trace/result layout real journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

##### P16 — Verification queue, schedule, lease and judging dependency proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown) | test(=pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart) | test(=pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it) | test(=pg_verification_routes::direct_execution_calls_the_judge_provider)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_runs::observation_runs_follow_the_writer) | test(=pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence) | test(=pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences) | test(=pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive) | test(=pg_verifier_runs::reclaimed_lease_fences_the_stale_token)"'
```

##### P17 — Result authoring and schema-by-name

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::results::tests::direct_task_writes_one_summary_row) | test(=verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary) | test(=verification::results::tests::sampled_out_eval_writes_zero_count_summary_only) | test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names)'
```

##### P18 — SQL bind forwarding and protected gateway payloads

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E "test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired)"'
```

### Material Stop Conditions

- A required product/wire/persisted/security/lifecycle outcome contradicts
  approved revision 4 or needs a different decision. Finish unaffected work,
  cite the exact conflict and request a semantic revision; do not guess.
- Supplied compaction commit `22cd178d14009468bc951a14d7acf63237af808a`
  becomes unreachable or fails source Arrow60 compatibility or main no-REST/Ring
  narrowing in the combined graph. Report the concrete failure; do not replace
  the pin silently, waive checks, or publish externally without authorization.
- Work would reset/migrate retained old Bifrost objects or damage main control
  plane. Fresh-only scope does not authorize data deletion.
- A proposed fix requires a new public client/service, dependency family,
  cross-tenant/raw SQL boundary, scope widening, or optional feature not earned
  by the approved analytical graph/foreign-runtime boundary.
- User/main/source input pins change materially; record the delta and resolve
  its authority before extending this packet's scope.

Compiler, lint, setup, current test-name, private plumbing or sanctioned fixture
failures are ordinary implementation diagnosis, not material stop conditions.
A required red check blocks completion; no authorship hunt, older-code baseline
checkout, disabling assertion, extra sleep, or unsupported no-op substitute.

An unexplained S divergence outside X1–X9 is a material authority conflict, not permission for a local redesign. Preserve previous task checkpoints; do not edit their acceptance outcome to hide a regression.

### Authority Links

- [Spec revision 4](../spec.md)
- [Six-task traceability and dependency map](../decomposition.md)
- [Shared implementation contract](../implementation-contract.md)
- [Complete original task, verbatim historical source](../source/TASK-001-integrate-bifrost-variant-with-main.md)
- [Complete proof recipes and inspected selectors](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Reconciliation history](../reconciliation.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Part D — Public consumers and product lifecycle (former TASK-005)

### Outcome and Value

Real Rust/Python/TypeScript SDK and agent callers register, emit, query, verify and shut down through one coherent source-authoritative contract while main Service, Workflow, MCP, gateway, identity and local bootstrap behavior survives.

Primary closure ownership: REQ-017 / AC-017, REQ-018 / AC-018, REQ-019 / AC-019, REQ-020 / AC-020, REQ-021 / AC-021, REQ-022 / AC-022, REQ-023 / AC-023. Supporting clause contributions: REQ-001, REQ-002, REQ-008, REQ-010, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, REQ-025, REQ-026, REQ-027, and this write set's share of AC-028. REQ-024 is struck: Bifrost migrations are S's files edited in place (INV-009). Inside Bifrost scope, main behavior enters only as X1–X9 adapted to S's owners; X5 (ten built-ins, no `vala.dev.agent_traces`), X6 (typed query parameters) and X7 (main identity at the Bifrost entrance) are this task's main exceptions. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md) and [approved spec revision 4](../spec.md); revision 4's Revision 3 rule overrides any conflicting sentence in the contract or evidence files. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 5 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

### Owners, Scope, Consumers, and Prohibited Changes

Own full public consumer closure and the atomic terminal/parameter protocol across spec/protobuf/conversions/server/forwarder/shared client/native bindings/SDKs/CLI/MCP. Reuse TASK-001's already-consistent foundation, adding required actual cross-surface behavior/assertions rather than temporary compatibility. Preserve before-Card-transaction Service ensure and lock-decided Created/AlreadyExists; all-ten eager schemas/payload restrictions and main installed bootstrap. Do not introduce a second client/transport, remove newer main errors, revive broad exports or overwrite main state/boot wholesale.

Depends on accepted TASK-004 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

#### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-001 / AC-001 — Source analytical engine | S BifrostCatalog, OraclePlanner, ScribeIngress, ForgeRewriteCommit; catalog/oracle/scribe/forge/storage owners | S published/distributed journeys; P01/P22 | M consumers still assume replaced providers/schema/storage | S engine unchanged; adapt consumers, not old owners | S rewrite owns real native execution/storage state; no additional engine justified | S01/S03/S04/S20; TASK-002 P-S |
| REQ-002 / AC-002 — Snapshot retention/deadlines | S OraclePlanner::request_deadline/pin_cut; OracleTableCuts::read; Forge expiry protections | P03; S `forge::reader_expiry_ordering::*` | None: S owns it | S retention/clamp unchanged; no reader workflow | Existing S retention is the authority; no reader-pin layer | S02 |
| REQ-008 / AC-008 — Membership lifetime | M ClusterTask; ClusterRegistry; BifrostComposition.membership_poller | P08/P09; Oracle/Scribe teardown and boot | S process token stops discovery before drain | Retain M final role tasks + one process poller | Existing concrete owners retain task lifetime | S08 |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S audit delivery differs from X2 | X2: M one process outbox; S row/frame projection. Prohibited: any publisher or staging path | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-011 / AC-011 — Audit schema/attribution | M project_audit_event; GateAudit blanket impl; Oracle query_audit_event; S AuditLogTable types | P12/P13; actual keyed Gate/Oracle -> retained history | M builders omit credential id | X2: M event content + S created_at/Variant; carry credential. Prohibited: chain columns or staging | Existing event/sink/projection; no chain/hash authority | S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-014 / AC-014 — Observation activation | M ObservationRunOutbox/ObservationRunSink; enqueue_observation_batch | P15/P16; ACK decoder created_at and writer scope | S best-effort task and subject-only activation regress M | Retain M retry/bulk/writer/replay; adapt ACK fields | Existing outbox/SQL; no semaphore task replacement | S13 |
| REQ-015 / AC-015 — Verification runtime | M VerifierRunQueue::renew/tick_schedule; EvalEngine::execute; writer_caller; gateway caller | P15/P16; runner/cache/leases, direct routes | S removes renew/hydration/delay and direct-calls providers | Retain M runtime; port only S read/type projections | Existing verification owner; no second runner/provider path | S14 |
| REQ-016 / AC-016 — Result evidence | M ResultPayloadBuilder::build/summary; ResultRun; S typed result schemas | P15/P17; direct Task/continuous Eval/Drift results | S typed Drift/Eval summaries omit M Task and app Run meaning | S typed structs + existing Task AssertionResult as assertion_result Variant | Existing AssertionResult; no new Task type/table | S15 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-018 / AC-018 — Remove dev table | M canonical built-in inventory/provisioning; S dev/CodeAxis sole owner | Static I18; source dev tests -> supported/dynamic property proof | X5: `vala.dev.agent_traces` is removed and must not be created by any S migration, registration, doc or test | Ten built-ins, table and CodeAxis removed; transfer still-reachable assertions to supported/dynamic tables | No test-only inventory or extra name-ban check | I18/S16/S20 |
| REQ-019 / AC-019 — Typed SQL parameters | M Bifrost::sql/sql_as/stream; QueryParam; S OraclePlanner | P18/P19/P23; HTTP/grpc/SDK query bodies | S unchanged-base signatures omit newer params | Bind M typed ordered values in S plan and keep public argument order | Existing planner/transport; no interpolation/second client | S17 |
| REQ-020 / AC-020 — Terminal problems | S WyrdProblem; tonic query_conversion; M from_problem/error catalog | P19/P22/P23; grpc/http/client/CLI/MCP collectors | Wire terminal changes must agree and preserve M other errors | Atomic S problem field with M params/security | Existing typed catalog/conversions; no compatibility parser | S18 |
| REQ-021 / AC-021 — Shared types/client boundaries | S wyrd-types schema mapper/Variant/timestamps; M Bifrost handle and Service declared_tables | P01/P19/P23; cards/service.rs preflight and SDK exports | Queue mapper removed; broad source exports regress M internals | Use S shared mapper and M narrow facade/preflight | Source shared type owner replaces duplicate mapper, no new transport | S01/S16/S19 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| INV-009 — S Bifrost migrations in place | S `crates/vala/vala-sql/migrations` | `pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent`; `pg_schema_usage::pg_tests::*` | None: REQ-024 struck | X2/X5 applied by editing S's files; no forward, drop or main-only Bifrost migration and no migration-history test | Nothing shipped | Static diff vs S |
| REQ-025 / AC-025 — Fresh v3 contract | S create_physical_table/physical_table_matches; spec no data migration | P28; dynamic/fresh built-ins and format refusal extension | Old tables/objects are not schema-compatible | Fresh isolated Bifrost; reject old schema/version without mutation | No conversion/dual reader; old-state deployment excluded | S20 |
| REQ-026 / AC-026 — Native graph/tooling | S native Cargo pins/ObjectStore factory helpers; M deps.sh/Hakari/profiles | Static I26; scoped dependency/Clippy/codegen checks | Locks/lints/test scripts/signing drift between snapshots | Resolve combined graph; retain M deleted tooling and artifact storage | Combined compatible compaction pin + required analytical helpers; S workloads in the one `capacity` binary (`bench:bifrost:query-capacity`, `bench:bifrost:write-capacity`, `bench:bifrost:client-queue`); no unrelated signer rewrite | I26/S23 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

### Requirement Authority

[spec.md](../spec.md) revision 4 is the requirement text (Revision 3 rule, Migrations, X1–X9, REQ-017–REQ-023, REQ-025–REQ-027, AC-028); it is not copied here.

### Approach

1. Complete declared schema/time/UID and Service preflight through existing shared mapper and registration outcome under lock; prove nested/time/Variant combined Service boundary.
2. Close all ten built-in producers/readers, payload-read security and removed dev/CodeAxis assertions across public surfaces.
3. Preserve ordered parameters and public argument positions in source planning/shared transport/bindings without SQL interpolation or changed deadline policy.
4. Prove terminal WyrdProblem propagation atomically across all surfaces, including failure after an initial batch, Python stable error_code/detail and protobuf reserved old field.
5. Retain connected main Workflow/MCP/gateway/Operator/Trigger/identity/verification owners and ordered shutdown; prove installed bootstrap selected HTTP/grpc config in each owning language.
6. Port source assertions and fixtures into main consolidated targets, regenerate outputs and update served OpenAPI/runtime MCP/examples/owning docs as contracts change.

### Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require Behavior/RED/GREEN/REFACTOR; unchanged S behavior uses verification-only proof.

#### S01 — Fresh registration and lossless typed rows

**Behavior.** A fresh user/model/Service table describes the registered schema and time axis, rejects undeclared/type-invalid rows before admission, and returns nested Struct/List/Variant and explicit timestamp values through the source contract. Mapped obligations: REQ-001 / AC-001, REQ-017 / AC-017, REQ-021 / AC-021.

**RED.** Use P01 and the source Python/TS file recipes P23; extend the registration/round-trip assertions at the combined Service and UID boundary. Expected RED is mismatched schema/time/Variant decoding or acceptance of a forbidden row, not an absent test module.

**GREEN.** Import source type mapping, preflight, declared layout, and projection in current owners; adapt main Service table preflight to the new mapper. Rerun the specific registering/writing/reading cases that changed.

**REFACTOR.** Remove duplicate mapper/conversion paths; preserve per-column types, strict validation and narrow client exports.

#### S16 — Ten built-ins and protected payloads use declared schemas

**Behavior.** All ten built-ins are eagerly available with source time/types; `vala.dev.agent_traces` and its CodeAxis owner are removed (X5) and no S migration creates the table. Gateway Variant payload projection/predicate/subquery access still requires payload-read authority; metadata remains ordinarily readable. Mapped obligations: REQ-017 / AC-017, REQ-018 / AC-018, REQ-021 / AC-021.

**RED.** Extend current query journey P19 with all-ten description/empty reads and removed-table absence, then run P18 for renamed payload protections. Expected RED is an old schema, any table or registration for the removed `vala.dev.agent_traces` (X5), a missing eager table, or payload authority bypass. Static I18 complements rather than replaces real reads.

**GREEN.** Update canonical table registrations, all consumers/time predicates and protected payload names. Transfer generic removed-table Variant assertions to supported/dynamic owners. Rerun S01/S12/S15.

**REFACTOR.** Remove sole-owner dev/CodeAxis definitions/docs/tests; do not add name-ban machinery or a test-only inventory.

#### S17 — Bound SQL stays data across first-class clients

**Behavior.** Ordered parameters bind in source plan and forward unchanged through HTTP/gRPC/Rust/Python/TS, preserving argument positions and separating hostile text from SQL. Source deadline behavior is unchanged. Mapped obligations: REQ-019 / AC-019.

**RED.** Run P19/P23 and shared-client bind proof P18. Expected RED is dropped/reordered values, interpolation, wrong TS rows/params position, or changed result projection.

**GREEN.** Carry main request/facade/native parameter projection into source planning and update imported callers. Rerun S01/S02/S16 typed reads.

**REFACTOR.** Reuse shared transport/planner and existing typed QueryParam conversion; no alternate query client or parameter parser.

#### S18 — Failure after rows remains a typed failed terminal

**Behavior.** Server/grpc/forwarder/client/SDK/CLI/MCP reconstruct the source WyrdProblem exactly; prior rows do not become complete success, old protobuf field remains reserved, and main newer non-Bifrost errors remain available. Mapped obligations: REQ-020 / AC-020.

**RED.** Extend existing streaming/error owners in P19/P22/P23 with an induced failure after an initial batch and assert typed problem/terminal behavior. Expected RED is missing problem fields, legacy terminal decoding, partial success, or widened worker context. Run P29 for actual MCP/CLI and HTTP framing; extend its compiled CLI/MCP journeys for late failure because the framing unit alone is insufficient.

**GREEN.** Integrate terminal contract and consumers atomically with main params/security; Python error_code is the catalog code and error_detail is the problem detail. Rerun S09/S17 frame and request proof.

**REFACTOR.** Use derive-backed catalog reconstruction; no manual parallel code map or old-terminal compatibility shim.

#### S19 — Main app owners and Service registration stay connected

**Behavior.** Service declared tables validate/ensure through source mapper before Card transaction, main Workflow/MCP/gateway/Operator/Trigger/auth owners retain mounts/config/authority, and shared outbox is boot-composed once. Mapped obligations: REQ-021 / AC-021, REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Run Service preflight and composed OpenAPI P30, changed-boot P24, and Workflow P20/P21; extend these existing proof owners for the Variant table and connected main owner seams. Expected RED is table preflight divergence, missing route/owner, duplicate outbox, or wrong credential/tenant composition. Record exact newly selected owner commands before GREEN.

**GREEN.** Adapt main state/boot and Service mapper use around source role/resource/storage construction; consume source registration outcome under its lock, never infer Created/AlreadyExists by a prior describe. Rerun S10/S12/S14/S16 after constructors change.

**REFACTOR.** Retain existing app services/handles; no new umbrella owner, graph layer or generic bootstrap wrapper.

#### S21 — Shutdown and installed local bootstrap preserve real callers

**Behavior.** Accepted Workflow tools settle before terminal run state; admission/reservations and abandoned blocking work remain tracked. Installed CLI provisions empty state, saves selected endpoints/credentials, and subsequent Rust/Python/TS/MCP clients work without developer credential leakage. Mapped obligations: REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Run P20/P21/P25/P26. Expected RED is a detached query, forgotten preparation, removed app owner, stale saved gRPC endpoint, altered credential state, or inherited local config winning. Bootstrap starts unmigrated; do not pre-migrate that fixture.

**GREEN.** Retain main product drain/config/endpoint persistence around source Bifrost lifetimes. Rerun S07-S11 where teardown/routing changes.

**REFACTOR.** Use existing trackers, installed harness and isolated SDK fixture; no duplicate test server or new config surface.

### Acceptance Criteria

- **TASK-005-C01.** S01 proves the combined Service-declared table/UID/nested/time/Variant workflow and strict refusal; no describe-before-create race changes registration outcome.
- **TASK-005-C02.** S16 proves all ten eager schemas and supported empty reads, the removed `vala.dev.agent_traces` table and CodeAxis owner and gateway payload protections without assuming audit history stays empty after authorized calls.
- **TASK-005-C03.** S17 proves typed bind values and original public argument positions through all first-class runtimes with hostile text remaining data; source deadlines remain clamped.
- **TASK-005-C04.** S18 proves full typed problem and no complete success after delivered rows in shared SDK/HTTP/grpc/forwarding/CLI/MCP; preserve stronger source assertions and the reserved old terminal field.
- **TASK-005-C05.** S19/S21 prove Service-before-Card ensure, connected main owners, tracked Workflow/query settlement and dependency-preserving ordered shutdown.
- **TASK-005-C06.** Installed CLI starts from unmigrated isolated state, preserves credentials/control plane after repeat/failure/interruption and saves selected endpoints consumed by real Rust/Python/TS/MCP callers.
- **TASK-005-C07.** Source assertions, runtime catalogs, served OpenAPI, generated schemas/stubs/declarations, examples and architecture agree; no user-visible negative flow is replaced by a unit-only substitute.
- **TASK-005-C08.** AC-028 share: every hunk of this write set's Bifrost files against S (`git diff cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- <Bifrost write-set paths>`) maps to X1–X9; `crates/vala/vala-sql/migrations` differs from S only by the X2/X5 in-place edits, and `rg -n 'agent_traces' crates architecture docs sdks` reports only text stating the table is removed.
- **TASK-005-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-005-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

### Expected Write Set and Consumer Closure

spec/API/errors/schema and tonic/proto converters; Oracle/forwarding/collectors; wyrd-client facade/query/transport/schema helpers and credential pooling; Rust/Python/TS SDK boundary/exports/stubs/declarations; Service/cards preflight; main state/boot/routes/workflow/query tracker/shutdown; CLI/MCP and installed harness; focused source/client/product journeys, examples/docs/CI registration.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

### Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P01, P03, P09, P12, P14, P15, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P29, P30. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

#### Non-TDD proof

Relevant I18/I26/I27 contributions (removal/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

#### Exact Selected Recipes

##### P01 — Declared schema/time conformance (source lib owners)

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=catalog::layout::tests::time_column_drives_partition_and_sort)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=catalog::layout::tests::invalid_time_column_is_refused_naming_the_field)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::parquet_writer::tests::row_group_bounds_read_the_time_column)'
```

##### P03 — Source deadline projection and retention configuration

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::tests::request_deadline_projects_unusable_budgets_to_the_default)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=config::tests::max_query_deadline_sits_between_default_deadline_and_retention)'
```

##### P09 — Real membership lifetime and audit drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

##### P12 — Native UID/ACK/audit projection and actual Gate adapter

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_carry_subject_and_created_at) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

##### P14 — OTLP API-key and UID runtime entrances

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

##### P15 — Evaluation/Task/trace/result layout real journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

##### P17 — Result authoring and schema-by-name

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::results::tests::direct_task_writes_one_summary_row) | test(=verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary) | test(=verification::results::tests::sampled_out_eval_writes_zero_count_summary_only) | test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names)'
```

##### P18 — SQL bind forwarding and protected gateway payloads

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E "test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired)"'
```

##### P19 — Focused first-class Rust query/observation/verification stories

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

##### P20 — Accepted Workflow owner accounting

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists) | test(=components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned)'
```

##### P21 — Real Workflow shutdown and forwarded query settlement

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_workflow_runs::lifecycle_races_retention_and_shutdown)" && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'
```

##### P22 — Source logical Variant reads and recovered publication

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::variant_sql_registry_covers_every_session)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=recovery::variant_staging_restores_and_publishes_once)'"

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
```

##### P23 — Focused Python/TypeScript public stories and imported source capability files

```bash
# Runtime prerequisites once, before the focused tests below.
mise run py:setup
mise run ts:build
mise run ts:build:testing

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_query_bifrost.py::test_bound_sql_text_is_treated_as_data tests/integration/test_query_bifrost.py::test_stream_yields_arrow_batches_and_a_terminal tests/integration/test_observe_a_run.py::test_run_observations_read_back_by_run_id tests/integration/test_observe_a_run.py::test_card_scoped_key_cannot_write_another_cards_observations tests/integration/test_scheduled_drift_alerts_operator.py::test_failed_schedule_alerts_its_operator_on_the_connection_origin tests/integration/test_verify_in_real_time.py::test_exact_verifier_grant_verifies_only_that_verifier'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/query-bifrost.test.ts tests/integration/observe-a-run.test.ts tests/integration/scheduled-drift-alerts-operator.test.ts'

# S capability files, present at these paths.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/bifrost/test_variant_tables.py tests/integration/bifrost/test_every_iceberg_column_type.py tests/integration/bifrost/test_register_a_table_from_a_model.py tests/integration/bifrost/test_three_timestamp_types.py tests/integration/bifrost/test_table_time_column.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/variant-tables.test.ts tests/integration/every-iceberg-column-type.test.ts tests/integration/register-a-table-from-a-model.test.ts tests/integration/three-timestamp-types.test.ts tests/integration/table-time-column.test.ts'

# Rehomed Rust client cases: discover actual final symbols in existing target.
mise exec -- cargo nextest list --locked -p wyrd-client --features test-support --test integration
```

##### P24 — Changed boot, credentials, signing boundary and platform initialization

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=boot::auth::tests::build_auth_handles_mints_tokens_the_request_verifier_accepts) | test(=boot::auth::tests::build_auth_handles_rejects_an_invalid_signing_key)'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=dsn::tests::explicit_serving_dsns_stay_separate_and_derive_the_catalog) | test(=dsn::tests::platform_url_falls_back_to_database_url)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=platform_admin_e2e::an_operator_initializes_the_deployment_through_the_shipped_binary) | test(=platform_admin_e2e::operator_provisions_a_usable_tenant_without_touching_the_database) | test(=platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions)"'
```

##### P25 — Installed CLI bootstrap (unmigrated fixture)

```bash
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
```

##### P26 — Installed local callers in owning language runtimes

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[typescript]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[exit]'
```

##### P29 — Mounted MCP, compiled CLI and late HTTP terminal

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots) | test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution) | test(=query::pg_tests::delegated_agent_query_is_authorized_as_the_subject)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey -E "test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)"'

mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=query::routes::tests::http_query_stream_preserves_late_failed_terminal)'
```

##### P30 — Service before-activation preflight and served OpenAPI

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_card_registration_route::service_declared_tables_are_ensured_before_activation)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_openapi_contract::bifrost_operations_publish_typed_problem_refusals) | test(=pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code) | test(=pg_openapi_contract::no_operation_documents_an_audit_caused_refusal)"'
```

### Material Stop Conditions

- A required product/wire/persisted/security/lifecycle outcome contradicts
  approved revision 4 or needs a different decision. Finish unaffected work,
  cite the exact conflict and request a semantic revision; do not guess.
- Supplied compaction commit `22cd178d14009468bc951a14d7acf63237af808a`
  becomes unreachable or fails source Arrow60 compatibility or main no-REST/Ring
  narrowing in the combined graph. Report the concrete failure; do not replace
  the pin silently, waive checks, or publish externally without authorization.
- Work would reset/migrate retained old Bifrost objects or damage main control
  plane. Fresh-only scope does not authorize data deletion.
- A proposed fix requires a new public client/service, dependency family,
  cross-tenant/raw SQL boundary, scope widening, or optional feature not earned
  by the approved analytical graph/foreign-runtime boundary.
- User/main/source input pins change materially; record the delta and resolve
  its authority before extending this packet's scope.

Compiler, lint, setup, current test-name, private plumbing or sanctioned fixture
failures are ordinary implementation diagnosis, not material stop conditions.
A required red check blocks completion; no authorship hunt, older-code baseline
checkout, disabling assertion, extra sleep, or unsupported no-op substitute.

An unexplained S divergence outside X1–X9 is a material authority conflict, not permission for a local redesign. Preserve previous task checkpoints; do not edit their acceptance outcome to hide a regression.

### Authority Links

- [Spec revision 4](../spec.md)
- [Six-task traceability and dependency map](../decomposition.md)
- [Shared implementation contract](../implementation-contract.md)
- [Complete original task, verbatim historical source](../source/TASK-001-integrate-bifrost-variant-with-main.md)
- [Complete proof recipes and inspected selectors](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Reconciliation history](../reconciliation.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
