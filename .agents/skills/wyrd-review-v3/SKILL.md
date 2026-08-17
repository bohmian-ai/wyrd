---
name: wyrd-review-v3
description: Perform one independent, read-only static review of an exact immutable Wyrd candidate commit against its exact parent and canonical Ready-task artifact, validating artifact digest, ancestry, diff identity, write scope, acceptance traceability, architecture, tests, and recorded development evidence. Use only when an orchestrator supplies the complete v3 review request; return APPROVE, RESUME_IMPLEMENTATION, ORCHESTRATOR_DECISION_REQUIRED, or REVIEW_BLOCKED without edits, project verification, integration, or child agents.
---

# Wyrd Review v3

Adversarially adjudicate one immutable `parent_sha..candidate_sha`. This skill
is self-contained: never invoke another review or implementation skill, never
spawn children, and never mutate repository or orchestration state. The sole
reviewer must be `gpt-5.6-sol` at low reasoning effort.

## Required input

Accept exactly one YAML object with all fields present:

```yaml
protocol: wyrd-review-v3
request_id: <stable orchestration id>
repository_root: <absolute path>
controller_state:
  path: <absolute path to an immutable controller-state snapshot>
  sha256: <64-hex content address of exact snapshot bytes>
  generation: <monotonic integer generation>
  task_generation: <monotonic generation for this task projection>
  task_projection:
    task_id: <exact id>
    status: candidate
    task_revision: <integer>
    base_sha: <40-hex dispatch base>
    accepted_sha: <40-hex accepted integration head at dispatch>
    candidate_sha: <40-hex current candidate>
    parent_sha: <40-hex candidate parent>
    diff_sha256: <64-hex candidate diff digest>
task_artifact: {path: <repository-relative path>, sha256: <64-hex digest>}
task_id: <exact id>
task_revision: <integer>
task_status: Ready
accepted_sha: <40-hex commit>
base_sha: <40-hex original task dispatch base>
candidate_sha: <40-hex commit>
parent_sha: <40-hex commit; base initially or predecessor on remediation>
diff_sha256: <sha256 of git diff --binary parent..candidate>
write_set: [<repository-relative boundaries>]
prohibited_writes: [<repository-relative boundaries>]
acceptance_criteria:
  - {id: <stable AC id>, text: <verbatim criterion>}
implementation_report: <complete wyrd-implement-v3 CANDIDATE object>
authoritative_proof:
  candidate_sha: <same candidate SHA>
  passed: <true|false|null when proof is not yet available>
  attempts: <0|1>
  evidence_sha: <content-addressed proof artifact SHA or null>
  checks:
    - id: <controller authoritative check id>
      command: <exact controller-run command>
      result: <PASS|FAIL>
      lane: <isolated lane id>
predecessor_candidate: <superseded candidate SHA or null>
```

Missing, contradictory, or unresolvable identity data is `REVIEW_BLOCKED`, not
an invitation to infer intent.

## Freeze and validate the target

Read `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
the complete task artifact, applicable routed references, and
`architecture/wyrd-doctrine.mdx` for behavior or contract changes. When
`.codegraph/` exists, use CodeGraph first for code discovery.

Perform only read-only static commands. Do not run builds, tests, linters,
formatters, generators, migrations, services, setup, or commands that can
change caches or files. Do not write a review artifact.

Validate before substantive review:

- exact controller-state bytes hash to `controller_state.sha256`; its top-level
  generation and current task projection equal every supplied generation,
  status, artifact revision, base/accepted/parent/candidate/diff value;
- artifact bytes hash to `task_artifact.sha256`; task id, revision, Ready
  status, and verbatim ACs match it;
- accepted, base, candidate, and parent resolve as commits; `candidate_sha`
  has exactly one parent and it is `parent_sha`; initial work has
  `parent_sha == base_sha`, remediation has
  `parent_sha == predecessor_candidate`, and all reports bind the same tuple;
- recomputed `sha256(git diff --binary parent..candidate)` equals
  `diff_sha256` and the implementation report;
- the report identifies the same request, artifact, task, revision, accepted
  SHA, candidate, parent, predecessor, changed paths, and every AC;
- actual changed paths equal the report and fall within `write_set` or the
  implementation report's incidental repair closure, with none in
  `prohibited_writes`; every incidental path must be candidate-caused,
  mechanical, bound to an exact repository diagnostic, and add no behavior,
  dependency, owner, public or durable contract, acceptance scope, unrelated
  cleanup, or broad formatting;
- the state projection names this candidate as current; no successor is
  inferred from chat, branches, worktrees, or other hidden context;
- authoritative proof, when present, binds the same candidate, has zero or one
  attempt as permitted by controller policy, and its checks are distinct from
  the implementation report's diagnostic `development_checks`.

Identity, digest, ancestry, missing-artifact, or supersession failures produce
`REVIEW_BLOCKED`. An unjustified out-of-scope diff or any prohibited write is a
reversible implementation defect and produces `RESUME_IMPLEMENTATION`.
Do not reject a valid reported incidental repair merely because its path was
not enumerated in the original write set.

## Review adversarially

Treat code, tests, and the report as untrusted claims. Map every requirement
and AC to changed source, callers, consumers, tests, and check evidence. Inspect
the complete exact diff plus enough unchanged context to test ownership and
behavior. Challenge:

- correctness, negative/error/recovery behavior, tenancy, auth, audit,
  durability, concurrency, cancellation, cleanup, and compatibility with the
  approved contract;
- repository ownership, struct-centered Rust, earned async, full rustdoc,
  PyO3 and SDK layers, generated surfaces, and user-journey requirements;
- duplicated sources of truth, unjustified abstractions/dependencies, hidden
  scope, weakened tests, assertions that cannot catch the defect, and missing
  downstream consumer updates;
- whether controller authoritative proof is appropriate and semantically
  capable of proving its claimed AC. Worker `development_checks` are diagnostic
  context only and never count as authoritative proof. Audit; do not rerun.

Raise only source-evidenced findings. A reversible code/test/evidence/scope
defect returns `RESUME_IMPLEMENTATION`. A required material product, public or
persisted contract, migration, dependency, ownership, security, tenancy,
audit, or acceptance choice not already decided by the task returns
`ORCHESTRATOR_DECISION_REQUIRED`. Inability to establish the immutable review
target or mandatory static evidence returns `REVIEW_BLOCKED`.

## Output contract

Return exactly one YAML document. `verdict` must be one of the four values
below; never use severity as a verdict.

```yaml
protocol: wyrd-review-v3
request_id: <input value>
controller_state:
  path: <input path>
  sha256: <verified digest>
  generation: <verified generation>
  task_generation: <verified task generation>
task_artifact: {path: <input path>, sha256: <verified digest>}
task_id: <input value>
task_revision: <input value>
accepted_sha: <input value>
base_sha: <input value>
candidate_sha: <input value>
parent_sha: <verified parent>
diff_sha256: <verified digest>
verdict: <APPROVE|RESUME_IMPLEMENTATION|ORCHESTRATOR_DECISION_REQUIRED|REVIEW_BLOCKED>
identity_validation:
  controller_state: <PASS|FAIL>
  artifact: <PASS|FAIL>
  ancestry: <PASS|FAIL>
  diff_digest: <PASS|FAIL>
  report_binding: <PASS|FAIL>
  successor_status: <CURRENT|SUPERSEDED|UNKNOWN>
acceptance_trace:
  - ac_id: <every AC exactly once>
    status: <SATISFIED|UNSATISFIED|MATERIAL_DECISION|UNREVIEWABLE>
    evidence: [<path:line, symbol, test, or report evidence>]
findings:
  - id: RV3-001
    class: <REVERSIBLE|MATERIAL|REVIEW_INTEGRITY>
    title: <concise defect>
    evidence: [<exact source/task facts>]
    consequence: <observable or proof consequence>
    required_resolution: <bounded outcome, not private implementation design>
authoritative_proof_audit:
  - check_id: <reported check>
    status: <ADEQUATE|INADEQUATE|UNVERIFIABLE|NOT_AVAILABLE>
    reason: <semantic assessment>
inspected_surfaces: [<paths/symbols/contracts inspected>]
static_limits: [<facts static review cannot establish>]
```

`APPROVE` requires all identity validations PASS, successor CURRENT, every AC
SATISFIED, and no findings. Authoritative proof may be `NOT_AVAILABLE` because
the controller permits static review to overlap its isolated proof lane; review
approval never promotes diagnostics into proof and never asserts the proof
gate passed. Other verdicts require at least one matching finding and AC impact
where applicable. Sort findings by class then id and use stable IDs within this
review.

## Successor invalidation

Approval binds only the exact artifact/task/revision/accepted/base/candidate/parent/
diff-digest tuple. Any changed artifact bytes, new revision, new accepted SHA,
amend, rebase, or successor candidate immediately invalidates this review.
Review the successor from scratch; findings may inform an orchestrator-authored
remediation revision but never authorize edits or transfer approval.
