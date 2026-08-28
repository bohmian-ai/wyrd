---
name: wyrd-implement-v3
description: Implement and verify an actionable Wyrd task or bounded remediation from direct instructions or a plan artifact. Align code to requested outcomes, repository authority, claims, evidence expectations, and referenced expertise; escalate only material unresolved decisions.
---

# Wyrd Implement v3

Own the complete implementation loop: understand the task, acquire relevant
context, edit code and tests, collect credible evidence, fix task-local
failures, and report the result. Task correctness matters; planner-specific
serialization and execution-controller bookkeeping do not.

## Determine the task contract

Accept direct instructions, an issue, a Markdown task, a structured packet, or
bounded review findings. The task is actionable when its requested outcome,
affected surface, material constraints, and observable success conditions can
be determined from the request plus repository authority.

No YAML block, section name, ID, digest, source SHA, clean worktree, write set,
predeclared command, candidate manifest, or controller generation is required.
If explicit claims or acceptance criteria are absent, derive a concise working
checklist from the requested behavior and repository completion rules, state
material assumptions, and proceed. Normalize unlabeled obligations with local
IDs when traceability helps.

Treat task claims as explainable obligations. Preserve their dispositions and
accepted evidence classes when supplied. Deterministic tests, LLM review, and
human attestation remain distinct; one never silently replaces another.

## Acquire relevant context

Read `AGENTS.md`, `architecture/agent-rules.md`, and applicable design/doctrine
authority. Read and apply every skill, architecture reference, pinned source,
prior decision, and implementation example explicitly named by the task. Use
each for the decisions it informs; repository authority and the locked outcome
still govern.

Follow CodeGraph instructions. Inspect the nearest behavior owner, callers,
consumers, tests, manifests, generated surfaces, and current `mise` tasks.
Treat a write set as a coordination forecast, not an allowlist. Make necessary
supporting edits to callers, fixtures, projections, documentation, and tests,
and report material scope expansion. Preserve unrelated user changes and avoid
unrelated cleanup.

If a named reference is unavailable, continue when the task and repository
provide enough authority. Block only when it is essential to choose among
materially different contracts or prove a required claim.

## Implement the complete cohesive change

Map every explicit or derived claim to source behavior and appropriate proof.
Implement the complete owned outcome, including required unit, integration,
user-journey, generated, and cross-language closure. Follow repository
ownership, struct-centered Rust, rustdoc, async, PyO3, contract, audit,
tenancy, and testing rules where applicable.

Compiler, formatter, lint, test, fixture, codegen, and setup failures are
development feedback. Diagnose and fix task-local causes, including necessary
consumers and declarations. Do not change a required claim's meaning, weaken a
gate, hide a failure, or choose production behavior solely to satisfy a
fixture. Ordinary local design choices remain yours when multiple
repository-native implementations satisfy the task.

For bounded remediation, read the original task, cited findings, and relevant
prior diff when available. Apply the required outcomes without demanding a
generation, superseded candidate, artifact digest, or replacement-commit
topology.

## Verify claims

Run the task's requested checks and the narrowest current `mise` checks required
by `AGENTS.md` for every touched surface. Inspect `mise.toml` before relying on
a command. Separate:

- diagnostics, which provide quick feedback;
- direct claim evidence, which proves specific required behavior; and
- integrated evidence, which proves cross-task or journey behavior.

If a listed command is stale but its proof intent is clear, run the current
canonical equivalent and disclose the substitution. Do not substitute when
exact command identity or output is itself an explicit requirement. Never skip,
weaken, mask, or falsely claim a check.

Record evidence compactly by subject, claim, evidence class, outcome, and
source/test/command references. Full logs remain with the execution
environment; do not invent an artifact store. Failed or missing evidence is
`failed` or `indeterminate`, never success.

Implementation completion means the behavior is implemented and declared
evidence is collected. It does not independently verify or authorize a Wyrd
`Change`, finalize an `EvidenceManifest`, imply merge, or grant deployment
authority.

## Escalate narrowly

Stop only when:

- unresolved ambiguity selects materially different public, durable, security,
  tenancy, migration, or acceptance behavior;
- the task conflicts with higher repository authority and resolution changes
  the requested outcome;
- required external or destructive action lacks authorization; or
- essential proof, infrastructure, or reference material is unavailable with
  no safe equivalent.

Do not block on missing protocol metadata, absent claim IDs, imperfect packet
structure, stale forecast paths, ordinary implementation choices, supporting
edits, the first failed check, or lack of controller/artifact-store machinery.

## Handoff

Return a concise human-readable report:

1. Implemented outcome and material design choices.
2. Changed owners/surfaces and any justified scope expansion.
3. Claim trace with evidence class and `satisfied`, `failed`, `indeterminate`,
   or `not_evaluated` status.
4. Commands and outcomes, including substitutions and limitations.
5. Referenced expertise that materially affected the implementation.
6. Remaining risks or one exact blocker.

Create a commit only when the caller or active execution environment requests
one. Then inspect the diff, run `git diff --check`, verify the existing Git
identity, follow repository identity rules, and never rewrite unrelated user
work. A commit is a delivery choice, not the definition of successful
implementation.
