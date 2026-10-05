# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Prior review/remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Source was inspected with commit-scoped `git show`/`git diff`; the later checked-out HEAD was not treated as the candidate.

## Review findings

### Critical

- `SYS-001` — [`crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:175`](../../../../../../crates/vala/vala-bifrost-redux/src/forge/scheduler.rs) and [`crates/wyrd/wyrd-server/src/app/supervise.rs:123`](../../../../../../crates/wyrd/wyrd-server/src/app/supervise.rs): the new restart supervisor catches a failed or panicked maintenance-scheduler task without cancelling process shutdown, but `Forge::run` revokes/resigns its leader term only after the normal `tokio::join!` return, while `ForgeRunGuard::drop` only clears the process-local `running` flag. A panic or dropped scheduler future after leadership acquisition therefore leaves `ForgeLeadership.held` containing an unrevoked term whose cancellation token is a child of the still-live process token; `ForgeLeadership::term` at `leadership.rs:425` consequently continues accepting notify, pull, and report calls, including after PostgreSQL expires the row and elects a successor, until a replacement scheduler eventually heartbeats. This reopens the exact concurrent/stale-leader behavior `FIND-TASK-001-1` required the remediation to make impossible and makes the added “scheduler failure costs only maintenance on this pod” recovery unsafe. Make the scheduler's RAII lifetime own local term revocation: every exit or unwind must synchronously clear/revoke the held term before the restarting wrapper can back off, while the normal path may still perform the best-effort asynchronous SQL resignation. Prove it with a production-topology test that acquires a term, forces the scheduler task to fail or panic, and asserts during the restart backoff that the old replica rejects notify/pull/report with `FenceLost`, a standby can take over, and the restarted scheduler can later contend normally.

### Important

No additional material findings.

### Suggestions

No optional improvements; the review is limited to required resilience behavior and reachable regressions.

## Deployed-path and failure-path evidence

| Path | Candidate behavior | System assessment |
|---|---|---|
| Leader renewal during slow promotion/maintenance | `Forge::run` drives renewal, promotion/debt sweeping, and maintenance as independent `tokio::join!` branches; `ForgeLeadership::heartbeat` bounds renewal by the local term deadline and revokes on refusal, SQL error, or timeout. | Correct for ordinary dependency stalls: slow promotion or maintenance no longer delays renewal, and revoked consumers observe the term token at their next durable boundary. |
| Leader task failure and in-pod restart | `BoundServer::run` wraps `maintenance_scheduler` in `restarting_worker`; task errors and panics back off and rebuild without cancelling the shared server token. `Forge::run` calls `leadership.resign()` only after its three branches return normally; `ForgeRunGuard::drop` does not touch leadership. | **Failed (`SYS-001`)**: the component restart boundary does not own the leader authority it must terminate before replacement. |
| PostgreSQL unavailable to Forge worker loop | `ForgeWorker::answered` treats only unanswered coordination reads (`PoolTimedOut` or SQL I/O) as database unavailability, retracts readiness, waits on admitted progress or 250 ms while idle, and retries without ending the loop. Other settlement/reconciliation failures remain worker-fatal. | Correctly isolates a transient database outage from the shared server while preserving fail-closed handling for uncertain durable state. The focused integration evidence records the old behavior as RED and the corrected path as PASS. |
| Worker failure and in-pod restart | Dedicated and embedded Forge workers use `restarting_worker`; each loop's readiness guard clears readiness, the loop cancels and joins its own plan runners/claim heartbeats, then a replacement reuses the boot-composed worker identity and runs startup recovery. | Correctly keeps the API/metrics process alive and makes Forge temporarily unready. The real-server journey injects a fatal lease-release failure, observes public reads throughout, readiness recovery, and subsequent compaction by the replacement. |
| Shutdown after leader dispatch but before episode start | `release_claim_at_shutdown` removes dispatch bookkeeping once, closes the durable row as cancelled, and reports `NotStarted` only when that close owned the row. | Correct: no fair claimant can acquire the dispatched row, pending leader commits remain due, and lease recovery covers a close/report failure. |
| Prepared cleanup refusal and replay | `CleanupRetained` bypasses generic retry/terminal settlement; the exact attempt and cursor remain prepared, `reconcile_claimed_prepared` returns a paced nonterminal result, and the worker later re-proves and replays the same candidate. | Correct: crash/refusal cannot lose ownership or authorize a second identity, and the recorded integration journey exercises refusal, retained identity, zero deletion, release, and successful replay. |
| Destructive cleanup during active reads | `ExclusiveTableAuthority` borrows the live tenant transaction; expiry preparation/commit and object deletion require that capability, and Oracle acquisition takes the conflicting shared authority. Orphan deletion is capped by the lease TTL and leaves the prepared operation open on timeout for idempotent re-proof. | Correct under the approved bound: readers either commit before exclusive acquisition and refuse destruction, or wait until the authority transaction ends; a timed-out external effect remains uncertain and is not falsely settled. |
| Leader stream dropped during Analytical execution | `LeaderStreamOwners::drop` destroys `AdmittedQueryGuard` before `ActiveReadClaim`; the inline `AnalyticalGraphLifecycle::drop` marks draining, cancels, closes exchanges, drops participant grants, and aborts local drivers before claim release starts. The residual spawned reclaimer owns only the capacity envelope. | Correct under the recorded revocation decision: no consumer receives later data, followers are synchronously revoked, local drivers are aborted, and only consumerless remote I/O may finish after release. The follow-up journey records and asserts graph-revoked before claim-release-started, including an unread stream. |
| External object-store stall while holding maintenance authority | Expired cleanup and orphan GC wrap authority-held deletion in `timeout(lease_ttl, ...)`; expiry catalog commits have their own bounded uncertain-completion handling. Durable prepared evidence remains open on timeout. | Correctly bounds reader obstruction without claiming a successful effect; replay rechecks current protection and identity. A late store acceptance is safe under the approved model because the submitted path was freshly proven outside the catalog cut, `file_list`, and every active/possible/uncertain operation; Forge output paths and identities are globally unique and cannot later be repurposed into a new cut. A reader admitted after authority surrender therefore cannot acquire a cut that names that candidate, while the retained prepared row prevents Forge from treating the unknown delete as complete. |
| Health/readiness and shared-process isolation | Worker and scheduler readiness guards clear on every loop exit; worker database backoff clears readiness; the API remains supervised independently; global resource-health poison remains process-terminal. | Worker and ordinary dependency isolation are sound. Scheduler readiness clears, but readiness alone does not revoke its still-routable leader capability, which is the `SYS-001` failure. |

## Affected capabilities

`SYS-001` affects Forge leader notification, pull, report, volatile scheduling, failover, and any worker receiving a stale dispatch; it does not require taking HTTP, public gRPC, Scribe, or Oracle offline. The correction boundary is the local scheduler/leader-term owner, not the process supervisor or downstream worker handlers.

## Proof assessment

- Credible focused proof exists for revocation during a deliberately slow promotion/maintenance pass, worker database backoff, dispatch shutdown closure, retained cleanup replay, reader/destructive-authority ordering, and Analytical graph-before-claim release.
- `failed_worker_restarts_while_the_api_serves` is a production-topology worker-restart journey and covers shared-server availability plus readiness recovery.
- `hung_cleanup_delete_surrenders_table_authority_at_the_lease_bound` and `hung_orphan_delete_surrenders_table_authority_at_the_lease_bound` directly prove that a suspended delete is classified uncertain, advances no cursor/terminal state, retains its exact prepared identity, and releases the row lock so Oracle can acquire shared authority. They do not emulate a remote store completing after the Rust request future is dropped; operational integrity of that late-acceptance case rests on the source-enforced non-reference/non-reuse invariants above, not on cancellation of the transport request.
- `restarting_worker_rebuilds_after_failures` proves only the generic wrapper. Static source-count assertions prove composition, not authority cleanup.
- No test forces the actual Forge maintenance scheduler to fail or panic after acquiring leadership. Consequently, the generic green restart proof does not exercise the stale held-term path in `SYS-001`.
- Recorded broad verification includes `mise run verify:bifrost`, principals integration, focused SQL/Forge/Oracle lanes, the 16/16 release benchmark, and final `mise run gate`; these do not invalidate the source-proven missing unwind revocation.

## Open questions

None. The leader authority owner and the required correction boundary are already fixed by REQ-001, `FIND-TASK-001-1`, and the existing `ForgeLeadership`/`ForgeHeldTerm` machinery.

## Overall result

**FAIL** — one critical, bounded lifecycle regression remains: a restarted Forge scheduler does not revoke the leader term owned by the failed scheduler instance.
