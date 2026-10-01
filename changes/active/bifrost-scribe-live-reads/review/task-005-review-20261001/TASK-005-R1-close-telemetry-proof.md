---
id: TASK-005-R1
title: Close Scribe backlog publication and original telemetry proof
kind: remediation
status: proposed
---

# TASK-005-R1 — Close Scribe backlog publication and original telemetry proof

**Approved spec:** `changes/active/bifrost-scribe-live-reads/spec.md` (revision 24, REQ-012, INV-009, AC-014; benchmark validity also intersects REQ-008, AC-009, AC-010). **Original task:** `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` (user-designated revision 14, SHA-256 `0e62758cd6790451c2fa7497c06f6eba7d57400ed15ea38b50695c30b0177814`). **Cumulative candidate:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2`, excluding `1f1cbcf5f`. See [verdict.md](verdict.md) and [findings-validation.md](findings-validation.md). Implement against the cumulative candidate and return to `$wyrd-task-review` afterward.

## Diagnosis and required outcome

| Finding | Current behavior, exact evidence, and consequence | Required outcome and smallest safe correction |
| --- | --- | --- |
| FIND-TASK-005-1 | `ScribeStagingRuntime` mutates `assembly`, drops its mutex, then `publish_backlog()` reacquires it and sets four gauges (`scribe/staging_runtime.rs:165-180,239-247,309-320,400-411,783-791`). Persistence workers share the runtime (`scribe/persistence.rs:1960,1980,2022,2063`). An older snapshot may publish after a later transition and leave a false backlog or false zero. The sequential restore test cannot expose this race. | Keep each registration, claim take/residue, settlement, and restore ownership change paired with publication of its resulting `StagingBacklog` while serialized by the **existing assembler mutex**. Preserve durable IO and state authority outside that lock. Do not create a second telemetry ledger, lock, or version. The final four gauges must equal actual ready plus claimed ownership after concurrent transitions. |
| FIND-TASK-005-2 | Original Scenario 1 names `telemetry::staged_backlog_survives_abrupt_restart`, but that selector is absent. `scribe/lifecycle.rs:205-228` restarts and reads rows without scraping; `scribe/telemetry.rs:320-378` scrapes without restart. The fresh-recorder `staging_runtime.rs:1614` test proves emission, not the full client/server recovery path. | Add the named real-server Scribe journey using the existing retained-root cluster restart, public client readback, production `/metrics` scrape, and publication operations. Prove nonzero count/bytes and persisted oldest timestamp before and after abrupt restart, actual outstanding claim count, readback before publication, and zero backlog/claims plus committed rows after publication. Retain the fresh-recorder test as independent emission-origin proof. |
| FIND-TASK-005-3 | The candidate dashboard table in `TASK-005-simplify-bifrost-telemetry.md:440-449` names families and tests but lacks rendered numeric before/after samples, independent expected facts, and a family/series inventory. Thus the original task's six chart meanings and removed-series claims cannot be audited from its evidence. | Reuse production recorder and trace capture from focused journeys. Append a compact evidence table for every original dashboard question: family and bounded labels or trace, unit and boundary, before/after samples at the owner transition (including held queue/stage and restart), independent client/owner/durable fact, focused test result, and trace IDs/parentage. Include removed-family/series inventory. Explain attempt versus new-data and Gate/Oracle/client timing gaps. Do not fabricate baseline or test-only metrics. |
| FIND-TASK-005-4 | Candidate evidence says `bench:bifrost:query-capacity`, whole Scribe/Oracle/Forge journey lanes, and `mise run gate` were not run (`TASK-005-simplify-bifrost-telemetry.md:414-419`). Original AC 4 and Verification and Evidence require them; `AGENTS.md` §11 also requires a broad gate for this multi-owner change. Focused green tests cannot establish benchmark validity or cross-owner acceptance. | Run the existing standard benchmark first with its normal local setup and configured object store; preserve raw samples/report and compare with recorded prechange evidence where available against approved targets. Diagnose any missed target or invalid workload. Then run the existing whole Scribe, Oracle, and Forge journey lanes individually and `mise run gate`, recording commands and exits. If prechange samples are unavailable, state that limit instead of claiming a measured comparison. No new workload, threshold, matrix, or harness. |
| FIND-TASK-005-5 | Newly added ordinary function-local imports occur at `scribe/tests/wal_closeout.rs:292`, `wyrd-testing/src/bifrost/scribe_workload.rs:728`, `wyrd-testing/src/bifrost/telemetry.rs:3415`, and `oracle/telemetry.rs:20`. Changed signatures fully qualify `StagingBacklog` at `scribe/staging_runtime.rs:175`, `scribe/persistence.rs:1007`, `scribe/mod.rs:2226`, and `DataTenantId` at `wyrd-testing/tests/bifrost/oracle/published.rs:602`. This violates `architecture/agent-rules.md`'s module import convention and obscures typed dependencies. | Follow existing module-top import convention, remove the duplicate `MemoryCategory` import, and use bare names in changed signatures. Preserve logic and assertions. |

## Constraints and non-goals

Preserve write ACKs, WAL retirement, Iceberg publication, admission, query terminals, tenant checks, Forge settlement, public SDK contracts, and the closed Forge metric catalog. Keep Scribe backlog measurements derived from `StagingAssembler`; no shadow state, exporter, framework, compatibility series, or high-cardinality labels. A proof gap must be closed with production-shaped evidence rather than a synthetic gauge or weakened assertion. Do not run a new benchmark matrix or the separate 100M-row qualification solely for this cleanup. If a required benchmark or lane fails, diagnose and fix its actual cause under the repository's gate rules before closeout; a new product or architecture decision returns to specification review.

## Acceptance and closure proof

1. **FIND-TASK-005-1:** A controlled concurrent stage/claim/settle path ends with all four scraped gauges matching `runtime.backlog()`, including final zero; the existing fresh-recorder restore proof remains green.
2. **FIND-TASK-005-2:** The original exact `telemetry::staged_backlog_survives_abrupt_restart` command passes and proves production scrapes, SDK readback, persisted oldest timestamp, restored claims, publication, and replay behavior across replacement. Keep and run the exact `scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog` command.
3. **FIND-TASK-005-3:** The original task contains auditable production before/after values, independent facts, focused results, trace parentage, and family/series inventory for all six dashboard questions.
4. **FIND-TASK-005-4:** `mise run bench:bifrost:query-capacity` produces valid raw outputs and target assessment; Scribe, Oracle, and Forge whole journey lanes and `mise run gate` pass, with exits recorded. Investigate any material regression; no unsupported comparison claim.
5. **FIND-TASK-005-5:** Source inspection shows module-top imports and bare changed signature types; `mise run fmt` and `mise run lints` pass.

Run the original task's remaining exact focused scenario commands through `mise` and the applicable docs check when its evidence or docs change. Preserve the candidate's passing trace, result, and durable assertions. Route this packet directly to `$wyrd-implement`.

## R1 implementation evidence

Commits: `7835f8535` (FIND-1), `052641ab4` (FIND-5), `2f557355a` (FIND-2),
`e43cf80bd` and `6d5cad774` (journey evidence prints), `49f3b601d` (release
build fix), `681de6bfd` (raw outputs), and the closing evidence commit.
Raw outputs: [r1-outputs/](r1-outputs/).

| Finding | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| FIND-TASK-005-1 | `scribe/staging_runtime.rs`: `lock_assembly()`; every transition publishes `assembly.backlog()` before releasing the assembler guard; `publish_backlog` removed | `scribe::staging_runtime::tests::concurrent_transitions_publish_the_final_backlog` (4 threads × 8 rounds, final gauges 0 = owner), `staged_backlog_counts`, `pg_tests::restored_stage_republishes_backlog` | PASS |
| FIND-TASK-005-2 | `tests/bifrost/scribe/telemetry.rs::staged_backlog_survives_abrupt_restart` on `WyrdTestCluster` | Scraped `[members, bytes, oldest, claims]` = owner `[1, 3464, 1790876177, 0]` before abrupt kill and after restore; replacement serves 48 rows; resend inserts 0; publication → 0 and 48 committed rows | PASS |
| FIND-TASK-005-3 | "R1 dashboard evidence" appended to the original task: per-question production samples, independent facts, trace ids/parentage, 105-family inventory, 18 removed families, timing/accounting gaps | Focused Scribe, Oracle, Forge journeys with `--no-capture` (logs in `r1-outputs/`) | PASS |
| FIND-TASK-005-4 | Benchmark run first; whole Bifrost journey lanes run | See verification below | PASS for lanes; benchmark FAIL, diagnosed |
| FIND-TASK-005-5 | Module-top imports and bare signature types in the touched test, support, and Scribe files | `mise run fmt`, `mise run lints` | PASS |

### Verification

- `mise exec -- cargo build --locked --release -p wyrd-server` first failed:
  `queries overflow the depth limit` computing the layout of the
  `dispatch_sql` future (`wyrd-server/src/oracle/forwarding.rs:148`).
  Diagnosis (fresh diagnostician, `-Z treat-err-as-bug` query stack): 131
  layout frames through Oracle forwarding → catalog lookup →
  `TenantConn::acquire` → `begin_bound` → SQLx `Pool::begin_with` connect/TLS
  (~74 frames). The SQLx call entered with task-004 commits `35343dd8f` and
  `990803fc0`, after the prechange release build. Fix site: the shared owner
  `wyrd_sql::tenant_conn::begin_bound` boxes the SQLx future (`49f3b601d`). An
  earlier box at `Oracle::query_sql_with_deadline` did not change the depth
  and was reverted. After the fix: release build exit 0; `mise run test:sql`
  exit 0 (295 tests).
- `mise run bench:bifrost:query-capacity` (standard, 10M rows): exit 1. Only
  `selective target` failed: 1 client p50 9.3 ms ≥ 7 ms. Prechange
  (`f0365f9ea`, same harness, recorded 07:58): 6.8 ms, PASS. Other deltas
  (p50, prechange → candidate): write 48.5 → 83.7 ms (1.08M → 834K rows/s),
  1m-aggregate 35.2 → 43.8 ms, full scan 81.6 → 95.7 ms; small-aggregate at 1
  client unchanged (12.8 → 12.9 ms); at 64 clients selective work per query
  unchanged (6.6 ms CPU/query, 1218 vs 1212 qps). Diagnosis (fresh
  diagnostician over samples, server logs, and the 49-commit range): no code
  change on these paths adds per-query work (the range removes telemetry
  ledgers and the pruning walk; the box is one allocation per transaction).
  The pattern is host contention: the harness sets `CPUQuota=800%` without
  `AllowedCPUs` on a 32-thread host while another worktree's test lanes and an
  8-core VM ran (15-minute load 16.85). Not proven; the benchmark was not
  rerun by owner decision. Raw report, samples, and server log for both runs:
  `r1-outputs/bench-{prechange,candidate}/`.
- `mise run test:bifrost:journey`: exit 0. sdk 17, observe 1, drift 2,
  forge 15, scribe 21 (1 skipped), oracle 42, otlp 11, server 27, mcp 11
  passed; 0 failed.
- `mise run gate`: not run; the owner directed the Bifrost journey lanes
  instead.
- `mise run fmt` and `mise run lints`: pass through `6d5cad774`; `cargo fmt -p wyrd-sql --check` after `49f3b601d`; `git diff --check`: pass.

Limits: the concurrent backlog test is regression proof under real thread
interleaving, not a deterministic RED; the prechange benchmark is from
`f0365f9ea`, not the exact base `05d7d7413`. No non-goal entered the diff: no
new workload, threshold, matrix, harness, metric family, or label.
