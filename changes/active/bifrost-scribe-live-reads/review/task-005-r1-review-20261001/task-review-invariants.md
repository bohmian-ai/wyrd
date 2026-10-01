# TASK-005 R1 independent invariant review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65`. Original task: `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` (revision 14). Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md` (revision 24). This review treats the prior verdict and R1 task as hypotheses and includes the whole cumulative range. The user has waived `mise run gate` for this task review; it is not a finding here. No source was modified.

## Producer-to-consumer paths checked

- Scribe's `StagingAssembler` owns ready and outstanding members. `ScribeStagingRuntime::{register_member,take_claim,take_residue,settle}` now publishes `assembly.backlog()` before releasing the same mutex (`staging_runtime.rs:221-249,310-321,395-410,778-788`); restoration publishes before startup returns (`:442-490`). `StagingBacklog::publish` sets the four pod gauges (`assembly.rs:715-743`). Persistence workers share this runtime. The new concurrent test ends with all four gauges equal to empty owner state (`staging_runtime.rs:1343-1438`). Its interleaving is probabilistic, but the lock ordering itself closes the earlier race.
- A durable staged member persists across `WyrdTestCluster` abrupt replacement. The new server journey scrapes the production recorder and compares it to the replacement owner, reads 48 acknowledged rows, checks replay inserts zero rows, publishes, and checks zero gauges and committed row count (`wyrd-testing/tests/bifrost/scribe/telemetry.rs:250-420`). The separate fresh-recorder `ScribeStagingRuntime::restore` test proves the replacement emits the restored gauges rather than inheriting the test process's earlier values (`staging_runtime.rs:1707-1805`).
- Scribe inserted-row counting is at successful shard insertion (`shards.rs:3996-4043`); ACK remains an attempt histogram. Oracle's retained `oracle_query_duration_seconds` is emitted at query completion and remains the HPA family (`oracle/mod.rs:430,761`); Gate's opening and stream terminal are distinct. Forge's settlement result is propagated to telemetry, with durable-state reads retained where the worker cannot know the committed result (`forge/worker.rs`, including the changed settlement and attempt closure paths). Shared storage's telemetry mirror was removed in favor of the cache/request owners. The focused journey evidence in the task and `r1-outputs/` covers these paths.
- `wyrd_sql::tenant_conn::begin_bound` now boxes the existing SQLx `begin_with` future (`tenant_conn.rs:101-125`), changing the future's compile shape but not the transaction statement, error mapping, or tenant binding. The recorded release build and SQL lane pass after this change. The separate dashboard draft is a documentation-only, unrelated commit in the range.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| AC 1: correlated write, streamed query, and Forge traces cover terminal work with one failure reason | Gate edge logging, Oracle stream instrumentation, Forge task/commit spans in cumulative diff | Scribe, published/remote Oracle, Forge journey captures; task trace IDs and parentage | PASS |
| AC 2: new Scribe rows and restored staged backlog reflect owners; replay does not add new rows | `shards.rs` insertion counter; `assembly.rs` backlog; `staging_runtime.rs` locked publication | Hot-path, fresh-recorder restore, concurrent transition, and abrupt-restart journeys | PASS |
| AC 2: waiting/active, Degraded/Success, query/HPA, storage, and closed Forge families retain their meanings | Oracle/Gate/Scribe/storage/Forge owner changes and architecture telemetry contract | Focused scenarios and whole Bifrost journey exit 0 | PASS |
| AC 3: no telemetry mirror, pruning walk, or routine Forge settlement read; ACK, durable, admission, query, and tenant paths preserved | Cumulative removals and retained uncertain-settlement reads; SQL `begin_bound` retains transaction mapping | Focused regression journeys; 147 Bifrost journey tests pass; SQL 295 tests and release build pass | PASS |
| AC 4 and REQ-008/AC-010: standard benchmark valid and numeric targets met | Existing benchmark and unmodified target; candidate report in `r1-outputs/bench-candidate/report.md` | Standard run exits 1: selective one-client p50 9.3 ms versus required <7 ms (prechange sample 6.8 ms at `f0365f9ea`). Shared-host contention is plausible but unproved; no quiet rerun | **FAIL** |
| AC 5: six dashboard questions have real samples, owner/client facts, trace parentage, and inventory | Original task's R1 dashboard section, lines 499 onward | Raw production capture logs and focused journey assertions; 105-family inventory and 18 removed families | PASS |
| Original Scenario 1 and R1: abrupt replacement exposes real staged backlog and later clears it | `telemetry::staged_backlog_survives_abrupt_restart` | Scraped owner match `[1,3464,1790876177,0]` before/after; zero after publication; fresh-recorder restore proof | PASS |
| R1: module import/signature cleanup | Top imports in changed Rust modules | Source inspection; recorded `mise run fmt` and `mise run lints` before the later SQL-only commit | PASS |
| Task non-goals: no extra product, dashboard, or unrelated scope | `changes/backlog/bifrost-operations-dashboard/spec.md` added by `1f1cbcf5f` within the range; its own text says it is a future draft | Cumulative `git log` and `git diff --name-only` | **FAIL** |
| Verification: whole Bifrost owner journeys | Unchanged journey lanes | `mise run test:bifrost:journey` exit 0, 147 passed, 0 failed | PASS |
| Broad gate | User explicitly defers it to another branch for this review | N/A under override | PASS |

## Prior finding closure

| Prior finding | Assessment |
| --- | --- |
| FIND-TASK-005-1 | Closed by publishing each mutated snapshot under the assembler mutex; final-state concurrency proof is present. |
| FIND-TASK-005-2 | Closed by the named abrupt-restart journey and fresh-recorder restore proof. |
| FIND-TASK-005-3 | Closed by the appended six-question samples, independent facts, trace IDs, and inventory. |
| FIND-TASK-005-4 | Partially closed: benchmark and whole journeys ran, but benchmark failed a required numeric target. The user waived the gate for this review. |
| FIND-TASK-005-5 | Closed by module-top imports and bare signature names. |

## Proposed findings

### INV-R1-1 — VIOLATION: benchmark target remains unproven

**Obligation:** Original task AC 4, approved spec REQ-008 and AC-010, and R1 closure for FIND-TASK-005-4 require the existing standard benchmark to meet required targets or diagnose a missed target before completion. **Location:** `changes/active/bifrost-scribe-live-reads/review/task-005-review-20261001/r1-outputs/bench-candidate/report.md:7-15`; R1 implementation evidence at `TASK-005-R1-close-telemetry-proof.md:65-79`. **Producer to consumer:** the public query workload produces client latency samples; the benchmark evaluates the one-client selective p50. It reports 9.3 ms, above the <7 ms target, and exits 1. The earlier 6.8 ms report is from `f0365f9ea`, not the exact task base. Other query/write paths also slowed, but source review did not establish whether the cause is code or shared-host contention. **Consequence:** the approved measured-capacity invariant cannot be certified for this cumulative candidate. **Required testable correction:** run the unchanged standard benchmark on a quiet, equivalent 8-CPU/16-GiB node with its raw report retained; if the selective target still fails, diagnose and correct its actual owner without changing the target or workload, then rerun. A failed run plus an unproved environmental explanation cannot close this finding.

### INV-R1-2 — DRIFT: unrelated dashboard specification entered the candidate

**Obligation:** The TASK-005 range is a telemetry cleanup and the user explicitly excluded `1f1cbcf5f` from the earlier review. **Location:** `changes/backlog/bifrost-operations-dashboard/spec.md:1-224`, added by `1f1cbcf5f` between base and current candidate. **Evidence:** `git log --oneline 05d7d7413..1fc68f3b7` and cumulative diff include the draft. It proposes a future platform-plane health endpoint and UI dashboard, while its own opening says it is a backlog draft and authorizes no implementation. **Consequence:** accepting this candidate as TASK-005 would also accept an unrelated product specification outside the approved task and earlier exclusion. **Required testable correction:** remove that draft from the reviewed candidate range or review it as its own separately authorized change; the TASK-005 base-to-candidate diff must not include it. No telemetry implementation depends on this draft.

**Overall: FAIL.** The two proposed findings are independent: one is unmet measured acceptance, and one is unrelated scope in the cumulative range. I did not find a remaining R1 telemetry-owner invariant defect. The standard benchmark remains the only red runtime proof in the supplied evidence; the user-waived gate is not charged against the candidate.
