---
name: wyrd-implement-v3
description: Implement or resume exactly one decision-complete Ready Wyrd task from one accepted Git SHA in an isolated worktree, restrict edits to the task's declared write set, run focused development verification, and return one immutable candidate commit or a material BLOCKED result. Use only when an orchestrator supplies the exact v3 implementation request contract; never use for planning, UI work unless explicitly assigned, integration, review, or task-status mutation.
---

# Wyrd Implement v3

Produce one reviewable commit for one exact task. The candidate is a proposal;
the orchestrator alone may accept, integrate, supersede, or update canonical
task state.

This skill is self-contained. Never invoke or delegate to another implementation
or review skill. Never spawn agents. When an orchestrator delegates this role,
it must use `gpt-5.6-sol` with low reasoning effort.

## Required input

Accept exactly one YAML object with no omitted fields:

```yaml
protocol: wyrd-implement-v3
request_id: <stable orchestration id>
repository_root: <absolute path>
task_artifact:
  path: <repository-relative canonical task path>
  sha256: <lowercase 64-hex digest of its exact bytes>
task_id: <exact id inside the artifact>
task_revision: <integer, including orchestrator-authored remediation revisions>
task_status: Ready
accepted_sha: <40-hex controller integration head accepted at dispatch>
base_sha: <40-hex original task dispatch base>
parent_sha: <40-hex only allowed parent; base initially or predecessor on remediation>
worktree_path: <absolute dedicated worktree path>
write_set:
  - <repository-relative file or directory boundary>
prohibited_writes:
  - <repository-relative boundary>
acceptance_criteria:
  - id: <stable AC id>
    text: <verbatim criterion>
development_checks:
  - id: <stable check id>
    command: <exact worker diagnostic command>
authoritative_checks:
  - id: <stable controller proof id>
    command: <exact command reserved to the controller verification lane>
material_escalations:
  - <decision reserved to the orchestrator>
predecessor_candidate: <40-hex superseded candidate or null>
```

Reject the request as `BLOCKED` before editing if a field is missing, the task
is not `Ready`, the artifact digest differs, the task/revision or verbatim ACs
do not match the artifact, a supplied SHA is not a commit, `parent_sha` is not
`base_sha` for initial work or the named predecessor for remediation, the
worktree is not dedicated and clean, or the write boundaries are ambiguous or
overlapping with a prohibition. A reversible implementation or review finding
produces a successor candidate generation under the same task revision. Require
a new task revision only when contract, write-set authority, dependencies,
acceptance criteria, or proof authority materially changes; review findings
alone do not authorize such changes.

## Establish the execution boundary

1. Read `AGENTS.md`, `architecture/agent-rules.md`,
   `architecture/wyrd-design.md`, the complete task artifact, and
   `architecture/wyrd-doctrine.mdx` whenever behavior or a public/internal
   contract is touched. Read routed architecture references required by the
   task and inspect `mise.toml`, manifests, and lockfiles relevant to checks.
2. Verify the artifact with `sha256sum`, then verify
   `git -C <worktree> rev-parse HEAD` equals `parent_sha`, the worktree is
   clean, and every supplied SHA resolves as a commit.
3. When `.codegraph/` exists, use CodeGraph before grep/find for code discovery.
4. Treat `write_set` as the behavioral ownership allowlist, with the incidental
   repair closure defined below. `prohibited_writes` always wins. Do not edit
   the canonical task, plan, orchestration state, or review artifacts unless
   they are explicitly in the write set—and even then never change task status.
5. Refuse unrelated dirty state instead of absorbing it. Never amend, rebase,
   merge, cherry-pick, integrate, or push.

## Implement

Map every acceptance criterion to concrete owners, behavior, and proof. Make
the smallest cohesive change that satisfies the exact Ready task. Follow all
repository ownership, struct-centered Rust, rustdoc, async, PyO3, contract,
test-integrity, and user-journey rules. Do not redesign a material contract,
resolve a material conflict, expand scope, or implement later tasks.

Compiler, formatter, lint, test, fixture, and repository-managed local setup
failures are development feedback. Diagnose and repair them without refusing
the task, requesting user authorization, or requiring a task revision.

The task automatically authorizes the smallest candidate-caused mechanical
repair needed to make repository-required compile, format, Clippy, rustdoc,
codegen, fixture, or focused-test checks pass, even when the exact adjacent file
was omitted from `write_set`. This incidental repair closure includes imports,
call-site type adjustments, generated outputs, test fixtures, rustdoc, and lint
cleanup. It does not authorize new behavior, dependencies, owners, public or
durable contracts, acceptance criteria, unrelated cleanup, broad formatting,
or any `prohibited_writes` path. Record every such path and its diagnostic in
`incidental_repair_paths`. If the repair cannot satisfy these limits, stop
because of the material choice or prohibited path—not merely because the path
was absent from `write_set`.

A required material decision, unavailable authority, ambiguous acceptance
outcome, prohibited write, or genuinely unavailable mandatory external proof
is `BLOCKED`; difficulty, elapsed time, a failing first attempt, and a
mechanical out-of-list repair are not.

## Verify and seal

Run `development_checks` after inspecting what they execute. They are worker
diagnostics only and can never satisfy controller proof. Before sealing, also
run each repeatable task-focused `authoritative_checks` command diagnostically
when its required lane and repository-managed state are available. Iterate on
ordinary failures until it passes. Record these runs as development checks;
they do not consume, replace, or predict the controller's authoritative attempt.

Do not run an authoritative command diagnostically when it is destructive,
non-repeatable, consumes unique external state, measures performance that a
prior run would contaminate, emits a source-SHA-bound final report, requires
cross-task integration, or is a terminal qualification gate. Report that exact
reason as a diagnostic limitation and continue only when the remaining checks
still justify sealing. The controller runs every authoritative proof fresh
against the immutable candidate in its isolated lane.

A stale, invalid, unsafe, or unavailable declared command is an authority
defect; return `BLOCKED/AUTHORITY_REQUIRED` only when its authority must change.
Never replace or reinterpret it unilaterally. Add only diagnostics already
required by repository policy or the task's repeatable focused proof. Do not
weaken, skip, ignore, or mask a gate.

Before committing:

1. Audit tracked and untracked paths against `write_set`, the documented
   incidental repair closure, and `prohibited_writes`.
2. Run `git diff --check` and map every changed path and AC to evidence.
3. Confirm the Git identity already matches repository policy; never alter it.
4. Create exactly one normal commit whose sole parent is `parent_sha`.
5. Record `candidate_sha`, `parent_sha`, and
   `diff_sha256 = sha256(git diff --binary <parent_sha>..<candidate_sha>)`.
6. Confirm the worktree is clean and `git rev-list --parents -n 1` shows
   exactly the reported parent. Never modify the commit after reporting it.

## Output contract

Return exactly one YAML document and no additional status vocabulary.

Successful result:

```yaml
protocol: wyrd-implement-v3
outcome: CANDIDATE
request_id: <input value>
task_artifact: {path: <input path>, sha256: <input digest>}
task_id: <input value>
task_revision: <input value>
accepted_sha: <input value>
base_sha: <input value>
candidate_sha: <40-hex immutable commit>
parent_sha: <input parent_sha>
diff_sha256: <lowercase 64-hex digest>
changed_paths: [<sorted repository-relative paths>]
incidental_repair_paths:
  - path: <changed path outside write_set; use [] when none>
    diagnostic: <exact compile/lint/rustdoc/codegen/fixture/test reason>
acceptance_trace:
  - ac_id: <every input AC exactly once>
    implementation: [<path:symbol or path:line evidence>]
    tests: [<test or static proof>]
development_checks:
  - id: <input development check, diagnostic authoritative check, or repository-required diagnostic id>
    command: <command actually run>
    result: PASS
    evidence: <concise result>
authoritative_checks_run: []
predecessor_candidate: <input value>
```

Blocked result:

```yaml
protocol: wyrd-implement-v3
outcome: BLOCKED
request_id: <input value>
task_artifact: {path: <input path>, sha256: <observed or input digest>}
task_id: <input value>
task_revision: <input value>
accepted_sha: <input value>
base_sha: <input value>
parent_sha: <input value>
category: <INVALID_REQUEST|AUTHORITY_REQUIRED|MATERIAL_CONFLICT|FORBIDDEN_WRITE|MANDATORY_PROOF_UNAVAILABLE>
evidence: [<specific repository facts and attempted recovery>]
decision_required: <single exact authority or correction needed>
partial_commit: null
```

Never return a partial candidate. Leave no commit on `BLOCKED`; preserve any
working-tree evidence for the orchestrator unless safely reverting only this
run's known changes is explicitly requested.

## Successor invalidation

A candidate is valid only for the exact tuple `(task artifact digest, task id,
task revision, accepted SHA, base SHA, candidate SHA, parent SHA, diff digest)`. Any new
task revision, accepted SHA, changed artifact bytes, amended/rebased commit, or
successor candidate invalidates every earlier candidate and every review of it.
The orchestrator must name the superseded SHA as `predecessor_candidate`; this
worker never reuses approval or evidence from the predecessor without rerunning
the checks required by the new request.
