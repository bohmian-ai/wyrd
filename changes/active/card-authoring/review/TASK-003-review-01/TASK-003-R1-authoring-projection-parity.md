---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 6
spec_approval: approved
original_task: changes/active/card-authoring/tasks/TASK-003-typescript-authoring.md
---

# Close TypeScript authoring projection gaps

TypeScript's finance and Artifact journeys pass, but several required public
construction forms still fail or lose type/capture guarantees. Repair those
forms at their existing owners, remove the unauthorized generic kind exposure,
and batch the mandatory Rust documentation corrections. This is bounded
remediation under the approved behavior, directly executable with
`$wyrd-implement`; it does not reopen product or architecture design.

## Subject and authority

- Approved authority: `changes/active/card-authoring/spec.md`, revision 6.
- Original task: `changes/active/card-authoring/tasks/TASK-003-typescript-authoring.md`.
  Its historical revision-3 API mechanics yield to revision 6.
- Review base: `9ebe07c9e7b4ce3de560b53d76d4a7c34c749f71`.
- Reviewed candidate: `83b806482778a2aa7d3c7c38015b096788bcb7ed`.
- Review and independent ledger: adjacent `verdict.md` and
  `findings-validation.md`; full discovery reports and fresh verification are
  preserved in this directory.
- Root/TypeScript AGENTS.md, relevant agent_docs and architecture owners remain
  mandatory. Shared canonical Rust authoring, loader, publication and state
  owners remain authoritative.

## Diagnosis and selected correction

### FIND-TASK-003-1 — File declarations lack the approved common entry point

REQ-006/AC-008 require `Cards.declare(path | card)`. At
`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1816`, the new method accepts only
`AuthoredCard` and calls `authoredOf`. A string cannot compile and an untyped
string fails as a host object instead of loading its file. The parity journey
uses `declareFromPath`, so its successful receipt comparison misses this
approved operation.

Fix only the TypeScript `Cards.declare` argument adaptation: accept
`string | AuthoredCard`, forward strings through the existing
`Cards.declareFromPath` path, and retain the existing native authored operation
for Card objects. `WyrdState.declare` already demonstrates this dispatch.
The file path already reaches `NativeCards::declare_from_path` and shared Rust
`Cards::declare_from_path`, which own loading, validation and submission;
reuse that entire path without adding file detection, parsing, graph traversal
or declaration logic in TypeScript or N-API. Preserve established
`declareFromPath` callers, options, result shape and error projection.

Rust `Cards::declare` already dispatches its `Declarable::Path` input to this
same file owner, and Python already converts path/Card arguments into the
shared `Declarable` input. Neither SDK nor the shared Rust declaration engine
needs a change for this finding; other findings may independently require
shared authoring changes.

Closure: a typed `Cards.declare(STORY)` call must compile and declare the same
registered content/identities as its authored equivalent. Keep compatibility
proof for the existing path helper; do not introduce an alias.

### FIND-TASK-003-2 — Canonical nullable Prompt fields fail at collection conversion

REQ-001/002/004/010 and AC-001/007 preserve canonical values and defer domain
validation. Generated Prompt types admit `messages: null` and `body: null`.
The native field dispatch (`native/src/authoring.rs:351`, `:363`, `:563`–`:565`,
`:629`–`:630`) forces list/map conversion and throws `InvalidArg` before the
shared Prompt validator. A portable Prompt with valid messages and `body:null`
validates canonically, yet the typed Prompt constructor refuses it. Setters
also cannot clear a mutually exclusive content form using its canonical null.

Preserve the actual null through the existing shared Prompt/Body authoring
owner, then adapt native arguments thinly to that owner. Keep current iterable
setters/additions and callers usable; preserve replacement of captures. Do not
coerce null to an empty collection, narrow generated canonical types, add a
wrapper-only representation or create another Prompt validator. The shared
owner must support the required canonical value; a downstream exception cannot
repair an already rejected constructor argument.

Closure: constructor/options and complete setters accept both nullable fields;
valid portable/native forms lower identically to canonical input, and malformed
combinations remain constructible until shared validation refuses them with the
existing error family. Add methods retain list/map behavior.

### FIND-TASK-003-3 — The new generic constructor exposes an unauthorized Card kind

Root locked Card inventory and REQ-001/INV-002 allow twelve kinds. The new
public `CardDefinition` at `authoring.ts:1745`/`:1755` and
`NativeAuthored::definition` at `native/src/authoring.rs:814`–`:827` bypass the
named-kind dispatch. The new unit case at `tests/unit/authoring.test.ts:245`
constructs and successfully validates a Policy, explicitly advertising a
thirteenth authorable kind. Historical internal variants do not authorize this
new surface.

Delete the newly added generic public constructor and its native Definition
adapter/dispatch variant: their only actual construction consumer is that
unauthorized inventory case, while every authorized kind has its named class.
Remove that positive Policy case. Preserve the existing internal shared Rust
`CardDefinition` and conversion used by legitimate declarations. Do not clean
up unrelated historical wire/server variants or extend Policy support.
Deletion is smaller than adding a second public kind allowlist and keeps the
approved named-kind projection complete.

Closure: all twelve named classes remain exported and constructible; current
public authoring exports and regenerated addon declarations expose no generic
route admitting Policy/Source/External.

### FIND-TASK-003-4 — Drift construction discards the common reference adaptation

REQ-001/008/010 and AC-007 include `signal.baseline_ref`. Public `Check.drift`
(`authoring.ts:2276`) accepts raw wire JSON instead of the existing authorable
projection; native `authoring.rs:1063`–`:1067` calls JSON-only conversion.
Supplying `CardRef.data(...)` or a Data object produces `spec takes JSON, not a
CardRef` (or corresponding Card-family error). Canonical Check composition
already preserves nested references; the inventory test uses only a raw
reference literal and misses this convenience-constructor failure.

Extend existing shared `Check::drift` argument/capture handling to use the same
`FieldValue` composition semantics as canonical Check construction. Project
that through the existing native field conversion and generated authorable
type machinery. Preserve fixed kind/on constructor fields, exact external
CardRef intent, authored sibling captures and existing heavy predeclaration
rules. No new TS constructor, resolver, dependency graph or automatic Data
upload is needed. Reuse the common capture owner rather than resolving a
baseline separately.

Closure: literal refs, public CardRefs and supported authored Data references
compile and compose at baseline_ref; compare canonical output and retained
sibling/external intent. Demonstrate that byte-upload restrictions remain in
the existing admission owner.

### FIND-TASK-003-5 — New iterable object assembly erases received duplicate keys

REQ-002/AC-002 forbid silently losing received duplicate map additions. At
`crates/shared/wyrd-client/src/authoring/mod.rs:153`–`:161` and
`:480`–`:492`, the new public object FromIterator adapters feed each entry to
replacement-only `Body::set`. A repeated destination/field erases the prior
value and capture without producing a duplicate marker. Unlike a JS object
whose duplicates disappeared before SDK entry, the Rust iterator supplies both
entries to the SDK. The shared Rust SDK exposes these adapters, and existing
whole-map iterable setters retain duplicate refusal. A repeated workspace
entry can silently replace its fixed source and remove a local Artifact capture.

Keep the composition adapters, but retain the first received value and capture
and record repeated-key diagnostics during object assembly through the existing
Body/Captures duplicate mechanism. Both
object iterator implementations share this correction boundary. Explicit
whole-field replacement must still clear old captures; unique entries and list
composition must retain their existing behavior. Deleting the adapters would
break required nested Node composition; graph/loader guards cannot recover
keys already erased upstream. No separate downstream check is justified.

Closure: extend shared offline proof with repeated canonical fields and a
repeated nested workspace destination, including an authored capture. Validation
must refuse with the canonical duplicate field path; distinct keys must still
preserve nested Card/file captures. Keep deliberate complete-field replacement
semantics intact.

### FIND-TASK-003-6 — Empty handle classes erase compile-time structure checks

REQ-010 says TypeScript checks structure. `Requirement` (`authoring.ts:1998`),
`ArtifactFile` (`:2453`) and `CodeSource` (`:2462`) have no instance members;
private constructors do not make their instance types distinct. Their union
positions collapse to `{}`. Strict TypeScript 5.9.3 accepts `Action.code(123,
"f")`, `Verifier.addRequirement(false)`, and a CodeSource passed as a Step
Requirement. Native conversion then rejects a wrong held family, or shared
validation rejects primitive structure. Positive no-never checks do not prove
these invalid values are refused by the compiler.

Make these existing handle-only instance types opaque and mutually distinct
using type-only identity declarations. Retain WeakMap runtime ownership,
canonical object alternatives and existing public construction names. Check
sibling handle-only families within this boundary. No runtime payload,
validator, wrapper class or dependency is required; type-only identity closes
the structural hole without duplicating state.

Closure: negative compile assertions reject primitive source/requirement values
and wrong authored families. Positive assertions retain genuine Requirement,
CodeSource and ArtifactFile values plus their canonical object forms. Preserve
existing no-never coverage and generated wire field types.

### FIND-TASK-003-7 — Mandatory changed Rust documentation is incomplete

Root AGENTS and agent_docs/rust.md require complete changed-item rustdoc,
including private fallible helpers and durable-operation failure behavior.
The standards report identifies the new `declared_state` helper without
`# Errors`; new declaration adapters without partial-progress/retry/cancellation
contracts; `NativeAuthored::of` with placeholder `Wrap inner`; and the changed
`roots!` macro without its invariant panic condition. Existing shared Cards
and state owners already document retained publications and retry semantics.
This is the explicit documentation gate, not a new functional requirement.

Batch concise corrections at those existing items with the functional work.
Document actual promise-creation versus catalog failure channels and native
promise/task semantics accurately: ignoring a JS promise does not necessarily
cancel the native future. Reuse/link shared declaration and state guarantees;
retain committed Artifacts/declarations after later failure. Explain the owned
Node-facing handle and schema macro's invariant panic. Generate affected N-API
declaration comments from Rust; never hand-edit generated declarations. No new
functional test or documentation-only follow-up round is needed.

## Constraints and preserved behavior

All edits remain projection or bounded shared-input adaptation under revision 6.
Keep synchronous offline constructors, deferred semantic validation, by-value
capture, duplicate refusal and field replacement, exact external/sibling intent,
shared error projection, selected-client continuity and the existing
publication/declaration/pull/hydration sequence. Do not add dependencies,
public wire fields, receiver classes, aliases, schedulers, upload engines,
security/tenancy/persistence behavior, automatic heavy uploads or compatibility
work. Do not change evaluation outcomes or expand the finance fixture story.
Do not weaken tests or suppress a gate. Keep Node lifetime tests in TypeScript.

## Focused proof and narrow verification

Implement focused checks alongside the changed owners. Reuse existing tests
and fixtures rather than adding a harness. The following are proof commands,
not a requirement to rerun broad journey suites:

1. Extend the existing Node authoring unit cases for null Prompt replacement,
   drift reference composition and the authorized public inventory. Run:
   `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/authoring.test.ts`.
   This closes FIND-2/3/4 with real Node values; retain existing capture/error
   assertions.
2. Extend `tests/types/authoring.test-d.ts` with negative and positive family
   assertions for FIND-6 and typed file declaration for FIND-1. Run the existing
   `ts:typecheck` lane after building changed native and public artifacts.
3. Extend the existing parity journey to use `Cards.declare(path)` for the
   required receipt comparison, preserving old-helper coverage. Run only:
   `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/authoring.test.ts -t '^local_artifacts_and_declaration_parity$'"`.
4. Extend existing shared object-composition offline proof for duplicate
   keys/captures and any shared null/drift boundary changes. Run:
   `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::entry_built_objects_keep_nested_references)'`.
   If an additional named test is needed, confirm its selector and record its
   exact nextest command; do not substitute broad package tests.
5. Generate native declarations via existing `ts:build`, then run
   `ts:napi:check`, `ts:pack:check`, `ts:format:check`, `ts:lints` and
   `ts:typecheck`. Run Rust `fmt:check`, `lints` and `check:deps` for changed
   shared/native Rust. Do not overlap Cargo commands. Run `codegen:check` only
   if consumed generated wire outputs change; no wire-schema change is needed.
   Finish with `mise exec -- git diff --check`.

The completed task evidence must map each of the seven stable IDs to its
correction and observed proof, including declaration regeneration and rustdoc
source review. Full cross-language journey suites and broad aggregates remain
integrated change-review work.

## Implementation Evidence

Commits: `52eeb1be3` (shared), `cb0306428` (native, wrappers, unit and type
proof), `2d99cf659` (journey).

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-1 | `wyrd/src/index.ts` `Cards.declare(root: string \| AuthoredCard)` forwards a string to `declareFromPath`; the native authored path is unchanged. | Type proof: `Cards["declare"]` takes `string` and `Service` (`tests/types/authoring.test-d.ts`). `local_artifacts_and_declaration_parity` now compares `cards.declare(STORY)` with the authored receipt and asserts `declareFromPath(STORY)` returns an equal receipt. | PASS |
| FIND-TASK-003-2 | Shared `Prompt::clear_messages`/`clear_body` set the canonical `null` (`kinds.rs`); the native `fields!` `null` section routes an exact JSON `null` for `messages`/`body` to them, while lists and maps still go through the iterable setters and add methods. The generated wire types already admit `null`. | Node `definitions_edit_offline`: constructor `body: null` keeps the null and validates; `body(...).messages(...).body(null)` equals it; `messages(null).addBody(...)` keeps the map; neither form is constructible and refused only at validation (`WYRD_LOADER_400_INVALID_ENVELOPE`). Rust `nullable_prompt_fields_and_drift_references_compose`. | PASS |
| FIND-TASK-003-3 | Deleted public `CardDefinition` (TS), `NativeAuthored::definition`, and the `Authored::Definition` variant and its dispatch arms. The internal shared `CardDefinition` is still used for declarations. | `index.d.ts` regenerated without `definition` (`ts:napi:check` clean). Unit inventory asserts all twelve named classes are exported and `CardDefinition` is not; the Policy case is removed. The type test's class list drops it. | PASS |
| FIND-TASK-003-4 | Shared `Check::drift(on, spec: impl Into<FieldValue>)`; `Body::set` locates the flattened field's captures from the object they are written into. Native `check_drift` uses `field()`; TS `Check.drift` takes `Authorable<…>`. | Rust test: a literal ref and `CardRef::data` produce the same spec and validate; an authored Data becomes `{sibling}` and joins the graph's submissions. Node test: the same for `CardRef.data` and `Data`, each validated. Type proof: `Data` and `CardRef` fit `baseline_ref`. Heavy byte admission is left to the declaration and registry owners: composition only produces the reference and the capture. | PASS |
| FIND-TASK-003-5 | `Body::assembled` keeps the first received value and capture, and records a repeated key through `Captures::duplicates`. Both object `FromIterator` impls use it. `set`/`set_map` replacement is unchanged. | `entry_built_objects_keep_nested_references`: repeated `id` and a repeated workspace destination keep the first value and the local Artifact selection, and are refused at `steps[0].id` and `steps[0].action.workspace./inputs/policy.json`; distinct keys keep the local Artifact (one publication) and the nested Agent. All 9 `authoring::` tests pass. | PASS |
| FIND-TASK-003-6 | A type-only `private declare readonly family` brand on Component, Binding, Step, Requirement, InputSlot, Action, Check, CardRef, ArtifactFile and CodeSource; it emits nothing at runtime. `Authorable` now picks `Action`/`Check` only when the position is the whole union. Agent/code actions and checks share one canonical shape, so the old per-member branch admitted an `Action` at a Check position. | Negative compile assertions reject `123` as a code source, `false` and `ArtifactFile` as requirements, `CodeSource` as a Step requirement or component, `Requirement` as a source, `Check` as a step action and `Action` as a check. Positive assertions keep the genuine handles and canonical objects. The existing no-`never` checks still pass (`ts:typecheck`). | PASS |
| FIND-TASK-003-7 | rustdoc: `declared_state` now has `# Errors` and `# Cancellation`; every native declaration adapter documents promise-creation versus result failures and that ignoring the promise does not stop the spawned future; nothing is rolled back and Artifacts already published stay registered, as the shared declaration documents. `NativeAuthored::of` documents the owned Node-facing handle. The `roots!` macro documents its heap list and the panic invariant. | Source review of each item. Declaration comments regenerated by `ts:build`; `ts:napi:check` reports no drift. | PASS |

Verification:
- `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/authoring.test.ts`: 3 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::entry_built_objects_keep_nested_references)'` and `-E 'test(=authoring::tests::nullable_prompt_fields_and_drift_references_compose)'`: PASS. `-E 'test(/^authoring::/)'`: 9 passed.
- Parity journey, by the exact command in this task: 1 passed.
- `ts:build`, `ts:napi:check`, `ts:pack:check`, `ts:format:check`, `ts:lints`, `ts:typecheck`: PASS.
- Rust `fmt:check`, `lints` (workspace clippy, which also builds the Python SDK) and `check:deps`: PASS. `git diff --check`: clean.
- `codegen:check` was not run: no generated wire output changed.

Non-goals held: no dependencies, wire fields, aliases, receiver classes, or
upload or declaration engines were added. Python still compiles against the
widened `Check::drift` signature, because `Value` converts into `FieldValue`.
No unrelated tracked file changed.
