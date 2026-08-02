# Executable Wyrd Task Packet

Use this exact ordered format for every task saved under
`.dev/plan/<slug>/tasks/<NN>-<task-slug>.md`. A task is a bounded compilation
target for its assigned Luna, Terra, or Sol model and a direct assignment for
its declared surface-appropriate execution skill or skill set.

## Contents

- [Required structure](#required-structure)
- [Detail rules](#detail-rules)
- [Controlled execution updates](#controlled-execution-updates)
- [Complete example](#complete-example)
- [Task readiness audit](#task-readiness-audit)

## Required structure

Use these headings in this order. Do not rename, reorder, merge, or omit them.
When a section is inapplicable, write
`Not applicable: <one-sentence reason>`.

```markdown
# T<N>: <Outcome-oriented title>

Status: Planned | Ready | Complete | Blocked
Plan: <canonical plan path>
Milestone: <ID or none>
Requirements: <R IDs>
Decisions: <D IDs>
Depends on: <task IDs or none>
Assigned model: Luna | Terra | Sol
Execution skill: $wyrd-implement | $wyrd-ui | $wyrd-implement + $wyrd-ui

## Objective

One cohesive observable implementation result.

## Context

Only the current behavior, existing seams, and approved decisions needed to
execute this task without reconstructing the full plan.

## Required changes

Ordered implementation outcomes.

## Non-goals

Adjacent behavior this task must not implement.

## Allowed scope

Expected paths, crates, packages, modules, tests, and conditionally allowed
support files.

## Prohibited changes

Public surfaces, dependencies, features, refactors, generated files, or later
tasks that must remain untouched.

## Target paths and symbols

Existing owners, methods, callers, tests, and generated seams. State the
responsibility of every new or materially changed symbol.

## Required types and interfaces

Typed stubs, field and variant semantics, method contracts, visibility,
invariants, and error behavior. Mark normative semantics and illustrative
syntax.

## Implementation guidance

Concrete ordering, repository precedents, ownership constraints, and local
mechanics that must or must not be used.

## Control flow and pseudocode

Normative pseudocode for consequential orchestration or state changes.

## Failure and edge cases

Task-specific negative, duplicate, concurrent, partial-progress, migration,
cancellation, and recovery behavior.

## Acceptance criteria

Stable `AC1`, `AC2`, and subsequent observable results.

## Required tests

Named test cases, tier, setup, action, and critical assertions.

## Required features

Default or optional Cargo/language features and the reason each non-default
feature is required.

## Focused verification

Affected dependency surface and exact sequential commands.

## Commands explicitly excluded

Broad, all-feature, unrelated, or closeout commands this bounded task must not
run.

## Stop and escalate if

Repository evidence or implementation needs that invalidate approved scope or
decisions.

## Completion evidence

Required structured report from the implementation agent.
```

A pure synchronous refactor may state “Not applicable: control flow and
failure state are unchanged” rather than inventing a failure matrix.

Select execution skills from the write set:

- `$wyrd-implement` for non-UI Wyrd implementation;
- `$wyrd-ui` for Svelte/UI-only implementation;
- `$wyrd-implement + $wyrd-ui` for one cohesive task that crosses non-UI and
  UI surfaces.

Do not assign a skill whose trigger or exclusions reject part of the write set.

Assign the risk tier. The `Assigned model:` field name is retained for schema
compatibility, but its value is a **risk tier**, not a model selection:

- Luna for mechanical, low-risk tasks with one established repository pattern;
- Terra for ordinary implementation and review work;
- Sol for security, public or persisted contracts, migrations, concurrency,
  cross-owner or cross-language work, and other materially high-risk tasks.

The tier governs task scope, reviewer rigor, and how quickly the orchestrator
escalates on repeated failure. It does not choose a model — implementation
agents run on Sonnet and reviewers on Opus at every tier. See
`.claude/skills/wyrd-implement-plan/references/model-routing.md`.

## Detail rules

- Name likely files and exact existing symbols after verifying them from live
  source. Use globs only when a generated or migration family is truly open.
- Treat expected paths and private symbol names as repository-verified
  guidance, not a strict whitelist. Permit adjacent private implementation and
  test-support files inside the established owner when acceptance requires
  them.
- Define new and materially changed symbols. Do not ask the assigned model to
  “add the necessary types” or “wire up the handler.”
- State function and method responsibilities, not just names.
- Include typed stubs when shape matters. Allow naming adaptation only to match
  an identified local convention.
- Include pseudocode when ordering, state, IO, error mapping, concurrency, or
  side effects matter.
- Turn edge cases into acceptance criteria and required tests.
- Name exact current `mise` commands and required feature sets.
- Mark an exact command normative only when its lane, feature set, or
  environment is part of the behavior being proved. Otherwise permit an
  equivalent non-weaker command when the prescribed recipe is defective.
- Explicitly exclude broad gates from bounded tasks when they belong to
  closeout.
- Escalate unexpected dependency, feature, migration, public contract, scope,
  ownership, or verification expansion.

## Controlled execution updates

An executing agent may append corrections under `Completion evidence` and
update factual mechanics in `Context`, `Target paths and symbols`,
`Implementation guidance`, or `Focused verification` when current repository
evidence establishes:

- a corrected internal path or private symbol name;
- an existing equivalent helper, fixture, or support export;
- a required adjacent private helper or test fixture within the established
  owner;
- an equivalent non-weaker verification command;
- progress, failure diagnosis, and verification results.

Record each correction with its evidence and classify it as `local` or
`bounded correction`. Do not create a remediation plan for these changes.

Do not modify the objective, requirement or decision IDs, required behavioral
outcomes, public or persisted contracts, security or tenancy semantics,
data-loss behavior, prohibited material boundaries, or acceptance criteria.
When one must change, set the task to `Blocked`, preserve the conflict
evidence, and request authority before implementation.

## Complete example

This example demonstrates the required implementation density. Its symbols and
paths are illustrative and must be verified before use in a real plan.

```markdown
# T1: Return a typed conflict from workspace hydration

Status: Ready
Plan: .dev/plan/hydration-conflict/implementation-plan.md
Milestone: M1
Requirements: R1, R2, R3
Decisions: D1
Depends on: None
Assigned model: Terra
Execution skill: $wyrd-implement

## Objective

Make workspace hydration reject incompatible duplicate exact Card identities
with the established typed conflict while keeping identical duplicates
idempotent and preserving successful hydration.

## Context

`WorkspaceHydrator::hydrate` owns the synchronous hydration workflow.
Insertion currently discovers duplicates after the exact identity is derived.
Relationship derivation runs only after all cards enter the workspace index.
The nearest tests are the inline hydration tests covering successful multi-card
workspaces.

## Required changes

1. Detect an existing exact identity before replacing or reusing its entry.
2. Reuse the existing entry when immutable spec hashes match.
3. Return the stable hydration conflict when hashes differ.
4. Preserve relationship derivation order and successful output.
5. Add focused regressions for equal and conflicting duplicates.

## Non-goals

- Do not change Card identity or version semantics.
- Do not add a registry or storage lookup.
- Do not change public wire schemas.
- Do not refactor unrelated hydration stages.

## Allowed scope

Expected:

- `crates/shared/wyrd-registry/src/hydrate/workspace.rs`
- Its existing inline or nearest hydration test module

Conditionally allowed:

- The owning registry error module only if the established conflict variant
  requires a missing conversion.

Affected crate:

- `wyrd-registry`

## Prohibited changes

- No new dependency or Cargo feature.
- No async conversion.
- No changes to server routes, SDK surfaces, or generated artifacts.
- No broad workspace refactor.
- No `--all-features` task test.

## Target paths and symbols

- Modify `WorkspaceHydrator::hydrate`; it remains the orchestration owner.
- Reuse the existing exact-identity key and immutable spec-hash accessors.
- Add a narrow private helper only if comparison and error construction would
  otherwise obscure the hydration loop.
- Keep relationship derivation unchanged and after successful insertion.

## Required types and interfaces

No new public type.

Illustrative helper shape; behavior is normative:

    fn classify_duplicate(
        existing: &HydratedCard,
        incoming: &HydratedCard,
    ) -> Result<DuplicateDisposition, RegistryError>

`DuplicateDisposition` need not be introduced if the existing control flow is
clearer as a direct comparison. Required semantics:

- equal immutable hashes => reuse;
- unequal immutable hashes => established typed conflict containing the exact
  identity;
- no mutation before conflict return.

## Implementation guidance

- Keep the workflow synchronous.
- Compare through existing typed identity/hash APIs; do not compare serialized
  JSON or debug strings.
- Construct the error through the established Wyrd error path.
- Preserve existing insertion and relationship behavior for non-duplicates.

## Control flow and pseudocode

    for incoming in cards:
        identity = incoming.exact_identity()
        existing = workspace.get(identity)

        if existing is absent:
            workspace.insert(incoming)
            continue

        if existing.spec_hash == incoming.spec_hash:
            continue

        return typed_hydration_conflict(identity)

    derive_relationships(workspace)
    return workspace

The comparison and “no mutation before conflict” semantics are normative.
Exact helper extraction is illustrative.

## Failure and edge cases

| Condition | Expected behavior |
|---|---|
| Identity absent | Insert normally |
| Same identity and immutable hash | Reuse without duplicate relationship work |
| Same identity and different hash | Return typed conflict; no workspace result |
| Conflict before later cards | Stop immediately; do not derive relationships |

## Acceptance criteria

- AC1. Conflicting duplicate exact identities return the established typed
  conflict with the identity represented in its structured fields.
- AC2. Identical duplicates hydrate successfully as one logical entry.
- AC3. Non-duplicate hydration and relationship derivation remain unchanged.
- AC4. The implementation adds no dependency, feature, async boundary, or
  public schema change.

## Required tests

- `conflicting_duplicate_returns_typed_error`: arrange two cards with the same
  exact identity and different immutable hashes; assert the precise error
  variant/code and identity.
- `identical_duplicate_is_idempotent`: arrange the same card twice; assert one
  logical hydrated entry and successful relationships.
- Run the existing successful multi-card hydration test unchanged to prove
  AC3.

Use unit tests because this task changes synchronous in-memory workflow and no
public client/server behavior.

## Required features

- Default features only.
- Do not enable optional registry, server, Python, or test-harness features.

## Focused verification

Run sequentially:

1. `mise exec -- cargo test --locked -p wyrd-registry hydrate -- --nocapture --test-threads=1`
2. `mise run test:shared`
3. `mise run fmt`
4. `mise run check:default`
5. `git diff --check`

## Commands explicitly excluded

Do not run:

- `mise run pre-pr`
- `mise run check`
- unrelated Python, TypeScript, storage, or server suites
- concurrent Cargo commands

The parent plan owns the all-feature closeout.

## Stop and escalate if

- Current source has no stable exact-identity or immutable-hash seam.
- The conflict requires a new public error instead of an existing mapping.
- Correctness requires changing Card identity or relationship semantics.
- Focused verification requires a non-default feature or another crate family.

## Completion evidence

Return:

- Status: COMPLETE or BLOCKED.
- AC1–AC4 as PASS, FAIL, or UNVERIFIED with source/test evidence.
- Files changed and tests added or modified.
- Commands executed in order, exact outcomes, and features used.
- Execution updates, bounded corrections, risks, and unresolved work.
- Final diff or commit reference.
```

## Task readiness audit

Before marking `Ready`, verify:

- the plan is approved;
- objective and requirement mapping are explicit;
- dependencies are satisfied or named;
- allowed and prohibited scope are concrete;
- target paths, symbols, responsibilities, and callers are identified;
- new or changed contracts have stubs and semantics;
- consequential control flow has pseudocode;
- failure and edge cases are testable;
- acceptance criteria are objective and complete;
- required tests name setup, action, and assertions;
- required features are explicit and justified;
- focused commands match live repository tasks;
- command/setup execution was checked and a documented cold read-only
  implementation rehearsal passed, using a fresh agent when available;
- mutable factual mechanics and immutable material decisions are explicit;
- broad closeout commands have an explicit exclusion section;
- escalation conditions catch material divergence;
- completion evidence is structured;
- The assigned Luna, Terra, or Sol model can execute without choosing
  architecture, contracts, persistence, scope, public errors, feature sets, or
  verification.
