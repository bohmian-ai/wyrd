# Execution Readiness Review

Use this reference for plan and task handoff gates. Architecture approval does
not imply that task packets are executable.

## Two-axis gate

Record independently:

| Axis | Result | Required evidence |
|---|---|---|
| Architecture | PASS / FAIL | Material decisions, authority, ownership, contracts, safety, workflow |
| Executability | PASS / FAIL | Impact closure, task compilation, command validity, setup reachability, rehearsal |

Overall approval requires both axes to pass.

## Validate impact closure

Start with every seed change in the plan's Mermaid impact graph. Independently
trace concrete repository nodes through:

- owning structs, modules, crates, packages, services, and stores;
- callers, downstream consumers, traits, implementations, callbacks, and
  dispatch paths;
- manifests, dependencies, Cargo features, explicit test targets, fixtures,
  support exports, scripts, and `mise` tasks;
- generated contracts and Rust, Python, TypeScript, HTTP, CLI, MCP, UI, docs,
  schema, and stub projections;
- persistence, migrations, audit, tenancy, lifecycle, deployment, recovery,
  concurrency, and cancellation;
- verification commands and their environment, service, migration, emulator,
  or credential dependencies.

Accept an explicit `no impact` only when source evidence supports it. Raise
`IMPACT_GRAPH_GAP` when a missed node can change scope, contracts, tests,
ownership, or completion proof materially. Do not demand unrelated inventory.

## Validate each task

For every task proposed as `Ready`, confirm:

1. objective, requirement IDs, dependencies, non-goals, and material
   prohibited scope are coherent;
2. owners, current seams, callers, consumers, and responsibilities are
   source-verified;
3. material interfaces and invariants are specified;
4. consequential ordering, IO, state, errors, concurrency, and recovery have
   normative semantics;
5. acceptance criteria are observable and map to named tests with setup,
   action, and critical assertions;
6. package, feature, target, filter, fixture, and support-export names exist;
7. filters select the intended tests rather than zero tests;
8. repository-managed services, migrations, environment, and local values are
   discoverable;
9. focused commands prove the task without unrelated broad gates;
10. living-task corrections and material stop boundaries are explicit;
11. the declared execution skill accepts the complete write set: non-UI work
    uses `$wyrd-implement`, while Svelte/UI work includes `$wyrd-ui`.

Exact private paths and symbols are repository-grounded guidance, not a
whitelist. Exact command strings are recipes unless lane, feature set, or
environment is itself normative.

## Judge proof equivalence

The task may permit a corrected command only when it:

- proves the same acceptance criterion;
- exercises the same relevant target, code, and features;
- preserves required test tier, negative flows, environment behavior,
  assertions, and consumers;
- records why the original recipe was defective.

Do not accept a unit test as equivalent to a required integration or
user-journey test. Do not approve a recipe that compiles an unrelated invalid
target, selects zero tests, omits repository-provided setup, or silently drops
relevant features.

Inspect every command definition. Independently rerun only commands whose
planner evidence is absent, stale, contradictory, or high-risk enough to
affect approval. Never update dependencies or lockfiles during review.

## Rehearse cold implementation

Rehearse from the task packet, repository, and normal authorities only:

1. locate every owner and target seam;
2. trace callers, consumers, dispatch, and generated effects;
3. locate the required tests, fixtures, exports, and setup;
4. inspect each verification command;
5. walk the first implementation and test steps;
6. identify the first choice the implementer must make.

The rehearsal passes when every remaining choice is a reversible local mechanic
or bounded correction already authorized by the task's declared
surface-appropriate execution skill or skill set.

Require planner evidence that every `Ready` task passed a documented cold
rehearsal, using a fresh agent when the planning environment supported it. When
delegation was unavailable, accept a documented self-rehearsal that covers the
same steps; lack of a fresh-agent tool is not itself a readiness failure. Give
any fresh agent the task packet rather than the planner's reasoning or expected
answer.

For cross-cutting, persistence, security, migration, concurrency, generated
contract, or multi-language work, independently repeat the rehearsal with a
new read-only agent when the review environment supports it. The reviewer-side
repeat must not edit source, update dependencies, or modify lockfiles.

Raise:

- `MISSING_REHEARSAL_EVIDENCE` when required cold rehearsal is absent;
- `UNEXECUTABLE_TASK` when rehearsal reaches an unresolved material decision;
- `INVALID_VERIFICATION_RECIPE` when proof or setup cannot execute as written
  or through an explicitly allowed equivalent;
- `IMPACT_GRAPH_GAP` when a missed dependency-cone node changes the plan
  materially.

## Protect controlled adaptability

Confirm the task distinguishes:

- **normative:** required behavior, public/durable contracts, ownership,
  persistence, security, acceptance outcomes, proof tier and semantics;
- **advisory:** private paths, helper names, fixture layout, illustrative
  syntax, and non-normative command spelling;
- **living updates:** corrected repository facts, adjacent private support,
  equivalent commands, progress, and evidence;
- **material stops:** dependencies/features, migrations, public/persisted
  contracts, auth/tenancy/policy/audit, ownership redesign, or changed
  acceptance outcomes.

Overly rigid private file whitelists, immutable helper names, or exact command
identity are Major only when they predict implementation interruption or force
weaker/incorrect proof. Do not report harmless verbosity.
