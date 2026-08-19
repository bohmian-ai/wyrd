---
name: wyrd-implement-v3
description: Implement one decision-complete Wyrd task packet or bounded terminal remediation in an isolated worktree, run focused diagnostic verification, and return one immutable candidate commit or a material BLOCKED result. Use only when an orchestrator supplies the packet, parent SHA, and dedicated worktree; never use for planning, integration, review, or task-status mutation.
---

# Wyrd Implement v3

Produce one reviewable commit from one immutable task packet, optionally
augmented by a bounded terminal-remediation contract. Together they are the
source of task scope, acceptance, dependencies, and focused verification. The
controller alone dispatches, verifies authoritatively, risk-routes review,
integrates, supersedes, and records execution state.

Never invoke another workflow or spawn children. Use the packet's
`execution_tier` through `.agents/model-routing.md`.

## Required input

Accept one complete YAML object:

```yaml
protocol: wyrd-implement-v3
request_id: <stable orchestration id>
repository_root: <absolute path>
task_packet:
  path: <repository-relative task packet path>
  sha256: <lowercase 64-hex digest of its exact bytes>
parent_sha: <immutable commit checked out in the worktree>
worktree_path: <absolute dedicated clean worktree path>
remediation: null
# For a replacement generation:
# remediation:
#   supersedes_candidate_sha: <rejected immutable candidate>
#   findings: [<exact proof diagnostics or review findings>]
terminal_remediation: null
# Or after terminal review:
# terminal_remediation:
#   review: {path: <terminal review artifact>, sha256: <digest>}
#   reviewed_target_sha: <current integrated parent>
#   finding_ids: [<REV IDs assigned to this worker>]
```

The task packet begins with a `Task contract` YAML block:

```yaml
id: <stable task ID>
depends_on: [<task IDs>]
write_set: [<repository-relative paths>]
prohibited_writes: [<repository-relative paths>]
execution_tier: <fast|general>
acceptance_criteria:
  - id: AC1
    text: <verbatim observable criterion>
verification:
  worker: <optional fastest safe diagnostic mise command, or null>
  candidate: <exact focused authoritative mise command>
```

Return `BLOCKED` before editing only if the request is incomplete, packet
digest differs, its contract cannot be understood after inspecting its stated
outcome and repository context, the worktree is not clean and checked out at
`parent_sha`, a necessary path overlaps a prohibition, or `parent_sha` is not
a commit. Do not require a task status, revision, manifest, or controller proof
record: controller dispatch is the authority to begin work.

Exactly one of `remediation` or `terminal_remediation` may be non-null. For
terminal remediation, verify the review artifact digest, target SHA, selected
finding IDs, owners, required outcomes, acceptance assertions, and verification
commands. Treat those selected findings as an additive bounded work contract;
they may authorize their named owners/paths even when the primary task packet
did not forecast them. They never authorize a new material decision.

## Establish the execution boundary

Read `AGENTS.md`, `architecture/agent-rules.md`, the complete task packet, and
the architecture/design/doctrine references relevant to its changed behavior.
Inspect the named paths, consumers, tests, manifests, and `mise` command.
Use CodeGraph only when its indexed revision matches `parent_sha`; otherwise
inspect the dedicated worktree directly.

Use `write_set` as a coordination forecast and `prohibited_writes` as the only
path-level hard boundary. Autonomously inspect and change additional task-local
paths when source, consumers, or diagnostics show they are required. Before calling something ambiguous or
blocked, use the acceptance criteria, surrounding code, consumers, tests, and
diagnostics to determine the smallest coherent way to achieve the stated
outcome. A packet need not enumerate every supporting file, implementation
detail, or local consequence. Refuse unrelated dirty state. Never amend,
rebase, merge, cherry-pick, integrate, push, or modify the packet/plan.

## Implement and verify

Map every acceptance criterion to source and test evidence. Make the smallest
cohesive change that satisfies the packet. Follow repository ownership,
struct-centered Rust, rustdoc, async, PyO3, contract, and journey-test rules.
Do not reopen a material task decision or implement later work.

Compiler, formatter, lint, test, fixture, codegen, and repository-managed
setup failures are ordinary development feedback. Use judgment to make the
smallest task-local completion change needed to deliver the packet's stated
outcome, even when a necessary supporting path is outside `write_set`:
imports, call-site type adjustments, declarations, generated output, fixtures,
rustdoc, and lint cleanup are examples, not a closed list. Record every such
path and the repository evidence or diagnostic that makes it necessary. This
completion closure never permits a materially new behavior, owner, dependency,
public or durable contract, acceptance outcome, unrelated cleanup, broad
formatting, or a prohibited path.

For a remediation generation, start again from the original `parent_sha`, read
the rejected candidate diff and exact findings, and produce one complete
replacement candidate. Do not create a repair-only descendant. The controller
supersedes the rejected candidate and integrates only the replacement.

For terminal remediation, require `parent_sha == reviewed_target_sha` and
produce one normal candidate atop that current integrated parent. Implement
only the assigned reversible findings. When findings have disjoint owners, the
controller may dispatch several terminal-remediation workers concurrently;
overlapping findings remain serial.

Finish one coherent implementation before checking it. Do not run compiler,
test, lint, or format commands after small edits or use tests as a stepwise
search mechanism. Run `verification.worker` after the coherent change when it
is present, repeatable, and safe; request the controller's heavy-resource lane
for a filtered Cargo diagnostic rather than omitting useful compiler feedback.
Batch all directly indicated mechanical
repairs before rerunning it, and never rerun without a relevant source or test
change. Continue while failures remain explained and task-local; escalate only
an unexplained diagnostic after focused investigation or a material decision.
The controller runs `verification.candidate` once per immutable candidate in
its authoritative resource lane. Never weaken, replace, skip, or mask either
command.

Return `BLOCKED` only after reasonable task-local investigation and recovery
for a material decision, unavailable authority, genuinely indeterminate
acceptance outcome, prohibited necessary write, or mandatory proof that cannot
be safely run. Do not elevate an omitted file, an unspecified implementation
detail, a first failing check, or an ordinary repair into a blocker when the
packet's intended outcome and repository evidence make the next action clear.

## Seal the candidate

Before committing, audit all tracked and untracked changes against the packet
contract and repair closure; run `git diff --check`; and map changed paths and
acceptance criteria to evidence. Confirm the configured Git identity already
matches repository policy. Create exactly one normal commit with
`parent_sha` as its sole parent, calculate the binary diff SHA-256, and leave
the worktree clean. Never alter the commit after reporting it.

This implementor audit is a self-check, not a review stage. Do not request,
dispatch, or perform a mutable-working-tree, pre-commit, or "pre-seal" code
review. The controller risk-routes at most one substantive independent review
after this immutable candidate commit exists.

## Output contract

Return exactly one YAML document.

Successful result:

```yaml
protocol: wyrd-implement-v3
outcome: CANDIDATE
request_id: <input value>
task_packet: {path: <input path>, sha256: <input digest>}
task_id: <packet contract ID>
candidate_sha: <immutable commit>
parent_sha: <input parent SHA>
supersedes_candidate_sha: <input remediation SHA or null>
terminal_finding_ids: [<input terminal finding IDs, or empty>]
terminal_binding: null
# Or exactly when terminal remediation is present:
# terminal_binding:
#   review: {path: <input path>, sha256: <input digest>}
#   reviewed_target_sha: <input target>
#   finding_ids: [<input IDs>]
#   assertion_ids: [<all selected assertion IDs>]
#   reviewed_integrated_tasks: [<terminal artifact's exact task identities>]
#   reviewed_proof_artifacts: [<terminal artifact's exact proof identities>]
diff_sha256: <lowercase 64-hex digest>
changed_paths: [<sorted repository-relative paths>]
forecast_expansion_paths:
  - path: <path outside write_set>
    evidence: <source, consumer, or diagnostic requiring it>
acceptance_trace:
  - assertion_id: <every packet AC, or every assigned terminal assertion, exactly once>
    implementation: [<path:symbol or path:line evidence>]
    tests: [<test or source evidence>]
diagnostic_verification:
  command: <packet worker diagnostic, or assigned terminal verification command, or null>
  result: <PASS|NOT_RUN|FAIL>
  evidence: <concise result or safety limitation>
```

Blocked result:

```yaml
protocol: wyrd-implement-v3
outcome: BLOCKED
request_id: <input value>
task_packet: {path: <input path>, sha256: <observed or input digest>}
category: <INVALID_REQUEST|AUTHORITY_REQUIRED|MATERIAL_CONFLICT|FORBIDDEN_WRITE|MANDATORY_PROOF_UNAVAILABLE>
evidence: [<specific repository facts and attempted recovery>]
decision_required: <single exact authority or correction needed>
partial_commit: null
```

Never return a partial candidate. Preserve working-tree evidence on `BLOCKED`
until the controller captures its binary diff, status, necessary untracked
evidence, report, and task binding. The controller may then reclaim it under
the plan-execution cleanup contract.

## Invalidation

A candidate is valid only for its task-packet digest, parent SHA, candidate
SHA, and diff digest. A changed packet, parent, amended/rebased commit, or
successor candidate requires fresh diagnostic verification and, when routed by
the controller's risk rule, fresh review.
