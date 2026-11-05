# FIX_REQUIRED — TASK-004-REV-02

The committed implementation does not satisfy the approved conventional
Observe task. Existing data components and shared loaders are reused, but the
canvas shell/composition and required interactions are incomplete, conventional
SSR is missing, and metric/evidence correctness and mandatory boundary gates
remain open. Passing data-flow tests does not establish task acceptance.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `4a76331113864f9bddc43d3cc8df01080a696937`.
- Candidate: `f6aeb83a77179ac4450110671df6f564adc1914a`.
- Reviewed range: `git diff 4a76331113864f9bddc43d3cc8df01080a696937...f6aeb83a77179ac4450110671df6f564adc1914a` (48 files).
- Approved spec: `changes/active/wyrd-ui-main-integration/spec.md`, revision 18.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-02-live-observe-workflow.md` (TASK-004-REV-02).
- Scope/navigation/reviewer selection: [subject.md](subject.md).

This is the first independent acceptance review of this task's full range.
No prior acceptance/closure rows carry forward. Retired TASK-004 R1 remains
inherited scope/evidence; superseded assignments and the conversation are not
two failed remediation reviews. A convergence assessment is therefore not due.
The implementation source and HEAD remained unchanged; the only added files
are reports and this remediation packet in the new review directory.

## Independent coverage and results

| Role | Coverage and report | Result |
|---|---|---|
| behavior-rev | Full task acceptance, six reference boards, inherited R1, shared UI invariants; [task-review-behavior.md](task-review-behavior.md) | FAIL |
| maintainer-rev | Frontend composition/loaders and predicate ownership/reuse; [maintainer-review.md](maintainer-review.md) | FAIL |
| repo-rev | All changed tiers/languages, nested rules, mandatory declarations/docs/instrumentation; [standards-review.md](standards-review.md) | FAIL |
| domain-rev-security | Predicate grammar, permission/tenant/schema disclosure and projections; [domain-review-security.md](domain-review-security.md) | FAIL |
| system-rev | Dev startup/drain/failure and BFF cancellation/retained completion paths; [system-review.md](system-review.md) | FAIL |
| fresh ponytail-rev | Independent source/caller, materiality, correction burden and coverage validation; [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

No separate invariant specialist was selected: behavior-rev covered UI state,
with system/security specialists covering their boundaries. Every selected
report is present. [claims-comparison.md](claims-comparison.md) records routing;
B-02/B-05/MAINT-01 share one host correction. No follow-up reviewer was needed:
no unresolved source conflict or untraced path remained, and the validator
introduced no new finding requiring separate validation. Agreement was not used
as proof; the fresh validator inspected source independently.

## Validated finding ledger

The complete evidence, materiality, correction burden, selected correction and
closure proof for each retained issue are in
[findings-validation.md](findings-validation.md). This table preserves its IDs
and disposition; no rejected or independently unvalidated proposals are added.

| ID | Sources | Status/classification | Required correction boundary |
|---|---|---|---|
| FIND-TASK-004-REV-02-1 | B-01 | CONFIRMED / MISSING | Approved shell/six board compositions; reusable app TimePicker; existing shared panels and synchronized series selection |
| FIND-TASK-004-REV-02-2 | B-02, B-05, MAINT-01 | REVISED / VIOLATION, INCORRECT | Conventional server-load/direct components, one URL/range/live generation and preserved held rows |
| FIND-TASK-004-REV-02-3 | B-03 | CONFIRMED / INCORRECT | Typed recorded metric identity, no implicit metric summation/folding, correct mixed-scalar exact summaries |
| FIND-TASK-004-REV-02-4 | B-04 | CONFIRMED / MISSING, INCORRECT | Inspectable retained metric structures with proven lossless nested-count projection |
| FIND-TASK-004-REV-02-5 | SYS-01 | CONFIRMED / MISSING | Keep healthy services inspectable after post-readiness setup failure |
| FIND-TASK-004-REV-02-6 | SYS-02 | REVISED / INCORRECT | Actual canonical request trace/Run/execution membership; ambient counts cannot prove completeness |
| FIND-TASK-004-REV-02-7 | RS-01 | CONFIRMED / VIOLATION | Authored native Python declaration for new predicate method |
| FIND-TASK-004-REV-02-8 | RS-02 | CONFIRMED / VIOLATION | Required error rustdoc on two changed fallible helpers |
| FIND-TASK-004-REV-02-9 | RS-03 | CONFIRMED / VIOLATION | Scrubbed instrumentation on new predicate handler |
| FIND-TASK-004-REV-02-10 | SECURITY-01 | CONFIRMED / VIOLATION | Configured PermissionCheck for exact scoped/payload decisions |

The metric plot actually sums duplicate instants and folds overflow; the
implementation's description of single-point retention is not the source
behavior. No current privilege escalation is claimed for finding 10: current
RBAC is equivalent, but the configured authorization boundary is mandatory.
No evidence establishes that every component was unused or why the implementor
missed the task. The findings concern observable/source-proven failure, not
attribution of intent. The user's reusable-control/left-rail/SSR priorities
reinforce existing approved obligations; no new human addition received a
retroactive FIND ID.

## Reconciled acceptance matrix

| Requirement/constraint | Implementation and verification evidence | Result |
|---|---|---|
| REQ-007/015–017/020/021/039; AC-008/010/011/013/022: agreed shell, Main/Spans/TraceDrawer/TimePicker/Logs/Metrics and inspected components | Behavior source/board matrix; existing V4–V6 checks data but omit required compositions and visual comparisons; human acceptance pending/rejected | FAIL: finding 1 |
| REQ-030 use case 1; REQ-039: conventional SSR, common range/predicate/live state and URL restoration | Observe server load returns predicate only; effects own data; only list slides and brush stays renderer-local. No initial-HTML proof | FAIL: finding 2 |
| REQ-017/039; AC-008/018: compatible truthful metric series, exact values and retained structures | Shared planner/plot/JSON source traced; existing unsafe scalar checks and small histogram fixtures do not prove mixed/dimensional/nested behavior | FAIL: findings 3–4 |
| REQ-038/AC-021: canonical current-SDK environment and complete request evidence | Pin/wheel/public-wait/export/app-first-drain paths inspected and prior V1/V2/V9 results recorded; setup and membership negative paths fail source audit | FAIL overall: findings 5–6; preserve successful owners |
| REQ-040/AC-023: grammar, bounds, permission-first schema visibility and first-class projections | Two fresh parser tests pass; full public integration assertions inspected; SDK transport, tenant lookup and disclosure sequencing implemented | FAIL mandatory closure: findings 7–10; preserve implemented behavior |
| REQ-015/016/022/023/028; AC-017/018: roots vs spans, exact authorized evidence, attribution/nulls, logs controls/detail/correlation | Source and existing focused assertions reviewed; fresh V3/V7 pass at their assertion scope | PASS for inspected existing behavior; affected host/shared paths must retain it during correction |
| Chart query inspection exact SQL/typed binds, bounded rerun/cancel/terminal/refusal | Existing actions and Node executor source inspected, recorded V4; source implements cancellation though no new real-browser cancel proof claimed | PASS for implemented path; evidence limitation explicit |
| INV-001–007: existing owners, portable hosts, credential/tenant boundaries, no new durable/query authority | Cross-tier standards/domain/system coverage; existing panel/load/parser/executor reuse confirmed | FAIL where host/permission/stub gates above apply; other inspected constraints preserved |
| Non-goals: no Ask, full Query workbench, new wire/schema/transport/provider/credential/lifecycle authority | No contrary material evidence in selected coverage | PASS within inspected range; remain excluded |

## Verification limits and handoff

Fresh checks passed: focused Observe V3 (one test), shared Observe V7 (one
selected test, 13 unselected), two exact parser unit tests, Python lints/typecheck
and whitespace diff check. Exact commands/results are in [verification.md](verification.md)
and the specialist reports. Full production-BFF browser journeys, all SDK
journeys, the default development replay and visual acceptance were not rerun.
Prior execution evidence remains implementation-recorded evidence, not a fresh
review result; its green status cannot override the source-proven gaps.

A bounded independent comprehension pass read the completed remediation with
the original task/spec. It found the correction boundaries, invariants and
focused proof clear, with no remaining material ambiguity. The example path was
corrected to repository-root `examples/ui-app/`. Its one questioned command was
resolved using `mise tasks info fmt:check`: the hidden repository task exists
and runs `cargo fmt --all -- --check`. No new approval or design decision was
introduced.

Only reports/remediation were written. No implementation, commit, merge, push,
deployment or human visual acceptance occurred. No material spec change is
required by the validated corrections. Required lossless metric projection must
be proven with existing SQL; a demonstrated need for a new wire boundary uses
the original task's stop condition rather than silently changing the contract.

**Verdict: FIX_REQUIRED.** Execute the self-contained
[TASK-004-REV-02-R1-approved-observe-ui.md](TASK-004-REV-02-R1-approved-observe-ui.md)
through `$wyrd-implement` (plus `$wyrd-ui`), then review finding closure, latest
fix diff and affected consumers with this original cumulative context. No broad
journey aggregate or default 3,000-request replay is required in that remediation;
those run at integrated change review. PASS and the specified separate human
visual checkpoints remain the task completion gate.
