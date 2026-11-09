---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-card-authoring
spec_revision: 3
spec_approval: approved
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-010, INV-001, INV-002, INV-003, INV-004, INV-005, AC-002, AC-003, AC-006, AC-007]
depends_on: [TASK-001]
---

# Expose common Card authoring through TypeScript and N-API declarations

## Outcome and Value

A TypeScript developer constructs native-backed Service/Verifier values with
runtime JSON Schemas, adds steps/bindings, and uses existing Cards/state
declaration. Its Node journey authors the same multi-component Service and
multi-step Verifiers as Rust, with the same fixture assets and expected cases.

This task wraps Task 1's shared interface rather than implementing another
registration engine. It depends on Task 1 and spec approval, not on Python.
The shared work is solely authoring/ergonomics and argument adaptation. This
wrapper introduces no registration or upload path; existing machinery is reused.
That dependency includes the entire [native primitive surface](TASK-001-common-rust-authoring.md#complete-native-primitive-surface),
not just the types exercised by the multi-component example.

## What Changes and What Must Stay True

Expose typed constructors and camelCase methods through the public authoring
exports, with thin N-API conversions. Task 1 owns every core definition, enum,
helper and canonical conversion, including AgentCheck, AssertCheck and the
remaining check/action/reference/input/Artifact families. Project the complete
inventory through native exports and accurate declarations. TypeScript may
adapt arguments, schema values and names, but must not implement missing core
types or behavior. Constructors and edits are synchronous
and network-free; add methods mutate and return the receiver. Child insertion
captures values, and declaration takes a validated snapshot. No builder/build
phase, mandatory JsonInput wrapper, TypeScript DAG compiler or local scheduler.

Use actual canonical JSON Schema values; erased TypeScript interfaces cannot
supply runtime schemas. No mandatory Zod or other dependency. Input omission
options and nullable schemas remain independent. Native errors retain the
same WyrdError codes/fields and runtime lifecycle.

Cards/state accepted inputs must reuse shared Rust sequencing: local Artifacts
are published first through existing sole-submission upload calls, then the graph
references their exact registered versions. Existing Artifact CardRefs cause no
upload, and graph failure leaves a successfully published Artifact intact.
TypeScript does not implement this sequence independently. Inputs reach existing
shared declaration, upload,
completion, pull and hydration. Existing path/holder calls stay valid. Handle
trust-boundary values as unknown until validated; do not store V8 handles
across awaits. Authoring wrapper data never becomes an alternate wire format.

Complete native exports, public wrappers/package subpath exports, generated
N-API declarations and applicable handwritten types. Never hand-edit generated
`index.d.ts` or produce public test-only native types. Node lifetime and return
identity are tested in TypeScript.

## Approach

1. Inspect Task 1's client interface and existing TypeScript/N-API Cards/state
   wiring; extend conversion at that boundary, not native lifecycle semantics.
   Map the complete native primitive inventory to N-API/public exports and
   declarations; do not stop at the finance journey's subset.
2. Add offline Node construction/schema/lifetime tests, then implement typed
   wrappers and native projections with direct mutable methods.
3. Author the common fixture story through TypeScript objects and component
   bindings, then run actual registered component verification.
4. Close local Artifact/version/error and path-parity branches using shared
   Rust behavior; update exports and generate native declarations.
5. Verify focused Node journeys, type coverage, package exports and generated
   declaration consistency; document the actual supported flow.

## Ordered Implementation Scenarios

### Scenario 1 — Construct and edit through typed native-backed Node objects

**Behavior.** Constructors/edit methods work without a server. Add methods
cover every Task 1 primitive family's native-backed projection, including
variants unused by the finance journey. Representative construction/lowering
reaches Rust rather than implementing the primitive in TypeScript. Methods
return the same receiver, reject duplicate IDs/aliases, and preserve captured
children after caller references leave scope. Runtime schemas and optional
metadata normalize to the native input representation; supplied-invalid and
missing-required behavior are not redefined. Explicit dependsOn and inferred
output references preserve the dependency behavior proved in Task 1. A completed local definition
can reach native conversion without leaking JS object identity into the wire.

**RED.** Add planned unit title `definitions_edit_offline` in the proposed
authoring unit file. Run [offline proof](#offline-proof) after collection
confirmation. Add only sufficient native skeleton to get a meaningful failing
assertion; missing imports/build artifacts alone do not establish RED.

**GREEN.** Implement native-backed constructors, reference/check helpers,
in-place methods and JSON Schema conversion using Task 1's owner. Keep local
methods synchronous and declared types precise; no independent validation/DAG
engine or new schema dependency.

**REFACTOR.** Remove incidental JSON round trips and wrapper state duplicated
by Rust. Preserve object/receiver identity while respecting native ownership;
avoid non-deterministic GC waits or sleep-based tests.

### Scenario 2 — Author the shared multi-component journey in TypeScript

**Behavior.** TypeScript constructs finance-release with distinct report-reviewer
and release-controls components and their two-step report-policy/release-access
Verifiers. Register/hydrate through public SDK calls. Component Runs carry the
correct exact subjects. Shared success/identifier/recipient/destination cases,
uncertainty, original evidence/citations and unchanged reruns follow the full
REQ-007 case matrix, including missing report, provider uncertainty and raw
schema-invalid JSON.
Cross-component binding lookup returns `WYRD_SDK_404_UNKNOWN_VERIFIER`.

**RED.** Add planned integration title
`multi_component_service_verifies_each_subject`; run
[component journey](#component-journey) with the existing real server, Postgres
and scripted-provider fixtures. Assert exact registered refs, two step results
per component and individual judgments, not merely response presence.

**GREEN.** Route authored values to Task 1's existing declaration/state input
path. Build the scenario in TypeScript rather than loading Rust-generated
submissions; reuse its checked-in policy, sources, identities and case data.
Rerun Scenario 1 and this journey.

**REFACTOR.** Keep fixture domain setup small and shared; preserve normal Node
runtime behavior instead of emulating it in Rust. No language-specific scheduler
or provider platform is needed.

### Scenario 3 — Preserve Artifact and registration parity through N-API

**Behavior.** Through both Cards and state authored-Service calls, local
file/directory sources publish first as sole submissions and the graph uses
exact registered references. Existing Artifact CardRefs bypass preparation and
upload. A later graph failure retains successful Artifact publication and the
original graph error. All sequencing delegates to the common Rust owner. Reuse and a new policy version retain exact
attribution. Cards returns the same registration outcome and state returns
the same hydrated graph as equivalent file authoring. Source conflicts,
invalid references, immutable-byte changes and read-only authority yield the
same stable codes and no silent overwrite or fallback.

**RED.** Add planned integration title
`local_artifacts_and_declaration_parity`; run [parity journey](#parity-journey).
Assert actual pinned file/ref and registered content, plus the existing exact
`WYRD_REGISTRY_409_SPEC_DRIFT` and `WYRD_PERMISSION_403_DENIED_RBAC` cases.

**GREEN.** Forward authored snapshots and local sources through the shared
Rust owner. Preserve path inputs and native lifecycle/error mapping. Complete
public package exports and generated type declarations, then rerun prior proof.

**REFACTOR.** Remove wrapper-only field names, native object lifetime leaks,
redundant upload loops or temporary file workarounds. Keep typed declarations
accurate rather than weakening unknown values to any.

## Acceptance Criteria

| Obligations | Expected evidence |
|---|---|
| REQ-002–004, REQ-008, REQ-010; AC-002, AC-006; INV-001–003 | Scenario 1 proves direct constructors, in-place identity, snapshots/lifetime, schema metadata and common error behavior without network or scheduler. |
| REQ-006–008, REQ-010; AC-006; INV-001–005 | Scenario 2 proves actual TypeScript authoring, both multi-step components, exact subjects, shared outcomes/citations and cross-component refusal. |
| REQ-005–006, REQ-008, REQ-010; AC-003, AC-006; INV-002, INV-004 | Scenario 3 proves native upload reuse/versioning, both existing registration routes, file parity and exact error cases. |
| REQ-010; AC-006 | Public exports/subpath resolution and precise wrappers agree with regenerated native declarations; type, pack and native-declaration checks pass. |
| REQ-001, REQ-010; AC-007; INV-001, INV-003 | Offline native-export/construction and declaration coverage maps every Task 1 inventory family to TypeScript; no missing core primitive/helper is supplied only by TypeScript. |

## Implementation Reference

- **Native/public seam:** `sdks/wyrd-sdk-ts/native/src/cards.rs` owns state
  declaration, file declaration and Artifact upload forwarding.
  `wyrd/src/index.ts` owns public Cards.connect, declare/state wrappers and
  runtime handles. Extend those inputs with thin authoring adapters; preserve
  existing constructor/connection behavior rather than publishing a second
  Cards client.
- **Shared behavior:** Task 1's authoring interface and existing client/loader
  owners supply normalization, local upload preparation, native reference/error
  meaning and state hydration. TypeScript fields/types cannot become a second
  contract hierarchy or wrapper request payload.
- **Exports/types:** native `#[napi]` exports and public `wyrd-ai/authoring`
  exports/package map need coordinated closure. Existing wire types come from
  `card-types.ts`; `wyrd/index.d.ts` is N-API generated. Regenerate through
  existing build tasks and update handwritten package exports/types at their
  owners. Do not commit generated JSON Schemas or change wire fields.
- **Fixtures:** `tests/support/server.ts::serverTest` supplies real verification
  runtime, local scripted provider and request capture. `verifier-building-
  blocks.test.ts` demonstrates real uploaded-document/tool/citation flows;
  `declare-and-pull.test.ts` covers existing declaration/authority paths.
  A new story uses Task 1's common fixture assets but constructs its own Node
  objects. No new integration target or fake backend is justified.

## Verification and Evidence

Planned files/titles below must be confirmed after adding tests, before RED.
Run sequentially. Setup uses `mise run build:server`, `mise run ts:build`, and
`mise run ts:build:testing`; this builds both public and private testing addons.
Integration commands match the owning lane's Postgres/migration/runner setup.

### Offline proof

```bash
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/authoring.test.ts -t '^definitions_edit_offline$'
```

### Component journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/authoring.test.ts -t '^multi_component_service_verifies_each_subject$'"
```

### Parity journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/authoring.test.ts -t '^local_artifacts_and_declaration_parity$'"
```

Run `mise run ts:format`, `mise run ts:lints`, `mise run ts:typecheck`,
`mise run ts:napi:check`, and `mise run ts:pack:check`. Native Rust changes also
require `mise run fmt`, `mise run lints`, `mise run check:deps`. If consumed
generated wire outputs actually change, run existing regeneration/check tooling;
authoring alone does not authorize a wire change. Finish with
`mise exec -- git diff --check`. Broader TypeScript journey suites run once at
integrated change review, not as repeated per-scenario loops.

## Material Stop Conditions

Stop if this projection needs a new schema library/dependency, registration or
upload lifecycle, client-owned graph scheduler, public wire/error semantics,
security/tenant change, private test exports or handwritten generated native
declarations. Correct private N-API/wrapper/type plumbing locally; incomplete
public exports or generated declarations are not an acceptable handoff.
If a required native primitive/helper is absent, route the gap back to Task 1
under the same spec; do not bypass the dependency with a TypeScript-only type
implementation or declare the sample subset a complete projection.

## Authority Links

- [Approved spec revision 3](../spec.md), [common Rust dependency](TASK-001-common-rust-authoring.md)
- [TypeScript sketch](../examples/typescript/multi_component.ts), [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md)
- [Repository rules](../../../../AGENTS.md), [contracts](../../../../architecture/wyrd-design.md)
- [Workflow](../../../spec-driven-development.md), [execution](../../../implementation-execution.md), [testing](../../../../TESTING.md)
