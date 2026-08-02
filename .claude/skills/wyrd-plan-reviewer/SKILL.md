---
name: wyrd-plan-reviewer
description: Independently review Wyrd feature specs, architecture proposals, implementation plans, and risk-tiered Luna/Terra/Sol task packets before implementation on separate architecture-soundness and execution-readiness axes. Use for Wyrd plan gates, spec gates, task handoff gates, material revisions made by $wyrd-implement-plan, API/SDK/CLI/MCP contracts, crate-boundary changes, Vala/Bifrost or Skald designs, and risk-tiered review of artifacts produced by $wyrd-plan. Validate change-impact closure, material decisions, executable verification, cold rehearsal, and controlled implementation adaptability; report only blocking Critical and Major issues.
---

# Wyrd Plan Reviewer

`$name` denotes a Wyrd skill; load it with the `Skill` tool. `Luna`, `Terra`,
and `Sol` are risk tiers, not model names.

Determine independently whether a Wyrd artifact is architecturally sound and,
for a plan gate, executable through its surface-appropriate implementation
skill without a material decision during coding. Concision is not a defect;
unsupported readiness is.

Report only blocking Critical and Major findings. Keep architecture correctness
and execution readiness as separate axes. Do not modify the reviewed plan,
task packets, implementation source, tests, manifests, or lockfiles. Write only
the durable review artifact when requested.

## Plan-execution advisory mode

When `$wyrd-implement-plan` submits an orchestrator-authored material revision,
this mode overrides conflicting standalone workflow and output instructions:

- Review the original intended outcome, revised canonical plan/tasks, decision
  record, repository evidence, affected authorities, downstream consumers, and
  revised proof.
- Perform static architecture, impact, contract, task, and cold-rehearsal
  analysis only. Do not run tests, builds, lints, formatters, generators,
  migrations, services, plan validators, or repository gates.
- Validate that the revision is repository-grounded, cohesive, reasonable, and
  no weaker in correctness, security, tenancy, auditability, ergonomics, user
  experience, or verification.
- Return `ADVISORY_APPROVE` only when architecture and executability both pass.
  Return `ADVISORY_REVISE` with consolidated Critical/Major root causes and
  required edits otherwise.
- Do not write a review artifact, modify plan/task/source, invoke planning or
  implementation, stop the user workflow, or communicate with the user.

The `$wyrd-implement-plan` root owns every revision and resolves findings
autonomously. Re-review prior findings first and repeat until
`ADVISORY_APPROVE`.

## Establish authority

Use the active Wyrd repository or worktree. Read completely:

1. `AGENTS.md`
2. `architecture/agent-rules.md`
3. `architecture/wyrd-design.md`
4. `architecture/wyrd-doctrine.mdx`
5. the reviewed artifact and every authority it explicitly names

For a `$wyrd-plan` plan gate, also read:

- `.claude/skills/wyrd-plan/SKILL.md`
- `.claude/skills/wyrd-implement/SKILL.md`
- `.claude/skills/wyrd-plan/references/plan-format.md`
- `.claude/skills/wyrd-plan/references/task-packet-format.md`
- `architecture/references/languages/implementation-execution.md`

When any task writes the Wyrd Svelte/UI tree, also read
`.claude/skills/wyrd-ui/SKILL.md`. Confirm the task's selected execution skills
accept its complete write set.

Run the structural plan validator for filesystem-backed artifacts. Structural
success proves format only; it does not prove architecture or executability.

Current Wyrd design wins over generated artifacts, old plans, predecessor
behavior, and implementation drift unless the proposal explicitly replaces a
decision and updates its owning authority.

When `.codegraph/` exists, use CodeGraph before grep or manual traversal to
verify owners, callers, consumers, dispatch, dependencies, and tests.

## Load references progressively

Always read `references/plan-review-rubric.md`. For a plan or task handoff gate,
also read `references/execution-readiness-review.md`.

Read `architecture/references/README.md`, then load only the Wyrd knowledge
required by the proposed surfaces:

| Reference | Load when the artifact touches |
|---|---|
| `references/production-risk-review.md` | Security, tenancy, audit, durable writes, migrations, storage, background work, distributed ownership, high-volume paths, or production claims |
| `references/public-workflow-review.md` | Developer or agent discovery, configuration, invocation, composition, debugging, or recovery |
| `references/review-format.md` | Writing a durable filesystem-backed review |
| `architecture/references/doctrine/positioning-and-vocabulary.md` | Card vocabulary, `CardRef`, v1 kinds, or removed concepts |
| `architecture/references/doctrine/architecture-constraints.md` | Tier boundaries, deployment, or observation identity |
| `architecture/references/architecture/patterns.md` | Crate placement and server/client/storage/provider/audit ownership |
| `architecture/references/languages/rust-core.md` | Rust ownership, traits, async, allocation, or API shape |
| `architecture/references/languages/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, conversions, or registration |
| `architecture/references/languages/errors.md` | Stable errors and boundary mappings |
| `architecture/references/languages/python-api-and-stubs.md` | Python exports, stubs, package layout, or tests |
| `architecture/references/languages/testing-workflows.md` | Test tiers, journey coverage, verification levels, or boundary gates |
| `architecture/references/languages/agent-harness.md` | MCP and other agent-facing contracts |
| `architecture/references/languages/typescript-guide.md` | `@wyrd/sdk` and napi conventions |
| `architecture/references/domain/iceberg-bifrost.md` | Bifrost, Iceberg, DataFusion, or object storage |

State which conditional references were loaded and the decision each governs.

## Select the gate

- **Spec gate:** settle workflow, requirements, non-goals, public and durable
  contracts, ownership, safety invariants, and material behavior. Do not
  require an implementation DAG. Mark execution readiness `Not applicable`.
- **Plan gate:** require both architecture soundness and execution readiness
  for every task.
- **Full sweep:** use only when explicitly requested or required by the
  artifact.

For a plan gate, apply:

```text
overall approval = architecture PASS and executability PASS
```

## Review architecture soundness

1. State the goal, primary workflow, declared scale, deployment assumptions,
   and readiness claim.
2. Identify changed public and durable contracts, owners, data paths,
   migrations, safety boundaries, and irreversible decisions.
3. Validate current-state claims against source and authority.
4. Confirm every material choice has one answer or an explicit authority
   update.
5. Trace success, likely failure, recovery, tenancy, audit, lifecycle, and
   deployment behavior where relevant.
6. Apply reversibility and KISS. Block material safety, contract, ownership,
   production, or workflow errors; do not demand speculative machinery.

## Review execution readiness

1. Independently validate the plan's concrete change-impact graph from every
   seed change through owners, callers, dispatch, consumers, manifests,
   features, explicit targets, fixtures, generated/language projections,
   persistence, audit, tenancy, lifecycle, deployment, and verification setup.
2. Map every requirement to a cohesive task, acceptance criteria, tests,
   focused proof, and integrated closeout.
3. Confirm each task locks material architecture, contracts, persistence,
   security, ownership, acceptance outcomes, and verification design.
4. Confirm private paths, helper names, fixture layout, and exact syntax remain
   advisory unless their exact shape protects a named material invariant.
5. Validate commands, packages, features, targets, filters, fixtures, support
   exports, services, migrations, and environment against the repository.
6. Confirm planner evidence that every `Ready` task passed a documented cold
   implementation rehearsal from the task packet alone, using a fresh agent
   when the planning environment supported it. Independently repeat the
   rehearsal with a new read-only agent for cross-cutting or high-risk tasks
   when the review environment supports it.
7. Confirm the task permits living factual corrections and equivalent
   non-weaker proof while stopping material deviations.

Independently execute only a high-risk or untrustworthy verification command
needed to resolve readiness. Do not duplicate trustworthy planner evidence,
run dependency updates, or modify lockfiles. Use repository setup and locked
dependency resolution when execution is necessary.

## Distinguish decisions from mechanics

A task is executable when the implementer can begin and finish without
inventing material architecture, public or durable contracts, persistence,
security semantics, ownership direction, acceptance outcomes, or proof design.
Its declared execution skill or skill set must accept the task's write set;
non-UI work uses `$wyrd-implement`, while Svelte/UI work includes `$wyrd-ui`.

Do not require immutable private file inventories, helper names, fixture
layout, or exact command strings. Require responsibilities, material
interfaces, consequential control flow, failure behavior, required test tier,
and proof semantics. A task that over-constrains reversible mechanics enough
to predict false blockers may itself have a Major executability defect. Judge
the packet against its assigned Luna, Terra, or Sol risk tier: the tier sets
scope, reviewer rigor, and escalation order, not the implementing model.

## Findings and verdict

Use these finding types:

- `ARCHITECTURE_CONFLICT`
- `MATERIAL_DECISION_GAP`
- `IMPACT_GRAPH_GAP`
- `UNEXECUTABLE_TASK`
- `INVALID_VERIFICATION_RECIPE`
- `MISSING_REHEARSAL_EVIDENCE`

Report only:

- **Critical:** credible tenant/security breach, data loss or corruption,
  invalid irreversible migration, broken durable/public contract, violation of
  a load-bearing invariant, or architecture unable to meet the stated goal.
- **Major:** a material unresolved decision, missed cross-cutting consumer,
  invalid proof recipe, failed cold rehearsal, or task boundary likely to
  require substantial redesign, scope invention, or interruption.

All findings block overall approval:

- **Approve:** architecture `PASS` and executability `PASS`.
- **Revise:** one or more Major findings and no Critical findings.
- **Stop/rethink:** one or more Critical findings.

For spec gates, base the verdict on architecture and mark executability
`Not applicable`.

Every finding names location, type, severity, issue, concrete impact, source
evidence, and one required plan edit. Never promote a mechanical preference
without a material consequence. Consolidate one root cause into one finding.
Allow at most three non-gating implementation handoff notes.

## Re-review and output

For a revised artifact, verify prior findings first, inspect affected seams,
and do not reopen accepted decisions without new evidence.

For a filesystem-backed review, read `references/review-format.md` and write:

```text
.dev/review/<review-id>/review.md
```

Lead the chat handoff with overall verdict, architecture axis, executability
axis, Critical/Major counts, and one clickable review path. If approved, state
plainly that implementation may proceed.
