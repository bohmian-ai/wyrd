---
name: wyrd-implement
description: Execute or resume exactly one active, decision-complete non-UI Wyrd implementation task in Rust, Python, TypeScript, PyO3, contracts, SDKs, server, CLI, MCP, registry, storage, Skald, Vala/Bifrost, codegen, or cross-language tests. Use when a current user instruction or approved task assigns one bounded change. Do not use to plan, decompose, or close an entire implementation plan; use wyrd-implement-plan instead. Do not use for Svelte UI work; use wyrd-ui instead.
---

# Wyrd Implement

Implement exactly one approved task as written. Optimize for faithful
execution, the smallest reviewable diff, repository consistency, objective
verification, and clear escalation. Do not act as a secondary planner.
Every execution rule below is mandatory; do not trade it for speed,
convenience, or agent autonomy.

## Enforce the task boundary

Treat the current user instruction or named task file as the single active
task. Do not implement later tasks, adjacent milestones, or whole-plan
closeout. Do not alter the task or plan to match the implementation.

Use this authority order when instructions conflict:

1. current user instructions;
2. active task;
3. approved plan;
4. applicable `AGENTS.md` files;
5. repository architecture and conventions;
6. local implementation preferences.

The task's behavior, acceptance criteria, non-goals, prohibited changes,
interfaces, feature requirements, verification, and escalation conditions are
authoritative.

An approved task may intentionally improve Wyrd design. When it explicitly
replaces a decision, name the superseded decision and update
`architecture/wyrd-design.md` before or with the implementation. Update
`architecture/wyrd-doctrine.mdx` when the principle changes. Otherwise follow
current design over implementation drift. Stop when the conflict is implicit,
ambiguous, unsafe, or outside the approved task.

## Load complete instructions

Before editing:

1. Read the active task to EOF. Never rely on a partial excerpt or
   conversational summary.
2. When the task was produced by `$wyrd-plan`, read
   `.codex/skills/wyrd-plan/references/task-packet-format.md` to EOF and
   run `.codex/skills/wyrd-plan/scripts/validate_plan_artifacts.py` against the
   task's parent plan directory. Every required heading must exist in order; an
   inapplicable section must use
   `Not applicable: <one-sentence reason>`. Stop before source inspection when
   validation fails or the packet is not marked `Ready`.
3. Read the referenced plan sections and all applicable `AGENTS.md` files to
   EOF.
4. Read `architecture/agent-rules.md` and `architecture/wyrd-design.md` to EOF.
5. Read `architecture/wyrd-doctrine.mdx` to EOF before changing contracts,
   APIs, SDKs, CLI, MCP, docs, generated schemas, or behavior.
6. Read
   `architecture/references/languages/implementation-execution.md` to EOF; it
   is the mandatory detailed execution contract for every task.
7. Extract every standardized task field: metadata, objective, context,
   required changes, non-goals, allowed and prohibited scope, target paths and
   symbols, required types and interfaces, implementation guidance, control
   flow, failure cases, acceptance criteria, required tests and features,
   focused verification, excluded commands, escalation conditions, and
   completion evidence.
8. Inspect Git status and preserve unrelated user changes.
9. Inspect `mise.toml`, manifests, `pyproject.toml`, and lockfiles before
   relying on commands, dependencies, or feature behavior.

Load only applicable architecture references, but read every selected file to
EOF and report the reference plus the decision it governs before editing:

| Reference | Load when the task touches |
|---|---|
| `architecture/references/languages/implementation-execution.md` | every task; mandatory contract, escalation, verification, diff audit, completion report |
| `architecture/references/doctrine/positioning-and-vocabulary.md` | Card vocabulary, `CardRef`, v1 kinds, deleted concepts |
| `architecture/references/doctrine/architecture-constraints.md` | tier boundaries, deployment, observation identity |
| `architecture/references/architecture/patterns.md` | crate placement and server/client/storage/provider/audit patterns |
| `architecture/references/languages/rust-core.md` | Rust ownership, traits, async, allocation, idioms |
| `architecture/references/languages/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, conversions, registration |
| `architecture/references/languages/errors.md` | stable errors and boundary mappings |
| `architecture/references/languages/python-api-and-stubs.md` | Python exports, stubs, package layout, tests |
| `architecture/references/languages/testing-workflows.md` | test tiers, verification levels, boundary checks |
| `architecture/references/languages/agent-harness.md` | MCP and agent-facing contracts |
| `architecture/references/languages/typescript-guide.md` | `@wyrd/sdk` and napi conventions |
| `architecture/references/domain/iceberg-bifrost.md` | Bifrost, Iceberg, DataFusion, object storage |

## Inspect focused repository reality

When `.codegraph/` exists, use `codegraph_explore` before grep, find, or manual
file-reading loops. Inspect only the affected implementation:

- locate current code paths, callers, and tests;
- confirm named files, crates, types, and commands exist;
- confirm required interfaces or types do not already exist;
- identify naming, ownership, and structural precedents;
- inspect default and optional Cargo features;
- inspect what every proposed `mise` task executes;
- identify contradictions between the task and current source.

Do not turn task inspection into a repository-wide architecture review.

Before editing Rust, report the owning concrete struct, enum, or newtype; its
state, dependencies, identity, and invariants; its public methods and private
workflow stages; justified pure free functions; sync and earned async
boundaries; rustdoc coverage for every touched item; and the nearest
struct-centered Wyrd precedent. Do not edit Rust until this gate is complete.

## Validate executability

Proceed only when the behavior is clear, the architectural owner exists, the
contracts fit the approved design, dependencies and features are available,
and acceptance criteria fit the allowed scope.

For a standardized `$wyrd-plan` task, report a concise pre-edit contract check:

- schema: valid or invalid;
- task status and dependency readiness;
- objective and mapped requirement/decision IDs;
- allowed and prohibited write scope;
- target owners, symbols, required interfaces, and normative pseudocode;
- acceptance criteria, required tests/features, focused commands, and excluded
  commands;
- escalation conditions and completion evidence.

Do not repair, reinterpret, or silently complete a malformed task. Return it to
planning.

Stop before editing and report a blocker when:

- a required owner, type, command, or feature is missing and adding it changes
  architecture or task scope;
- a public contract, migration, security boundary, test, or acceptance
  criterion conflicts with the task;
- a new or modified dependency or undocumented feature is required;
- prohibited files must change;
- focused verification must expand materially;
- later tasks must change to complete this task;
- repository behavior materially contradicts an approved decision.

Do not silently reinterpret the task. Follow the full deviation protocol in
`architecture/references/languages/implementation-execution.md`; stop before
implementation and wait for replanning or explicit approval. Do not escalate
minor local choices that preserve semantics and established patterns.

## Implement the required design

Treat behavioral semantics as normative. Treat pseudocode according to its
declared status:

- preserve normative semantics;
- adapt exact names only when the task permits repository alignment;
- adapt illustrative structure without changing behavior;
- never change a public interface without explicit permission.

Preserve operation order, validation boundaries, transactions, concurrency,
cancellation, side-effect order, error mapping, and data invariants.

Prefer existing owners, abstractions, helpers, errors, fixtures, dependencies,
and repository patterns. Do not add architectural layers, generic frameworks,
broad abstractions, feature flags, dependencies, helpers, extension points, or
refactors not required by acceptance criteria.

Map every changed file to a task requirement or necessary verification support.
Do not reformat, rename, upgrade, clean up, or modify tests outside that map.
Never hand-edit generated artifacts; change the source or generator and
regenerate.

Keep Wyrd ownership aligned with `AGENTS.md`:

- `wyrd-spec` owns pure contracts and remains IO-, async-, SQL-, and PyO3-free;
- `crates/shared/*` owns reusable client, runtime, auth, telemetry, registry,
  and testing foundations;
- Skald owns provider and reusable agent runtime behavior;
- Vala owns observability, evaluation, drift, and analytical data-plane work;
- `crates/wyrd/*` owns server, CLI, MCP, storage, testing, and application
  integration;
- language bindings and `python/py-wyrd` remain thin projections.

Apply every risk-specific rule in
`architecture/references/languages/implementation-execution.md`. Load the
conditional language and domain references above for the task's affected
surfaces; they refine the general contract without weakening it.

## Prove behavior with tests

Write tests alongside behavior. Map every new or changed test to an acceptance
criterion and cover required success, regression, failure, boundary, error,
state-transition, concurrency, compatibility, and Wyrd user-journey behavior.
Do not add tests solely for line coverage.

Never make a test pass by removing or broadening assertions, adding sleeps
instead of synchronization, ignoring or disabling cases, mocking away the
behavior under test, swallowing errors, changing production behavior to match
an incorrect test, or replacing precise assertions with snapshots or existence
checks. Report conflicts between existing tests and the approved task.

## Run focused verification

Follow task-provided commands and the ordered verification contract in
`architecture/references/languages/implementation-execution.md`.

- Run only focused verification over the smallest complete affected surface.
- Inspect every `mise` task before use; never invent a task name.
- Run Rust-related commands sequentially across every agent sharing the
  checkout or target directory.
- Use default features unless the task earns exact optional features.
- Never use `--all-features` for bounded implementation unless the task
  explicitly requires and justifies it.
- Leave workspace-wide and all-feature verification to
  `$wyrd-implement-plan` closeout.

## Inspect the complete diff

Apply the final diff audit in
`architecture/references/languages/implementation-execution.md` to tracked and
untracked changes. Remove accidental or unrelated changes. Update only task
fields or logs the task explicitly permits.

## Report evidence

Apply the exact completion standard and structured report in
`architecture/references/languages/implementation-execution.md`. Do not report
`COMPLETE` while any acceptance criterion is failed or unverified. Use
`INCOMPLETE` when work remains and `BLOCKED` when escalation prevents correct
completion.
