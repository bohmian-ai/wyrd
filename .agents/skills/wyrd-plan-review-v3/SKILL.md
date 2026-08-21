---
name: wyrd-plan-review-v3
description: Adversarially review an explicitly opted-in Wyrd v3 implementation plan for the shortest credible wall-clock path to accepted integrated code, balancing useful concurrency against task, proof, reconciliation, and integration overhead. Use after wyrd-plan-v3 drafts a v3 plan and before execution; do not migrate or reject legacy plans unless conversion is explicitly requested.
---

# Wyrd Plan Review v3

Review a plan as an execution contract, not as prose. Find only source-evidenced
gaps that would force implementors to rediscover a material decision, serialize
independent work, use a broad or invalid test, or make an unsafe integration.
Judge concurrency by expected time-to-integrated-acceptance, not packet count or
theoretical DAG width.
Remain read-only; do not edit, run verification, create artifacts, invoke a
workflow, or spawn children. Use the plan-review route in
`.agents/model-routing.md`.

## Required input

First inspect the caller-named plan or plan directory only far enough to find
its execution handoff. When it does not name `wyrd-implement-plan-v3`, return
the following without requiring v3 artifacts or reviewing architecture:

```yaml
protocol: wyrd-plan-review-v3
verdict: NOT_APPLICABLE
reason: <plan did not opt into wyrd-implement-plan-v3>
```

For an opted-in v3 plan, accept one YAML object:

```yaml
protocol: wyrd-plan-review-v3
repository_root: <absolute path>
source_sha: <immutable planning revision>
intent: {path: <repository-relative working-tree path>, sha256: <64-hex digest>}
plan: {path: <repository-relative working-tree path>, sha256: <64-hex digest>}
tasks:
  - {path: <repository-relative working-tree path>, sha256: <64-hex digest>}
```

Do not treat missing v3 artifacts, fields, or schema as defects in a legacy
plan. Do not migrate it unless the caller separately and explicitly requests
conversion through `$wyrd-plan-v3`.

Return `REVIEW_BLOCKED` if the paths, digests, source SHA, intent, task set, or
plan/task binding cannot be verified. Read the exact digest-bound intent, plan, and tasks from the
working tree; read production source and authorities at `source_sha`, never from
uncommitted source edits. Then read `AGENTS.md`, `architecture/agent-rules.md`, and only the
authorities, implementations, callers, consumers, tests, manifests, and `mise`
tasks needed to adjudicate a concrete concern. Use CodeGraph only when its
indexed revision equals `source_sha`; otherwise use `git show` or a detached
read-only worktree.

## Review the executable shape

Verify that every intent outcome has an owned task; material contracts have
one producer and named consumers; and dependencies represent actual committed
contract/behavior needs. Reconstruct the DAG from `depends_on`, write-set
forecasts, shared test/generated/migration surfaces, and cross-task
contracts. Require full decomposition of outcomes, owners, producer/consumer
obligations, actual dependencies, acceptance criteria, and proof without
requiring one packet per repository owner. A task must have one primary outcome
and integration owner, but may include inseparable cross-owner consumer, test,
journey, generated, or wiring closure. A separate authority or review packet
does not imply ordering.

Apply the wall-time test to scheduling and any optional packetization beyond
what authority or review integrity requires. Expose another independent track
when its expected critical-path savings exceed context, dispatch, proof, review,
reconciliation, invalidation, and integration costs. A packet count is not
concurrency: reject fixed waves, umbrella tasks, broad join tasks, phase
ordering, and unnecessary dependencies that delay wall-time-beneficial work.
Every dependency must identify the exact predecessor artifact or behavior the
successor cannot implement before integration. When planning fixes a shared
contract, require independent producers and consumers to fan out. A task
becomes runnable immediately after its last genuine direct dependency
integrates.

Reject a plan that exposes only a small parallel pair when additional owners
have substantial independent work against an already fixed contract and
separate execution is likely to shorten the critical path. Conversely, reject
unsafe fanout across a genuinely shared mutable seam. Require the smallest
coherent foundation for a shared contract, then expose only the independent
consumer tracks whose separate execution is expected to ship sooner. Do not
prescribe arbitrary packet names or an exact split without repository evidence;
state the independent outcomes and dependency defect the planner must correct.

Compare the proposed decomposition with the user's existing deliverables. For
every concurrency-motivated split, require evidence that substantial
implementation can overlap and that removed serial work is likely to exceed
context, dispatch, proof, review, reconciliation, invalidation, and integration
cost. Reject journey-per-packet, selector-per-packet, wiring-only, or tiny
mechanical splits that add DAG depth without shortening the critical path.
Reject a higher-width plan when a smaller set of end-to-end owner tracks is
likely to ship sooner. Full decomposition must cover every outcome, decision,
owner, dependency, acceptance criterion, and proof obligation; it does not
require a separate packet for every owner or separable edit. Concurrency is
subordinate to wall time: expose ready work only when separate execution is
likely to improve time to accepted integration.

For every task, verify bounded responsibility, real non-goals, exact owner/context,
concrete ACs, an appropriate implementation skill, an evidence-backed
`execution_tier`, and an escalation boundary limited to material authority
gaps. Do not demand private helper names,
incidental mechanics, or an exhaustive file list where repository-native local
adaptation can satisfy the approved outcome.

Require the exact plan handoff `wyrd-implement-plan-v3` and task skill
`wyrd-implement-v3`; reject a v3 plan that could fall back to the serial
controller.

Require YAML `acceptance_criteria` to be canonical. Prose may reference AC IDs
and evidence mapping but must not restate editable AC text.

Treat `write_set` as an evidence-backed forecast for optimistic scheduling, not
a completeness gate. Report an omitted path only when source already proves it
is a shared high-collision seam whose omission makes optimistic scheduling unsafe.
Do not require prediction of incidental imports, callers, fixtures, generated
outputs, or diagnostics-discovered repairs.

Map every AC to its smallest direct proof. Require:

- `verification.worker` to be `null` or one narrow safe diagnostic; a filtered
  Cargo compile/test signal is permitted when routed through the controller's
  heavy lane;
- `verification.candidate` to be one exact narrow `mise` command that exists
  and can exercise the claimed behavior, including focused database, Node,
  language-runtime, or integration checks when that task owns the boundary; and
- broader module, journey, or closeout checks only when they cover a distinct
  cross-boundary risk, placed after its actual dependencies or at plan closeout.

Reject vague commands, zero-test filters, overlapping per-task broad suites,
and tests that cannot detect the claimed behavior. Preserve the `AGENTS.md`
journey-test requirement for user-facing behavior; do not use fast unit tests
as its substitute.

## Return a decisive result

Return exactly one YAML document:

```yaml
protocol: wyrd-plan-review-v3
reviewed:
  source_sha: <verified SHA>
  intent: {path: <input path>, sha256: <verified digest>}
  plan: {path: <input path>, sha256: <verified digest>}
  tasks:
    - {path: <input path>, sha256: <verified digest>}
verdict: <APPROVE|REVISE_PLAN|MATERIAL_DECISION_REQUIRED|REVIEW_BLOCKED|NOT_APPLICABLE>
checks:
  task_coverage: <PASS|FAIL>
  contracts_and_owners: <PASS|FAIL>
  dependency_dag: <PASS|FAIL>
  wall_time_execution_shape: <PASS|FAIL>
  task_boundaries: <PASS|FAIL>
  verification_strategy: <PASS|FAIL>
findings:
  - id: PRV3-001
    class: <EXECUTABILITY|WALL_TIME|VERIFICATION|MATERIAL_DECISION|INTEGRITY>
    evidence: [<task/source/mise facts>]
    consequence: <what implementation or proof cannot safely establish>
    required_resolution: <bounded outcome, not private design>
inspected_surfaces: [<paths/symbols/commands>]
```

`APPROVE` requires all checks to pass and no findings. Use `REVISE_PLAN` for
bounded packet/plan repairs, `MATERIAL_DECISION_REQUIRED` only for a genuinely
unresolved product, public/durable contract, ownership, security, tenancy,
migration, or acceptance choice, and `REVIEW_BLOCKED` only when target identity
or essential source evidence cannot be established. Use `NOT_APPLICABLE` only
for a plan that did not opt into v3. Do not manufacture optional findings.
