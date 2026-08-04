---
name: wyrd-implement
description: Execute or resume exactly one active, decision-complete non-UI Wyrd implementation task through verified completion or a genuine material-authority blocker in Rust, Python, TypeScript, PyO3, contracts, SDKs, server, CLI, MCP, registry, storage, Skald, Vala/Bifrost, codegen, or cross-language tests. Use when a current user instruction or approved task assigns one bounded change. Persist through compile, lint, test, fixture, command, and repository-managed local-environment failures. Do not use to plan, decompose, close an entire implementation plan, or implement Svelte UI work.
---

# Wyrd Implement

Execute exactly one approved task through `COMPLETE` or a genuine material
`BLOCKED` condition. Own reversible implementation mechanics. Do not redesign
requirements or material boundaries.

Use one fixed loop:

```text
orient -> localize -> implement -> validate -> refine
             ^                                  |
             +----------------------------------+
```

Test, compiler, formatter, Clippy, rustdoc, command, fixture, and local
environment failures are refinement inputs. They are not terminal conditions
by themselves.

## Load the task and repository

Before editing:

1. Read the active task, referenced approved-plan context, and applicable
   `AGENTS.md` files completely.
2. For a standardized Wyrd task, read
   `.codex/skills/wyrd-plan/references/task-packet-format.md` and validate its
   parent plan directory. The task must be `Ready`.
3. Read `architecture/agent-rules.md` and `architecture/wyrd-design.md`. Read
   `architecture/wyrd-doctrine.mdx` before changing Wyrd contracts, APIs,
   SDKs, CLI, MCP, docs, generated artifacts, or behavior.
4. Read
   `architecture/references/languages/implementation-execution.md`; it defines
   material boundaries, verification equivalence, local-environment recovery,
   living-task updates, test integrity, and completion evidence.
5. Read `architecture/references/README.md`, then load only the other
   architecture references required by the affected surface using the routing
   table below. Read every selected reference completely and state the
   decision it governs before editing.
6. Inspect Git status and preserve unrelated user changes.
7. Inspect `mise.toml`, manifests, package configuration, and lockfiles before
   relying on commands, features, dependencies, or environment.
8. When `.codegraph/` exists, use CodeGraph before grep, find, or manual source
   discovery.

Build a checklist from required changes, acceptance criteria, required tests,
focused commands, and completion evidence. Continue until every item is
checked or a material boundary prevents it.

When resuming after review, require an active orchestrator-owned `Remediation
revision RR<N>` in the canonical task. Treat the original task plus that
revision as the executable assignment. The revision must be decision-complete
at the same level as the original task: exact correction, owners and symbols,
interfaces, consequential pseudocode, edge behavior, tests and assertions,
verification, scope, prohibited fixes, escalation boundaries, and finding
closure. Findings, review prose, a diff, or a desired outcome alone are not an
implementation assignment. Return `BLOCKED` for missing specification before
guessing at the fix.

### Architecture reference routing

| Reference | Load when the task touches |
|---|---|
| `architecture/references/languages/implementation-execution.md` | Every task: authority boundaries, verification recovery, test integrity, diff audit, and completion evidence |
| `architecture/references/doctrine/positioning-and-vocabulary.md` | Card vocabulary, `CardRef`, v1 kinds, or removed concepts |
| `architecture/references/doctrine/architecture-constraints.md` | Tier boundaries, deployment, or observation identity |
| `architecture/references/architecture/patterns.md` | Crate placement and server/client/storage/provider/audit patterns |
| `architecture/references/languages/rust-core.md` | Rust ownership, traits, async, allocation, and idioms |
| `architecture/references/languages/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, conversions, and registration |
| `architecture/references/languages/errors.md` | Stable errors and boundary mappings |
| `architecture/references/languages/python-api-and-stubs.md` | Python exports, stubs, package layout, and tests |
| `architecture/references/languages/testing-workflows.md` | Test tiers, verification levels, and boundary checks |
| `architecture/references/languages/agent-harness.md` | MCP and other agent-facing contracts |
| `architecture/references/languages/typescript-guide.md` | `@wyrd/sdk` and napi conventions |
| `architecture/references/domain/iceberg-bifrost.md` | Bifrost, Iceberg, DataFusion, and object storage |

When an approved task explicitly supersedes a Wyrd design decision, update the
named design authority with the implementation and update doctrine when the
principle changes. Otherwise follow current design and classify an implicit
conflict as material.

## Enforce the material boundary

Classify every mismatch before deciding whether to continue:

| Class | Examples | Authority |
|---|---|---|
| Local and reversible | Private helpers, rustdoc, formatting, diff-caused Clippy, mechanical callers, existing fixtures | Implement, fix, and rerun |
| Bounded correction | Equivalent command/filter, current private symbol, adjacent fixture inside the owner, repository-managed local environment, small touched-file lint | Proceed, record in the task, and continue |
| Material | Public/wire/persisted contract, migration or destructive behavior, dependency or Cargo feature, auth/tenancy/policy/audit semantics, ownership redesign, changed acceptance outcome | Stop before the change and request authority |

Expected paths and private symbol names are not a strict whitelist. Add an
adjacent private implementation or test-support file inside the established
owner when it is the smallest way to satisfy acceptance. Do not cross a
prohibited material boundary.

Exact verification commands are recipes unless the task explicitly makes the
exact lane, feature set, or environment normative. A defective recipe may be
replaced with equivalent non-weaker proof.

## Execute the loop

### Orient

- Confirm objective, requirements, non-goals, allowed and prohibited scope,
  material contracts, acceptance criteria, tests, features, and commands.
- Reconstruct prior progress from the task, current diff, and verification
  evidence after any interruption or context compaction.
- For remediation, map every active finding to its prescribed correction and
  closure assertion. Stop before editing when any finding lacks a
  decision-complete remediation revision or leaves a material choice open.

### Localize

- Locate current owners, callers, consumers, dispatch, fixtures, and tests.
- Prefer current repository reality over stale private paths or helper names.
- Reuse existing owners, abstractions, errors, fixtures, and support exports.

Before editing Rust, identify the owning concrete struct or domain type,
earned async boundaries, nearest structural precedent, and rustdoc obligations
for every touched item.

### Implement

- Make the smallest cohesive in-scope change for the task at hand.
- Preserve normative behavior, operation order, transactions, concurrency,
  cancellation, side effects, error mapping, and invariants.
- Follow `AGENTS.md`; do not duplicate its language, ownership, or test rules
  here.
- Write required tests with the implementation.
- Do not implement later tasks, speculative cleanup, new architecture, or
  unapproved material changes.

### Validate

Run focused verification sequentially over the smallest complete affected
surface. Inspect `mise` tasks before using them. Prefer the repository task
that owns setup; use direct Cargo only for a narrow pure test or exact focused
filter that needs no missing repository setup.

### Refine

Route every failure:

1. **Caused by the diff:** diagnose, fix, and rerun.
2. **Small defect in touched code:** fix and report as incidental.
3. **Defective command, filter, feature, or lane:** derive equivalent
   non-weaker proof, update the task evidence, and rerun.
4. **Missing private plumbing or fixture:** add the smallest adjacent support
   inside the established owner and continue.
5. **Missing repository-managed local setup:** inspect `mise.toml` and setup
   scripts, start services, run migrations, use checked-in local test values,
   and rerun.
6. **Proven unrelated baseline failure:** record the proof and continue every
   unaffected implementation and verification item. Fix if instructed.
7. **Material change required:** stop before it and return `BLOCKED` with
   repository evidence and the authority needed.
8. **Uncertain classification:** gather more source and command evidence; do
   not stop merely because diagnosis is incomplete.

Do not create a remediation plan for classes 1–6.

## Recover repository-managed test environments

Missing local test variables or services are mechanical when the repository
defines them.

1. Inspect the relevant `mise` task, its `depends`, `env`, and invoked scripts.
2. Run canonical setup and migration tasks.
3. Invoke the owning `mise` task, or apply its checked-in local-only test
   values to an equivalent focused command.
4. Retry the intended test.

Never invent, print, or persist production credentials. A genuinely
unavailable external credential or protected service may block its mandatory
proof only after all remaining work and verification complete.

## Update the living task

Record bounded corrections in the active task:

- corrected repository facts, private paths, or symbol names;
- existing or added private fixtures and support files;
- equivalent commands and why their proof is not weaker;
- progress, diagnoses, and verification evidence.

Do not edit requirements, public or persisted contracts, security semantics,
material architecture, data-loss behavior, or acceptance outcomes. A required
change to those fields is `BLOCKED`, not a task rewrite.

## Protect test integrity

Never pass a gate by weakening assertions, adding sleeps for synchronization,
ignoring tests, adding unjustified lint allowances, mocking away required
behavior, swallowing errors, or changing production semantics to satisfy an
incorrect test. Fix the cause or identify a material conflict.

## Finish only at a terminal outcome

Elapsed time, difficulty, partial progress, context length, recoverable tool
failure, missing local setup, an invalid command, or remaining in-scope work
are not terminal.

- `COMPLETE`: every acceptance criterion and required proof is satisfied.
- `BLOCKED`: no in-scope solution remains without a material change or
  genuinely unavailable authority.

Before reporting, inspect tracked and untracked changes, map every changed file
to acceptance, remove accidental artifacts, and run `git diff --check`.
Use the structured completion report in
`architecture/references/languages/implementation-execution.md`.
