# TASK-001 cumulative re-review — FIX_REQUIRED

## Immutable subject

- Worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Base and merge base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `ae9c5a6aee22b691e2dc04c805cb5d1dddfcb9bf`.
- Reviewed the full cumulative base-to-candidate range, including the original implementation and both remediations. Latest-fix navigation did not replace cumulative review.
- Authority: [original task](../../tasks/TASK-001-grouped-verification.md), user-selected approved [specification revision 13](../../spec.md), AGENTS.md and the applicable authorities recorded in the independent reports. The draft-14 header creates neither a new approval nor retained-upgrade scope.
- Prior packets: [R1](../task-001-e4b1e313a-20261017/verdict.md) and [R2](../task-001-47e0c9ab7-20261018/verdict.md), their validation ledgers, remediation tasks and appended evidence.
- Human decisions remain binding: `f983d7b95` accepts first-source finality, newest activity ordering and one newest bounded page without a continuing cursor; `368ba8e3a` makes one refusal-filtered selection the capacity and payload-membership authority. Neither changes eligible activity, Trigger end conditions or selected-result authority.
- TASK-002, TASK-003 and REQ-015 capacity qualification remain excluded.

## Independent review and follow-up

Seven fresh discovery agents independently read the cumulative subject without another reviewer's conclusions. Root orchestrated and ran verification; it did not act as a discovery reviewer or validate its own findings.

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/tenancy | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | PASS |
| Data/concurrency | [domain-review-data-concurrency.md](domain-review-data-concurrency.md) | FAIL |

A fresh [focused follow-up](followup-review.md) was required because discovery exposed a previously untraced queued-result authority transition and materially different closure assessments for capped activity and the two proof obligations. It resolved those uncertainties through source tracing. A separate fresh structured Ponytail validator then independently checked every proposal, conflicting closure claim, correction boundary and prior finding. Its [final ledger](findings-validation.md) is authoritative for this verdict; reviewer agreement alone was not treated as proof. All required reports are complete.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence and limit | Result |
|---|---|---|---|
| REQ-001/011/014: group metadata, reserved slot, authenticated owner, shared SDK/wire projections, no producer Postgres attribution IO | Observe/Target, Principal, VerificationControl, results builder, typed HTTP/MCP and SDK contracts | Three-language retained journeys, registration/OpenAPI/schema proof; fresh Rust/MCP stories | PASS |
| REQ-002/013: whole-slot/outcome syntax, exact same-site pinned source tuples, ambiguity/cycle/undeclared refusal | SourceReference, EffectiveSpecs/GroupSite, BindingProjector, server validation | Source/schema/registration proof and coding journeys | PASS |
| REQ-003/004: accepted newest-page discovery, safe duration domain, serial outstanding work and immutable retry inputs | GroupPoller, checked arithmetic, queue locks/index/leases | Fresh recovery/late-work journeys and duration unit; no cursor demanded | PASS for these mechanisms; activity defects below remain |
| REQ-002/003/005: authoritative queued outcomes, genuine nulls, later serial whole review | Coverage is checked before selected_results; visible unselected result contributes coverage | Fresh ordinary late-work story passes but does not execute authority-only transition | FAIL — FIND-1 |
| REQ-003/004/005/009: eligible complete activity controls finality, idle/max_age and coverage | Raw candidate MIN precedes refusal filtering; capped earliest identities feed Ended | Fresh stable refusal/ceiling case passes but misses old refused-first and N+2 active-tail cases | FAIL — FIND-2 |
| REQ-006/009: one selected capacity/membership authority, overflow before payload reads, exact-ID payload predicates | Sources::identities/selection, Ended::over, payload SQL | Fresh six units, refusal-at-ceiling and media cases | PASS for corrected count/list and payload boundaries |
| AC-006/R2 FIND-2: successful incomplete selected-payload read defers and later recovers | Sources::complete and snapshot(None) guard exist | Map-only unit and discovery-EOF test do not exercise the required successful-omission poll/queue sequence | FAIL — FIND-2 proof |
| REQ-006/AC-006: failed/truncated internal read never enqueues partial evidence and later retries | ScheduledQueryCaller applies shared test fault; existing terminal consumer | Fresh real poll/commit EOF-after-batch test passes, then complete snapshot and no duplicate | PASS; previous FIND-8 transport gap closed |
| REQ-008/010: non-vacuous each, overflow inconclusive, first baseline, verdict-change-only dispatch | Vala engine, runner limit handling, queue complete/dispatch | Fresh frames/refund stories; retained engine/SQL cases | PASS apart from incorrect lifecycle inputs above |
| REQ-011: owner grants, ordinary query/tool authorization, narrow declared live query | Accepted Caller/ScheduledQueryCaller, HostedTools, check Agent validator | Fresh grant-denial/regrant, coding and query-lifecycle cases; security review PASS | PASS |
| AC-003: pinned IDs precede later span publication, then live query sees the span | Live-query implementation exists; current coding bodies flush before observing pinning | No deterministic pin-before-publication sequence established | FAIL — FIND-8 proof |
| AGENTS rustdoc gate | Original omissions and FrameAgent Panics fixed; new Support::tick lacks Errors | Source inspection; Clippy does not establish prose completion | FAIL — FIND-6 |
| Public schema/declaration parity and authority prose | Source string plus four typed objects; generated/authored surfaces; design/evaluation query exception | Fresh codegen/lints; retained served OpenAPI/type negatives | PASS |
| Ownership, tenancy, portability and non-goals | Existing query/queue/evaluator/registration owners extended; exact tuple/tenant filters, one outbox | Security review, fresh deps/tenant checks, cumulative source audit | PASS; TASK-002/003 excluded |

## Validated findings

| Stable ID | Classification | Validated consequence and correction boundary |
|---|---|---|
| FIND-TASK-001-1 | INCORRECT — REVISED | A result visible before its SQL selection can be counted as covered while the snapshot pins null. Selecting that same visible result later changes no analytical time/member coverage, so no subsequent review occurs. Correct source activity/coverage using existing queue selected-result authority before claiming consumption. |
| FIND-TASK-001-2 | INCORRECT, with MISSING proof — REVISED | The earliest capped overflow prefix can falsely declare idle; a refused first row can expire a legitimate new group before filtering. Keep complete eligible lifecycle activity while retaining one bounded capacity/membership selection. Also prove successful incomplete payload deferral and later recovery through the existing owner. |
| FIND-TASK-001-6 | VIOLATION — CONFIRMED | New Support::tick omits mandatory Errors documentation for returned poll errors after committing. Add its actual error/partial-progress contract without behavior changes. |
| FIND-TASK-001-8 | MISSING — REVISED | Explicit pin-before-late-span acceptance order remains unproved. Establish one deterministic existing Rust real-server temporal case; live-query production behavior and internal EOF recovery remain closed. |

Exact locations, producer-to-consumer evidence, proposal dispositions and focused closure proofs are in [findings-validation.md](findings-validation.md). FIND-1 and FIND-2 are one coherent correction at GroupPoller/Sources and existing queue authority, not separate SDK/evaluator/runner defenses. No new FIND ID or specification decision is required. Rejected presentation preferences create no remediation work.

## Prior-finding closure

| ID | Disposition |
|---|---|
| FIND-1 | Original watermark/hour defect closed; cursor superseded by human decision. Retained only for the authority-only late-result transition. |
| FIND-2 | Physical skip, broad payload reads, separate count/list and refused-at-capacity defects closed. Eligible lifecycle activity and successful-incomplete proof remain. |
| FIND-3 | CLOSED — checked timing and maximum-duration behavior. |
| FIND-4 | CLOSED — source schema/runtime/generated parity. |
| FIND-5 | CLOSED — accepted-owner grants and audit. |
| FIND-6 | Original sites and FrameAgent Panics CLOSED; only new tick Errors omission remains. |
| FIND-7 | CLOSED — active design/evaluation narrow query exception agrees with implementation. |
| FIND-8 | Registration, grants, media and actual internal truncation/retry portions CLOSED; only pin-before-late-span proof remains. |

## Independent verification

All Cargo lanes ran sequentially, through mise. Every Postgres run used:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <focused-command>'
```

Commands independently completed in this round:

```sh
mise run build:monty-worker
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::groups::tests::groups_end_count_and_order_by_their_rows) | test(=verification::groups::tests::only_eligible_executions_exceed_the_ceiling) | test(=verification::groups::tests::largest_accepted_durations_judge_without_overflow) | test(=verification::groups::tests::reads_select_only_listed_membership) | test(=verification::groups::tests::incomplete_selections_defer) | test(=verification::groups::tests::values_unwrap_inputs_and_outcomes)'
# The following three commands and the server command used the Postgres wrapper above.
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all -E 'test(=pg_verification_groups::partial_group_reads_enqueue_nothing_until_a_complete_poll) | test(=pg_verification_groups::refused_submissions_do_not_count_toward_the_ceiling)'
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::grouped_support_resumes) | test(=verify_in_real_time::grouped_refund_changes_verdict) | test(=verify_in_real_time::grouped_coding_uses_declared_sources) | test(=verify_in_real_time::grouped_sampled_frames_are_bounded) | test(=verify_in_real_time::grouped_conversations_recover) | test(=verify_in_real_time::grouped_reviews_cover_late_work) | test(=verify_in_real_time::grouped_reads_need_the_owner_grant)'
mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/^query::/) | test(=verification::pg_tests::grouped_frames_over_mcp_are_bounded) | test(=verification::pg_tests::grouped_verification_discover_act_observe)'
mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=query_lifecycle::grpc_lifecycle_controls_are_tenant_scoped_and_audited) | test(=query_lifecycle::denied_describe_is_audited_before_admission)'
mise run fmt:check
mise run lints
mise run codegen:check
mise run check:tenant-isolation
mise run check:deps
git diff --check
```

Results: 6 group units, 2 new server cases, 7 Rust grouped journeys, 11 MCP/query cases and 2 query-lifecycle cases passed. Migration setup tests passed for every environment invocation. Both lints commands passed; deps included its inline-source gate. `fmt:check` preserved the immutable source instead of running the rewriting formatter.

The first Rust journey invocation used unqualified names and selected zero/error 4. Nextest list identified the exact module-qualified names; the corrected invocation above selected all seven and passed with WYRD_LOG. There was no runtime test failure to diagnose and no synthetic load. Python/TS runtime lanes, served OpenAPI, docs and full journey aggregates were not rerun this round; prior evidence is retained and source-checked, not represented as fresh execution. Static allowed interleavings and unexercised proof variants remain limits despite green checks.

Only the new review directory was written. Candidate and tracked source remained unchanged; no commit, merge, push or deployment was performed.

## Decision

**FIX_REQUIRED.** Route [TASK-001-R3-source-activity-and-proof.md](TASK-001-R3-source-activity-and-proof.md) to `$wyrd-implement`. Re-review the resulting cumulative immutable candidate against the original task and all retained findings.
