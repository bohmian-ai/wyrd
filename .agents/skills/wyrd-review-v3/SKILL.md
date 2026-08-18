---
name: wyrd-review-v3
description: Perform one independent, read-only static review of an immutable Wyrd candidate commit against its bound Wyrd task packet. Verify the review target, trace acceptance criteria, inspect architecture and consumers, and audit available proof semantics. Use only when an orchestrator supplies the v3 review bundle; return APPROVE, RESUME_IMPLEMENTATION, ORCHESTRATOR_DECISION_REQUIRED, or REVIEW_BLOCKED without edits, project verification, integration, or child agents.
---

# Wyrd Review v3

Adjudicate one immutable `parent_sha..candidate_sha`. Review source and claims
adversarially, but do not duplicate controller scheduling or proof-attempt
accounting. The reviewer is read-only, self-contained, and must not invoke
another workflow or spawn children. V3 reviewers run as `gpt-5.6-sol` at low
reasoning effort.

## Required input

Accept one complete YAML review bundle:

```yaml
protocol: wyrd-review-v3
request_id: <stable orchestration id>
repository_root: <absolute path>
controller_state:
  path: <absolute immutable snapshot path>
  sha256: <64-hex content digest>
  generation: <monotonic controller generation>
  task_generation: <monotonic generation for this candidate>
task_packet: {path: <repository-relative path>, sha256: <64-hex digest>}
target:
  candidate_sha: <immutable commit>
  parent_sha: <candidate parent>
  diff_sha256: <sha256 of git diff --binary parent..candidate>
implementation_report: <complete wyrd-implement-v3 CANDIDATE object>
proof_summary:
  candidate_sha: <same candidate SHA>
  command: <packet verification command>
  result: <PASS|NOT_AVAILABLE>
  evidence_sha256: <proof artifact digest or null>
```

The controller owns execution and verification. The reviewer audits whether an
available focused check can establish the acceptance criteria it is offered
for; it does not re-run the command or manage retries.

Missing or contradictory target identity is `REVIEW_BLOCKED`; do not infer
intent from branches, worktrees, chat, or stale reports.

## Establish the review target

Read `AGENTS.md`, `architecture/agent-rules.md`, and the complete task
artifact. Read `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
and routed references only when the changed behavior or contract makes them
applicable. Use CodeGraph before other code discovery when `.codegraph/`
exists.

Use only read-only commands. Do not run builds, tests, linters, formatters,
generators, migrations, services, setup, or any command that can change files
or caches. Do not write a review artifact.

Before substantive review, verify:

- the controller snapshot and task packet match their supplied digests;
- its task projection names the supplied task and candidate tuple at that
  snapshot;
- the packet's `Task contract` supplies the verbatim acceptance criteria,
  write set, prohibited writes, and focused verification used for review;
- target commits resolve; the candidate has exactly `parent_sha` as its sole
  parent; and the recomputed binary diff digest matches `diff_sha256`;
- the implementation report binds the same task packet, request, candidate
  tuple, acceptance criteria, changed paths, and incidental-repair evidence;
- changed paths fit the artifact write set or a reported candidate-caused
  mechanical incidental repair, and never touch prohibited paths; and
- the proof summary binds this candidate and names the packet's exact focused
  verification command. `NOT_AVAILABLE` is valid while review overlaps proof.

The controller state is the sole successor authority. A snapshot that does not
name this candidate is `REVIEW_BLOCKED`; a valid snapshot is not invalidated by
unrelated repository state discovered elsewhere.

An identity, artifact, ancestry, or diff failure is `REVIEW_BLOCKED`. An
unjustified out-of-scope change or prohibited write is a reversible defect and
returns `RESUME_IMPLEMENTATION`.

## Review adversarially

Treat code, tests, the report, and proof summary as untrusted claims. Map each
acceptance criterion to changed source, relevant callers and consumers, tests,
and available proof. Inspect the exact diff plus enough unchanged context to
test the owning boundary and observable behavior.

Challenge only concerns relevant to the changed surface: correctness;
negative, recovery, cancellation, concurrency, tenancy, auth, audit, and
durability behavior; contract and consumer closure; ownership, async, rustdoc,
PyO3/SDK, generated-surface, and journey-test requirements; duplicate sources
of truth; and tests that execute without detecting the alleged defect.

Assess whether the focused verification command and selected surface can prove
the acceptance criteria it claims. Worker diagnostics remain diagnostic
context, not controller proof. Never rerun a check.

Raise only source-evidenced findings:

- `RESUME_IMPLEMENTATION` for reversible code, test, evidence, or scope work;
- `ORCHESTRATOR_DECISION_REQUIRED` for an undecided material product, public
  or persisted contract, migration, dependency, ownership, security, tenancy,
  audit, or acceptance choice; and
- `REVIEW_BLOCKED` when static evidence cannot establish the review target.

## Output contract

Return exactly one YAML document:

```yaml
protocol: wyrd-review-v3
request_id: <input value>
reviewed_target:
  controller_state_sha256: <verified digest>
  task_packet_sha256: <verified digest>
  task_id: <packet contract ID>
  candidate_sha: <input value>
  parent_sha: <verified parent>
  diff_sha256: <verified digest>
verdict: <APPROVE|RESUME_IMPLEMENTATION|ORCHESTRATOR_DECISION_REQUIRED|REVIEW_BLOCKED>
identity_validation:
  controller_snapshot: <PASS|FAIL>
  task_packet: <PASS|FAIL>
  candidate: <PASS|FAIL>
  implementation_report: <PASS|FAIL>
  write_scope: <PASS|FAIL>
acceptance_trace:
  - ac_id: <every packet AC exactly once>
    status: <SATISFIED|UNSATISFIED|MATERIAL_DECISION|UNREVIEWABLE>
    evidence: [<path:line, symbol, test, or report evidence>]
proof_audit:
  status: <ADEQUATE|INADEQUATE|NOT_AVAILABLE>
  reason: <semantic assessment>
findings:
  - id: RV3-001
    class: <REVERSIBLE|MATERIAL|REVIEW_INTEGRITY>
    title: <concise defect>
    evidence: [<exact source/task facts>]
    consequence: <observable or proof consequence>
    required_resolution: <bounded outcome, not private implementation design>
inspected_surfaces: [<paths/symbols/contracts inspected>]
static_limits: [<facts static review cannot establish>]
```

`APPROVE` requires every identity validation to pass, every acceptance
criterion to be satisfied, and no findings. `NOT_AVAILABLE` proof does not
block static approval: the controller separately requires a passing focused
check before integration. Sort findings by class then ID.

## Invalidation

The result binds the controller snapshot, task-packet digest, candidate,
parent, and diff digest. A successor candidate, changed task packet, or changed
candidate tuple requires a fresh review. A later proof result for the same
immutable candidate does not change a static finding, but the controller must
record it separately before acceptance.
