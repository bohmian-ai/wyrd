---
name: wyrd-review-v3
description: Perform one risk-routed, independent, read-only static review of an immutable Wyrd candidate commit against its bound Wyrd task packet. Use when the controller routes contract, shared-seam, durable, security, tenancy, audit, migration, cross-owner, or suspicious-evidence risk; do not review every isolated passing leaf by default.
---

# Wyrd Review v3

Adjudicate one immutable `parent_sha..candidate_sha`. Review adversarially but
do not duplicate scheduling or proof execution. Remain read-only, invoke no
workflow or children, and use `.agents/model-routing.md`.

This is the sole substantive candidate review for its immutable review
identity. Never review a mutable working tree or act as a pre-commit/
"pre-seal" pass. Verifying the candidate, parent, packet, implementation
report, and diff digest is identity validation within this review, not a
separate defect-review stage.

## Required input

Accept one YAML review bundle:

```yaml
protocol: wyrd-review-v3
request_id: <stable orchestration id>
repository_root: <absolute path>
task_packet: {path: <repository-relative path>, sha256: <64-hex digest>}
terminal_remediation: null
# Or:
# terminal_remediation:
#   review: {path: <terminal review artifact>, sha256: <digest>}
#   reviewed_target_sha: <candidate parent>
#   finding_ids: [<assigned reversible finding IDs>]
target:
  candidate_sha: <immutable commit>
  parent_sha: <candidate parent>
  diff_sha256: <sha256 of git diff --binary parent..candidate>
implementation_report: <complete wyrd-implement-v3 CANDIDATE object>
proof_attestations:
  - candidate_sha: <same candidate SHA>
    command: <packet candidate command, or one assigned terminal verification command>
    assertion_ids: [<criteria this command proves>]
    working_directory: <absolute candidate worktree>
    result: <PASS|CODE_FAILURE|INFRA_UNAVAILABLE|NOT_AVAILABLE>
    exit_status: <integer or null>
    started_at: <timestamp or null>
    finished_at: <timestamp or null>
    artifact: {path: <absolute bounded stdout/stderr artifact or null>, sha256: <digest or null>}
```

The immutable static-review identity is the request, task-packet digest,
terminal-remediation binding when present, candidate tuple, diff digest, and
implementation report. Proof is a separate controller-owned attestation bound
to that candidate and its asserted criteria; it may arrive after static review
without changing the static identity. Missing or contradictory identity is
`REVIEW_BLOCKED`; do not infer intent from
branches, worktrees, chat, controller state, or stale reports.

## Establish the review target

Read `AGENTS.md`, `architecture/agent-rules.md`, and the task. Read design,
doctrine, and routed references only when the changed behavior makes them
applicable. Use CodeGraph only when its indexed revision equals
`candidate_sha`; otherwise use `git show` or a detached read-only worktree.

Use read-only commands. Do not run builds, tests, linters, formatters,
generators, migrations, services, setup, or commands that change caches.

Verify:

- task packet and supplied digest;
- candidate existence, sole parent, and binary diff digest;
- implementation report binding to request, packet, candidate, ACs, actual
  changed paths, forecast expansions, and remediation predecessor when present;
- terminal-review artifact, target parent, assigned finding IDs, owners,
  assertions, verification, and implementation-report binding when terminal
  remediation is present;
- no changed path violates `prohibited_writes`;
- every change outside forecast `write_set` is task-local completion work rather
  than unrelated behavior or a new material contract; and
- every required proof attestation's artifact digest, candidate, command,
  assertion IDs, working directory, exit status, timestamps, and bounded
  stdout/stderr agree. Normal tasks require the packet candidate command;
  terminal remediation requires every assigned verification command.

Do not reject a candidate merely because its actual paths exceed `write_set`.
An identity or ancestry failure is `REVIEW_BLOCKED`; an unrelated change or
prohibited write is `RESUME_IMPLEMENTATION`.

## Review adversarially

Map every packet AC—or every assigned terminal acceptance assertion—to changed
source, relevant callers/consumers, tests, and proof.
Inspect enough unchanged context to challenge correctness; negative, recovery,
cancellation, concurrency, tenancy, auth, audit, durability, ownership, async,
rustdoc, PyO3/SDK, generated-surface, and journey-test requirements where
relevant. Reject duplicate truth and tests unable to detect the alleged defect.

Assess whether the selected focused command can prove its claimed ACs. Never
rerun it. `INFRA_UNAVAILABLE` is not a code finding; the controller owns bounded
same-candidate recovery. `CODE_FAILURE` prevents integration and normally
returns `RESUME_IMPLEMENTATION` with the exact diagnostic.

Raise only source-evidenced findings:

- `RESUME_IMPLEMENTATION` for reversible code, test, evidence, or scope work;
- `TASK_CONTRACT_REPAIR_REQUIRED` for a non-material invalid command, mistaken
  mechanical prohibition, missing proof seam, or other packet defect a worker
  cannot change;
- `ORCHESTRATOR_DECISION_REQUIRED` only for an undecided material product,
  public/persisted contract, dependency, ownership, security, tenancy, audit,
  migration, or acceptance choice; and
- `REVIEW_BLOCKED` when immutable target identity cannot be established.

## Output contract

Return exactly one YAML document:

```yaml
protocol: wyrd-review-v3
request_id: <input value>
reviewed_target:
  task_packet_sha256: <verified digest>
  task_id: <packet contract ID>
  candidate_sha: <input value>
  parent_sha: <verified parent>
  diff_sha256: <verified digest>
  terminal_binding: <null or exact verified implementation-report terminal_binding>
verdict: <APPROVE|RESUME_IMPLEMENTATION|TASK_CONTRACT_REPAIR_REQUIRED|ORCHESTRATOR_DECISION_REQUIRED|REVIEW_BLOCKED>
identity_validation:
  task_packet: <PASS|FAIL>
  candidate: <PASS|FAIL>
  implementation_report: <PASS|FAIL>
  prohibited_writes: <PASS|FAIL>
acceptance_trace:
  - assertion_id: <every packet AC or terminal assertion exactly once>
    status: <SATISFIED|UNSATISFIED|MATERIAL_DECISION|UNREVIEWABLE>
    evidence: [<path:line, symbol, test, or report evidence>]
proof_audit:
  status: <ADEQUATE|INADEQUATE|INFRA_UNAVAILABLE|NOT_AVAILABLE>
  reason: <semantic assessment>
findings:
  - id: RV3-001
    class: <REVERSIBLE|CONTRACT_REPAIR|MATERIAL|REVIEW_INTEGRITY>
    title: <concise defect>
    evidence: [<exact source/task facts>]
    consequence: <observable or proof consequence>
    required_resolution: <bounded outcome, not private design>
inspected_surfaces: [<paths/symbols/contracts>]
static_limits: [<facts static review cannot establish>]
```

`APPROVE` requires valid identity, satisfied ACs, and no findings. Static review
may approve with required attestations `NOT_AVAILABLE` or `INFRA_UNAVAILABLE`,
but the controller requires later `PASS` attestations bound to the same
candidate, commands, and assertion IDs before integration; those attestations
do not require repeating static review. For a risk-routed candidate, a
successor/replacement candidate, changed packet, changed parent, or changed
diff requires fresh review. Candidates the controller classifies
`NOT_REQUIRED` never invoke this skill.

For `TASK_CONTRACT_REPAIR_REQUIRED`, each finding must name the exact packet
field, repository evidence proving it defective, and a bounded correction that
does not choose new material behavior.
