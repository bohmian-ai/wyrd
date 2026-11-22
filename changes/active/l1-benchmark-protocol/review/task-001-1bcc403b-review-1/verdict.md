# TASK-001 verdict: FIX_REQUIRED

Behavioral acceptance: **FIX_REQUIRED**. Six blocking findings remain after
the maintainer approved spec revision 2, voiding all previous performance
targets for the refactored L1 cases and withdrawing FIND-TASK-001-5.
Independent review originally validated seven findings against revision 1;
this scope update is a maintainer decision, not a new code review or a fix.
TDD compliance is separately
**VIOLATED** for scenarios 1–4.

Current implementation authority is [approved spec revision 2](../../spec.md).
The immutable identities, source evidence and revision 1 coverage matrix below
remain the historical review record. The six unchanged findings carry forward;
the [revised remediation task](TASK-001-R1-l1-protocol-boundaries.md) also checks
that new L1 cases do not inherit old performance targets. No replacement
targets or new independently discovered findings were added.

## Immutable subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
- Candidate: `1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
- Reviewed diff: `git diff 2418dc78...1bcc403b`.
- Reviewed authority: SPEC-l1-benchmark-protocol revision 1 at the candidate,
  candidate blob `f5f18f8b9a9f05d86596621bf3b3c7efa5d250bf`.
- Original task: [TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md),
  candidate blob `e82e229a6030ec90b004d2554ce2f3db884436ae`.
- Checkout HEAD remained `6e158cd39407a5c25e5edad163633765d99fdad9`.
  Its sole candidate-relative change is an unrelated fence-lifecycle task
  status. Reviewed source matches candidate; review introduced reports and
  remediation only. See [subject record](subject.md).

Applicable authority: root AGENTS.md, benchmarks/AGENTS.md,
changes/spec-driven-development.md, agent_docs/{maintainer-style,rust,agent-surfaces}.md,
TESTING.md and architecture/bifrost-design.md § Durability and visibility.
The approved spec's design and case audit settle lifecycle and selection details.

## Independent review allocation

| Reviewer | Assigned boundary | Evidence |
|---|---|---|
| `behavior_rev` | Full requirements/scenarios, selection, summary/report/protocol, mise/docs, reuse and unassigned repository rules | [Primary report](task-review-behavior.md) |
| `system_rev` | Fresh subprocess/resources, setup/teardown, interruption, cleanup continuation, persistence and connected harness | [System report](task-review-system.md) |
| `invariant_rev` | Window populations, CPU/counters, drains, aggregation, histogram pooling, targets and smoke integrity | [Invariant report](task-review-invariants.md) |
| Fresh `ponytail_rev` | Independent source validation of every proposal, requirement/proof coverage and specialist sufficiency | [Validation](findings-validation.md) |

The orchestrator did not discover or validate findings. Separate repo,
maintainer and domain reviewers were omitted because existing assignments cover
the actual owner/repository/accounting/recovery boundaries; production auth,
tenant SQL capabilities, domain durability and SDK/wire behavior are unchanged.
The validator independently confirmed this allocation is sufficient.

## Revision 1 acceptance matrix and current disposition

Implementation locations below are relative to
`crates/wyrd/wyrd-testing/src/` unless qualified. C1–C3 are execution results
recorded in the original task, not fresh review passes. Full retained findings
are recorded once in [findings-validation.md](findings-validation.md).

| Existing requirement / scenario | Implementation and assertion/execution evidence | Result |
|---|---|---|
| REQ-001, AC-001 / 1 | `bin/capacity/cases.rs:428–647`, `main.rs:959–1029` assert inventory, order, collisions, profiles and stable declarations; actual mise path uses `scripts/benchmarks/l1.sh:21–26`; C1 recorded GREEN | FAIL — FIND-TASK-001-1 |
| REQ-002, AC-002 / 3 | `sweep.rs::attempt/supervise`, release server and Postgres wrapper; `tests/pg_tests.rs:720–975` checks fresh processes/projects/storage and sequential outcomes; C3 recorded GREEN | FAIL — FIND-TASK-001-2, -3 |
| REQ-003, AC-003 / 2 | `experiment.rs:2190,2602–2614,2672–2678`; `capacity.rs:1386` and `experiment.rs:6228,7337` assert stop/drain/clock behavior; C1 recorded GREEN; actual teardown and persistence paths traced by specialists | FAIL — FIND-TASK-001-2, -3, -4, -7 |
| REQ-004, AC-003 / 2 | `Repetition::aggregate`, `Reading::pooled`, `judge_criterion`; unequal-window/target assertions at `experiment.rs:7190,7473`, recorder restore at `wyrd-benchmark/src/registry.rs:1239`; C1/C2 recorded GREEN | Original FAIL under FIND-TASK-001-5 withdrawn by revision 2; aggregate/pooling proof retained; no inherited targets to be checked in remediation |
| REQ-005, AC-004 / 4 | `summary.rs::CaseRecord/SweepRecord`, `sweep.rs::keep_case`, child JSON boundary; persistence assertions at `sweep.rs:915`, historical guard checks at `main.rs:815–860`, `experiment.rs:7567`; C1/C3 recorded GREEN | FAIL — FIND-TASK-001-4, -6; protocol guards supported |
| REQ-006, AC-001/004 / 1,4 | `summary.rs::render_sweep/render_case`; `main.rs:655,869` asserts saved order/units/headlines, invalid/unstarted cells, diagnostic profiles and capture notes; C1/C3 recorded GREEN | FAIL — FIND-TASK-001-6 remains; FIND-TASK-001-5 withdrawn by revision 2 |
| REQ-007, AC-002 / 3 | `test:benchmark:l1` drives two actual release-server/client cases through sweep and repository Postgres; C3 records one pass in 152 s; numeric integrity judgment bypass traced independently | FAIL — FIND-TASK-001-7 |
| AC-005 / 5 | Approved spec and original task explicitly disclose execution hold; no full-size result claimed | PASS for hold/disclosure; feasibility UNVERIFIED |

## Findings and remediation

Retained IDs: **FIND-TASK-001-1, -2, -3, -4, -6 and -7**.
**FIND-TASK-001-5 is WITHDRAWN** under the maintainer-approved revision 2;
its original source validation remains recorded as history, not remediation.
Discovery proposals
SYS-1 and SYS-4 were consolidated under phase ownership (finding 2), preserving
both closure cases. No new validator finding required another discovery pass.
No prior remediation round or closure evidence applies to this first review.

The [bounded remediation task](TASK-001-R1-l1-protocol-boundaries.md) covers the
listing build, phase ownership, cleanup deadline, persistence timing,
full snapshot and smoke integrity corrections, plus the approved removal of
legacy target expectations. Review does not
authorize implementation, commit, merge, push or deployment.

## Verification and process limits

Implementation records C1: 181 passed / 3 skipped, C2: GREEN, C3: one connected
test passed in 152 s, plus fmt/lints and diff checks GREEN. Review inspected
actual assertions and source, rather than treating those summaries as proof
of the broken branches. The review's focused five-test attempt was interrupted
with SIGINT/exit 130 during `wyrd-server` dependency compilation before tests
started; its exact command is in the invariant report. No fresh runtime pass or
test failure is claimed. The reviewed range passes `git diff --check`.

Named-test exact-selector execution history is generally absent from C1/C2,
which are package-wide results. Remediation must record exact focused commands
and actual outcomes for its closure checks. Full-size execution and live
profiler capture were not rerun; AC-005 explicitly permits the feasibility hold.

TDD: **VIOLATED**, as recorded for scenarios 1–4: production preceded the focused
tests and no expected-behavior RED was recorded. Later verification cannot
repair that history. Preserve it; do not manufacture historical RED or churn
correct production code to simulate compliance.
