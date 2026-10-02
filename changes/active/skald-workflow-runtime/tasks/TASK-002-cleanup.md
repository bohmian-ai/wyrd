---
id: TASK-002-cleanup
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
requirements: [REQ-001, REQ-002, REQ-003, REQ-013, REQ-013A, REQ-014, REQ-024, REQ-025, REQ-028, REQ-029, REQ-040, REQ-052, REQ-054, REQ-055, REQ-056, REQ-057, REQ-059, INV-002, INV-003, INV-005, INV-007, INV-008, INV-014, AC-001, AC-002, AC-003, AC-006, AC-013, AC-029, AC-030, AC-031]
depends_on: [TASK-001]
replaces: TASK-002
---

# Replace duplicate loading machinery with the shared client contract

Implementation skill: `$wyrd-implement`. Original TASK-002 is stopped and
superseded. Work forward from the stopped candidate; do not reset to a2cea54e0
or discard validated provenance/UID-fence corrections. Revision 12 is approved;
this task passed its independent Revision 12 readiness review. Readiness does
not waive TASK-001's candidate-bound implementation review obligations.

## Outcome and Value

Users load a local Workflow file with automatic registered Agent/Prompt refs,
run it, apply it, and load its exact registered version through Cards. Rust,
Python and TypeScript project one shared composition and one Skald runtime.
Obsolete WorkflowLoader/WorkflowGraph and keyed-reference normalization are
removed without a compile-broken or validation-weakened intermediate outcome.
This task proves Native execution using existing provider configuration;
TASK-003 adds shared selected gateway/binding preparation and TASK-005 closes
all supported routes. Loading accepts declarative routes without executing them.

## Owners, Scope, Consumers, and Prohibited Changes

Existing wyrd-loader owns parsing, sandboxed relative paths, !file, diagnostics,
and ReferenceSlotVisitor projection. Existing Cards hydration owns registered
exact graph traversal and necessary in-memory composition. Skald owns native
Agent/Prompt hydration, pure/resolved validation and execution. Existing server
EffectiveSpecs and composite registration own tenant SQL, binding and writes.
Language wrappers own only conversion and runtime boundaries.

No Workflow principal, Workflow-root WyrdState, public hydrator/loader handle,
new executor, registry cache, transport, parser dialect, compatibility alias,
server loading via client HTTP, execution during registration, or secret/provider
resolution during load. No forced filesystem publication or irrelevant artifact
fetches. Preserve existing Service hydration, registration audit, and UID fence.

## Source-backed reuse map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Authored bundle | `wyrd-loader/src/lib.rs::load`, `resolve.rs::resolve_tree`, `ReferenceSlotVisitor` | `Cards::register_from_path`; loader bundle tests | Public client construction with automatic external refs | Feed existing normalized bodies to existing graph/hydration owner | No new parser or traversal |
| Registered graph | `wyrd-client/src/cards/hydrate/{mod,graph}.rs::CardGraphHydrator`, `GraphTraversal`, `resolve_graph` | `CardGraphHydrator::hydrate`; graph tests and SDK Service hydration | In-memory consumption, lazy external-ref composition, no artifact/disk work for Workflow loading | Extend existing graph owner while retaining disk hydration caller | No second body store/traversal; authored bodies need provenance-aware inputs to this owner |
| Runtime lowering | `skald-workflow/src/workflow_surface.rs::Workflow::from_card_with_agent_resolver`, `plan.rs::ResolvedGraph::resolve` | Workflow builder/validate/run; TASK-001 proof | Shared client IO boundary and declaration-only server validation without execution tools | Delegate to current resolver/validation owners | Thin client Workflow facade earns its existence because client IO cannot be added as an inherent method to a foreign Skald type |
| Registration | `cards/service.rs::plan_registration_graph`, `resolve.rs::EffectiveSpecs::{load,validate_workflows}`, `write_registration` | `pg_workflow_registration`; SQL lifecycle test | Remove WorkflowGraph lookup loop while retaining resolved validation | Existing effective-body resolution supplies Skald rules; canonical reference inventory supplies closure | No parallel registration or SQL lookup workflow |
| Python | `sdks/wyrd-sdk-python/src/workflow.rs`, `src/state/mod.rs::PyCards` typed getters | public agent stubs; workflow save/load tests; Cards CRUD journey | from_path and Cards Workflow view | Project shared client and existing runtime bridge | Typed boundary view only, no Python graph/validation |
| TypeScript | `sdks/wyrd-sdk-ts/native/src/{cards,client,lib}.rs`, `wyrd/src/index.ts` | cards-state integration and public errors | Workflow wrapper/loading/execution projection | Project same shared client through native async boundary | Node runtime wrapper only, no TS implementation of durable/native logic |
| Rust SDK | `sdks/wyrd-sdk-rust/src/lib.rs` and manifest `[lib] name = wyrd_sdk` | verification_run real SDK journey | shared facade export and real Workflow journey | Re-export shared types; reuse WyrdTestServer | No Rust SDK implementation |

## Public contracts and ordering

Python: `Workflow.from_path(path: str | os.PathLike[str]) -> Workflow`;
`cards.workflow.load(*, space: str, name: str, version: str) -> Workflow` or
`load(*, uid: str) -> Workflow`. No client argument, mixed selector, or versionless
named lookup. Existing `run` and string-input shorthand remain synchronous through
the shared runtime. Replace old static `Workflow.load`, update consumers, no alias.

TypeScript: `Workflow.fromPath(path: string): Promise<Workflow>`;
`cards.workflow.load({space,name,version} | {uid}): Promise<Workflow>`;
`workflow.run(input?: Record<string, JsonValue>): Promise<WorkflowRun>`.
Reject mixed selectors both statically and dynamically. Existing WyrdError
projection remains the error contract.

Rust public import is `wyrd_sdk`. The shared client facade exposes async
`Workflow::from_path(impl AsRef<Path>) -> Result<Workflow, WyrdError>`;
`cards.workflow().load(&CardSelector) -> Result<Workflow, WyrdError>`;
async `run(impl Into<WorkflowInput>) -> WorkflowResult<WorkflowRun>`.
Expose existing CardKind/CardRef through the shared Cards module so callers can
construct its existing selectors without a separate contract implementation.
Reject wrong kinds and versionless named selectors; exact and UID selectors
retain their existing identity assertions. Preserve native builder/explicit
execution-dependency behavior through delegation, not a parallel runtime.

Normative boundary ordering:

```text
file: existing load/normalize → select root from entry file → existing graph
      owner consumes local siblings/inline bodies → first external ref lazily
      obtains default Cards context → exact authorized transitive reads →
      Skald hydration/validation → publish complete Workflow
registered: validate exact Workflow selector → existing Cards graph resolution
            by locked relationships/UIDs → Skald hydration/validation → Workflow
registration: existing pure composition → tenant effective bodies (external
              and sibling sources distinct) → Skald declarative validation →
              existing exact-UID write recheck/binding/audited transaction
```

Only registration writes. Cancellation of loading publishes no partial Workflow
and performs no durable write. Local siblings do not require UID-bearing registered
relationships; extend the existing graph owner for authored inputs rather than
fabricating UIDs or converting Sibling into external Ref. Registration validation
binds no tools, resolves no secrets, and runs no provider. Execution binds declared
tools from the existing caller registry and uses the unchanged runtime.

## Approach

1. Revalidate this map and record the selected existing-owner extension.
2. Replace shared graph/loading composition and server validation together;
   preserve pure validation, provenance and exact-preflight-UID refusal.
3. Add the thin shared Workflow facade and Cards typed view, project all SDKs,
   and update old static load consumers without a compatibility alias.
4. Remove obsolete types/loops/normalization and correct the example syntax.
5. Retarget behavioral proof, add missing automatic-resolution/SDK journeys,
   regenerate declarations and verify the complete outcome.

## Ordered Implementation Scenarios

All added selectors below are planned. Manifests currently auto-discover Rust
SDK integration targets; confirm selection after adding the target. Every GREEN
reruns earlier scenarios; REFACTOR retains their proof. Use deterministic local
upstreams and repository-managed Postgres, no live provider credential.

### Scenario 1 — Local file loading is automatic and registry-free when possible

**Behavior.** REQ-025/054/055/059, AC-001/030: actual entry file loads transitive
paths and inline native bodies through existing sandbox rules; wholly local
load requires no credentials/registry IO. Bad graph/bindings refuse before dispatch.

**RED.** Planned shared-client lib test `workflow::tests::from_path_uses_existing_loader`:
`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow::tests::from_path_uses_existing_loader)'`.
Use the checked-in bundle and existing deterministic provider fixture. Assert
zero registry requests and unchanged named output/binding behavior; expected
failure is missing public from_path/shared composition, not test setup.

**GREEN.** Compose existing loader and graph owner, lazy Cards only for external
refs, existing runtime/default tool registry; retain pure/resolved validation.

**REFACTOR.** Remove WorkflowLoader/fetch_missing and its competing graph state;
keep helpers private to the existing owners.

### Scenario 2 — Composite registration keeps validation and provenance fences

**Behavior.** REQ-014/028/056/059, AC-002/006/030: raw HTTP and SDK registration
reject invalid sibling/external resolved graphs; same-identity sibling cannot
satisfy an external ref. Apply writes exact relationships, no provider calls or
partial graph, no Workflow principal. Preflight UID replacement is refused.

**RED.** Retarget existing `registers_only_valid_explicit_workflow_graphs` in
`pg_workflow_registration`, retaining its sibling/external collision assertions:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=registers_only_valid_explicit_workflow_graphs)"'`.
Existing correct refusal assertions are regression proof, not manufactured RED;
preserve and separately run `refuses_stale_preflight_after_dependency_replacement`
in that target to prove the exact-UID write fence:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=refuses_stale_preflight_after_dependency_replacement)"'`.
Add only missing public-source/Workflow-principal assertions. An already-correct
scenario is verification-only; record that result.

**GREEN.** Existing EffectiveSpecs supplies exact bodies to Skald validation;
keep existing transaction/audit/write-time identity checks, declaration-only tools.

**REFACTOR.** Delete WorkflowGraph import and graph assembly loop, not validation.

### Scenario 3 — Rust SDK exercises automatic refs and pinned registered loading

**Behavior.** REQ-054–057, AC-003/029/030: real SDK and server load shared team
Agent/Prompt plus local Agent, apply and reload, remain pinned after v2, and
refuse missing credentials/denied/inactive/mismatched refs before dispatch.

**RED.** Planned ignored `workflow_loading_journey` in new SDK target
`sdks/wyrd-sdk-rust/tests/workflow_loading.rs`; package is `wyrd-sdk-rust`, import
`wyrd_sdk`, existing WyrdTestServer/Bootstrap precedent is `verification_run.rs`:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'`.
Expect absent facade/view. Drive real SDK reads/apply registration path; CLI apply
itself is independently proved in TASK-005. Count reads/dispatch and assert exact
stored Agent/Prompt UIDs, outputs and no privilege transfer.

**GREEN.** Thin Rust re-export; current client credential configuration and
exact traversal serve both external authored refs and locked registered closure.

**REFACTOR.** Retarget existing `fetches_and_executes_locked_workflow_graph` to
Cards Workflow loading; preserve its negatives rather than deleting them.
Focused regression command:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=fetches_and_executes_locked_workflow_graph)"'`.

### Scenario 4 — Python exposes the same complete loading journey

**Behavior.** REQ-054/055/056, AC-029/030: public wyrd.agent Workflow.from_path and
wyrd.cards Cards.workflow.load exercise local/mixed/registered paths, exact pins,
credential and read refusals. Interpreter-dependent behavior stays in Python.

**RED.** Planned `test_workflow_loading_journey` appended to existing
`tests/integration/cards/test_cards_crud.py` (integration marker); reuse existing
server bootstrap/default-client env isolation. Exact command:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py -k test_workflow_loading_journey'`.
Expect missing from_path/typed view. Existing public workflow save/load unit tests
move to from_path; keep their native authoring assertions.

**GREEN.** Existing Python wrapper/typed Cards context project shared native
composition; synchronous bridge releases the GIL as required, no Python graph.

**REFACTOR.** Regenerate stubs from sources; no manual .pyi edits or aliases.

### Scenario 5 — TypeScript exposes async loading and native execution

**Behavior.** REQ-054/055/056, AC-029/030: public @wyrd/sdk Workflow.fromPath and
Cards.workflow.load have the same Native journey, selectors, pins and negatives.
Node lifetime-dependent assertions execute through Node, never Rust unit tests.

**RED.** Planned Vitest case `workflow loading journey` in new
`tests/integration/workflow-loading.test.ts`, using cards-state.test.ts and
@wyrd/testing bootstrap; exact command:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^workflow loading journey$"'`.
Expect absent Workflow/view. Assert type/runtime selector refusal and canonical
WyrdError; port native result fields, not JS execution logic.

**GREEN.** Native async projection over the shared facade/engine; handle owned
values across awaits with the existing napi lifetime pattern.

**REFACTOR.** Generate declarations and update exports; no TS transport/graph.

### Scenario 6 — Obsolete machinery disappears without collateral regression

**Behavior.** REQ-057/059, AC-031: old types/parser normalization are absent;
existing Service hydration, !file/sandbox and reference visitors still work.

**RED.** Source/diff audit is static proof, not manufactured RED. Existing
Service/loader tests supply regression proof. The functional RED/GREEN work is
in Scenarios 1–5; no lexical-ban checker or test of a check is introduced.

**GREEN.** Delete workflow_loader.rs and exports/docs/consumers; remove only
keyed normalization and its sole-use InlineableSlotField metadata/tests. Retain
CardRefIdentity, canonical visitor, versioned Prompt Cards and safety assertions.

**REFACTOR.** Remove now-unused manifest edges only after inspecting remaining
consumers; no broad revert or task-specific code comments.

## Acceptance Criteria

All scenarios close; the actual bundle works through three SDKs; local loading
is registry-free unless external refs require it; registered refs are exact and
authorized; no partial writes/dispatch on refusal; provenance and UID fences
remain; Workflow creates no principal; WyrdState remains Service-rooted;
obsolete machinery is physically removed and existing Service/loader behavior
passes. All shared/native/SDK contracts and generated declarations align.

## Expected Write Set and Consumer Closure

Shared loader parsing/validation; shared Cards hydration/context/view and thin
Workflow facade; existing Skald hydration/validation seam only as needed;
existing server cards resolve/service and SQL UID-fence regression proof;
Rust SDK exports and journey; Python existing workflow/state wrappers, exports,
generated stubs and tests; TS native wrappers/public exports/declarations/tests;
versioned example syntax and its loader proof. Paths are ownership guidance,
not a private file allowlist. Keep existing lower-tier dependency direction.

## Verification and Evidence

Run every named command above and record selected counts. Then the smallest
complete relevant lanes: `mise run test:shared`, `mise run test:skald`,
`mise run test:cards:integration`, `mise run test:wyrd-sdk`,
`mise run py:test:unit`, `mise run py:test:cards:integration`,
`mise run py:typecheck`, `mise run ts:test:unit`, `mise run ts:test:integration`,
`mise run ts:typecheck`, `mise run ts:napi:check`, `mise run codegen:check`,
`mise run check:client-tier`, `mise run check:sdk-client-tier`,
`mise run check:pyo3-scope`, `mise run check:registry-tx-coupling`,
`mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`,
`git diff --check`. SDK unit projection is not journey proof. Confirm existing
lanes select the new tests; exact SDK ignored journey remains separately run.
Full capability gate belongs to TASK-005, not this cleanup acceptance.

## Material Stop Conditions

Route changes to public APIs, principal/WyrdState semantics, reference dialect,
exact read authority, registration audit/atomicity, Skald dependency direction,
or execution engine through specification approval. Correct invalid private
mechanics through planning; do not rebuild duplicate orchestration to obey them.

## Authority Links

- [Approved Revision 12](../spec.md); [TASK-001](TASK-001-explicit-local-runtime.md)
- AGENTS.md; architecture/agent-rules.md; architecture/wyrd-design.md
- architecture/wyrd-doctrine.mdx; architecture/wyrd-security-posture.md
- architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows,pyo3-boundaries,typescript-guide,errors}.md

## Implementation Evidence

Commits: b5d88a590, d46018cde, 1f893a91c, a4c4c73c5, 0d3dbbc5a, c61168a1b, b9b27c2b4 (+ this evidence commit).
Shared journey fixtures: `tests/fixtures/workflow-loading/` (README describes each case).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| S1 local file loading is automatic and registry-free when possible | `crates/shared/wyrd-client/src/workflow.rs::Workflow::from_path` (builds `Cards` only when external refs exist) | `wyrd-client --lib workflow::tests::from_path_uses_existing_loader`; each SDK journey step 1 loads `examples/workflows/code-review` with no server/credentials | PASS |
| S2 composite registration keeps validation and provenance fences; no Workflow principal | `wyrd-server` cards `resolve`/`service` (WorkflowGraph lookup loop removed) | `pg_workflow_registration::{registers_only_valid_explicit_workflow_graphs, refuses_stale_preflight_after_dependency_replacement, fetches_and_executes_locked_workflow_graph}` | PASS |
| S3 Rust SDK: refs, apply/reload, pinned after v2, refusals | `sdks/wyrd-sdk-rust/src/lib.rs` re-exports; `tests/workflow_loading.rs` | `wyrd-sdk-rust --test workflow_loading --run-ignored all workflow_loading_journey` | PASS |
| S4 Python `Workflow.from_path`, `cards.workflow.load` | `sdks/wyrd-sdk-python/src/{workflow.rs,state/mod.rs}`, stubs `agent.pyi`/`cards.pyi` | `test_cards_crud.py::test_workflow_loading_journey`; `py:typecheck`; `codegen:check` | PASS |
| S5 TypeScript `Workflow.fromPath`, `cards.workflow.load`, `run` | `sdks/wyrd-sdk-ts/native/src/workflow.rs`, `native/src/cards.rs::load_workflow`, `wyrd/src/index.ts::{Workflow,WorkflowCards,WorkflowSelector,WorkflowRun}` | `tests/integration/workflow-loading.test.ts` "workflow loading journey"; `ts:typecheck`; `ts:napi:check` | PASS |
| S6 obsolete machinery removed, no collateral regression | `workflow_loader.rs`, keyed normalization, `InlineableSlotField` deleted; `CardRefIdentity` retained (`cards/hydrate/workflow.rs`, `wyrd-spec/src/graph`) | `git grep` WorkflowLoader/workflow_loader/InlineableSlotField/keyed refs: none outside `changes/`; `test:shared`, `test:cards:integration` Service hydration and loader tests | PASS |

Observed codes, consistent across SDKs:

| Case | Code |
|---|---|
| Registry ref, no credentials | `WYRD_CLIENT_401_NO_CREDENTIALS` |
| Principal cannot read Cards | `WYRD_PERMISSION_403_DENIED_RBAC` |
| Deleted Card / Agent UID used as Workflow UID | `WYRD_REGISTRY_404_CARD_NOT_FOUND` |
| Exact Agent ref used as Workflow selector | `WYRD_REGISTRY_400_INVALID_CARD_SPEC` |
| Versionless selector | `WYRD_REGISTRY_400_VERSION_REQUIRED` |
| Ref UID naming another Card | `WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE` |
| Mixed TS/Python selector | `WYRD_SPEC_400_VALIDATION` |
| `run()` on the `wyrd_gateway` route | `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` (pre-dispatch) |

### Material limits and deviations

- Ambient-credential `from_path` with registry refs is proved in Python and
  TypeScript only. The Rust journey would need `unsafe { set_var }` under
  edition 2024 with `unsafe_code = deny`; the maintainer asked for plain tests.
- The code-review bundle uses the `wyrd_gateway` route. Python and TypeScript
  `run()` refuse with `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` until the gateway
  binding lands (TASK-003). The Rust journey executes the pinned graph through a
  fake gateway via `as_skald().run_with_options`.
- "Inactive" is exercised as deletion (a referenced Card cannot be deleted, so
  the `retired` fixture references a standalone Prompt that is deleted).
- Skald's engine-level `Workflow::load(path, tools, prompts)` remains as the
  lower-tier seam used by `examples/rust/workflow_from_yaml.rs`.

### Reuse-map revalidation

| Capability | Existing owner reused | Gap closed | New machinery |
|---|---|---|---|
| Authored bundle | `wyrd-loader::load`/`resolve_tree` | bodies fed to Cards graph owner | none |
| Registered graph | `CardGraphHydrator`/`GraphTraversal` | in-memory Workflow consumption | none |
| Runtime lowering | Skald `Workflow::from_card_bodies`/`validate_card_bodies` | client IO boundary | thin `wyrd_client::Workflow` facade |
| Registration | `EffectiveSpecs::{load,validate_workflows}` | WorkflowGraph lookup loop removed | none |
| Python | `PyWorkflow`, `PyCards` getters | `from_path`, `PyWorkflowCards` view | typed boundary view only |
| TypeScript | `NativeCards`, `NativeLifecycleResult`, `nativeHandle` | `NativeWorkflow`, `loadWorkflow` | napi wrapper + selector parse only |
| Rust SDK | `wyrd_sdk` re-exports, `WyrdTestServer` | journey | none |

### Verification commands

All PASS:

- Focused: `wyrd-client --lib workflow::tests::from_path_uses_existing_loader` (1/1);
  `wyrd-server --test pg_workflow_registration` three named tests (3/3);
  `wyrd-sdk-rust --test workflow_loading --run-ignored all workflow_loading_journey` (1/1);
  Python `-k test_workflow_loading_journey` (1 passed, 13 deselected);
  TS `vitest run tests/integration/workflow-loading.test.ts -t "^Workflow loading workflow loading journey$"` (1/1).
  The task's `-t "^workflow loading journey$"` selects nothing because Vitest
  matches the full name including the `describe` block; corrected above.
- Lanes: `test:shared`, `test:skald`, `test:cards:integration`, `test:wyrd-sdk`,
  `py:test:unit`, `py:test:cards:integration`, `py:typecheck`, `ts:test:unit`,
  `ts:test:integration` (11 files / 26 tests, includes the new journey),
  `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:client-tier`,
  `check:sdk-client-tier`, `check:pyo3-scope`, `check:registry-tx-coupling`,
  `fmt`, `lints`, `py:format`, `py:lints`, `git diff --check`.
- The Rust SDK journey is `#[ignore]` and is not selected by `test:wyrd-sdk`;
  it is run by the exact focused command above.

Non-goals stayed excluded: no gateway execution binding, no Workflow
principal, WyrdState unchanged and Service-rooted, no new reference dialect.

**Status: IMPLEMENTED.**
