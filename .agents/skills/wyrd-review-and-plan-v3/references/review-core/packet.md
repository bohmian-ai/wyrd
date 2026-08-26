# Shared Terminal Static Review Packet

Apply these rules in the full v3 terminal review. The packet is immutable
orchestration context for one completed v3 execution, not a durable
user-facing output.

Read `orchestration-contract.md` before constructing the packet or reviewer
roster.

## Input contract

Require the complete immutable input tuple declared by the skill entrypoint:
request and review identity, repository root, execution baseline, integrated
target and merge-base SHAs, output directory, approved intent, v3 plan and
plan-review artifacts, every task packet and candidate lineage record, and
integrated-verification evidence. The terminal review always retains its
required evidence.

Resolve the target, base, and merge base to commit SHAs before analysis. Review
only the committed range from the merge base through the target. Do not include
staged, unstaged, untracked, or other working-tree content.

Record:

- integrated target and resolved SHA;
- execution baseline and resolved SHA;
- merge-base SHA;
- changed files, renames, commits, and complete committed diff;
- every required authority, plan, task, lineage, and proof path and digest;
- mode, review ID, risk, change profile, and reviewer assignments;
- the complete baseline roster, trigger matches, assignment IDs, required
  capabilities, reviewer identities, and prompt digests;
- applicable repository authorities and hard gates.

If any SHA, required artifact, digest, or lineage binding cannot be resolved,
return `REVIEW_BLOCKED`. Do not silently choose another ref or authority.

## Static-analysis boundary

Do not execute project tests, builds, formatters, linters, generators,
migrations, servers, live-service checks, benchmarks, or repository gates. Do
not modify source or implement findings.

Reviewers may inspect:

- the committed diff and complete target-branch symbols;
- owners, callers, consumers, dispatch, contracts, manifests, and generated
  projections;
- existing and changed tests, assertions, fixtures, feature gates, and command
  definitions;
- already-recorded evidence when the reference provides it, as intent context
  rather than independently reproduced proof.

Static uncertainty is not a required finding. Record an important unresolved
limit under `Static analysis boundary`; promote only defects supported by
source, contracts, tests, repository rules, or the committed diff.

Provide the packet through each reviewer's complete immutable assignment file.
Persist the stable packet and input digests as
`{REVIEW_DIR}/evidence/packet.md` under `evidence-contract.md`.

## Intent before judgment

Read the approved intent, plan, plan review, and task packets completely.
Extract intended user outcomes, requirements, invariants, locked decisions,
non-goals, and stated acceptance. Use that map to focus review, but continue
independent analysis of the complete changed impact cone.

Those artifacts communicate intent; they do not override current repository
architecture, governing instructions, public contracts, or hard gates. Private
paths, helper names, and illustrative implementation details are non-normative
unless the reference makes their exact shape material for a stated invariant.

Missing approved intent is a malformed v3 execution handoff and produces
`REVIEW_BLOCKED`; do not infer it from commits or chat.

## Rule authority

Read applicable `AGENTS.md`, nested instructions, architecture, review policy,
and criteria from the target snapshot, while continuing to obey host-provided
instructions. Identify hard gates explicitly described as `MUST`,
non-negotiable, a hard acceptance criterion, incomplete when violated, or
prohibited from passing review.

Every baseline and triggered assignment checks applicable hard gates within
its changed impact cone. A statically confirmed violation in new or materially modified code is
`BLOCK_BEFORE_MERGE`. Final findings quote only the exact governing rule and
source location.

## Risk and assignment routing

Classify risk:

- `low`: documentation, examples, formatting-only, dependency metadata with no
  runtime effect, or isolated behavior-preserving cleanup;
- `standard`: ordinary bounded implementation, tests, internal APIs, build
  scripts, or configuration;
- `high`: public contracts, auth, secrets, persistence, migrations, storage,
  concurrency, tenant isolation, SDK/CLI/MCP/UI surfaces, or broad
  compatibility and data-loss risk.

Risk changes static-analysis depth, not the selected mode or baseline roster.
Always create the seven baseline assignments. Add only triggered questions
justified by the impact cone. Concurrency limits create dispatch waves; they
never justify combining required assignments.

## CodeGraph

Use CodeGraph only when its indexed working tree corresponds to the resolved
target snapshot for the symbols being inspected. Otherwise use commit-aware
Git reads so working-tree drift cannot contaminate the branch review.

## Repository review policy

Discover applicable repository review skills and classify them before use:

- An integration binding writes a final review, assigns a verdict, or invokes
  planning. Load its rules in the root orchestrator; never dispatch it as a
  candidate-producing reviewer.
- A specialist lens returns candidates only. The terminal review may dispatch
  it when its question is independently relevant.

Only the root orchestrator writes the consolidated artifact, assigns final
finding IDs, or creates the v3 execution handoff.
