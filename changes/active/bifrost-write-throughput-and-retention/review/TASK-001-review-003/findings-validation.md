# TASK-001 R2 independent remediation validation

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Cumulative base: `66f44aee193077ef3565aa76140a942d8eb593f9`.
- Prior candidate: `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`.
- Authority/review-record commit: `11e7aed813693bc2312fab313e49e9602d8f48ca`.
- Candidate: `e1f22abb06737ce63360117e590a1b40e0a1307a`.
- Contract: SPEC-bifrost-write-throughput-and-retention revision 2, original
  TASK-001 and implemented TASK-001-R2 in review-002.

Inspected `git diff bf438a8d7 e1f22abb0`, the runtime increment
`git diff 11e7aed81 e1f22abb0`, cumulative context and prior source-backed
validation. The first new commit records the already approved authority overlay
and prior review artifacts; it changes no executable behavior. No rebasing or
changed patch correspondence was found. Source stayed at the candidate.

This fresh `ponytail-rev` independently traces prior finding closure, changed
callers, actual assertions, the execution record and mandatory evidence. Root
orchestrates. Root AGENTS, the skill, maintainer style, testing/workflow and
relevant Bifrost receipt/recovery authority govern this review. CodeGraph preceded
source tracing. No nested AGENTS applies to the changed Vala/test directories;
SQL implementation is unchanged.

The prior primary, custody-invariant specialist, SQL/concurrency/lifecycle
specialist and independent validation remain applicable for unchanged boundaries.
One focused remediation reviewer suffices for these two bounded corrections;
no new production abstraction, SQL/schema, SDK projection or separate specialist
boundary appears. A separate performance-evidence reviewer checked the checkpoint
obligation. The maintainer subsequently explicitly directed “ignore the benchmark”
and “its not required right now”, deferring that obligation for this review.
No new blocking finding is retained and no new code defect is proposed.

## FIND-TASK-001-1 — CLOSED

REQ-002 requires the mirror to cease acknowledging after the backing receipt is
removed. `scribe/shards.rs:3713` now skips every batch in `spent_batch_ids` when
populating `CommittedBatches`.

The replacement of the former `accepted_under`/`conflicted` exclusions is sound:
the actual SQL resolution branches insert both classes into `spent_batch_ids`
before the successful population call (`shards.rs:4270–4342`). `AlreadyCommitted`
also enters the set; new `Committed`/`Absent` results do not. Thus B's duplicate
attempt gets the existing correct ACK but never installs A's receipt in B's
mirror. An already cached locally accepted identity remains untouched. A's
cleaner still forgets its local identity before DELETE commits. No ingress,
receipt SQL, mirror lookup/storage or conflict/version protocol changed.

Actual assertions:

- `scribe::shards::tests::record_committed_skips_spent_retries_and_keeps_cached_acceptances`
  (`shards.rs:7533`) invokes the real population method with fresh, spent and
  previously cached identities. It requires exact match for fresh/local cached
  entries and absence of a newly learned spent retry.
- `lifecycle::a_running_duplicate_pod_accepts_anew_after_the_winning_receipt_is_deleted`
  (`lifecycle.rs:873`) uses two running mixed pods and the same credential/batch.
  It checks B's first retry adds no rows, ages A's receipt, observes actual
  background deletion, retries on still-running B, requires a new receipt owner
  different from A, publishes and checks exactly two copies of the original rows.
  It neither restarts B nor calls the SQL deletion helper to create closure.
- The earlier losing-duplicate restart test remains unchanged, preserving its
  different accepted-only crash-recovery assertion.

These assertions falsify the original stale-mirror failure. Recorded focused
GREEN and the closing owner/journey lanes provide credible executable proof.
The expected cost is that retries learned through durable `AlreadyCommitted`
arbitration, including accepting-pod retries after FIFO eviction, do not repopulate
the mirror and use that existing WAL/SQL path again. No new cache lifetime or
distributed invalidation mechanism is introduced.

## FIND-TASK-001-2 — CLOSED

REQ-008 gates fresh admission on initial cleanup; REQ-009 preserves recovered
reads/service readiness. `ScribeImpl::replay_wal_async` no longer awaits
`sweep_and_report` after successful custody recovery (`scribe/mod.rs:2715`).
Its staging/WAL recovery, errors and failure readiness remain intact. Success
now sets recovery readiness immediately after the existing recovery result.

Independent caller trace: `ScribeImpl::build` still constructs one cleaner and
calls `start`; the existing loop immediately executes its first sweep before
waiting (`receipt_cleaner.rs:262`). Default `CleanupHealth.covered` is None,
and unchanged admission refuses fresh reservations until a whole successful
sweep publishes current coverage. No added post-recovery sweep or alternate
readiness state is needed. Existing shutdown, wake/retry and freshness owners
remain in place.

`lifecycle::a_restart_serves_reads_while_its_first_receipt_sweep_is_blocked`
(`lifecycle.rs:1352`) drives the production-shaped process restart and real
PostgreSQL row lock. It publishes rows, waits for a real persisted handoff,
terminates while the receipt is recent, ages it with the node stopped, holds
`FOR UPDATE`, and restarts on retained roots. It requires restart completion
within 60 seconds and observes the receipt DELETE in a Lock wait through
`pg_stat_activity`. With the lock held it reads the exact accepted rows and
checks fresh append's exact `WYRD_VALA_429_INGEST_BUSY` code. After lock release
it observes deletion, successful admission and exact new rows without another
restart. The RED record includes completed WAL recovery and a waiting receipt
DELETE, establishing the intended behavioral failure rather than setup failure.

The extracted `await_handed_off` helper retains the original persisted-frontier
observation and returns the same `HandedOffShard`; the old deletion helper still
uses the real SQL predicate and commit. This reuse does not relax custody proof.

## Reconciled coverage matrix

This is the current canonical matrix. Detailed unchanged source/assertion mapping
in review-001/002 is carried forward only where dependencies and authority remain
unchanged; the amended R1 visibility contract remains approved and settled.

| Existing requirement / scenario | Source, proof and applicability | Assessment |
|---|---|---|
| REQ-001 / scenario 1; unchanged durable completion | Prior enqueue/WAL/fence/visibility-before-ACK and terminal-persistence traces remain valid. R2 neither changes those operations nor adds ACK cleanup work. | PASS |
| REQ-002 / scenarios 1, 3 | Existing age/count/cutoff and local cleaner assertions plus the new real live-two-pod deletion/reacceptance journey; spent exclusion traced across all resolution branches. | PASS; FIND-TASK-001-1 closed |
| REQ-003 / scenario 1 | Approved Bifrost-only/latest-readable consumer behavior and prior real history/retry assertions remain unchanged by R2. | PASS on carried source-backed evidence |
| REQ-004 / scenario 2 | Canonical accepted-only recovery logic is unchanged; the losing-duplicate restart journey remains preserved and included in the closing lane. | PASS |
| REQ-005 / scenario 2 | Group-wide custody, contiguous atomic frontier, replay skip ordering and torn/staged recovery remain unchanged. The helper extraction preserves its real frontier observation. | PASS |
| REQ-006 / scenarios 2, 3 | Tenant/canonical deletion predicate unchanged; new live-pod and blocked-cleanup journeys observe actual worker deletion after real handoff. | PASS |
| REQ-007 / scenario 3 | One existing background loop remains sole production sweep driver; no new cleanup SQL/wait enters writer or ACK. | PASS |
| REQ-008 / scenario 3 | Default unverified health, finite completed-sweep publication and local admission remain unchanged; blocked-startup test checks no admission before cleanup and automatic resumption afterward. | PASS |
| REQ-009 / scenario 3 | Custody-only readiness plus actual locked DELETE, retained reads, exact busy refusal and post-release successful write; prior freshness and failure checks retained. | PASS; FIND-TASK-001-2 closed |
| Performance checkpoint / specification acceptance, task README and R2 | No R2 checkpoint ran. The maintainer explicitly directed “ignore the benchmark” and “its not required right now” during this review. Final CPU qualification is distinct. | Deferred by current maintainer instruction; no performance PASS claimed |

## Execution evidence and TDD

R2 records the focused unit's actual selector and 17 lifecycle, 1059 redux,
53 Scribe journey successes, format, lints, tenant isolation and diff checks.
The inspected test assertions use the required real process/Postgres boundaries.
These are committed implementation execution results, not new test execution
by this reviewer. Source inspection independently establishes the structural
corrections; it does not substitute for their runtime assertions.

TDD is not a blanket PASS for every new test. The unit test records behavioral
RED before the mirror correction, and the blocked-restart journey records
behavioral RED before removing the foreground wait. Those support test-first
implementation of the two corrections. The live-two-pod journey was added in
`da6f9d150` after the mirror correction `6af4ffe0d`; its recorded RED explicitly
restored the old predicate for that run. That is valuable mutation evidence,
not historical test-first execution of that journey. R2's assertion that all
three closure tests preceded their production change is therefore overstated.
Preserve this journey's post-implementation history separately; do not demand
code churn or historical RED to repair it. Original TASK-001's recorded
post-implementation tests/TDD violations also remain recorded and unchanged.

## Wait inventory and verification limits

- Nonaccepting-pod retries no longer get a newly cached duplicate fast path;
  they use the existing shared append WAL fsync and PostgreSQL acceptance
  transaction/visibility ACK boundary sanctioned by REQ-001–002 and R2. No new
  kind of request SQL, fsync or cleanup wait is introduced.
- Recovery removes the foreground cleanup SQL/sweep-mutex wait. Background
  discovery, manifest reads and bounded deletion remain sanctioned by REQ-006–008.
- No new object-store or Vault wait, worker, migration or durable state is added.

Earlier VALID diagnostic subset records predate R2; they cannot establish this
candidate's checkpoint. The checkpoint obligation was independently identified
and checked, then explicitly deferred by the current maintainer instruction.
This is a current scope decision, not an implementer's waiver inferred from
earlier exceptions. Preserve the unmeasured retry-cost tradeoff and previously
open hot-read regression as limits. No new cross-pod benchmark, 80 target at
task 1, or final CPU qualification is inferred. No FIND-TASK-001-3 is retained.

**Verdict: PASS under the current explicit benchmark deferral.** Both retained
runtime findings are closed with credible implementation and real-boundary
proof; unchanged requirement coverage remains valid. This is functional task
acceptance, not performance qualification or erasure of TDD process history.
No new runtime defect was found, and no remediation task is needed. No source
or test was edited.

Final immutable check: HEAD remained `e1f22abb06737ce63360117e590a1b40e0a1307a`
and tracked source/tests stayed unchanged. `git diff --check bf438a8d7 e1f22abb0`
passed. Only this new review report was written by this reviewer.

Skipped: fresh test, lint and benchmark runs by this reviewer. Risk: R2 performance
remains unmeasured under the explicit deferral; no code regression is inferred
from missing measurements.
