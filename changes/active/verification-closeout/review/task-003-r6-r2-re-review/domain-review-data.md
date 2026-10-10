# Data, concurrency, and durability review — TASK-003 R2

**Result: PASS.** No material finding in this domain.

## Subject, authority, and coverage

- Immutable base `7f79fb3417db651adedac194ada8908f0a0372d7`, candidate `e2d324a916cfff25e2d362c7d7d2ab1c6aec74d9`; focused R2 diff `9a8f9f7ee..e2d324a91`. HEAD matched the candidate when inspected. I used the cumulative diff to retain the original task boundary and the focused diff to locate R2 edits.
- Authority: `AGENTS.md` resource, async, SQL, and verification rules; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` query memory and Forge lease/fence sections; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/operations/reliability-and-recovery.md`; approved `changes/active/verification-closeout/spec.md`, original `tasks/TASK-003-r4-canonical-support-desk-closeout.md`, prior R1/R2 verdict and remediation packets. Prior findings were hypotheses, not proof.
- Traced Oracle `GovernedMemoryRoot` pool `grow`/`try_grow`/`shrink` through `BifrostResourceGovernor` reserve/release/snapshot, `OracleAdmission::shutdown`, and its focused tests. Traced Forge worker construction and clone, server supervision, startup recovery, event-loop recovery, Prepared SQL claim, Claimed/Running reclaim, exact attempt and lease behavior, and the focused worker/SQL/integration tests. The SQL owner retains `OperatorPool`; R2 adds no tenant-bypass path.

## Prior finding closure

| Finding | Source assessment | Proof assessment | Result |
|---|---|---|---|
| FIND-TASK-003-2 | `resources.rs:2687–2723` records Oracle infallible headroom separately while retaining the global cap/headroom charge; `:2737–2760` returns both components from the same reservation; `:2213–2226` snapshots both. `oracle/admission.rs:738–792` enables slot and memory wakeups before checking Oracle governed plus Oracle headroom bytes, and reports that same sum. Forge/Scribe charges do not enter either Oracle counter. A poisoned ledger reads as held. | `oracle_shutdown_drains_oracle_infallible_headroom` creates an Oracle child with only headroom after its slot returns, verifies residual bytes, then frees it while Forge bytes remain. Recorded exact selector passed. | Closed |
| FIND-TASK-003-7 | `ForgeWorker::run` at `worker.rs:2175–2206` atomically reads predecessor quiescence and marks this invocation live before startup recovery. Its child stop-token drop guard covers startup and event loop; only a completed startup or joined event loop marks the flag clean. The supervisor restarts a clone of the retained worker (`boot/mod.rs:1242–1266`; `app/supervise.rs:115–177`), so the flag and owner carry forward. Startup forwards the predecessor decision to both `reclaim_expired_attempts` and `claim_prepared_for_reconciliation`; the latter's SQL at `forge_tasks.rs:439–467` selects unexpired same-owner Prepared rows only with that explicit owner, while ordinary expired rows remain eligible. The event-loop and test-only paths keep their attended same-owner behavior. | The interrupted-startup Postgres journey holds a Prepared claim, aborts startup, observes no premature renewal or readiness, expires the lease, and observes recovery. Recorded worker unit, SQL, and integration selectors passed. | Closed |
| FIND-TASK-003-8 | `worker.rs:716–719` documents the shared quiescence field. | Source inspection and recorded format check. | Closed |

## Limits

I did not rerun verification lanes. The interrupted-startup journey aborts at the Prepared claim gate before a heartbeat starts, and its no-renewal assertion observes eight recovery passes over two seconds; it does not directly test a panic with a live plan. Source shows the drop guard covers that later startup window and an unjoined predecessor forces ordinary lease-expiry recovery. This is a proof limit, not a remaining independently demonstrated defect. The global `memory_used` gauge still includes all roles' infallible headroom; R2 changes the Oracle shutdown predicate, not that telemetry contract.
