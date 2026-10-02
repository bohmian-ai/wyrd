---
id: TASK-002
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 11
requirements: [REQ-001, REQ-002, REQ-003, REQ-013, REQ-013A, REQ-014, REQ-024, REQ-025, REQ-028, REQ-029, REQ-040, REQ-052, INV-002, INV-003, INV-005, INV-008, INV-014, AC-001, AC-002, AC-003, AC-006, AC-013]
depends_on: [TASK-001]
---

# Load and register exact runnable graphs

Implementation skill: `$wyrd-implement`.

## Outcome and Value

The actual code-review YAML bundle hydrates locally without registration;
composite registration accepts only a valid resolved graph; registered local
loading fetches the exact locked Workflow/Agent/Prompt versions. The same
Skald runtime consumes all three, with no second YAML or executor.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-loader` owns offline path/inline/sibling normalization and pure contract
validation. `wyrd-client` composes optional registry reads with loaded native
bodies. Server Cards composition owns transactional registered reference
resolution and acceptance validation. Skald's existing
`Workflow::from_card_with_agent_resolver` is the synchronous hydration seam;
its `AgentResolver`/`PromptResolver` receive already-fetched exact bodies.
`WyrdState` is Service-root hydrated state and MUST NOT be broadened to invent a
Workflow root. Skald MUST NOT depend on loader/client/server or perform registry
IO. Do not register during local loading, resolve secrets/network endpoints
during registration, forbid every declared tool, or rewrite standalone
PromptDraft authoring into a native-only global format.

## Approach

1. Check in the exact native-request code-review bundle with independent and
   dependent steps; native schema/loader roundtrip proves the examples.
2. Connect the existing shared loader to exact Agent/Prompt hydration outside
   Skald and run both pure and resolved validation as early as data permits.
3. Reuse Cards exact reads for authored refs and registered graph fetching;
   preserve version/UID/space assertions and pass resolved tools explicitly.
4. Apply the same pure/resolved validation at server composite registration
   after tenant-qualified dependency binding and before durable acceptance.
5. Close local/registration/registered-local negative and relationship proof.

### Packet-local loading seam

Shared client exports a cohesive `WorkflowLoader` owning optional `WyrdClient`
and `Arc<dyn ToolResolver>`, with `new(tools)`, `with_client(client)`, async
`load_file(&Path) -> Result<Workflow, WyrdError>` and
`load_registered(&CardRef) -> Result<Workflow, WyrdError>`. This owns composition,
not runtime behavior. The registered method requires a client and exact active
Workflow identity; the file method uses `wyrd_loader::load`, performs no
registration, and requires a client only for external `ref` dependencies.
Tools are the existing caller registry, not server built-ins installed locally.
Owned resolved native bodies feed the existing Skald resolver boundary; no new
resolver trait. Existing raw Skald YAML entry points must not remain a competing
bundle/ref semantics path: either route callers to the shared composition or
retain only their correctly documented inline-native parsing role.
Do not put Workflow-specific IO on foundational Card envelopes.

The exact registration seam precedent is
`components::cards::resolve::bind_card_references` in
`crates/wyrd/wyrd-server/src/components/cards/resolve.rs`; reuse its effective
spec/dependency resolution before cross-Card validation. Keep loader diagnostics
and route suitability distinct: declarative registration permits Native and
known built-in tool names; server execution suitability belongs to TASK-004.

## Ordered Implementation Scenarios

Selectors are **planned**. Default features. Every GREEN reruns earlier tests;
every REFACTOR retains them. PG commands use the checked-in lifecycle wrapper,
not invented DSNs or global serialized fixtures.

### Scenario 1 — Local bundles load with native Prompt bodies

**Behavior.** REQ-001–003/013–014/025: actual checked-in multi-file bundle
loads paths relative to the containing file, inline and transitive dependencies,
respects containing spaces and existing sandbox rules, then hydrates the same
explicit bound graph. A ref without registry access fails clearly. Bad graph,
Prompt bindings or dialect mismatch refuses before provider execution.

**RED.** Add `tests::load_explicit_workflow_bundle` in `wyrd-loader`;
`mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=tests::load_explicit_workflow_bundle)'`.
Add `workflow_loader::tests::hydrate_local_workflow_graph` in `wyrd-client`;
`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow_loader::tests::hydrate_local_workflow_graph)'`.
Expect missing Workflow validation or referenced-Agent hydration. Assert actual
example bundle rather than substitute simplified Prompt input.

**GREEN.** Use existing loader normalization and native synchronous validation;
do not execute or resolve secrets during load/registration.

**REFACTOR.** Reuse reference-slot inventory and existing resolvers instead of a
Workflow-specific path parser or an alternate Prompt representation.

### Scenario 2 — Registration validates every graph boundary

**Behavior.** REQ-014/028/052 and AC-002/006: real Cards registration stores exact
Workflow relationships and locked Agent/Prompt refs in dependency order;
Native and the two built-in tool declarations are registrable. Invalid pure
or resolved bindings/outputs/actions fail identically for sibling and external
dependencies, with no provider call, partial durable Card graph or secret lookup.

**RED.** Add `registers_only_valid_explicit_workflow_graphs` in new
`crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=registers_only_valid_explicit_workflow_graphs)"'`.
Expect current acceptance of invalid Workflow semantics. This is real
client→server Cards registration, not a raw SQL insertion substitute.

**GREEN.** Validate effective typed bodies after reference binding, before
transactional persistence; preserve caller-owned transaction/audit behavior.

**REFACTOR.** Share pure/resolved rules on their existing native owners and avoid
duplicated registration-only validation or server logic in loader.

### Scenario 3 — Registered local execution stays pinned

**Behavior.** REQ-024–025/029 and INV-005: real shared/Rust client fetches a
registered graph and executes locally with equivalent outputs/steps; later
versions do not float dependencies. Missing/inactive/foreign/mismatched refs
refuse through existing Cards contracts, never silently substitute a version.
Authored file external refs and registered loading share exact-read behavior.

**RED.** Add `fetches_and_executes_locked_workflow_graph` to
`pg_workflow_registration`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=fetches_and_executes_locked_workflow_graph)"'`.
Expect missing shared loading composition/reference hydration. Use local
deterministic upstreams and real HTTP registry reads.

**GREEN.** Finish shared loader/client projection; Skald only consumes hydrated
native bodies and caller tools. Preserve exact registered Workflow identity in
the returned WorkflowRun.

**REFACTOR.** One loading composition and the existing engine, no Service-root
WyrdState changes or hidden registry writes.

## Acceptance Criteria

Actual YAML parses and executes; pure/resolved validation runs at local load,
registration and registered local load; exact versions/relationships survive
all projections; invalid cases dispatch no provider/tool. Native and built-in
tools remain declaratively registrable. TASK-004 applies server suitability;
TASK-005 closes `wyrd apply` and CLI execution against this same bundle.

## Expected Write Set and Consumer Closure

`crates/shared/wyrd-loader/src/{validate,parse,resolve,...}` as needed;
`crates/shared/wyrd-client` loading composition/export/native adapters;
`crates/wyrd/wyrd-server/src/components/cards/{resolve,service,...}`;
checked-in YAML/input example bundle and tests above. Shared exact-read consumers
and affected codegen annotations move with changed contracts. Existing-workspace
edges necessary for the approved native resolver boundary are permitted;
no upward Skald edge, new third-party package, or source-private fixture API.

## Verification and Evidence

Run exact selectors, `mise run test:shared`, `mise run test:cards:integration`,
`mise run test:skald`, `mise run codegen:check`,
`mise run check:client-tier`, `mise run check:pyo3-scope`,
`mise run check:registry-tx-coupling`, `mise run fmt`, `mise run lints`,
`git diff --check`. YAML/docs static alignment has roundtrip/regression proof,
not artificial RED. Execute the newly added PG target exactly; existing Cards
lane does not select arbitrary new targets by name. TASK-005's aggregate selects
the server family, but this task's acceptance does not wait for that closeout.

## Material Stop Conditions

Stop for floating refs, new YAML/schema dialect, durable registration bypass,
new Skald registry dependency, broader WyrdState root contract, external secret
resolution during registration or inability to reuse native resolver boundaries.

## Authority Links

- [Approved Revision 11](../spec.md); [TASK-001](TASK-001-explicit-local-runtime.md)
- `AGENTS.md`; `architecture/agent-rules.md`
- `architecture/wyrd-design.md` §§Workflow, Spec-file authoring, Reference slots
- `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows,errors}.md`
