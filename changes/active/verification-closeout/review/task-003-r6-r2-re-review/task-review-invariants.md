# TASK-003 R2 invariant review

## Subject and method

Base `7f79fb3417db651adedac194ada8908f0a0372d7`; candidate `e2d324a916cfff25e2d362c7d7d2ab1c6aec74d9`. I read the approved specification, original r4 task, prior R1 and R2 verdicts/findings, R2 remediation task, repository and agent rules, the full cumulative change map, the latest fix diff, and current producer/consumer source. `.codegraph/` is absent. The worktree's source remained at the candidate. This is source review; I did not rerun recorded checks.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Oracle shutdown counts all Oracle query memory and excludes Forge/Scribe memory (FIND-2) | `resources.rs:2687-2724` attributes both governed and infallible growth by `MemoryHolder::Oracle`; `:2737-2759` returns the exact split; `oracle/admission.rs:739-793` checks the Oracle-only sum after arming both notifications | `oracle_shutdown_drains_oracle_infallible_headroom` holds child headroom after slot return while Forge bytes remain live; two focused Oracle selectors recorded green | PASS |
| Forge startup may self-reclaim unexpired attempts only after prior work joined (FIND-7) | `forge/worker.rs:2172-2208` atomically captures predecessor state and marks the new invocation live before `start_and_drain`, with child stop-token drop guard already installed; startup passes that captured option to both reclaim and Prepared claim paths; SQL `forge_tasks.rs:439-466,897-910` permits same-owner unexpired rows only with that option | Focused quiescence unit, Prepared startup interruption journey, and three SQL selectors recorded green | PASS |
| Interrupted Forge startup must not advertise ready or renew its own unexpired Prepared claim | `worker.rs:2126-2153,2233-2291` gates readiness on completed recovery; `claim_prepared_for_reconciliation` requires expiry when `previous_owner=None`; an abort leaves quiescence false and cancels the child token | `interrupted_startup_waits_for_its_prepared_lease` aborts at claim gate, checks the lease remains unchanged and readiness stays false, then expires lease and checks recovery; recorded green | PASS |
| Clean Forge stop retains fast self-reclaim and periodic expiry recovery | `worker.rs:2195-2208` marks joined after successful startup cancellation or joined event loop; `:2490-2543` cancels and joins loop plans/heartbeats before return; `:2583-2604` uses expiry-only reclaim during ordinary loop | Quiescence unit and previous-owner Postgres selector recorded green | PASS |
| FIND-8 and earlier documentation/correlation/role-test corrections remain closed | `worker.rs:715-719` documents the tuple field; earlier `card_uid` design passages, role `ON_ERROR_STOP`, fixture and example rustdoc remain present | `fmt`, `lints`, `docs:check`, roles lane and prior journey evidence recorded green at their respective rounds | PASS |
| Exact gateway deployment identity, late Scribe staging, and shared outbox remain intact | Three support-desk examples still compare gateway provider/model; `server/query.rs` stages audit before Scribe boots; latest fix diff does not alter gateway/outbox sources | Prior three SDK journeys, late-Scribe journey, and original gate recorded green | PASS |
| No extra governor, persistent Forge fence, CardRef alias, or unrelated behavior | Latest fix diff is limited to resource attribution, admission proof, Forge worker/SQL and focused tests plus evidence; cumulative review found no extra sink or identity path | Source/diff inspection | PASS |

## State trace and prior findings

The governor receives every Oracle pool reservation through `MemoryHolder::Oracle` (the query view and its child pools share that holder). Fallible growth contributes to `oracle_query_memory_used_bytes`; infallible growth divides into governed bytes and `oracle_infallible_headroom_bytes` under the same governor lock. Release subtracts the same `GovernedMemoryCharge` split. The snapshot exposes both, and Oracle shutdown sums them for completion and residual reporting. Forge's holder never modifies either Oracle counter. The new child-reservation test exercises the previously missing slot-returned, headroom-only state.

The Forge supervisor retains one worker identity and clones it on restart. `ForgeLoopQuiescence::enter` uses a single atomic swap to capture whether its predecessor joined and to mark the new invocation active before recovery can claim or spawn work. `start_and_drain` forwards the captured option to both same-owner SQL paths; the ordinary event loop continues to reclaim only expired Running/Claimed attempts. The child cancellation drop guard covers startup and loop execution. A startup error or task abort skips `joined`; a successful event loop first cancels and joins its retained plans and heartbeats. The Prepared takeover retains the attempt UUID, making the newly added SQL same-owner predicate necessary rather than a duplicate guard.

Prior FIND-1, -3, -4, -5, -6 remain closed on current source; FIND-2, -7, -8 close in this candidate. No new material invariant finding emerged.

## Verification limits

The new Forge interruption journey aborts before reconciliation starts a heartbeat or a plan runner. The source places the drop guard before either can spawn, but this specific journey does not observe a live child after abort. Its two-second unchanged-lease check also uses elapsed time rather than a synchronized number of recovery passes. These are proof limits, not an observed behavioral contradiction: the producer-to-consumer path uses the same cancellation token and guarded previous-owner policy throughout startup. Recorded checks are accepted only for the paths they exercised; I did not independently execute them.

**Overall: PASS.**
