---
name: wyrd-implement-v3
description: Implement one decision-complete Wyrd task packet in an isolated worktree, stay within its declared scope, run its focused diagnostic verification, and return one immutable candidate commit or a material BLOCKED result. Use only when an orchestrator supplies the packet, parent SHA, and dedicated worktree; never use for planning, integration, review, or task-status mutation.
---

# Wyrd Implement v3

Produce one reviewable commit from one immutable task packet. The packet is the
single source of task scope, acceptance, dependencies, and focused
verification. The controller alone dispatches, verifies authoritatively,
reviews, integrates, supersedes, and records execution state.

Never invoke another workflow or spawn children. When delegated, run as
`gpt-5.6-sol` at low reasoning effort.

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
```

The task packet begins with a `Task contract` YAML block:

```yaml
id: <stable task ID>
depends_on: [<task IDs>]
write_set: [<repository-relative paths>]
prohibited_writes: [<repository-relative paths>]
acceptance_criteria:
  - id: AC1
    text: <verbatim observable criterion>
verification:
  command: <exact focused mise command>
```

Return `BLOCKED` before editing if the request is incomplete, packet digest
differs, its contract is missing or ambiguous, the worktree is not clean and
checked out at `parent_sha`, a path overlaps a prohibition, or `parent_sha` is
not a commit. Do not require a task status, revision, manifest, or controller
proof record: controller dispatch is the authority to begin work.

## Establish the execution boundary

Read `AGENTS.md`, `architecture/agent-rules.md`, the complete task packet, and
the architecture/design/doctrine references relevant to its changed behavior.
Inspect the named paths, consumers, tests, manifests, and `mise` command.
Use CodeGraph first when indexed.

Treat the packet's `write_set` as the behavioral ownership allowlist;
`prohibited_writes` always wins. Refuse unrelated dirty state. Never amend,
rebase, merge, cherry-pick, integrate, push, or modify the packet/plan.

## Implement and verify

Map every acceptance criterion to source and test evidence. Make the smallest
cohesive change that satisfies the packet. Follow repository ownership,
struct-centered Rust, rustdoc, async, PyO3, contract, and journey-test rules.
Do not reopen a material task decision or implement later work.

Compiler, formatter, lint, test, fixture, codegen, and repository-managed
setup failures are ordinary development feedback. Apply the smallest
candidate-caused mechanical repair needed to pass a required check even when
its adjacent path is outside `write_set`: imports, call-site type adjustments,
generated output, fixtures, rustdoc, and lint cleanup are allowed. Record each
such path and exact diagnostic. This repair closure never permits new behavior,
owners, dependencies, public or durable contracts, acceptance scope, unrelated
cleanup, broad formatting, or a prohibited path.

Run `verification.command` diagnostically when it is repeatable and safe. If
it is destructive, non-repeatable, cross-task, or a terminal qualification
gate, report why it was not run; the controller will run it once in its
authoritative lane. Never weaken, replace, skip, or mask the command.

Return `BLOCKED` only for a material decision, unavailable authority,
ambiguous acceptance outcome, prohibited write, or mandatory proof that cannot
be safely run. A first failing check or ordinary mechanical repair is not a
blocker.

## Seal the candidate

Before committing, audit all tracked and untracked changes against the packet
contract and repair closure; run `git diff --check`; and map changed paths and
acceptance criteria to evidence. Confirm the configured Git identity already
matches repository policy. Create exactly one normal commit with
`parent_sha` as its sole parent, calculate the binary diff SHA-256, and leave
the worktree clean. Never alter the commit after reporting it.

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
diff_sha256: <lowercase 64-hex digest>
changed_paths: [<sorted repository-relative paths>]
incidental_repair_paths:
  - path: <path outside write_set>
    diagnostic: <exact diagnostic>
acceptance_trace:
  - ac_id: <every packet AC exactly once>
    implementation: [<path:symbol or path:line evidence>]
    tests: [<test or source evidence>]
diagnostic_verification:
  command: <packet verification command>
  result: <PASS|NOT_RUN>
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
unless reverting only this run's known changes is explicitly requested.

## Invalidation

A candidate is valid only for its task-packet digest, parent SHA, candidate
SHA, and diff digest. A changed packet, parent, amended/rebased commit, or
successor candidate requires fresh diagnostic verification and review.
