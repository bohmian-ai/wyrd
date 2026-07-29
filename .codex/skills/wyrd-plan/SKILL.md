---
name: wyrd-plan
description: Plan Wyrd feature, refactor, migration, API, SDK, CLI, MCP, UI, Skald, Vala/Bifrost, storage, testing, and architecture work as a decision-complete implementation specification with executable Terra/Luna task packets. Use when Codex must investigate, design, scope, sequence, decompose, or prepare Wyrd coding work before execution by $wyrd-implement. Do not use to implement the plan or review completed code.
---

# Wyrd Plan

Act as the high-reasoning planning model. Convert ambiguous intent and live
repository evidence into the smallest complete implementation specification
that transfers consequential reasoning from Sol to Terra/Luna.

Plan so that `$wyrd-implement` can execute each task without redesigning the
solution, reconstructing architecture, choosing among material alternatives,
or reinterpreting the product request. Terra/Luna may choose syntax, local
variable names, small helper extraction, and other reversible mechanics already
fixed by repository conventions.

## Establish authority

Use the active Wyrd repository or worktree. Before planning:

1. Read `AGENTS.md` and `architecture/agent-rules.md` to EOF.
2. Read `architecture/wyrd-design.md` and
   `architecture/wyrd-doctrine.mdx` to EOF.
3. Read the applicable repo-local execution skills:
   `.codex/skills/wyrd-implement/SKILL.md` for non-UI work and
   `.codex/skills/wyrd-ui/SKILL.md` when the write set enters the UI.
4. Read `architecture/references/README.md`, then load only the Wyrd doctrine,
   architecture, language, testing, agent, or domain references relevant to
   the request.
5. Inspect `mise.toml`, affected manifests, `pyproject.toml`, and lockfiles
   before naming commands, dependencies, or feature sets.

When `.codegraph/` exists, use CodeGraph before grep, find, or manual
source-reading loops. Hydrate the current owners, seams, callers, data flow,
tests, and generated artifacts from source. Do not infer repository shape from
the request or an old plan.

Current Wyrd design is authority, not immutable history. When the requested
workflow proves a current decision wrong, name the superseded decision and
include the owning design and doctrine updates. Treat an implicit, unsafe, or
out-of-scope conflict as unresolved.

## Load planning guidance progressively

Read each selected reference completely when its stage begins. All references
are direct children of this skill.

| Reference | Load when |
|---|---|
| `references/decision-completeness.md` | After initial investigation, before resolving requirements, contracts, control flow, failures, or architecture. Always load for an implementation plan. |
| `references/verification-planning.md` | After affected surfaces are known, before defining task or closeout verification. Always load for an executable plan. |
| `references/task-decomposition.md` | After decisions and verification topology are known, before choosing task count, dependencies, or parallelism. Always load when work will be handed to Terra/Luna. |
| `references/plan-format.md` | When drafting, auditing, presenting, or materializing the canonical plan. |
| `references/task-packet-format.md` | When drafting each separate implementation task under the canonical plan directory. |

Use the examples in the references as detail and structure standards. Adapt
their Wyrd nouns, owners, commands, and test tiers to current source; never
copy an example as repository evidence.

## Investigate before deciding

Trace the current workflow and nearest precedent. Establish:

- the user or agent outcome and primary persona;
- verified current behavior, limitations, and source evidence;
- affected public and internal surfaces;
- owning crates, packages, services, stores, and dependency direction;
- target paths, symbols, callers, tests, and generated artifacts;
- security, tenancy, audit, migration, concurrency, and recovery boundaries;
- existing `mise` tasks, feature gates, and relevant test lanes;
- assumptions, unknowns, and unresolved product preferences.

Run non-mutating checks when they materially reduce uncertainty. Separate
verified facts, assumptions, and unknowns. Resolve discoverable facts from the
repository before asking the user. Ask one high-impact question at a time only
when its answer changes the implementation specification.

## Resolve the implementation specification

Lock the objective, success criteria, stable requirements, non-goals,
constraints, compatibility, and rollout expectations before task generation.
Resolve every choice whose alternatives could change behavior, architecture,
security, data integrity, performance, build features, operations, or proof of
correctness.

Define the code shape required to carry those decisions:

- owning paths, modules, crates, packages, and existing symbols;
- new or materially changed structs, enums, traits, methods, functions,
  request/response shapes, migrations, and generated contracts;
- responsibilities, inputs, outputs, invariants, visibility, and dependency
  direction;
- validation, authorization, ordering, atomicity, error mapping, side effects,
  retries, idempotency, cancellation, partial progress, and recovery;
- positive, negative, concurrency, migration, and user-journey behavior.

Provide typed stubs for new or materially changed interfaces and data
structures. Provide pseudocode for consequential orchestration, transactions,
state transitions, side-effect order, and error mapping. Mark behavioral
semantics as normative and incidental syntax or layout as illustrative.

Do not reproduce full source bodies or prescribe inconsequential syntax. Do
name expected files, symbols, test locations, and implementation stages when
they constrain scope or reduce Terra/Luna inference.

## Compile decisions into tasks

Treat task generation as compilation:

```text
intent + repository evidence + decisions + constraints + verification
    -> executable Terra/Luna task packets
```

Every plan contains at least one complete implementation task.

- Create one separate `T1` packet for a small cohesive change.
- Default medium work to two through five dependency-ordered vertical tasks.
- Use more tasks only when scale, risk, context size, or independently
  verifiable outcomes justify them.
- Materialize every task as a separate file under the canonical plan
  directory.
- Allow sequential tasks to touch the same files when dependencies require it.
  Require non-overlapping write sets only for parallel execution.

Use `$wyrd-implement` for execution and validation. Each task must be directly
usable as a bounded-mode assignment. The parent plan owns feature-wide
decisions, task ordering, requirement traceability, and closeout.

## Design verification before handoff

Map every requirement to implementation tasks and objective proof. For every
task, define the affected dependency surface, required tests, exact focused
commands, feature sets, excluded broad commands, and structured completion
evidence. Distinguish task completion from feature completion.

Use current `mise.toml` task names. Keep Cargo-backed commands sequential
across agents sharing a checkout or target directory. Use default or exact
features for tasks and milestones. Reserve the all-feature workspace gate for
integrated closeout. Require a real client-to-server journey for every new
user- or agent-facing capability.

## Audit readiness

Load the plan and task formats and audit the complete handoff. A plan is ready
only when:

- every requirement maps to at least one task and objective verification;
- every task is small enough for its assigned Terra/Luna execution model;
- every task names exact scope, target owners, code structure, acceptance
  criteria, tests, commands, features, exclusions, escalation, and evidence;
- required interfaces, stubs, pseudocode, control flow, failures, and edge
  cases remove material implementation choices;
- dependencies and execution order leave the repository coherent;
- focused checks and closeout gates match live repository commands;
- `$wyrd-implement` can execute each task without returning to design.

Use a risk-tiered independent review:

- Require `$wyrd-plan-reviewer` for wire or persisted contracts, migrations,
  tenant/auth/policy/audit boundaries, cross-store correctness,
  distributed/background behavior, broad ownership changes, or public
  workflows spanning multiple first-class surfaces.
- Use the complete readiness self-audit for lower-risk plans unless the user
  requests independent review.

Do not mark a risk-gated plan `Approved` until the reviewer returns `Approve`.
Revise the canonical artifact and re-review blocking findings.

For a materialized plan, run:

```bash
python .codex/skills/wyrd-plan/scripts/validate_plan_artifacts.py \
  .dev/plan/<slug>
```

Do not hand off artifacts until the schema validator passes.

## Present or materialize

Follow the active collaboration mode.

- In a non-mutating context, return one `<proposed_plan>` block with explicit
  `<!-- artifact: implementation-plan.md -->` and
  `<!-- artifact: tasks/<NN>-<task-slug>.md -->` markers. Include the complete
  contents of every artifact after its marker.
- When the user authorizes a durable plan, save it to
  `.dev/plan/<slug>/implementation-plan.md`.
- Save every task under
  `.dev/plan/<slug>/tasks/<NN>-<task-slug>.md`.

Use `Draft`, `Review Required`, or `Approved` for plans and `Blocked`,
`Planned`, or `Ready` for tasks. Only tasks compiled from an approved plan may
be marked `Ready`.

Do not create `tasks.yaml`, duplicate specs, implementation ledgers, run
directories, or completion sentinels. Finish with the artifact paths, approval
status, task inventory, and any genuinely unresolved blocker.
