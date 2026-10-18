---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-009, AC-001, AC-004, AC-006, INV-004]
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-6, FIND-TASK-001-8]
---

# Correct selected source capacity and prove partial-read recovery

Execute with `$wyrd-implement`. This task packages independently validated residual findings under the approved behavior. It is not a new design plan or permission to reopen closed scope.

## Subject, authority and prior closure

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Approved specification: `changes/active/long-horizon-verification/spec.md`, revision **13** selected by the user. Its draft-14 header does not approve that exception or add retained-upgrade scope.
- Cumulative review base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`.
- Previous candidate: `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`.
- Prior review/implementation packet: `changes/active/long-horizon-verification/review/task-001-e4b1e313a-20261017/`, including TASK-001-R1 and its appended diagnoses/evidence. Current independent ledger and verdict are `findings-validation.md` and `verdict.md` beside this task.

Findings 1, 3, 4, 5 and 7 are source-closed. Preserve the user's finding-1 amendment recorded in `f983d7b95`: first source within pinned finality, newest activity first, only the newest page each poll, accepted `ponytail:` ceiling, no continuing discovery cursor. Finding 2 concerns a different limit: eligible source executions per reference. Original finding-6 sites are fixed; only the new helper omission below remains. Most finding-8 variants are proved; only partial grouped-read failure/retry remains.

## FIND-TASK-001-2 — INCORRECT: count authority differs from actual eligible membership

### Diagnosis

REQ-009 permits exactly the ceiling of distinct eligible executions and requires overflow to produce the existing inconclusive reason before fetching that reference's payloads. REQ-002/004–006 require aligned complete selection, accepted multi-query visibility and bounded reads, not a prefix judged as complete.

The source owner is `crates/wyrd/wyrd-server/src/verification/groups.rs`: `GroupPoller::poll` at 139–147 classifies `over` from `counts` (246–273), then runs a later identity query (282–297,631–637), then asks queue `refused_submissions`. Snapshot at 320–358 trusts the earlier classification for readable membership and the overflow marker. `Ended::judge` at 749–752 acknowledges counting refused rows for over tuples. Queue acceptance/refusal authority is `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:259–265,1230–1385,1721–1743`; producer staging is `components/verification/service.rs:610–647`; runner consumes the marker at `verification/runner.rs:420–430`.

Two reachable failures share this source:

1. Enqueue input is retained before asynchronous acceptance determines a key conflict. Its distinct attempt becomes errored with attempts=0. With ceiling N=20, twenty accepted executions plus one settled conflicting attempt count as 21. The tuple is classified over and never identity-listed, so refusal filtering cannot remove the conflict. A valid twenty-member group receives false `source_execution_limit_exceeded` and omits its valid values.
2. Counts sees twenty, but another already-recorded execution becomes visible before the separate identity statement pins its cut. That later query has no ceiling tied to its returned membership; snapshot makes all twenty-one readable with no overflow marker. An already max_age-due, still-in-finality group can then read/evaluate twenty-one normally. Both queries can terminate correctly. This is an allowed visibility interleaving, not an executed reproduction or broken transport claim.

The existing stable 20/21 stories and below-ceiling conflict story do not exercise these states. Query-wide resource admission does not make a small twenty-one-member response satisfy the per-reference limit. Fixing only a runner guard would be too late to prevent payload collection.

### Selected correction outcome and boundary

Replace the separate count with one selection (user decision, 2026-10-18). Delete `counts` and the `over` classification it feeds. One identity statement per poll lists each source reference's eligible executions after refused and finality-excluded rows are removed. That statement is deduplicated, ordered, and stops at ceiling+1 per reference. The returned list is the only ceiling authority:
- ceiling+1 identities mark that reference over the limit, using the existing overflow marker and runner reason, and none of its payloads are read;
- otherwise the returned identities are exactly the snapshot's membership, which both payload statements select by exact id.

Because the decision and the membership come from one result, a refused attempt cannot consume capacity, and no execution can become visible between a count and a list. Reuse the queue's acceptance/refusal and selected-results authority. When existing read/admission bounds prevent a complete selection within the limit, defer and retry instead of pinning a prefix.

Retain exact-ID payload predicates, tenant/owner/subject/Verifier/group/finality filtering, deterministic ordering, aligned genuine nulls, selected queued results, immutable retry snapshots and the queue outstanding fence. Correct the invariant once at its source for input and outcome references. Do not add producer IO, a membership/readiness store, a cross-query snapshot service, a new limit/budget, an SDK guard or an evaluator workaround. Private query/helper/control-flow choices remain implementation-owned.

### Focused closure

- At N, exactly N accepted executions plus a settled same-tuple idempotency conflict judge exactly N accepted IDs normally. N+1 accepted yields the existing completed inconclusive reason with no overflowing-reference payload collection.
- The ceiling decision and the snapshot membership come from the same identity result. No separate count statement remains, so an execution that becomes visible after selection joins only a later snapshot. Preserve mixed under/over tuple behavior and input/outcome alignment.
- Exercise a successfully terminated but incomplete selected-payload read: no prefix is enqueued, a later complete read succeeds, and genuinely absent results remain null. This is SQL selection completeness, distinct from failed transport below.

Use existing group/query/queue owners and focused fixtures. No mandatory third all-refused journey, host-load generation, broad suite or general harness is added.

## FIND-TASK-001-8 — MISSING: partial grouped-read failure/no-enqueue/retry remains unproved

### Diagnosis

AC-006 (`spec.md:1026–1032`), original TASK-001 scenario 5 and R1 finding 8 explicitly require failed/truncated discovery/snapshot reads to yield no partial judgment and then retry. Current `grouped_reads_need_the_owner_grant` at Rust SDK `tests/integration/verify_in_real_time.rs:2800–2900` proves admission denial before source batches and successful regrant. Groups units at `verification/groups.rs:970–1250` prove timing/SQL/value mapping, not partial grouped queue behavior. Existing scheduled-query terminal tests support the shared consumer's safety, but no grouped test deliberately delivers partial data then fails and checks enqueue/cadence recovery. R1 evidence row 8e therefore overstates closure.

The relevant source is group snapshot/complete/run at `groups.rs:311–393,480–523`, using `ScheduledQueryCaller::run_with` and terminal settlement (`query/scheduled.rs:133–185,259–342,434–540`). This is a missing acceptance proof, not an independently established production terminal defect.

### Selected correction outcome and boundary

Add one focused proof through the real internal grouped polling/snapshot and queue/cadence owner: fail/truncate a discovery or snapshot statement after partial delivery, assert that the affected group's prefix is not pinned/enqueued, clear the fault, and prove a later poll creates one complete immutable snapshot. Reuse existing ScheduledQueryCaller frame/terminal facilities and server/queue fixture. Only a narrow local test-only adaptation at the actual owner is needed if injection cannot otherwise reach it; do not introduce a new parser, transport, generic harness or public fault API.

Reachability matters: `WyrdTestServer::fail_next_query_after_batch` arms a controller claimed by HTTP `query/routes.rs:299–324` and separately by BoundedQuery `query/collect.rs:288–305`. GroupPoller → `ScheduledQueryCaller::dispatch` (`query/scheduled.rs:194–212`) calls `stream_query` directly and claims neither. Arming the HTTP knob unchanged does **not** prove internal group truncation. Target the internal path without routing groups through HTTP or changing sibling fault behavior.

Keep terminal guards at their stream trust boundary and selected-payload completeness at the snapshot owner. No duplicate downstream guard is requested. One shared Rust-runtime proof suffices; do not add Python/TS restart controls or rerun whole language suites. Preserve previously complete groups if a later group's read fails; the safety assertion concerns the affected incomplete group.

## FIND-TASK-001-6 — VIOLATION: new helper omits its panic contract

AGENTS §16 and agent-rules require Panics docs on new/materially changed private test helpers whenever a panic remains possible. `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1547–1555`, `FrameAgent::submit`, documents Result errors but has an explicit upload-success assertion at 1585–1588. A refused HTTP upload panics without a Panics section.

Add the missing rejected-upload panic contract to that existing helper, preserving assertion and error behavior. Nearby `group_binding` distinguishes Errors/Panics. Source inspection closes this prose correction; no behavior test, checker, dependency or separate style round is needed. The original named rustdoc sites remain closed.

## Preserved behavior and non-goals

Keep accepted-owner query permissions/audit, exact tuple isolation, server registration authority/shared manifest validation, typed source schema, ordinary verify/submit and ownerless calls, one ScribeOutbox and accepted crash loss, ordinary Window/Drift and its narrow authority exception, fixed cadence, amended discovery bound, finality/non-refusal, serial group work, immutable history/retries, selected-result/null semantics and existing verdict-change/ordinary on_failure dispatch.

No TASK-002 schedule inputs, TASK-003 staging/pressure/progress, capacity qualification, retained-upgrade feature, continuing discovery cursor, producer lookup/write, new group table/lifecycle kind, source syntax, compatibility alias, budget, dependency, platform-specific mechanism or fault product. Do not weaken tests/gates. New material behavior or authority changes exceed this task and cannot substitute for these bounded corrections.

## Acceptance and verification

| Finding | Required observable closure |
|---|---|
| FIND-TASK-001-2 | One bounded identity selection decides the ceiling and is the snapshot membership; refused rows consume no capacity; overflow precedes payload collection; incomplete SQL selection defers and recovers |
| FIND-TASK-001-8 | Deliberately partial internal group read cannot enqueue that prefix; later poll produces one complete immutable snapshot through the real queue/cadence owner |
| FIND-TASK-001-6 | Called media helper documents its explicit panic without changing behavior |

Use Red–Green–Refactor and retain exact commands, features, nonzero selected counts, diagnoses and finding-to-source/proof evidence. Re-run any failure with WYRD_LOG before diagnosis; no synthetic load. Run Cargo-backed lanes sequentially through mise.

Select and record exact names for new seam/negative tests before execution; do not invent an empty selector. Newly named Rust cases use `mise exec -- cargo nextest run --locked -p <owning-crate> --lib|--test <actual-target> -E 'test(=<actual-name>)'`, with the repository-managed Postgres/migration wrapper when needed. Prefer the focused existing group/query/queue owner, not a new target or general harness. For existing stories actually modified, these are exact command examples, not a mandate to rerun unchanged stories:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_sampled_frames_are_bounded)"'
```

Build the existing Monty worker if required by a touched real-server journey. Run only the exact focused counterexample/retry tests and narrowest regression lanes covering the actual write set. `mise run fmt` and `mise run lints` apply to changed Rust; `mise run check:tenant-isolation` applies if SQL/query paths change. Run `mise run check:deps` only if dependency/boundary composition changes, and codegen/language/docs checks only if those surfaces actually change. No current finding requires generated or foreign-runtime API changes.

Do not require the full grouped journey set, full language suites, `mise run gate` or capacity runs for this remediation. Integrated change review owns broader verification. Return to cumulative review against the original task and all prior FIND IDs after implementation; no merge or deployment is authorized here.

## Implementation evidence

Commit `648089428`. Existing owners reused: `GroupPoller` and `BindingPoll` (selection), `QueryStreamFaultController` (the one armed fault, now applied by `ScheduledQueryCaller` as well as the public collect path), and the existing bound-server, registration-client and queue helpers.

| Finding | Implementation | Verification | Result |
|---|---|---|---|
| FIND-TASK-001-2 | `groups.rs` `BindingPoll::selection` + `Sources::identities`: one `DENSE_RANK` statement per source tuple takes the first ceiling+1 executions. Refused submissions are dropped. The same rows feed `Ended::judge`, `Ended::over` and `snapshot`. It re-runs with more places only when refused rows sit inside a capped tuple (`ponytail:` marked). Overflow is decided before any payload read. | `pg_verification_groups::refused_submissions_do_not_count_toward_the_ceiling` (ceiling 2): 2 accepted + 1 refused conflict lists exactly the 2; 3 accepted gives `exceeded_ceiling: 2` and an empty list. Unit: `only_eligible_executions_exceed_the_ceiling`, `incomplete_selections_defer`, `reads_select_only_listed_membership` | PASS |
| FIND-TASK-001-8 | `state.rs` `QueryStreamFaultController::apply`, shared by `query/collect.rs` and `query/scheduled.rs` (test-support only) | `pg_verification_groups::partial_group_reads_enqueue_nothing_until_a_complete_poll`: an armed EOF after the first batch gives `GroupPollError::Read` and the committed tick enqueues nothing. The next tick enqueues one snapshot listing both turns (members 2, no results yet). A third tick enqueues nothing. Negative control: with the scheduled `apply` removed, the same test fails because the truncated tick enqueued (`Ok([Enqueued(..)])`). | PASS |
| FIND-TASK-001-6 | `wyrd-mcp/tests/bifrost/mcp/verification.rs` `FrameAgent::submit` `# Panics` (non-success storage PUT) | Source inspection; MCP selector below | PASS |

Commands (sequential, all through mise):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::groups::tests::/)'   # 6 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all -E "test(=pg_verification_groups::partial_group_reads_enqueue_nothing_until_a_complete_poll) | test(=pg_verification_groups::refused_submissions_do_not_count_toward_the_ceiling)"'   # 2 passed
# wyrd-ai journey, -P journey: verify_in_real_time::{grouped_support_resumes, grouped_refund_changes_verdict, grouped_coding_uses_declared_sources, grouped_sampled_frames_are_bounded, grouped_conversations_recover, grouped_reviews_cover_late_work, grouped_reads_need_the_owner_grant}   # 7 passed
# wyrd-mcp --test mcp: grouped_verification_discover_act_observe, grouped_frames_over_mcp_are_bounded, ^query::   # 11 passed
# wyrd-testing --test server: query_lifecycle::   # 2 passed
mise run fmt && mise run lints && mise run check:tenant-isolation && git diff --check   # pass
```

Limits: the partial-read proof truncates the first statement of a poll (discovery). A snapshot-statement truncation takes the same `Read` path through the same `?` and is not separately armed. Non-goals stayed excluded; no generated, dependency or foreign-runtime surface changed.

Status: IMPLEMENTED, for cumulative re-review.
