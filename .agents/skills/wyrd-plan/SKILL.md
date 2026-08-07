---
name: wyrd-plan
description: Plan Wyrd feature, refactor, migration, API, SDK, CLI, MCP, UI, Skald, Vala/Bifrost, storage, testing, and architecture work as a decision-complete, execution-grounded specification with rehearsed, risk-routed Luna/Terra/Sol task packets. Use when Codex must investigate, design, scope, sequence, decompose, or prepare Wyrd coding work before execution by $wyrd-implement or $wyrd-implement-plan. Do not use to implement the plan or review completed code.
---

# Wyrd Plan

Convert user intent and live repository evidence into the smallest
decision-complete plan that an implementation agent can execute without
redesign. Lock material behavior and boundaries. Leave reversible mechanics to
`$wyrd-implement`.

Planning is complete only when the proposed implementation is executable, not
when the document merely looks complete.

## Establish authority

Before planning:

1. Read `AGENTS.md`, `architecture/agent-rules.md`,
   `architecture/wyrd-design.md`, and `architecture/wyrd-doctrine.mdx`.
2. Read the applicable repo-local execution skills:
   `.agents/skills/wyrd-implement/SKILL.md` and, for UI scope,
   `.agents/skills/wyrd-ui/SKILL.md`.
3. Read `architecture/references/README.md`, then only the references relevant
   to the affected surfaces.
4. Inspect `mise.toml`, affected manifests, `pyproject.toml`, and lockfiles
   before naming commands, dependencies, or features.
5. When `.codegraph/` exists, use CodeGraph before grep, find, or manual
   source-reading loops.

Current design is authority, not immutable history. A requested change may
replace an existing decision only when the plan names the superseded authority
and includes its update. Otherwise treat the conflict as unresolved.

## Load planning references progressively

Read each selected reference completely when its stage begins:

| Reference | Load when |
|---|---|
| `references/decision-completeness.md` | Resolving requirements, contracts, material boundaries, or allowed adaptation |
| `references/verification-planning.md` | Inspecting commands, features, fixtures, setup, and proof |
| `references/task-decomposition.md` | Creating tasks and running implementation rehearsal |
| `references/plan-format.md` | Drafting, auditing, presenting, or saving the plan |
| `references/task-packet-format.md` | Drafting or updating task packets |

For Wyrd domain knowledge, route through `architecture/references/README.md`
and load only the needed Vala slice: `domain/vala-architecture.md`,
`domain/telemetry-observations.md`, `domain/evaluation.md`,
`domain/drift-monitoring.md`, `domain/olap-serving.md`, `domain/iceberg.md`,
`domain/datafusion.md`, `domain/arrow-analytical-interop.md`, or
`domain/analytical-operations-reliability.md`.

Examples establish density and structure, not repository facts.

## Investigate the repository

Trace the primary workflow and nearest precedent. Establish:

- user or agent outcome and entry point;
- current behavior, owners, source seams, callers, and tests;
- public, internal, generated, persisted, and language-projected surfaces;
- crate, package, service, store, and dependency ownership;
- security, tenancy, audit, lifecycle, deployment, concurrency, and recovery;
- manifests, features, explicit targets, fixtures, support exports, and setup;
- verification commands affected by the dependency cone.

Separate verified facts, assumptions, unknowns, and unresolved choices.
Discover repository facts before asking the user.

## Build the change-impact graph

Every implementation plan includes a repository-specific Mermaid graph under
`Current state and evidence`. Start at each seed change and trace labelled
edges through:

- owners, callers, consumers, traits, implementations, and dispatch;
- manifests, Cargo features, explicit test targets, fixtures, and support
  exports;
- generated contracts and Rust, Python, TypeScript, HTTP, CLI, MCP, and UI
  projections;
- persistence, audit, tenancy, lifecycle, deployment, and recovery;
- verification commands and their environment or service dependencies.

Use concrete repository nodes. A prose checklist may explain evidence but does
not replace the graph. Mark a category `no impact` when repository evidence
shows it is inapplicable.

## Resolve material decisions

Lock objective, requirements, non-goals, constraints, compatibility, rollout,
owners, interfaces, state transitions, failure behavior, and proof.

A choice is material when alternatives change public or durable behavior,
cross-owner contracts, dependencies or features, security or tenancy,
persistence or migration, data-loss behavior, acceptance outcomes, or required
verification. Resolve it in the plan.

Leave local, reversible mechanics adaptable: private helper extraction,
repository-aligned private names and paths, incidental local structure,
mechanical caller changes, existing fixture use, and equivalent non-weaker
verification commands.

Define typed stubs for new or materially changed interfaces. Provide normative
pseudocode when ordering, transactions, state transitions, side effects,
concurrency, cancellation, or error mapping affect correctness.

## Prove task executability

Before marking a task `Ready`:

1. Inspect every proposed `mise`, Cargo, package, or script command.
2. Confirm the named task, package, feature, target, filter, fixture, support
   export, and repository setup exist.
3. Run the narrow command when feasible. At minimum compile the exact target
   and feature selection and prove the filter selects the intended tests.
4. Identify repository-provided services, migrations, environment, and
   checked-in local test configuration required at execution time.
5. Record unavailable external infrastructure without presenting runtime proof
   as passed.

Do not substitute a nearby command without recording the corrected command in
the task. See `references/verification-planning.md`.

## Rehearse implementation cold

After drafting a task, run a read-only cold rehearsal using only the task
packet, repository, and normal repository authorities. Use a fresh agent when
the environment supports it. When fresh-agent delegation is unavailable,
perform and document the same cold pass yourself rather than blocking task
readiness. The rehearsal must:

- locate every target owner and symbol;
- trace callers, consumers, dispatch, and test seams;
- inspect every verification command and its setup;
- walk the first implementation and test steps;
- identify missing dependencies, unreachable fixtures, or material decisions.

Do not give a fresh rehearsal agent the intended answer or planning
conclusions. Revise and repeat until the rehearsal can begin implementation
without making a material decision. Document-only architecture review does not
replace this gate.

## Compile decisions into tasks

Treat task generation as compilation:

```text
intent + repository evidence + impact graph + decisions + executable proof
    -> rehearsed risk-routed implementation task packets
```

Every plan has at least one separate task. Prefer two through five
dependency-ordered vertical tasks for medium work. Split on cohesive outcomes,
stable prerequisites, ownership, risk, or useful context boundaries—not files
or layers. Assign Luna to mechanical tasks, Terra to ordinary implementation,
and Sol to security, public/persisted contracts, migrations, concurrency,
cross-owner work, and other materially high-risk tasks. Keep plan-level
integration and closeout in the parent plan.

## Hand off a controlled living plan

Requirements, public or persisted contracts, security and tenancy semantics,
data-loss behavior, material architecture, and acceptance outcomes remain
immutable without user or planning authority.

During execution, `$wyrd-implement` may append or correct:

- discovered repository facts;
- internal paths and private symbol names;
- equivalent non-weaker verification commands;
- incidental private implementation structure;
- progress, failures, and evidence.

Record these updates in the active task using
`references/task-packet-format.md`. A bounded correction does not require a new
remediation plan. A material conflict sets the task to `Blocked` and returns it
for authority.

## Audit and emit artifacts

Load both format references and confirm:

- every requirement maps to implementation and objective proof;
- the impact graph covers the affected dependency cone;
- all material choices have one answer;
- every command and setup requirement was execution-checked;
- every task passed a documented cold rehearsal, fresh-agent when available;
- task dependencies leave coherent repository states;
- required independent review passed for risk-gated changes.

Use the global `$plan-readiness-reviewer` for that independent gate. Supply the
canonical plan directory and request a plan gate. The reviewer discovers Wyrd
authority from `AGENTS.md`, the artifact, and current repository evidence; do
not make its approval depend on loading this planning skill as review policy.

Run the structural validator for materialized plans:

```bash
python .agents/skills/wyrd-plan/scripts/validate_plan_artifacts.py \
  .dev/plan/<slug>
```

The validator proves structure, not semantic readiness.

In non-mutating contexts, return one `<proposed_plan>` block with complete
artifact markers. When authorized, save the plan under
`.dev/plan/<slug>/implementation-plan.md` and each task under `tasks/`.
Use `Approved` only after readiness and required review; use `Ready` only for
tasks in an approved plan that passed executable preflight and cold rehearsal.
