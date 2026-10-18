# TASK-001-R3 — Correct eligible source activity and close focused proof

Status: READY FOR IMPLEMENTATION through `$wyrd-implement`.

## Authority and subject

- Approved specification: `changes/active/long-horizon-verification/spec.md`, revision **13**, explicitly selected by the user.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed candidate: `ae9c5a6aee22b691e2dc04c805cb5d1dddfcb9bf`.
- Worktree/branch: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`, `change/long-horizon-verification-task-001`.
- Diagnosis authority: [validated ledger](findings-validation.md), [verdict](verdict.md), original R1/R2 packets and human amendments `f983d7b95` and `368ba8e3a`.

The first amendment accepts first-source-within-finality discovery, newest activity ordering and one newest bounded page without a continuing cursor. The second makes one refusal-filtered identity selection the sole capacity and payload-membership authority. Preserve both. They do not waive eligible first/newest activity, idle/max_age, queued selected-result authority or expressly required proof. TASK-002, TASK-003, REQ-015 and retained-schema upgrade work are excluded.

## Shared diagnosis and selected correction — FIND-1 and FIND-2

The capacity/membership correction is implemented: one bounded identity selection after refusal removal feeds the ceiling and payload IDs, and payload statements select only those IDs. The remaining source defect is that discovery, end timing and coverage can still describe different activity from the values eligible for a snapshot. Fix this once in **GroupPoller/Sources with the existing queue refusal and selected-result authorities**. Do not add SDK, evaluator, Operator or producer-side defenses.

### User decision (2026-10-18): delete the identity cap

The R2 packet's wording that the one selection "stops at ceiling+1" was the implementer's, not the user's decision, and REQ-009 bounds only payload reads ("never eagerly read unbounded payloads merely to detect overflow"). Read boundedness is judged by the REQ-015 capacity benchmark, not designed in ahead of it. The selected correction deletes code:

- **One uncapped identity statement per group.** It selects every eligible source execution ID and time of the group, refused submissions removed, and no `DENSE_RANK` cut or re-run loop. That one result is the complete lifecycle activity (first, newest, idle/max_age, finality, coverage) and the ceiling decision (more than the ceiling distinct executions marks the reference over). Payloads are still read only for listed, non-over references. Mark the identity read with a `ponytail:` comment naming its growth with group size and the benchmark as the upgrade trigger.
- **FIND-1:** drop result rows of queued executions that have no selected result yet (`VerifierExecutionQueue::selected_results`, already called by the snapshot, called once earlier instead). When the queue later selects that result, the members count rises, so existing coverage yields one new whole review.
- **Refused-first finality:** discovery bounds candidates by newest activity within finality (`MAX(at)`, not `MIN(at)`). `Ended::judge` refuses a group whose first eligible row is already past finality at the tick clock, so an old accepted first source still expires.

### FIND-TASK-001-1 — INCORRECT: already-visible outcome is covered before SQL selects it

REQ-002/003/005 and AC-003/006 require genuine null snapshots followed by a new serial whole snapshot when the selected outcome becomes available within finality.

Runner settlement (`crates/wyrd/wyrd-server/src/verification/runner.rs:474–565,708–759,942–960`) stages its result before committing SQL completion. A failed/delayed commit can leave that result visible while the source queue row still has no chosen result; settlement retries the same result without publishing a new row. In `verification/groups.rs:139–151,311–324,421–432,638–649,737–773`, raw visible result activity is counted before selected_results is consulted. The payload correctly pins null for the unselected queued result, but coverage already claims its activity. When SQL later chooses that exact visible result, through/members do not change. `GroupState::covers` (`wyrd-sql/src/queries/verifier_executions.rs:985–994`) then skips before consulting result authority. The actual selected value can remain unjudged indefinitely without unrelated activity.

**Required outcome and recommendation:** produce group activity/coverage using existing queued selected-result authority before claiming that an outcome has been consumed. An unselected or abandoned queued result must not permanently cover the eventual chosen value. Reuse `VerifierExecutionQueue::selected_results` and existing group snapshot/coverage ownership. Keep direct result authority distinct from queued absence, allow genuine input-only null snapshots, preserve old null history, and ensure the later authority transition creates exactly one subsequent whole review. Do not change settlement staging/commit order, Scribe loss semantics, lease ownership or producer IO; those accepted boundaries create the allowed state and are not the defect. No readiness tracker, receipt or second result store is needed.

### FIND-TASK-001-2 — INCORRECT: raw or capped activity changes finality and end timing

REQ-003/004/005/009 require eligible first/newest source activity to govern finality and idle/max_age, including overflowing groups. Capacity and payload membership remain bounded.

**Capped activity:** `groups.rs:248–278,571–600` returns the earliest ceiling+1 identities. Those rows feed `Ended` at `:139–144,737–773`. With ceiling=2, idle=10s, max_age=100s, finality=200s, accepted executions active at t=0,1,2,20 and poll t=21, the selected prefix says newest=2 and enqueues overflow inconclusive early. Actual newest activity20 means neither end condition is due. This can also dispatch a premature verdict-change reaction or hide later activity exclusively in discarded IDs. The documented implementation shortcut is not an approved change to the Trigger clock.

**Refused first row:** enqueue stages input before asynchronous acceptance (`components/verification/service.rs:610–647`; queue acceptance/refusal at `wyrd-sql/src/queries/verifier_executions.rs:1240–1385,1730–1743`). A conflicting key originally accepted in H can leave G's refused input at t=0. G's first accepted source arrives at61, finality60, poll62. Raw candidates MIN/HAVING (`groups.rs:201–221`) rejects G before selection can remove the refused row. Valid G never reaches snapshot selection despite its first eligible source being recent.

**Required outcome and recommendation:** make eligibility authoritative before a refused attempt can establish discovery finality, and preserve complete eligible activity for first/newest/end/coverage with the one uncapped selected identity result as sole activity, capacity and payload-ID authority (user decision above). Reuse the existing group/query/queue owners and normal SQL/Arrow facilities to convey lifecycle activity without collecting overflowing payloads or reinstating a separate capacity count. Resolve FIND-1 and these two consumers as one coherent source-authority correction. Do not merely enlarge the ceiling, add a runner guard or mark the changed timing as deferred debt. Preserve ordinary query failures as deferrals rather than false completeness. Private query layout and control flow remain implementation-owned.

### FIND-TASK-001-2 — MISSING: successful incomplete payload deferral/recovery proof

The actual completeness guard appears correct: `groups.rs:311–334,632–653` defers when a required selected input/direct-result payload is absent. But `incomplete_selections_defer` (`:1192–1252`) only calls a map predicate. The new PG partial-read case (`tests/integration/pg_verification_groups.rs:275–310`) fails discovery transport before successful payload completeness is reached. Neither exercises the R2-required successful omission → no enqueue → complete later poll sequence, which is permitted by independent Bifrost cuts.

**Recommendation:** prove the existing snapshot/poll-to-queue boundary with one successful but incomplete selected-payload response and later completion. Reuse existing group/query/queue test ownership. No extra production guard is required by this proof finding; retain genuine queued-null semantics. Do not add a public fault API, generic harness or duplicate terminal parser.

## Other bounded corrections

### FIND-TASK-001-6 — VIOLATION: tick lacks Errors rustdoc

`crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs:188–225`, `Support::tick`, returns GroupPollError after committing its cursor and preceding enqueues, but documents only Panics. AGENTS §16 and agent rules explicitly require Errors even on private fallible test helpers.

Add the actual propagated SQL/read error and commit/partial-progress contract, using the existing GroupPoller documentation. Preserve assertions, Panics and behavior. Source inspection closes this documentation correction; add no test or wrapper. Original rustdoc omissions and FrameAgent Panics remain closed.

### FIND-TASK-001-8 — MISSING: pin-before-late-span proof

AC-003 and R1 explicitly require pinned inputs first, a later flushed span second, and then a live query seeing that span while pinned IDs stay unchanged. Rust coding (`sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1770–1778`), Python (`sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:663–669`) and TS (`sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:703–707`) all publish the later span before observing the group execution. Background timing does not prove the required ordering. Current assertions prove live querying, not publication after pinning.

Establish one deterministic temporal case using the existing Rust real-server coding, queue inspection and local-provider owners: observe pinned trace IDs before publishing/flushing the later span, perform the declared query after visibility, then verify the later span and unchanged stored IDs. One shared Rust-runtime proof is sufficient; do not create all-language synchronization machinery or alter live-span production behavior. Preserve tool/query denial coverage. Actual internal EOF/no-enqueue/retry proof is closed and does not need another snapshot-statement fault variant.

## Constraints and preserved behavior

- Keep exact owner/tenant/subject/Verifier/group filters, accepted owner grants, ordinary query/tool admission and audit.
- Keep one ScribeOutbox and existing queue/leases/coverage/snapshot owners; no new durable group inventory, queue, readiness barrier, receipt, producer lookup, timer, configuration limit or dependency.
- Keep single selected (uncapped, per the user decision above) capacity/membership authority, exact-ID payload reads, N allowed/N+1 overflow, no overflowing-reference payload collection, ordered aligned values and genuine absent-result nulls.
- Keep immutable history/retry inputs, serial outstanding execution, finality, safe duration domain, first-result baseline and verdict-change-only dispatch.
- Preserve direct/queued authority distinctions and ordinary Drift/Window behavior; no source grammar, public contract, SDK durability or span-snapshot change.
- No continuing cursor, TASK-002 schedule inputs, TASK-003 staging/pressure/progress, capacity qualification, retained upgrades or optional refactor.

## Acceptance and focused closure proof

| Finding | Required focused proof |
|---|---|
| FIND-1 | Input and result visible while queued result authority is absent; due review pins null and settles; choosing that exact visible result without republishing causes one next whole actual-outcome snapshot. Old snapshot remains null; unchanged subsequent poll does not duplicate. Unchosen values never leak. Drive this allowed state directly through existing group/queue ownership rather than injecting a network commit fault. |
| FIND-2 timing | N+2 accepted executions, first N+1 idle and final one active: no enqueue before real idle/max_age; once due, existing overflow result and no over-tuple payload collection. |
| FIND-2 eligibility | Old settled refused-only G row plus recent first accepted G source yields one due normal snapshot listing only eligible IDs. An actually old accepted first source must still enforce finality. |
| FIND-2 proof | Successful selected-payload omission yields no affected-group execution; later complete response yields exactly one complete immutable snapshot. Preserve genuine-null behavior. |
| FIND-6 | Source inspection shows Errors describes returned poll errors and committed partial-progress behavior. |
| FIND-8 | Deterministically establish pin → later span flush → live query; verify returned later span and unchanged pinned trace IDs. |

Use existing fixture times/coordination clocks and focused query/queue seams; no long sleeps, synthetic load, public control product or general harness. Preserve existing at-ceiling refusal, over-limit, mixed tuple alignment, lease and transport-recovery behavior with only the narrowest regressions covering the actual write set.

Run tests through mise; Cargo lanes sequentially. Postgres uses:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <focused-command>'
```

Server seams use `-p wyrd-server --features test-support --test integration --run-ignored=all`. Rust SDK stories use `-p wyrd-ai --test integration -P journey --run-ignored=all`. For every specifically named new/changed test, record and run its exact module-qualified `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command. Confirm selection counts. Re-run any failure with WYRD_LOG before diagnosis; document the trace-supported cause and correction. No synthetic host load.

Run applicable Rust format/lints and targeted tenancy/dependency checks if their owned boundary changes. Regenerate/check contracts only if changed. Build `mise run build:monty-worker` as needed. This task does not require complete SDK journey suites or broad aggregates; change review owns those once. Append focused execution evidence and source closure for every retained ID. The later task review assesses the full cumulative candidate against the original task and all prior findings.
