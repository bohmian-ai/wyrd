# TASK-003 R2 maintainer review

## Subject and coverage

- Immutable cumulative subject: `7f79fb3417db651adedac194ada8908f0a0372d7..e2d324a916cfff25e2d362c7d7d2ab1c6aec74d9`; latest repair: `9a8f9f7ee..e2d324a91`. `HEAD` matched the candidate and source was clean during this review. `.codegraph/` is absent.
- Read `AGENTS.md` §§5, 11, 16, `architecture/agent-rules.md`, `architecture/references/languages/maintainer-style.md`, the approved spec, original TASK-003 r4, prior verdicts, and R2 task. Inspected cumulative and repair diffs; checked the materially changed symbols, nearby owners, callers, and relevant tests. The task records focused tests and checks passing; I did not rerun them.

| Surface and caller/test path | Maintainer assessment |
|---|---|
| Oracle resource governor `reserve_pool_memory_infallible`, `release_pool_memory`, `snapshot`; `OracleAdmission::oracle_memory_reserved`, `shutdown`; admission tests | Oracle headroom attribution stays in the existing governor alongside governed Oracle bytes. The snapshot field documents why shutdown includes it, and the admission helper names the sum it returns. The new test holds Forge memory while a child retains Oracle headroom, then releases the child. No second memory owner or helper hierarchy appears. |
| `ForgeWorker::run`, `start_and_drain`, `drain_recoverable_work`, `reconcile_one_prepared`, `claim_prepared`; `ForgeLoopQuiescence`; server's supervised clone/restart; worker unit and production-route tests | The existing worker remains the lifecycle owner. `enter` combines the predecessor read and current-run mark in one operation before startup; the child token's drop guard now spans startup and event loop. The reclaim policy is passed through the startup calls to the existing SQL owner. The shared tuple field and changed methods have substantive rustdoc, including the restart invariant and cancellation behavior. |
| `ForgeTasks::claim_prepared_for_reconciliation` and `reclaim_expired_attempts`; worker startup/event-loop/test caller; Postgres integration tests | The SQL method's new optional previous-owner parameter describes when an unexpired Prepared claim is eligible. The worker is the only production caller, and it chooses `None` for uncertain startup recovery while continuing its prior event-loop self-renewal. Existing Postgres tests exercise the SQL surface and the new production-route test exercises restart behavior. |
| Cumulative server/gateway, outbox, client/SDK, support-desk examples, schemas, docs, test and tooling changes | Rechecked the prior review's owner map against the cumulative diff and sampled the public `Run::invoke`, gateway UID check, three example deploy functions, and matching journeys. R2 adds no public SDK contract, generated declaration, example behavior, or new crate boundary. Prior documentation and role-script corrections remain present. |

## Material findings

None. I found no new owner split, redundant abstraction, misleading public type, or missing rustdoc with a concrete maintenance consequence in the R2 write set.

## Non-blocking calibration

- The new `oracle_shutdown_drains_oracle_infallible_headroom` test imports `MemoryConsumer` inside its function (`oracle/admission.rs:2597`), whereas `architecture/agent-rules.md` calls for imports at module top. This placement has no behavioral or public-contract consequence.
- `interrupted_startup_waits_for_its_prepared_lease` uses a fixed two-second observation period before asserting the claim timestamp stayed fixed. That is readable and the task records a red check, but a stalled worker could also leave the timestamp fixed; the test's remaining expiry/recovery assertion reduces that risk. I leave proof strength to the behavior and system reviews.

**Overall: PASS** — R2's changed owners and documentation are maintainable under the repository guide. Verification evidence is recorded, not independently rerun here.
