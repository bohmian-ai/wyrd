# Domain Review: Stream Lifecycle, Concurrency, and Shutdown Durability

## Review Findings

### Critical

No critical findings.

### Important

- **`STREAM-LIFECYCLE-001` — INCORRECT — [`crates/shared/wyrd-client/src/bifrost/query.rs:912`](../../../../../../../crates/shared/wyrd-client/src/bifrost/query.rs#L912): settlement cancellation makes unfinished cleanup non-resumable.**
  - **Violated obligation:** `TASK-002-R3` explicitly requires settlement to remain resumable when the settlement future itself is cancelled and to become terminal only after a completed cancel/drain/status cycle (`TASK-002-R3-close-r2-review-findings.md:96-107`). Spec `REQ-056` requires dropped callers to trigger bounded cancel/drain or lifecycle settlement.
  - **Exact evidence:** `QueryResultStream::settle` replaces `Healthy` or `Broken` with `Settled` at lines 912-917 before its first await. The future can then be dropped while awaiting `cancel` (line 925), `drain_to_terminal` (line 926), or `poll_until_retired` (line 929). Because the object has already retained `Settled`, a later call returns at line 915 without finishing the interrupted work. The new test at `query.rs:1396-1436` awaits the first settlement to completion and only checks completed-call re-entry; it never cancels an in-progress settlement.
  - **Observable consequence:** a caller that times out or cancels `settle()` can retain a live server query/body with no resumable client cleanup. A later explicit settlement attempt is a no-op, leaving retirement to the server deadline instead of the promised bounded lifecycle path.
  - **Minimum testable correction:** keep the existing `QueryResultStream` as the sole owner, but retain enough in-progress phase in its existing settlement state to resume unfinished work after future cancellation and to skip lifecycle operations already completed. Transition to `Settled` only when the cycle completes. Add a deterministic test that lets cancel complete, parks the status proof, drops the first `settle()` future, invokes `settle()` again, and proves retirement completes with one cancel and no second completed cycle.

- **`STREAM-LIFECYCLE-002` — VIOLATION — [`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:34`](../../../../../../../crates/wyrd/wyrd-server/src/oracle/query_audit.rs#L34): the connection cap moves audit pressure into an unbounded tracked task set.**
  - **Violated obligation:** `architecture/bifrost-design.md:672-683` requires every Wyrd-owned task set and queue to be bounded. The domain reliability authority rejects unbounded queues. Spec `REQ-014`/`REQ-026A` requires tracked non-blocking audit commits that do not starve reader renewal and query execution; revision 8 permits a failed commit only when it is logged and counted.
  - **Exact evidence:** `OracleQueryAudit` owns a `TaskTracker` and a semaphore that limits only connection holders (lines 34-46). Every read decision and verified tripwire calls `stage` (lines 121-159), and `stage` unconditionally spawns another tracked task before it waits indefinitely for a semaphore permit (lines 63-94). No queue/task capacity or refusal branch exists. The journey at `wyrd-testing/tests/bifrost/server/audit_publication.rs:314-371` locks the audit chain head, submits twice the pool size in reads, and asserts that at least a pool-sized number remain pending; it proves the accumulation path but not any upper bound. Shutdown waits only to its deadline and explicitly leaves remaining tasks running (lines 97-104).
  - **Observable consequence:** while a tenant chain head or database commit is slow, sustained allowed reads allocate one retained event and future per request without limit. Memory and shutdown debt can therefore grow with request volume precisely while the audit path is degraded, despite the pool itself retaining spare connections.
  - **Minimum testable correction:** add a finite pending-commit admission bound to the existing `OracleQueryAudit` owner while preserving its connection-share semaphore and non-blocking call contract. When that bound is full, take the already-authorized failed-commit path immediately—log and increment `oracle_audit_commit_failures_total`—rather than blocking the query or spawning another waiter. Extend the locked-chain-head journey (or a deterministic owner test) beyond the configured capacity and assert pending tasks remain bounded, overflow is counted, reads and a spare pooled connection still succeed, and shutdown has bounded debt.

### Suggestions

No optional improvements.

## Open Questions

None. The findings above follow from reachable public/server paths and the pinned dependency behavior; they do not require a product or persistence decision.

## Prior Finding Closure

| Prior finding / adjacent obligation | Result | Evidence |
|---|---|---|
| `FIND-TASK-002-16` — direct Arrow send can outlive successful shutdown | **CLOSED** | `WriterPool` now serializes direct admission with closure under the producer-map lock, counts admitted sends through `DirectSendPermit`, and waits for zero before producer drain (`handle.rs:220-230`, `267-315`, `383-423`). The exact focused test passed. |
| `FIND-TASK-002-17` — failed healthy-drain settlement repeats; cancelled settlement remains resumable | **PARTIAL / REOPENED as `STREAM-LIFECYCLE-001`** | The completed drain-failure/status path now restores `Settled` and no longer repeats. Pre-await replacement still loses the explicitly required future-cancellation resumability. |
| Prior stream findings `FIND-TASK-002-10` through `-12` | **REMAIN CLOSED** | Producer admission remains serialized with shutdown; failed terminals require clean EOF; lazy frame decoding remains one delivered batch per poll. No candidate integration change reopened those paths. |
| TASK-003 background producer terminal refusal | **PASS** | `Task::settle_background` retains the first non-retryable sink error and `report_deferred` returns it once through the next flush/shutdown (`wyrd-queue/src/producer.rs:935-970`). The exact focused test passed. |

## Reviewed Boundary and Authority Coverage

- Immutable range: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `bbfdf35e26212b2a831bda5e31e1ef4433e41900`; candidate tree `58f5c2acfe8da01f3e4b58363f38ce761626df35`.
- Authorities read: repository `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/languages/rust-core.md`; `TESTING.md`; approved spec revision 8; original TASK-002; its R2 verdict, validation, and stream-domain report; TASK-002-R3; TASK-004; and TASK-003.
- Source/caller coverage: complete `WriterPool`/`DirectSendPermit`, `Bifrost` facade and runtime wrappers, `Producer` task/control/retry paths, complete `QueryResultStream` terminal and settlement paths, Oracle analytical graph/session/ingress/egress ownership, pinned `datafusion-distributed` routing and in-process worker-channel implementation, Oracle audit owner/callers/shutdown/runtime inspection, and the relevant unit, integration, and journey tests. Scribe/Forge integration fixes were inspected for admission, recovery, queueing, and shutdown interactions; no additional material lifecycle finding survived validation.
- CodeGraph was absent, so source and call paths were inspected directly.

## Verification Notes

- Independently passed:
  - `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::handle::tests::shutdown_waits_for_admitted_direct_write) | test(=bifrost::query::tests::failed_healthy_drain_settles_once)'` — 2/2 passed.
  - `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::background_terminal_refusal_is_reported_by_the_next_flush_once)'` — 1/1 passed.
- The passing settlement test proves completed-call idempotence only; it does not exercise cancellation of the settlement future.
- The locked-chain-head audit journey proves connection preservation and pending-task accumulation, but does not establish a task bound.
- Full `verify:bifrost`, Postgres journeys, distributed redux integration, and credentialed/cloud lanes were not rerun in this domain pass. Their recorded evidence cannot close the source-proven missing cases above.

## Immutability Check

- Before review: `HEAD=bbfdf35e26212b2a831bda5e31e1ef4433e41900`, `HEAD^{tree}=58f5c2acfe8da01f3e4b58363f38ce761626df35`.
- The worktree already contained untracked review output and unrelated `changes/active/verified-change-contract/architecture/verifier/` content. This reviewer changed only this assigned report.
- After review: candidate commit and candidate tree remain unchanged; only the assigned report was added outside the immutable candidate tree.

## Overall Verdict

**FAIL.** The direct-write shutdown race, completed failed-drain re-entry, follower cache-cycle correction, and deferred producer refusal are correct. The candidate still loses settlement work when the settlement future is cancelled and leaves the degraded Oracle audit backlog unbounded. Each issue violates an explicit cumulative lifecycle/resource obligation and has a bounded correction and proof path within an existing owner.
