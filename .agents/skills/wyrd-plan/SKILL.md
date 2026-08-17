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

### Use bounded evidence scouts when breadth exceeds symbol tracing

CodeGraph remains the first choice for locating named symbols, reading exact
implementations, and tracing callers or dispatch. It is not a substitute for
broad repository inventory or comparative summarization.

When a concrete evidence gap spans an unfamiliar or multi-owner surface, the
planner may dispatch one through three read-only Luna agents at medium
reasoning effort. Dispatch only when the result has a clear deliverable, such
as:

- inventorying current owners, entry points, manifests, tests, and fixtures;
- finding the nearest two or three implementation precedents;
- mapping public projections and generated artifacts across Rust, Python,
  TypeScript, HTTP, CLI, MCP, or UI; or
- summarizing configuration, feature, migration, or verification conventions.

Give each scout one bounded question and an explicit repository scope. Require
an evidence-only report: verified facts with file or symbol references,
relevant tests or commands, contradictions, and unknowns. Scouts do not edit,
propose architecture, assign tasks, or resolve material decisions.

The parent planner synthesizes scout outputs, verifies every fact that affects
a contract, ownership boundary, security, tenancy, persistence, or acceptance
criterion, and remains solely responsible for the impact graph and plan. Do
not dispatch scouts for a single named-symbol question, a localized change
with an obvious precedent, or judgment-heavy design work.

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

Per-task command preflight is necessary, but material readiness is granted only
by the current cohesive-milestone rehearsal; local preflight alone never makes
a task dispatchable.

1. Inspect every proposed `mise`, Cargo, package, or script command.
2. Require named tests or journeys, explicit affected packages, and default
   features or exact non-default features earned by the behavior under test.
3. Confirm the named task, package, feature, target, filter, fixture, support
   export, and repository setup exist.
4. Run the narrow command when feasible. At minimum compile the exact target
   and feature selection and prove the filter selects the intended tests.
5. Include only affected codegen, docs, typing, migration, or boundary checks.
6. Identify repository-provided services, migrations, environment, and
   checked-in local test configuration required at execution time.
7. Record unavailable external infrastructure without presenting runtime proof
   as passed.

Task packets must not contain workspace, crate-family, aggregate, full-language,
canonical journey/cluster/fuzz matrix, or generic all-feature verification.
Those lanes belong exclusively to parent closeout unless an exact broad lane is
itself the acceptance contract and the packet uses the structured,
requirement-bound exception in `references/verification-planning.md`. Inspect
every chained, piped, continued, prompted, environment-prefixed, or recursively
wrapped shell segment; normalize supported CLI shorthand, reject executable
substitutions and unauditable aliases/functions, and do not trust opaque
repository scripts merely because their path is local.

Do not substitute a nearby command without recording the corrected command in
the task. See `references/verification-planning.md`.

## Rehearse implementation cold

After drafting or materially revising a cohesive milestone's material
contracts or dependency order, invoke `$wyrd-cold-rehearsal` in a fresh
subagent using only the milestone task packets, parent plan, repository,
accepted source/predecessor identities, and normal authorities. An evidence
scout is not a rehearsal agent. Do not provide planning conclusions, suspected
omissions, prior rehearsal findings, or the intended solution.

The rehearsal must simulate material construction and lifecycle, not review
prose or prescribe private implementation. For production allocations that can
scale materially with input, workload, concurrency, retries, or elapsed work,
it proves the pre-allocation facts or configured ceiling, authoritative owner,
simultaneous-live-set bound, transfer and terminal release semantics,
observable enforcement, and verification. Plans need not choose private guard
types, helpers, containers, iterators, or allocation APIs when multiple
repository-native implementations enforce the same invariant. A guessed
production bound, unbounded workload-scaled path, hidden material copy, or
missing authority is `FAIL`; bounded metadata and test-harness mechanics are
implementation work.

Revise the affected milestone and rerun the skill from fresh context until it
returns `PASS` for the current material packet/plan digests and accepted
predecessors. Tasks in that milestone cannot become `Ready` with missing,
stale, self-authored, conditional, or failed milestone rehearsal evidence.
Private helper, adapter, fixture, local-layout, or equivalent non-weaker
command corrections do not trigger a rerun. Document-only architecture review
and the structural validator do not replace this gate.

Readiness, qualification, sealing, and promotion claims are limited to the
explicitly named language/runtime surfaces and journeys. Evidence for one
runtime never implies readiness for an omitted projection; omitted surfaces
remain unproved rather than blocking a deliberately scoped milestone.

After a `FAIL`, do not patch only the first cited line and immediately rerun.
First perform a bounded remediation audit of the entire root-cause cluster:
the affected owner and constructor, sibling allocations and phase guards,
direct callers and cross-crate handoffs, exhaustive error projections,
cancellation/retry/partial/shutdown/restart paths, dependent packet contracts,
and the commands intended to prove them. Resolve every concrete issue in that
cluster, validate the revised artifacts, and only then start a fresh rehearsal.

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

For persisted artifacts, require the caller to supply absolute `REPO_ROOT` and
`PLAN_PATH`. Never discover a destination plan or task by searching the
filesystem. Record the current checkout as `Repository origin` in canonical
`host/owner/repository` form and `Repository revision` as `git rev-parse HEAD`.
Use those same two values in every task packet.

Load both format references and confirm:

- every requirement maps to implementation and objective proof;
- the impact graph covers the affected dependency cone;
- all material choices have one answer;
- every command and setup requirement was execution-checked;
- every cohesive milestone passed a fresh `$wyrd-cold-rehearsal` bound to its
  current material packet digests, source revision, and predecessor contracts;
- task dependencies leave coherent repository states;
- required independent review passed for risk-gated changes.

Run the structural and focused-verification validator for materialized plans:

```bash
python .agents/skills/wyrd-plan/scripts/validate_plan_artifacts.py \
  "$(dirname "$PLAN_PATH")"
```

The validator proves artifact structure and rejects semantically overbroad task
commands. It does not replace executable preflight or cold rehearsal.

In non-mutating contexts, return one `<proposed_plan>` block with complete
artifact markers. When authorized, save the plan under
`$PLAN_PATH` and each task under `$(dirname "$PLAN_PATH")/tasks/`.
Use `Approved` only after readiness and required review; use `Ready` only for
tasks in an approved plan whose cohesive milestone passed executable preflight
and cold rehearsal.
