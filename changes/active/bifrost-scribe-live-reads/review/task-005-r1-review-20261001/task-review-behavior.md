# TASK-005 R1 behavior review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65`, including the original TASK-005 implementation, independent dashboard draft, prior review, and R1 commits. The candidate was stable during this review. Original task: `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md`; approved specification: `changes/active/bifrost-scribe-live-reads/spec.md` revision 24. Governing behavior: AGENTS.md, `architecture/agent-rules.md`, `architecture/bifrost-design.md`, and the original task. The user expressly deferred `mise run gate` to another branch, so its absence is not a finding here.

## Acceptance matrix

| Requirement, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Gate write attempts and stream opening remain distinct from durable rows and terminal streams | Gate receipt counters removed; `gate/mod.rs`, `oracle/query_stream.rs` place stream terminal at client-edge lifecycle | Scribe write/retry/fault and published-query journey samples in original task R1 evidence | PASS |
| New Scribe rows exclude same-process replay; ACK remains an attempt | Insertion emits from `scribe/shards.rs`; `scribe/telemetry.rs` retains ACK histogram | Hot-path and abrupt-restart journeys compare row readback and replay delta (`+0`) | PASS |
| Staged member/byte/oldest/claim gauges reflect owner state after concurrent transitions | `scribe/staging_runtime.rs:241-248,311-319,400-408,779-786` publishes `StagingAssembler::backlog()` under its existing lock; `assembly.rs` owns ready and outstanding state | `concurrent_transitions_publish_the_final_backlog`, `restored_stage_republishes_backlog`, restart journey; thread test is interleaving dependent but the source invariant is direct | PASS |
| A replacement Scribe exposes recovered backlog, serves ACKed rows, and reaches zero after publication | `staging_runtime.rs:451-490` restores then publishes before ready; `wyrd-testing/tests/bifrost/scribe/telemetry.rs:314` drives abrupt termination, replacement, scrape, readback, replay, and flush | Exact journey and fresh-recorder restore test recorded green; production samples `[1,3464,1790876177,0]` before/after and `[0,0,0,0]` after publication | PASS |
| Waiting work and admission are distinct from active work; Degraded is not Success | `scribe/execution_lanes.rs`; `oracle/admission.rs`, `oracle/query_stream.rs` | Held queue sample 2 waiting/4 active; Degraded journey asserts distinct outcome | PASS |
| Oracle spans cover actual streamed lifetime; pruning does not scan files solely for telemetry | `oracle/query_stream.rs::polled_in_span` and `oracle/pruning.rs` cumulative diff | Local published and remote failure journey trace parentage and terminals | PASS |
| Storage has no shadow ledger; logical cached request and backend IO remain distinct | `storage/{cache,mod,telemetry}.rs` cumulative diff deletes reconciliation arrays and derives metrics at owner | Cache unit test and published-query journey; source inspection | PASS |
| Forge uses committed settlement for known result without changing durable authority or its 17 families | `forge/worker.rs` cumulative diff passes `ForgeTaskResult` into recorder, retaining durable lookup on ambiguous outcomes | Forge live-rewrite journey and closed-catalog unit test recorded green | PASS |
| Six dashboard questions have real samples, independent facts, trace correlation, and family inventory | R1 dashboard evidence in original task, lines 539 onward; runtime emitters and removed-series diff | Retained raw focused journey logs and samples; 105-family inventory, 18 removed families | PASS |
| Standard read/write benchmark remains valid and reports approved latency and throughput after cleanup; investigate material regression before closeout | Existing `bench:bifrost:query-capacity` unchanged; R1 report and raw samples preserved | **FAIL:** candidate `bench-candidate/report.md:13` records selective 1-client p50 9.3 ms against approved `<7 ms` (`spec.md:170`); prechange report has 6.8 ms. Host contention is plausible but unproved, and there is no clean rerun or causal diagnosis | FAIL: B-R1-001 |
| Whole Scribe, Oracle, and Forge journey lanes | Existing journeys and R1 restart test | `mise run test:bifrost:journey`: 147 passed, 0 failed, recorded output | PASS |
| Preserve ACK, WAL retirement, Iceberg, admission, terminal, tenant, Forge settlement, SDK; no exporter, telemetry state machine, high-cardinality labels, compatibility alias | Cumulative runtime diff, family inventory, and focused journey assertions | No contrary reachable path found in behavior trace; separate domain review covers deep invariants | PASS |
| No unrelated change enters TASK-005 candidate | `git show 1f1cbcf5f` adds `changes/backlog/bifrost-operations-dashboard/spec.md`, a 224-line draft for a future UI and platform health API | Complete base-to-candidate diff; this commit was expressly excluded from the original TASK-005 review but lies in the R1 cumulative range | FAIL: B-R1-002 |

## Prior finding closure

| Prior ID | Assessment |
| --- | --- |
| FIND-TASK-005-1 | Closed. Mutation and publication share the assembler lock at every normal ownership transition; restored snapshot publishes before readiness. The new concurrent test is nondeterministic as a RED but supports the source argument. |
| FIND-TASK-005-2 | Closed. Named abrupt-restart server journey exists and was run with production exposition and client readback; separate fresh-recorder test proves re-emission. |
| FIND-TASK-005-3 | Closed. Original task now records numeric production samples, independent facts, trace IDs and parentage, and family inventory. |
| FIND-TASK-005-4 | Partly closed. Whole Bifrost journey lanes passed and the benchmark ran, but its approved selective target failed. The user waived the broad gate for this review; no gate-only failure remains. |
| FIND-TASK-005-5 | Closed. R1 source uses module-top imports and bare changed signature types. |

## Proposed findings

### B-R1-001 — VIOLATION: benchmark acceptance remains red

**Obligation:** Original TASK-005 Acceptance Criterion 4 and Verification and Evidence require a valid standard benchmark, approved targets, and diagnosis of a material regression before closeout; current approved specification REQ-008 sets 1-client selective p50 `<7 ms` and p95/p99 `<10 ms`.

**Location/evidence:** `changes/active/bifrost-scribe-live-reads/review/task-005-review-20261001/r1-outputs/bench-candidate/report.md:5-13` reports 9.3/12.6/15.5 ms at one client, and `selective target` FAIL. The prechange report records 6.8/8.2/8.9 ms. The candidate task's R1 diagnosis attributes the difference to concurrent host work but calls that unproven; no quiet rerun is retained. The prechange report is from `f0365f9ea`, an ancestor of the base rather than the exact base, so it does not isolate the TASK-005 range.

**Observable consequence:** The candidate cannot show that selective read latency remains within the approved service target after telemetry cleanup. Client latency, p95, and p99 all missed their respective one-client limits in the candidate run. Passing journeys prove result correctness but not this performance obligation.

**Required testable correction:** Re-run the **existing** standard benchmark on a quiet, controlled 8-CPU/16-GiB host using the same configured store and keep its raw samples and report. If the target still fails, diagnose the actual changed hot path and correct it without loosening the approved thresholds or adding a benchmark harness. Compare against a valid prechange run or explicitly state the attribution limit. This is proof/diagnosis work; the present evidence does not justify a speculative code change.

### B-R1-002 — DRIFT: an independent dashboard draft is in the immutable candidate

**Obligation:** TASK-005 is telemetry cleanup and does not authorize a new UI/platform health API specification; the review skill's PASS condition requires no unrelated change in the diff.

**Location/evidence:** Commit `1f1cbcf5f` adds `changes/backlog/bifrost-operations-dashboard/spec.md:1-224`, status `draft`, describing future UI, `/platform/bifrost/health`, RBAC, and audit behavior. It is a separate backlog proposal and was explicitly excluded from the original TASK-005 review, yet is present in this cumulative candidate.

**Observable consequence:** The task candidate includes an independent product proposal with unapproved public and security decisions, so a PASS verdict would accept unrelated scope.

**Required testable correction:** Remove this unrelated draft from the TASK-005 candidate or present a new immutable candidate whose base-to-head range excludes it. Keep the dashboard proposal in its own review flow. No telemetry code change is required.

## Result

**FAIL.** The gate override is honored. The failed benchmark target and unrelated dashboard draft remain for independent validation. Other prior findings appear closed on the behavior paths inspected.
