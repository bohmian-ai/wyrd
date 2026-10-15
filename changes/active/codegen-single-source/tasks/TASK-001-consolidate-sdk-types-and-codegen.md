---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-codegen-single-source
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
depends_on: []
---

# Consolidate SDK types and remove unused codegen artifacts

## Outcome and Value

Deliver the complete five-item cleanup from approved spec revision 1: Rust
owns wire contracts, Python has one authored stub tree and one native Card
envelope, TypeScript consumes Rust-derived wire types, and generation retains
only consumed outputs. All three language lints and both SDK typechecks pass.

This is one task because retiring the shared pipeline without its SDK, MCP,
docs, setup, and verification consumers would leave a broken checkout. It maps
every requirement, invariant, and acceptance criterion in the spec. There are
no task dependencies and no additional planned scenario matrix.

Required implementation skill: `$wyrd-implement`. Apply `$wyrd-ui` only if
implementation actually changes UI source; preserving its existing JSON import
does not require unrelated UI work.

## Owners, Scope, Consumers, and Prohibited Changes

Owners are contract tooling/`wyrd-spec`, the Python and TypeScript SDKs, server
MCP, docs tooling, and repository verification/authority files. Rust wire types
and errors remain authoritative. Python's direct public stubs are its sole
native typing declarations; they are not a second runtime implementation.

### Source-backed reuse map

Paths and symbols below were inspected in the current tree. They identify
existing capabilities and gaps, not a private symbol/file allowlist.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Consumed generation | `wyrd-spec/examples/gen_schemas.rs::{main,write,write_fixture,write_ts_error_codes,write_problem_examples}` | `mise.toml` codegen tasks; UI `lib/server/problem.ts`; schema golden readers | Emits unused persistent schemas and fixtures alongside useful outputs | Keep error codes/UI examples; produce transient metadata only for live consumers; retire snapshot-only writers/readers | None; reuse schema derives and existing command owner |
| TypeScript wire types | `scripts/gen_card_types.py::{Lowering,TypeScript,envelope,lower,main}` | SDK `src/index.ts` imports, generic Card getter, handwritten registry interfaces; `tests/types/` | Reads committed `card.json`; emits Python; other wire shapes are handwritten | Keep TS lowering/rendering, accept transient Rust roots, replace duplicate wire declarations | Existing lowerer already projects schemas; extend it rather than add a generator dependency |
| Python generic Card retrieval | `src/state/mod.rs::{PyCards::get,PyCardEnvelope}` and `PythonStateHydrator::hydrate_envelopes` | `python/wyrd/cards/__init__.py`; integration retrieval/workflow consumers | Getter imports generated dataclasses; envelope getter assumes a hydrated alias | Return retained native envelope; registry aliases are empty, hydrated aliases remain validated | Existing envelope already owns all Card data and serialization |
| Python typing and protocols | `scripts/assemble_stubs.py::{assemble_root_stub,assemble_public_module_stubs,rewrite_public_imports,validate_package_exports,validate_source_stub}` | Public `.pyi`, root/cards/agent runtime modules, `pyproject.toml`, `py:setup` | Handwritten templates duplicate shipped declarations; runtime `Card` is defined twice and replaced | Author public declarations directly; keep useful validation, consolidate inline Python definitions and re-export | No new extractor; current Rust `PyAny` signatures cannot reconstruct rich Python typing |
| Python callable tool schemas | `python/wyrd/_schema.py::{annotation_to_schema,output_to_json_schema}` and `agent/tool.py::tool` | Three `examples/python/workflow_*.py`; Skald Python tool invocation; existing Python unit coverage | Unused output conversion remains beside used annotation conversion | Remove unused conversion only; preserve decorator/agent-tool flow | No replacement machinery |
| Gateway MCP schemas | `mcp/gateway.rs::{gated,descriptors}`; `mcp/principals.rs::schema_of`; five request types re-exported by `wyrd_spec::gateway` | Gateway catalog regression in `mcp/gateway.rs::tests`; actual gateway handler deserialization | Descriptors embed five persistent schema files | Derive from same Rust request types, following existing MCP schema pattern | Server already depends on schemars/serde_json; no new schema owner |
| Documentation and diagram contracts | `generate_llms_txt.py::{render_llms_txt,render_llms_full}`; `generate_card_docs.py::main`; `generate_api_docs.py::{render_schemas,main}` | `docs:generate`, `docs:check`, `diagrams/diagrams.test.ts`, publish-docs workflow | Raw schema appendices/links, inactive renderer, diagram test file dependencies | Keep index/prose; remove inactive schema rendering; feed meaningful diagram checks transient Rust metadata | Existing generator/test owners suffice; no documentation schema service |
| Build/declaration closure | `mise.toml` Python/codegen/TS/docs tasks; TS `package.json` exports; `tsconfig.json`; Python `pyproject.toml` | Lint/type/test tasks, N-API build, docs/CI path selection, package consumers | Setup assumes copied stubs and codegen snapshots deleted outputs | Remove obsolete dependencies and adjust surviving checks/packaging; reconcile authority text | Existing lanes are sufficient; do not add a deleted-name ban or duplicate check |

Do not add dependencies/features, compatibility aliases, alternate Card models,
server routes, transport logic, persistent schema replacements, broad
frameworks, or unrelated UI changes. Preserve the UI example data, existing
N-API generation, error-code generation, and theme-token checks.

## Approach

1. Promote package-local Python declarations to their sole authored locations,
   consolidate runtime protocols, and update affected repository rules. Retain
   useful documentation/export/layout validation without copying declarations.
2. Update the existing retrieval journey for the selected `CardEnvelope`
   contract, establish RED, then replace generated Card construction with the
   existing native owner. Close all generated-class imports, stubs, fixtures,
   support helpers, workflow tests, and docs; remove only unused `_schema`
   behavior.
3. Retain and extend Rust schema extraction and the TS lowerer for transient
   metadata. Register the Rust roots needed by current TS wire interfaces,
   including transitive fields/enums; remove their handwritten counterparts
   and Python generation. Keep the public wrapper/N-API declaration ownership
   split and existing package exports.
4. Derive gateway MCP input schemas directly from existing request types.
   Rewire docs indexes and diagram contract checks, then remove committed
   generated schemas, generated fixture schemas, dead renderers, and their
   snapshot-only assertions. Keep substantive validation/serde/error tests.
5. Close codegen/setup/build/docs/CI/packaging references. Preserve error codes,
   UI examples, generated TS types, and theme-token reproducibility. Verify
   that regeneration does not recreate deleted files or rewrite authored
   Python stubs.
6. Run the focused proof and existing checks below. Fix lint/type failures at
   their owners; record command results and inspect the full diff against every
   spec obligation. Leave integration review's final aggregate to that stage.

## Ordered Implementation Scenarios

### Scenario 1 — One native Python envelope after removing duplicate generation

**Behavior.** A real Python SDK caller registers the existing fixture graph
and fetches its Cards through the server. Each result is the existing
`wyrd.state.CardEnvelope`; its exact identity, complete metadata/spec,
relationships/status, and serialized content remain available. Registry
aliases are empty; offline hydration still supplies its stable aliases.
The test reads mapping fields rather than generated dataclass attributes.
This proves REQ-004, INV-001/002, and AC-004; the rest of the cleanup uses
static and existing regression proof rather than additional planned scenarios.

**RED.** Change the expectations of the existing
`test_cards_get_returns_every_kind_typed` journey in
`tests/integration/test_register_and_hydrate.py` to the native envelope contract,
keeping its existing kind/content coverage. Establish setup first, then run
the exact command below. Before replacing the runtime, the expected failure is
that the returned generated dataclass is not a `CardEnvelope`; a missing
extension, invalid fixture, or unavailable database is not RED evidence.

Run from the repository root:

```bash
mise run py:setup
mise exec -- env WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/integration/test_register_and_hydrate.py::test_cards_get_returns_every_kind_typed'
```

The current symbol/path are confirmed from source. If it is renamed to reflect
the new return type, record and run its new exact node ID in the execution
evidence; do not use a broad or zero-selected filter.

**GREEN.** Return `PyCardEnvelope` through the existing Rust-backed retrieval
operation, with exact response identity and empty registry aliases. Preserve
the hydration owner's nonempty-alias validation. Update Python declarations,
exports, and every generated-model consumer, then delete the obsolete model
and emitter. Rerun the exact journey and affected existing Python regressions;
keep wire content and error assertions intact.

**REFACTOR.** Remove obsolete conversion/serialization hops, duplicate protocol
definitions, template assembly, and dead helpers within existing owners. Keep
the journey green. No replacement dataclass family, general projection layer,
or native stub extraction framework is permitted.

## Acceptance Criteria

- AC-001: generated schema trees and snapshot-only machinery are removed;
  generated/consumed UI examples remain.
- AC-002: all five gateway descriptors derive their input schemas directly
  from the actual Rust request types; catalog semantics stay intact.
- AC-003: documentation indexes/builds and meaningful diagram checks work
  without committed schema files; raw schema appendices/dead links are gone.
- AC-004: the single journey proves native envelope retrieval, content,
  identity, and the selected alias semantics; existing hydration is preserved.
- AC-005: Python has no generated Card module or template-copy stub source;
  declarations are canonical, exports resolve, and callable tool behavior is
  retained.
- AC-006: Rust owns existing TS wire declarations, public/native declaration
  modules resolve from their proper sources, and typing precision is retained
  or corrected, including CardSummary timestamps/enums.
- AC-007: all three mandatory language lint lanes pass with existing scope.
- AC-008: clean setup, codegen, packaging references, documentation, and rules
  agree on the new authority; remaining generated output is reproducible.

## Expected Write Set and Consumer Closure

Paths are guidance, not a private implementation allowlist:

- Contract/tooling: `crates/wyrd-spec/examples/gen_schemas.rs`, its generated
  schema/fixture trees and snapshot readers, `scripts/gen_card_types.py`.
- Python: native Card projection/getter under `sdks/wyrd-sdk-python/src/state`,
  `python/wyrd/_card_types.py`, `_schema.py`, public runtime modules and `.pyi`,
  `python/wyrd/stubs`, `scripts/assemble_stubs.py`, `pyproject.toml`, affected
  integration/unit tests and support helpers.
- TypeScript: generated wire type module(s), `wyrd/src/index.ts` and affected
  wrappers/imports, type fixtures/tests, declaration/package/build config only
  as required by existing consumers. Keep native bindings thin.
- Server: gateway MCP schema declarations and their existing regression owner.
- Docs: schema-dependent generators, `llms*.txt`, diagram-contract test setup,
  stale schema links/inventories, and docs generation/drift paths.
- Verification/authority: `mise.toml`, affected `.github` path-selection/docs
  publishing references, `AGENTS.md`, `architecture/agent-rules.md`, applicable
  design/doctrine/language/testing text, and `TESTING.md`.

Audit direct imports, build scripts, task dependencies, packaging globs, docs
links, tests, and exported types. Do not broadly delete other JSON, authored
fixtures, schema derives, generated error codes, or meaningful regressions.

## Verification and Evidence

Planning does not run these lanes. Implementation records command results.
One behavioral scenario is sufficient; do not invent more scenario packets.

### Mandatory language acceptance

Run these existing lint/type lanes on the implementation candidate:

```bash
mise run lints
mise run py:lints
mise run ts:lints
mise run py:typecheck
mise run ts:typecheck
```

The Rust lint lane includes all features/targets and the standalone production
server pass. Python uses Ruff; TypeScript uses Biome with warnings as errors.
The TS type lane builds its declared native/testing prerequisites. Do not
substitute compilation alone for a language's lint lane.

### Focused runtime and regression proof

Use the exact Python journey command above for RED and GREEN. For surviving
gateway catalog behavior, source confirms this exact library selector:

```bash
mise exec -- env WYRD_LOG=info cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::gateway::tests::gateway_catalog_annotates_every_administration_operation)'
```

Run the existing Python/TS unit lanes, preserving callable-tool/import/export
coverage and updating obsolete Card assertions:

```bash
mise run py:test:unit
mise run ts:test:unit
```

These are supporting regression lanes, not additional designed scenarios. If a
new behavioral branch is necessary in the lowerer, add the smallest ordinary
owner-level regression rather than a framework or a separate plan scenario.
Any specifically named added test must be recorded with its exact focused
command before handoff.

### Static, generated, and documentation proof

Authored stub relocation, artifact removal, and configuration/rule updates do
not need manufactured RED failures. Use typechecking, existing validation,
regeneration, and a source/diff audit. These lanes cover their direct outputs:

```bash
mise run fmt:check
mise run py:format:check
mise run ts:format:check
mise run codegen:check
mise run ts:napi:check
mise run docs:check
git diff --check
```

If Python tooling outside the SDK is changed, lint/format those touched files
through `mise exec -- uv run --project sdks/wyrd-sdk-python ruff check <paths>`
and the corresponding `ruff format --check <paths>`; replace `<paths>` with
the actual write set in evidence. The existing `py:lints` task covers only its
SDK package, tests, and scripts, not root/docs Python tools.

Inspect remaining generated output and confirm that a Rust contract change has
one declaration-generation path, authored `.pyi` files are not rewritten, and
the production package does not ship templates/testing-only declarations.
Typechecking cannot alone prove native signature parity; retain useful runtime
and export checks rather than claiming that it does.

Final integrated verification belongs to change review under the spec's
acceptance policy; do not add or rerun a broad aggregate inside this task.

## Material Stop Conditions

- Correctness would require a different public Card retrieval shape,
  compatibility aliases, or new server/wire/persisted behavior.
- An actual consumer requires a committed schema rather than a transient or
  direct Rust projection and cannot be closed within these approved outcomes.
- Native/TS generation requires a new dependency, feature, ownership boundary,
  or deliberately weaker public typing. Extend existing owners for supported
  schema constructs; do not silently drop contracts.
- Passing requires weakening a lint, excluding live code, deleting meaningful
  regression coverage, or changing authorization, audit, tenancy, or durability.

Private helper choices, corrected focused commands, transient transport,
adjacent consumer repairs, and in-scope lint fixes are implementation-owned
and are not reasons to stop. A red required lane remains a blocker until fixed
or diagnosed with the unavailable material authority; do not investigate blame.

## Authority Links

- [Approved spec revision 1](../spec.md).
- [AGENTS.md](../../../../AGENTS.md) and
  [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md) and
  [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md),
  [implementation execution](../../../../architecture/references/languages/implementation-execution.md),
  and [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
- [Python API/stubs](../../../../architecture/references/languages/python-api-and-stubs.md)
  and [TypeScript guide](../../../../architecture/references/languages/typescript-guide.md).

The current generated-stub rules are superseded only as described by REQ-009;
updating those authority files is part of this task, not an optional exception.

## Execution Evidence

Status: `IMPLEMENTED`. Commits on `split/codegen-single-source`: e4210b940,
ef77a81d5, 31f1124a7, 91869d6a4, 76267ce6f, df57d38fa, plus this evidence.

### Reuse-map revalidation

- Contract bundle: `gen_schemas.rs` keeps its owner; `contracts` prints one
  transient `{roots, definitions}` document from the existing derives.
  Consumers pipe it (TS lowerer) or read it from a temp file through
  `WYRD_CONTRACTS` (docs diagram test). Nothing is committed.
- Gateway MCP: existing `mcp::principals::schema_of` is reused for the five
  request types.
- Python `Cards.get`: the existing `PyCardEnvelope` is returned. Identity is
  enforced by the existing `wyrd_client` `assert_selector_identity`
  (`crates/shared/wyrd-client/src/cards/reads/get.rs`), and
  `hydrate_envelopes` keeps its nonempty-alias validation.
- TS: the existing lowerer (renamed `scripts/gen_ts_types.py`) now emits only
  TS. `HydrationSummary`/`HydratedArtifact` stay handwritten because
  `wyrd-client` owns them and `wyrd-spec` cannot depend on it.

### RED/GREEN

The journey was renamed to
`tests/integration/test_register_and_hydrate.py::test_cards_get_returns_every_kind_as_envelope`.

- RED (before the Rust change):
  `AssertionError: assert False ... isinstance(RegisteredDataCard(...), CardEnvelope)`.
- GREEN: `1 passed`, with:

```bash
mise run py:setup
mise exec -- env WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/integration/test_register_and_hydrate.py::test_cards_get_returns_every_kind_as_envelope'
```

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 | Schema/fixture trees and snapshot tests removed; `crates/wyrd-spec/schemas/` holds only `ui_problem_examples.json` | `codegen:check`; tree audit | PASS |
| AC-002 | `mcp/gateway.rs` descriptors use `schema_of::<T>()` for the five request types | exact `gateway_catalog_annotates_every_administration_operation` (1 passed) | PASS |
| AC-003 | llms appendices/links, `generate_card_docs.py` and the schema inventory removed; the diagram test reads `WYRD_CONTRACTS` | `docs:check` | PASS |
| AC-004 | `PyCards::get` returns `PyCardEnvelope`, `aliases == ()`; hydration validation is unchanged | exact journey above (RED→GREEN); `py:test:integration` 222 passed (py-stubs run) | PASS |
| AC-005 | `_card_types.py`, `stubs/` and `assemble_stubs.py` deleted; pure-Python definitions live once in private modules re-exported by the stubs; `output_to_json_schema` removed, `tool` kept | `py:lints` (ruff + `scripts/check_stubs.py`), `py:typecheck`, `py:test:unit` 592 passed | PASS |
| AC-006 | `card-types.ts` generated from Rust roots; `index.ts` re-exports under the existing public names; `CardSummary` gains timestamps/enums | `ts:typecheck`, `ts:test:unit` 66 passed, `ts:napi:check` | PASS |
| AC-007 | — | `lints`, `py:lints`, `ts:lints`, `py:typecheck`, `ts:typecheck` | PASS |
| AC-008 | mise/CI/docs/authority files updated; `codegen:regen` writes no `.pyi` | `codegen:check`, `check:ci-selection`, `fmt:check`, `py:format:check`, `ts:format:check`, `git diff --check`, reference audit | PASS |

The reference audit (`git grep`) found no remaining references to the
deleted schema paths, `generate_card_docs`, `gen_card_types`,
`codegen:stubs`, `assemble_stubs`, `wyrd/stubs`, `_card_types` or
`output_to_json_schema` outside `changes/`.

Root Python tools touched (`.github/scripts/select-ci.py`,
`docs/scripts/generate_{api_docs,llms_txt}.py`, `scripts/gen_ts_types.py`)
pass `ruff check` and `ruff format --check`. Ruff formatting was applied to
files that previously did not comply.

### Non-goals and limits

- No dependencies, features, routes, compatibility aliases or lint
  weakening were added.
- The UI examples, error-code generation, N-API generation and theme-token
  checks are preserved.
- The TS response fields that are `Option` and not schema-defaulted lower
  to `?: T | null`, because the schema cannot tell skipped fields from null
  ones. Fields the server always sends stay required.
- The `CardTypes` namespace exposes `FieldSpec2` (a schemars collision
  suffix, like the existing `HuggingfaceMeta2`/`TorchMeta2`); the public
  name is `FieldDescription`.
- `test:local-dev:journey` passes (4 Rust plus 5 installed-CLI journeys,
  which cover `test_cli_bootstrap_saved_config`).
- `WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA` and `PYDANTIC_REQUIRED` stay in
  the catalog because other owners reference them.
