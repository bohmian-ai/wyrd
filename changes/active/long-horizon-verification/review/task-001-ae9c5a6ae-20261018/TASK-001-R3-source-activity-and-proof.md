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
- **One read, no completeness guard (user decision, 2026-10-18).** The identity statement also carries each row's payload, gated per source so a source over the ceiling reads none. Membership and values come from the same read, so a payload missing beside a listed identity cannot be expressed. The separate payload statements, the completeness check, its deferral and its proof obligation are deleted.
- **PostgreSQL time (spec revision 15, approved).** Every row time the poll uses is the managed `wyrd_ingested_at`, never `recorded_at` or `ended_at`. Tests advance time through binding durations and the Postgres cursor, never a process clock, and do not shift `due.clock`.

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
| FIND-2 proof | Deleted: one read carries membership and payloads, so a successful selected-payload omission is not expressible (user decision above). Preserve genuine-null behavior. |
| FIND-6 | Source inspection shows Errors describes returned poll errors and committed partial-progress behavior. |
| FIND-8 | Deterministically establish pin → later span flush → live query; verify returned later span and unchanged pinned trace IDs. |

Use existing fixture times/coordination clocks and focused query/queue seams; no long sleeps, synthetic load, public control product or general harness. Preserve existing at-ceiling refusal, over-limit, mixed tuple alignment, lease and transport-recovery behavior with only the narrowest regressions covering the actual write set.

Run tests through mise; Cargo lanes sequentially. Postgres uses:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <focused-command>'
```

Server seams use `-p wyrd-server --features test-support --test integration --run-ignored=all`. Rust SDK stories use `-p wyrd-ai --test integration -P journey --run-ignored=all`. For every specifically named new/changed test, record and run its exact module-qualified `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command. Confirm selection counts. Re-run any failure with WYRD_LOG before diagnosis; document the trace-supported cause and correction. No synthetic host load.

Run applicable Rust format/lints and targeted tenancy/dependency checks if their owned boundary changes. Regenerate/check contracts only if changed. Build `mise run build:monty-worker` as needed. This task does not require complete SDK journey suites or broad aggregates; change review owns those once. Append focused execution evidence and source closure for every retained ID. The later task review assesses the full cumulative candidate against the original task and all prior findings.

## Implementation evidence

Existing owners reused: `GroupPoller`/`BindingPoll`, `VerifierExecutionQueue`, the `GroupSnapshot`/`GroupState` coverage owners, `ResultPayloadBuilder` and the one `ScribeOutbox`, the bound-server `pg_verification_groups` seam, and the Rust journey's local judge.

**One read.** Bifrost states that even a successful later cut "can omit or duplicate rows" (bifrost-design, Durability and visibility), so membership and values must share one statement; refusing Degraded terminals would not close a two-read gap. Per group the queue first names the group's refused submissions and selected results (`VerifierExecutionQueue::group_submissions`, one `WHERE group_id = $1` statement replacing the two by-ID lookups, with a `(data_tenant_id, group_id)` partial index). One Bifrost statement then reads every eligible row with `COUNT(DISTINCT execution_id) OVER (PARTITION BY card_uid, verifier_card_uid)` and `CASE WHEN executions <= ceiling … THEN payload END`, a window over the same scan, so the count, the over decision and the values agree with the rows. Deleted: `selection`'s Rust filtering, both payload statements, `Sources::complete`, the deferral, `Ended::over`/`eligible`, the `cutoff` field and `incomplete_selections_defer`. The snapshot is now synchronous.

**PostgreSQL time.** Every row time is `wyrd_ingested_at`. Finality needs no row filter: rows and the tick clock are both PostgreSQL time, so `Ended::judge` refuses a group once the clock or any row reaches first + finality. Tests elapse time only on PostgreSQL (`pg_sleep`) under binding durations set on the claimed `due.settings`; `due.clock` is never shifted.

**Consequence of spec revision 15.** One Scribe batch shares one ingest time, so rows ingested together tie and list by `execution_id`, not by submission order. The Rust, Python and TypeScript sampled-frames journeys asserted submission order; each now asserts the exact member set and derives each value's position from its listed ID. The unit test pins the (time, `execution_id`) order.

| Finding | Implementation | Verification | Result |
|---|---|---|---|
| FIND-1 | Unselected queued results are excluded in the one read (`NOT queued OR result_id IN selected`); a later selection raises members | `pg_verification_groups::a_visible_result_counts_once_its_settlement_selects_it`: visible result before selection → review pins `[null]`, members 1; settle review; select that exact result without republishing → one new review, members 2, `["passed"]`; next tick enqueues nothing. Negative control (authority filter neutralised): fails at members 2 ≠ 1 | PASS |
| FIND-2 eligibility | Refused submissions excluded in the one read; `judge` anchors on the first eligible row; discovery bounds `MAX(at)` | `pg_verification_groups::finality_anchors_on_the_first_eligible_row`: refused-first group judged from its later accepted turn (lists only it); old accepted group past 10s finality enqueues nothing. Negative control (refused filter neutralised): fails at 0 ≠ 1 enqueues | PASS |
| FIND-2 timing | Uncapped identity read; no rank, limit or re-run | `verification::groups::tests::one_read_selects_eligible_rows_and_gates_payloads` asserts no `LIMIT`/`RANK`, the group, owner/tuple, refused and authority filters, the ceiling gate and no `recorded_at`/`ended_at` | PASS |
| FIND-2 proof | Deleted as inexpressible: one statement carries membership and values | `verification::groups::tests::decoding_marks_tuples_over_the_ceiling`; `pg_verification_groups::refused_submissions_do_not_count_toward_the_ceiling`; `pg_verification_groups::partial_group_reads_enqueue_nothing_until_a_complete_poll` | PASS |
| FIND-6 | `Support::tick` documents `# Errors` (returned poll errors; the tick still commits its cursor and whole snapshots) | Source inspection | PASS |
| FIND-8 | The coding judge holds its first review answer (`HeldReview`) until the test has read the pinned trace ID, published and flushed the late span, and released it | `verify_in_real_time::grouped_coding_uses_declared_sources`: pinned ask carries the trace ID; live read contains `late-test-run`; evidence traces `[trace, null, null]` | PASS |
| Ordering regression | Journeys assert the members and ID-aligned values | Rust/Python/TS `grouped_sampled_frames_are_bounded` | PASS |

**Diagnosis (MCP, intermittent).** Symptom: `verification::pg_tests::grouped_verification_discover_act_observe` failed once at `verification.rs:1457`, where review `result.outcome.evidence.turns` was `Null`; the same run's `execution_ids` assertion passed and the re-run passed. Evidence, from a fresh read-only diagnostician: `get_execution` fills `result` from a Bifrost read and leaves it absent until the row is visible (`components/verification/service.rs:221–280`). The contract says "`completed` may briefly lack a visible result" (`wyrd-spec/src/verification/execution.rs:150–153`). Settlement stages the result on the outbox in the completing transaction (`runner.rs:510–563`). Cause: the test helper `group_review` returned on `completed` without waiting for `result`. Fix site: that helper now requires a visible result, the same rule as the Rust journey helper `support::group_execution`. 11/11 MCP grouped and `query::` tests pass after the fix.

Commands (Postgres-backed ones under `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && …'`):

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/^verification::groups::tests::/)'   # 5 passed
cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all -E 'test(/^pg_verification_groups::/)'   # 4 passed
cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all -E 'test(=pg_verification_groups::a_visible_result_counts_once_its_settlement_selects_it) | test(=pg_verification_groups::finality_anchors_on_the_first_eligible_row)'   # both fail under each neutralised filter; source restored
cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/^verify_in_real_time::grouped_/)'   # 7 passed
cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/grouped_/) | test(/^query::/)'   # 11 passed
cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(/^query_lifecycle::/)'   # 2 passed
cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m 'integration and not local_dev' tests/integration/test_verify_in_real_time.py -k grouped   # 5 passed (after mise run py:setup)
cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t grouped   # 5 passed (after ts:build, ts:build:testing)
mise run fmt && mise run lints && mise run check:tenant-isolation && mise run codegen:regen && mise run codegen:check && mise run py:format && mise run py:lints && mise run ts:format:check && mise run ts:lints && git diff --check   # all pass
```

Non-goals held: no continuing cursor, TASK-002/003 work, new queue, readiness tracker, receipt, configuration or dependency. `card-types.ts` changed only through `codegen:regen`, from the `GroupCoverage` rustdoc.
