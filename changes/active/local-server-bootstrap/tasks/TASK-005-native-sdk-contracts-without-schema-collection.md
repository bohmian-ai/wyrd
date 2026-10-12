---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-local-server-bootstrap@6
depends_on: []
maps: [REQ-017, REQ-018, REQ-019, INV-001, INV-003, INV-005, AC-009, AC-010]
---

## Outcome and Value

Remove the checked-in schema collection and schema-to-SDK model pipeline.
Python exposes Rust-backed contract objects through PyO3 with accurate public
`.pyi` stubs; TypeScript exposes typed contract values through N-API with
accurate `.d.ts` declarations. Existing Rust, SDK, MCP, documentation, UI error,
and package behavior works without those files. This is one outcome with its
necessary consumer closure, not a schema-directory relocation.

Use `$wyrd-implement`; apply `wyrd-ui` when touching the UI consumer. No
functional prerequisite from TASK-001–TASK-004 is required. Incorporate their
integrated SDK callers and MCP catalog additions when closing consumers;
shared-file coordination does not create a new behavior dependency.

## Owners, Scope, Consumers, and Prohibited Changes

Inspected reuse map (paths are repository-relative):

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Rust Card contracts | `crates/wyrd-spec/src/card`, `Card`, kind Specs, metadata/relationships/status; shared `wyrd_client::cards::Cards::get` | `sdks/wyrd-sdk-python/src/state/mod.rs::Cards::get`; `sdks/wyrd-sdk-ts/native/src/cards.rs::NativeCards::get` | Foreign boundaries do not expose the complete typed result natively | Project these existing contracts at the owning SDK boundaries | No new domain model, parser, transport, or durable owner |
| Python returned objects and stubs | `sdks/wyrd-sdk-python/src/state/mod.rs::Cards::get`; native registration; `scripts/assemble_stubs.py::PUBLIC_MODULE_STUBS` | `python/wyrd/cards/__init__.py`; `python/wyrd/stubs/cards.pyi`; unit registered-Card typing tests and real register/hydrate journey | Generic get serializes to Python values then calls `_card_types.from_wire`; stubs import generated dataclasses | Replace that conversion with SDK-owned PyO3 projections; extend existing registration, exports, and stub sources | Foreign-runtime wrappers are necessary because `wyrd-spec` stays PyO3-free; existing stub assembly is reused |
| Existing Python authoring/runtime behavior | `crates/wyrd/wyrd-cards/src/{data,model,prompt}.rs` holders and `skald-prompt::Prompt` | Kind-specific getters in SDK state; `tests/integration/cards/test_cards_crud.py` | Holders are not lossless substitutes for full envelopes: Data conversion resets evidence; Model reconstruction omits status/relationships | Reuse suitable native values while preserving authoring behavior and complete returned envelopes | Do not create another authoring/runtime engine or wrap a lossy holder as a complete Card |
| TypeScript native results and declarations | `native/src/cards.rs::{NativeCards,NativeWyrdState}`; `native/src/lib.rs::NativeLifecycleResult`; napi build | `wyrd/src/index.ts::{Cards,WyrdState}`; `tests/types/card-types.test-d.ts`; register/hydrate and Workflow journeys | Native Card results are JSON strings; TS facades assert generated Card types over decoded values | Expose typed Card results through N-API and project its declarations in the public package | Thin N-API projections are required for Node; reuse native client/state handles and structured errors |
| MCP gateway input discovery | `wyrd-server/src/mcp/gateway.rs::tools`; `mcp/principals.rs::{schema_of,tool}`; `wyrd-spec::gateway` DTOs | Gateway catalog unit test and MCP administration journey | Five advertised inputs depend on `include_str!` schema files | Derive from the exact Rust handler DTOs using the existing derivation pattern | Existing schemars dependency is sufficient; no second catalog or schema registry |
| Contract regression proof | `wyrd-spec` Card/Prompt/security schema tests; Eval/Trace/Observation schema-fixture tests | `src/card/mod.rs::model_schema_drift_tests`; `src/card/prompt/mod.rs::prompt_schema_drift_tests`; `src/security/mod.rs::schema_drift_tests`; `src/vala/{eval,trace,observation}` | Tests read the public collection or fixture schema collections written by the same exporter | Replace file comparisons with explicit contract/serialization/derived-schema property assertions in existing test owners | No replacement bulk snapshot collection; do not compare a derivation with itself |
| Documentation | `docs/scripts/{generate_card_docs,generate_api_docs,generate_llms_txt}.py` | Generated Card reference pages, schema inventory, `docs/public/llms*.txt`; `docs:generate` and `docs:check` | Field docs, inventory, and raw schema dumps read the collection | Keep useful contract field/reference information from Rust contracts and existing runtime descriptions; retire inventory/dump publication | No new stored schema bundle or general documentation engine |
| Other generated data and build/package closure | `wyrd-spec/examples/gen_schemas.rs::{write_ts_error_codes,write_problem_examples}`; `mise.toml` codegen tasks | UI `src/lib/server/problem.ts`; `docker/official/Dockerfile`; `scripts/check_ts_package.mjs` package artifact expectations | Error data shares the bulk exporter/directory; packaging expects generated `card-types` files | Retain catalog-derived error codes and owner-local UI error data; update existing codegen/build/package consumers | Preserve existing generators for useful non-schema data; no new dependency or permanent name-ban check |

The generator currently types Data, Model, Prompt, Agent, Verifier, Service,
Trigger, and Operator plus their reachable nested types. It exposes envelopes
for the other native kinds with intentionally opaque specs. Preserve at least
that typing coverage and all registered-kind envelope information; this task
does not require speculative expansion of the previously opaque specs.
Include every public type and typed field reachable through the existing
generated exports, not just the top-level eight classes. Discriminator tags,
reference values, collection element types, defaults, and optional fields must
remain accurate in both runtime objects and declarations.

Python's stub pipeline assembles hand-authored declaration sources; it does
not currently infer runtime objects from Rust. Update those sources and
regenerate public stubs. TypeScript's napi pipeline generates native
declarations; public ergonomic declarations may project them without creating
an independent contract definition.

Preserve public entry points and type names where currently exported, typed
field access, read-only envelopes, native authoring/Prompt behavior, artifact
loading, and complete server evidence. Retiring dataclass implementation and
dataclass-specific reflection/exception behavior is authorized by D-006.
Keep native authoring holders and registered evidence views distinct wherever
they have different lifecycles; do not manufacture duplicate data-only models.

Do not move PyO3/N-API into `wyrd-spec`, alter Rust wire fields, introduce a
new transport or credential path, weaken errors/security/tenancy, erase typed
fields into JSON/`Any`, or hide missing binding types with unchecked casts.
Do not retain schema-driven SDK generation through stdin, temporary files,
a relocated directory, or a checked-in bundle. Runtime MCP/OpenAPI derivation
and genuine schema contract assertions remain. Do not move unrelated native
wrappers, expand Card registration support, or rewrite other SDK capabilities.

## Approach

1. Trace the currently public generated type closure and every runtime,
   documentation, snapshot, package, and codegen consumer against the integrated
   tree; separate reusable native values from authoring/runtime holders.
2. Expose lossless typed Python Card results through the existing native SDK
   boundary; align registration, public exports, and assembled stub sources.
3. Expose equivalent typed N-API results for Cards and every WyrdState accessor
   currently returning generated Card types; regenerate native/public
   declarations and preserve errors and kind narrowing.
4. Derive gateway MCP inputs directly from handler DTOs while preserving the
   existing catalog semantics and reference/secret metadata.
5. Close docs, schema snapshot tests, UI examples, error-code generation,
   Docker, and package consumers; reconcile owning architecture/reference
   guidance with native SDK projection and runtime schema discovery.
6. Remove the bulk exporter, generated schema collections including the
   Eval/Trace/Observation schema fixtures, schema-model generator, and its
   Python/TypeScript outputs. Regenerate the remaining useful artifacts and
   prove consumers operate without recreating the retired pipeline.

## Ordered Implementation Scenarios

### Scenario 1 — Python reads complete native-bound Cards

**Behavior.** Public generic get returns native-bound, accurately typed Card
objects for the currently supported kinds, preserving nested variants,
metadata, relationships, optional status and verification evidence. Public
imports and stubs agree; authoring/artifact workflows remain usable. Unsupported
references and unauthorized operations retain their stable errors.
(REQ-018, INV-001, INV-005, AC-009)

**RED.** Extend the existing real-server typing journey to assert native-bound
results and complete evidence, not merely the generated class names. It must
fail because current get constructs `_card_types` dataclasses. Prepare with
`mise run py:setup`, then run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_cards_get_returns_every_kind_typed'
```

**GREEN.** Extend the existing Python boundary and public declaration sources
to return complete Rust-backed values. Replace private decoder-centric unit
tests with public binding tests for their meaningful variant, optional-field,
and read-only guarantees; retain genuinely opaque spec slots. Exercise
missing/denied reads and artifact authoring/loading through the real server.
Rerun the exact journey and the focused runtime-owned unit tests.

**REFACTOR.** Reuse native value types where they represent the same fact;
keep runtime resources out of declarative specs and avoid JSON round trips
used solely to reconstruct a second object model.

### Scenario 2 — TypeScript reads typed native results and hydrated state

**Behavior.** Cards and all affected WyrdState accessors receive typed values
through N-API, with declarations that retain literal-kind/variant narrowing,
required/optional fields, nested types, and complete evidence. Typed Card
success is not represented by a JSON string plus an unchecked TS cast.
Stable error projections and negative flows remain unchanged.
(REQ-018, INV-001, INV-005, AC-009)

**RED.** Extend the existing eight-kind real-server journey to prove the
native boundary exposes typed values and preserves evidence. Add compile-time
assertions to the existing Card typing test for the native declaration path;
current JSON-only native result/declarations cannot satisfy them. Prepare with
`mise run ts:build` and `mise run ts:build:testing`, then run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/register-and-hydrate.test.ts -t "^cards get returns every kind typed$"'
mise run ts:typecheck
```

**GREEN.** Extend native SDK results and the public facade without duplicating
shared-client operations or error semantics. Include Service and per-kind
WyrdState accessors in the same type closure. Rerun the focused journey,
compile-time proof, and Python scenario. Prove hydrated-state relationships
remain exact with the existing focused Workflow journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^applied workflow stays pinned to its registered cards$"'
```

**REFACTOR.** Remove obsolete JSON-only Card projection and schema-generated
imports as their native replacements become green. Keep unrelated lifecycle
result handling intact where it is outside this contract replacement.

### Scenario 3 — MCP discovery retains its input contract without schema files

**Behavior.** Gateway tools advertise the same accepted DTO contract directly
from Rust, with resolved nested references, required fields, secret `writeOnly`
metadata, and unchanged read/replace/destroy annotations. Existing discover,
act, redacted read-back, and permission-denied behavior remains usable.
(REQ-019, INV-003, INV-005, AC-010)

**RED.** This scenario preserves already-correct advertised behavior, so do
not manufacture a behavioral failure. Before changing derivation, extend the
existing catalog test to pin all five accepted DTO contracts, nested references,
secret metadata, and effects, and record its baseline. A passing baseline is
verification-only proof; collection-free compilation/consumer closure in
Scenario 4 proves removal of the file dependency. Run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=mcp::gateway::tests::gateway_catalog_annotates_every_administration_operation)'
```

**GREEN.** Reuse direct Rust derivation while preserving gateway-specific
catalog semantics. The bulk exporter only overwrites the dialect header; do
not silently normalize dialect or omit definitions. Reusing the entire
principals tool helper would change destructive annotations and add output
schemas: preserve current effects and output behavior. Rerun the unit test and
real MCP administration journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=gateway::pg_tests::agent_administers_redacted_gateway_configuration)'"
```

The journey also needs actual tools/list assertions for these input schemas;
its current administration calls alone do not prove advertised-schema parity.

**REFACTOR.** Share only the derivation behavior that is actually common.
Keep write/view DTO distinctions, structured errors, validation, authorization,
and audit in their current owners.

### Scenario 4 — Generation and consumers operate with no schema collection

**Behavior.** Remaining codegen, docs, package checks, UI error projection,
and Rust contract regression tests succeed without reading or recreating any
retired schema collection or schema-based SDK model outputs.
(REQ-017, REQ-019, INV-005, AC-010)

**RED.** Add the smallest regression proof around the affected existing
generation or consumer owner showing that normal execution depends on the
collection or recreates it. Record its exact focused command before running
RED; do not add a permanent repository-wide name-ban check. Existing
collection-reading contract assertions also expose missing-file failures once
their file dependency is removed. Static deletions themselves need no
manufactured RED.

**GREEN.** Close the remaining consumers before deleting their inputs.
Preserve useful docs, error-code declarations and UI error data; retire the
schema inventory and raw dump. Replace snapshot file dependencies with
explicit tested contract properties before deleting their fixtures. Complete
regeneration, runtime/build and package proof with all earlier scenarios green.

**REFACTOR.** Remove obsolete export lists, parsing/lowering machinery,
package artifact expectations, directory copies and drift entries. Do not
create a replacement collection or duplicate a property already protected by
the compiler or an ordinary contract test.

## Acceptance Criteria

1. PyO3/N-API projections and accompanying public declarations preserve the
   current reachable typed field closure, kind/variant narrowing, optionality,
   complete server evidence, and read-only Python envelopes. Deliberately
   opaque values do not become authority for typed contracts. (AC-009)
2. Existing generic and kind-specific get, authoring, registration, hydration,
   artifact loading, and affected WyrdState consumers work; stable missing,
   invalid, mismatched-kind and denied-operation failures remain. (AC-009)
3. Gateway tools/list and calls retain their accepted contract, references,
   effects, secret protection, permissions and audit. Runtime MCP/OpenAPI
   derivation remains supported without the files. (AC-010)
4. No public or fixture schema collection, bulk schema exporter,
   schema-to-SDK generator, `_card_types.py`, or schema-generated
   `card-types.ts` remains, and normal generation does not recreate them.
   No relocated/temporary schema-to-SDK pipeline or compatibility copy is
   substituted. (AC-010)
5. Docs retain meaningful contract field/reference information and
   machine-readable guidance without advertising deleted file paths. Existing
   non-schema error examples and code declarations remain catalog-derived;
   UI/build/package consumers and installed imports work. (AC-010)
6. Contract tests retain meaningful expected-property coverage without
   file snapshots; bindings/stubs/declarations match and regenerate cleanly.
   Record which retired checks protected only the deleted pipeline and which
   live properties have replacement proof. (INV-005, AC-009, AC-010)

## Expected Write Set and Consumer Closure

Likely owners: `sdks/wyrd-sdk-python/{src,python/wyrd,scripts,tests}`;
`sdks/wyrd-sdk-ts/{native,wyrd/src,wyrd/tests}` and emitted declarations;
`crates/wyrd-spec/{examples,schemas,tests/fixtures,src}` and its manifest;
gateway MCP descriptors and owning server/MCP tests; `scripts/gen_card_types.py`
and package checks; `mise.toml`; `docker/official/Dockerfile`; docs generators,
generated Card/API references and `llms*.txt`; UI error-example consumer/data;
applicable architecture/reference guidance and fixture documentation.

Source changes to `wyrd-spec` concern exporter retirement and meaningful
contract tests, not binding annotations or altered wire models. No SQL,
storage, provider, verification engine, or authentication redesign belongs here.
Paths are guidance, not an implementation allowlist. Preserve other ongoing
work and revalidate consumers against the integrated source.

## Verification and Evidence

Run every exact scenario command above. For Python boundary unit regressions,
after native setup, run the affected existing test module through:

```bash
mise exec -- bash -lc \
  'cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/unit/cards/test_registered_card_typing.py'
```

Use the focused register/hydrate modules for supporting happy, edge and
negative flows, rather than the entire SDK journey suites:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py tests/integration/test_workflow_loading.py tests/integration/cards/test_cards_crud.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/register-and-hydrate.test.ts tests/integration/workflow-loading.test.ts'
```

For changed Rust contract test owners, confirm exact retained/replacement test
names from source/nextest and record/run their exact package/lib expressions.
Do not prescribe nonexistent replacement test names. Reuse the original
serialization, variant, required-field, and security expectations; do not
delete failing coverage to obtain a green lane. Pure Rust proof stays in Rust;
Python/Node lifetime proof runs in those runtimes.

Applicable task checks: `mise run fmt`, `mise run lints`, `mise run py:format`,
`mise run py:lints`, `mise run py:typecheck`, `mise run py:test:unit`,
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`,
`mise run ts:napi:check`, `mise run ts:test:unit`, `mise run ts:pack:check`,
`mise run test:wyrd-sdk`, `mise run codegen:check`, `mise run check:deps`,
`mise run check:py-wheel-no-testing`, `mise run docs:check`, and
`git diff --check`. Run the owning UI check/build/error tests and the existing
Docker build proof if their consumers change; discover exact test selectors
from the current owners. If served OpenAPI behavior changes, add
`mise run test:principals:integration`; removing file exports alone does not
require changing that behavior.

Deletion, documentation and declaration generation receive static/regression
proof; new conversion, binding, MCP derivation and generator behavior follow
the ordered TDD scenarios. Record native declaration coverage and a consumer
closure matrix for AC-009/AC-010, including remaining intentionally opaque
slots and retired dataclass-specific behavior. No new permanent checker is
needed to prove another check. Full suites and the repository aggregate run
once on the integrated candidate at change review, not as task iteration.

## Material Stop Conditions

Return for specification revision if preserving the existing typed surface
requires changing a durable wire contract, discarding server evidence,
weakening permissions/validation, moving bindings into the foundational crate,
or removing public entry points/fields beyond D-006. A lossy native holder,
schema-only public type, or binding-generator limitation is a gap to implement
at the SDK boundary, not permission to retain the retired pipeline or weaken
typing. Reversible wrapper/declaration mechanics remain implementation-owned.

## Authority Links

- [Approved spec revision 6](../spec.md): REQ-017–REQ-019, INV-005, D-006,
  AC-009–AC-010; prior bootstrap obligations remain intact.
- [AGENTS.md](../../../../AGENTS.md), especially §§2, 5, 7–12, 14 and 16.
- [Agent rules](../../../../architecture/agent-rules.md).
- [Protocol authority](../../../../architecture/wyrd-design.md) and
  [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [PyO3 boundaries](../../../../architecture/references/languages/pyo3-boundaries.md),
  [Python API/stubs](../../../../architecture/references/languages/python-api-and-stubs.md),
  [TypeScript guide](../../../../architecture/references/languages/typescript-guide.md).
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md),
  [implementation execution](../../../../architecture/references/languages/implementation-execution.md),
  [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
