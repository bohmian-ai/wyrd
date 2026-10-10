# Data, concurrency, and durability review

## Subject and authority

- Immutable subject: `7f79fb3417db651adedac194ada8908f0a0372d7..9a8f9f7eef95f70d356c037a192b7d7b90a37f31`; focused remediation diff `f6c841d57..9a8f9f7ee`. HEAD was checked at `9a8f9f7eef95f70d356c037a192b7d7b90a37f31`.
- Authority: `AGENTS.md` §§2, 6, 9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` (durability, query memory, Forge recovery); approved `changes/active/verification-closeout/spec.md`; original `tasks/TASK-003-r4-canonical-support-desk-closeout.md`; prior `review/task-003-r4-canonical-closeout/verdict.md` and `TASK-003-R1-canonical-closeout.md`. The latter's implementation table is evidence to check, not acceptance authority.
- Reviewed boundary: Forge worker supervision/restart, startup recovery, plan and heartbeat ownership, SQL attempt reclaim; Oracle admission shutdown and shared memory/governor attribution; delayed-Scribe audit journey and cluster restart. I inspected the cumulative changed-surface list and the remediation diff, then traced these owners and their callers through `app/supervise.rs`, `boot/mod.rs`, `state.rs`, `resources.rs`, `forge_tasks.rs`, and the test cluster.

## Prior-finding closure and proof limits

| Prior finding | Source assessment | Result |
|---|---|---|
| FIND-TASK-003-2 Oracle shutdown | The two wakeups are enabled before each check, and Forge governed bytes no longer delay Oracle. But the new predicate omits Oracle infallible headroom, described below. | Open in a narrower form |
| FIND-TASK-003-7 Forge self-reclaim | A panic in the event loop leaves shared quiescence false; a normal loop joins its plans and heartbeats before marking true. Startup recovery executes the same work before quiescence is marked running, described below. | Open in a narrower form |
| FIND-TASK-003-6 late Scribe | `query.rs:765–817` uses delayed-last on a topology whose last node is Scribe (`cluster.rs:253–262`), asserts no ready ingest node, stages the read decision, boots Scribe, drains the Oracle, and reads the retained audit row. | Closed |

No tests were rerun in this review. The task's evidence reports the focused Oracle admission test and suite, Forge quiescence unit test, SQL reclaim test, Forge journeys, and late-Scribe journey passing. Those tests do not exercise the two states below; the Forge evidence explicitly excludes a panic with a running plan. Accepted abrupt-process-loss of unwritten Scribe outbox entries is not a defect.

## Proposed material findings

### DDATA-R2-1 — Oracle shutdown omits infallible Oracle query memory

**Classification:** INCORRECT; continuation of FIND-TASK-003-2. **Obligation:** Oracle shutdown must wait for and report all Oracle query memory after its slot returns, while excluding other roles' bytes (`bifrost-design.md` query cleanup; R1 acceptance for FIND-TASK-003-2).

**Source and reachable path:** `oracle/admission.rs:752,773,784–790` checks only `ResourceSnapshot.oracle_query_memory_used_bytes`. In `resources.rs:2678–2709`, an Oracle `MemoryPool::grow` on the infallible DataFusion path attributes only its **governed** portion to that counter; the rest goes to `infallible_headroom_bytes`. `resources.rs:3489–3510` calls this path from a real Oracle pool, and its existing test around `resources.rs:4730–4759` proves headroom can remain after a query exceeds its grant. A child `MemoryConsumer` may outlive the parent slot (the precise condition `shutdown` was changed to await). If other governed usage fills the shared cap first, the child's infallible growth can be entirely headroom: `active_queries == 0`, Oracle governed bytes `== 0`, but Oracle-owned bytes still live. `state.rs:687–700` then accepts the zero residual as a clean Oracle drain and can continue teardown while that child remains.

**Correction and proof:** Include Oracle-attributed infallible growth in the Oracle drain predicate/report through the existing governed pool owner, without counting Forge headroom or changing the shared governor. Extend the focused admission test with an Oracle child that retains solely infallible headroom after its slot returns and prove shutdown reports/waits for it, while Forge headroom remains excluded. The current global `infallible_headroom_bytes` cannot simply be added because Forge can contribute to it (`resources.rs:1913–1920`).

### DDATA-R2-2 — Startup recovery can panic before Forge marks itself running

**Classification:** INCORRECT; continuation of FIND-TASK-003-7. **Obligation:** Same-owner unexpired attempt reclaim is permitted only after the previous worker incarnation has physically stopped its plans and heartbeats.

**Source and reachable path:** `forge/worker.rs:2174` runs `start_and_drain` before creating the unwind cancellation guard or setting `quiescence.enter()` at `2186–2187`. Startup recovery calls `execute_one_recoverable_cleanup` (`2250–2255`, `2293–2330`), which calls `execute_claim`; its single-attempt pool can spawn a compaction plan (`3976–4019`, `spawn_plan_runner`), and `reconcile_prepared_attempt` spawns a detached claim heartbeat (`4606–4629`). A panic or abort in either recovery operation drops its owning future before joining that work. The shared flag remains true from construction or the preceding joined loop. `app/supervise.rs:119–180` starts a clone with the same owner, `boot/mod.rs:1250–1267` preserves the shared flag, and `drain_recoverable_work` passes `Some(self.owner)` (`2236–2240`) to `forge_tasks.rs:890–903`, which clears unexpired attempts. The event-loop unit test at `worker.rs:9150–9175` exercises `enter()` followed by panic, so it cannot catch this startup window.

**Consequence:** A recovered attempt can be taken over under the same owner while its previous heartbeat or detached plan remains live; exact-attempt SQL and table fencing limit later commits, but do not establish the required physical quiescence before reclaim.

**Correction and proof:** Make the existing worker quiescence/cancellation ownership cover startup recovery after consulting predecessor state and before recovery can spawn work. Preserve fast reclaim after a proven joined predecessor and lease-expiry recovery after an unjoined one. A focused startup-recovery panic/abort test should show that the next incarnation does not select the unexpired same-owner attempt until old work stops or its lease expires; no new persistent fence or supervisor policy is required.

## Result

**FAIL** — the late-Scribe journey is closed, but Oracle shutdown can falsely report clean with retained Oracle headroom, and Forge startup recovery can leave an unprotected same-owner reclaim window.
